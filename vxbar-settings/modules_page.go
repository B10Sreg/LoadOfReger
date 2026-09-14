package main

import (
	"github.com/diamondburned/gotk4-adwaita/pkg/adw"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// zone связывает одну из трёх колонок бара с её группой в интерфейсе.
// Группа полностью перестраивается на каждое изменение: список короткий
// (максимум шесть строк), а инкрементальная правка виджетов здесь дороже
// в сопровождении, чем пересборка.
type zone struct {
	title string
	desc  string
	get   func(*Config) []string
	set   func(*Config, []string)
	group *adw.PreferencesGroup
	// PreferencesGroup не отдаёт список своих детей, поэтому строки, которые
	// мы в неё добавили, приходится помнить самим, чтобы уметь их снять.
	rows []gtk.Widgetter
}

func (a *App) pageModules() *adw.PreferencesPage {
	page := adw.NewPreferencesPage()
	page.SetTitle("Модули")
	page.SetIconName("view-list-symbolic")

	zones := []*zone{
		{
			title: "Слева",
			desc:  "Прижимается к левому краю",
			get:   func(c *Config) []string { return c.Modules.Left },
			set:   func(c *Config, v []string) { c.Modules.Left = v },
		},
		{
			title: "По центру",
			desc:  "Центрируется по ширине бара",
			get:   func(c *Config) []string { return c.Modules.Center },
			set:   func(c *Config, v []string) { c.Modules.Center = v },
		},
		{
			title: "Справа",
			desc:  "Прижимается к правому краю",
			get:   func(c *Config) []string { return c.Modules.Right },
			set:   func(c *Config, v []string) { c.Modules.Right = v },
		},
	}

	for _, z := range zones {
		z.group = adw.NewPreferencesGroup()
		z.group.SetTitle(z.title)
		z.group.SetDescription(z.desc)
		page.Add(z.group)
	}

	// Перестраиваем сразу все три зоны: перенос модуля меняет две из них,
	// и отрисовывать их порознь пришлось бы с оглядкой на порядок.
	var rebuild func()
	rebuild = func() {
		for _, z := range zones {
			a.fillZone(z, zones, rebuild)
		}
	}
	rebuild()

	return page
}

func (a *App) fillZone(z *zone, all []*zone, rebuild func()) {
	// PreferencesGroup не отдаёт список детей, поэтому держим строки сами.
	for _, w := range z.rows {
		z.group.Remove(w)
	}
	z.rows = nil

	items := z.get(&a.cfg)

	for i, id := range items {
		i, id := i, id
		row := adw.NewActionRow()
		row.SetTitle(moduleTitle(id))
		row.SetSubtitle(id)

		box := gtk.NewBox(gtk.OrientationHorizontal, 4)
		box.SetVAlign(gtk.AlignCenter)

		up := gtk.NewButtonFromIconName("go-up-symbolic")
		up.SetTooltipText("Выше")
		up.SetSensitive(i > 0)
		up.ConnectClicked(func() {
			a.edit(func() { z.set(&a.cfg, swap(z.get(&a.cfg), i, i-1)) })
			rebuild()
		})

		down := gtk.NewButtonFromIconName("go-down-symbolic")
		down.SetTooltipText("Ниже")
		down.SetSensitive(i < len(items)-1)
		down.ConnectClicked(func() {
			a.edit(func() { z.set(&a.cfg, swap(z.get(&a.cfg), i, i+1)) })
			rebuild()
		})

		move := gtk.NewMenuButton()
		move.SetIconName("view-sort-descending-symbolic")
		move.SetTooltipText("Перенести в другую зону")
		move.SetPopover(a.movePopover(z, all, i, id, rebuild))

		del := gtk.NewButtonFromIconName("list-remove-symbolic")
		del.SetTooltipText("Убрать из бара")
		del.ConnectClicked(func() {
			a.edit(func() { z.set(&a.cfg, removeAt(z.get(&a.cfg), i)) })
			rebuild()
		})

		for _, b := range []*gtk.Button{up, down, del} {
			b.AddCSSClass("flat")
		}
		move.AddCSSClass("flat")

		box.Append(up)
		box.Append(down)
		box.Append(move)
		box.Append(del)
		row.AddSuffix(box)

		z.group.Add(row)
		z.rows = append(z.rows, row)
	}

	// Строка добавления: показываем только те модули, которых нет ни в одной
	// зоне — один и тот же модуль дважды бар рисовать не умеет.
	addRow := adw.NewActionRow()
	free := a.freeModules()
	if len(free) == 0 {
		addRow.SetTitle("Все модули уже размещены")
		addRow.SetActivatable(false)
		addRow.SetSensitive(false)
	} else {
		addRow.SetTitle("Добавить модуль")
		btn := gtk.NewMenuButton()
		btn.SetIconName("list-add-symbolic")
		btn.AddCSSClass("flat")
		btn.SetVAlign(gtk.AlignCenter)
		btn.SetPopover(a.addPopover(z, free, rebuild))
		addRow.AddSuffix(btn)
		addRow.SetActivatableWidget(btn)
	}
	z.group.Add(addRow)
	z.rows = append(z.rows, addRow)
}

func (a *App) freeModules() []string {
	used := append(append(append([]string{},
		a.cfg.Modules.Left...), a.cfg.Modules.Center...), a.cfg.Modules.Right...)
	var out []string
	for _, m := range KnownModules {
		if !contains(used, m) {
			out = append(out, m)
		}
	}
	return out
}

func (a *App) addPopover(z *zone, free []string, rebuild func()) *gtk.Popover {
	box := gtk.NewBox(gtk.OrientationVertical, 2)
	pop := gtk.NewPopover()
	pop.SetChild(box)
	for _, id := range free {
		id := id
		b := gtk.NewButtonWithLabel(moduleTitle(id))
		b.AddCSSClass("flat")
		b.SetHAlign(gtk.AlignFill)
		if c := b.Child(); c != nil {
			if lbl, ok := c.(*gtk.Label); ok {
				lbl.SetXAlign(0)
			}
		}
		b.ConnectClicked(func() {
			pop.Popdown()
			a.edit(func() { z.set(&a.cfg, append(z.get(&a.cfg), id)) })
			rebuild()
		})
		box.Append(b)
	}
	return pop
}

func (a *App) movePopover(from *zone, all []*zone, idx int, id string, rebuild func()) *gtk.Popover {
	box := gtk.NewBox(gtk.OrientationVertical, 2)
	pop := gtk.NewPopover()
	pop.SetChild(box)
	for _, to := range all {
		if to == from {
			continue
		}
		to := to
		b := gtk.NewButtonWithLabel("В зону «" + to.title + "»")
		b.AddCSSClass("flat")
		b.SetHAlign(gtk.AlignFill)
		if c := b.Child(); c != nil {
			if lbl, ok := c.(*gtk.Label); ok {
				lbl.SetXAlign(0)
			}
		}
		b.ConnectClicked(func() {
			pop.Popdown()
			a.edit(func() {
				from.set(&a.cfg, removeAt(from.get(&a.cfg), idx))
				to.set(&a.cfg, append(to.get(&a.cfg), id))
			})
			rebuild()
		})
		box.Append(b)
	}
	return pop
}
