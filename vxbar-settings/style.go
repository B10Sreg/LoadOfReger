package main

import (
	"fmt"

	"github.com/diamondburned/gotk4-adwaita/pkg/adw"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// Окно настроек красится теми же цветами, что и сам бар: смысл не в
// украшательстве, а в предпросмотре -- поменяв фон или акцент, видно результат
// сразу на странице, а не только после «Применить» на панели.
//
// Провайдер один на всё приложение и перезаряжается на месте: добавлять новый
// на каждую правку цвета нельзя, они копились бы на дисплее до выхода.
// Приоритет выше GTK_STYLE_PROVIDER_PRIORITY_USER (800): libadwaita 1.9
// раскладывает свою палитру провайдером, который стоит на уровне USER, и на
// равных наши --accent-bg-color и прочее проигрывали ему по порядку добавления.
const cssPriority = 900

type styler struct {
	provider *gtk.CSSProvider
}

func newStyler() *styler {
	adw.StyleManagerGetDefault().SetColorScheme(adw.ColorSchemeForceDark)

	s := &styler{provider: gtk.NewCSSProvider()}
	if disp := gdk.DisplayGetDefault(); disp != nil {
		// Именно USER, а не APPLICATION: libadwaita 1.6+ подмешивает свою
		// палитру (в том числе системный акцент) провайдером, который стоит
		// выше APPLICATION, и наш акцент оставался бы синим.
		gtk.StyleContextAddProviderForDisplay(disp, s.provider, cssPriority)
	}
	return s
}

func (s *styler) apply(cfg Config) {
	s.provider.LoadFromString(css(cfg))
}

// Палитра риса графитовая и почти монохромная, поэтому именованные цвета
// libadwaita переопределяются напрямую значениями из конфига. Альфу режем:
// бар живёт на ARGB-визуале и может быть полупрозрачным, а полупрозрачный фон
// окна настроек означал бы дыру в окне.
func css(cfg Config) string {
	bg := opaque(cfg.Style.Background)
	fg := opaque(cfg.Style.Foreground)
	accent := opaque(cfg.Style.Accent)
	card := opaque(cfg.Tags.ActiveBG)

	// Цвета задаются прямыми селекторами, а не через палитру libadwaita:
	// @define-color в GTK4 виден только правилам того же провайдера, а
	// переопределить --accent-bg-color и соседей не выходит ни на каком
	// приоритете -- свои значения библиотека ставит после нас.
	return fmt.Sprintf(`
window.background,
preferencespage,
preferencespage > scrolledwindow,
viewswitcherbar,
headerbar {
	background-color: %[1]s;
	color: %[2]s;
}

/* Плашки строк: скругление берём у бара, чтобы настройка читалась как
   предпросмотр, а не как случайная величина. */
.card,
list.boxed-list,
popover > contents {
	background-color: %[4]s;
	color: %[2]s;
	border-radius: %[5]dpx;
}

list.boxed-list > row {
	background-color: transparent;
}

/* Акцент вместо синего по умолчанию. Строку в фокусе подсвечиваем только
   заголовком: фон у строк рисует внутренний виджет, и background-color на
   самой row не виден -- получалась бы тёмная надпись на тёмном фоне. */
row:focus-within > box > label.title,
row:focus-within label.title {
	color: %[3]s;
}

viewswitcher button:checked,
viewswitcherbar button:checked {
	background-color: %[4]s;
	color: %[3]s;
}

switch:checked,
checkbutton:checked > check,
radiobutton:checked > check {
	background-color: %[3]s;
	color: %[1]s;
}

/* Идентификатор модуля -- это то, что пишется в TOML; моноширинный шрифт
   отделяет его от человеческого названия сверху. Класс вешается только на
   строки модулей: общее правило по label.subtitle раздуло бы подписи во всём
   окне и они полезли бы под спинбоксы. */
row.vxbar-module label.subtitle {
	font-family: monospace;
	font-size: 0.85em;
	opacity: 0.7;
}

/* Поле ввода в строках настроек. GTK4 всегда читает ~/.config/gtk-4.0/gtk.css,
   независимо от GTK_THEME, и тема пользователя даёт entry рамку, фон и отступы
   в 8px. В libadwaita поле внутри строки задумано плоским, и с чужими рамками
   оно разрастается и накрывает подпись своим непрозрачным фоном. Возвращаем
   плоский вид -- иначе половина строк во всём окне нечитаема. */
row spinbutton:not(.vertical),
row spinbutton > text,
row entry {
	border: none;
	background: none;
	box-shadow: none;
	min-height: 0;
	padding-left: 2px;
	padding-right: 2px;
}

/* Кнопки -/+ у спинбокса от той же темы получают собственный фон и раздувают
   строку по высоте. */
row spinbutton button {
	min-height: 0;
	min-width: 24px;
	padding: 0 2px;
	background: none;
	border: none;
	box-shadow: none;
}
`, bg, fg, accent, card, radiusPx(cfg.Style.Radius))
}

// opaque приводит цвет конфига к форме #rrggbb, понятной CSS, и отбрасывает
// альфу. Мусор в конфиге даёт мадженту -- ровно как в баре.
func opaque(spec string) string {
	c := hexToRGBA(spec)
	q := func(v float32) uint8 { return uint8(clamp01(v)*255 + 0.5) }
	return fmt.Sprintf("#%02x%02x%02x", q(c.Red()), q(c.Green()), q(c.Blue()))
}

// Скругление бара может быть нулевым или огромным; у карточек в окне оба
// края выглядят плохо, поэтому держим их в разумных пределах.
func radiusPx(r float64) int {
	v := int(r + 0.5)
	if v < 6 {
		return 6
	}
	if v > 18 {
		return 18
	}
	return v
}
