package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Правка не должна съедать комментарии: rice.toml -- ещё и документация.
func TestSaveRiceKeepsComments(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("VXWM_RICE_DIR", "../vxwm/rice")

	r, err := LoadRice()
	if err != nil {
		t.Fatalf("LoadRice: %v", err)
	}
	if r.Theme.Name == "" {
		t.Fatal("тема пустая: умолчания не подхватились")
	}

	r.Windows.Gap = 33
	r.Compositor.Blur = false
	r.Power.IdleSeconds = 900
	if err := SaveRice(r); err != nil {
		t.Fatalf("SaveRice: %v", err)
	}

	out, err := os.ReadFile(RicePath())
	if err != nil {
		t.Fatal(err)
	}
	text := string(out)
	for _, want := range []string{"gap = 33", "blur = false", "idle_seconds = 900"} {
		if !strings.Contains(text, want) {
			t.Errorf("не записано: %s", want)
		}
	}
	if !strings.Contains(text, "# Настройки риса") {
		t.Error("шапка с комментарием потеряна")
	}
	if strings.Count(text, "#") < 10 {
		t.Errorf("комментариев осталось слишком мало: %d", strings.Count(text, "#"))
	}

	// Перечитали -- получили то же самое.
	again, err := LoadRice()
	if err != nil {
		t.Fatal(err)
	}
	if again.Windows.Gap != 33 || again.Compositor.Blur || again.Power.IdleSeconds != 900 {
		t.Errorf("после перечитывания значения разошлись: %+v", again.Windows)
	}
}

// Ключ, которого в файле нет, должен дописаться, а не потеряться молча.
func TestSaveRiceAddsMissingKey(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("VXWM_RICE_DIR", "../vxwm/rice")

	if err := os.MkdirAll(dir+"/vxwm-rice", 0o755); err != nil {
		t.Fatal(err)
	}
	// Конфиг без секции [windows] целиком.
	handmade := "[theme]\nname = \"deep\"\n\n[power]\nidle_seconds = 120\n"
	if err := os.WriteFile(RicePath(), []byte(handmade), 0o644); err != nil {
		t.Fatal(err)
	}

	r, _ := LoadRice()
	r.Windows.Gap = 7
	r.Power.LockOnIdle = true
	if err := SaveRice(r); err != nil {
		t.Fatalf("SaveRice: %v", err)
	}
	again, err := LoadRice()
	if err != nil {
		t.Fatal(err)
	}
	if again.Power.LockOnIdle != true {
		t.Error("lock_on_idle не дописался в существующую секцию")
	}
	if again.Power.IdleSeconds != 120 {
		t.Errorf("затёрлось существующее значение: %d", again.Power.IdleSeconds)
	}
	// Секции [windows] в исходном файле не было вовсе -- она должна
	// появиться целиком, иначе правка молча пропадает.
	if again.Windows.Gap != 7 {
		t.Errorf("отсутствующая секция потеряна: gap = %d, ждали 7", again.Windows.Gap)
	}
}

// Живой rice.toml мог остаться от руки и без половины ключей. Отсутствующее
// берётся из умолчаний, а не превращается в нули: иначе приложение показало
// бы «прозрачность бара 0» и первой же правкой записало это в файл.
func TestLoadRiceFillsMissingKeysFromDefaults(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", dir)
	t.Setenv("VXWM_RICE_DIR", "../vxwm/rice")

	if err := os.MkdirAll(dir+"/vxwm-rice", 0o755); err != nil {
		t.Fatal(err)
	}
	live := "[theme]\nname = \"paper\"\n\n[windows]\ngap = 4\n"
	if err := os.WriteFile(RicePath(), []byte(live), 0o644); err != nil {
		t.Fatal(err)
	}

	def, err := DefaultRice()
	if err != nil {
		t.Fatalf("DefaultRice: %v", err)
	}
	r, err := LoadRice()
	if err != nil {
		t.Fatalf("LoadRice: %v", err)
	}

	if r.Theme.Name != "paper" || r.Windows.Gap != 4 {
		t.Errorf("живые значения потерялись: %+v", r)
	}
	if r.Windows.Border != def.Windows.Border {
		t.Errorf("border=%d, ждали умолчание %d", r.Windows.Border, def.Windows.Border)
	}
	if r.Compositor.BarOpacity != def.Compositor.BarOpacity {
		t.Errorf("bar_opacity=%v, ждали умолчание %v",
			r.Compositor.BarOpacity, def.Compositor.BarOpacity)
	}
	if r.Power.IdleSeconds != def.Power.IdleSeconds {
		t.Errorf("idle_seconds=%d, ждали умолчание %d",
			r.Power.IdleSeconds, def.Power.IdleSeconds)
	}
}

// Обои ищутся и в репозитории, а не только в собранной теме: apply.py умеет
// брать их оттуда, и приложение не должно говорить «нет файла» о картинке,
// которую генератор находит.
func TestHasWallpaperFindsRepoFile(t *testing.T) {
	live := t.TempDir()
	repo := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", live)
	t.Setenv("VXWM_RICE_DIR", repo)

	if _, ok := HasWallpaper("carbon"); ok {
		t.Fatal("обои нашлись там, где их нет")
	}

	dir := filepath.Join(repo, "themes", "carbon")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	want := filepath.Join(dir, "wallpaper.png")
	if err := os.WriteFile(want, []byte("png"), 0o644); err != nil {
		t.Fatal(err)
	}

	got, ok := HasWallpaper("carbon")
	if !ok {
		t.Fatal("обои в репозитории не найдены")
	}
	if got != want {
		t.Errorf("путь разошёлся: %s, ждали %s", got, want)
	}
}

// Собранная тема важнее репозитория: если картинку положили в ~/.config,
// apply.py возьмёт именно её.
func TestHasWallpaperPrefersLiveTheme(t *testing.T) {
	live := t.TempDir()
	repo := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", live)
	t.Setenv("VXWM_RICE_DIR", repo)

	for _, dir := range []string{
		filepath.Join(repo, "themes", "carbon"),
		filepath.Join(live, "vxwm-rice", "themes", "carbon"),
	} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, "wallpaper.png"), []byte("png"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	got, _ := HasWallpaper("carbon")
	want := filepath.Join(live, "vxwm-rice", "themes", "carbon", "wallpaper.png")
	if got != want {
		t.Errorf("взят %s, а живая тема должна быть важнее: %s", got, want)
	}
}
