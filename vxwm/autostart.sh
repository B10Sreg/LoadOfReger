#!/usr/bin/env bash

# Даем X-серверу долю секунды на инициализацию
sleep 0.5

RICE="$HOME/.config/vxwm-rice"

# 0. Цвета активной темы в X resources. vxwm читает их оттуда, а не из
# config.h, поэтому merge здесь -- это и есть применение темы к окнам.
# Делаем до всего остального: rofi и прочие тоже смотрят в Xresources.
[ -f "$RICE/themes/current/colors.Xresources" ] \
    && xrdb -merge "$RICE/themes/current/colors.Xresources"

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

# 5. Блокировка экрана. xss-lock связывает две вещи: таймаут X-скринсейвера
# (блокировка по простою) и сигнал засыпания от logind -- он придерживает
# систему инхибитором, пока slock не встал, так что после пробуждения
# рабочий стол не мелькает. Без пакета просто пропускаем: остальной рис от
# этого не страдает, а ручная блокировка есть в power.sh.
if command -v xss-lock >/dev/null 2>&1; then
    pkill -x xss-lock
    # 10 минут до гашения -- на этом же событии xss-lock поднимает slock.
    xset s 600 600
    xss-lock -l -- slock &
else
    echo "vxwm: xss-lock не установлен, блокировка по простою отключена" >&2
fi

# 6. Сессия: возвращаем окна, открытые в прошлый раз, и дальше держим снимок
# свежим. Обе задачи в одном фоне и строго по очереди -- демон, стартовавший
# посреди восстановления, записал бы полупустой список.
# Выключается файлом ~/.config/vxwm-rice/no-session-restore.
VXWM_DIR=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")
pkill -f 'session-save\.sh --watch'
(
    "$VXWM_DIR/session-restore.sh"
    exec "$VXWM_DIR/session-save.sh" --watch
) &
