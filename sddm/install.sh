#!/usr/bin/env bash
# Ставит тему экрана входа в систему. Нужен root: греетер читает её из
# /usr/share/sddm/themes, куда обычному пользователю писать нечем.
#
#     sudo bash sddm/install.sh
#
# Цвета берутся из текущей темы риса (~/.config/vxwm-rice/themes/<тема>/
# sddm-theme.conf, собирается генератором apply.py), обои -- оттуда же.
# Поэтому после смены темы скрипт стоит прогнать заново: сам греетер про рис
# ничего не знает и живёт тем, что лежит рядом с ним.
set -euo pipefail

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/graphite"
DEST=/usr/share/sddm/themes/graphite

[ "$(id -u)" = 0 ] || { echo "нужен root: sudo bash $0" >&2; exit 1; }

# Домашний каталог того, кто запустил sudo: под root ~ -- это /root, а тема
# риса лежит у пользователя.
USER_HOME=$(getent passwd "${SUDO_USER:-$USER}" | cut -d: -f6)
RICE_HOME="${USER_HOME}/.config/vxwm-rice"

mkdir -p "$DEST"
install -m 644 "$SRC/Main.qml" "$SRC/metadata.desktop" "$SRC/chevron.svg" "$DEST/"

# theme.conf: из темы риса, если она собрана, иначе тот, что лежит в репозитории.
theme=$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$RICE_HOME/rice.toml" 2>/dev/null | head -1)
generated="$RICE_HOME/themes/$theme/sddm-theme.conf"
if [ -n "$theme" ] && [ -f "$generated" ]; then
	install -m 644 "$generated" "$DEST/theme.conf"
	echo "цвета: тема $theme"
else
	install -m 644 "$SRC/theme.conf" "$DEST/theme.conf"
	echo "цвета: из репозитория (темы риса не нашлось)"
fi

# Обои: те же, что на рабочем столе. Нет файла -- остаются прежние.
wallpaper="$RICE_HOME/themes/$theme/wallpaper.png"
if [ -f "$wallpaper" ]; then
	install -m 644 "$wallpaper" "$DEST/background.png"
	echo "обои: $wallpaper"
elif [ -f "$SRC/background.png" ]; then
	install -m 644 "$SRC/background.png" "$DEST/background.png"
fi

# Тема должна быть выбрана в конфиге sddm, иначе всё это никто не покажет.
mkdir -p /etc/sddm.conf.d
if ! grep -rqs '^Current=graphite' /etc/sddm.conf.d /etc/sddm.conf; then
	printf '[Theme]\nCurrent=graphite\n' >/etc/sddm.conf.d/10-theme.conf
	echo "тема выбрана в /etc/sddm.conf.d/10-theme.conf"
fi

# Сам SDDM как менеджер входа. Было в прежней версии скрипта и осталось:
# на чистой установке тему поставить есть куда, а показывать её некому.
# Запущенный сеанс не трогаем -- включается со следующей загрузки.
if ! systemctl is-enabled --quiet sddm.service 2>/dev/null; then
	systemctl enable sddm.service
	echo "sddm включён, стартует со следующей загрузки"
fi

echo "готово. Посмотреть, не выходя из сессии: sddm-greeter --test-mode --theme $DEST"
