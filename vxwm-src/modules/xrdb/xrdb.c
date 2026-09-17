/* Один ресурс из базы в буфер цвета. Отсутствие ресурса -- не ошибка: цвет
 * остаётся тем, что зашит в config.h, и WM работает даже без Xresources. */
void
xrdbcolor(XrmDatabase db, const char *name, char *dst, size_t len)
{
  char *type;
  XrmValue val;

  if (!XrmGetResource(db, name, "*", &type, &val) || !val.addr)
    return;
  /* Цвет уходит в XftColorAllocName, и мусор оттуда возвращается ошибкой
   * разбора на каждой перерисовке. Дешевле проверить форму здесь. */
  if (strnlen(val.addr, 8) != 7 || val.addr[0] != '#')
    return;
  for (int i = 1; i <= 6; i++)
    if (!isxdigit((unsigned char)val.addr[i]))
      return;
  snprintf(dst, len, "%s", val.addr);
}

/* Целое из базы с зажимом в разумные пределы. Ресурс правит человек или
 * приложение настроек, и опечатка в одну цифру не должна уводить рамку в
 * пол-экрана: зажать дешевле, чем потом искать, куда делись окна. */
void
xrdbint(XrmDatabase db, const char *name, unsigned int *dst,
        unsigned int lo, unsigned int hi)
{
  char *type, *end;
  XrmValue val;
  long v;

  if (!XrmGetResource(db, name, "*", &type, &val) || !val.addr)
    return;
  errno = 0;
  v = strtol(val.addr, &end, 10);
  /* Хвост после числа означает мусор вроде "10px": лучше оставить прежнее
   * значение, чем молча понять его как 10. */
  if (errno || end == val.addr || *end != '\0')
    return;
  if (v < (long)lo)
    v = lo;
  if (v > (long)hi)
    v = hi;
  *dst = (unsigned int)v;
}

/* Читаем RESOURCE_MANAGER с root-окна сами, а не через
 * XResourceManagerString: Xlib запоминает эту строку при открытии дисплея и
 * после xrdb -merge отдавала бы старую -- тема не менялась бы никогда.
 * Стоковый модуль здесь ещё и открывал второе соединение с X на каждый вызов. */
void
loadxrdb(void)
{
  Atom type;
  int format;
  unsigned long items, after;
  unsigned char *prop = NULL;
  XrmDatabase db;

  if (XGetWindowProperty(dpy, root, XA_RESOURCE_MANAGER, 0, 64L * 1024, False,
                         XA_STRING, &type, &format, &items, &after, &prop) != Success)
    return;
  if (!prop)
    return;
  if (!(db = XrmGetStringDatabase((char *)prop))) {
    XFree(prop);
    return;
  }
  xrdbcolor(db, "vxwm.normfg",     norm_fg,     sizeof norm_fg);
  xrdbcolor(db, "vxwm.normbg",     norm_bg,     sizeof norm_bg);
  xrdbcolor(db, "vxwm.normborder", norm_border, sizeof norm_border);
  xrdbcolor(db, "vxwm.selfg",      sel_fg,      sizeof sel_fg);
  xrdbcolor(db, "vxwm.selbg",      sel_bg,      sizeof sel_bg);
  xrdbcolor(db, "vxwm.selborder",  sel_border,  sizeof sel_border);
  /* Верхние границы -- не вкус, а предел вменяемости: на 4K-мониторе зазор
   * в 200px ещё оставляет окна видимыми, а больше уже нет. */
  xrdbint(db, "vxwm.gappx",    &gappx,    0, 200);
  xrdbint(db, "vxwm.borderpx", &borderpx, 0, 32);
  XrmDestroyDatabase(db);
  XFree(prop);
}

/* Применить всё, что пришло из Xresources: цвета -- пересборкой схем,
 * числа -- рассылкой по живым мониторам и окнам. Глобальных gappx/borderpx
 * недостаточно: зазор хранится у каждого монитора, а толщина рамки -- у
 * каждого окна, и оба копируются из глобальных только при создании. */
void
reloadcolors(void)
{
  Monitor *m;
  Client *c;
  XWindowChanges wc;
  size_t i;

  for (i = 0; i < LENGTH(colors); i++) {
    /* drw_scm_create выделяет массив заново; старый освобождаем, иначе
     * каждая смена темы подтекала бы на схему. */
    free(scheme[i]);
    scheme[i] = drw_scm_create(drw, colors[i], 3);
  }
  /* Рамки красят focus/unfocus, и после смены темы их никто не позовёт:
   * обходим клиентов сами, иначе новый цвет доехал бы до окна только когда
   * его в следующий раз тронут фокусом. */
  for (m = mons; m; m = m->next) {
    m->gappx = gappx;
    for (c = m->clients; c; c = c->next) {
      XSetWindowBorder(dpy, c->win,
          scheme[c == selmon->sel ? SchemeSel : SchemeNorm][ColBorder].pixel);
      if (c->bw != (int)borderpx) {
        c->bw = borderpx;
        wc.border_width = c->bw;
        XConfigureWindow(dpy, c->win, CWBorderWidth, &wc);
        /* Клиент считает свой размер сам и о смене рамки узнаёт только из
         * ConfigureNotify -- без него терминал остался бы с прежней сеткой. */
        configure(c);
      }
    }
    arrange(m);
  }
  drawbars();
}

/* Ручная перезагрузка на хоткей. Обычно не нужна -- vxwm сам ловит смену
 * RESOURCE_MANAGER в propertynotify, -- но полезна, когда Xresources правили
 * в файле и перечитали чем-то, что свойство не трогает. */
void
xrdb(const Arg *arg)
{
  loadxrdb();
  reloadcolors();
}
