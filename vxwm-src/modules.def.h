#pragma once

/* vxwm compile-time options */

/* KILLER FEATURES */
#define INFINITE_TAGS 1
/* 
Most tiling window managers (like the default dwm) treat your screen like a slide-projector. You click a button, and the current "slide" is swapped for another. If an window is off-screen, it doesn't exist.

With infinite tags enabled, vxwm treats your screen like a magnifying glass over a giant wooden desk.

1. The Canvas is Infinite
Your windows aren't "on" tags. They are placed on a massive, invisible surface. Your monitor is just a small window through which you look at that surface.

2. Move the View, Not Just the Windows
Instead of managing "layers" or "hidden states," you manage position.

Want more space? Slide the view over.
Can't find a window? Switch your focus to it, and the world slides until that window is right under your nose.
Lost? Hit the "homecanvas" keybind to snap your view back to the start.
Even though this sounds complex, it is actually pretty lightweight ~250 l.o.c, and is very easy to use.
 
 */
#define IT_SHOW_COORDINATES_IN_BAR 0 // Shows your coordinates in the bar.



/* Move/Resize */
#define BETTER_RESIZE 1 // Enables 8 sided window resize.
#define BR_CHANGE_CURSOR 1 // When resizing windows while having BETTER_RESIZE set to 1, the cursor will change depending on from which side you are resizing.
#define LOCK_MOVE_RESIZE_REFRESH_RATE 1 // Recommended to use on every pc, because cpu (software) rendered apps like ST will lag when resizing even if you have a good pc.
#define USE_RESIZECLIENT_FUNC 0 // Use resizeclient function instead of resize function which ignores window's resize hints, not recommended.
#define RESIZING_WINDOWS_IN_ALL_LAYOUTS_FLOATS_THEM 0 // yeah.
#define TILED_RESIZE 1 // Перетаскивание края плиточного окна двигает границу раскладки, а не выбрасывает окно во float: соседи подстраиваются. 0 -- прежнее поведение.
#define MOVE_RESIZE_WITH_KEYBOARD 1 // Allows to move and resize windows with keyboard.

/* Kind of eye candy */
#define GAPS 1 // Gaps support.
#define XRDB 1 // Цвета из X resources: смена темы без пересборки.



/* Tagging */
#define TAG_TO_TAG 0 // If you switch to tag where you already are, it'll switch to previous tag.
#define SLOWER_TAGS_ANIMATION 1 // This slows down tags animation speed, which results in smoother tags animations (requires picom to see any difference).
#define WINDOWMAP 1
/* This makes the windows get mapped or unmapped in X11. This results in certain
   behaviour being enabled, some examples are: fix issues with some applications 
   losing input forever after a tag change or when you use a compositor like picom, 
   your windows will fade in and out when you switch dwm tags. */

#define PDWM_LIKE_TAGS_ANIMATION 0
/* This makes function showhide be like in pdwm, if using a compositor like picom,
   this results in an alternative tags animation. */



/* Bar */
#define ALT_CENTER_OF_BAR_COLOR 0 // changes center of bar color to a dark color.
#define BAR_HEIGHT 0 // Support for changing bar height.
#define BAR_PADDING 0 // Support for changing the bar padding.
#define OCCUPIED_TAGS_DECORATION 0 // This provides the ability to use an alternative text for tags which contain at least one window aka occupied tags.



/* Warp to client */
#define WARP_TO_CLIENT 0 // Includes the warp to client function needed for all options below.
#define WARP_TO_CENTER_OF_NEW_WINDOW 0 // Warps the cursor to center of the new window.
#define WARP_TO_CENTER_OF_PREVIOUS_WINDOW 0 // Warps cursor to center of the previous window after closing a window.
#define WARP_TO_CENTER_OF_ZOOMED_WINDOW 0 // Warps cursor to center of the window that was zoomed using zoom function.
#define WARP_TO_CENTER_OF_WINDOW_AFFECTED_BY_INCNMASTER 0 // Warps the cursor to center of the window that gets affected in use of incnmaster.
#define WARP_TO_CENTER_OF_WINDOW_AFFECTED_BY_ENHANCED_TOGGLE_FLOATING 0 // Warps cursor to center of the window that was affected by using enhancedtogglefloating function.
#define WARP_TO_CENTER_OF_WINDOW_AFFECTED_BY_FOCUSSTACK 0 // Warps cursor to center of the window that was focused by using focusstack function. 
#define WARP_TO_CENTER_OF_WINDOW_MOVED_BY_KEYBOARD 0 // Warps cursor to center of the window that is moved by moveresize function. 



/* Misc */
#define AUTOSTART 1 // Support for vxwm being able to start apps defined in config.h in startup.
#define FULLSCREEN 0 // Support for toggling fullscreen.
#define MOVE_IN_TILED 1 // Support for moving windows in tiled mode.
#define EWMH_TAGS 1 // Support for EWMH tags, recommended if you want to use external bars with less pain
#define EXTERNAL_BAR 1 // Docks (_NET_WM_WINDOW_TYPE_DOCK) reserve space via _NET_WM_STRUT_PARTIAL; needed for vxbar.
#define TAG_SLIDE 1 // Окна уезжают/приезжают при смене тега. Ось -- по ориентации бара, направление -- по номеру тега.
#define TAG_SLIDE_MS 300 // Сколько держать уехавшие окна замапленными, мс. Это НЕ длительность
                         // анимации: кадры считает композитор своей пружиной, а WM только
                         // ставит окна за край и обратно (modules/tagslide/tagslide.h).
                         // Число должно быть чуть больше времени пружины picom, иначе окно
                         // размапится на полпути и переход оборвётся. 0 -- листание выключено.
                         //
                         // Прошлая версия рисовала переход сама, nanosleep по кадрам, и
                         // держала XGrabServer на стыке: экран замирал на ~40 мс (замерено
                         // записью экрана; с выключенным picom пауза та же), и всё это время
                         // не обрабатывался ввод. Теперь ни захвата, ни сна в обработчике.
                         //
                         // Чего picom при этом НЕ умеет: своих ключей animation-for-next-tag
                         // и animation-for-prev-tag. Они включаются эвристикой в win.c
                         // (pending_g.y > 0 && g.y < 0 && ...), рассчитанной на ванильный dwm,
                         // который прячет окна сдвигом за верхний край. У нас WINDOWMAP 1
                         // (его требует INFINITE_TAGS), showhide размапливает окна, а у
                         // незамапленного окна picom не обновляет w->g -- условие не
                         // выполняется никогда. Поэтому листание собрано на обычной пружине
                         // (ANIM_IN_TAG): окно едет, оставаясь замапленным.
#define DIRECTIONAL_FOCUS 0 // yeah.
#define DIRECTIONAL_MOVE 1 
/* Makes moving windows in tiled layout possible with keyboard and it is directional,
   bind for move is in #if MOVE_RESIZE_WITH_KEYBOARD section which makes it depending on 
   MOVE_RESIZE_WITH_KEYBOARD at the first sight but, it doesn't. You can bind the movedir
   function manually which makes it independent like this:
#if DIRECTIONAL_MOVE
  { MODKEY|ALTERNATE_MODKEY, XK_Left,  movedir, {.i = 0} }, // Left
  { MODKEY|ALTERNATE_MODKEY, XK_Right, movedir, {.i = 1} }, // Right
  { MODKEY|ALTERNATE_MODKEY, XK_Up,    movedir, {.i = 2} }, // Up
  { MODKEY|ALTERNATE_MODKEY, XK_Down,  movedir, {.i = 3} }, // Down
#endif
*/



/* Floating */
/* Recomended to use with ALWAYS_CENTER_NEW_FLOATING_WINDOWS set to 1. */ 

#define ALWAYS_CENTER_NEW_FLOATING_WINDOWS 0 // so, basically, it does what it says.

#define FLOATING_LAYOUT_FLOATS_WINDOWS 0 
/* By default, in floating layout, windows appear to be floating, but, for dwm,
   they are not. Because of this, when switching to tiled layout after floating
   layout, windows will be tiled, enable this if you don't want that behaviour. */

#define ENHANCED_TOGGLE_FLOATING 0 
/* Support for enhanced toggle floating, when toggling floating window will 
   resize to its natural size, and in floating layout, window will be tiled. 
   REQUIRES "FLOATING_LAYOUT_FLOATS_WINDOWS" SET TO 1 TO WORK PROPERLY. */

#define RESTORE_SIZE_AND_POS_ETF 0 // Restore previous size and position of window when toggling floating 



/* Dependency list */
/* INFINITE_TAGS requires WINDOWMAP, please set WINDOWMAP to 1, if not, it will be automatically enabled.
 * ENHANCED_TOGGLE_FLOATING requires FLOATING_LAYOUT_FLOATS_WINDOWS, please set FLOATING_LAYOUT_FLOATS_WINDOWS to 1, if not, it will be automatically enabled.  */
