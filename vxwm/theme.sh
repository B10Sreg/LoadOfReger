#!/bin/bash
# Переключатель графитовых тем vxwm: deep / silver / carbon
BASE="$HOME/.config/vxwm-rice"
THEMES=("deep" "silver" "carbon")
CURRENT=$(readlink -f "$BASE/themes/current" 2>/dev/null | xargs basename 2>/dev/null || echo "none")

CHOICE=$(printf "%s\n" "${THEMES[@]}" | rofi -dmenu \
    -p "🎨 Тема (сейчас: $CURRENT)" \
    -no-config -theme-str 'window{width:350px;border-radius:10px;} element{padding:10px;}')

[ -z "$CHOICE" ] && exit 0
[ ! -d "$BASE/themes/$CHOICE" ] && { notify-send "❌ Ошибка" "Тема '$CHOICE' не найдена"; exit 1; }

notify-send "🔄 Применяем тему" "$CHOICE..."

ln -sfn "$BASE/themes/$CHOICE" "$BASE/themes/current"

mkdir -p "$HOME/.config/picom" "$HOME/.config/rofi" "$HOME/.config/kitty"
cp -f "$BASE/themes/$CHOICE/picom.conf" "$HOME/.config/picom/picom.conf"
cp -f "$BASE/themes/$CHOICE/rofi.rasi"  "$HOME/.config/rofi/config.rasi"
cp -f "$BASE/themes/$CHOICE/kitty.conf" "$HOME/.config/kitty/kitty.conf"

if [ -f "$BASE/themes/$CHOICE/config.h" ]; then
    cp -f "$BASE/themes/$CHOICE/config.h" "$HOME/vxwm/config.h"
    notify-send "🔨 Пересборка VXWM..." "Введите пароль, если запросит"
    (cd "$HOME/vxwm" && make clean && make && sudo make install)
fi

pkill -x picom 2>/dev/null; sleep 0.4
picom --config "$HOME/.config/picom/picom.conf" -b &

killall -SIGUSR1 kitty 2>/dev/null

pkill -x dunst 2>/dev/null; sleep 0.2
dunst &

pkill -f vxwm-statusbar.sh 2>/dev/null
"$HOME/.local/bin/vxwm-statusbar.sh" &

[ -f "$BASE/themes/$CHOICE/wallpaper.png" ] && feh --bg-fill "$BASE/themes/$CHOICE/wallpaper.png"

notify-send "✅ Готово" "Тема '$CHOICE' применена. Перезайдите (Alt+F4), если бар не обновил цвета."
