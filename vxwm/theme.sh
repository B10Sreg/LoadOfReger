#!/bin/bash
# Переключатель графитовых тем vxwm: deep / silver / carbon
#
# Тема -- это шесть цветов в themes/<имя>/colors.Xresources плюс готовые
# конфиги picom/rofi/kitty и обои. Раньше в теме лежал ещё и config.h, и смена
# темы означала make + sudo make install + перезапуск WM. Теперь цвета едут
# через X resources: xrdb меняет RESOURCE_MANAGER на root-окне, vxwm ловит это
# обычным PropertyNotify и перекрашивается на лету. Ни пересборки, ни пароля.
#
# Бар перекрашивается из той же палитры: его цвета выводятся из тех же шести,
# правятся в ~/.config/vxbar/config.toml и подхватываются по SIGUSR1.

BASE="$HOME/.config/vxwm-rice"
THEMES=("deep" "silver" "carbon")
CURRENT=$(readlink -f "$BASE/themes/current" 2>/dev/null | xargs basename 2>/dev/null || echo "none")

# Меню рисуем текущей темой риса (rofi/config.rasi её подменяет при
# переключении), сверху накидывая только геометрию списка -- так выбор темы
# выглядит как остальной рис, а не как дефолтный rofi.
menu() {
    local t
    for t in "${THEMES[@]}"; do
        if [ "$t" = "$CURRENT" ]; then printf '%s  ●\n' "$t"; else printf '%s\n' "$t"; fi
    done
}

CHOICE=$(menu | rofi -dmenu -i -no-show-icons \
    -p "Тема" \
    -mesg "Сейчас: $CURRENT" \
    -theme-str 'window{width:340px;} mainbox{children:[inputbar,message,listview];} listview{lines:3;} textbox{text-color:@fg-base;padding:2px 4px;}')
CHOICE=${CHOICE%%  ●}

[ -z "$CHOICE" ] && exit 0
[ ! -d "$BASE/themes/$CHOICE" ] && { notify-send "❌ Ошибка" "Тема '$CHOICE' не найдена"; exit 1; }

THEME_DIR="$BASE/themes/$CHOICE"
ln -sfn "$THEME_DIR" "$BASE/themes/current"

mkdir -p "$HOME/.config/picom" "$HOME/.config/rofi" "$HOME/.config/kitty"
cp -f "$THEME_DIR/picom.conf" "$HOME/.config/picom/picom.conf"
cp -f "$THEME_DIR/rofi.rasi"  "$HOME/.config/rofi/config.rasi"
cp -f "$THEME_DIR/kitty.conf" "$HOME/.config/kitty/kitty.conf"

# Цвета WM. Merge, а не load: в Xresources могут лежать настройки других
# программ, и load стёр бы их заодно с темой.
xrdb -merge "$THEME_DIR/colors.Xresources"

# Цвета бара выводим из той же шестёрки, чтобы палитра была одна на рис и не
# разъезжалась при правке темы.
python3 - "$THEME_DIR/colors.Xresources" "$HOME/.config/vxbar/config.toml" <<'PY'
import re, sys

src, dst = sys.argv[1], sys.argv[2]

col = {}
for line in open(src):
    m = re.match(r'vxwm\.(\w+):\s*(#[0-9a-fA-F]{6})', line.strip())
    if m:
        col[m.group(1)] = m.group(2)
if len(col) < 6:
    sys.exit(f"в {src} не хватает цветов, бар не трогаю")

# Роли бара -- те же роли, что у окон: фон бара = фон неактивного окна,
# акцент = цвет рамки активного. Так бар и рамки читаются как одна система.
want = {
    'style': {
        'background': col['normbg'],
        'foreground': col['normfg'],
        'accent':     col['selborder'],
        'muted':      col['normborder'],
    },
    'tags': {
        'active_bg':   col['selbg'],
        'active_fg':   col['selfg'],
        'occupied_fg': col['normfg'],
        'empty_fg':    col['normborder'],
    },
}

try:
    text = open(dst).read()
except FileNotFoundError:
    # Конфига ещё нет -- vxbar создаст его сам при следующем старте.
    sys.exit(0)

out, section = [], None
# Разбираем построчно без концов строк и возвращаем перевод сами: иначе
# пересобранная строка теряет \n и весь файл слипается в одну.
for raw in text.splitlines():
    line = raw
    m = re.match(r'\s*\[(\w+)\]', line)
    if m:
        section = m.group(1)
    elif section in want:
        # Правим только значение, сохраняя отступы, кавычки и комментарии:
        # config.toml пишет сам vxbar, и его форматирование лучше не ломать.
        km = re.match(r"(\s*)(\w+)(\s*=\s*)(['\"])([^'\"]*)(['\"])(.*)", line)
        if km and km.group(2) in want[section]:
            line = "{}{}{}{}{}{}{}".format(
                km.group(1), km.group(2), km.group(3), km.group(4),
                want[section][km.group(2)], km.group(6), km.group(7))
    out.append(line)

open(dst, 'w').write('\n'.join(out) + '\n')
PY

# SIGUSR1 -- бару перечитать конфиг. Перезапускать его незачем: он умеет сам.
pkill -USR1 -x vxbar

# Обои меняются файлом, а не цветом, поэтому их перевешиваем отдельно.
WALL="$THEME_DIR/wallpaper.png"
[ -f "$WALL" ] && feh --bg-fill "$WALL"

notify-send "🎨 Тема" "$CHOICE"
