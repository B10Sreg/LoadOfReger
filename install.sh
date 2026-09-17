#!/usr/bin/env bash
# Установщик Load of Reger: от чистого Arch до готовой сессии одной командой.
#
#     ./install.sh --fresh         -- на чистую систему: ставим всё
#     ./install.sh --existing      -- поверх настроенной: спрашиваем каждый шаг
#     ./install.sh                 -- спросит режим сам
#     ./install.sh --help          -- что умеет
#
# Два режима -- про разную цену ошибки. На чистой системе ломать нечего:
# ставим все зависимости, кладём свои конфиги, включаем экран входа, выставляем
# раскладку -- получается ровно тот рабочий стол, что на скриншотах. На
# обжитой машине у человека уже есть свой kitty, своя раскладка и свой
# менеджер входа, и рис, который молча пройдёт по ним катком, -- это не
# установка, а потеря настроек. Поэтому там мы сначала показываем, что именно
# затронем, потом спрашиваем по каждой группе пакетов, а личные умолчания
# риса (та же раскладка) не трогаем вовсе.
#
# Что делает по шагам -- см. функции ниже; каждая печатает, что именно
# сделала. Скрипт идемпотентен: повторный запуск не ломает уже поставленное,
# а обновляет. Все чужие файлы, которые он трогает (~/.xinitrc, ~/.zprofile),
# либо сохраняются в .bak-<дата>, либо правятся блоком между маркерами -- то,
# что человек написал вокруг, остаётся на месте.
#
# Почему только Arch: рис завязан на AUR-соседство (picom-ftlabs), pacman и
# mkinitcpio. На другом дистрибутиве скрипт честно откажется работать и
# покажет список пакетов, которые надо найти самому: --list-packages.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin"
CFG="${XDG_CONFIG_HOME:-$HOME/.config}"
RICE_HOME="$CFG/vxwm-rice"
STAMP=$(date +%Y%m%d-%H%M%S)

# ------------------------------------------------------------------ пакеты
#
# Разбиты по назначению: так видно, что можно не ставить (--no-picom,
# --no-optional, --no-sddm) и что сломается, если не поставить.

# Сборка и X: без этого не соберётся сам vxwm и не стартует сессия.
PKGS_CORE=(
    base-devel git make pkgconf
    libx11 libxft libxinerama
    xorg-server xorg-xinit xorg-xrdb xorg-xsetroot xorg-setxkbmap xorg-xset
    xorg-xprop xdotool
    rust go python
)

# Сам рис: то, что рисует, звучит и уведомляет.
PKGS_RICE=(
    kitty rofi dunst feh slock xss-lock libnotify
    wireplumber playerctl brightnessctl
    ttf-jetbrains-mono-nerd
    gtk3 gtk4 libadwaita qt5ct qt6ct breeze
    cairo pango glib2 desktop-file-utils
    # libgirepository -- ради gobject-introspection-1.0.pc: без него cgo-сборка
    # gotk4 (приложение настроек) падает на первом же пакете. На обжитой машине
    # он обычно уже стоит прицепом к чему-нибудь, на чистой -- нет.
    libgirepository
)

# Зависимости сборки патченого picom (FT-Labs): анимации и прозрачность.
PKGS_PICOM=(
    meson ninja libconfig libepoxy libev pcre2 pixman uthash
    libxcb xcb-util-image xcb-util-renderutil dbus libdrm mesa
)

# Комфорт: то, без чего чистая система формально работает, а пользоваться ею
# неудобно. Оболочка с дополнением и подсветкой, эмодзи в уведомлениях, сеть,
# звук, монтирование дисков, буфер обмена, архивы.
PKGS_COMFORT=(
    zsh zsh-completions zsh-autosuggestions zsh-syntax-highlighting
    # Без noto-fonts-emoji вместо эмодзи в меню питания и уведомлениях
    # будут пустые прямоугольники: рис ими подписывает пункты.
    noto-fonts noto-fonts-emoji
    # Сеть: настраивается через nmtui в терминале -- трея у бара нет,
    # и апплету некуда было бы сесть.
    networkmanager
    # Звук целиком: wireplumber уже в группе rice, здесь -- совместимость с
    # программами, которые умеют только PulseAudio или ALSA, и регулятор.
    pipewire-pulse pipewire-alsa pavucontrol
    # Флешки и эскизы в файловом менеджере плюс агент авторизации: без него
    # окно "введите пароль" не появится, и монтирование молча не сработает.
    gvfs gvfs-mtp tumbler polkit-gnome ntfs-3g
    # Буфер обмена для скриптов, архивы, каталоги вида ~/Загрузки.
    xclip unzip zip 7zip xdg-utils xdg-user-dirs
    # То, чего не хватает в первый же день работы в терминале.
    man-db less btop neovim wget tree
    # Bluetooth без графики: bluetoothctl. Служба сама не включается.
    bluez bluez-utils
)

# Экран входа.
PKGS_SDDM=(sddm)

# Программы, на которые повешены хоткеи в config.def.h. Рис без них работает,
# но Mod+B, Mod+E и PrintScreen будут звать несуществующее.
PKGS_OPTIONAL=(firefox thunar flameshot)

# ------------------------------------------------------------------- флаги
MODE=""            # fresh | existing -- см. шапку
GROUPS_SET=""      # --groups: выбор групп без вопросов
DO_PKG_CORE=1
DO_PKG_RICE=1
DO_PACKAGES=1
DO_PICOM=1
DO_SDDM=1
DO_OPTIONAL=1
DO_COMFORT=1
DO_AUR=1
DO_XINERAMA=0
DRY=0
ASSUME_YES=0

usage() {
    cat <<EOF
Установщик Load of Reger.

    ./install.sh [--fresh|--existing] [флаги]

Режимы:
  --fresh           чистая система: ставим все зависимости, свои конфиги,
                    экран входа и раскладку риса. Вопросов не задаём
  --existing        поверх настроенной системы: показываем, что затронем,
                    спрашиваем по каждой группе пакетов и перед каждой
                    заменой чужого файла. Раскладку и прочие личные
                    умолчания риса не трогаем

  Без флага режим спрашивается при запуске.

Флаги:
  --groups СПИСОК   какие группы пакетов ставить без вопросов, через запятую:
                    core,rice,picom,sddm,optional
  --no-packages     не трогать pacman (всё уже стоит)
  --no-picom        не собирать патченый picom (тогда анимации листания тегов
                    будут дёрганые: системный picom не умеет нужного поведения)
  --no-sddm         не ставить экран входа; вместо него startx с tty1
  --no-optional     без firefox/thunar/flameshot
  --no-comfort      без zsh, сети, звука, шрифтов эмодзи и прочих удобств
  --no-aur          не ставить paru (помощник для AUR)
  --xinerama        собрать vxwm с поддержкой нескольких мониторов. Без флага
                    все экраны видятся как один. Включай, если монитора два:
                    в этом форке мультимонитор не обкатан, поэтому не по умолчанию
  --list-packages   показать список пакетов и выйти
  --dry-run         показать, что было бы сделано, ничего не меняя
  -y, --yes         не спрашивать подтверждений
  -h, --help        это сообщение
EOF
}

while [ $# -gt 0 ]; do
    case $1 in
        --fresh)       MODE=fresh ;;
        --existing)    MODE=existing ;;
        --groups)
            shift
            [ $# -gt 0 ] || { echo "--groups: нужен список групп" >&2; exit 2; }
            GROUPS_SET=$1 ;;
        --groups=*)    GROUPS_SET=${1#*=} ;;
        --no-packages) DO_PACKAGES=0 ;;
        --no-picom)    DO_PICOM=0 ;;
        --no-sddm)     DO_SDDM=0 ;;
        --no-optional) DO_OPTIONAL=0 ;;
        --no-comfort)  DO_COMFORT=0 ;;
        --no-aur)      DO_AUR=0 ;;
        --xinerama)    DO_XINERAMA=1 ;;
        --dry-run)     DRY=1 ;;
        -y|--yes)      ASSUME_YES=1 ;;
        --list-packages)
            printf '%s\n' "${PKGS_CORE[@]}" "${PKGS_RICE[@]}" "${PKGS_PICOM[@]}" \
                          "${PKGS_SDDM[@]}" "${PKGS_OPTIONAL[@]}"
            exit 0 ;;
        -h|--help)     usage; exit 0 ;;
        *) echo "неизвестный флаг: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

# ------------------------------------------------------------------- вывод
if [ -t 1 ]; then
    B=$'\033[1m'; G=$'\033[32m'; Y=$'\033[33m'; R=$'\033[31m'; N=$'\033[0m'
else
    B=; G=; Y=; R=; N=
fi

step() { printf '\n%s==> %s%s\n' "$B" "$*" "$N"; }
ok()   { printf '    %s✓%s %s\n' "$G" "$N" "$*"; }
warn() { printf '    %s!%s %s\n' "$Y" "$N" "$*" >&2; }
die()  { printf '\n%sОшибка:%s %s\n' "$R" "$N" "$*" >&2; exit 1; }

# Единственное место, где что-то выполняется: --dry-run честен ровно потому,
# что показывает те же команды, которые иначе запустил бы.
run() {
    if [ "$DRY" = 1 ]; then
        printf '    %s[dry]%s %s\n' "$Y" "$N" "$*"
        return 0
    fi
    "$@"
}

# Нет терминала и не сказано -y -- считаем это отказом. Молча согласиться за
# человека на замену его же конфигов хуже, чем не сделать ничего: запустят
# из скрипта, отвернутся, а вернётся он к перезаписанному kitty.conf.
NOTTY_WARNED=0
notty_is_no() {
    [ -t 0 ] && return 1
    if [ "$NOTTY_WARNED" = 0 ]; then
        warn "нет терминала: все вопросы считаю за «нет» (для автоматической установки добавь -y)"
        NOTTY_WARNED=1
    fi
    return 0
}

confirm() {
    [ "$ASSUME_YES" = 1 ] && return 0
    [ "$DRY" = 1 ] && return 0
    notty_is_no && return 1
    local a
    read -r -p "    $1 [Y/n] " a
    case ${a:-y} in [YyДд]*) return 0 ;; *) return 1 ;; esac
}

# То же, но молчание означает "нет": для шагов, которые трогают систему
# целиком, а не домашний каталог.
confirm_default_no() {
    [ "$ASSUME_YES" = 1 ] && return 0
    [ "$DRY" = 1 ] && return 0
    notty_is_no && return 1
    local a
    read -r -p "  $1 [y/N] " a
    case $a in [YyДд]*) return 0 ;; *) return 1 ;; esac
}

# ------------------------------------------------------------------ режим
#
# Вопрос ровно один и задаётся до всякой работы: дальше поведение установщика
# от него зависит целиком, и менять его на середине было бы хуже, чем спросить.
choose_mode() {
    [ -n "$MODE" ] && return 0
    if [ ! -t 0 ]; then
        die "укажи режим: --fresh (чистая система) или --existing (поверх настроенной)"
    fi
    cat <<EOF

${B}Куда ставим?${N}

  ${B}1${N}) Чистая система -- Arch поставлен, рабочего стола ещё нет.
     Ставим все зависимости, свои конфиги, экран входа и раскладку риса.
     Вопросов больше не задаём.

  ${B}2${N}) Поверх настроенной системы -- здесь уже есть свой рабочий стол.
     Спрашиваем по каждой группе пакетов и перед заменой каждого чужого
     файла. Раскладку и прочие личные умолчания риса не трогаем.
     ${Y}Этот путь обкатан хуже: что-то может не завестись.${N}

EOF
    local a
    while :; do
        read -r -p "  Выбор [1/2]: " a
        case $a in
            1) MODE=fresh; return 0 ;;
            2) MODE=existing; return 0 ;;
            *) echo "  Введи 1 или 2." ;;
        esac
    done
}

# Дисклеймер второго режима. Списком, а не абзацем: человек должен увидеть
# конкретные файлы, а не общее "может что-то изменить".
disclaimer() {
    [ "$MODE" = existing ] || return 0
    cat <<EOF

${Y}!! Установка поверх настроенной системы.${N}

   Рис рассчитан на чистый Arch. Здесь он:
     - заменит ~/.xinitrc и ~/.xprofile (спросит, старые сохранит как .bak-<дата>);
     - перепишет конфиги kitty, rofi, dunst и picom в ~/.config -- они
       генерируются из rice.toml (перед первой записью сделает .bak-<дата>);
     - допишет блок между маркерами в профиль оболочки (PATH до ~/.local/bin);
     - поставит vxwm в /usr/local/bin, а свой picom -- в ~/.local/bin, который
       в PATH идёт раньше системного;
     - добавит свой блок в gtk.css и схему цветов в qt5ct/qt6ct;
     - если согласишься -- включит SDDM и сменит экран входа.

   Раскладку клавиатуры и прочие личные умолчания риса в этом режиме не
   трогаю: ими управляет [input] в rice.toml.

   Снимается через ./uninstall.sh, но идеальной обратимости не обещаю:
   сделай снимок системы или бэкап ~/.config, если там есть чем дорожить.

EOF
    confirm_default_no "Продолжить установку?" || { echo "Отменено."; exit 0; }
}

# Шаг, который на чистой системе делается молча, а на обжитой -- с вопросом.
ask_step() {
    [ "$MODE" = fresh ] && return 0
    confirm "$1"
}

# --------------------------------------------------------------- проверки
preflight() {
    step "Проверки"
    [ "$(id -u)" != 0 ] || die "не запускай от root: рис ставится в домашний каталог, а sudo скрипт попросит сам"
    command -v pacman >/dev/null 2>&1 || {
        echo "Этот установщик рассчитан на Arch (pacman)." >&2
        echo "Список пакетов для ручной установки: $0 --list-packages" >&2
        exit 1
    }
    command -v sudo >/dev/null 2>&1 || die "нужен sudo: ставим vxwm в /usr/local и тему входа в /usr/share"
    [ -f "$REPO/vxwm/rice/apply.py" ] || die "запускай из каталога репозитория: не нашёлся $REPO/vxwm/rice/apply.py"
    ok "Arch, обычный пользователь, репозиторий на месте"

    # Один запрос пароля на всю установку вместо трёх в разных местах.
    if [ "$DRY" = 0 ]; then
        sudo -v || die "нужен sudo"
        # Держим талон живым, пока идёт долгая сборка.
        while true; do sudo -n true; sleep 50; kill -0 "$$" 2>/dev/null || exit; done 2>/dev/null &
        SUDO_KEEPALIVE=$!
        trap 'kill "$SUDO_KEEPALIVE" 2>/dev/null || true' EXIT
    fi
}

# ---------------------------------------------------------------- пакеты
#
# Пакеты разложены по группам, и группа -- это единица выбора во втором режиме:
# человеку важно не "поставить 57 пакетов", а решить, нужен ли ему экран входа
# и готов ли он собирать композитор.
group_pkgs() {
    case $1 in
        core)     printf '%s\n' "${PKGS_CORE[@]}" ;;
        rice)     printf '%s\n' "${PKGS_RICE[@]}" ;;
        picom)    printf '%s\n' "${PKGS_PICOM[@]}" ;;
        comfort)  printf '%s\n' "${PKGS_COMFORT[@]}" ;;
        sddm)     printf '%s\n' "${PKGS_SDDM[@]}" ;;
        optional) printf '%s\n' "${PKGS_OPTIONAL[@]}" ;;
    esac
}

group_title() {
    case $1 in
        core)     echo "сборка и X11 -- без них не соберётся ни vxwm, ни бар" ;;
        rice)     echo "сам рис: терминал, меню, уведомления, звук, шрифт, темы GTK/Qt" ;;
        picom)    echo "композитор с анимациями: тени, прозрачность, листание тегов" ;;
        comfort)  echo "удобства: zsh с дополнением, сеть, звук, эмодзи, флешки, архивы" ;;
        sddm)     echo "экран входа SDDM" ;;
        optional) echo "программы под хоткеями: firefox, thunar, flameshot" ;;
    esac
}

# Группа base-devel -- это группа pacman, а не пакет: -Qq про неё не знает.
have_pkg() { pacman -Qq "$1" >/dev/null 2>&1 || pacman -Qgq "$1" >/dev/null 2>&1; }

group_off() {
    case $1 in
        core)     DO_PKG_CORE=0 ;;
        rice)     DO_PKG_RICE=0 ;;
        picom)    DO_PICOM=0 ;;
        comfort)  DO_COMFORT=0 ;;
        sddm)     DO_SDDM=0 ;;
        optional) DO_OPTIONAL=0 ;;
    esac
}

# Что ставим. Заодно решает судьбу шагов: отказ от группы picom -- это и отказ
# собирать композитор, иначе сборка упала бы на отсутствующих заголовках.
WANT_GROUPS=()
select_groups() {
    local g missing n
    for g in core rice comfort picom sddm optional; do
        # Флаги командной строки уже ответили за человека.
        case $g in
            picom)    [ "$DO_PICOM" = 1 ]    || continue ;;
            sddm)     [ "$DO_SDDM" = 1 ]     || continue ;;
            optional) [ "$DO_OPTIONAL" = 1 ] || continue ;;
            comfort)  [ "$DO_COMFORT" = 1 ] || continue ;;
            core|rice) [ "$DO_PACKAGES" = 1 ] || continue ;;
        esac

        if [ -n "$GROUPS_SET" ]; then
            case ",$GROUPS_SET," in
                *",$g,"*) WANT_GROUPS+=("$g") ;;
                *)        group_off "$g" ;;
            esac
            continue
        fi

        if [ "$MODE" = fresh ]; then
            WANT_GROUPS+=("$g")
            continue
        fi

        # Режим «поверх настроенной системы»: показываем, чего не хватает, и
        # спрашиваем. Видеть список важнее, чем количество: по нему сразу
        # понятно, потянет ли группа за собой половину KDE.
        missing=$(group_pkgs "$g" | while read -r one; do
            have_pkg "$one" || printf '%s ' "$one"
        done)
        n=$(printf '%s' "$missing" | wc -w)
        printf '\n    %s%s%s -- %s\n' "$B" "$g" "$N" "$(group_title "$g")"
        if [ "$n" = 0 ]; then
            printf '      пакеты уже стоят\n'
        else
            printf '      поставит (%s): %s\n' "$n" "$missing"
        fi

        # У picom и sddm вопрос не про пакеты, а про шаг: собранный picom
        # перекроет системный в PATH, а sddm сменит экран входа. Поэтому их
        # спрашиваем даже тогда, когда ставить уже нечего. Остальным группам,
        # если всё на месте, вопрос задавать не о чем.
        case $g in
            picom)
                if confirm "Собрать патченый picom в ~/.local/bin? Он перекроет системный"; then
                    WANT_GROUPS+=("$g")
                else
                    group_off "$g"
                fi ;;
            sddm)
                if confirm_default_no "  Ставить SDDM и менять им экран входа?"; then
                    WANT_GROUPS+=("$g")
                else
                    group_off "$g"
                fi ;;
            optional)
                if [ "$n" = 0 ]; then
                    WANT_GROUPS+=("$g")
                elif confirm_default_no "  Поставить эти программы?"; then
                    WANT_GROUPS+=("$g")
                else
                    group_off "$g"
                fi ;;
            *)
                if [ "$n" = 0 ]; then
                    WANT_GROUPS+=("$g")
                elif confirm "Поставить группу $g?"; then
                    WANT_GROUPS+=("$g")
                else
                    group_off "$g"
                fi ;;
        esac
    done
}

install_packages() {
    if [ "$DO_PACKAGES" != 1 ]; then
        warn "пакеты пропущены (--no-packages)"
        return 0
    fi
    step "Пакеты"

    local pkgs=() g one
    for g in ${WANT_GROUPS[@]+"${WANT_GROUPS[@]}"}; do
        case $g in
            core)    [ "$DO_PKG_CORE" = 1 ] || continue ;;
            rice)    [ "$DO_PKG_RICE" = 1 ] || continue ;;
            comfort) [ "$DO_COMFORT" = 1 ] || continue ;;
        esac
        while read -r one; do pkgs+=("$one"); done < <(group_pkgs "$g")
    done

    if [ ${#pkgs[@]} = 0 ]; then
        warn "ни одной группы не выбрано -- pacman не трогаю"
        return 0
    fi

    # --needed: уже стоящее не переустанавливаем, иначе pacman тянет мегабайты
    # ради ничего. Ставим одной транзакцией: так конфликты видны сразу, а не
    # на середине списка.
    run sudo pacman -S --needed --noconfirm "${pkgs[@]}" \
        || die "pacman не поставил пакеты. Разбери конфликт и запусти снова."
    ok "пакетов запрошено: ${#pkgs[@]}"
}

# ------------------------------------------------------------------- vxwm
install_vxwm() {
    step "Оконный менеджер vxwm"
    local src="$REPO/vxwm-src"

    # config.h и modules.h -- личные правки пользователя, Makefile создаёт их
    # из *.def.h при первой сборке. Уже существующие не трогаем: там могут
    # быть чужие хоткеи, и затирать их обновлением риса нельзя.
    for f in config modules; do
        if [ ! -f "$src/$f.h" ]; then
            run cp "$src/$f.def.h" "$src/$f.h"
            ok "$f.h создан из $f.def.h"
        elif ! cmp -s "$src/$f.h" "$src/$f.def.h"; then
            warn "$f.h отличается от $f.def.h -- оставляю твою версию"
        fi
    done

    # Xinerama включается флагами сборки, а не правкой config.mk: так файл
    # остаётся апстримовым, а выбор виден в команде установки.
    local xin=()
    if [ "$DO_XINERAMA" = 1 ]; then
        xin=(XINERAMALIBS=-lXinerama XINERAMAFLAGS=-DXINERAMA)
    fi

    run make -C "$src" clean >/dev/null
    run make -C "$src" ${xin[@]+"${xin[@]}"} >/dev/null || die "vxwm не собрался"
    run sudo make -C "$src" ${xin[@]+"${xin[@]}"} install >/dev/null || die "vxwm не установился"
    ok "vxwm собран и установлен в /usr/local/bin${xin:+ (с Xinerama)}"
}

# ------------------------------------------------------- бар и настройки
install_vxbar() {
    step "Бар vxbar"
    run bash "$REPO/vxbar/install.sh" || die "vxbar не собрался (нужны rust, cairo, pango)"

    step "Приложение настроек vxbar-settings"
    run bash "$REPO/vxbar-settings/install.sh" || die "vxbar-settings не собрался (нужны go, gtk4, libadwaita)"
}

# ------------------------------------------------------------------ picom
install_picom() {
    if [ "$DO_PICOM" != 1 ]; then
        warn "патченый picom пропущен: анимации и прозрачность будут зависеть от системного"
        command -v picom >/dev/null 2>&1 || warn "системного picom тоже нет: рис поедет без теней, прозрачности и анимаций"
        return 0
    fi
    step "Композитор picom (патченая сборка FT-Labs)"
    # Сборка тянет исходники из сети. Оборваться здесь не смертельно: рис
    # работает и на системном picom, просто листание тегов будет дёрганым.
    if run bash "$REPO/picom/build-patched.sh"; then
        ok "picom собран с патчами и лежит в $BIN/picom"
    else
        warn "picom не собрался. Рис заработает и без него; собрать позже: bash picom/build-patched.sh"
    fi
}

# ------------------------------------------------------------ AUR-помощник
#
# paru сам в репозиториях не лежит -- его собирают из AUR. Берём paru-bin:
# это готовый бинарь, тогда как обычный paru тянет полную сборку на Rust ради
# того же результата.
install_aur_helper() {
    if [ "$DO_AUR" != 1 ]; then
        warn "AUR-помощник пропущен"
        return 0
    fi
    for helper in paru yay; do
        if command -v "$helper" >/dev/null 2>&1; then
            ok "помощник для AUR уже есть: $helper"
            return 0
        fi
    done
    ask_step "Поставить paru -- помощник для AUR?" || { warn "paru пропущен"; return 0; }

    step "Помощник для AUR (paru)"
    if [ "$DRY" = 1 ]; then
        printf '    %s[dry]%s собрал бы paru-bin из AUR\n' "$Y" "$N"
        return 0
    fi
    local tmp
    tmp=$(mktemp -d)
    if git clone -q https://aur.archlinux.org/paru-bin.git "$tmp/paru-bin" \
       && ( cd "$tmp/paru-bin" && makepkg -si --noconfirm --needed >/dev/null ); then
        ok "paru поставлен: paru -S <пакет> ставит из AUR"
    else
        # Не смертельно: рис целиком собирается из исходников и AUR не требует.
        warn "paru не собрался -- поставь позже руками, если нужен AUR"
    fi
    rm -rf "$tmp"
}

# ---------------------------------------------------------------- комфорт
#
# Пакеты этой группы уже поставлены выше; здесь -- то, что после установки
# надо ещё включить или связать, иначе они просто лежат без дела.
setup_comfort() {
    if [ "$DO_COMFORT" != 1 ]; then
        warn "настройка удобств пропущена"
        return 0
    fi
    step "Оболочка и удобства"

    # zsh как оболочка входа. Меняем через chsh под sudo: своя оболочка --
    # это /etc/passwd, и без root туда не записать.
    local zsh_path current
    zsh_path=$(command -v zsh 2>/dev/null || true)
    current=$(getent passwd "$USER" | cut -d: -f7)
    if [ -n "$zsh_path" ] && [ "$current" != "$zsh_path" ]; then
        if ask_step "Сделать zsh оболочкой входа? (сейчас $current)"; then
            if run sudo chsh -s "$zsh_path" "$USER"; then
                ok "оболочка входа: zsh (подхватится при следующем входе)"
            else
                warn "chsh не сработал -- смени оболочку сам: chsh -s $zsh_path"
            fi
        fi
    elif [ -n "$zsh_path" ]; then
        ok "zsh уже оболочка входа"
    fi

    # Конфиг оболочки. Свой не трогаем без спроса -- см. backup_and_link.
    if [ -n "$zsh_path" ]; then
        backup_and_link "$REPO/zsh/zshrc" "$HOME/.zshrc"
    fi

    # Сеть. На чистой системе её не поднимает никто: ни один пакет риса
    # не тянет NetworkManager, и после перезагрузки машина осталась бы без
    # интернета. Настраивается потом через nmtui.
    if command -v nmcli >/dev/null 2>&1; then
        if systemctl is-enabled --quiet NetworkManager.service 2>/dev/null; then
            ok "NetworkManager уже включён"
        elif ask_step "Включить NetworkManager? (настройка сети -- nmtui)"; then
            run sudo systemctl enable --now NetworkManager.service \
                && ok "NetworkManager включён" \
                || warn "NetworkManager не включился (в контейнере это норма)"
        fi
    fi

    # Каталоги вида ~/Загрузки: их ждут браузер, файловый менеджер и
    # скриншотилка, а сами они не появляются.
    if command -v xdg-user-dirs-update >/dev/null 2>&1; then
        run xdg-user-dirs-update
        ok "каталоги пользователя (Загрузки, Документы, Изображения)"
    fi
}

# --------------------------------------------------------------- ссылки
#
# Скрипты риса живут в репозитории, а вызываются по коротким именам из
# ~/.local/bin: так config.def.h не знает, куда человек склонировал репозиторий,
# и обновление риса -- это git pull, без переустановки.
link_scripts() {
    step "Ссылки на скрипты"
    run mkdir -p "$BIN" "$CFG/vxwm"

    link() {
        local target=$1 name=$2
        if [ -e "$BIN/$name" ] && [ ! -L "$BIN/$name" ]; then
            run mv "$BIN/$name" "$BIN/$name.bak-$STAMP"
            warn "$BIN/$name был обычным файлом -- сохранён как $name.bak-$STAMP"
        fi
        run ln -sfn "$target" "$BIN/$name"
    }

    link "$REPO/vxwm/theme.sh"       vxwm-theme.sh
    link "$REPO/vxwm/powermenu.sh"   vxwm-powermenu.sh
    link "$REPO/vxwm/power.sh"       vxwm-power.sh
    link "$REPO/vxwm/scratchpad.sh"  vxwm-scratchpad.sh
    link "$REPO/vxwm/session.sh"     vxwm-session
    link "$REPO/vxwm/rice/apply.py"  vxwm-rice-apply

    # Автозапуск: vxwm зовёт его по фиксированному пути из config.def.h.
    run ln -sfn "$REPO/vxwm/autostart.sh" "$CFG/vxwm/autostart.sh"
    ok "vxwm-theme.sh, vxwm-powermenu.sh, vxwm-power.sh, vxwm-scratchpad.sh, vxwm-session, vxwm-rice-apply"
    ok "$CFG/vxwm/autostart.sh"
}

# ------------------------------------------------------- вход в сессию
#
# Три файла, от которых зависит, запустится ли рис вообще: ~/.xinitrc (что
# исполняет startx), ~/.xprofile (переменные окружения для GTK/Qt) и ~/.zprofile
# (PATH до ~/.local/bin). Чужие версии не затираем молча.
SKIPPED_SESSION_FILES=""

# Положить ссылку на файл из репозитория, не потеряв то, что лежало там
# раньше. Во втором режиме -- с вопросом: чужой ~/.zshrc или ~/.xinitrc может
# быть делом нескольких лет.
backup_and_link() {
        local target=$1 dest=$2
        # Сравниваем саму цель ссылки, а не разрешённый путь: readlink -f
        # отдаёт пустую строку, если каталога цели ещё нет, и тогда уже
        # правильная ссылка выглядела бы чужой и уезжала в .bak на каждом прогоне.
        if [ -L "$dest" ] && [ "$(readlink "$dest")" = "$target" ]; then
            ok "$(basename "$dest") уже указывает куда надо"
            return
        fi
        if [ -e "$dest" ] || [ -L "$dest" ]; then
            if ! ask_step "Заменить $(basename "$dest")? Старый сохраню рядом как .bak-$STAMP"; then
                warn "$(basename "$dest") оставлен как был -- сессию придётся запускать самому"
                SKIPPED_SESSION_FILES="$SKIPPED_SESSION_FILES $(basename "$dest")"
                return
            fi
            run mv "$dest" "$dest.bak-$STAMP"
            warn "$(basename "$dest") сохранён как $(basename "$dest").bak-$STAMP"
        fi
        run ln -sfn "$target" "$dest"
        ok "$(basename "$dest") -> $target"
}

setup_session_files() {
    step "Файлы входа в сессию"

    backup_and_link "$REPO/vxwm/session.sh" "$HOME/.xinitrc"
    backup_and_link "$REPO/x11/xprofile"    "$HOME/.xprofile"

    # zprofile правим блоком: у человека там может быть своё, и подменять файл
    # целиком ради двух строк PATH -- перебор.
    local marker_begin='# >>> vxwm-rice >>>' marker_end='# <<< vxwm-rice <<<'
    local block startx=""
    # Без экрана входа сессию поднимает startx при логине на tty1. С SDDM этой
    # части в блоке нет: два механизма поспорили бы за один tty.
    if [ "$DO_SDDM" != 1 ]; then
        startx="
# Автозапуск X при логине на tty1 -- вместо менеджера входа.
if [ -z \"\$DISPLAY\" ] && [ \"\$(tty)\" = \"/dev/tty1\" ]; then
    exec startx
fi"
    fi
    block="$marker_begin
# PATH до ~/.local/bin: оттуда запускаются vxbar, vxbar-settings и скрипты
# риса, и оттуда же их берёт vxwm (он зовёт их по имени, без пути).
case \":\$PATH:\" in
    *\":\$HOME/.local/bin:\"*) ;;
    *) export PATH=\"\$HOME/.local/bin:\$PATH\" ;;
esac$startx
$marker_end"

    # Куда писать блок -- решает логин-оболочка: zsh читает ~/.zprofile, bash --
    # ~/.bash_profile, всё остальное -- ~/.profile. Промахнуться значит оставить
    # человека без PATH: vxbar и скрипты риса лежат в ~/.local/bin и по имени
    # не найдутся.
    local shell_name zp
    shell_name=$(basename "$(getent passwd "$USER" | cut -d: -f7)")
    case $shell_name in
        zsh)
            zp="$HOME/.zprofile" ;;
        bash)
            # Создать ~/.bash_profile там, где его не было, -- значит заслонить
            # им ~/.profile: bash читает только первый из них. Поэтому дописываем
            # в тот файл, который у человека уже есть.
            if [ ! -f "$HOME/.bash_profile" ] && [ -f "$HOME/.profile" ]; then
                zp="$HOME/.profile"
            else
                zp="$HOME/.bash_profile"
            fi ;;
        *)
            zp="$HOME/.profile" ;;
    esac
    if [ "$DRY" = 1 ]; then
        printf '    %s[dry]%s обновил бы блок vxwm-rice в %s (оболочка: %s)\n' "$Y" "$N" "$zp" "$shell_name"
    else
        local existing=""
        [ -f "$zp" ] && existing=$(cat "$zp")
        if printf '%s' "$existing" | grep -qF "$marker_begin"; then
            # Меняем только свой блок, остальное не трогаем.
            python3 - "$zp" "$marker_begin" "$marker_end" "$block" <<'PY'
import sys, pathlib
path, begin, end, block = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
text = pathlib.Path(path).read_text()
head, _, rest = text.partition(begin)
_, _, tail = rest.partition(end)
pathlib.Path(path).write_text(head + block + tail)
PY
        else
            { [ -n "$existing" ] && printf '%s\n\n' "$existing"; printf '%s\n' "$block"; } > "$zp.new"
            mv "$zp.new" "$zp"
        fi
        ok "$zp: блок vxwm-rice на месте"
    fi

    if [ "$DO_SDDM" != 1 ]; then
        ok "без экрана входа: X поднимется через startx при логине на tty1"
    fi
}

# ---------------------------------------------------------------- рис
#
# Конфиги kitty, rofi, dunst и picom рис генерирует из rice.toml -- то есть
# перезаписывает. На чистой системе переписывать нечего, на обжитой там лежит
# чужая работа, и молча стереть её нельзя.
backup_foreign_configs() {
    local f
    for f in "$CFG/rofi/config.rasi" "$CFG/kitty/kitty.conf" \
             "$CFG/dunst/dunstrc" "$CFG/picom/picom.conf"; do
        [ -f "$f" ] || continue
        # Свой же сгенерированный файл бэкапить незачем: он собирается из
        # rice.toml заново. Опознаём по строке генератора в шапке.
        grep -qs -e 'mktheme.py' -e 'apply.py' "$f" && continue
        run cp -a "$f" "$f.bak-$STAMP"
        warn "$(basename "$f") сохранён как $(basename "$f").bak-$STAMP -- его перезапишет rice.toml"
    done
}

# Раскладка и прочие личные умолчания риса. Пишем их только в первом режиме:
# на обжитой машине клавиатура уже настроена, и переучивать её под рис -- это
# ровно тот сюрприз, ради которого второй режим и заведён.
set_rice_input() {
    local layout=$1 options=$2
    if [ "$DRY" = 1 ]; then
        printf '    %s[dry]%s записал бы в rice.toml раскладку %s\n' "$Y" "$N" "${layout:-<системную>}"
        return 0
    fi
    python3 - "$RICE_HOME/rice.toml" "$layout" "$options" <<'RICEPY'
import re, sys, pathlib

path, layout, options = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
want = {"layout": layout, "options": options}
out, section, seen = [], None, set()
for line in path.read_text().splitlines():
    m = re.match(r"\s*\[(\w+)\]", line)
    if m:
        section = m.group(1)
    elif section == "input":
        km = re.match(r"\s*(\w+)\s*=", line)
        if km and km.group(1) in want:
            line = f'{km.group(1)} = "{want[km.group(1)]}"'
            seen.add(km.group(1))
    out.append(line)

missing = [k for k in want if k not in seen]
if missing:
    # Секции [input] в конфиге может не быть вовсе: он мог быть написан руками
    # или достаться от прежней версии риса.
    sections = {m.group(1) for m in (re.match(r"\s*\[(\w+)\]", l) for l in out) if m}
    out.append("")
    if "input" not in sections:
        out.append("[input]")
    out += [f'{k} = "{want[k]}"' for k in missing]

path.write_text("\n".join(out) + "\n")
RICEPY
    ok "раскладка: ${layout:-системная, риса не навязываю}"
}

bootstrap_rice() {
    step "Конфиг риса"
    run mkdir -p "$RICE_HOME"
    local created=0
    if [ -f "$RICE_HOME/rice.toml" ]; then
        ok "rice.toml уже есть -- не трогаю (это твои настройки)"
    else
        run cp "$REPO/vxwm/rice/rice.toml" "$RICE_HOME/rice.toml"
        created=1
        ok "rice.toml из репозитория -> $RICE_HOME/rice.toml"
    fi

    if [ "$MODE" = fresh ]; then
        set_rice_input "us,ru" "grp:alt_shift_toggle"
    elif [ "$created" = 1 ]; then
        # Свежий конфиг во втором режиме: раскладка остаётся системной.
        set_rice_input "" ""
    fi

    backup_foreign_configs

    # Бар создаёт свой config.toml сам при первом запуске -- но до этого
    # apply.py нечего в нём красить, и первая сессия поехала бы с цветами по
    # умолчанию. Запускаем бар без DISPLAY: конфиг он пишет до подключения к
    # X, так что файл появится, а сам процесс сразу же и закончится.
    if [ ! -f "$CFG/vxbar/config.toml" ] && [ -x "$BIN/vxbar" ]; then
        run env -u DISPLAY "$BIN/vxbar" >/dev/null 2>&1 || true
        [ -f "$CFG/vxbar/config.toml" ] && ok "config.toml бара создан"
    fi

    step "Применяю рис"
    # apply.py разворачивает rice.toml во все конфиги; живую сессию он трогает,
    # только если она есть (DISPLAY).
    run "$REPO/vxwm/rice/apply.py" || die "apply.py не отработал"
}

# --------------------------------------------------------------- вход
install_sddm() {
    if [ "$DO_SDDM" != 1 ]; then
        warn "экран входа пропущен"
        return 0
    fi
    step "Экран входа SDDM"
    # Вопрос только во втором режиме: на чистой системе менеджера входа всё
    # равно нет, а здесь мы меняем то, через что человек попадает в систему.
    if ! ask_step "Включить sddm и поставить тему? Это сменит экран входа"; then
        warn "экран входа оставлен как был"
        return 0
    fi

    # Сессия в списке менеджера входа. Путь абсолютный: sddm запускает Exec
    # с системным PATH, и короткого имени из ~/.local/bin он не найдёт.
    local desktop=/usr/share/xsessions/vxwm.desktop
    if [ "$DRY" = 1 ]; then
        printf '    %s[dry]%s записал бы %s\n' "$Y" "$N" "$desktop"
    else
        sudo install -d /usr/share/xsessions
        sed "s|@SESSION@|$BIN/vxwm-session|" "$REPO/x11/vxwm.desktop.in" \
            | sudo tee "$desktop" >/dev/null
        ok "$desktop"
    fi

    run sudo bash "$REPO/sddm/install.sh" || warn "тема входа не поставилась"
}

# -------------------------------------------------------------- итоги
summary() {
    step "Готово"

    # То, от чего отказались во втором режиме, должно вернуться человеку
    # инструкцией: молчание он прочтёт как "и так заработает".
    if [ -n "$SKIPPED_SESSION_FILES" ]; then
        cat <<EOF
    ${Y}Ты оставил свои${N}:$SKIPPED_SESSION_FILES
    Чтобы сессия запускалась, в ~/.xinitrc должно быть:

        exec $BIN/vxwm-session

    а переменные Qt/GTK лежат в $REPO/x11/xprofile -- перенеси нужное к себе.

EOF
    fi
    if [ "$MODE" = existing ]; then
        cat <<EOF
    ${Y}Раскладку не трогал${N}: рис пользуется системной. Включить свою --
    в ~/.config/vxwm-rice/rice.toml:

        [input]
        layout = "us,ru"
        options = "grp:alt_shift_toggle"

EOF
    fi

    cat <<EOF
    Дальше:
      1. Выйди из сессии и войди заново (или перезагрузись) -- рис
         поднимается из ~/.xinitrc, а PATH -- из профиля оболочки.
      2. Тема:            Mod+W        (или vxwm-rice-apply --set-theme abyss)
      3. Настройки риса:  Mod+Shift+W
      4. Все хоткеи:      docs/keybindings.md

    Гибернация (Mod+Shift+F4 -> «Гибернация») требует swap. Отдельным шагом,
    потому что трогает fstab и параметры ядра:

        sudo bash vxwm/hibernate-setup.sh

    Видеокарта NVIDIA: обязательно прочти docs/nvidia.md -- без пары
    настроек композитор будет рвать картинку и ронять анимации.
EOF
}

main() {
    printf '%sLoad of Reger%s -- установка из %s\n' "$B" "$N" "$REPO"
    [ "$DRY" = 1 ] && warn "холостой прогон: ничего не изменится"
    choose_mode
    disclaimer
    preflight
    # Выбор групп -- до первого изменения в системе: он решает не только что
    # ставить из пакетов, но и какие шаги вообще делать (сборка picom, sddm).
    select_groups
    install_packages
    install_aur_helper
    install_vxwm
    install_vxbar
    install_picom
    link_scripts
    # Комфорт -- до файлов входа: там мы, возможно, меняем оболочку входа, а
    # блок с PATH пишется как раз в профиль той оболочки, что стоит в passwd.
    setup_comfort
    setup_session_files
    bootstrap_rice
    install_sddm
    summary
}

main "$@"
