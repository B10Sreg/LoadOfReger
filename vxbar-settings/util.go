package main

import (
	"strings"

	"github.com/diamondburned/gotk4/pkg/pango"
)

// Подписи тегов хранятся списком, а правятся одной строкой: разделитель —
// пробел, потому что сами подписи в рисе односимвольные (цифры, иероглифы,
// значки Nerd Font).
func joinLabels(v []string) string { return strings.Join(v, " ") }

func splitLabels(s string) []string {
	f := strings.Fields(s)
	if f == nil {
		return []string{}
	}
	return f
}

// Строка шрифта в конфиге — в формате Pango ("JetBrainsMono Nerd Font 10"),
// том же, что понимает pango_font_description_from_string в баре.
func pangoDescFromString(s string) *pango.FontDescription {
	if strings.TrimSpace(s) == "" {
		s = "Sans 10"
	}
	return pango.FontDescriptionFromString(s)
}

// remove/move — мелкая арифметика для переупорядочивания списка модулей.
func removeAt(v []string, i int) []string {
	if i < 0 || i >= len(v) {
		return v
	}
	out := make([]string, 0, len(v)-1)
	out = append(out, v[:i]...)
	return append(out, v[i+1:]...)
}

func swap(v []string, i, j int) []string {
	if i < 0 || j < 0 || i >= len(v) || j >= len(v) {
		return v
	}
	out := append([]string(nil), v...)
	out[i], out[j] = out[j], out[i]
	return out
}

func contains(v []string, s string) bool {
	for _, x := range v {
		if x == s {
			return true
		}
	}
	return false
}

// Человеческие названия модулей для интерфейса.
var moduleTitles = map[string]string{
	"tags":    "Теги рабочих столов",
	"title":   "Заголовок активного окна",
	"cpu":     "Загрузка процессора",
	"ram":     "Оперативная память",
	"volume":  "Громкость",
	"clock":   "Часы",
	"temp":    "Температура",
	"net":     "Сеть",
	"music":   "Проигрыватель",
	"disk":    "Свободное место",
	"mic":     "Микрофон",
	"battery": "Батарея",
	"uptime":  "Время работы",
	"power":   "Кнопка питания",
}

// Что модуль показывает и что умеет по клику. Идёт подписью в списке
// доступных: по одному имени неочевидно, чем "uptime" отличается от "clock" и
// что кнопка питания вообще кликается.
var moduleHints = map[string]string{
	"tags":    "Рабочие столы, клик переключает",
	"title":   "Заголовок активного окна",
	"cpu":     "Процент загрузки, клик — топ процессов",
	"ram":     "Занято и всего, клик — топ по памяти",
	"temp":    "Процессор и видеокарта по датчикам hwmon",
	"net":     "Скорость приёма и отдачи по активному интерфейсу",
	"disk":    "Свободно на разделе, клик — список разделов",
	"music":   "Текущий трек, клик — пауза, колесо — соседний",
	"volume":  "Клик — регулятор, колесо — ±5%",
	"mic":     "Клик — выключить и включить микрофон",
	"battery": "Заряд; на машине без батареи модуль не появляется",
	"uptime":  "Сколько машина на ногах",
	"power":   "Клик — гибернация, сон, блокировка, выключение",
}

func moduleHint(id string) string { return moduleHints[id] }

func moduleTitle(id string) string {
	if t, ok := moduleTitles[id]; ok {
		return t
	}
	return id
}
