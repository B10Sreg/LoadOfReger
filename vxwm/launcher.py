#!/usr/bin/env python3
"""
vxwm-launcher: Высокопроизводительный TUI лаунчер приложений для vxwm.
- Мгновенный поиск с фильтрацией на лету (поддержка кириллицы и латиницы)
- Nerd Font иконки + ANSI плашки категорий
- Красивые ASCII / ANSI арт баннеры карточек
- Переключение категорий по Tab и Shift+Tab
- Запуск приложений в фоне с немедленным закрытием терминала
"""

import os
import sys
import json
import glob
import re
import shlex
import subprocess
import termios
import tty
import select
import signal
import unicodedata

HOME = os.path.expanduser("~")
CACHE_DIR = os.path.join(HOME, ".cache", "vxwm-search")
APPS_CACHE_FILE = os.path.join(CACHE_DIR, "apps_cache.json")

# -----------------------------------------------------------------------------
# Цветовая палитра ANSI TrueColor (YourMusic / Obsidian)
# -----------------------------------------------------------------------------
RESET     = "\033[0m"
BOLD      = "\033[1m"
DIM       = "\033[2m"
ITALIC    = "\033[3m"
UNDERLINE = "\033[4m"

# Цвета текста
C_TEXT    = "\033[38;2;224;226;236m"
C_MUTED   = "\033[38;2;115;120;140m"
C_BORDER  = "\033[38;2;48;52;68m"
C_CYAN    = "\033[38;2;56;189;248m"
C_GOLD    = "\033[38;2;246;193;119m"
C_GREEN   = "\033[38;2;74;222;128m"
C_PURPLE  = "\033[38;2;196;167;231m"
C_RED     = "\033[38;2;248;113;113m"
C_ORANGE  = "\033[38;2;251;146;60m"

# Фоновые подсветки
BG_SEL         = "\033[48;2;36;42;60m"
BG_BADGE_WEB   = "\033[48;2;22;40;65m\033[38;2;96;176;255m"
BG_BADGE_DEV   = "\033[48;2;45;30;60m\033[38;2;196;167;231m"
BG_BADGE_CLI   = "\033[48;2;20;45;32m\033[38;2;74;222;128m"
BG_BADGE_GAME  = "\033[48;2;55;35;20m\033[38;2;251;146;60m"
BG_BADGE_MEDIA = "\033[48;2;50;25;45m\033[38;2;244;114;182m"
BG_BADGE_SYS   = "\033[48;2;32;36;45m\033[38;2;156;163;175m"
BG_BADGE_FILE  = "\033[48;2;30;45;45m\033[38;2;45;212;191m"
BG_BADGE_CHAT  = "\033[48;2;45;40;22m\033[38;2;250;204;21m"
BG_BADGE_AI    = "\033[48;2;40;25;55m\033[38;2;216;180;254m"

# -----------------------------------------------------------------------------
# Категории и ASCII Art (все символы моноширинные)
# -----------------------------------------------------------------------------
ASCII_ART = {
    "web": [
        " ╭──────────────╮   󰖟 WEB BROWSER",
        " │   ●      ●   │   Internet & Cloud",
        " │      ▲       │   Protocol: HTTPS",
        " │   ───────    │   Rendering: Modern",
        " │  ◄   ▶   ↻   │   Status: Online",
        " ╰──────────────╯",
    ],
    "dev": [
        " ╭──────────────╮    DEVELOPMENT",
        " │  < / >  { }  │   Code, Compile & Build",
        " │  class App:  │   Integrated Dev Env",
        " │    return 0  │   Language / Runtime",
        " │  [ git:main ]│   Status: Ready",
        " ╰──────────────╯",
    ],
    "cli": [
        " ╭──────────────╮    TERMINAL & SHELL",
        " │  reg@arch:~$ │   Command Line Interface",
        " │  >_ run app  │   POSIX Environment",
        " │  [  OK  ]    │   Process / Daemon",
        " │  0.0ms 100%  │   Status: Active",
        " ╰──────────────╯",
    ],
    "chat": [
        " ╭──────────────╮   󰍡 MESSENGER & CHAT",
        " │  ╭───╮       │   Instant Communication",
        " │  │ ● │ ╭───╮ │   Communities & Teams",
        " │  ╰───╯ │ ● │ │   End-to-End Messages",
        " │        ╰───╯ │   Status: Connected",
        " ╰──────────────╯",
    ],
    "media": [
        " ╭──────────────╮    AUDIO & MEDIA",
        " │  ♫       ♪   │   Music / Video Player",
        " │    ▶  ▌▌  ■  │   Stereo Playback Engine",
        " │  ılı.lıllılı │   High Definition Output",
        " │  [ 02:45 ]   │   Status: Playing",
        " ╰──────────────╯",
    ],
    "game": [
        " ╭──────────────╮   󰊴 VIDEO GAME",
        " │   ▲      ▲   │   Interactive Gaming",
        " │ ◄   ●  ►     │   Vulkan / OpenGL Engine",
        " │   ▼      ▼   │   Steam & Native Arcade",
        " │  [ START ]   │   Status: Ready to play",
        " ╰──────────────╯",
    ],
    "file": [
        " ╭──────────────╮   󰉋 FILE MANAGER",
        " │  ╭───────╮   │   Storage & Archives",
        " │  │ ~/data│   │   Local File Explorer",
        " │  │ ──────│   │   Mounts & Directories",
        " │  [ Storage ] │   Status: Mounted",
        " ╰──────────────╯",
    ],
    "sys": [
        " ╭──────────────╮   󰒓 SYSTEM CONFIG",
        " │    ╭───╮     │   Desktop & Hardware",
        " │  ──│ ⚙ │──   │   Kernel & Modules",
        " │    ╰───╯     │   Configuration Tools",
        " │  STATUS: OK  │   Status: Verified",
        " ╰──────────────╯",
    ],
    "ai": [
        " ╭──────────────╮   󰚩 ARTIFICIAL INTEL",
        " │   [ AI ]     │   Neural Network Core",
        " │   (•  •)     │   Local Large Language",
        " │   [====]     │   Ollama / Hermes Model",
        " │   READY      │   Status: Running",
        " ╰──────────────╯",
    ],
}

CATEGORIES = [
    ("all", "Все"),
    ("web", "Интернет"),
    ("dev", "Разработка"),
    ("cli", "Терминал"),
    ("chat", "Общение"),
    ("media", "Медиа"),
    ("game", "Игры"),
    ("file", "Файлы"),
    ("sys", "Система"),
]

def strip_ansi(s):
    return re.sub(r"\x1b\[[0-9;]*[a-zA-Z]", "", s)

def str_width(s):
    clean = strip_ansi(s)
    # Emojis and full-width chars take 2 columns, standard & Nerd Font glyphs in Kitty take 1
    w = 0
    for c in clean:
        if unicodedata.east_asian_width(c) in ('W', 'F'):
            w += 2
        else:
            w += 1
    return w

def classify_app(name, exec_cmd, categories, comment=""):
    nl = name.lower()
    el = exec_cmd.lower()
    cl = comment.lower()
    catl = categories.lower()

    # 1. AI / LLM
    if any(k in nl or k in el for k in ("ollama", "local-ai", "artcraft", "claude", "gpt")) or "ai" in nl:
        return "ai", "󰚩", BG_BADGE_AI, " AI "

    # 2. Chat & Messenger
    if any(k in nl or k in el for k in ("discord", "telegram", "ayugram", "regram", "element", "signal", "slack", "vk")):
        return "chat", "󰍡", BG_BADGE_CHAT, " CHAT "

    # 3. Web browsers
    if any(k in nl or k in el for k in ("firefox", "chrome", "chromium", "clash", "surf", "zen", "brave", "tor", "edge")):
        return "web", "󰈹", BG_BADGE_WEB, " WEB "

    # 4. Dev & 3D & Editors
    if any(k in nl or k in el for k in ("code", "antigravity", "blender", "unity", "godot", "neovim", "vim", "cmake", "qt", "git", "sublime")):
        return "dev", "", BG_BADGE_DEV, " DEV "

    # 5. Media & Audio
    if any(k in nl or k in el for k in ("spotify", "mpv", "vlc", "obs", "audacity", "music", "player", "kdenlive", "gimp", "inkscape")):
        return "media", "", BG_BADGE_MEDIA, " MEDIA "

    # 6. File managers & Archives
    if any(k in nl or k in el for k in ("thunar", "7z", "zip", "nautilus", "dolphin", "pcmanfm", "ranger", "archive", "file")):
        return "file", "󰉋", BG_BADGE_FILE, " FILE "

    # 7. Terminals & CLI
    if any(k in nl or k in el for k in ("kitty", "alacritty", "terminal", "btop", "htop", "bash", "zsh", "fish", "wezterm")):
        return "cli", "", BG_BADGE_CLI, " CLI "

    # 8. Games
    if any(k in nl or k in el for k in ("steam", "minecraft", "cristalix", "ddnet", "portal", "rounds", "ark", "geometry", "lutris", "heroic")):
        return "game", "󰊴", BG_BADGE_GAME, " GAME "

    # 9. Fallback on broader categories / comments
    if any(k in catl for k in ("development", "ide", "programming")):
        return "dev", "", BG_BADGE_DEV, " DEV "
    if any(k in catl for k in ("chat", "telephony", "instantmessaging")):
        return "chat", "󰍡", BG_BADGE_CHAT, " CHAT "
    if any(k in catl for k in ("audio", "video", "multimedia", "player")):
        return "media", "", BG_BADGE_MEDIA, " MEDIA "
    if any(k in catl for k in ("game", "games")):
        return "game", "󰊴", BG_BADGE_GAME, " GAME "
    if any(k in catl for k in ("network", "webbrowser")):
        return "web", "󰈹", BG_BADGE_WEB, " WEB "
    if any(k in catl for k in ("settings", "system", "utility", "hardwaresettings")):
        return "sys", "󰒓", BG_BADGE_SYS, " SYS "

    return "sys", "󰘳", BG_BADGE_SYS, " APP "

# -----------------------------------------------------------------------------
# Загрузка приложений
# -----------------------------------------------------------------------------
def load_apps():
    dirs = [
        os.path.join(HOME, ".local", "share", "applications"),
        "/usr/local/share/applications",
        "/usr/share/applications",
    ]
    seen = set()
    files = []
    for d in dirs:
        if not os.path.isdir(d):
            continue
        for f in glob.glob(os.path.join(d, "*.desktop")):
            bname = os.path.basename(f)
            if bname not in seen:
                seen.add(bname)
                files.append(f)

    latest_mtime = max((os.path.getmtime(f) for f in files), default=0)
    apps = []
    for f in files:
        try:
            with open(f, "r", encoding="utf-8", errors="ignore") as fh:
                entry = {}
                is_desktop_entry = False
                for line in fh:
                    line = line.strip()
                    if line == "[Desktop Entry]":
                        is_desktop_entry = True
                        continue
                    elif line.startswith("[") and is_desktop_entry:
                        break
                    if is_desktop_entry and "=" in line:
                        k, v = line.split("=", 1)
                        entry[k.strip()] = v.strip()

                if entry.get("Type") != "Application":
                    continue
                if entry.get("NoDisplay", "").lower() == "true":
                    continue

                name = entry.get("Name[ru]") or entry.get("Name")
                exec_cmd = entry.get("Exec")
                if not name or not exec_cmd:
                    continue

                comment = entry.get("Comment[ru]") or entry.get("Comment", "")
                generic = entry.get("GenericName[ru]") or entry.get("GenericName", "")
                categories = entry.get("Categories", "")
                keywords = entry.get("Keywords[ru]") or entry.get("Keywords", "")

                cat_key, icon, badge, badge_text = classify_app(name, exec_cmd, categories, comment)

                apps.append({
                    "name": name,
                    "exec": exec_cmd,
                    "desc": comment or generic,
                    "meta": f"{name} {generic} {comment} {categories} {keywords}".lower(),
                    "cat_key": cat_key,
                    "icon": icon,
                    "badge": badge,
                    "badge_text": badge_text,
                })
        except Exception:
            continue

    apps.sort(key=lambda a: a["name"].lower())
    try:
        os.makedirs(CACHE_DIR, exist_ok=True)
        with open(APPS_CACHE_FILE, "w", encoding="utf-8") as fh:
            json.dump({"mtime": latest_mtime, "apps": apps}, fh, ensure_ascii=False)
    except Exception:
        pass

    return apps

# -----------------------------------------------------------------------------
# Запуск приложения
# -----------------------------------------------------------------------------
def launch_app(exec_cmd):
    clean_cmd = re.sub(r"%[a-zA-Z]", "", exec_cmd).strip()
    try:
        args = shlex.split(clean_cmd)
        subprocess.Popen(
            args,
            cwd=HOME,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
        )
    except Exception:
        try:
            subprocess.Popen(
                clean_cmd,
                shell=True,
                cwd=HOME,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                stdin=subprocess.DEVNULL,
                start_new_session=True,
            )
        except Exception as e:
            subprocess.run(["notify-send", "Ошибка запуска", str(e)])

# -----------------------------------------------------------------------------
# Интерактивный TUI
# -----------------------------------------------------------------------------
class LauncherTUI:
    def __init__(self, apps):
        self.apps = apps
        self.query = ""
        self.selected_idx = 0
        self.cat_idx = 0
        self.filtered = []
        self.scroll_offset = 0
        self.running = True
        self.chosen_exec = None
        self.update_filter()

    def update_filter(self):
        q = self.query.strip().lower()
        active_cat = CATEGORIES[self.cat_idx][0]

        res = []
        for a in self.apps:
            if active_cat != "all" and a["cat_key"] != active_cat:
                continue
            if not q:
                res.append((0, a))
            else:
                name_l = a["name"].lower()
                if name_l.startswith(q):
                    res.append((3, a))
                elif q in name_l:
                    res.append((2, a))
                elif q in a["meta"]:
                    res.append((1, a))

        if q:
            res.sort(key=lambda x: (-x[0], x[1]["name"].lower()))
        self.filtered = [x[1] for x in res]
        self.selected_idx = max(0, min(self.selected_idx, len(self.filtered) - 1))
        self.scroll_offset = max(0, min(self.scroll_offset, len(self.filtered) - 1))

    def render(self):
        try:
            cols, rows = os.get_terminal_size()
        except Exception:
            cols, rows = 103, 27

        box_w = cols
        box_h = rows

        left_w = max(38, int(box_w * 0.50))
        right_w = box_w - left_w - 3

        lines = []

        # 1. Верхняя граница
        lines.append(f"{C_BORDER}╭{'─' * (box_w - 2)}╮{RESET}")

        # 2. Строка поиска
        search_prompt = f" {C_CYAN} {RESET} {BOLD}{C_TEXT}Поиск:{RESET} "
        cursor_char = f"{C_CYAN}█{RESET}"
        disp_query = self.query + cursor_char
        max_q_len = max(10, box_w - 32)
        if len(self.query) > max_q_len:
            disp_query = "…" + self.query[-max_q_len:] + cursor_char

        counter_clean = f"{self.selected_idx + 1 if self.filtered else 0}/{len(self.filtered)}"
        counter_str = f"{C_MUTED}{counter_clean}{RESET} "

        clean_prompt_w = str_width(search_prompt) + str_width(disp_query)
        space_len = max(1, box_w - 2 - clean_prompt_w - len(counter_clean) - 1)
        lines.append(f"{C_BORDER}│{RESET}{search_prompt}{disp_query}{' ' * space_len}{counter_str}{C_BORDER}│{RESET}")

        # 3. Разделитель строки поиска и категорий
        lines.append(f"{C_BORDER}├{'─' * (box_w - 2)}┤{RESET}")

        # 4. Строка категорий (Pill Bar)
        cat_pills = []
        for idx, (ck, cname) in enumerate(CATEGORIES):
            if idx == self.cat_idx:
                cat_pills.append(f"{BG_SEL}{BOLD}{C_GOLD} {cname} {RESET}")
            else:
                cat_pills.append(f"{C_MUTED} {cname} {RESET}")
        cat_line = " ".join(cat_pills)
        cat_pad = max(0, box_w - 4 - str_width(cat_line))
        lines.append(f"{C_BORDER}│{RESET} {cat_line}{' ' * cat_pad} {C_BORDER}│{RESET}")

        # 5. Разделитель с Т-образным стыком
        lines.append(f"{C_BORDER}├{'─' * left_w}┬{'─' * right_w}┤{RESET}")

        # 6. Контент: левый список и правая карточка
        list_h = max(3, box_h - 8)
        if self.selected_idx < self.scroll_offset:
            self.scroll_offset = self.selected_idx
        elif self.selected_idx >= self.scroll_offset + list_h:
            self.scroll_offset = self.selected_idx - list_h + 1

        selected_app = self.filtered[self.selected_idx] if self.filtered else None

        right_lines = []
        if selected_app:
            cat_k = selected_app["cat_key"]
            art = ASCII_ART.get(cat_k, ASCII_ART["sys"])
            for al in art:
                right_lines.append(f"{C_CYAN}{al}{RESET}")
            right_lines.append(f"{C_BORDER}{'─' * (right_w - 2)}{RESET}")
            right_lines.append(f"{BOLD}{C_GOLD}{selected_app['name'][:max(10, right_w - 4)]}{RESET}")

            badge_str = f"{selected_app['badge']}{selected_app['badge_text']}{RESET}"
            right_lines.append(f"{C_MUTED}Категория:{RESET} {badge_str}")

            clean_cmd = re.sub(r"%[a-zA-Z]", "", selected_app["exec"]).strip()
            right_lines.append(f"{C_MUTED}Команда:{RESET} {C_TEXT}{clean_cmd[:max(10, right_w - 12)]}{RESET}")

            if selected_app["desc"]:
                right_lines.append(f"{C_MUTED}Инфо:{RESET} {C_TEXT}{selected_app['desc'][:max(10, right_w - 10)]}{RESET}")

            right_lines.append(f"{C_BORDER}{'─' * (right_w - 2)}{RESET}")
            right_lines.append(f"{C_GREEN}↵ Enter{RESET} {C_MUTED}Запустить{RESET}   {C_RED}Esc{RESET} {C_MUTED}Закрыть{RESET}")
        else:
            right_lines.append(f"{C_MUTED}Ничего не найдено{RESET}")
            right_lines.append(f"{C_MUTED}Попробуйте изменить поисковый запрос{RESET}")

        for row_i in range(list_h):
            app_idx = self.scroll_offset + row_i
            if app_idx < len(self.filtered):
                app = self.filtered[app_idx]
                is_sel = (app_idx == self.selected_idx)

                ptr = "▸" if is_sel else " "
                icon = app["icon"]
                badge = f"{app['badge']}{app['badge_text']}{RESET}"

                avail_name_w = max(10, left_w - 15)
                name_disp = app["name"]
                if len(name_disp) > avail_name_w:
                    name_disp = name_disp[:avail_name_w - 1] + "…"

                if is_sel:
                    core = f" {ptr} {C_GOLD}{icon}{RESET}{BG_SEL} {BOLD}{C_TEXT}{name_disp}{RESET}{BG_SEL} "
                    pad = max(1, left_w - str_width(core) - str_width(badge) - 1)
                    left_cell = f"{BG_SEL}{core}{' ' * pad}{badge}{BG_SEL} {RESET}"
                else:
                    core = f" {ptr} {C_CYAN}{icon}{RESET} {C_TEXT}{name_disp}{RESET} "
                    pad = max(1, left_w - str_width(core) - str_width(badge) - 1)
                    left_cell = f"{core}{' ' * pad}{badge} "

                cell_w = str_width(left_cell)
                if cell_w < left_w:
                    left_cell += " " * (left_w - cell_w)
            else:
                left_cell = " " * left_w

            right_cell = right_lines[row_i] if row_i < len(right_lines) else ""
            pad_r = max(0, right_w - 1 - str_width(right_cell))

            lines.append(f"{C_BORDER}│{RESET}{left_cell}{C_BORDER}│{RESET} {right_cell}{' ' * pad_r}{C_BORDER}│{RESET}")

        # 7. Нижний разделитель
        lines.append(f"{C_BORDER}├{'─' * left_w}┴{'─' * right_w}┤{RESET}")

        # 8. Подвал подсказок
        hints = (
            f" {C_CYAN}↑/↓{RESET}{C_MUTED}: Выбор  "
            f"{C_CYAN}↵ Enter{RESET}{C_MUTED}: Запуск  "
            f"{C_CYAN}Tab/S-Tab{RESET}{C_MUTED}: Категории  "
            f"{C_CYAN}Esc{RESET}{C_MUTED}: Закрыть{RESET}"
        )
        hints_pad = max(0, box_w - 2 - str_width(hints))
        lines.append(f"{C_BORDER}│{RESET}{hints}{' ' * hints_pad}{C_BORDER}│{RESET}")

        # 9. Нижняя рамка
        lines.append(f"{C_BORDER}╰{'─' * (box_w - 2)}╯{RESET}")

        # Точная отрисовка каждой строки по абсолютным координатам терминала (без авто-прокрутки)
        out = ["\033[?25l\033[?7l"]  # Скрыть курсор, отключить auto-wrap
        for row_idx, line in enumerate(lines, start=1):
            if row_idx <= box_h:
                out.append(f"\033[{row_idx};1H\033[2K{line}")
        sys.stdout.write("".join(out))
        sys.stdout.flush()

    def handle_key(self, k):
        if k in ("QUIT", "ESC"):
            self.running = False
        elif k == "ENTER":
            if self.filtered:
                self.chosen_exec = self.filtered[self.selected_idx]["exec"]
            self.running = False
        elif k == "UP":
            if self.selected_idx > 0:
                self.selected_idx -= 1
        elif k == "DOWN":
            if self.selected_idx < len(self.filtered) - 1:
                self.selected_idx += 1
        elif k == "PGUP":
            self.selected_idx = max(0, self.selected_idx - 8)
        elif k == "PGDN":
            self.selected_idx = min(len(self.filtered) - 1, self.selected_idx + 8)
        elif k == "HOME":
            self.selected_idx = 0
        elif k == "END":
            self.selected_idx = max(0, len(self.filtered) - 1)
        elif k == "TAB":
            self.cat_idx = (self.cat_idx + 1) % len(CATEGORIES)
            self.update_filter()
        elif k == "SHIFT_TAB":
            self.cat_idx = (self.cat_idx - 1) % len(CATEGORIES)
            self.update_filter()
        elif k == "BACKSPACE":
            if self.query:
                self.query = self.query[:-1]
                self.update_filter()
        elif k == "CTRL_U":
            self.query = ""
            self.update_filter()
        elif k == "CTRL_W":
            parts = self.query.rsplit(" ", 1)
            self.query = parts[0] if len(parts) > 1 else ""
            self.update_filter()
        elif k and all(c.isprintable() for c in k):
            self.query += k
            self.update_filter()

def read_key(fd):
    b = os.read(fd, 64)
    if not b:
        return 'QUIT'
    if b == b'\x1b':
        r, _, _ = select.select([fd], [], [], 0.035)
        if r:
            b += os.read(fd, 64)
        else:
            return 'ESC'

    if b.startswith(b'\x1b['):
        code = b[2:]
        if code == b'A': return 'UP'
        if code == b'B': return 'DOWN'
        if code == b'C': return 'RIGHT'
        if code == b'D': return 'LEFT'
        if code.startswith(b'5~') or code == b'5': return 'PGUP'
        if code.startswith(b'6~') or code == b'6': return 'PGDN'
        if code.startswith(b'H') or code.startswith(b'1~'): return 'HOME'
        if code.startswith(b'F') or code.startswith(b'4~'): return 'END'
        if code.startswith(b'3~'): return 'DELETE'
        if code == b'Z': return 'SHIFT_TAB'
        return ''
    elif b.startswith(b'\x1bO'):
        code = b[2:]
        if code == b'A': return 'UP'
        if code == b'B': return 'DOWN'
        if code == b'C': return 'RIGHT'
        if code == b'D': return 'LEFT'
        if code == b'H': return 'HOME'
        if code == b'F': return 'END'
        return ''
    elif b == b'\x1b':
        return 'ESC'
    elif b in (b'\r', b'\n'):
        return 'ENTER'
    elif b in (b'\x7f', b'\x08'):
        return 'BACKSPACE'
    elif b == b'\t':
        return 'TAB'
    elif b in (b'\x03', b'\x11'):  # Ctrl+C, Ctrl+Q
        return 'QUIT'
    elif b == b'\x15':  # Ctrl+U
        return 'CTRL_U'
    elif b == b'\x17':  # Ctrl+W
        return 'CTRL_W'

    try:
        return b.decode('utf-8', errors='ignore')
    except Exception:
        return ''

def main():
    apps = load_apps()
    tui = LauncherTUI(apps)

    fd = sys.stdin.fileno()
    old_settings = termios.tcgetattr(fd)

    def on_resize(sig, frame):
        tui.render()

    signal.signal(signal.SIGWINCH, on_resize)

    try:
        tty.setraw(fd)
        sys.stdout.write("\033[?1049h\033[2J")  # Alternate screen buffer
        sys.stdout.flush()

        while tui.running:
            tui.render()
            k = read_key(fd)
            tui.handle_key(k)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old_settings)
        sys.stdout.write("\033[?7h\033[?25h\033[?1049l")  # Restore wrap, cursor, and screen buffer
        sys.stdout.flush()

    if tui.chosen_exec:
        launch_app(tui.chosen_exec)

if __name__ == "__main__":
    main()
