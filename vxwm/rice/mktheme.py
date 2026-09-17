#!/usr/bin/env python3
"""Сборка темы риса из палитры.

    mktheme.py                 -- собрать все палитры
    mktheme.py deep carbon     -- только названные
    mktheme.py --check         -- собрать во временный каталог и сравнить с тем,
                                  что лежит в themes/, ничего не записывая

Тема -- это палитра (vxwm/rice/palettes/<имя>.toml) плюс обои. Всё остальное --
colors.Xresources, rofi.rasi, kitty.conf -- отсюда и генерируется, поэтому
цвета физически не могут разъехаться между конфигами: их негде разъехать.
picom.conf у всех тем общий и темой не управляется.

Обои генератор не трогает: они лежат в themes/<имя>/wallpaper.png и в git не
попадают из-за размера.
"""

import shutil
import sys
import tomllib
from pathlib import Path

RICE = Path(__file__).resolve().parent
PALETTES = RICE / "palettes"
TEMPLATES = RICE / "templates"
THEMES = Path.home() / ".config" / "vxwm-rice" / "themes"

# Что во что разворачивается. Имя шаблона -> имя файла в теме.
OUTPUTS = {
    "colors.Xresources.tmpl": "colors.Xresources",
    "rofi.rasi.tmpl": "rofi.rasi",
    "kitty.conf.tmpl": "kitty.conf",
    "dunstrc.tmpl": "dunstrc",
    "gtk.css.tmpl": "gtk.css",
    "qtct-colors.conf.tmpl": "qtct-colors.conf",
    "sddm-theme.conf.tmpl": "sddm-theme.conf",
}


def rgb(hex_color: str) -> str:
    """#1e1e20 -> "30, 30, 32". Нужно rofi: там прозрачность задаётся только
    через rgba(), а hex с альфой он не понимает."""
    h = hex_color.lstrip("#")
    return ", ".join(str(int(h[i:i + 2], 16)) for i in (0, 2, 4))


# Прозрачность темы не касается: это настройка из [compositor] в rice.toml.
# Здесь лежат умолчания на случай, когда mktheme зовут руками, без apply.py.
DEFAULT_OPACITY = {
    "TERMINAL_OPACITY": 0.75,
    "MENU_OPACITY_PCT": 85,
    "MENU_ALT_OPACITY_PCT": 77,
}


def substitutions(palette: dict) -> dict:
    ui = palette["ui"]
    ansi = palette["ansi"]

    subs = {
        "NAME": palette["name"],
        "NAME_UPPER": palette["name"].upper(),
        "DESCRIPTION": palette.get("description", ""),
    }
    for key, value in ui.items():
        subs[key.upper()] = value
    # Отдельные роли, которых может не быть в палитре: без dim_bg выпадающие
    # окна просто совпадут по фону с обычными.
    subs.setdefault("MUTED", ui["norm_border"])
    subs.setdefault("DIM_BG", ui["norm_bg"])
    subs["DIM_BG_RGB"] = rgb(subs["DIM_BG"])
    subs["SEL_BG_RGB"] = rgb(ui["sel_bg"])
    # Qt пишет цвета как #aarrggbb и без альфы файл схемы не читает. Добавляем
    # непрозрачный вариант каждой роли отдельным именем.
    for key in list(subs):
        value = str(subs[key])
        if value.startswith("#") and len(value) == 7:
            subs[f"ARGB_{key}"] = "#ff" + value[1:]
    for i in range(16):
        subs[f"COLOR{i}"] = ansi[f"color{i}"]
    subs.update(DEFAULT_OPACITY)
    return subs


def render(template: str, subs: dict) -> str:
    out = template
    for key, value in subs.items():
        out = out.replace("{{" + key + "}}", str(value))
    # Незакрытый плейсхолдер означает опечатку в шаблоне: молча оставить его в
    # конфиге хуже, чем упасть -- kitty и rofi проглотят мусор и покрасят
    # что-нибудь чёрным.
    if "{{" in out:
        leftover = out[out.index("{{"):out.index("{{") + 40]
        raise SystemExit(f"неподставленный плейсхолдер: {leftover!r}")
    return out


def build(name: str, dest_root: Path, extra: dict | None = None) -> list[str]:
    """extra -- подстановки поверх палитры; через них apply.py передаёт
    прозрачность из rice.toml, которая от темы не зависит."""
    path = PALETTES / f"{name}.toml"
    if not path.exists():
        raise SystemExit(f"нет палитры {path}")
    with path.open("rb") as fh:
        palette = tomllib.load(fh)

    missing = {"norm_fg", "norm_bg", "norm_border", "sel_fg", "sel_bg",
               "sel_border"} - set(palette.get("ui", {}))
    if missing:
        raise SystemExit(f"{name}: в [ui] не хватает {', '.join(sorted(missing))}")
    if len(palette.get("ansi", {})) != 16:
        raise SystemExit(f"{name}: в [ansi] должно быть ровно 16 цветов")

    subs = substitutions(palette)
    subs.update(extra or {})
    dest = dest_root / name
    dest.mkdir(parents=True, exist_ok=True)

    written = []
    for tmpl, out_name in OUTPUTS.items():
        text = render((TEMPLATES / tmpl).read_text(), subs)
        (dest / out_name).write_text(text)
        written.append(out_name)
    return written


def main() -> None:
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    check = "--check" in sys.argv[1:]
    names = args or sorted(p.stem for p in PALETTES.glob("*.toml"))

    if check:
        tmp = Path("/tmp") / f"mktheme-check-{os.getpid()}"
        shutil.rmtree(tmp, ignore_errors=True)
        differs = False
        for name in names:
            for out_name in build(name, tmp):
                new = (tmp / name / out_name).read_text()
                old_path = THEMES / name / out_name
                old = old_path.read_text() if old_path.exists() else None
                if old is None:
                    print(f"{name}/{out_name}: нет в themes/")
                    differs = True
                elif old != new:
                    print(f"{name}/{out_name}: РАСХОЖДЕНИЕ")
                    differs = True
                else:
                    print(f"{name}/{out_name}: совпадает")
        shutil.rmtree(tmp, ignore_errors=True)
        sys.exit(1 if differs else 0)

    for name in names:
        written = build(name, THEMES)
        print(f"{name}: {', '.join(written)}")


if __name__ == "__main__":
    import os
    main()
