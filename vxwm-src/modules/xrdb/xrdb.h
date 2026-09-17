#include <X11/Xresource.h>

/* Цвета темы приходят из X resources, а не из config.h: так theme.sh меняет
 * тему одним xrdb, без make и без sudo. Значения в config.h остаются
 * запасными -- на случай пустого ~/.Xresources.
 *
 * Имена ресурсов:
 *   цвета  -- vxwm.normfg, vxwm.normbg, vxwm.normborder,
 *             vxwm.selfg, vxwm.selbg, vxwm.selborder
 *   числа  -- vxwm.gappx (зазор между окнами), vxwm.borderpx (толщина рамки) */

static void loadxrdb(void);
static void reloadcolors(void);
static void xrdbcolor(XrmDatabase db, const char *name, char *dst, size_t len);
static void xrdbint(XrmDatabase db, const char *name, unsigned int *dst,
                    unsigned int lo, unsigned int hi);
static void xrdb(const Arg *arg);
