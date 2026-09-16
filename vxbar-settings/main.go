package main

import (
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

type App struct {
	path string
	cfg  Config

	// Настройки риса (тема, окна, композитор, сессия) живут в своём файле и
	// применяются своим генератором, поэтому у них отдельный отложенный
	// таймер: правка ползунка бара не должна дёргать picom.
	rice          Rice
	ricePendingID glib.SourceHandle

	app *adw.Application
	win *adw.PreferencesWindow

	pendingID glib.SourceHandle
	style     *styler

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
	a.win.Add(a.pageWidgets())
	a.win.Add(a.pageSession())
	a.applyOrientation()

	// Правка, сделанная за миг до закрытия, иначе терялась: таймер на 180 мс
	// не успевал сработать и умирал вместе с циклом событий.
	a.win.ConnectCloseRequest(func() bool {
		a.flush()
		a.flushRice()
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
	reset.SetTitle("Сбросить всё на значения по умолчанию")
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

	fmtRow := adw.NewEntryRow()
	fmtRow.SetTitle("Формат (strftime)")
	fmtRow.SetText(a.cfg.Clock.Format)
	fmtRow.SetShowApplyButton(true)
	applyFmt := func() {
		v := fmtRow.Text()
		a.edit(func() { a.cfg.Clock.Format = v })
	}
	fmtRow.ConnectApply(applyFmt)
	fmtRow.ConnectEntryActivated(applyFmt)
	gc.Add(fmtRow)

	clickRow := adw.NewEntryRow()
	clickRow.SetTitle("Команда по клику на часы")
	clickRow.SetText(a.cfg.Clock.OnClick)
	clickRow.SetShowApplyButton(true)
	applyClick := func() {
		v := clickRow.Text()
		a.edit(func() { a.cfg.Clock.OnClick = v })
	}
	clickRow.ConnectApply(applyClick)
	clickRow.ConnectEntryActivated(applyClick)
	gc.Add(clickRow)
	page.Add(gc)

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
	if err := Save(a.path, a.cfg); err != nil {
		a.toast(fmt.Sprintf("Не сохранилось: %v", err))
		return
	}
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
	d := adw.NewAlertDialog("Сбросить настройки?",
		"Все значения вернутся к заводским. Текущий "+a.path+" будет перезаписан.")
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
		// Виджеты держат старые значения, а привязка к полям — по указателям
		// на прежний a.cfg. Пересобираем окно целиком: дешевле и надёжнее,
		// чем обходить каждую строку и глушить её сигналы.
		old := a.win
		a.win = nil
		old.Close()
		a.build(nil)
	})
	d.Present(a.win)
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
