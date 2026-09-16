package main

import (
	"os"
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
