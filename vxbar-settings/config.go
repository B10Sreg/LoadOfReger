package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

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

// Известные модули. Порядок задаёт порядок в палитре "доступные".
var KnownModules = []string{
	"tags", "title", "cpu", "ram", "temp", "net", "music", "volume", "clock",
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
			HideEmpty: false, ItemPadding: 9,
		},
		Modules: Modules{
			Left:   []string{"tags"},
			Center: []string{"title"},
			Right:  []string{"music", "net", "temp", "cpu", "ram", "volume", "clock"},
		},
		Clock: Clock{Format: "%a %d %b  %H:%M", OnClick: ""},
		Music: Music{MaxChars: 32},
		Popups: Popups{
			Enabled: true, Width: 260, Gap: 6,
			Padding: 12, Radius: 10, ProcRows: 5,
		},
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

// Save пишет через временный файл рядом с целевым и переименовывает: vxbar
// может читать конфиг ровно в этот момент (SIGUSR1 от другого процесса), и
// обрывок TOML заставил бы его откатиться на дефолты.
func Save(path string, cfg Config) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	data, err := toml.Marshal(cfg)
	if err != nil {
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
func Reload() error {
	out, err := exec.Command("pkill", "-USR1", "-x", "vxbar").CombinedOutput()
	if err != nil {
		// pkill возвращает 1, когда не нашёл ни одного процесса
		if ee, ok := err.(*exec.ExitError); ok && ee.ExitCode() == 1 {
			return nil
		}
		return fmt.Errorf("pkill: %v: %s", err, out)
	}
	return nil
}
