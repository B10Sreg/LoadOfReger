static Dock ebdocks[EB_MAX_DOCKS];
static int ebndocks = 0;
static Atom ebatom[EbLast];

void
externalbar_setup(void)
{
	ebatom[EbWindowTypeDock] = XInternAtom(dpy, "_NET_WM_WINDOW_TYPE_DOCK", False);
	ebatom[EbStrut] = XInternAtom(dpy, "_NET_WM_STRUT", False);
	ebatom[EbStrutPartial] = XInternAtom(dpy, "_NET_WM_STRUT_PARTIAL", False);
	ebatom[EbVertical] = XInternAtom(dpy, "_VXBAR_VERTICAL", False);

	/* дописываем к уже выставленному в setup() списку -- иначе панели,
	 * которые проверяют _NET_SUPPORTED, решат что струты не поддержаны */
	XChangeProperty(dpy, root, netatom[NetSupported], XA_ATOM, 32, PropModeAppend,
		(unsigned char *) ebatom, EbVertical);
}

/* _NET_WM_WINDOW_TYPE -- список атомов, а не один: getatomprop() читает только
 * первый и к тому же требует Client*, которого у дока нет. */
int
isdock(Window w)
{
	int format, isd = 0;
	unsigned long i, nitems, dl;
	unsigned char *p = NULL;
	Atom da;

	if (XGetWindowProperty(dpy, w, netatom[NetWMWindowType], 0L, 32L, False, XA_ATOM,
		&da, &format, &nitems, &dl, &p) != Success || !p)
		return 0;
	if (format == 32)
		for (i = 0; i < nitems; i++)
			if (((Atom *)p)[i] == ebatom[EbWindowTypeDock]) {
				isd = 1;
				break;
			}
	XFree(p);
	return isd;
}

Dock *
wintodock(Window w)
{
	int i;

	for (i = 0; i < ebndocks; i++)
		if (ebdocks[i].win == w)
			return &ebdocks[i];
	return NULL;
}

/* Возвращает 1, если значения струтов изменились -- вызывающий решает,
 * надо ли перекладывать окна. */
int
updatedockstrut(Dock *d)
{
	int format, i, changed = 0, val[4] = { 0, 0, 0, 0 };
	unsigned long nitems, dl;
	unsigned char *p = NULL;
	Atom da;

	if (XGetWindowProperty(dpy, d->win, ebatom[EbStrutPartial], 0L, 12L, False,
		XA_CARDINAL, &da, &format, &nitems, &dl, &p) == Success && p && nitems >= 4) {
		for (i = 0; i < 4; i++)
			val[i] = (int)((long *)p)[i];
	} else if (p) {
		XFree(p);
		p = NULL;
	}
	if (!p && XGetWindowProperty(dpy, d->win, ebatom[EbStrut], 0L, 4L, False,
		XA_CARDINAL, &da, &format, &nitems, &dl, &p) == Success && p && nitems >= 4) {
		for (i = 0; i < 4; i++)
			val[i] = (int)((long *)p)[i];
	}
	if (p)
		XFree(p);

	for (i = 0; i < 4; i++)
		if (d->strut[i] != val[i]) {
			d->strut[i] = val[i];
			changed = 1;
		}
	return changed;
}

void
managedock(Window w, XWindowAttributes *wa)
{
	Dock *d;

	if (ebndocks >= EB_MAX_DOCKS) {
		XMapWindow(dpy, w);
		return;
	}
	d = &ebdocks[ebndocks++];
	d->win = w;
	d->x = wa->x;
	d->y = wa->y;
	d->w = wa->width;
	d->h = wa->height;
	memset(d->strut, 0, sizeof d->strut);
	updatedockstrut(d);

	/* док не получает границы, не участвует в фокусе и не тайлится;
	 * StructureNotify нужен чтобы поймать его resize, Property -- смену струтов */
	XSelectInput(dpy, w, StructureNotifyMask|PropertyChangeMask);
	XMapRaised(dpy, w);
	refreshstruts();
}

void
unmanagedock(Window w)
{
	int i, j;

	for (i = 0; i < ebndocks; i++)
		if (ebdocks[i].win == w) {
			for (j = i; j < ebndocks - 1; j++)
				ebdocks[j] = ebdocks[j + 1];
			ebndocks--;
			refreshstruts();
			return;
		}
}

/* Вычитает из рабочей области монитора струты тех доков, что лежат на нём.
 * Вызывается в хвосте updatebarpos(), уже после учёта встроенного бара. */
void
applystruts(Monitor *m)
{
	int i, left = 0, right = 0, top = 0, bottom = 0;
	Dock *d;

	for (i = 0; i < ebndocks; i++) {
		d = &ebdocks[i];
		if (recttomon(d->x, d->y, MAX(d->w, 1), MAX(d->h, 1)) != m)
			continue;
		left = MAX(left, d->strut[EbStrutLeft]);
		right = MAX(right, d->strut[EbStrutRight]);
		top = MAX(top, d->strut[EbStrutTop]);
		bottom = MAX(bottom, d->strut[EbStrutBottom]);
	}
	if (!(left || right || top || bottom))
		return;

	/* струты задаются от края экрана, а m->wx/wy уже могли сместиться под
	 * встроенный бар -- поэтому режем по пересечению, а не вычитаем вслепую */
	if (top > 0 && m->wy < m->my + top) {
		m->wh -= (m->my + top) - m->wy;
		m->wy = m->my + top;
	}
	if (bottom > 0 && m->wy + m->wh > m->my + m->mh - bottom)
		m->wh = (m->my + m->mh - bottom) - m->wy;
	if (left > 0 && m->wx < m->mx + left) {
		m->ww -= (m->mx + left) - m->wx;
		m->wx = m->mx + left;
	}
	if (right > 0 && m->wx + m->ww > m->mx + m->mw - right)
		m->ww = (m->mx + m->mw - right) - m->wx;

	m->wh = MAX(m->wh, (int)bh);
	m->ww = MAX(m->ww, (int)bh);
}

void
refreshstruts(void)
{
	Monitor *m;

	for (m = mons; m; m = m->next)
		updatebarpos(m);
	arrange(NULL);
}

/* Док лежит поверх обычных окон: в тайлинге это обеспечивают струты, но в
 * режиме холста окна двигаются свободно и легко заезжают под бар. Поэтому
 * после каждого подъёма клиента возвращаем доки на самый верх. Полноэкранное
 * окно -- исключение, его накрывать баром нельзя, см. вызов в restack(). */
void
raisedocks(void)
{
	int i;

	for (i = 0; i < ebndocks; i++)
		XRaiseWindow(dpy, ebdocks[i].win);
}

/* Ориентация бара: её публикует сам vxbar свойством _VXBAR_VERTICAL на своём
 * окне, чтобы настройка не дублировалась в конфиге WM. Хватает первого дока,
 * который её выставил: панель на экране одна. */
int
dockvertical(void)
{
	int i, format, vert = 0;
	unsigned long nitems, dl;
	unsigned char *p = NULL;
	Atom da;

	for (i = 0; i < ebndocks; i++) {
		if (XGetWindowProperty(dpy, ebdocks[i].win, ebatom[EbVertical], 0L, 1L, False,
			XA_CARDINAL, &da, &format, &nitems, &dl, &p) != Success || !p)
			continue;
		if (nitems >= 1)
			vert = *(long *)p != 0;
		XFree(p);
		p = NULL;
		if (vert)
			break;
	}
	return vert;
}

/* Доки, поднявшиеся до запуска vxwm (или пережившие его рестарт). */
void
scandocks(void)
{
	unsigned int i, num;
	Window d1, d2, *wins = NULL;
	XWindowAttributes wa;

	if (!XQueryTree(dpy, root, &d1, &d2, &wins, &num))
		return;
	for (i = 0; i < num; i++) {
		if (!XGetWindowAttributes(dpy, wins[i], &wa) || wa.override_redirect)
			continue;
		if (wa.map_state == IsViewable && !wintoclient(wins[i]) && isdock(wins[i]))
			managedock(wins[i], &wa);
	}
	if (wins)
		XFree(wins);
}

/* Возвращает 1, если событие поглощено модулем. */
int
ebclientmessage(XClientMessageEvent *cme)
{
#if EWMH_TAGS
	int idx;

	if (cme->window == root && cme->message_type == netatom[NetCurrentDesktop]) {
		idx = (int)cme->data.l[0];
		if (idx >= 0 && idx < (int)LENGTH(tags))
			view(&(Arg){ .ui = 1 << idx });
		return 1;
	}
#endif
	return 0;
}

int
ebpropertynotify(XPropertyEvent *ev)
{
	Dock *d;

	if (!(d = wintodock(ev->window)))
		return 0;
	if (ev->atom == ebatom[EbStrut] || ev->atom == ebatom[EbStrutPartial])
		if (updatedockstrut(d))
			refreshstruts();
	return 1;
}
