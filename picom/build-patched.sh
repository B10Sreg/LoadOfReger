#!/usr/bin/env bash
# Собирает picom (FT-Labs) с патчами из ./patches и кладёт бинарь в ~/.local/bin.
#
# Зачем не системный пакет: патч no-dwm-tag-heuristic снимает эвристику,
# рассчитанную на ванильный dwm, -- без неё vxwm листает теги по вертикали с
# анимацией в обе стороны (см. комментарий в самом патче). Пакет picom-ftlabs-git
# при обновлении затёр бы правку, а ~/.local/bin стоит в PATH раньше /usr/bin
# (~/.zprofile), поэтому сессия берёт отсюда. Системный пакет остаётся на месте
# нетронутым: удалить этот бинарь -- и всё вернётся к нему.
#
# После обновления picom в системе имеет смысл прогнать скрипт заново: он
# собирает ту же ревизию, что зафиксирована в REV.
set -euo pipefail

REV=df4c6a3d9b11e14ed7f3142540babea4c775ddb1   # r2236, 2024-02-17 -- ревизия пакета picom-ftlabs-git
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin"
SRC="$(mktemp -d)"
trap 'rm -rf "$SRC"' EXIT

# Из кеша yay, если он есть: там уже лежит нужная ревизия и не нужна сеть.
CACHE="$HOME/.cache/yay/picom-ftlabs-git/picom"
if [ -d "$CACHE" ]; then
	git clone -q "$CACHE" "$SRC/picom"
else
	git clone -q https://github.com/FT-Labs/picom "$SRC/picom"
fi

cd "$SRC/picom"
git checkout -q "$REV"
# Первый патч -- от мейнтейнера AUR, он есть и в системной сборке.
patch -s -d src -p1 < "$HERE/patches/fix_ewmh_fullscreen.patch"
patch -s -p1 < "$HERE/patches/no-dwm-tag-heuristic.patch"

meson setup --buildtype=release build --prefix=/usr -Dwith_docs=false >/dev/null
ninja -C build >/dev/null

mkdir -p "$BIN"
# Через временный файл: работающий picom держит свой бинарь открытым.
install -m 755 build/src/picom "$BIN/.picom.new"
mv "$BIN/.picom.new" "$BIN/picom"
echo "picom собран и установлен в $BIN/picom"

if pkill -x picom; then
	sleep 1
	setsid "$BIN/picom" -b --config "${XDG_CONFIG_HOME:-$HOME/.config}/picom/picom.conf" >/dev/null 2>&1
	echo "запущенный picom перезапущен с патчем"
fi
