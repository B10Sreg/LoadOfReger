package main

import (
	"bytes"
	"fmt"
	"log"
	"os"

	"github.com/diamondburned/gotk4-adwaita/pkg/adw"
	"github.com/diamondburned/gotk4/pkg/core/glib"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// Пауза между последней правкой и записью конфига. Ползунок высоты за один
// драг даёт десятки изменений; без задержки мы бы на каждое переписывали файл
// и слали SIGUSR1, а бар на каждый сигнал пересоздаёт поверхность cairo.
const applyDelayMS = 180

// Настройки риса применяются реже: генератор перезапускает picom и dunst, и
// экран на этом заметно моргает. Полсекунды -- это пауза, после которой драг
// ползунка уже кончился, а не его середина.
const riceDelayMS = 500

type App struct {
	path string
	cfg  Config

	// Настройки риса (тема, окна, композитор, сессия) живут в своём файле и
	// применяются своим генератором, поэтому у них отдельный отложенный
	// таймер: правка ползунка бара не должна дёргать picom.
	rice          Rice
	ricePendingID glib.SourceHandle
	// Последнее применённое состояние риса и признак того, что оно есть:
	// пустая структура -- законное значение, по ней одной не отличить
	// «ещё ничего не применяли» от «применили пустое».
	riceApplied Rice
	riceValid   bool
	// Генератор запускается в горутине, и пока он работает, следующий запуск
	// ждёт в очереди -- один, самый свежий.
	riceRunning     bool
	riceQueued      bool
	riceQueuedTheme bool

	app *adw.Application
	win *adw.PreferencesWindow

	pendingID glib.SourceHandle
	style     *styler
	// TOML, который уже лежит в файле: по нему видно, что правка ничего не
	// изменила и писать нечего.
	lastSaved []byte

	// Строки, подписи которых зависят от ориентации бара: у вертикального
	// «высота» становится шириной, а зоны модулей -- верхом и низом.
	zones    []*zone
	thickRow *adw.SpinRow
	edgeRow  *adw.SpinRow
	sideRow  *adw.SpinRow
}

func main() {
	// В сессии выставлен GTK_THEME=Arc-Dark, а тема GTK4 у Arc неполная: её
	// gtk.css не подхватывается, и строки libadwaita остаются без отступов --
	// подписи наезжают на спинбоксы. Приложение на libadwaita и так рисуется
	// своим стилем, поэтому просто снимаем переменную до инициализации GTK.
	os.Unsetenv("GTK_THEME")

	app := adw.NewApplication("dev.reg.vxbar.Settings", gio.ApplicationFlagsNone)
	a := &App{path: ConfigPath()}

	cfg, err := Load(a.path)
	if err != nil {
		// Битый TOML — не повод не открыться: показываем дефолты и говорим,
		// что при первом же изменении файл будет перезаписан.
		log.Printf("vxbar-settings: %v", err)
	}
	a.cfg = cfg
	// Снимок того, что уже в файле: первая правка сравнивается с ним, а не с
	// пустотой.
	if data, mErr := Marshal(cfg); mErr == nil && err == nil {
		a.lastSaved = data
	}

	rice, riceErr := LoadRice()
	if riceErr != nil {
		// Настройки риса -- не повод не открыть настройки бара: показываем
		// то, что прочиталось, и говорим о проблеме тостом.
		log.Printf("vxbar-settings: rice.toml: %v", riceErr)
	}
	a.rice = rice

	a.app = app
	app.ConnectActivate(func() { a.build(err) })
	if code := app.Run(os.Args); code > 0 {
		os.Exit(code)
	}
}

func (a *App) build(loadErr error) {
	if a.style == nil {
		a.style = newStyler()
	}
	a.style.apply(a.cfg)

	a.win = adw.NewPreferencesWindow()
	a.win.SetApplication(&a.app.Application)
	a.win.SetTitle("Настройки риса")
	a.win.SetDefaultSize(760, 780)
	a.win.SetSearchEnabled(true)

	a.win.Add(a.pageTheme())
	a.win.Add(a.pageCompositor())
	a.win.Add(a.pageBar())
	a.win.Add(a.pageStyle())
	a.win.Add(a.pageTags())
	a.win.Add(a.pageClock())
	a.win.Add(a.pageModules())
	a.win.Add(a.pagePower())
	a.win.Add(a.pageWidgets())
	a.win.Add(a.pageSession())
	a.applyOrientation()

	// Правка, сделанная за миг до закрытия, иначе терялась: таймер на 180 мс
	// не успевал сработать и умирал вместе с циклом событий.
	a.win.ConnectCloseRequest(func() bool {
		a.flush()
		a.flushRice(true)
		return false
	})

	a.win.Present()

	if loadErr != nil {
		a.toast(fmt.Sprintf("Конфиг не разобран, показаны значения по умолчанию: %v", loadErr))
	}
}

// ---------------------------------------------------------------- страницы

func (a *App) pageBar() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Бар")
	page.SetIconName("view-reveal-symbolic")

	g := adw.NewPreferencesGroup()
	g.SetTitle("Положение")
	g.SetDescription("Бар резервирует место через _NET_WM_STRUT_PARTIAL, окна подвинутся сами")

	pos := adw.NewComboRow()
	pos.SetTitle("Сторона экрана")
	names := make([]string, len(Positions))
	for i, p := range Positions {
		names[i] = p.Name
	}
	pos.SetModel(gtk.NewStringList(names))
	for i, p := range Positions {
		if p.ID == a.cfg.Bar.Position {
			pos.SetSelected(uint(i))
		}
	}
	pos.Connect("notify::selected", func() {
		i := int(pos.Selected())
		if i < 0 || i >= len(Positions) {
			return
		}
		a.edit(func() { a.cfg.Bar.Position = Positions[i].ID })
		// Слева/справа бар вертикальный: переименовываем всё, что от этого
		// зависит, иначе «высота» у вертикальной панели читается как ошибка.
		a.applyOrientation()
	})
	g.Add(pos)

	mons := monitorNames()
	mon := adw.NewComboRow()
	mon.SetTitle("Монитор")
	mon.SetModel(gtk.NewStringList(mons))
	if a.cfg.Bar.Monitor < len(mons) {
		mon.SetSelected(uint(a.cfg.Bar.Monitor))
	} else {
		// Монитор из конфига отключили. Список показывал бы первый, а в файле
		// оставался бы недостижимый индекс -- и бар не поднимался бы там, где
		// его показывают настройки.
		mon.SetSelected(0)
		a.edit(func() { a.cfg.Bar.Monitor = 0 })
	}
	mon.Connect("notify::selected", func() {
		a.edit(func() { a.cfg.Bar.Monitor = int(mon.Selected()) })
	})
	g.Add(mon)
	page.Add(g)

	gg := adw.NewPreferencesGroup()
	gg.SetTitle("Размеры")
	a.thickRow = a.spin("Высота", "пикселей", 12, 400, 1, 0,
		float64(a.cfg.Bar.Height),
		func(v float64) { a.cfg.Bar.Height = int(v) })
	a.edgeRow = a.spin("Отступ от края", "", 0, 64, 1, 0,
		float64(a.cfg.Bar.MarginEdge),
		func(v float64) { a.cfg.Bar.MarginEdge = int(v) })
	a.sideRow = a.spin("Отступ по бокам", "", 0, 400, 1, 0,
		float64(a.cfg.Bar.MarginSide),
		func(v float64) { a.cfg.Bar.MarginSide = int(v) })
	gg.Add(a.thickRow)
	gg.Add(a.edgeRow)
	gg.Add(a.sideRow)
	page.Add(gg)

	gt := adw.NewPreferencesGroup()
	gt.SetTitle("Обновление")
	gt.Add(a.spin("Период опроса", "миллисекунд; влияет на cpu/ram/часы, клики обрабатываются сразу",
		100, 10000, 100, 0,
		float64(a.cfg.Bar.IntervalMS),
		func(v float64) { a.cfg.Bar.IntervalMS = int(v) }))
	page.Add(gt)

	return page
}

func (a *App) pageStyle() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Стиль")
	page.SetIconName("applications-graphics-symbolic")

	g := adw.NewPreferencesGroup()
	g.SetTitle("Цвета")
	g.SetDescription("Прозрачность учитывается: бар живёт на 32-битном ARGB-визуале")
	g.Add(a.color("Фон", "", &a.cfg.Style.Background))
	g.Add(a.color("Текст", "", &a.cfg.Style.Foreground))
	g.Add(a.color("Акцент", "активные элементы", &a.cfg.Style.Accent))
	g.Add(a.color("Приглушённый", "неактивные элементы", &a.cfg.Style.Muted))
	page.Add(g)

	gf := adw.NewPreferencesGroup()
	gf.SetTitle("Шрифт")
	fontRow := adw.NewActionRow()
	fontRow.SetTitle("Шрифт бара")
	fontRow.SetSubtitle(a.cfg.Style.Font)
	btn := gtk.NewFontDialogButton(gtk.NewFontDialog())
	btn.SetVAlign(gtk.AlignCenter)
	btn.SetFontDesc(pangoDescFromString(a.cfg.Style.Font))
	btn.Connect("notify::font-desc", func() {
		d := btn.FontDesc()
		if d == nil {
			return
		}
		s := d.String()
		a.edit(func() { a.cfg.Style.Font = s })
		fontRow.SetSubtitle(s)
	})
	fontRow.AddSuffix(btn)
	fontRow.SetActivatableWidget(btn)
	gf.Add(fontRow)
	page.Add(gf)

	gm := adw.NewPreferencesGroup()
	gm.SetTitle("Геометрия")
	gm.Add(a.spin("Скругление углов", "имеет смысл при ненулевых отступах", 0, 40, 1, 0,
		a.cfg.Style.Radius, func(v float64) { a.cfg.Style.Radius = v }))
	gm.Add(a.spin("Внутренние поля", "слева и справа внутри бара", 0, 60, 1, 0,
		a.cfg.Style.Padding, func(v float64) { a.cfg.Style.Padding = v }))
	gm.Add(a.spin("Зазор между модулями", "", 0, 60, 1, 0,
		a.cfg.Style.ModuleGap, func(v float64) { a.cfg.Style.ModuleGap = v }))
	page.Add(gm)

	gr := adw.NewPreferencesGroup()
	reset := adw.NewActionRow()
	// Кнопка живёт на странице бара и сбрасывает только его конфиг. Раньше
	// подпись обещала «всё», а настройки риса (тема, окна, композитор,
	// сессия) лежат в другом файле и не трогались.
	reset.SetTitle("Сбросить настройки бара")
	reset.SetSubtitle("Тема, окна и композитор останутся как есть — они в rice.toml")
	rb := gtk.NewButtonWithLabel("Сбросить")
	rb.SetVAlign(gtk.AlignCenter)
	rb.AddCSSClass("destructive-action")
	rb.ConnectClicked(a.confirmReset)
	reset.AddSuffix(rb)
	gr.Add(reset)
	page.Add(gr)

	return page
}

func (a *App) pageTags() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Теги")
	page.SetIconName("view-grid-symbolic")

	g := adw.NewPreferencesGroup()
	g.SetTitle("Цвета тегов")
	g.Add(a.color("Фон активного", "", &a.cfg.Tags.ActiveBG))
	g.Add(a.color("Текст активного", "", &a.cfg.Tags.ActiveFG))
	g.Add(a.color("Занятый тег", "есть окна", &a.cfg.Tags.OccupiedFG))
	g.Add(a.color("Пустой тег", "", &a.cfg.Tags.EmptyFG))
	page.Add(g)

	gb := adw.NewPreferencesGroup()
	gb.SetTitle("Поведение")

	hide := adw.NewSwitchRow()
	hide.SetTitle("Прятать пустые теги")
	hide.SetSubtitle("Скрывает теги без окон, кроме активного")
	hide.SetActive(a.cfg.Tags.HideEmpty)
	hide.Connect("notify::active", func() {
		a.edit(func() { a.cfg.Tags.HideEmpty = hide.Active() })
	})
	gb.Add(hide)

	gb.Add(a.spin("Поля вокруг тега", "", 0, 40, 1, 0,
		a.cfg.Tags.ItemPadding, func(v float64) { a.cfg.Tags.ItemPadding = v }))

	// Направление переезда задаёт сам бар: у горизонтального плашка едет
	// влево-вправо, у вертикального -- вверх-вниз, так что настраивать тут
	// нечего, кроме длительности.
	gb.Add(a.spin("Анимация переключения", "Миллисекунды на переезд к соседнему тегу; дальние едут дольше, 0 — мгновенно",
		0, 1000, 10, 0,
		float64(a.cfg.Tags.AnimMS), func(v float64) { a.cfg.Tags.AnimMS = int(v) }))

	labels := adw.NewEntryRow()
	labels.SetTitle("Подписи тегов")
	labels.SetText(joinLabels(a.cfg.Tags.Labels))
	labels.SetShowApplyButton(true)
	apply := func() {
		v := splitLabels(labels.Text())
		a.edit(func() { a.cfg.Tags.Labels = v })
	}
	labels.ConnectApply(apply)
	labels.ConnectEntryActivated(apply)
	gb.Add(labels)

	hint := adw.NewActionRow()
	hint.SetTitle("Через пробел, например: 一 二 三 四 五")
	hint.SetSubtitle("Пусто — брать имена из _NET_DESKTOP_NAMES, которые задаёт vxwm")
	hint.SetActivatable(false)
	gb.Add(hint)
	page.Add(gb)

	return page
}

// Часы жили на странице тегов -- по недосмотру: с тегами их роднит только то,
// что оба модуля рисует бар.
func (a *App) pageClock() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Часы")
	page.SetIconName("preferences-system-time-symbolic")

	gc := adw.NewPreferencesGroup()
	gc.SetTitle("Отображение")
	gc.SetDescription("Формат strftime: %H:%M -- часы и минуты, %a %d %b -- день недели и дата")

	gc.Add(a.entry("Формат (strftime)", "У вертикального бара строка разбирается на строки по пробелам и двоеточию",
		a.cfg.Clock.Format, func(v string) { a.cfg.Clock.Format = v }))
	gc.Add(a.entry("Команда по клику на часы", "Пусто — открывать встроенный календарь",
		a.cfg.Clock.OnClick, func(v string) { a.cfg.Clock.OnClick = v }))
	page.Add(gc)

	return page
}

// pagePower -- команды модуля power. Отдельная страница, а не строка в
// «Модулях»: команд пять, и каждая из них умеет увести сессию.
func (a *App) pagePower() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Питание")
	page.SetIconName("system-shutdown-symbolic")

	g := adw.NewPreferencesGroup()
	g.SetTitle("Кнопка питания")
	g.SetDescription("Модуль power. Клик по нему открывает список, строка запускает свою команду. " +
		"Пустая строка убирает пункт из списка.")

	type field struct {
		title, subtitle string
		get             func() string
		set             func(string)
	}
	fields := []field{
		{"Гибернация", "Сохраняет память на диск: после включения возвращается вся сессия",
			func() string { return a.cfg.Power.Hibernate },
			func(v string) { a.cfg.Power.Hibernate = v }},
		{"Сон", "Питание остаётся на памяти",
			func() string { return a.cfg.Power.Suspend },
			func(v string) { a.cfg.Power.Suspend = v }},
		{"Блокировка", "",
			func() string { return a.cfg.Power.Lock },
			func(v string) { a.cfg.Power.Lock = v }},
		{"Перезагрузка", "",
			func() string { return a.cfg.Power.Reboot },
			func(v string) { a.cfg.Power.Reboot = v }},
		{"Выключение", "",
			func() string { return a.cfg.Power.Poweroff },
			func(v string) { a.cfg.Power.Poweroff = v }},
	}
	for _, f := range fields {
		g.Add(a.entry(f.title, f.subtitle, f.get(), f.set))
	}
	page.Add(g)

	gh := adw.NewPreferencesGroup()
	def := adw.NewActionRow()
	def.SetTitle("Вернуть команды по умолчанию")
	def.SetSubtitle("power.sh из vxwm: снимает слепок сессии и проверяет swap перед гибернацией")
	rb := gtk.NewButtonWithLabel("Вернуть")
	rb.SetVAlign(gtk.AlignCenter)
	rb.ConnectClicked(func() {
		a.edit(func() { a.cfg.Power = DefaultPower() })
		// Поля держат старый текст: строки собраны один раз, и обновить их
		// проще пересборкой окна, чем ручным обходом каждой.
		a.rebuildWindow()
	})
	def.AddSuffix(rb)
	gh.Add(def)
	page.Add(gh)

	return page
}

// pageWidgets -- выдвижные виджеты: окошко, которое бар открывает рядом с
// модулем по клику.
func (a *App) pageWidgets() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Виджеты")
	page.SetIconName("view-paged-symbolic")

	g := adw.NewPreferencesGroup()
	g.SetTitle("Выдвижные виджеты")
	g.SetDescription("Клик по часам — календарь, по громкости — регулятор, по cpu и ram — топ процессов. " +
		"Повторный клик или клик мимо закрывает.")

	on := adw.NewSwitchRow()
	on.SetTitle("Включить")
	on.SetSubtitle("Выключенные модули остаются кликабельными только там, где есть быстрое действие")
	on.SetActive(a.cfg.Popups.Enabled)
	on.Connect("notify::active", func() {
		a.edit(func() { a.cfg.Popups.Enabled = on.Active() })
	})
	g.Add(on)
	page.Add(g)

	gm := adw.NewPreferencesGroup()
	gm.SetTitle("Геометрия")
	gm.Add(a.spin("Ширина", "пикселей", 140, 600, 10, 0,
		float64(a.cfg.Popups.Width), func(v float64) { a.cfg.Popups.Width = int(v) }))
	gm.Add(a.spin("Зазор до бара", "", 0, 40, 1, 0,
		float64(a.cfg.Popups.Gap), func(v float64) { a.cfg.Popups.Gap = int(v) }))
	gm.Add(a.spin("Внутренние поля", "", 4, 40, 1, 0,
		a.cfg.Popups.Padding, func(v float64) { a.cfg.Popups.Padding = v }))
	gm.Add(a.spin("Скругление углов", "", 0, 24, 1, 0,
		a.cfg.Popups.Radius, func(v float64) { a.cfg.Popups.Radius = v }))
	page.Add(gm)

	gc := adw.NewPreferencesGroup()
	gc.SetTitle("Содержимое")
	gc.Add(a.spin("Процессов в списке", "для виджетов cpu и ram", 1, 15, 1, 0,
		float64(a.cfg.Popups.ProcRows), func(v float64) { a.cfg.Popups.ProcRows = int(v) }))
	page.Add(gc)

	return page
}

// applyOrientation переписывает подписи, у которых разный смысл для
// горизонтального и вертикального бара.
func (a *App) applyOrientation() {
	vert := a.cfg.Bar.Vertical()
	if a.thickRow != nil {
		if vert {
			a.thickRow.SetTitle("Ширина")
			a.thickRow.SetSubtitle("пикселей поперёк бара")
			a.edgeRow.SetSubtitle("отодвигает бар от боковой кромки — «плавающий» вид")
			a.sideRow.SetSubtitle("укорачивает бар сверху и снизу")
		} else {
			a.thickRow.SetTitle("Высота")
			a.thickRow.SetSubtitle("пикселей")
			a.edgeRow.SetSubtitle("поднимает бар над краем экрана — «плавающий» вид")
			a.sideRow.SetSubtitle("сужает бар слева и справа")
		}
	}
	for _, z := range a.zones {
		t, d := z.labels(vert)
		z.group.SetTitle(t)
		z.group.SetDescription(d)
	}
}

// ------------------------------------------------------------- помощники UI

// spin строит числовую строку со спинбоксом. onSet вызывается уже под a.edit,
// поэтому сам ничего не сохраняет — только меняет поле конфига.
func (a *App) spin(title, subtitle string, min, max, step float64, digits uint,
	val float64, onSet func(float64)) *adw.SpinRow {

	adj := gtk.NewAdjustment(val, min, max, step, step*5, 0)
	row := adw.NewSpinRow(adj, step, digits)
	row.SetTitle(title)
	if subtitle != "" {
		row.SetSubtitle(subtitle)
	}
	adj.Connect("value-changed", func() {
		v := adj.Value()
		a.edit(func() { onSet(v) })
	})
	return row
}

// entry строит строку ввода с кнопкой применения. Текст уходит в конфиг по
// Enter или по кнопке, а не на каждый символ: иначе бар перечитывал бы конфиг
// на каждую букву пути или команды.
func (a *App) entry(title, subtitle, val string, onSet func(string)) *adw.EntryRow {
	row := adw.NewEntryRow()
	row.SetTitle(title)
	if subtitle != "" {
		// EntryRow не показывает подзаголовок, поэтому подсказка идёт
		// всплывающей: место под строкой занято самим полем ввода.
		row.SetTooltipText(subtitle)
	}
	row.SetText(val)
	row.SetShowApplyButton(true)
	apply := func() {
		v := row.Text()
		a.edit(func() { onSet(v) })
	}
	row.ConnectApply(apply)
	row.ConnectEntryActivated(apply)
	return row
}

// color строит строку с кнопкой выбора цвета, пишущей результат прямо в
// переданное поле конфига. Указатель безопасен: a.cfg живёт столько же,
// сколько окно, и заменяется целиком только в confirmReset — который
// пересобирает окно.
func (a *App) color(title, subtitle string, field *string) *adw.ActionRow {
	row := adw.NewActionRow()
	row.SetTitle(title)
	if subtitle != "" {
		row.SetSubtitle(subtitle)
	}
	dlg := gtk.NewColorDialog()
	dlg.SetWithAlpha(true)
	btn := gtk.NewColorDialogButton(dlg)
	btn.SetVAlign(gtk.AlignCenter)
	btn.SetRGBA(hexToRGBA(*field))
	btn.Connect("notify::rgba", func() {
		hex := rgbaToHex(btn.RGBA())
		a.edit(func() { *field = hex })
	})
	row.AddSuffix(btn)
	row.SetActivatableWidget(btn)
	return row
}

// edit применяет правку к конфигу и ставит отложенное сохранение.
func (a *App) edit(mutate func()) {
	mutate()
	// Окно перекрашивается сразу, а файл пишется с задержкой: цвет должен
	// отзываться на глаз мгновенно, бару же лишние SIGUSR1 ни к чему.
	a.style.apply(a.cfg)
	a.scheduleApply()
}

func (a *App) scheduleApply() {
	if a.pendingID != 0 {
		glib.SourceRemove(a.pendingID)
	}
	a.pendingID = glib.TimeoutAdd(applyDelayMS, func() bool {
		a.pendingID = 0
		a.applyNow()
		return false
	})
}

// flush досрочно выполняет отложенное сохранение, если оно было назначено.
func (a *App) flush() {
	if a.pendingID == 0 {
		return
	}
	glib.SourceRemove(a.pendingID)
	a.pendingID = 0
	a.applyNow()
}

func (a *App) applyNow() {
	data, err := Marshal(a.cfg)
	if err != nil {
		a.toast(fmt.Sprintf("Не сохранилось: %v", err))
		return
	}
	// Ползунок, вернувшийся в исходное значение, и повторное «Применить» с тем
	// же текстом не должны стоить ни записи, ни сигнала: на SIGUSR1 бар
	// пересоздаёт поверхность и заново поднимает шрифт, и это видно глазом.
	if a.lastSaved != nil && bytes.Equal(a.lastSaved, data) {
		return
	}
	if err := SaveBytes(a.path, data); err != nil {
		a.toast(fmt.Sprintf("Не сохранилось: %v", err))
		return
	}
	a.lastSaved = data
	if err := Reload(); err != nil {
		a.toast(fmt.Sprintf("Сохранено, но бар не перечитал: %v", err))
	}
}

func (a *App) toast(msg string) {
	if a.win == nil {
		log.Print(msg)
		return
	}
	a.win.AddToast(adw.NewToast(msg))
}

func (a *App) confirmReset() {
	d := adw.NewAlertDialog("Сбросить настройки бара?",
		"Настройки бара вернутся к заводским, "+a.path+" будет перезаписан. "+
			"Тема, окна, композитор и сессия не изменятся.")
	d.AddResponse("cancel", "Отмена")
	d.AddResponse("reset", "Сбросить")
	d.SetResponseAppearance("reset", adw.ResponseDestructive)
	d.SetDefaultResponse("cancel")
	d.SetCloseResponse("cancel")
	d.ConnectResponse(func(resp string) {
		if resp != "reset" {
			return
		}
		a.cfg = DefaultConfig()
		a.applyNow()
		a.rebuildWindow()
	})
	d.Present(a.win)
}

// rebuildWindow пересобирает окно поверх текущего конфига. Нужна там, где
// значения поменялись мимо виджетов (сброс настроек, возврат команд питания):
// строки держат старый текст, а привязка к полям идёт по указателям на прежний
// a.cfg. Пересобрать целиком дешевле и надёжнее, чем обходить каждую строку и
// глушить её сигналы.
//
// Новое окно поднимаем до закрытия старого: закрыть единственное окно
// приложения значит дать GApplication повод завершиться, и настройки просто
// исчезали бы вместо пересборки.
func (a *App) rebuildWindow() {
	old := a.win
	a.build(nil)
	if old != nil {
		old.Close()
	}
}

func monitorNames() []string {
	disp := gdk.DisplayGetDefault()
	if disp == nil {
		return []string{"0"}
	}
	mons := disp.Monitors()
	n := int(mons.NItems())
	if n == 0 {
		return []string{"0"}
	}
	out := make([]string, 0, n)
	for i := 0; i < n; i++ {
		label := fmt.Sprintf("%d", i)
		if obj := mons.Item(uint(i)); obj != nil {
			if m, ok := obj.Cast().(*gdk.Monitor); ok {
				geo := m.Geometry()
				label = fmt.Sprintf("%d — %s (%d×%d)", i, m.Connector(), geo.Width(), geo.Height())
			}
		}
		out = append(out, label)
	}
	return out
}
