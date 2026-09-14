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

	app *adw.Application
	win *adw.PreferencesWindow

	pendingID glib.SourceHandle
	// Пока идёт загрузка значений в виджеты, их сигналы менять конфиг не должны.
	loading bool
}

func main() {
	app := adw.NewApplication("dev.reg.vxbar.Settings", gio.ApplicationFlagsNone)
	a := &App{path: ConfigPath()}

	cfg, err := Load(a.path)
	if err != nil {
		// Битый TOML — не повод не открыться: показываем дефолты и говорим,
		// что при первом же изменении файл будет перезаписан.
		log.Printf("vxbar-settings: %v", err)
	}
	a.cfg = cfg

	a.app = app
	app.ConnectActivate(func() { a.build(err) })
	if code := app.Run(os.Args); code > 0 {
		os.Exit(code)
	}
}

func (a *App) build(loadErr error) {
	a.win = adw.NewPreferencesWindow()
	a.win.SetApplication(&a.app.Application)
	a.win.SetTitle("Настройки vxbar")
	a.win.SetDefaultSize(600, 720)
	a.win.SetSearchEnabled(true)

	a.win.Add(a.pageBar())
	a.win.Add(a.pageStyle())
	a.win.Add(a.pageTags())
	a.win.Add(a.pageModules())

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
	pos.SetModel(gtk.NewStringList([]string{"Сверху", "Снизу"}))
	if a.cfg.Bar.Position == "bottom" {
		pos.SetSelected(1)
	}
	pos.Connect("notify::selected", func() {
		a.edit(func() {
			if pos.Selected() == 1 {
				a.cfg.Bar.Position = "bottom"
			} else {
				a.cfg.Bar.Position = "top"
			}
		})
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
	gg.Add(a.spin("Высота", "пикселей", 12, 96, 1, 0,
		float64(a.cfg.Bar.Height),
		func(v float64) { a.cfg.Bar.Height = int(v) }))
	gg.Add(a.spin("Отступ от края", "поднимает бар над краем экрана — «плавающий» вид", 0, 64, 1, 0,
		float64(a.cfg.Bar.MarginEdge),
		func(v float64) { a.cfg.Bar.MarginEdge = int(v) }))
	gg.Add(a.spin("Отступ по бокам", "сужает бар слева и справа", 0, 400, 1, 0,
		float64(a.cfg.Bar.MarginSide),
		func(v float64) { a.cfg.Bar.MarginSide = int(v) }))
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
	btn.SetValign(gtk.AlignCenter)
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
	rb.SetValign(gtk.AlignCenter)
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

	gc := adw.NewPreferencesGroup()
	gc.SetTitle("Часы")

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

// color строит строку с кнопкой выбора цвета, писающей результат прямо в
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
	btn.SetValign(gtk.AlignCenter)
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
	if a.loading {
		return
	}
	mutate()
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
