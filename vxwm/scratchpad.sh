#!/usr/bin/env bash
# Выпадающий терминал: одно плавающее окно kitty, которое прячется и
# возвращается одной клавишей.
#
# Прячем не размапливанием окна, а отправкой на запасной тег: dwm-подобные WM
# на UnmapNotify снимают окно с управления, и вернуть его потом нечем. Перенос
# по _NET_WM_DESKTOP окно не теряет, а vxwm его понимает -- модуль ewmh_tags.
#
# Правило на класс scratchpad (плавающее, по центру) лежит в config.h.

set -uo pipefail

CLASS=scratchpad
# Последний тег держим как «карман»: окно на нём есть, но его не видно.
POCKET=$(($(xprop -root -notype _NET_NUMBER_OF_DESKTOPS 2>/dev/null | awk '{print $NF}') - 1))
[ "$POCKET" -ge 0 ] 2>/dev/null || POCKET=8

win=$(xdotool search --class "$CLASS" 2>/dev/null | head -1)

if [ -z "$win" ]; then
    # Первый вызов -- окна ещё нет. Правило в config.h само сделает его
    # плавающим, а появится оно на текущем теге, то есть сразу видимым.
    exec kitty --class "$CLASS" --config "$HOME/.config/kitty/kitty.conf"
fi

cur=$(xprop -root -notype _NET_CURRENT_DESKTOP 2>/dev/null | awk '{print $NF}')
desk=$(xprop -id "$win" -notype _NET_WM_DESKTOP 2>/dev/null | awk '{print $NF}')

if [ "$desk" = "$cur" ]; then
    xdotool set_desktop_for_window "$win" "$POCKET"
else
    xdotool set_desktop_for_window "$win" "$cur"
    xdotool windowactivate "$win"
fi
