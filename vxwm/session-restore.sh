#!/usr/bin/env bash
# Восстановление снимка, снятого session-save.sh: запускает приложения и
# раскладывает их по тем же тегам.
#
# Тег задаётся не «подвинь окно потом», а «переключись на тег и запусти»:
# так окно с самого начала попадает куда надо и не мелькает на чужом теге, и
# так это работает независимо от того, умеет ли WM двигать чужие окна по
# client-message _NET_WM_DESKTOP. Догоняющая правка в конце всё же есть -- для
# приложений, которые форкаются и открывают окно сильно позже запуска.
#
# Геометрию не сохраняем: vxwm тайлящий, размеры и места он раздаёт сам, и
# навязывать ему прошлые -- значит драться с раскладкой.

set -uo pipefail

source "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/session-lib.sh"

# Сколько ждать появления окон одной группы, прежде чем идти к следующему тегу.
GROUP_TIMEOUT=${VXWM_SESSION_GROUP_TIMEOUT:-8}

client_count() {
    local c
    c=$(session_clients) || return 0
    # shellcheck disable=SC2086 -- нам нужно именно разбиение на слова
    set -- $c
    printf '%s\n' $#
}

desktop_count() {
    local raw
    raw=$(xprop -root -notype _NET_NUMBER_OF_DESKTOPS 2>/dev/null) || return 1
    printf '%s\n' "${raw#*= }"
}

session_enabled || exit 0
[ -s "$SESSION_FILE" ] || exit 0

# Восстанавливаем только в пустую сессию. Это и защита от повторного запуска
# autostart.sh (перевыбор темы, ручной вызов), и единственная нужная: в свежей
# сессии окон ещё нет по определению.
if [ "$(client_count)" != "0" ]; then
    exit 0
fi

ndesk=$(desktop_count) || ndesk=9
[ "$ndesk" -gt 0 ] 2>/dev/null || ndesk=9

declare -A group=()
home_desktop=0

while IFS=$'\t' read -r desk cwd class cmd; do
    case $desk in
        '#desktop '*)
            home_desktop=${desk#*' '}
            continue
            ;;
        '' | '#'*) continue ;;
    esac
    [ -n "$cmd" ] || continue
    # Тегов могло стать меньше, чем было: без зажима такое окно уехало бы в
    # никуда, а с ним -- и всё, что мы собирались на нём открыть.
    [ "$desk" -ge 0 ] 2>/dev/null || desk=0
    [ "$desk" -lt "$ndesk" ] 2>/dev/null || desk=$((ndesk - 1))
    group[$desk]+="$cwd	$class	$cmd"$'\n'
done <"$SESSION_FILE"

[ ${#group[@]} -gt 0 ] || exit 0

# pid запущенного процесса -> тег, на котором его окно должно оказаться.
declare -A want=()

for desk in $(printf '%s\n' "${!group[@]}" | sort -n); do
    xdotool set_desktop "$desk" 2>/dev/null
    before=$(client_count)
    launched=0

    while IFS=$'\t' read -r cwd class cmd; do
        [ -n "$cmd" ] || continue
        IFS="$SESSION_SEP" read -r -a argv <<<"$cmd"
        [ ${#argv[@]} -gt 0 ] || continue
        command -v "${argv[0]}" >/dev/null 2>&1 || [ -x "${argv[0]}" ] || continue

        (
            cd "$cwd" 2>/dev/null || cd "$HOME" || exit
            exec "${argv[@]}" >/dev/null 2>&1
        ) &
        want[$!]=$desk
        launched=$((launched + 1))
    done <<<"${group[$desk]}"

    # Ждём, пока окна группы появятся: если уйти на следующий тег раньше, они
    # откроются уже на нём. Таймаут -- на приложения, которые стартуют дольше
    # всякого разумного ожидания; их подберёт догоняющая правка ниже.
    deadline=$((SECONDS + GROUP_TIMEOUT))
    while [ "$(client_count)" -lt $((before + launched)) ] && [ $SECONDS -lt $deadline ]; do
        sleep 0.25
    done
done

xdotool set_desktop "$home_desktop" 2>/dev/null

# Догоняющая правка: три прохода с паузами ловят тех, кто открыл окно уже после
# того, как мы ушли с его тега. Окна ищем по pid, поэтому приложение, которое
# форкается и теряет наш pid, останется там, где открылось -- поправить его
# нечем, и это лучше, чем утащить чужое окно по совпадению класса.
for _ in 1 2 3; do
    sleep 3
    for pid in "${!want[@]}"; do
        for win in $(xdotool search --pid "$pid" 2>/dev/null); do
            session_read_window "$win" || continue
            [ -n "$win_class" ] || continue
            [ "${win_desktop:-}" = "${want[$pid]}" ] && continue
            xdotool set_desktop_for_window "$win" "${want[$pid]}" 2>/dev/null
        done
    done
done
