#!/bin/bash
# Единая точка входа в X-сессию: используется и как ~/.xinitrc (startx/xinit),
# и как ~/.local/bin/vxwm-session (если когда-нибудь вернёшь SDDM).
# Обои/композитор/dunst/статус-бар поднимает autostart.sh через встроенный
# AUTOSTART-модуль vxwm (см. config.h) — здесь их дублировать не нужно.

# Раскладка. CapsLock, а не Alt+Shift — последний конфликтует с хоткеями vxwm (Mod+Shift+*).
setxkbmap -layout us,ru -option grp:caps_toggle &

exec vxwm
