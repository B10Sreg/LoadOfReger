package main

import (
	"fmt"
	"os/exec"

	"github.com/diamondburned/gotk4-adwaita/pkg/adw"
	"github.com/diamondburned/gotk4/pkg/core/glib"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// Роли палитры, показываемые образцами на странице «Тема», и их подписи.
// Порядок тот же, что в палитре: сначала обычное окно, потом активное.
var paletteRoles = []struct{ key, label string }{
	{"norm_bg", "Фон"},
	{"norm_fg", "Текст"},
	{"norm_border", "Рамка"},
	{"sel_bg", "Фон активного"},
	{"sel_fg", "Текст активного"},
	{"sel_border", "Акцент"},
}

// riceEdit -- то же, что edit, но для настроек риса: правка в память,
// отложенная запись и вызов генератора. Разделено с настройками бара
// намеренно: у бара свой файл и свой способ применения (SIGUSR1), и склеивать
// их значило бы дёргать генератор на каждое движение ползунка высоты бара.
func (a *App) riceEdit(mutate func()) {
	mutate()
	a.scheduleRiceApply()
}

func (a *App) scheduleRiceApply() {
	if a.ricePendingID != 0 {
		glib.SourceRemove(a.ricePendingID)
	}
	a.ricePendingID = glib.TimeoutAdd(applyDelayMS, func() bool {
		a.ricePendingID = 0
		a.applyRiceNow(false)
		return false
	})
}

func (a *App) flushRice() {
	if a.ricePendingID == 0 {
		return
	}
	glib.SourceRemove(a.ricePendingID)
	a.ricePendingID = 0
	a.applyRiceNow(false)
}

// applyRiceNow пишет конфиг и разворачивает его. Генератор перезапускает picom
// и dunst, поэтому themeOnly бережёт от лишних морганий, когда менялась только
// тема.
func (a *App) applyRiceNow(themeOnly bool) {
	if err := SaveRice(a.rice); err != nil {
		a.toast(fmt.Sprintf("Не сохранилось: %v", err))
		return
	}
	if err := ApplyRice(themeOnly); err != nil {
		a.toast(fmt.Sprintf("Сохранено, но не применилось: %v", err))
	}
}

// ------------------------------------------------------------------- тема

func (a *App) pageTheme() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Тема")
	page.SetIconName("applications-graphics-symbolic")

	group := adw.NewPreferencesGroup()
	group.SetTitle("Тема")
	group.SetDescription("Палитра задаёт цвета окон, бара, меню, терминала и уведомлений разом")

	names := Palettes()
	if len(names) == 0 {
		row := adw.NewActionRow()
		row.SetTitle("Палитры не найдены")
		row.SetSubtitle("Ожидались в " + RiceDir() + "/palettes")
		group.Add(row)
		page.Add(group)
		page.Add(a.windowsGroup())
		return page
	}

	combo := adw.NewComboRow()
	combo.SetTitle("Тема")
	combo.SetModel(gtk.NewStringList(names))
	for i, n := range names {
		if n == a.rice.Theme.Name {
			combo.SetSelected(uint(i))
		}
	}

	// Описание и образцы обновляются вместе с выбором, поэтому держим их
	// рядом и перерисовываем одной функцией.
	descRow := adw.NewActionRow()
	descRow.SetTitle("Описание")
	swatchRow := adw.NewActionRow()
	swatchRow.SetTitle("Палитра")
	swatches := gtk.NewBox(gtk.OrientationHorizontal, 4)
	swatches.SetVAlign(gtk.AlignCenter)
	swatchRow.AddSuffix(swatches)
	wallRow := adw.NewActionRow()
	wallRow.SetTitle("Обои")

	refresh := func(name string) {
		pal, err := LoadPalette(name)
		if err != nil {
			descRow.SetSubtitle(fmt.Sprintf("палитра не прочиталась: %v", err))
			return
		}
		descRow.SetSubtitle(pal.Description)

		for swatches.FirstChild() != nil {
			swatches.Remove(swatches.FirstChild())
		}
		for _, role := range paletteRoles {
			hex, ok := pal.UI[role.key]
			if !ok {
				continue
			}
			swatches.Append(colorSwatch(hex, role.label))
		}

		if path, ok := HasWallpaper(name); ok {
			wallRow.SetSubtitle(path)
		} else {
			// Это не ошибка: apply.py в таком случае оставляет прежние обои.
			wallRow.SetSubtitle("нет файла — останутся прежние")
		}
	}
	refresh(a.rice.Theme.Name)

	combo.Connect("notify::selected", func() {
		name := names[combo.Selected()]
		if name == a.rice.Theme.Name {
			return
		}
		refresh(name)
		a.rice.Theme.Name = name
		// Тему применяем сразу и без задержки: это одно осознанное действие,
		// а не перетаскивание ползунка, и ждать после него нечего.
		a.applyRiceNow(true)
		a.toast("Тема: " + name)
	})

	group.Add(combo)
	group.Add(descRow)
	group.Add(swatchRow)
	group.Add(wallRow)
	page.Add(group)
	page.Add(a.windowsGroup())
	return page
}

// colorSwatch -- квадратик цвета с подсказкой. Рисуем через CSS: отдельный
// виджет-образец в GTK4 для этого не предусмотрен.
func colorSwatch(hex, tooltip string) *gtk.Box {
	box := gtk.NewBox(gtk.OrientationHorizontal, 0)
	box.SetSizeRequest(22, 22)
	box.SetTooltipText(fmt.Sprintf("%s: %s", tooltip, hex))

	css := gtk.NewCSSProvider()
	css.LoadFromData(fmt.Sprintf(
		"box{background:%s;border:1px solid alpha(currentColor,.25);border-radius:5px;}",
		hex))
	box.StyleContext().AddProvider(css, gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
	return box
}

// ------------------------------------------------------------------- окна

// windowsGroup -- зазор и рамка. Живёт на странице темы: это тоже про
// внешний вид, а отдельная страница ради двух ползунков только удлиняла бы
// переключатель, где подписи и так режутся.
func (a *App) windowsGroup() *adw.PreferencesGroup {
	group := adw.NewPreferencesGroup()
	group.SetTitle("Раскладка")
	group.SetDescription("Едут через X resources, поэтому применяются сразу — без пересборки vxwm")

	group.Add(a.riceSpin("Зазор", "Расстояние между окнами, px", 0, 200, 1, 0,
		float64(a.rice.Windows.Gap),
		func(v float64) { a.rice.Windows.Gap = int(v) }))
	group.Add(a.riceSpin("Рамка", "Толщина рамки окна, px", 0, 32, 1, 0,
		float64(a.rice.Windows.Border),
		func(v float64) { a.rice.Windows.Border = int(v) }))

	return group
}

// ------------------------------------------------------------ композитор

func (a *App) pageCompositor() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Эффекты")
	page.SetIconName("preferences-desktop-effects-symbolic")

	main := adw.NewPreferencesGroup()
	main.SetTitle("picom")
	main.SetDescription("Каждая правка перезапускает композитор — экран моргнёт")
	main.Add(a.riceSwitch("Включён", "Без него не будет ни теней, ни прозрачности, ни скруглений",
		a.rice.Compositor.Enabled, func(v bool) { a.rice.Compositor.Enabled = v }))
	main.Add(a.riceSwitch("Вертикальная синхронизация", "Убирает разрыв кадра",
		a.rice.Compositor.Vsync, func(v bool) { a.rice.Compositor.Vsync = v }))
	main.Add(a.riceSwitch("Анимации", "Появление и закрытие окон, переключение тегов",
		a.rice.Compositor.Animations, func(v bool) { a.rice.Compositor.Animations = v }))
	main.Add(a.riceSwitch("Плавное проявление", "",
		a.rice.Compositor.Fading, func(v bool) { a.rice.Compositor.Fading = v }))
	main.Add(a.riceSpin("Скругление углов", "Радиус, px", 0, 32, 1, 0,
		float64(a.rice.Compositor.CornerRadius),
		func(v float64) { a.rice.Compositor.CornerRadius = int(v) }))
	page.Add(main)

	shadow := adw.NewPreferencesGroup()
	shadow.SetTitle("Тени")
	shadow.Add(a.riceSwitch("Тени", "", a.rice.Compositor.Shadow,
		func(v bool) { a.rice.Compositor.Shadow = v }))
	shadow.Add(a.riceSpin("Радиус", "Он же задаёт смещение тени", 0, 60, 1, 0,
		float64(a.rice.Compositor.ShadowRadius),
		func(v float64) { a.rice.Compositor.ShadowRadius = int(v) }))
	shadow.Add(a.riceSpin("Непрозрачность", "", 0, 1, 0.05, 2,
		a.rice.Compositor.ShadowOpacity,
		func(v float64) { a.rice.Compositor.ShadowOpacity = v }))
	page.Add(shadow)

	blur := adw.NewPreferencesGroup()
	blur.SetTitle("Размытие и прозрачность")
	blur.Add(a.riceSwitch("Размытие фона", "Стеклянный фон у бара, меню и терминала",
		a.rice.Compositor.Blur, func(v bool) { a.rice.Compositor.Blur = v }))
	blur.Add(a.riceSpin("Сила размытия", "", 0, 20, 1, 0,
		float64(a.rice.Compositor.BlurStrength),
		func(v float64) { a.rice.Compositor.BlurStrength = int(v) }))
	blur.Add(a.riceSpin("Непрозрачность бара", "1.0 — полностью непрозрачный", 0.1, 1, 0.02, 2,
		a.rice.Compositor.BarOpacity,
		func(v float64) { a.rice.Compositor.BarOpacity = v }))
	page.Add(blur)

	return page
}

// ------------------------------------------------------ сессия и питание

func (a *App) pageSession() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Сессия")
	page.SetIconName("system-shutdown-symbolic")

	session := adw.NewPreferencesGroup()
	session.SetTitle("Восстановление сессии")
	session.SetDescription("Возвращает окна, открытые в прошлый раз: программу, тег и каталог. " +
		"Вкладки и несохранённое так не вернуть — для этого есть гибернация")
	session.Add(a.riceSwitch("Восстанавливать окна", "При входе в сессию",
		a.rice.Session.Restore, func(v bool) { a.rice.Session.Restore = v }))
	session.Add(a.riceSpin("Интервал снимка", "Секунды. Нужен на случай, когда гибернация не случилась",
		5, 300, 5, 0, float64(a.rice.Session.SnapshotInterval),
		func(v float64) { a.rice.Session.SnapshotInterval = int(v) }))
	page.Add(session)

	power := adw.NewPreferencesGroup()
	power.SetTitle("Блокировка")
	if !haveXssLock() {
		// Молча показывать переключатель, который ничего не делает, хуже, чем
		// сказать почему.
		power.SetDescription("Требуется пакет xss-lock — сейчас не установлен, настройка не действует")
	}
	power.Add(a.riceSwitch("Блокировать по простою", "",
		a.rice.Power.LockOnIdle, func(v bool) { a.rice.Power.LockOnIdle = v }))
	power.Add(a.riceSpin("Простой до блокировки", "Секунды", 30, 3600, 30, 0,
		float64(a.rice.Power.IdleSeconds),
		func(v float64) { a.rice.Power.IdleSeconds = int(v) }))
	page.Add(power)

	note := adw.NewPreferencesGroup()
	note.SetTitle("Применение")
	row := adw.NewActionRow()
	row.SetTitle("Часть настроек подхватывается при следующем входе")
	row.SetSubtitle("Блокировка по простою поднимается в autostart.sh: сейчас изменится только конфиг")
	note.Add(row)
	page.Add(note)

	return page
}

// Блокировку по простою поднимает xss-lock, и без пакета настройка ничего не
// делает. Проверяем наличие, чтобы сказать об этом прямо на странице.
func haveXssLock() bool {
	_, err := exec.LookPath("xss-lock")
	return err == nil
}

// ----------------------------------------------------------- общие строки

func (a *App) riceSpin(title, subtitle string, min, max, step float64, digits uint,
	val float64, onSet func(float64)) *adw.SpinRow {

	adj := gtk.NewAdjustment(val, min, max, step, step*5, 0)
	row := adw.NewSpinRow(adj, step, digits)
	row.SetTitle(title)
	if subtitle != "" {
		row.SetSubtitle(subtitle)
	}
	adj.Connect("value-changed", func() {
		v := adj.Value()
		a.riceEdit(func() { onSet(v) })
	})
	return row
}

func (a *App) riceSwitch(title, subtitle string, val bool, onSet func(bool)) *adw.SwitchRow {
	row := adw.NewSwitchRow()
	row.SetTitle(title)
	if subtitle != "" {
		row.SetSubtitle(subtitle)
	}
	row.SetActive(val)
	row.Connect("notify::active", func() {
		v := row.Active()
		a.riceEdit(func() { onSet(v) })
	})
	return row
}
