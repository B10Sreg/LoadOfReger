#!/usr/bin/env python3
"""
vxwm-search: Ультимативный поиск для Load of Reger.

Объединяет:
- Запуск приложений (с иконками, локализацией и категориями)
- Умный калькулятор (арифметика, проценты, тригонометрия, логарифмы, степени, hex/bin)
- Поиск в интернете (Google, YouTube, GitHub, Яндекс, DuckDuckGo, Wikipedia, ArchWiki, Translate)
- Локальный ИИ (Ollama: быстрый ответ в окне/уведомлении + интерактивный чат в Kitty)
- Поиск файлов и папок (~/, /, f:)
- Системные действия (:lock, :sleep, :reboot, :poweroff, :theme, :settings, :shot)
- Выполнение команд терминала (>, $)
"""

import ast
import glob
import json
import math
import operator
import os
import re
import shlex
import subprocess
import sys
import urllib.parse
import urllib.request

# -----------------------------------------------------------------------------
# Конфигурация и пути
# -----------------------------------------------------------------------------
HOME = os.path.expanduser("~")
CACHE_DIR = os.path.join(HOME, ".cache", "vxwm-search")
APPS_CACHE_FILE = os.path.join(CACHE_DIR, "apps_cache.json")
OLLAMA_API = "http://127.0.0.1:11434"
DEFAULT_AI_MODEL = "hermes-8k:latest"

os.makedirs(CACHE_DIR, exist_ok=True)

# -----------------------------------------------------------------------------
# 1. Безопасный калькулятор
# -----------------------------------------------------------------------------
CALC_OPS = {
    ast.Add: operator.add,
    ast.Sub: operator.sub,
    ast.Mult: operator.mul,
    ast.Div: operator.truediv,
    ast.FloorDiv: operator.floordiv,
    ast.Mod: operator.mod,
    ast.Pow: operator.pow,
    ast.BitXor: operator.pow,  # 2^10 как степень
    ast.USub: operator.neg,
    ast.UAdd: operator.pos,
}

CALC_FUNCS = {
    "sqrt": math.sqrt,
    "cbrt": math.cbrt,
    "sin": math.sin,
    "cos": math.cos,
    "tan": math.tan,
    "asin": math.asin,
    "acos": math.acos,
    "atan": math.atan,
    "degrees": math.degrees,
    "deg": math.degrees,
    "radians": math.radians,
    "rad": math.radians,
    "log": math.log,
    "log10": math.log10,
    "log2": math.log2,
    "ln": math.log,
    "exp": math.exp,
    "abs": abs,
    "round": round,
    "ceil": math.ceil,
    "floor": math.floor,
    "factorial": math.factorial,
    "fact": math.factorial,
    "gcd": math.gcd,
    "hex": hex,
    "bin": bin,
    "oct": oct,
}

CALC_CONSTS = {
    "pi": math.pi,
    "e": math.e,
    "tau": math.tau,
}


def _eval_ast(node):
    if isinstance(node, ast.Constant):
        if isinstance(node.value, (int, float)):
            return node.value
        raise ValueError("Неподдерживаемая константа")
    elif isinstance(node, ast.BinOp):
        op = type(node.op)
        if op not in CALC_OPS:
            raise ValueError(f"Оператор {op} не поддерживается")
        return CALC_OPS[op](_eval_ast(node.left), _eval_ast(node.right))
    elif isinstance(node, ast.UnaryOp):
        op = type(node.op)
        if op not in CALC_OPS:
            raise ValueError(f"Унарный оператор {op} не поддерживается")
        return CALC_OPS[op](_eval_ast(node.operand))
    elif isinstance(node, ast.Name):
        if node.id in CALC_CONSTS:
            return CALC_CONSTS[node.id]
        raise ValueError(f"Неизвестная константа: {node.id}")
    elif isinstance(node, ast.Call):
        if not isinstance(node.func, ast.Name) or node.func.id not in CALC_FUNCS:
            raise ValueError("Неизвестная функция")
        func = CALC_FUNCS[node.func.id]
        args = [_eval_ast(a) for a in node.args]
        return func(*args)
    raise ValueError("Неподдерживаемое выражение")


def evaluate_math(expr_str):
    """Вычисляет математическое выражение или возвращает None, если это не математика."""
    clean = expr_str.strip().lstrip("=").strip()
    if not clean:
        return None

    # Поддержка процентов вида "15% of 200" или "15% * 200"
    m_pct_of = re.match(r"^([\d.]+)\s*%\s*(?:of|\*)\s*([\d.]+)$", clean, re.I)
    if m_pct_of:
        pct = float(m_pct_of.group(1))
        val = float(m_pct_of.group(2))
        return (pct / 100.0) * val

    # Поддержка процентов вида "200 + 15%" или "200 - 15%"
    m_pct_add = re.match(r"^([\d.]+)\s*([+-])\s*([\d.]+)\s*%$", clean)
    if m_pct_add:
        base = float(m_pct_add.group(1))
        sign = m_pct_add.group(2)
        pct = float(m_pct_add.group(3))
        diff = base * (pct / 100.0)
        return base + diff if sign == "+" else base - diff

    try:
        parsed = ast.parse(clean, mode="eval")
        res = _eval_ast(parsed.body)
        if isinstance(res, float) and res.is_integer():
            return int(res)
        return res
    except Exception:
        return None


def is_math_query(text):
    """Проверяет, похоже ли пользовательское выражение на калькулятор."""
    text = text.strip()
    if text.startswith("="):
        return True
    # Содержит операторы или мат-функции и цифры
    if re.search(r"[\d]\s*[\+\-\*\/\^%]\s*[\d]", text):
        return True
    if re.match(r"^(sqrt|cbrt|sin|cos|tan|log|ln|abs|fact|hex|bin)\s*\(", text, re.I):
        return True
    if re.match(r"^[\d.]+\s*%\s*(of|\*|\+|-)", text, re.I):
        return True
    return False


# -----------------------------------------------------------------------------
# 2. Индексация приложений (.desktop)
# -----------------------------------------------------------------------------
def get_desktop_files():
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
    return files


def load_applications():
    """Загружает список приложений с кэшированием."""
    files = get_desktop_files()
    latest_mtime = max((os.path.getmtime(f) for f in files), default=0)

    if os.path.exists(APPS_CACHE_FILE):
        try:
            with open(APPS_CACHE_FILE, "r", encoding="utf-8") as fh:
                cache = json.load(fh)
                if cache.get("mtime") == latest_mtime and "apps" in cache:
                    return cache["apps"]
        except Exception:
            pass

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

                icon = entry.get("Icon", "application-x-executable")
                comment = entry.get("Comment[ru]") or entry.get("Comment", "")
                generic = entry.get("GenericName[ru]") or entry.get("GenericName", "")
                categories = entry.get("Categories", "")
                keywords = entry.get("Keywords[ru]") or entry.get("Keywords", "")

                apps.append({
                    "name": name,
                    "exec": exec_cmd,
                    "icon": icon,
                    "desc": comment or generic,
                    "meta": f"{name} {generic} {comment} {categories} {keywords}".lower(),
                })
        except Exception:
            continue

    apps.sort(key=lambda a: a["name"].lower())

    try:
        with open(APPS_CACHE_FILE, "w", encoding="utf-8") as fh:
            json.dump({"mtime": latest_mtime, "apps": apps}, fh, ensure_ascii=False)
    except Exception:
        pass

    return apps


# -----------------------------------------------------------------------------
# 3. Утилиты (Буфер обмена, уведомления, открытие)
# -----------------------------------------------------------------------------
def copy_to_clipboard(text):
    try:
        proc = subprocess.Popen(["xclip", "-selection", "clipboard"], stdin=subprocess.PIPE)
        proc.communicate(input=str(text).encode("utf-8"))
    except Exception:
        pass


def notify(title, msg, icon="info", urgency="normal"):
    try:
        subprocess.run(
            ["dunstify", "-a", "vxwm-search", "-u", urgency, "-i", icon, title, msg],
            check=False,
        )
    except Exception:
        try:
            subprocess.run(["notify-send", "-i", icon, "-u", urgency, title, msg], check=False)
        except Exception:
            pass


def open_url(url):
    subprocess.Popen(["xdg-open", url], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def launch_app(exec_cmd):
    """Очищает %u, %F и запускает приложение отвязанно."""
    clean_cmd = re.sub(r"%[a-zA-Z]", "", exec_cmd).strip()
    subprocess.Popen(
        shlex.split(clean_cmd),
        cwd=HOME,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )


# -----------------------------------------------------------------------------
# 4. Локальный ИИ (Ollama)
# -----------------------------------------------------------------------------
def ensure_ollama_running():
    """Проверяет доступность Ollama и при необходимости запускает сервис."""
    try:
        with urllib.request.urlopen(f"{OLLAMA_API}/api/tags", timeout=1):
            return True
    except Exception:
        pass

    # Запускаем через systemctl --user
    subprocess.run(["systemctl", "--user", "start", "ollama"], check=False)
    for _ in range(15):
        import time
        time.sleep(0.2)
        try:
            with urllib.request.urlopen(f"{OLLAMA_API}/api/tags", timeout=1):
                return True
        except Exception:
            pass
    return False


def get_available_ai_model():
    try:
        with urllib.request.urlopen(f"{OLLAMA_API}/api/tags", timeout=2) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            models = [m["name"] for m in data.get("models", [])]
            if DEFAULT_AI_MODEL in models:
                return DEFAULT_AI_MODEL
            if models:
                return models[0]
    except Exception:
        pass
    return DEFAULT_AI_MODEL


def query_ai_quick(prompt):
    """Выполняет быстрый запрос к Ollama и отображает ответ."""
    notify("🤖 ИИ думает...", f"«{prompt}»", icon="help-about")
    if not ensure_ollama_running():
        notify("❌ Ошибка ИИ", "Не удалось запустить службу Ollama", icon="dialog-error", urgency="critical")
        return

    model = get_available_ai_model()
    req_data = json.dumps({
        "model": model,
        "prompt": prompt,
        "stream": False,
    }).encode("utf-8")

    try:
        req = urllib.request.Request(
            f"{OLLAMA_API}/api/generate",
            data=req_data,
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req, timeout=120) as resp:
            result = json.loads(resp.read().decode("utf-8"))
            answer = result.get("response", "").strip()
            if not answer:
                notify("🤖 ИИ", "Пустой ответ от модели.", icon="dialog-warning")
                return

            copy_to_clipboard(answer)

            # Сохраняем последний ответ
            ans_file = os.path.join(CACHE_DIR, "last_ai_response.txt")
            with open(ans_file, "w", encoding="utf-8") as fh:
                fh.write(f"Запрос: {prompt}\n\nОтвет ({model}):\n{answer}\n")

            # Короткое уведомление
            short = answer.split("\n")[0][:120]
            notify("🤖 ИИ: Готово!", f"{short}...\n\n<i>(Полный ответ скопирован в буфер)</i>", icon="help-about")

            # Показываем ответ в диалоговом окне rofi
            rofi_msg = f"<b>Запрос:</b> {prompt}\n\n{answer}"
            subprocess.Popen([
                "rofi",
                "-e",
                rofi_msg,
                "-theme-str",
                'window{width:700px;} textbox{padding:18px;font:"JetBrainsMono Nerd Font 10";}',
            ])
    except Exception as e:
        notify("❌ Ошибка ИИ", f"{e}", icon="dialog-error", urgency="critical")


def launch_ai_chat(prompt=""):
    """Открывает плавающий терминал Kitty с Ollama."""
    ensure_ollama_running()
    model = get_available_ai_model()
    cmd = ["kitty", "--class", "local-ai", "-T", f"Local AI ({model})"]
    if prompt:
        cmd += ["-e", "ollama", "run", model, prompt]
    else:
        cmd += ["-e", "ollama", "run", model]
    subprocess.Popen(cmd, start_new_session=True)


# -----------------------------------------------------------------------------
# 5. Поиск файлов (~/, /, f)
# -----------------------------------------------------------------------------
def search_files(query, max_results=20):
    query = query.strip()
    if query.startswith("~/"):
        search_dir = os.path.expanduser("~/" + os.path.dirname(query[2:]))
        pattern = os.path.basename(query[2:])
    elif query.startswith("/"):
        search_dir = os.path.dirname(query)
        pattern = os.path.basename(query)
    else:
        search_dir = HOME
        pattern = query

    if not os.path.isdir(search_dir):
        search_dir = HOME

    results = []
    # Быстрый поиск через find с отсечением скрытых каталогов (-prune)
    try:
        cmd = ["find", search_dir, "-maxdepth", "4", "-name", ".*", "-prune", "-o", "-iname", f"*{pattern}*", "-print"]
        proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=3, text=True)
        for line in proc.stdout.splitlines():
            line = line.strip()
            if line:
                results.append(line)
                if len(results) >= max_results:
                    break
    except Exception:
        pass
    return results


# -----------------------------------------------------------------------------
# 6. Обработка поисковых запросов и веб-поиска
# -----------------------------------------------------------------------------
WEB_ENGINES = {
    "google": "https://www.google.com/search?q={}",
    "youtube": "https://www.youtube.com/results?search_query={}",
    "github": "https://github.com/search?q={}",
    "yandex": "https://ya.ru/search/?text={}",
    "duckduckgo": "https://duckduckgo.com/?q={}",
    "wikipedia": "https://ru.wikipedia.org/w/index.php?search={}",
    "archwiki": "https://wiki.archlinux.org/index.php?search={}",
    "stackoverflow": "https://stackoverflow.com/search?q={}",
    "translate": "https://translate.google.com/?sl=auto&tl=ru&text={}",
}


def search_web(engine, query):
    url_template = WEB_ENGINES.get(engine, WEB_ENGINES["google"])
    encoded = urllib.parse.quote_plus(query.strip())
    open_url(url_template.format(encoded))


# -----------------------------------------------------------------------------
# 7. Генерация элементов меню для Rofi
# -----------------------------------------------------------------------------
def generate_menu_items():
    items = []

    # Заголовок / подсказка
    print("\0prompt\x1f\uf002 Поиск")
    print("\0markup-rows\x1ftrue")

    # Системные и быстрые действия с Nerd Font иконками
    quick_actions = [
        ("\uf1ec <b>Калькулятор</b> <span size='small' alpha='60%'>Введите выражение: =2+2, sqrt(144), 15% of 200</span>",
         "accessories-calculator", "action:calc_hint", "= calc math калькулятор посчитать математика"),
        ("\uf0ac <b>Поиск в Google</b> <span size='small' alpha='60%'>? запрос или g запрос</span>",
         "google", "action:web_hint", "? g google поиск гугл интернет"),
        ("\uf4bc <b>Спросить ИИ (Ollama)</b> <span size='small' alpha='60%'>ai вопрос или ! вопрос</span>",
         "help-about", "action:ai_hint", "ai ollama chat ии нейросеть промпт hermes"),
        ("\uf120 <b>Терминал с ИИ</b> <span size='small' alpha='60%'>Интерактивный диалог в Kitty</span>",
         "utilities-terminal", "action:ai_chat", "ai chat terminal терминал чат"),
        ("\uf07b <b>Найти файлы</b> <span size='small' alpha='60%'>~ название или / путь или f файл</span>",
         "system-file-manager", "action:files_hint", "~ / f find files файлы папки документы"),
        ("\uf120 <b>Выполнить команду</b> <span size='small' alpha='60%'>&gt; команда (например: &gt; htop)</span>",
         "utilities-terminal", "action:cmd_hint", "> $ cmd run sh bash терминал выполнить"),
        ("\uf013 <b>Настройки бара и риса</b> <span size='small' alpha='60%'>vxbar-settings</span>",
         "preferences-system", "action:settings", "settings настройки vxbar vxwm rice"),
        ("\uf53f <b>Сменить тему</b> <span size='small' alpha='60%'>abyss, carbon, deep, paper, silver, yourmusic</span>",
         "preferences-desktop-theme", "action:theme_menu", "theme тема палитра abyss carbon deep paper silver yourmusic"),
        ("\uf030 <b>Сделать скриншот</b> <span size='small' alpha='60%'>Flameshot GUI</span>",
         "flameshot", "action:flameshot", "shot screen screenshot снимок экран flameshot"),
        ("\uf023 <b>Заблокировать экран</b> <span size='small' alpha='60%'>slock</span>",
         "system-lock-screen", "action:lock", ":lock замок блокировка экрана"),
        ("\uf186 <b>Гибернация</b> <span size='small' alpha='60%'>Сохранить сессию и уснуть</span>",
         "system-suspend-hibernate", "action:hibernate", ":sleep :hibernate сон гибернация"),
        ("\uf021 <b>Перезагрузить систему</b> <span size='small' alpha='60%'>systemctl reboot</span>",
         "system-reboot", "action:reboot", ":reboot перезагрузка рестарт"),
        ("\uf011 <b>Выключить компьютер</b> <span size='small' alpha='60%'>systemctl poweroff</span>",
         "system-shutdown", "action:poweroff", ":poweroff :shutdown выключение питание"),
    ]

    for label, icon, info, meta in quick_actions:
        print(f"{label}\0icon\x1f{icon}\x1finfo\x1f{info}\x1fmeta\x1f{meta}")

    # Загрузка и вывод приложений
    apps = load_applications()
    for app in apps:
        name = app["name"]
        desc = app["desc"]
        icon = app["icon"]
        exec_cmd = app["exec"]
        meta = app["meta"]

        if desc:
            display = f"<b>{name}</b> <span size='small' alpha='60%'>({desc})</span>"
        else:
            display = f"<b>{name}</b>"

        print(f"{display}\0icon\x1f{icon}\x1finfo\x1fapp:{exec_cmd}\x1fmeta\x1f{meta}")


# -----------------------------------------------------------------------------
# 8. Диспетчер действий (Enter)
# -----------------------------------------------------------------------------
def handle_action(selected_text):
    retv = os.environ.get("ROFI_RETV", "0")
    info = os.environ.get("ROFI_INFO", "")
    query = selected_text.strip()

    # 1. Если был клик/выбор конкретного элемента с известным info
    if info.startswith("app:"):
        launch_app(info[4:])
        return
    elif info == "action:settings":
        subprocess.Popen(["vxbar-settings"], start_new_session=True)
        return
    elif info == "action:theme_menu":
        subprocess.Popen(["vxwm-theme.sh"], start_new_session=True)
        return
    elif info == "action:flameshot":
        subprocess.Popen(["flameshot", "gui"], start_new_session=True)
        return
    elif info == "action:lock":
        subprocess.Popen(["vxwm-power.sh", "lock"], start_new_session=True)
        return
    elif info == "action:hibernate":
        subprocess.Popen(["vxwm-power.sh", "hibernate"], start_new_session=True)
        return
    elif info == "action:reboot":
        subprocess.Popen(["systemctl", "reboot"], start_new_session=True)
        return
    elif info == "action:poweroff":
        subprocess.Popen(["systemctl", "poweroff"], start_new_session=True)
        return
    elif info == "action:ai_chat":
        launch_ai_chat()
        return
    elif info == "action:calc_hint":
        subprocess.Popen(["vxwm-search", "--calc"], start_new_session=True)
        return
    elif info == "action:web_hint":
        subprocess.Popen(["vxwm-search", "--web"], start_new_session=True)
        return
    elif info == "action:ai_hint":
        subprocess.Popen(["vxwm-search", "--ai"], start_new_session=True)
        return
    elif info == "action:files_hint":
        subprocess.Popen(["vxwm-search", "--files"], start_new_session=True)
        return
    elif info == "action:cmd_hint":
        subprocess.Popen(["vxwm-search", "> "], start_new_session=True)
        return

    # 2. Пользовательский ввод (Калькулятор, Поиск, ИИ, Команды)
    if not query:
        return

    # А) Калькулятор: выражение с "=" или чистая математика
    if is_math_query(query):
        res = evaluate_math(query)
        if res is not None:
            copy_to_clipboard(str(res))
            expr_clean = query.lstrip("=").strip()
            notify(
                "🧮 Калькулятор",
                f"<b>{expr_clean} = {res}</b>\n<i>(Результат скопирован в буфер обмена)</i>",
                icon="accessories-calculator",
            )
            return

    # Б) Запрос к ИИ
    if query.startswith("ai ") or query.startswith("! ") or query.startswith("@ai "):
        prompt = re.sub(r"^(ai|!|@ai)\s+", "", query).strip()
        if prompt:
            notify("🤖 ИИ думает...", f"«{prompt}»", icon="help-about")
            subprocess.Popen(
                [sys.executable, os.path.abspath(__file__), "--ai-query", prompt],
                start_new_session=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            return

    if query.startswith("ai-chat ") or query == "ai-chat":
        prompt = re.sub(r"^ai-chat\s*", "", query).strip()
        launch_ai_chat(prompt)
        return

    # В) Поиск в интернете по префиксам
    prefix_map = {
        r"^(?:\?|g|google)\s+": "google",
        r"^(?:yt|youtube)\s+": "youtube",
        r"^(?:gh|github)\s+": "github",
        r"^(?:ya|yandex)\s+": "yandex",
        r"^(?:d|ddg|duck)\s+": "duckduckgo",
        r"^(?:w|wiki|wikipedia)\s+": "wikipedia",
        r"^(?:aw|arch|archwiki)\s+": "archwiki",
        r"^so\s+": "stackoverflow",
        r"^tr\s+": "translate",
    }
    for pat, engine in prefix_map.items():
        if re.search(pat, query, re.I):
            clean_q = re.sub(pat, "", query, flags=re.I).strip()
            search_web(engine, clean_q)
            return

    # Г) Системные команды через ":"
    if query.startswith(":"):
        cmd = query[1:].strip().lower()
        if cmd in ("lock", "блок"):
            subprocess.Popen(["vxwm-power.sh", "lock"], start_new_session=True)
        elif cmd in ("sleep", "hibernate", "сон"):
            subprocess.Popen(["vxwm-power.sh", "hibernate"], start_new_session=True)
        elif cmd in ("reboot", "restart", "ребут"):
            subprocess.Popen(["systemctl", "reboot"], start_new_session=True)
        elif cmd in ("poweroff", "shutdown", "выкл"):
            subprocess.Popen(["systemctl", "poweroff"], start_new_session=True)
        elif cmd in ("settings", "настройки", "бар"):
            subprocess.Popen(["vxbar-settings"], start_new_session=True)
        elif cmd.startswith("theme"):
            parts = cmd.split(maxsplit=1)
            if len(parts) > 1:
                subprocess.Popen(["vxwm-rice-apply", "--set-theme", parts[1]], start_new_session=True)
            else:
                subprocess.Popen(["vxwm-theme.sh"], start_new_session=True)
        elif cmd in ("shot", "скрин"):
            subprocess.Popen(["flameshot", "gui"], start_new_session=True)
        elif cmd == "picom":
            subprocess.Popen(["vxwm-rice-apply", "--compositor"], start_new_session=True)
        return

    # Д) Команды терминала через ">" или "$"
    if query.startswith(">") or query.startswith("$"):
        cmd = query[1:].strip()
        if cmd:
            subprocess.Popen(["kitty", "--hold", "-e", "bash", "-c", cmd], start_new_session=True)
            return

    # Е) Файлы (~/, /, f:)
    if query.startswith("~/") or query.startswith("/") or query.startswith("f "):
        path = query[2:].strip() if query.startswith("f ") else query
        expanded = os.path.expanduser(path)
        if os.path.exists(expanded):
            subprocess.Popen(["xdg-open", expanded], start_new_session=True)
            return
        else:
            # Ищем и открываем первый подходящий результат
            matches = search_files(path)
            if matches:
                subprocess.Popen(["xdg-open", matches[0]], start_new_session=True)
                return

    # Ж) Фолбэк: если пользователь просто ввёл текст, которого нет в списке приложений
    # и это не команда — открываем поиск в Google!
    search_web("google", query)


# -----------------------------------------------------------------------------
# Точка входа
# -----------------------------------------------------------------------------
def main():
    if len(sys.argv) > 2 and sys.argv[1] == "--ai-query":
        query_ai_quick(sys.argv[2])
        return

    retv = os.environ.get("ROFI_RETV", "0")

    if retv == "0" and len(sys.argv) == 1:
        # Rofi запрашивает список элементов
        generate_menu_items()
    else:
        # Пользователь нажал Enter на элементе или ввёл текст
        selected = sys.argv[1] if len(sys.argv) > 1 else ""
        handle_action(selected)


if __name__ == "__main__":
    main()
