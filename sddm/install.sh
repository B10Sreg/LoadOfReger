#!/bin/bash
# Устанавливает тему graphite для SDDM и включает сам SDDM (без старта прямо сейчас).
set -euo pipefail
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/graphite" && pwd)"

sudo mkdir -p /usr/share/sddm/themes/graphite
sudo cp "$SRC"/Main.qml "$SRC"/metadata.desktop "$SRC"/theme.conf "$SRC"/background.png "$SRC"/chevron.svg /usr/share/sddm/themes/graphite/
sudo chmod 644 /usr/share/sddm/themes/graphite/*

sudo mkdir -p /etc/sddm.conf.d
printf '[Theme]\nCurrent=graphite\n' | sudo tee /etc/sddm.conf.d/10-theme.conf >/dev/null

sudo systemctl enable sddm.service

echo "Готово. SDDM включён и стартует со следующей загрузки (сейчас Xorg не трогаем)."
echo "При первом входе выбери сессию 'vxwm' в выпадающем списке — дальше он запомнится."
