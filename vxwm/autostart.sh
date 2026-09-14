#!/usr/bin/env bash

# Даем X-серверу долю секунды на инициализацию
sleep 0.5

RICE="$HOME/.config/vxwm-rice"

# 1. Обои: берём из активной темы, если она выбрана, иначе — дефолтный файл
WALL="$RICE/themes/current/wallpaper.png"
[ -f "$WALL" ] || WALL="$HOME/Downloads/wallpaper.png"
feh --bg-fill "$WALL" &

# 2. Перезапускаем композитор с конфигом активной темы
pkill -x picom
sleep 0.3
picom -b --config "$HOME/.config/picom/picom.conf" &

# 3. Уведомления
pkill -x dunst
dunst &

# 4. Бар. vxbar рисует всё сам (теги, заголовок, cpu/ram/громкость/часы) и
# резервирует место через _NET_WM_STRUT_PARTIAL, поэтому встроенный бар vxwm
# выключен (showbar = 0 в config.h). Старый statusbar.sh больше не нужен:
# он писал строку в имя root-окна, а vxbar читает свойства напрямую.
pkill -f vxwm-statusbar.sh
pkill -x vxbar
"$HOME/.local/bin/vxbar" &
