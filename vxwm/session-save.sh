#!/usr/bin/env bash
# Снимок открытых окон: что запущено, на каком теге и из какого каталога.
#
#   session-save.sh           -- снять снимок один раз
#   session-save.sh --watch   -- демон: снимок раз в N секунд и ещё раз по
#                                SIGTERM, когда сессия закрывается
#
# Демон нужен не ради частоты, а ради внезапного выключения: кнопку питания и
# пропажу электричества никакой хук на выходе не переживёт, а снимок двадцати-
# секундной давности -- переживёт.

set -uo pipefail

source "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/session-lib.sh"

INTERVAL=${VXWM_SESSION_INTERVAL:-20}

snapshot() {
    session_enabled || return 0

    local clients
    clients=$(session_clients) || return 1
    # Пустой список -- скорее всего сессия только поднимается или уже легла.
    # Затирать им прошлый снимок нельзя: ровно так теряются все окна.
    [ -n "$clients" ] || return 0

    local out="" seen=" " win cmd exe cwd desk
    for win in $clients; do
        session_read_window "$win" || continue

        case $win_type in
            *_NET_WM_WINDOW_TYPE_DOCK* | *_NET_WM_WINDOW_TYPE_DESKTOP* \
                | *_NET_WM_WINDOW_TYPE_SPLASH*) continue ;;
        esac

        # Одно приложение -- одна запись, сколько бы окон оно ни открыло:
        # второй раз запускать его при восстановлении не нужно, свои окна оно
        # откроет само.
        case $seen in *" $win_pid "*) continue ;; esac

        [ -r "/proc/$win_pid/cmdline" ] || continue
        cmd=$(tr '\0' "$SESSION_SEP" <"/proc/$win_pid/cmdline")
        cmd=${cmd%"$SESSION_SEP"}
        [ -n "$cmd" ] || continue

        exe=${cmd%%"$SESSION_SEP"*}
        case ${exe##*/} in $SESSION_SKIP) continue ;; esac

        cwd=$(session_deep_cwd "$win_pid") || cwd=$HOME
        [ -d "$cwd" ] || cwd=$HOME
        desk=${win_desktop:-0}

        seen+="$win_pid "
        out+=$(printf '%s\t%s\t%s\t%s' "$desk" "$cwd" "$win_class" "$cmd")$'\n'
    done

    [ -n "$out" ] || return 0

    mkdir -p "$SESSION_DIR"
    # Пишем через временный файл: снимок читают на старте сессии, и наткнуться
    # там на половину строки -- значит потерять хвост списка окон.
    local tmp="$SESSION_FILE.tmp.$$"
    {
        printf '#desktop %s\n' "$(session_current_desktop)"
        printf '%s' "$out"
    } >"$tmp" && mv -f "$tmp" "$SESSION_FILE"
    rm -f "$tmp"
}

if [ "${1:-}" = "--watch" ]; then
    # Последний снимок -- по сигналу завершения: в этот момент и WM, и клиенты
    # ещё живы, так что он самый точный из всех.
    trap 'snapshot; exit 0' TERM HUP INT
    while :; do
        snapshot
        # sleep фоном + wait, иначе сигнал ждал бы конца интервала.
        sleep "$INTERVAL" &
        wait $!
    done
else
    snapshot
fi
