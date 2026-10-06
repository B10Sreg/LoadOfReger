/* See LICENSE file for copyright and license details. */
#include <X11/XF86keysym.h>

/* === Внешний вид === */
static unsigned int borderpx        = 2;  /* живое значение приходит из vxwm.borderpx */
static unsigned int gappx           = 10; /* живое значение приходит из vxwm.gappx */
static const unsigned int snap      = 32;
static const int showbar            = 0; // внешний бар vxbar рисует всё сам
static const int topbar             = 1;

/* Включено антиалиасинг и автохинтинг для идеального рендера шрифтов */
static const char *fonts[]          = { "JetBrainsMono Nerd Font:size=10:antialias=true:autohint=true" };
static const char dmenufont[]       = "JetBrainsMono Nerd Font:size=10";

static const unsigned int refreshrate = 144; 
static const int lockfullscreen       = 1;

/* Цвета-запасные. Живые значения приходят из X resources (vxwm.normfg и
 * далее): их кладёт theme.sh через xrdb, и vxwm подхватывает их на лету, без
 * пересборки. То, что написано здесь, используется только если ресурса нет. */
static char norm_fg[16] = "#948d84"; /* Тёплый серый текст */
static char norm_bg[16] = "#1c1a18"; /* Тёплый ганметал (Фон) */
static char norm_border[16] = "#2b2825"; /* Рамка неактивных окон */
static char sel_fg[16] = "#f5efe6"; /* Тёплый белый для фокуса */
static char sel_bg[16] = "#2b2825"; /* Выделение */
static char sel_border[16] = "#c9b28a"; /* Бронзовое серебро (Акценты) */

static char *colors[][3] = {
    /*               fg         bg         border   */
    [SchemeNorm] = { norm_fg,   norm_bg,   norm_border },
    [SchemeSel]  = { sel_fg,    sel_bg,    sel_border  },
};

/* === Теги === */
static const char *tags[] = { "1:dev", "2:web", "3:sys", "4:doc", "5:ai", "6:media", "7", "8", "9" };

/* === Правила окон === */
static const Rule rules[] = {
    /* class      instance    title       tags mask     isfloating   monitor */
    { "kitty",    NULL,       NULL,       0,            0,           -1 },
    { "firefox",  NULL,       NULL,       1 << 1,       0,           -1 }, /* Firefox всегда на 2-м теге */
    { "Thunar",   NULL,       NULL,       0,            0,           -1 },
    /* Правило для локального ИИ: всегда плавающее окно по центру */
    { "local-ai", NULL,       "Local AI", 0,            1,           -1 }, 
    /* Выпадающий терминал: всегда плавающий, тег ему назначает scratchpad.sh */
    { "scratchpad", NULL,     NULL,       0,            1,           -1 },
    /* Varwin 3D Client: плавающее окно, чтобы тайлинг не ломал swapchain Vulkan */
    { "VarwinClient", NULL,   NULL,       0,            1,           -1 },
};

/* === Layouts === */
static const float mfact     = 0.55;
static const int nmaster     = 1;
/* CRITICAL FIX: 0 отключает привязку к сетке шрифта, устраняя микро-зазоры между окнами */
static const int resizehints = 0; 

static const Layout layouts[] = {
    { "[]=",      tile },
    { "><>",      NULL },
    { "[M]",      monocle },
};

/* === Модификатор === */
/* Mod1Mask = Alt. Mod4Mask = Super (Windows). Оставляем твой Alt, но для Pro-сетапов чаще юзают Super */
#define MODKEY Mod1Mask

#define TAGKEYS(KEY,TAG) \
    { MODKEY,                       KEY,      view,           {.ui = 1 << TAG} }, \
    { MODKEY|ControlMask,           KEY,      toggleview,     {.ui = 1 << TAG} }, \
    { MODKEY|ShiftMask,             KEY,      tag,            {.ui = 1 << TAG} }, \
    { MODKEY|ControlMask|ShiftMask, KEY,      toggletag,      {.ui = 1 << TAG} },

/* === Команды === */
static char dmenumon[2] = "0";
/* Rofi теперь вызывается с жестко зашитым путем к скомпилированной теме */
static const char *dmenucmd[]   = { "rofi", "-show", "drun", NULL };
static const char *termcmd[]    = { "kitty", NULL };
static const char *browsercmd[] = { "firefox", NULL };
static const char *filecmd[]  = { "thunar", NULL };
static const char *shotcmd[]  = { "flameshot", "gui", NULL };
/* === Автозапуск системного скрипта === */
static const char *autostart[] = {
    "$HOME/.config/vxwm/autostart.sh", NULL,
    NULL /* Финальный маркер завершения массива */
};
/* VibeAI Panel: Запуск плавающего инстанса Kitty с Ollama */
static const char *aicmd[]    = { "kitty", "--class", "local-ai", "-T", "Local AI", "-e", "ollama", "run", "llama3", NULL };

/* Мультимедиа (Управление звуком и медиа-плеером) */
static const char *volup[]      = { "wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", "5%+", NULL };
static const char *voldown[]    = { "wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", "5%-", NULL };
static const char *voltoggle[]  = { "wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle", NULL };
static const char *brightup[]   = { "brightnessctl", "set", "+5%", NULL };
static const char *brightdown[] = { "brightnessctl", "set", "5%-", NULL };
static const char *nexttrack[]  = { "playerctl", "next", NULL };
static const char *playpause[]  = { "playerctl", "play-pause", NULL };
static const char *prevtrack[]  = { "playerctl", "previous", NULL };

static const char *themecmd[]   = { "vxwm-theme.sh", NULL };
static const char *barcfgcmd[]  = { "vxbar-settings", NULL };
/* Показать/скрыть внешний бар: vxbar по SIGUSR2 сам прячет окно и снимает струт */
static const char *bartogglecmd[] = { "pkill", "-USR2", "-x", "vxbar", NULL };

/* Выход из сессии. Гибернация возвращает вкладки и несохранённое, поэтому она
 * в меню первая; сам выбор -- в powermenu.sh, чтобы хоткей был один. */
static const char *powercmd[]     = { "vxwm-powermenu.sh", NULL };
static const char *lockcmd[]      = { "vxwm-power.sh", "lock", NULL };
/* Выпадающий терминал: скрипт сам решает, показать его или убрать. */
static const char *scratchcmd[]   = { "vxwm-scratchpad.sh", NULL };
/* Последнее уведомление обратно на экран: без этого пропущенное теряется. */
static const char *dunsthistcmd[] = { "dunstctl", "history-pop", NULL };

/* === Клавиши === */
static Key keys[] = {
    /* Модификатор + Кнопка             Действие        Аргумент */
    
    /* 1. Запуск Приложений */
    { MODKEY,              XK_Return, spawn,          {.v = termcmd } },
    { MODKEY,              XK_p,      spawn,          {.v = dmenucmd } },
    { MODKEY,              XK_b,      spawn,          {.v = browsercmd } },
    { MODKEY,              XK_e,      spawn,          {.v = filecmd } },
    { MODKEY,              XK_a,      spawn,          {.v = aicmd } },       /* Вызов VibeAI (local-ai) */
    
    /* 2. Скрипты и Утилиты */
    /* Без Shift: XK_t занят tile-layout, поэтому смена темы висит на Mod+W */
    { MODKEY,              XK_w,      spawn,          {.v = themecmd } },
    { MODKEY|ShiftMask,    XK_w,      spawn,          {.v = barcfgcmd } },   /* Настройки vxbar */
    { 0,                   XK_Print,  spawn,          {.v = shotcmd } },
    { MODKEY,              XK_Escape, spawn,          {.v = scratchcmd } },  /* Выпадающий терминал */
    { MODKEY,              XK_n,      spawn,          {.v = dunsthistcmd } },/* Вернуть уведомление */
    { MODKEY|ShiftMask,    XK_l,      spawn,          {.v = lockcmd } },     /* Заблокировать экран */
    
    /* 3. Управление окнами (Фокус и закрытие) */
    /* Перемещение окон по сетке тайлинга с клавиатуры */
    { MODKEY,              XK_c,      killclient,     {0} },
    /* Mod+` прячет внешний бар. Встроенный бар выключен (showbar = 0),
       поэтому togglebar здесь ничего бы не сделал — шлём сигнал vxbar. */
    { MODKEY,              XK_grave,  spawn,          {.v = bartogglecmd } },
    { MODKEY,              XK_j,      focusstack,     {.i = +1 } },
    { MODKEY,              XK_k,      focusstack,     {.i = -1 } },
    { MODKEY|ShiftMask,    XK_Return, zoom,           {0} },                 /* Перенос окна в мастер-зону (Zoom) */
    
    /* 4. Layouts (Раскладки) */
    { MODKEY,              XK_t,      setlayout,      {.v = &layouts[0]} },  /* Tile */
    { MODKEY,              XK_f,      setlayout,      {.v = &layouts[1]} },  /* Floating */
    { MODKEY,              XK_m,      setlayout,      {.v = &layouts[2]} },  /* Monocle */
    { MODKEY,              XK_space,  setlayout,      {0} },                 /* Переключение предыдущего layout */
    { MODKEY|ShiftMask,    XK_space,  togglefloating, {0} },
    
    /* 5. Размер мастер-окна */
    { MODKEY,              XK_h,      setmfact,       {.f = -0.05} },
    { MODKEY,              XK_l,      setmfact,       {.f = +0.05} },

    /* 5.1 Живая регулировка отступов между окнами (gaps) */
    { MODKEY,              XK_minus,  setgaps,        {.i = -2} },
    { MODKEY,              XK_equal,  setgaps,        {.i = +2} },
    { MODKEY|ShiftMask,    XK_equal,  setgaps,        {.i = 0} },   /* сброс в 0 */
    
    /* 6. Канвас (VXWM Specific) */
    { MODKEY|ControlMask,  XK_h,      movecanvas,     {.i = 0} },
    { MODKEY|ControlMask,  XK_l,      movecanvas,     {.i = 1} },
    { MODKEY|ControlMask,  XK_k,      movecanvas,     {.i = 2} },
    { MODKEY|ControlMask,  XK_j,      movecanvas,     {.i = 3} },
    
    /* 7. Мультимедиа (Привязка системных Fn-кнопок) */
    { 0, XF86XK_AudioRaiseVolume,     spawn,          {.v = volup } },
    { 0, XF86XK_AudioLowerVolume,     spawn,          {.v = voldown } },
    { 0, XF86XK_AudioMute,            spawn,          {.v = voltoggle } },
    { 0, XF86XK_MonBrightnessUp,      spawn,          {.v = brightup } },
    { 0, XF86XK_MonBrightnessDown,    spawn,          {.v = brightdown } },
    { 0, XF86XK_AudioPlay,            spawn,          {.v = playpause } },
    { 0, XF86XK_AudioNext,            spawn,          {.v = nexttrack } },
    { 0, XF86XK_AudioPrev,            spawn,          {.v = prevtrack } },

    /* 8. Теги (Воркспейсы) */
    TAGKEYS(XK_1, 0) TAGKEYS(XK_2, 1) TAGKEYS(XK_3, 2)
    TAGKEYS(XK_4, 3) TAGKEYS(XK_5, 4) TAGKEYS(XK_6, 5)
    TAGKEYS(XK_7, 6) TAGKEYS(XK_8, 7) TAGKEYS(XK_9, 8)
    
    /* 9. Выход */
    { MODKEY,              XK_F4,     quit,           {0} },
    { MODKEY|ShiftMask,    XK_F4,     spawn,          {.v = powercmd } },    /* Гибернация/выключение */
};

/* === Мышь === */
/* === Мышь === */
static Button buttons[] = {
    /* Нажатия на панель (если она есть) */
    { ClkLtSymbol,          0,              Button1, setlayout,      {0} },
    
    /* Управление окнами (Над окном) */
    { ClkClientWin,         MODKEY,         Button1, movemouse,      {0} }, /* Alt + ЛКМ = Таскать окно */
    { ClkClientWin,         MODKEY,         Button3, resizemouse,    {0} }, /* Alt + ПКМ = Менять размер окна */
    { ClkClientWin,         MODKEY|ShiftMask, Button2, togglefloating, {0} }, /* Alt + Shift + Колесико = Сделать плавающим */
    
    /* Управление холстом (Canvas) */
    /* Теперь Alt + Клик колесиком тянет холст, даже если ты над окном! */
    { ClkClientWin,         MODKEY,         Button2, manuallymovecanvas, {0} },
    /* И то же самое, если кликаешь по пустому фону */
    { ClkRootWin,           MODKEY,         Button2, manuallymovecanvas, {0} },
};
