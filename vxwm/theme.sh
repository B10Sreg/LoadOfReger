#!/bin/bash
# Выбор темы риса через rofi.
#
# Вся работа -- в rice/apply.py: он пишет тему в rice.toml и разворачивает её
# во все конфиги. Здесь только меню, поэтому тему можно сменить и без него:
#
#     rice/apply.py --set-theme abyss
#
# Меню рисуем текущей темой риса (rofi/config.rasi её подменяет при
# переключении), сверху накидывая только геометрию списка -- так выбор темы
# выглядит как остальной рис, а не как дефолтный rofi.

RICE_DIR=$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/rice
BASE="$HOME/.config/vxwm-rice"

# Список тем -- это список палитр: тема без палитры не соберётся, а каталог в
# themes/ может остаться от удалённой.
mapfile -t THEMES < <(find "$RICE_DIR/palettes" -name '*.toml' -printf '%f\n' \
    | sed 's/\.toml$//' | sort)
[ ${#THEMES[@]} -gt 0 ] || { notify-send "❌ Тема" "В $RICE_DIR/palettes пусто"; exit 1; }

CURRENT=$(sed -n '/^\[theme\]/,/^\[/p' "$BASE/rice.toml" 2>/dev/null \
    | sed -n 's/^name *= *"\(.*\)"/\1/p' | head -1)
CURRENT=${CURRENT:-none}

menu() {
    local t
    for t in "${THEMES[@]}"; do
        if [ "$t" = "$CURRENT" ]; then printf '%s  ●\n' "$t"; else printf '%s\n' "$t"; fi
    done
}

CHOICE=$(menu | rofi -dmenu -i -no-show-icons \
    -p "Тема" \
    -mesg "Сейчас: $CURRENT" \
    -theme-str "window{width:340px;} mainbox{children:[inputbar,message,listview];} listview{lines:${#THEMES[@]};} textbox{text-color:@fg-base;padding:2px 4px;}")
CHOICE=${CHOICE%%  ●}

[ -z "$CHOICE" ] && exit 0

if out=$("$RICE_DIR/apply.py" --set-theme "$CHOICE" 2>&1); then
    notify-send "🎨 Тема" "$CHOICE"
else
    notify-send -u critical "❌ Тема" "$CHOICE: $(printf '%s' "$out" | tail -1)"
    exit 1
fi
