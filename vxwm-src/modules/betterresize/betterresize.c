#if TILED_RESIZE
/* Плиточный ресайз: тянем не окно, а границу между окнами.
 *
 * Прежнее поведение -- выбросить окно во float, как только его потянули, --
 * оставляло раскладку в покое: соседи не двигались, а окно оказывалось поверх
 * них. Здесь вместо этого правятся два числа раскладки:
 *
 *   mfact -- граница между колонками. Её тянут за боковой край окна;
 *   cfact -- доля окна в своей колонке. Её тянут за верхний или нижний край,
 *            и ровно столько, сколько прибавилось одному, отнимается у соседа.
 *
 * Всё считается от положения указателя, а не от накопленной дельты: граница
 * идёт ровно под курсором и не уезжает от него за длинный драг.
 *
 * Возвращает 0, если тянуть нечего: нет соседа с этой стороны, окно
 * единственное, раскладка не плиточная. Тогда вызывающий делает то же, что и
 * раньше.
 */
static int
tiledresize(Client *c, int left, int right, int top, int bottom, int px, int py)
{
	Monitor *m = c->mon;
	Client *i, *prev = NULL, *next = NULL;
	unsigned int n = 0, idx = 0, seen = 0;
	int master, ux, uw, midgap, pairtop, pairbot, want, pairpx;
	float total, f;
#if GAPS
	unsigned int gpx = m->gappx;
#else
	unsigned int gpx = 0;
#endif

	if (c->isfloating || !m->lt[m->sellt]->arrange || m->lt[m->sellt]->arrange != tile)
		return 0;

	for (i = nexttiled(m->clients); i; i = nexttiled(i->next), n++)
		if (i == c)
			idx = n;
	if (n < 2)
		return 0;
	master = idx < m->nmaster;

	/* Соседи -- только по своей колонке: окно master и окно stack стоят рядом
	 * по горизонтали, и общей горизонтальной границы у них нет. */
	for (i = nexttiled(m->clients); i; i = nexttiled(i->next), seen++) {
		if ((seen < m->nmaster) != (master != 0))
			continue;
		if (seen < idx)
			prev = i;
		else if (seen > idx && !next)
			next = i;
	}

	ux = m->wx + gpx;
	uw = m->ww - 2 * gpx;
	midgap = m->nmaster ? gpx : 0;

	/* --- вертикальная граница между колонками --- */
	if ((left || right) && m->nmaster && n > m->nmaster) {
		/* Тянуть имеет смысл только за тот край, который и есть граница:
		 * у master это правый край, у stack -- левый. Внешние края упираются
		 * в кромку экрана, там двигать нечего. */
		if ((master && right) || (!master && left)) {
			f = (float)(px - ux) / (float)(uw - midgap);
			if (f < 0.05)
				f = 0.05;
			if (f > 0.95)
				f = 0.95;
			m->mfact = f;
			return 1;
		}
	}

	/* --- горизонтальная граница между соседями по колонке --- */
	if (top && prev) {
		/* Пара -- сосед сверху и мы: их общая высота никуда не денется,
		 * граница внутри неё едет за курсором. */
		pairtop = prev->y;
		pairbot = c->y + HEIGHT(c);
		want = pairbot - py;          /* сколько достанется нам */
	} else if (bottom && next) {
		pairtop = c->y;
		pairbot = next->y + HEIGHT(next);
		want = py - pairtop;
	} else {
		return 0;
	}

	pairpx = pairbot - pairtop - gpx;   /* зазор между окнами долям не достаётся */
	if (pairpx <= 0)
		return 0;
	/* Меньше этого окно превращается в полоску, из которой его не вытянуть
	 * обратно: край становится недоступен курсору. */
	if (want < 2 * bh)
		want = 2 * bh;
	if (want > pairpx - 2 * bh)
		want = pairpx - 2 * bh;

	i = (top && prev) ? prev : next;
	total = c->cfact + i->cfact;
	c->cfact = total * (float)want / (float)pairpx;
	i->cfact = total - c->cfact;
	return 1;
}
#endif

void
resizemouse(const Arg *arg)
{
    Client *c;
    Monitor *m;
    XEvent ev;
#if LOCK_MOVE_RESIZE_REFRESH_RATE
    Time lasttime = 0;
#endif
    if (!(c = selmon->sel) || c->isfullscreen)
        return;

    restack(selmon);

    int orig_x = c->x;
    int orig_y = c->y;
    int orig_w = c->w;
    int orig_h = c->h;

    int rx, ry;
    Window junkwin;
    int junk_signed;
    unsigned int junk;
    if (!XQueryPointer(dpy, c->win, &junkwin, &junkwin, &junk_signed, &junk_signed, &rx, &ry, &junk))
        return;
    int left   = rx < orig_w / 3;
    int right  = rx > orig_w * 2 / 3;
    int top    = ry < orig_h / 3;
    int bottom = ry > orig_h * 2 / 3;
#if BR_CHANGE_CURSOR
    Cursor cur;
    if (top && left)         cur = cursor[CurNW]->cursor;
    else if (top && right)   cur = cursor[CurNE]->cursor;
    else if (bottom && left) cur = cursor[CurSW]->cursor;
    else if (bottom && right) cur = cursor[CurSE]->cursor;
    else if (top)            cur = cursor[CurN]->cursor;
    else if (bottom)         cur = cursor[CurS]->cursor;
    else if (left)           cur = cursor[CurW]->cursor;
    else if (right)          cur = cursor[CurE]->cursor;
    else                     cur = cursor[CurResize]->cursor; // fallback
#else 
    Cursor cur = cursor[CurResize]->cursor; 
#endif
    if (XGrabPointer(dpy, root, False, MOUSEMASK, GrabModeAsync, GrabModeAsync,
                     None, cur, CurrentTime) != GrabSuccess)
        return;
    do {
        XMaskEvent(dpy, MOUSEMASK|ExposureMask|SubstructureRedirectMask, &ev);

        if (ev.type == MotionNotify) {
#if LOCK_MOVE_RESIZE_REFRESH_RATE
            if (ev.xmotion.time - lasttime <= (1000 / refreshrate))
                continue;
            lasttime = ev.xmotion.time;
#endif
            int dx = ev.xmotion.x_root - (orig_x + rx);
            int dy = ev.xmotion.y_root - (orig_y + ry);

            int nx = orig_x;
            int ny = orig_y;
            int nw = orig_w;
            int nh = orig_h;

            if (left)   nw = orig_w - dx;
            else if (right) nw = orig_w + dx;

            if (top)    nh = orig_h - dy;
            else if (bottom) nh = orig_h + dy;

            int min_w = MAX(1, c->minw);
            int min_h = MAX(1, c->minh);
            
            if (nw < min_w) nw = min_w;
            if (nh < min_h) nh = min_h;

            if (left)   nx = orig_x + (orig_w - nw);
            if (top)    ny = orig_y + (orig_h - nh);

            int dx_final = nw - orig_w;
            int dy_final = nh - orig_h;
#if TILED_RESIZE
            /* Плиточное окно тянем как границу раскладки: правим mfact и
             * cfact и перекладываем всё заново. Соседи двигаются вместе с
             * границей, а само окно остаётся в тайлинге. */
            if (!c->isfloating && selmon->lt[selmon->sellt]->arrange
                && tiledresize(c, left, right, top, bottom,
                               ev.xmotion.x_root, ev.xmotion.y_root)) {
                arrange(selmon);
                drawbar(selmon);
                continue;
            }
#endif
#if !RESIZING_WINDOWS_IN_ALL_LAYOUTS_FLOATS_THEM
            if (!c->isfloating && selmon->lt[selmon->sellt]->arrange &&
              (abs(dx_final) > snap || abs(dy_final) > snap)) {
#else
            if (!c->isfloating && (abs(dx_final) > snap || abs(dy_final) > snap)) {
#endif
              if (nx >= selmon->wx && nx + nw <= selmon->wx + selmon->ww &&
                ny >= selmon->wy && ny + nh <= selmon->wy + selmon->wh) {

                togglefloating(NULL);

                orig_x = c->x;
                orig_y = c->y;
                orig_w = c->w;
                orig_h = c->h;
              }
            }
#if USE_RESIZECLIENT_FUNC           
           resizeclient(c, nx, ny, nw, nh);
#else
           resize(c, nx, ny, nw, nh, 1);
#endif
           drawbar(selmon); //fix for issue 1
        }
    } while (ev.type != ButtonRelease);

    XUngrabPointer(dpy, CurrentTime);
    while (XCheckMaskEvent(dpy, EnterWindowMask, &ev));

    if ((m = recttomon(c->x, c->y, c->w, c->h)) != selmon) {
        sendmon(c, m);
        selmon = m;
        focus(NULL);
    }
#if ENHANCED_TOGGLE_FLOATING && RESTORE_SIZE_AND_POS_ETF
  c->wasmanuallyedited = 1;
  if (c->isfloating) {
    c->sfx = c->x;
    c->sfy = c->y;
    c->sfw = c->w;
    c->sfh = c->h;
  }
#endif
}
