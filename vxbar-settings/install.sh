#!/usr/bin/env bash
# Собирает приложение настроек и регистрирует его в меню окружения.
set -euo pipefail
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"

cd "$SRC"
go build -o "$SRC/vxbar-settings" .

mkdir -p "$BIN" "$APPS"
install -m 755 "$SRC/vxbar-settings" "$BIN/.vxbar-settings.new"
mv "$BIN/.vxbar-settings.new" "$BIN/vxbar-settings"
install -m 644 "$SRC/vxbar-settings.desktop" "$APPS/vxbar-settings.desktop"

# Без обновления кеша rofi -show drun не увидит новый пункт до перелогина.
if command -v update-desktop-database >/dev/null 2>&1; then
	update-desktop-database "$APPS"
fi

echo "vxbar-settings установлен в $BIN/vxbar-settings, пункт меню — $APPS/vxbar-settings.desktop"
echo "Убедись, что $BIN есть в PATH (zsh/zprofile)."
