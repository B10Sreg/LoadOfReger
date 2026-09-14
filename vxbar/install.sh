#!/usr/bin/env bash
# Собирает vxbar в release и кладёт бинарь в ~/.local/bin, откуда его
# запускает autostart.sh. Работающий бар после установки перезапускается,
# иначе в памяти останется старая сборка.
set -euo pipefail
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin"

cargo build --release --manifest-path "$SRC/Cargo.toml"

mkdir -p "$BIN"
# Замена на месте ломает уже запущенный процесс ("text file busy"), поэтому
# ставим через временный файл и rename.
install -m 755 "$SRC/target/release/vxbar" "$BIN/.vxbar.new"
mv "$BIN/.vxbar.new" "$BIN/vxbar"

if pkill -x vxbar; then
	sleep 0.3
	# Без перенаправления бар держит унаследованные stdout/stderr открытыми,
	# и вызывающий скрипт (или пайп вроде "install.sh | tail") висит до его
	# завершения.
	"$BIN/vxbar" >/dev/null 2>&1 &
	disown
	echo "vxbar установлен в $BIN/vxbar и перезапущен."
else
	echo "vxbar установлен в $BIN/vxbar (запущенный экземпляр не найден)."
fi
