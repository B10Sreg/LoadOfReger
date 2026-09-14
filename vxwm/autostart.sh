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

# 4. Статус-бар (громкость, дата/время)
pkill -f vxwm-statusbar.sh
"$HOME/.local/bin/vxwm-statusbar.sh" &
