package main

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"github.com/pelletier/go-toml/v2"
)

// Rice -- настройки риса целиком: ~/.config/vxwm-rice/rice.toml. Всё остальное
// (Xresources, picom.conf, rofi, kitty, dunst, цвета бара) из него
// разворачивает apply.py, поэтому приложению достаточно править этот файл и
// звать генератор.
//
// Настройки самого бара живут отдельно, в ~/.config/vxbar/config.toml: его
// читает vxbar сам, и дублировать их сюда значило бы завести второй источник
// правды ради единообразия.
type Rice struct {
	Theme struct {
		Name string `toml:"name"`
	} `toml:"theme"`

	Windows struct {
		Gap    int `toml:"gap"`
		Border int `toml:"border"`
	} `toml:"windows"`

	Compositor struct {
		Enabled       bool    `toml:"enabled"`
		Vsync         bool    `toml:"vsync"`
		Animations    bool    `toml:"animations"`
		Shadow        bool    `toml:"shadow"`
		ShadowRadius  int     `toml:"shadow_radius"`
		ShadowOpacity float64 `toml:"shadow_opacity"`
		Blur          bool    `toml:"blur"`
		BlurStrength  int     `toml:"blur_strength"`
		Fading        bool    `toml:"fading"`
		CornerRadius  int     `toml:"corner_radius"`
		BarOpacity    float64 `toml:"bar_opacity"`
	} `toml:"compositor"`

	Session struct {
		Restore          bool `toml:"restore"`
		SnapshotInterval int  `toml:"snapshot_interval"`
	} `toml:"session"`

	Power struct {
		LockOnIdle  bool `toml:"lock_on_idle"`
		IdleSeconds int  `toml:"idle_seconds"`
	} `toml:"power"`
}

// Palette -- палитра темы. Нужна только ради образцов цвета на странице
// «Тема»: применяет её всё равно apply.py.
type Palette struct {
	Name        string            `toml:"name"`
	Description string            `toml:"description"`
	UI          map[string]string `toml:"ui"`
}

func riceHome() string {
	base := os.Getenv("XDG_CONFIG_HOME")
	if base == "" {
		home, _ := os.UserHomeDir()
		base = filepath.Join(home, ".config")
	}
	return filepath.Join(base, "vxwm-rice")
}

func RicePath() string { return filepath.Join(riceHome(), "rice.toml") }

// RiceDir ищет каталог с apply.py и палитрами. Путь не зашит: репозиторий
// могут держать где угодно, а вот autostart.sh симлинком лежит в
// ~/.config/vxwm -- по нему и находим остальное.
func RiceDir() string {
	if dir := os.Getenv("VXWM_RICE_DIR"); dir != "" {
		return dir
	}
	base := os.Getenv("XDG_CONFIG_HOME")
	if base == "" {
		home, _ := os.UserHomeDir()
		base = filepath.Join(home, ".config")
	}
	if target, err := filepath.EvalSymlinks(filepath.Join(base, "vxwm", "autostart.sh")); err == nil {
		return filepath.Join(filepath.Dir(target), "rice")
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, "dotfiles", "vxwm", "rice")
}

// LoadRice читает живой конфиг. Если его ещё нет, берём умолчания из
// репозитория -- там же, где лежит apply.py.
func LoadRice() (Rice, error) {
	var r Rice
	data, err := os.ReadFile(RicePath())
	if err != nil {
		if !os.IsNotExist(err) {
			return r, err
		}
		data, err = os.ReadFile(filepath.Join(RiceDir(), "rice.toml"))
		if err != nil {
			return r, fmt.Errorf("нет ни живого rice.toml, ни умолчаний: %w", err)
		}
	}
	return r, toml.Unmarshal(data, &r)
}

// Palettes -- список доступных тем, по именам файлов палитр. Каталог тем для
// этого не годится: там может остаться папка от удалённой палитры, а собрать
// такую тему нечем.
func Palettes() []string {
	entries, err := os.ReadDir(filepath.Join(RiceDir(), "palettes"))
	if err != nil {
		return nil
	}
	var out []string
	for _, e := range entries {
		if name, ok := strings.CutSuffix(e.Name(), ".toml"); ok {
			out = append(out, name)
		}
	}
	return out
}

func LoadPalette(name string) (Palette, error) {
	var p Palette
	data, err := os.ReadFile(filepath.Join(RiceDir(), "palettes", name+".toml"))
	if err != nil {
		return p, err
	}
	return p, toml.Unmarshal(data, &p)
}

// HasWallpaper говорит, есть ли у темы обои. Без них apply.py оставит
// прежние, и на странице темы это стоит показать заранее.
func HasWallpaper(name string) (string, bool) {
	for _, ext := range []string{".png", ".jpg", ".jpeg"} {
		path := filepath.Join(riceHome(), "themes", name, "wallpaper"+ext)
		if _, err := os.Stat(path); err == nil {
			return path, true
		}
	}
	return "", false
}

var sectionRe = regexp.MustCompile(`^\s*\[(\w+)\]`)

// SaveRice переписывает только значения, оставляя комментарии на месте.
// Маршалить структуру целиком было бы короче, но rice.toml -- это ещё и
// документация настроек: половина файла там комментарии, и терять их на
// каждом щелчке переключателя нельзя.
func SaveRice(r Rice) error {
	want := map[string]map[string]string{
		"theme":   {"name": quote(r.Theme.Name)},
		"windows": {"gap": itoa(r.Windows.Gap), "border": itoa(r.Windows.Border)},
		"compositor": {
			"enabled":        btoa(r.Compositor.Enabled),
			"vsync":          btoa(r.Compositor.Vsync),
			"animations":     btoa(r.Compositor.Animations),
			"shadow":         btoa(r.Compositor.Shadow),
			"shadow_radius":  itoa(r.Compositor.ShadowRadius),
			"shadow_opacity": ftoa(r.Compositor.ShadowOpacity),
			"blur":           btoa(r.Compositor.Blur),
			"blur_strength":  itoa(r.Compositor.BlurStrength),
			"fading":         btoa(r.Compositor.Fading),
			"corner_radius":  itoa(r.Compositor.CornerRadius),
			"bar_opacity":    ftoa(r.Compositor.BarOpacity),
		},
		"session": {
			"restore":           btoa(r.Session.Restore),
			"snapshot_interval": itoa(r.Session.SnapshotInterval),
		},
		"power": {
			"lock_on_idle": btoa(r.Power.LockOnIdle),
			"idle_seconds": itoa(r.Power.IdleSeconds),
		},
	}

	src, err := os.ReadFile(RicePath())
	if err != nil {
		// Живого файла ещё нет -- начинаем с умолчаний, чтобы не потерять
		// комментарии, ради которых всё это и затевалось.
		src, err = os.ReadFile(filepath.Join(RiceDir(), "rice.toml"))
		if err != nil {
			return err
		}
	}

	var out []string
	section := ""
	seen := map[string]map[string]bool{}
	scanner := bufio.NewScanner(strings.NewReader(string(src)))
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for scanner.Scan() {
		line := scanner.Text()
		if m := sectionRe.FindStringSubmatch(line); m != nil {
			section = m[1]
			seen[section] = map[string]bool{}
		} else if keys, ok := want[section]; ok {
			if key, _, found := strings.Cut(line, "="); found {
				k := strings.TrimSpace(key)
				if v, ok := keys[k]; ok {
					line = fmt.Sprintf("%s = %s", k, v)
					seen[section][k] = true
				}
			}
		}
		out = append(out, line)
	}
	if err := scanner.Err(); err != nil {
		return err
	}

	// Ключа -- а то и целой секции -- могло не быть в файле: конфиг писали
	// руками и часть настроек опустили. Дописываем недостающее, иначе правка
	// из приложения молча никуда не попала бы.
	out = appendMissing(out, want, seen)
	out = appendMissingSections(out, want, seen)

	if err := os.MkdirAll(filepath.Dir(RicePath()), 0o755); err != nil {
		return err
	}
	tmp := RicePath() + ".tmp"
	if err := os.WriteFile(tmp, []byte(strings.Join(out, "\n")+"\n"), 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, RicePath())
}

func appendMissing(lines []string, want map[string]map[string]string,
	seen map[string]map[string]bool) []string {

	// Идём с конца каждой секции, поэтому собираем вставки и применяем их
	// за один проход: вставлять по одной, пересчитывая индексы, легко
	// напутать.
	insert := map[int][]string{}
	section := ""
	closeSection := func(end int) {
		if section == "" {
			return
		}
		for k, v := range want[section] {
			if !seen[section][k] {
				insert[end] = append(insert[end], fmt.Sprintf("%s = %s", k, v))
			}
		}
	}
	for i, line := range lines {
		if m := sectionRe.FindStringSubmatch(line); m != nil {
			closeSection(i)
			section = m[1]
		}
	}
	closeSection(len(lines))

	if len(insert) == 0 {
		return lines
	}
	var out []string
	for i, line := range lines {
		if add, ok := insert[i]; ok {
			out = append(out, add...)
		}
		out = append(out, line)
	}
	if add, ok := insert[len(lines)]; ok {
		out = append(out, add...)
	}
	return out
}

// appendMissingSections дописывает в конец файла секции, которых в нём не было
// вовсе. Порядок фиксирован: перебор map в Go случаен, и без сортировки файл
// каждый раз тасовался бы по-новому, засоряя diff.
func appendMissingSections(lines []string, want map[string]map[string]string,
	seen map[string]map[string]bool) []string {

	sections := make([]string, 0, len(want))
	for name := range want {
		if _, ok := seen[name]; !ok {
			sections = append(sections, name)
		}
	}
	if len(sections) == 0 {
		return lines
	}
	sort.Strings(sections)

	for _, name := range sections {
		keys := make([]string, 0, len(want[name]))
		for k := range want[name] {
			keys = append(keys, k)
		}
		sort.Strings(keys)

		lines = append(lines, "", "["+name+"]")
		for _, k := range keys {
			lines = append(lines, fmt.Sprintf("%s = %s", k, want[name][k]))
		}
	}
	return lines
}

// ApplyRice зовёт генератор. Он сам решает, что перезапускать: vxwm и vxbar
// подхватывают изменения без рестарта, picom и dunst -- только с ним.
func ApplyRice(themeOnly bool) error {
	args := []string{}
	if themeOnly {
		args = append(args, "--theme")
	}
	cmd := exec.Command(filepath.Join(RiceDir(), "apply.py"), args...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		// Последняя строка вывода информативнее кода возврата: там причина.
		lines := strings.Split(strings.TrimSpace(string(out)), "\n")
		return fmt.Errorf("%s", lines[len(lines)-1])
	}
	return nil
}

func quote(s string) string { return strconv.Quote(s) }
func itoa(v int) string     { return strconv.Itoa(v) }
func btoa(v bool) string    { return strconv.FormatBool(v) }
func ftoa(v float64) string { return strconv.FormatFloat(v, 'g', -1, 64) }
