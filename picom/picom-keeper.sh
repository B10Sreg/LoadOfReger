#!/usr/bin/env bash
# Сторож композитора: держит picom поднятым и говорит, когда тот падает.
#
#     picom-keeper.sh [путь к конфигу]
#
# Зачем. Эта сборка picom (FT-Labs) иногда умирает с SIGSEGV в paint_all_new
# -- и до патча, и после него, падения одинаковые:
#
#     coredumpctl list | grep picom
#
# На экране это выглядит так, что разом пропали анимации, тени и прозрачность,
# и причину видно только в coredumpctl -- догадаться, что упал именно
# композитор, неоткуда. Сторож поднимает его обратно за секунду и показывает
# уведомление, чтобы падение не проходило незамеченным.
#
# Запускается из apply.py вместо самого picom. Чтобы попросить его уйти,
# достаточно погасить сторожа (pkill -f picom-keeper.sh) -- уходя, он уносит
# с собой и picom.
set -u

CONFIG=${1:-$HOME/.config/picom/picom.conf}
PICOM=$(command -v picom) || { echo "picom-keeper: picom не найден" >&2; exit 1; }

# Порог: столько падений подряд за окно времени -- значит дело не в случайной
# ошибке, а в конфиге или драйвере, и крутить цикл бесполезно. Лучше остаться
# без композитора, чем сжечь батарею на бесконечных перезапусках.
MAX_CRASHES=5
WINDOW=60

notify() {
    command -v notify-send >/dev/null 2>&1 && notify-send -u normal "Композитор" "$1"
    echo "picom-keeper: $1" >&2
}

# Уходя -- уносим picom: иначе он останется сиротой, а следующий сторож
# поднимет второй, и они подерутся за композитный оверлей.
child=0
cleanup() { [ "$child" != 0 ] && kill "$child" 2>/dev/null; exit 0; }
trap cleanup TERM INT

count=0
window_start=$(date +%s)

while :; do
    # В foreground: с -b picom отдал бы управление сразу, и сторожить было бы
    # нечего.
    "$PICOM" --config "$CONFIG" &
    child=$!
    wait "$child"
    code=$?
    child=0

    # Нас попросили уйти: SIGTERM (143) или SIGINT (130) -- это pkill из
    # apply.py при смене конфига, и поднимать picom обратно не нужно, новый
    # запустит тот, кто нас погасил.
    case $code in
        0|130|143) exit 0 ;;
    esac

    now=$(date +%s)
    if [ $((now - window_start)) -ge "$WINDOW" ]; then
        count=0
        window_start=$now
    fi
    count=$((count + 1))

    if [ "$count" -ge "$MAX_CRASHES" ]; then
        notify "picom падает подряд ($count раз за ${WINDOW}с), больше не поднимаю. Подробности: coredumpctl list | grep picom"
        exit 1
    fi

    notify "picom упал (код $code), поднимаю"
    sleep 1
done
