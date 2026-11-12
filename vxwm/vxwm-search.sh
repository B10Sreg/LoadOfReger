#!/usr/bin/env bash
# =============================================================================
# vxwm-search: Ультимативный поиск для Load of Reger
# =============================================================================
# Использование:
#   vxwm-search           -- главное окно поиска (приложения, веб, калькулятор, ИИ, файлы)
#   vxwm-search --calc    -- открыть сразу в режиме калькулятора (=)
#   vxwm-search --ai      -- открыть сразу для запроса к ИИ (ai )
#   vxwm-search --web     -- открыть сразу для поиска в интернете (? )
#   vxwm-search --files   -- открыть сразу поиск файлов (~ )
# =============================================================================

set -uo pipefail

SEARCH_PY="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/search.py"
FILTER=""

case "${1:-}" in
    --calc|-c)
        FILTER="="
        ;;
    --ai|-a)
        FILTER="ai "
        ;;
    --web|-w)
        FILTER="? "
        ;;
    --files|-f)
        FILTER="~ "
        ;;
    -h|--help)
        echo "Использование: $0 [--calc|--ai|--web|--files|фильтр]"
        exit 0
        ;;
    *)
        if [ -n "${1:-}" ]; then
            FILTER="$1"
        fi
        ;;
esac

ROFI_ARGS=(-show search -modes "search:$SEARCH_PY")
if [ -n "$FILTER" ]; then
    ROFI_ARGS+=(-filter "$FILTER")
fi

exec rofi "${ROFI_ARGS[@]}"
