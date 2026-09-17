#!/usr/bin/env bash
# Включение гибернации (suspend-to-disk) на этой машине. Запускать под root:
#
#     sudo bash vxwm/hibernate-setup.sh                      -- swapfile на /
#     sudo bash vxwm/hibernate-setup.sh /mnt/mydata/swapfile -- на другом диске
#
# Второй вид заодно переносит: старый swapfile отключается, удаляется и
# вычищается из fstab, а resume= и resume_offset пересчитываются под новый
# раздел. Смысл переноса -- освободить место на системном диске: образ памяти
# занимает столько же, сколько вся RAM.
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

SWAPFILE=${1:-/swapfile}
case $SWAPFILE in
    /*) ;;
    *) echo "путь к swapfile должен быть абсолютным: $SWAPFILE" >&2; exit 1 ;;
esac

SWAPDIR=$(dirname "$SWAPFILE")
[ -d "$SWAPDIR" ] || { echo "нет каталога $SWAPDIR" >&2; exit 1; }

step() { printf '\n== %s\n' "$*"; }

# Файловая система, на которой будет лежать образ. btrfs требует отдельного
# обращения (nodatacow, своя команда для offset), и молча делать вид, что
# сработает, нельзя -- гибернация просто не случилась бы.
SWAPFS=$(findmnt -no FSTYPE -T "$SWAPDIR")
case $SWAPFS in
    ext2|ext3|ext4|xfs) ;;
    btrfs) echo "на btrfs нужен свой путь (nodatacow, btrfs inspect-internal), здесь не поддержан" >&2; exit 1 ;;
    *) echo "неизвестная ФС $SWAPFS под $SWAPDIR -- swapfile на ней может не заработать" >&2; exit 1 ;;
esac

# Раздел, на котором лежит swapfile, должен монтироваться при загрузке: иначе
# swapon в fstab окажется без своей ФС, и swap не включится. Само ядро образ
# при пробуждении читает мимо монтирования, по UUID раздела и смещению, так
# что resume от этого не страдает -- страдает обычный swap после загрузки.
SWAPMNT=$(findmnt -no TARGET -T "$SWAPDIR")
if [ "$SWAPMNT" != "/" ] && ! findmnt -sno TARGET "$SWAPMNT" >/dev/null 2>&1; then
    echo "внимание: $SWAPMNT не описан в /etc/fstab -- после перезагрузки swap не включится" >&2
fi

# --- swapfile ----------------------------------------------------------------
# Размер: вся RAM плюс гигабайт про запас. Меньше RAM брать нельзя -- образ
# просто не поместится, и гибернация молча не случится.
mem_kb=$(awk '/^MemTotal:/ {print $2}' /proc/meminfo)
want_gib=$(((mem_kb + 1048575) / 1048576 + 1))

if [ -f "$SWAPFILE" ]; then
    step "swapfile уже есть ($(du -h "$SWAPFILE" | cut -f1)), не трогаю"
else
    avail_gib=$(($(df --output=avail -BG "$SWAPDIR" | tail -1 | tr -dc 0-9)))
    left=$((avail_gib - want_gib))
    step "создаю $SWAPFILE на ${want_gib} GiB (на $SWAPMNT свободно ${avail_gib} GiB, останется ${left} GiB)"
    if [ "$left" -lt 10 ]; then
        echo "на $SWAPMNT останется меньше 10 GiB -- отказываюсь, освободи место" >&2
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

# --- старые swapfile ----------------------------------------------------------
# Перенос: всё, что было swapfile'ом раньше и лежит не там, где надо теперь,
# гасим и убираем. Разделы-swap не трогаем -- их заводили руками и не нам
# решать, что они лишние.
for old in $(swapon --show=NAME --noheadings); do
    [ "$old" = "$SWAPFILE" ] && continue
    [ -f "$old" ] || continue
    step "убираю прежний swapfile $old ($(du -h "$old" | cut -f1))"
    swapoff "$old"
    rm -f "$old"
done
# Строки в fstab чистим отдельно: файл могли уже удалить, а запись осталась.
awk -v keep="$SWAPFILE" '
    $3 == "swap" && $1 ~ /^\// && $1 != keep { print "  убрал из fstab: " $1 > "/dev/stderr"; next }
    { print }
' /etc/fstab >/etc/fstab.new && mv /etc/fstab.new /etc/fstab

grep -qE "^$SWAPFILE[[:space:]]" /etc/fstab \
    || printf '%s none swap defaults 0 0\n' "$SWAPFILE" >>/etc/fstab

# --- параметры ядра ----------------------------------------------------------
# UUID не корня, а того раздела, на котором лежит образ: при переносе на
# другой диск это разные вещи, и ядро ищет образ именно там.
swap_uuid=$(findmnt -no UUID -T "$SWAPFILE")
[ -n "$swap_uuid" ] || { echo "не смог определить UUID раздела под $SWAPFILE" >&2; exit 1; }
# Первый экстент файла -- то самое физическое смещение. Хвостовые ".." в выводе
# filefrag срезаем.
offset=$(filefrag -v "$SWAPFILE" | awk '$1 == "0:" {gsub(/\.\./, "", $4); print $4; exit}')
[ -n "$offset" ] || { echo "не смог получить resume_offset" >&2; exit 1; }

step "resume=UUID=$swap_uuid resume_offset=$offset ($SWAPFILE на $SWAPMNT)"

cmdline=$(grep '^GRUB_CMDLINE_LINUX_DEFAULT=' /etc/default/grub | cut -d'"' -f2)
# Старые resume*-параметры выкидываем: после пересоздания swapfile смещение
# другое, и оставшийся хвост увёл бы ядро не туда.
cmdline=$(printf '%s\n' "$cmdline" | sed -E 's/ *resume(_offset)?=[^ ]*//g')
# Карта давно amdgpu, а параметр остался от прежней nvidia и ничего не значит.
cmdline=$(printf '%s\n' "$cmdline" | sed -E 's/ *nvidia[^ ]*//g')
cmdline="$cmdline resume=UUID=$swap_uuid resume_offset=$offset"
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
