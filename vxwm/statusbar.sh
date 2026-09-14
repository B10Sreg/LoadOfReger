#!/bin/bash
# Статус-бар vxwm через xsetroot: CPU + RAM + громкость + дата/время.
# Цвет текста бара берётся из активной темы (SchemeNorm), отдельного стилинга не требует.

ICON_CPU=$''
ICON_RAM=$''
ICON_VOL_MUTE=$''
ICON_VOL_LOW=$''
ICON_VOL_HIGH=$''
SEP="  "

read -r _ prev_user prev_nice prev_sys prev_idle prev_iowait prev_irq prev_softirq _ < /proc/stat

while true; do
    # CPU% -- дельта загрузки с прошлой итерации (5с назад)
    read -r _ user nice sys idle iowait irq softirq _ < /proc/stat
    prev_idle_all=$((prev_idle + prev_iowait))
    idle_all=$((idle + iowait))
    prev_total=$((prev_user + prev_nice + prev_sys + prev_idle_all + prev_irq + prev_softirq))
    total=$((user + nice + sys + idle_all + irq + softirq))
    diff_total=$((total - prev_total))
    diff_idle=$((idle_all - prev_idle_all))
    if [ "$diff_total" -gt 0 ]; then
        CPU_PCT=$(( (100 * (diff_total - diff_idle)) / diff_total ))
    else
        CPU_PCT=0
    fi
    prev_user=$user; prev_nice=$nice; prev_sys=$sys; prev_idle=$idle
    prev_iowait=$iowait; prev_irq=$irq; prev_softirq=$softirq

    # RAM: занято/всего в GiB
    read -r MEM_TOTAL_KB MEM_AVAIL_KB < <(awk '/^MemTotal:/{t=$2} /^MemAvailable:/{a=$2} END{print t, a}' /proc/meminfo)
    MEM_USED_GB=$(awk -v t="$MEM_TOTAL_KB" -v a="$MEM_AVAIL_KB" 'BEGIN{printf "%.1f", (t-a)/1024/1024}')
    MEM_TOTAL_GB=$(awk -v t="$MEM_TOTAL_KB" 'BEGIN{printf "%.0f", t/1024/1024}')

    # Громкость: иконка меняется по уровню/mute (раньше у mute стоял не тот
    # символ -- иероглиф вместо иконки Nerd Font, из-за чего бар "ломался")
    VOL_RAW=$(wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null)
    if [[ "$VOL_RAW" == *MUTED* ]]; then
        VOL="${ICON_VOL_MUTE} mute"
    else
        VOL_PCT=$(awk '{printf "%d", $2*100}' <<< "$VOL_RAW")
        VOL_PCT=${VOL_PCT:-0}
        if [ "$VOL_PCT" -ge 34 ]; then VICON="$ICON_VOL_HIGH"; else VICON="$ICON_VOL_LOW"; fi
        VOL="${VICON} ${VOL_PCT}%"
    fi

    NOW=$(date '+%a %d %b  %H:%M')
    xsetroot -name "${ICON_CPU} ${CPU_PCT}%${SEP}${ICON_RAM} ${MEM_USED_GB}/${MEM_TOTAL_GB}G${SEP}${VOL}${SEP}${NOW} "
    sleep 5
done
