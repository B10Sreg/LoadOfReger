#!/usr/bin/env bash
# Включение гибернации (suspend-to-disk) на этой машине. Запускать под root:
#
#     sudo bash vxwm/hibernate-setup.sh
#
# Что делает и зачем:
#   swapfile      -- образу памяти некуда ложиться, swap тут отсутствует вовсе;
#   resume=       -- ядру нужно сказать, откуда поднимать образ при загрузке;
#   resume_offset -- для файла (а не раздела) ядро адресует не файл, а его
#                    физическое смещение на диске: initramfs файловую систему
#                    ещё не смонтировал и имени файла не знает;
#   amdgpu        -- видеокарта здесь Radeon на amdgpu, ему для гибернации
#                    ничего настраивать не нужно. Заодно выкидываем хвосты от
#                    старой nvidia: модули, которых в системе нет, и параметр
#                    nvidia_drm.modeset=1 -- на них ругается mkinitcpio.
#
# Хук resume в mkinitcpio не нужен: initramfs здесь systemd-based (HOOKS=(base
# systemd ...)), а systemd поднимает образ сам, увидев resume= в cmdline.
#
# Скрипт идемпотентен: повторный запуск ничего не ломает и не переделывает.

set -euo pipefail

# Первый запуск оборвался молча где-то после swapon. Ловушка называет строку и
# команду, на которой всё встало, -- иначе причину видно только по тому, что
# именно не доделано в системе.
trap 'echo "ОШИБКА на строке $LINENO: $BASH_COMMAND (код $?)" >&2' ERR

[ "$(id -u)" = 0 ] || { echo "нужен root: sudo bash $0" >&2; exit 1; }

SWAPFILE=/swapfile

step() { printf '\n== %s\n' "$*"; }

# --- swapfile ----------------------------------------------------------------
# Размер: вся RAM плюс гигабайт про запас. Меньше RAM брать нельзя -- образ
# просто не поместится, и гибернация молча не случится.
mem_kb=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo)
want_gib=$(((mem_kb + 1048575) / 1048576 + 1))

if [ -f "$SWAPFILE" ]; then
    step "swapfile уже есть ($(du -h "$SWAPFILE" | cut -f1)), не трогаю"
else
    avail_gib=$(($(df --output=avail -BG / | tail -1 | tr -dc 0-9)))
    left=$((avail_gib - want_gib))
    step "создаю $SWAPFILE на ${want_gib} GiB (свободно ${avail_gib} GiB, останется ${left} GiB)"
    if [ "$left" -lt 10 ]; then
        echo "на / останется меньше 10 GiB -- отказываюсь, освободи место" >&2
        exit 1
    fi

    # mkswap --file (util-linux 2.40+) сам делает файл пригодным для swapon.
    # fallocate здесь не годится: на ext4 он оставляет unwritten extents, и
    # swapon такой файл отвергает -- поэтому запасной путь именно dd.
    if ! mkswap -U clear --size "${want_gib}G" --file "$SWAPFILE" 2>/dev/null; then
        echo "mkswap --file не поддержан, заполняю через dd (небыстро)"
        dd if=/dev/zero of="$SWAPFILE" bs=1M count=$((want_gib * 1024)) status=progress
        mkswap -U clear "$SWAPFILE"
    fi
    chmod 600 "$SWAPFILE"
fi

swapon --show | grep -q "^$SWAPFILE " || swapon "$SWAPFILE"

grep -qE "^$SWAPFILE[[:space:]]" /etc/fstab \
    || printf '%s none swap defaults 0 0\n' "$SWAPFILE" >>/etc/fstab

# --- параметры ядра ----------------------------------------------------------
root_uuid=$(findmnt -no UUID /)
# Первый экстент файла -- то самое физическое смещение. Хвостовые ".." в выводе
# filefrag срезаем.
offset=$(filefrag -v "$SWAPFILE" | awk '$1 == "0:" {gsub(/\.\./, "", $4); print $4; exit}')
[ -n "$offset" ] || { echo "не смог получить resume_offset" >&2; exit 1; }

step "resume=UUID=$root_uuid resume_offset=$offset"

cmdline=$(grep '^GRUB_CMDLINE_LINUX_DEFAULT=' /etc/default/grub | cut -d'"' -f2)
# Старые resume*-параметры выкидываем: после пересоздания swapfile смещение
# другое, и оставшийся хвост увёл бы ядро не туда.
cmdline=$(printf '%s\n' "$cmdline" | sed -E 's/ *resume(_offset)?=[^ ]*//g')
# Карта давно amdgpu, а параметр остался от прежней nvidia и ничего не значит.
cmdline=$(printf '%s\n' "$cmdline" | sed -E 's/ *nvidia[^ ]*//g')
cmdline="$cmdline resume=UUID=$root_uuid resume_offset=$offset"
cmdline=$(printf '%s\n' "$cmdline" | sed -E 's/^ +//; s/ +/ /g')

cp /etc/default/grub /etc/default/grub.bak.$(date +%Y%m%d%H%M%S)
sed -i "s|^GRUB_CMDLINE_LINUX_DEFAULT=.*|GRUB_CMDLINE_LINUX_DEFAULT=\"$cmdline\"|" /etc/default/grub

step "grub-mkconfig"
grub-mkconfig -o /boot/grub/grub.cfg

# --- mkinitcpio --------------------------------------------------------------
# В MODULES прописаны nvidia-модули, которых в системе нет: mkinitcpio на
# каждой сборке ругается, что не нашёл их. Меняем на amdgpu -- он и есть
# реальный драйвер, а в initramfs нужен, чтобы KMS поднялся до монтирования
# корня (хук kms в HOOKS уже стоит).
step "mkinitcpio: nvidia-модули -> amdgpu"
if grep -q '^MODULES=.*nvidia' /etc/mkinitcpio.conf; then
    cp /etc/mkinitcpio.conf /etc/mkinitcpio.conf.bak.$(date +%Y%m%d%H%M%S)
    sed -i 's/^MODULES=.*/MODULES=(amdgpu)/' /etc/mkinitcpio.conf
fi

step "mkinitcpio -P"
mkinitcpio -P

cat <<'DONE'

Готово. Осталась перезагрузка -- параметры ядра и initramfs подхватятся только
после неё. Проверка после перезагрузки:

    cat /proc/cmdline | tr ' ' '\n' | grep resume
    swapon --show
    systemctl hibernate

Если что-то пойдёт не так -- journalctl -b -1 (лог сеанса до гибернации) и
journalctl -b | grep -i hibernat.
DONE
