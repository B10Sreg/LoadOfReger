package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	"github.com/pelletier/go-toml/v2"
)

// Go-структуры в config.go -- ручная копия vxbar/src/config.rs. Если в Rust
// появится поле, которого нет здесь, "Применить" молча сотрёт его из файла.
// Тест ловит расхождение: просим сам vxbar выписать дефолтный конфиг и
// сравниваем набор ключей с тем, что пишем мы.
func TestDefaultsMatchRustSideKeys(t *testing.T) {
	bar, err := exec.LookPath("vxbar")
	if err != nil {
		t.Skip("vxbar не найден в PATH, сравнивать не с чем")
	}

	dir := t.TempDir()
	rustPath := filepath.Join(dir, "rust.toml")

	// vxbar создаёт конфиг на старте и дальше крутит цикл -- ждать нечего,
	// файл появляется сразу, поэтому просто прибиваем процесс по таймауту.
	cmd := exec.Command(bar)
	cmd.Env = append(os.Environ(), "VXBAR_CONFIG="+rustPath)
	if err := cmd.Start(); err != nil {
		t.Fatalf("не запускается vxbar: %v", err)
	}
	defer func() {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
	}()

	if err := waitForFile(rustPath); err != nil {
		t.Skipf("vxbar не выписал конфиг (нет DISPLAY?): %v", err)
	}

	goPath := filepath.Join(dir, "go.toml")
	if err := Save(goPath, DefaultConfig()); err != nil {
		t.Fatalf("Save: %v", err)
	}

	rustKeys := keysOf(t, rustPath)
	goKeys := keysOf(t, goPath)

	if !reflect.DeepEqual(rustKeys, goKeys) {
		t.Errorf("набор ключей разошёлся\n vxbar: %v\n  наш:  %v", rustKeys, goKeys)
	}
}

// Значения по умолчанию тоже должны совпадать: иначе первое же открытие
// настроек молча переедет конфиг пользователя на другие цифры.
func TestDefaultValuesMatchRustSide(t *testing.T) {
	cfg := DefaultConfig()
	data, err := toml.Marshal(cfg)
	if err != nil {
		t.Fatal(err)
	}
	var back Config
	if err := toml.Unmarshal(data, &back); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(cfg, back) {
		t.Errorf("конфиг не переживает круг marshal/unmarshal:\n%+v\n%+v", cfg, back)
	}
}

func TestSaveIsAtomicAndReadable(t *testing.T) {
	path := filepath.Join(t.TempDir(), "nested", "config.toml")
	want := DefaultConfig()
	want.Bar.Position = "bottom"
	want.Bar.Height = 40
	want.Tags.Labels = []string{"一", "二"}
	if err := Save(path, want); err != nil {
		t.Fatalf("Save: %v", err)
	}
	got, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if !reflect.DeepEqual(want, got) {
		t.Errorf("круг записи/чтения потерял данные:\n want %+v\n got  %+v", want, got)
	}
	// Временный файл не должен остаться рядом с конфигом.
	entries, _ := os.ReadDir(filepath.Dir(path))
	if len(entries) != 1 {
		t.Errorf("рядом с конфигом остался мусор: %v", entries)
	}
}

// Частичный конфиг -- ровно то, что напишет человек руками. Отсутствующие
// поля обязаны подставиться из дефолтов, а не занулиться.
func TestPartialConfigKeepsDefaults(t *testing.T) {
	path := filepath.Join(t.TempDir(), "config.toml")
	if err := os.WriteFile(path, []byte("[bar]\nheight = 33\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	got, err := Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if got.Bar.Height != 33 {
		t.Errorf("высота не прочиталась: %d", got.Bar.Height)
	}
	if got.Style.Background != DefaultConfig().Style.Background {
		t.Errorf("цвет фона занулился вместо дефолта: %q", got.Style.Background)
	}
	if got.Bar.IntervalMS != DefaultConfig().Bar.IntervalMS {
		t.Errorf("интервал занулился: %d", got.Bar.IntervalMS)
	}
}

func keysOf(t *testing.T, path string) []string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if err := toml.Unmarshal(data, &m); err != nil {
		t.Fatalf("разбор %s: %v", path, err)
	}
	return flatten(m, "")
}

func flatten(m map[string]any, prefix string) []string {
	var out []string
	for k, v := range m {
		full := k
		if prefix != "" {
			full = prefix + "." + k
		}
		if sub, ok := v.(map[string]any); ok {
			out = append(out, flatten(sub, full)...)
			continue
		}
		out = append(out, full)
	}
	sortStrings(out)
	return out
}

func sortStrings(v []string) {
	for i := 1; i < len(v); i++ {
		for j := i; j > 0 && v[j] < v[j-1]; j-- {
			v[j], v[j-1] = v[j-1], v[j]
		}
	}
}

func waitForFile(path string) error {
	for i := 0; i < 100; i++ {
		if _, err := os.Stat(path); err == nil {
			return nil
		}
		time.Sleep(20 * time.Millisecond)
	}
	_, err := os.Stat(path)
	return err
}
