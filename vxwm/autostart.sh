#!/usr/bin/env bash

# Даем X-серверу долю секунды на инициализацию
sleep 0.5

RICE="$HOME/.config/vxwm-rice"
VXWM_DIR=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")

# Настройки сессии из единого конфига: интервал снимка, блокировка по простою.
# Файл генерирует apply.py; без него работают значения по умолчанию.
# shellcheck source=/dev/null
[ -f "$RICE/session.env" ] && . "$RICE/session.env"

# 1. Тема, окна, композитор, уведомления и обои -- одним вызовом. apply.py
# разворачивает rice.toml во все конфиги и сам поднимает picom и dunst; до
# него то же самое делали четыре куска этого файла, каждый со своей копией
# путей и своим представлением о том, какая тема сейчас активна.
"$VXWM_DIR/rice/apply.py" || echo "vxwm: apply.py не отработал" >&2

# 2. Бар. vxbar рисует всё сам (теги, заголовок, cpu/ram/громкость/часы) и
# резервирует место через _NET_WM_STRUT_PARTIAL, поэтому встроенный бар vxwm
# выключен (showbar = 0 в config.h). Старый statusbar.sh больше не нужен:
# он писал строку в имя root-окна, а vxbar читает свойства напрямую.
pkill -f vxwm-statusbar.sh
pkill -x vxbar
"$HOME/.local/bin/vxbar" &

# 3. Блокировка экрана. xss-lock связывает две вещи: таймаут X-скринсейвера
# (блокировка по простою) и сигнал засыпания от logind -- он придерживает
# систему инхибитором, пока slock не встал, так что после пробуждения
# рабочий стол не мелькает. Без пакета просто пропускаем: остальной рис от
# этого не страдает, а ручная блокировка есть в power.sh.
if [ "${VXWM_LOCK_ON_IDLE:-1}" = 1 ] && command -v xss-lock >/dev/null 2>&1; then
    pkill -x xss-lock
    # На этом же событии xss-lock поднимает slock. Время -- из rice.toml.
    xset s "${VXWM_IDLE_SECONDS:-600}" "${VXWM_IDLE_SECONDS:-600}"
    xss-lock -l -- slock &
elif [ "${VXWM_LOCK_ON_IDLE:-1}" = 1 ]; then
    echo "vxwm: xss-lock не установлен, блокировка по простою отключена" >&2
fi

# 4. Агент авторизации polkit. Без него графические программы, которым нужен
# пароль root -- монтирование диска в Thunar, gparted, установка принтера, --
# молча ничего не делают: спросить пароль им некому. Демон лёгкий и нужен
# ровно на время такого запроса.
# Только настоящие агенты: polkit-agent-helper-1 из /usr/lib/polkit-1 сюда не
# годится -- это setuid-помощник, которого агент зовёт сам.
for agent in /usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1 \
             /usr/lib/polkit-kde-authentication-agent-1; do
    [ -x "$agent" ] || continue
    pgrep -f "$agent" >/dev/null 2>&1 || "$agent" &
    break
done

# 5. Сессия: возвращаем окна, открытые в прошлый раз, и дальше держим снимок
# свежим. Обе задачи в одном фоне и строго по очереди -- демон, стартовавший
# посреди восстановления, записал бы полупустой список.
# Выключается файлом ~/.config/vxwm-rice/no-session-restore.
pkill -f 'session-save\.sh --watch'
(
    "$VXWM_DIR/session-restore.sh"
    exec "$VXWM_DIR/session-save.sh" --watch
) &
