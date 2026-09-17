package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"

	"github.com/pelletier/go-toml/v2"
)

// Структуры зеркалят vxbar/src/config.rs. Имена полей в TOML заданы явно,
// чтобы расхождение с Rust-стороной ловилось глазами, а не на рантайме.
// При добавлении поля в config.rs его нужно добавить и сюда, иначе
// "Применить" молча сотрёт его из файла.

type Config struct {
	Bar     Bar     `toml:"bar"`
	Style   Style   `toml:"style"`
	Tags    Tags    `toml:"tags"`
	Modules Modules `toml:"modules"`
	Clock   Clock   `toml:"clock"`
	Popups  Popups  `toml:"popups"`
	Music   Music   `toml:"music"`
	Power   Power   `toml:"power"`
	Disk    Disk    `toml:"disk"`
}

type Bar struct {
	Position   string `toml:"position"` // "top" | "bottom" | "left" | "right"
	Height     int    `toml:"height"`   // толщина: высота у горизонтального, ширина у вертикального
	MarginEdge int    `toml:"margin_edge"`
	MarginSide int    `toml:"margin_side"`
	Monitor    int    `toml:"monitor"`
	IntervalMS int    `toml:"interval_ms"`
}

type Style struct {
	Background string  `toml:"background"`
	Foreground string  `toml:"foreground"`
	Accent     string  `toml:"accent"`
	Muted      string  `toml:"muted"`
	Font       string  `toml:"font"`
	Radius     float64 `toml:"radius"`
	Padding    float64 `toml:"padding"`
	ModuleGap  float64 `toml:"module_gap"`
}

type Tags struct {
	Labels      []string `toml:"labels"`
	ActiveBG    string   `toml:"active_bg"`
	ActiveFG    string   `toml:"active_fg"`
	OccupiedFG  string   `toml:"occupied_fg"`
	EmptyFG     string   `toml:"empty_fg"`
	HideEmpty   bool     `toml:"hide_empty"`
	ItemPadding float64  `toml:"item_padding"`
	AnimMS      int      `toml:"anim_ms"` // переезд плашки активного тега, мс; 0 -- без анимации
}

type Modules struct {
	Left   []string `toml:"left"`
	Center []string `toml:"center"`
	Right  []string `toml:"right"`
}

// Popups -- выдвижные виджеты бара: календарь у часов, регулятор у громкости,
// топ процессов у cpu и ram.
type Popups struct {
	Enabled  bool    `toml:"enabled"`
	Width    int     `toml:"width"`
	Gap      int     `toml:"gap"`
	Padding  float64 `toml:"padding"`
	Radius   float64 `toml:"radius"`
	ProcRows int     `toml:"proc_rows"`
}

type Clock struct {
	Format  string `toml:"format"`
	OnClick string `toml:"on_click"`
}

// Music -- модуль проигрывателя. Название трека шире всего остального в баре,
// поэтому единственная его настройка -- предел длины.
type Music struct {
	MaxChars int `toml:"max_chars"`
}

// Power -- команды кнопки питания (модуль power). Строки уходят в sh, поэтому
// сюда можно вписать что угодно, хоть свой скрипт.
type Power struct {
	Hibernate string `toml:"hibernate"`
	Suspend   string `toml:"suspend"`
	Lock      string `toml:"lock"`
	Reboot    string `toml:"reboot"`
	Poweroff  string `toml:"poweroff"`
}

// Disk -- модуль свободного места. Показывает раздел, которому принадлежит
// путь: "/" -- корень, "$HOME" -- тот раздел, где лежит домашний каталог.
type Disk struct {
	Path string `toml:"path"`
}

// Известные модули. Порядок задаёт порядок в палитре "доступные".
var KnownModules = []string{
	"tags", "title", "cpu", "ram", "temp", "net", "disk", "music",
	"volume", "mic", "battery", "uptime", "clock", "power",
}

// Позиции бара в порядке, в котором они показываются в выпадающем списке.
var Positions = []struct {
	ID   string
	Name string
}{
	{"top", "Сверху"},
	{"bottom", "Снизу"},
	{"left", "Слева"},
	{"right", "Справа"},
}

// Vertical -- стоит ли бар у боковой кромки: от этого зависят подписи
// «высота/ширина» и смысл зон модулей.
func (b Bar) Vertical() bool {
	return b.Position == "left" || b.Position == "right"
}

func DefaultConfig() Config {
	return Config{
		Bar: Bar{
			Position: "top", Height: 28, MarginEdge: 0,
			MarginSide: 0, Monitor: 0, IntervalMS: 1000,
		},
		Style: Style{
			Background: "#1e1e20", Foreground: "#8a8a8e",
			Accent: "#c0c0c0", Muted: "#5a5a5e",
			Font:   "JetBrainsMono Nerd Font 10",
			Radius: 0, Padding: 10, ModuleGap: 14,
		},
		Tags: Tags{
			Labels: []string{}, ActiveBG: "#2a2a2d", ActiveFG: "#c0c0c0",
			OccupiedFG: "#8a8a8e", EmptyFG: "#5a5a5e",
			HideEmpty: false, ItemPadding: 9, AnimMS: 160,
		},
		Modules: Modules{
			Left:   []string{"tags"},
			Center: []string{"title"},
			Right:  []string{"music", "net", "temp", "cpu", "ram", "volume", "clock"},
		},
		Clock: Clock{Format: "%a %d %b  %H:%M", OnClick: ""},
		Power: DefaultPower(),
		Disk:  Disk{Path: "/"},
		Music: Music{MaxChars: 32},
		Popups: Popups{
			Enabled: true, Width: 260, Gap: 6,
			Padding: 12, Radius: 10, ProcRows: 5,
		},
	}
}

// DefaultPower зеркалит config.rs: по умолчанию зовём power.sh из vxwm -- он
// снимает слепок сессии и проверяет, что гибернации есть куда писать.
func DefaultPower() Power {
	const vxwm = "$HOME/dotfiles/vxwm/power.sh"
	return Power{
		Hibernate: vxwm + " hibernate",
		Suspend:   "systemctl suspend",
		Lock:      vxwm + " lock",
		Reboot:    vxwm + " reboot",
		Poweroff:  vxwm + " poweroff",
	}
}

func ConfigPath() string {
	if p := os.Getenv("VXBAR_CONFIG"); p != "" {
		return p
	}
	base := os.Getenv("XDG_CONFIG_HOME")
	if base == "" {
		base = filepath.Join(os.Getenv("HOME"), ".config")
	}
	return filepath.Join(base, "vxbar", "config.toml")
}

// Load стартует с дефолтов и накатывает поверх то, что есть в файле, — так
// частичный или слегка устаревший TOML не оставляет нулевые поля (высота 0,
// пустые цвета), из-за которых окно настроек выглядело бы сломанным.
func Load(path string) (Config, error) {
	cfg := DefaultConfig()
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return cfg, nil
		}
		return cfg, err
	}
	if err := toml.Unmarshal(data, &cfg); err != nil {
		return DefaultConfig(), fmt.Errorf("разбор %s: %w", path, err)
	}
	return cfg, nil
}

// Marshal отдаёт тот же TOML, что ушёл бы в файл. Вынесено из Save, чтобы
// вызывающий мог сравнить результат с уже записанным и не трогать файл, когда
// ничего не поменялось: каждая запись тянет за собой SIGUSR1, а на нём бар
// пересоздаёт поверхность cairo и заново поднимает шрифт.
func Marshal(cfg Config) ([]byte, error) { return toml.Marshal(cfg) }

// Save пишет через временный файл рядом с целевым и переименовывает: vxbar
// может читать конфиг ровно в этот момент (SIGUSR1 от другого процесса), и
// обрывок TOML заставил бы его откатиться на дефолты.
func Save(path string, cfg Config) error {
	data, err := Marshal(cfg)
	if err != nil {
		return err
	}
	return SaveBytes(path, data)
}

func SaveBytes(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(filepath.Dir(path), ".config.toml.*")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	defer os.Remove(tmpName) // no-op после успешного Rename
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Chmod(tmpName, 0o644); err != nil {
		return err
	}
	return os.Rename(tmpName, path)
}

// Reload просит работающий бар перечитать конфиг. Отсутствие процесса — не
// ошибка: настройки можно править и при выключенном баре.
//
// Сигнал шлём сами, а не через pkill: правка ползунка за один драг даёт
// несколько применений, и каждое стоило форка с exec ради одного kill(2).
func Reload() error {
	pids, err := barPIDs()
	if err != nil {
		return err
	}
	for _, pid := range pids {
		// ESRCH -- процесс успел уйти между чтением /proc и сигналом; это не
		// ошибка настроек, бар просто закрыли.
		if err := syscall.Kill(pid, syscall.SIGUSR1); err != nil && err != syscall.ESRCH {
			return fmt.Errorf("сигнал процессу %d: %w", pid, err)
		}
	}
	return nil
}

// barPIDs -- процессы с именем vxbar. Имя берём из /proc/<pid>/comm: оно
// обрезано до 15 символов, но "vxbar" в них помещается целиком.
func barPIDs() ([]int, error) {
	entries, err := os.ReadDir("/proc")
	if err != nil {
		return nil, err
	}
	var out []int
	for _, e := range entries {
		pid, err := strconv.Atoi(e.Name())
		if err != nil {
			continue
		}
		comm, err := os.ReadFile(filepath.Join("/proc", e.Name(), "comm"))
		if err != nil {
			continue // процесс ушёл, пока мы шли по каталогу
		}
		if strings.TrimSpace(string(comm)) == "vxbar" {
			out = append(out, pid)
		}
	}
	return out, nil
}
