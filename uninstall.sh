#!/usr/bin/env bash
# Откат установки: снимает то, что поставил install.sh.
#
#     ./uninstall.sh            -- убрать бинари, ссылки и файлы входа
#     ./uninstall.sh --purge    -- заодно снести настройки (~/.config/vxwm-rice
#                                  и сгенерированные конфиги kitty/rofi/dunst)
#
# Пакеты pacman не трогаем: их ставили в систему, и что из них нужно дальше --
# решать не установщику риса. Список, который он ставил: ./install.sh --list-packages
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin"
CFG="${XDG_CONFIG_HOME:-$HOME/.config}"
PURGE=0
[ "${1:-}" = "--purge" ] && PURGE=1

say() { printf '  %s\n' "$*"; }

# Сначала гасим то, что работает прямо сейчас: иначе удалённый бар останется
# в памяти до конца сессии, а сторож поднимет picom обратно.
pkill -f picom-keeper.sh 2>/dev/null || true
pkill -x vxbar 2>/dev/null || true
pkill -f 'session-save\.sh --watch' 2>/dev/null || true

for f in vxwm-theme.sh vxwm-powermenu.sh vxwm-power.sh vxwm-scratchpad.sh \
         vxwm-session vxwm-rice-apply vxwm-statusbar.sh vxbar vxbar-settings picom; do
    [ -e "$BIN/$f" ] || [ -L "$BIN/$f" ] || continue
    rm -f "$BIN/$f"
    say "удалён $BIN/$f"
done

rm -f "$CFG/vxwm/autostart.sh"
rmdir "$CFG/vxwm" 2>/dev/null || true
rm -f "$HOME/.local/share/applications/vxbar-settings.desktop"

# Файлы входа снимаем, только если это наши ссылки: чужой ~/.xinitrc,
# написанный до риса, удалять нельзя.
for link in "$HOME/.xinitrc" "$HOME/.xprofile"; do
    if [ -L "$link" ] && case "$(readlink -f "$link")" in "$REPO"/*) true ;; *) false ;; esac; then
        rm -f "$link"
        say "удалена ссылка $link"
    fi
done

# Блок в профиле оболочки -- между маркерами, остальное не наше. Перебираем
# все три файла: install.sh писал в тот, который читает логин-оболочка, а с тех
# пор её могли и сменить.
for profile in "$HOME/.zprofile" "$HOME/.bash_profile" "$HOME/.profile"; do
    [ -f "$profile" ] && grep -qF '# >>> vxwm-rice >>>' "$profile" || continue
    python3 - "$profile" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
text = p.read_text()
head, _, rest = text.partition('# >>> vxwm-rice >>>')
_, _, tail = rest.partition('# <<< vxwm-rice <<<')
p.write_text((head.rstrip('\n') + '\n' + tail.lstrip('\n')).lstrip('\n'))
PY
    say "блок vxwm-rice убран из $profile"
done

if [ -d "$REPO/vxwm-src" ]; then
    sudo make -C "$REPO/vxwm-src" uninstall >/dev/null 2>&1 \
        && say "vxwm удалён из /usr/local/bin" \
        || say "vxwm из /usr/local/bin убрать не вышло (нужен sudo?)"
fi
sudo rm -f /usr/share/xsessions/vxwm.desktop 2>/dev/null || true

if [ "$PURGE" = 1 ]; then
    rm -rf "$CFG/vxwm-rice" "$CFG/vxbar"
    rm -f "$CFG/rofi/config.rasi" "$CFG/kitty/kitty.conf" "$CFG/dunst/dunstrc" "$CFG/picom/picom.conf"
    say "настройки риса удалены (--purge)"
    say "блок vxwm-rice в gtk-3.0/gtk.css и gtk-4.0/gtk.css убери руками, если он мешает"
fi

printf '\nГотово. Тему SDDM (/usr/share/sddm/themes/graphite) и пакеты оставил на месте.\n'
