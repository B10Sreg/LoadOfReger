#!/usr/bin/env bash
# Меню выхода из сессии на rofi. Сама работа делается в power.sh -- здесь
# только выбор; так гибернацию можно повесить и на хоткей напрямую, минуя меню.
#
# Оформление берётся из темы риса (rofi/config.rasi, её подменяет theme.sh),
# сверху -- только геометрия списка. Ровно тот же приём, что в theme.sh: меню
# выглядит частью риса, а не дефолтным rofi.

set -uo pipefail

VXWM_DIR=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")

# Иконка, подпись, действие. Гибернация первой -- она основной путь: только
# она возвращает вкладки и несохранённые буферы.
ITEMS=(
    "󰤄  Гибернация|hibernate"
    "  Заблокировать|lock"
    "  Перезагрузить|reboot"
    "⏻  Выключить|poweroff"
)

# Предупреждение в шапке, если гибернация недоступна: лучше увидеть причину
# здесь, чем выбрать её и получить уведомление об отказе.
if [ -z "$(swapon --show --noheadings 2>/dev/null)" ]; then
    MESG="Гибернация недоступна: нет swap"
else
    MESG="Гибернация вернёт сессию целиком"
fi

CHOICE=$(printf '%s\n' "${ITEMS[@]}" | cut -d'|' -f1 \
    | rofi -dmenu -i -no-show-icons \
        -p "Выход" \
        -mesg "$MESG" \
        -theme-str 'window{width:320px;} mainbox{children:[inputbar,message,listview];} listview{lines:4;} textbox{text-color:@fg-base;padding:2px 4px;}')

[ -n "$CHOICE" ] || exit 0

for item in "${ITEMS[@]}"; do
    if [ "${item%%|*}" = "$CHOICE" ]; then
        exec "$VXWM_DIR/power.sh" "${item##*|}"
    fi
done
