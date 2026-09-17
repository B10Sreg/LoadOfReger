#!/usr/bin/env python3
"""Развернуть rice.toml во все конфиги риса и применить их к живой сессии.

    apply.py            -- применить всё
    apply.py --theme    -- только тему (не трогает композитор)
    apply.py --dry-run  -- показать, что было бы сделано

Единственный источник правды -- ~/.config/vxwm-rice/rice.toml. Отсюда
генерируются Xresources, picom.conf, rofi.rasi, kitty.conf, dunstrc и цвета
бара, и отсюда же берутся настройки сессии для скриптов.

Применение к живой сессии сделано так, чтобы ничего не перезапускать без
нужды: vxwm ловит смену RESOURCE_MANAGER сам, vxbar перечитывает конфиг по
SIGUSR1, kitty красится через сокет. Перезапускать приходится только picom и
dunst -- они конфиг на лету не перечитывают.

Вне сессии (нет DISPLAY) генерация всё равно проходит: файлы обновятся, а
шаги применения молча пропустятся. Так apply.py можно звать из установки.
"""

import argparse
import os
import shlex
import shutil
import signal
import subprocess
import sys
import time
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mktheme  # noqa: E402  -- лежит рядом, путь добавлен строкой выше

RICE = Path(__file__).resolve().parent
CONFIG_HOME = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
RICE_HOME = CONFIG_HOME / "vxwm-rice"
RICE_TOML = RICE_HOME / "rice.toml"
THEMES = RICE_HOME / "themes"

# Куда уезжают сгенерированные конфиги.
DEST = {
    "rofi.rasi": CONFIG_HOME / "rofi" / "config.rasi",
    "kitty.conf": CONFIG_HOME / "kitty" / "kitty.conf",
    "dunstrc": CONFIG_HOME / "dunst" / "dunstrc",
}

# Роли бара выводятся из палитры окон: фон бара -- фон неактивного окна,
# акцент -- цвет рамки активного. Так бар и рамки читаются как одна система.
BAR_COLORS = {
    "style": {
        "background": "norm_bg",
        "foreground": "norm_fg",
        "accent": "sel_border",
        "muted": "muted",
    },
    "tags": {
        "active_bg": "sel_bg",
        "active_fg": "sel_fg",
        "occupied_fg": "norm_fg",
        "empty_fg": "norm_border",
    },
}


def log(msg):
    print(f"  {msg}")


class Runner:
    """Обёртка над действиями, чтобы --dry-run был честным: один и тот же код
    и выполняет, и показывает, вместо двух расходящихся веток."""

    def __init__(self, dry):
        self.dry = dry

    def write(self, path: Path, text: str):
        if self.dry:
            log(f"записал бы {path}")
            return
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp = path.with_suffix(path.suffix + ".tmp")
        tmp.write_text(text)
        # Заменяем через переименование: конфиг могут читать прямо сейчас, и
        # наткнуться на половину файла хуже, чем на старую версию целиком.
        # unlink нужен, потому что путь мог оказаться симлинком в dotfiles --
        # так сгенерированное когда-то попадало прямо в репозиторий.
        if path.is_symlink():
            path.unlink()
        tmp.replace(path)
        log(f"записал {path}")

    def run(self, cmd, **kw):
        if self.dry:
            log("запустил бы: " + " ".join(cmd))
            return None
        try:
            return subprocess.run(cmd, check=False, capture_output=True, **kw)
        except FileNotFoundError:
            log(f"нет команды {cmd[0]}, пропускаю")
            return None

    def signal_to(self, name, sig):
        if self.dry:
            log(f"послал бы {sig.name} процессу {name}")
            return
        self.run(["pkill", f"-{sig.name[3:]}", "-x", name])
        log(f"{name}: {sig.name}")


def load_rice() -> dict:
    """Живой конфиг, с дефолтами из репозитория под ним. Отсутствующую секцию
    или ключ добираем из умолчаний: конфиг, написанный человеком, не обязан
    перечислять всё, а падать из-за этого незачем."""
    with (RICE / "rice.toml").open("rb") as fh:
        cfg = tomllib.load(fh)
    if RICE_TOML.exists():
        with RICE_TOML.open("rb") as fh:
            live = tomllib.load(fh)
        for section, values in live.items():
            cfg.setdefault(section, {}).update(values)
    return cfg


def opacity_subs(cfg) -> dict:
    """Прозрачность из [compositor] в виде подстановок для шаблонов темы:
    kitty и rofi красят свой фон сами, а не через picom, поэтому значение надо
    донести до них. picom проценты хочет целыми, rofi -- тоже."""
    c = cfg.get("compositor", {})
    menu = c.get("menu_opacity", 0.85)
    return {
        "TERMINAL_OPACITY": c.get("terminal_opacity", 0.75),
        "MENU_OPACITY_PCT": pct(menu),
        # Выделенная строка в rofi чуть прозрачнее фона: так она читается как
        # подсветка, а не как вторая плашка поверх первой.
        "MENU_ALT_OPACITY_PCT": max(0, pct(menu) - 8),
    }


def pct(value) -> int:
    return int(round(float(value) * 100))


# Сторож композитора: поднимает picom, когда тот падает сам по себе.
# Имя отдельной константой -- по нему же его и находит pkill.
KEEPER_NAME = "picom-keeper.sh"
KEEPER = RICE.parent.parent / "picom" / KEEPER_NAME

SDDM_THEME = Path("/usr/share/sddm/themes/graphite")


def apply_sddm(name: str, r: Runner):
    """Экран входа. Греетер работает под root до всякой сессии и про рис ничего
    не знает: цвета и обои он берёт из файлов рядом с собой, в /usr/share. Туда
    нам писать нечем, поэтому под обычным пользователем мы только сверяем и
    говорим, что тема входа отстала."""
    src = THEMES / name / "sddm-theme.conf"
    if not src.exists() or not SDDM_THEME.is_dir():
        return
    dest = SDDM_THEME / "theme.conf"
    if os.access(SDDM_THEME, os.W_OK):
        r.write(dest, src.read_text())
        log("экран входа перекрашен")
        return
    same = dest.exists() and dest.read_text() == src.read_text()
    if not same:
        script = RICE.parent.parent / "sddm" / "install.sh"
        log(f"экран входа остался прежним, нужен root: sudo bash {script}")


def have_display() -> bool:
    return bool(os.environ.get("DISPLAY"))


# --------------------------------------------------------------------- тема

def apply_theme(cfg, r: Runner):
    name = cfg["theme"]["name"]
    palette_path = RICE / "palettes" / f"{name}.toml"
    if not palette_path.exists():
        raise SystemExit(f"нет палитры {palette_path}")

    print(f"тема: {name}")
    if not r.dry:
        mktheme.build(name, THEMES, opacity_subs(cfg))
        log(f"собрана {THEMES / name}")

    theme_dir = THEMES / name
    for src_name, dest in DEST.items():
        src = theme_dir / src_name
        if src.exists():
            r.write(dest, src.read_text())

    with palette_path.open("rb") as fh:
        palette = tomllib.load(fh)
    apply_bar_colors(palette, r)
    apply_sddm(name, r)

    if not have_display():
        log("нет DISPLAY, живую сессию не трогаю")
        return

    xres = theme_dir / "colors.Xresources"
    if xres.exists():
        # merge, а не load: в Xresources лежат настройки и других программ.
        r.run(["xrdb", "-merge", str(xres)])
        log("цвета в X resources")

    # Открытые терминалы: kitty читает конфиг только при старте. К имени из
    # listen_on он дописывает свой pid, поэтому сокет у каждого окна свой.
    runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    socks = sorted(runtime.glob("kitty-vxwm-*"))
    for sock in socks:
        r.run(["kitty", "@", "--to", f"unix:{sock}", "set-colors", "--all",
               "--configured", str(DEST["kitty.conf"])])
    if socks:
        log(f"kitty: перекрашено окон -- {len(socks)}")

    apply_gtk(theme_dir, palette, r)
    apply_qt(theme_dir, r)
    r.signal_to("vxbar", signal.SIGUSR1)
    restart_dunst(r)
    apply_wallpaper(theme_dir, r)


def apply_bar_colors(palette, r: Runner):
    """Цвета бара живут в его собственном config.toml -- vxbar читает его сам.
    Правим только значения, сохраняя форматирование: остальную часть файла
    пишет сам бар, и ломать её незачем."""
    import re

    path = CONFIG_HOME / "vxbar" / "config.toml"
    if not path.exists():
        log("конфига vxbar нет, бар создаст его сам")
        return
    ui = palette["ui"]
    want = {section: {k: ui[role] for k, role in mapping.items() if role in ui}
            for section, mapping in BAR_COLORS.items()}

    out, section = [], None
    for line in path.read_text().splitlines():
        m = re.match(r"\s*\[(\w+)\]", line)
        if m:
            section = m.group(1)
        elif section in want:
            km = re.match(r"(\s*)(\w+)(\s*=\s*)(['\"])([^'\"]*)(['\"])(.*)", line)
            if km and km.group(2) in want[section]:
                line = (km.group(1) + km.group(2) + km.group(3) + km.group(4)
                        + want[section][km.group(2)] + km.group(6) + km.group(7))
        out.append(line)
    r.write(path, "\n".join(out) + "\n")


# Наш блок в чужом gtk.css отмечен маркерами: при следующем применении он
# заменяется целиком, а всё, что пользователь написал вокруг, остаётся.
GTK_BEGIN = "/* >>> vxwm-rice >>> */"
GTK_END = "/* <<< vxwm-rice <<< */"


def is_dark(hex_color: str) -> bool:
    """Тёмная ли палитра. Считаем по яркости фона обычного окна: светлая
    paper и тёмная carbon должны по-разному отвечать приложениям, которые
    спрашивают систему про тёмный режим."""
    h = hex_color.lstrip("#")
    r_, g_, b_ = (int(h[i:i + 2], 16) for i in (0, 2, 4))
    # Коэффициенты Rec. 601: глаз видит зелёный ярче синего, и без них
    # синеватый фон abyss считался бы светлее, чем он есть.
    return (0.299 * r_ + 0.587 * g_ + 0.114 * b_) < 128


def graft(existing: str, block: str) -> str:
    """Вставляет блок в конец файла, заменяя прежний между маркерами."""
    body = block.strip("\n")
    marked = f"{GTK_BEGIN}\n{body}\n{GTK_END}\n"
    start = existing.find(GTK_BEGIN)
    end = existing.find(GTK_END)
    if start != -1 and end > start:
        return existing[:start] + marked + existing[end + len(GTK_END):].lstrip("\n")
    if not existing.strip():
        return marked
    return existing.rstrip("\n") + "\n\n" + marked


def set_ini_key(text: str, key: str, value: str, section: str = "Settings") -> str:
    """Меняет одну строку key=value в ini-файле, не трогая остальные. Если
    ключа нет, он дописывается в начало названной секции, а не в конец файла:
    ниже могут быть другие секции, и ключ попал бы в чужую."""
    lines = text.splitlines()
    head = f"[{section}]"
    for i, line in enumerate(lines):
        if line.split("=", 1)[0].strip() == key:
            lines[i] = f"{key}={value}"
            break
    else:
        at = lines.index(head) + 1 if head in lines else len(lines)
        if head not in lines:
            lines.append(head)
            at = len(lines)
        lines.insert(at, f"{key}={value}")
    return "\n".join(lines) + "\n"


def apply_gtk(theme_dir: Path, palette: dict, r: Runner):
    """Цвета GTK-приложений. Установленную тему (gtk-theme-name) не трогаем --
    поверх неё кладётся палитра риса, и этого хватает, чтобы Thunar и прочие
    следовали за сменой темы. GTK сам следит за gtk.css и перекрашивает
    открытые окна, перезапуск не нужен."""
    css = theme_dir / "gtk.css"
    if not css.exists():
        log("gtk.css темой не собран, пропускаю")
        return
    block = css.read_text()

    for version in ("gtk-3.0", "gtk-4.0"):
        path = CONFIG_HOME / version / "gtk.css"
        existing = path.read_text() if path.exists() else ""
        r.write(path, graft(existing, block))

        ini = CONFIG_HOME / version / "settings.ini"
        if ini.exists():
            dark = "1" if is_dark(palette["ui"]["norm_bg"]) else "0"
            r.write(ini, set_ini_key(ini.read_text(),
                                     "gtk-application-prefer-dark-theme", dark))
    log("цвета GTK: gtk-3.0 и gtk-4.0")

    # Портал и GTK4 спрашивают тёмный режим у gsettings, а не у settings.ini.
    if have_display() and shutil.which("gsettings"):
        scheme = "prefer-dark" if is_dark(palette["ui"]["norm_bg"]) else "prefer-light"
        r.run(["gsettings", "set", "org.gnome.desktop.interface",
               "color-scheme", scheme])


def apply_qt(theme_dir: Path, r: Runner):
    """Цвета Qt-приложений. qt5ct/qt6ct держат палитру отдельным файлом схемы,
    поэтому кладём её рядом со штатными и переключаем конфиг на неё. Стиль
    (style=Breeze и прочее) не трогаем -- меняются только цвета.

    Живую сессию это не перекрашивает: Qt читает схему при старте приложения.
    Открытые Qt-окна подхватят тему после перезапуска."""
    src = theme_dir / "qtct-colors.conf"
    if not src.exists():
        return
    touched = []
    for tool in ("qt5ct", "qt6ct"):
        base = CONFIG_HOME / tool
        if not base.is_dir():
            continue
        scheme = base / "colors" / "vxwm-rice.conf"
        r.write(scheme, src.read_text())
        conf = base / f"{tool}.conf"
        if conf.exists():
            text = set_ini_key(conf.read_text(), "color_scheme_path",
                               str(scheme), "Appearance")
            text = set_ini_key(text, "custom_palette", "true", "Appearance")
            r.write(conf, text)
        touched.append(tool)
    if touched:
        log("цвета Qt: " + " и ".join(touched) + " (применятся при перезапуске окон)")


def find_wallpaper(theme_dir: Path):
    """Обои ищем сперва в живой теме, потом в репозитории: собранные темы
    лежат в ~/.config, а картинки версионируются рядом с палитрами."""
    for base in (theme_dir, RICE / "themes" / theme_dir.name):
        for ext in (".png", ".jpg", ".jpeg"):
            if (base / f"wallpaper{ext}").exists():
                return base / f"wallpaper{ext}"
    return None


def apply_wallpaper(theme_dir: Path, r: Runner):
    wall = find_wallpaper(theme_dir)
    if wall is None:
        log("обоев у темы нет, оставляю прежние")
        return
    r.run(["feh", "--bg-fill", str(wall)])
    log(f"обои: {wall.name}")


def restart_dunst(r: Runner):
    """dunst конфиг на лету не перечитывает -- только рестартом. Поднимаем
    отвязанно, иначе демон умрёт вместе с этим процессом."""
    if r.dry:
        log("перезапустил бы dunst")
        return
    subprocess.run(["pkill", "-x", "dunst"], capture_output=True)
    if shutil.which("dunst"):
        subprocess.Popen(["dunst"], start_new_session=True,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        log("dunst перезапущен")


# -------------------------------------------------------------------- окна

def apply_windows(cfg, r: Runner):
    w = cfg["windows"]
    print("окна")
    # Числа едут тем же путём, что и цвета: vxwm ловит смену RESOURCE_MANAGER
    # и применяет их к живым мониторам и окнам.
    res = f"vxwm.gappx: {int(w['gap'])}\nvxwm.borderpx: {int(w['border'])}\n"
    path = RICE_HOME / "windows.Xresources"
    r.write(path, res)
    if have_display():
        r.run(["xrdb", "-merge", str(path)])
        log(f"зазор {w['gap']}, рамка {w['border']}")


# -------------------------------------------------------------- композитор

def apply_compositor(cfg, r: Runner):
    c = cfg["compositor"]
    print("композитор")
    if not c.get("enabled", True):
        # Сначала сторож, иначе он поднимет picom обратно через секунду.
        r.run(["pkill", "-f", KEEPER_NAME])
        r.run(["pkill", "-x", "picom"])
        log("выключен")
        return

    def b(key):
        return "true" if c.get(key, True) else "false"

    subs = {
        "VSYNC": b("vsync"),
        "BACKEND": c.get("backend", "glx"),
        "USE_DAMAGE": b("use_damage"),
        "ANIMATIONS": b("animations"),
        "OPEN_ANIMATION": c.get("open_animation", "zoom"),
        "CLOSE_ANIMATION": c.get("close_animation", "slide-down"),
        "SHADOW": b("shadow"),
        "SHADOW_RADIUS": c.get("shadow_radius", 18),
        "SHADOW_OPACITY": c.get("shadow_opacity", 0.4),
        "BLUR": b("blur"),
        "BLUR_STRENGTH": c.get("blur_strength", 6),
        "FADING": b("fading"),
        "CORNER_RADIUS": c.get("corner_radius", 8),
        "BAR_OPACITY": c.get("bar_opacity", 0.80),
        "INACTIVE_OPACITY": c.get("inactive_opacity", 0.82),
        "MENU_OPACITY": c.get("menu_opacity", 0.85),
        "BAR_OPACITY_PCT": pct(c.get("bar_opacity", 0.80)),
        "WINDOW_OPACITY_PCT": pct(c.get("window_opacity", 0.90)),
        "MENU_OPACITY_PCT": pct(c.get("menu_opacity", 0.85)),
    }
    tmpl = (RICE / "templates" / "picom.conf.tmpl").read_text()
    dest = CONFIG_HOME / "picom" / "picom.conf"
    r.write(dest, mktheme.render(tmpl, subs))

    if not have_display():
        return
    if r.dry:
        log("перезапустил бы picom")
        return
    # Дожидаемся смерти старого: он до последнего держит composite overlay, и
    # запущенный внахлёст новый picom молча падает с "another composite manager
    # is already running". Раньше так и выходило -- после apply.py не оставалось
    # ни одного picom, и прозрачность с размытием просто пропадали.
    # Сторожа гасим первым: он для того и живёт, чтобы поднимать упавший
    # picom, и наш pkill он принял бы за падение.
    subprocess.run(["pkill", "-f", KEEPER_NAME], capture_output=True)
    subprocess.run(["pkill", "-x", "picom"], capture_output=True)
    for _ in range(40):
        if subprocess.run(["pgrep", "-x", "picom"],
                          capture_output=True).returncode != 0:
            break
        time.sleep(0.05)
    else:
        subprocess.run(["pkill", "-9", "-x", "picom"], capture_output=True)
        time.sleep(0.2)
    if not shutil.which("picom"):
        return
    # Через сторожа: picom этой сборки иногда падает сам по себе, и без него
    # это выглядит как "разом пропали все анимации" без единого следа на
    # экране. Нет скрипта -- запускаем picom напрямую, как раньше.
    if KEEPER.exists():
        subprocess.Popen([str(KEEPER), str(dest)], start_new_session=True,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        log("picom перезапущен под сторожем")
    else:
        subprocess.Popen(["picom", "-b", "--config", str(dest)],
                         start_new_session=True,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        log("picom перезапущен")


# ------------------------------------------------------ сессия и питание

def apply_session(cfg, r: Runner):
    """Скрипты сессии читают не TOML, а простой env-файл: разбирать TOML в
    bash нечем, а source -- одна строка."""
    s, p = cfg["session"], cfg["power"]
    inp = cfg.get("input", {})
    print("сессия и питание")
    env = (
        "# Собран из rice.toml генератором apply.py. Правки затрутся.\n"
        "# Восстановления сессии здесь нет намеренно: им управляет файл-флаг\n"
        "# no-session-restore, который читает session-lib.sh. Два выключателя\n"
        "# у одной настройки рано или поздно разойдутся.\n"
        f"export VXWM_SESSION_INTERVAL={int(s.get('snapshot_interval', 20))}\n"
        f"export VXWM_LOCK_ON_IDLE={1 if p.get('lock_on_idle', True) else 0}\n"
        f"export VXWM_IDLE_SECONDS={int(p.get('idle_seconds', 600))}\n"
        # Раскладку применяет session.sh при старте сессии. Пустая строка --
        # не трогать системную: на чужой машине рис не должен переучивать
        # клавиатуру под себя.
        f"export VXWM_KB_LAYOUT={shlex.quote(str(inp.get('layout', '')))}\n"
        f"export VXWM_KB_OPTIONS={shlex.quote(str(inp.get('options', '')))}\n"
    )
    r.write(RICE_HOME / "session.env", env)

    # Прежний выключатель восстановления -- файл-флаг. Держим его в согласии с
    # конфигом, иначе настройка в приложении и файл на диске спорили бы.
    flag = RICE_HOME / "no-session-restore"
    if s.get("restore", True):
        if flag.exists() and not r.dry:
            flag.unlink()
    elif not r.dry:
        flag.touch()
    log("восстановление сессии: " + ("включено" if s.get("restore", True) else "выключено"))


def set_theme(name: str, dry: bool = False):
    """Правим одну строку в живом rice.toml, а не переписываем файл целиком:
    в нём комментарии, которые и есть документация настроек."""
    import re

    if not (RICE / "palettes" / f"{name}.toml").exists():
        raise SystemExit(f"нет палитры {name}")
    if dry:
        log(f"записал бы в rice.toml тему {name}")
        return
    if not RICE_TOML.exists():
        RICE_TOML.parent.mkdir(parents=True, exist_ok=True)
        RICE_TOML.write_text((RICE / "rice.toml").read_text())

    out, section, done = [], None, False
    for line in RICE_TOML.read_text().splitlines():
        m = re.match(r"\s*\[(\w+)\]", line)
        if m:
            section = m.group(1)
        elif section == "theme" and re.match(r"\s*name\s*=", line) and not done:
            line = f'name = "{name}"'
            done = True
        out.append(line)
    if not done:
        raise SystemExit("в rice.toml нет [theme] name -- правь файл руками")
    RICE_TOML.write_text("\n".join(out) + "\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--set-theme", metavar="ИМЯ",
                    help="записать тему в rice.toml и применить её")
    ap.add_argument("--theme", action="store_true", help="только тема")
    ap.add_argument("--dry-run", action="store_true", help="ничего не менять")
    args = ap.parse_args()

    if args.set_theme:
        set_theme(args.set_theme, args.dry_run)
        args.theme = True

    cfg = load_rice()
    if args.set_theme:
        cfg["theme"]["name"] = args.set_theme
    r = Runner(args.dry_run)

    apply_theme(cfg, r)
    if not args.theme:
        apply_windows(cfg, r)
        apply_compositor(cfg, r)
        apply_session(cfg, r)
    print("готово")


if __name__ == "__main__":
    main()
