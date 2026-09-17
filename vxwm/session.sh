#!/bin/bash
# Единая точка входа в X-сессию: используется и как ~/.xinitrc (startx/xinit),
# и как ~/.local/bin/vxwm-session -- на него смотрит пункт vxwm в меню SDDM
# (/usr/share/xsessions/vxwm.desktop, его пишет install.sh).
#
# Обои, композитор, dunst и бар поднимает autostart.sh через встроенный
# AUTOSTART-модуль vxwm (см. config.def.h) -- здесь их дублировать не нужно.

RICE="${XDG_CONFIG_HOME:-$HOME/.config}/vxwm-rice"

# PATH до ~/.local/bin. Нужен именно здесь, а не только в ~/.zprofile: сессию
# из меню входа запускает SDDM, и профиль оболочки при этом не читается --
# vxwm не нашёл бы ни vxbar, ни скриптов, которые зовёт по имени.
case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) export PATH="$HOME/.local/bin:$PATH" ;;
esac

# Переменные окружения (Qt, GTK, курсор). SDDM читает ~/.xprofile сам, startx --
# нет, поэтому берём явно: одна сессия не должна отличаться от другой тем,
# откуда её запустили.
# shellcheck source=/dev/null
[ -f "$HOME/.xprofile" ] && . "$HOME/.xprofile"

# Раскладка -- из rice.toml ([input]), через session.env, который собирает
# apply.py. Пустая VXWM_KB_LAYOUT означает "не трогать": рис, поставленный
# поверх настроенной системы, не должен переучивать клавиатуру под себя.
# shellcheck source=/dev/null
[ -f "$RICE/session.env" ] && . "$RICE/session.env"
if [ -n "${VXWM_KB_LAYOUT:-}" ]; then
    # Пустой -option первым: setxkbmap накапливает опции, и без сброса
    # переключатель раскладки прибавился бы к тому, что уже стоит в системе,
    # а не заменил его.
    setxkbmap -layout "$VXWM_KB_LAYOUT" -option "" \
        ${VXWM_KB_OPTIONS:+-option "$VXWM_KB_OPTIONS"} &
fi

exec vxwm
