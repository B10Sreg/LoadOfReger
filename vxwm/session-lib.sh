#!/usr/bin/env bash
# Общее для session-save.sh и session-restore.sh: где лежит снимок, что в него
# не попадает и как читать свойства окон.
#
# Снимок -- это TSV, по строке на процесс:
#
#     тег <TAB> рабочий каталог <TAB> WM_CLASS <TAB> argv
#
# argv склеен символом 0x1f (US), потому что в аргументах бывают пробелы, а в
# /proc/PID/cmdline они и так разделены NUL -- нам остаётся только заменить
# один невидимый разделитель на другой, который переживает чтение в bash.
# Первая строка -- заголовок "#desktop N": тег, активный на момент снимка.

SESSION_SEP=$'\037'

SESSION_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/vxwm"
SESSION_FILE="$SESSION_DIR/session.tsv"

# Выключатель: пока файл существует, сессия не пишется и не восстанавливается.
SESSION_OFF="${XDG_CONFIG_HOME:-$HOME/.config}/vxwm-rice/no-session-restore"

# Инфраструктуру риса поднимает autostart.sh, и поднимает раньше, чем мы
# восстанавливаем окна. Попади она в снимок -- получили бы второй бар и второй
# композитор поверх уже работающих.
SESSION_SKIP='vxbar|vxbar-settings|picom|dunst|feh|xsetroot|rofi|vxwm|Xorg'

session_enabled() {
    [ ! -e "$SESSION_OFF" ]
}

# Список окон из _NET_CLIENT_LIST. Пусто -- значит либо окон нет, либо WM ещё
# не проставил свойство; вызывающий различает эти случаи сам.
session_clients() {
    local raw
    raw=$(xprop -root -notype _NET_CLIENT_LIST 2>/dev/null) || return 1
    case $raw in
        *'# '*) raw=${raw#*'# '} ;;
        *) return 0 ;;
    esac
    printf '%s\n' "${raw//,/ }"
}

# Каталог, в котором приложение стоит «с точки зрения пользователя».
#
# У терминала собственный cwd -- это место, откуда его когда-то запустили, и
# оно почти всегда бесполезно: каталог, в котором человек работает, помнит
# оболочка внутри. Поэтому спускаемся по детям, пока находим оболочку, и
# берём каталог у неё. У обычных приложений оболочек в детях нет, и мы сразу
# остаёмся с их собственным cwd -- у браузера с десятком процессов-вкладок
# правило не срабатывает и ничего не портит.
session_deep_cwd() {
    local pid=$1 depth=0 kids kid comm found dir
    while [ "$depth" -lt 8 ]; do
        # children заканчивается без перевода строки, поэтому read отдаёт
        # ненулевой код, успев наполнить переменную: смотрим на неё, а не на код.
        kids=
        read -r kids <"/proc/$pid/task/$pid/children" 2>/dev/null
        [ -n "$kids" ] || break
        found=
        for kid in $kids; do
            read -r comm <"/proc/$kid/comm" 2>/dev/null || continue
            case $comm in
                sh | bash | zsh | fish | dash | ksh | tcsh) found=$kid && break ;;
            esac
        done
        [ -n "$found" ] || break
        pid=$found
        depth=$((depth + 1))
    done
    dir=$(readlink "/proc/$pid/cwd" 2>/dev/null) || return 1
    printf '%s\n' "$dir"
}

session_current_desktop() {
    local raw
    raw=$(xprop -root -notype _NET_CURRENT_DESKTOP 2>/dev/null) || return 1
    printf '%s\n' "${raw#*= }"
}

# Свойства одного окна разом: один xprop вместо четырёх. Результат кладётся в
# переменные вызывающего -- возвращать четыре значения из функции в bash
# всё равно нечем, кроме глобалок.
session_read_window() {
    local win=$1 line
    win_pid= win_desktop= win_class= win_type=
    while IFS= read -r line; do
        case $line in
            '_NET_WM_PID = '*) win_pid=${line#*= } ;;
            '_NET_WM_DESKTOP = '*) win_desktop=${line#*= } ;;
            '_NET_WM_WINDOW_TYPE = '*) win_type=${line#*= } ;;
            'WM_CLASS = "'*)
                line=${line#*= \"}
                win_class=${line%%\"*}
                ;;
        esac
    done < <(xprop -id "$win" -notype \
        _NET_WM_PID _NET_WM_DESKTOP _NET_WM_WINDOW_TYPE WM_CLASS 2>/dev/null)
    [ -n "$win_pid" ]
}
