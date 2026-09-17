void
window_set_state(Display *dpy, Window win, long state)
{
	long data[] = { state, None };

	XChangeProperty(dpy, win, wmatom[WMState], wmatom[WMState], 32,
		PropModeReplace, (unsigned char*)data, 2);
}

void
window_map(Display *dpy, Client *c, int deiconify)
{
	Window win = c->win;
	/* Заведомо за верхним краем: высота монитора плюс высота самого окна. */
	int off = c->mon->mh + HEIGHT(c);

	if (deiconify)
		window_set_state(dpy, win, NormalState);

	/* Окно появляется за краем экрана и только потом встаёт на место.
	 *
	 * Ради picom: анимацию появления он запускает только там, где у окна
	 * сменилась геометрия -- весь блок с init_animation в win.c висит под
	 * проверкой WIN_FLAGS_SIZE_STALE|WIN_FLAGS_POSITION_STALE, а сам по себе
	 * MapNotify этих флагов не ставит. Мапили сразу на место -- координаты те
	 * же, что были до размапливания, ConfigureNotify не приходит, флага нет,
	 * появления не видно.
	 *
	 * Речь про окна, которые всплывают вне листания тегов: их вернули с
	 * другого тега, отдали текущему через toggletag и так далее. Листание
	 * рисуется иначе, оно уводит окна за край само (tagslide.h).
	 *
	 * Куда именно мапить, на картинку не влияет: picom всё равно перебивает
	 * стартовую геометрию своей (win.c, init_animation -> w->g), а промежуточный
	 * кадр не виден -- showhide держит XGrabServer на всё время обхода. Важно
	 * только то, что позиция отличается от конечной. */
	XMoveResizeWindow(dpy, c->win, c->x, c->y - off, c->w, c->h);
	XSetInputFocus(dpy, win, RevertToPointerRoot, CurrentTime);
	XMapWindow(dpy, win);
	XMoveWindow(dpy, c->win, c->x, c->y);
  focus(NULL);
}

void
window_unmap(Display *dpy, Window win, Window root, int iconify)
{
	static XWindowAttributes ca, ra;

	XGetWindowAttributes(dpy, root, &ra);
	XGetWindowAttributes(dpy, win, &ca);

	/* Prevent UnmapNotify events */
	XSelectInput(dpy, root, ra.your_event_mask & ~SubstructureNotifyMask);
	XSelectInput(dpy, win, ca.your_event_mask & ~StructureNotifyMask);

	XUnmapWindow(dpy, win);
  focus(NULL);
	if (iconify)
		window_set_state(dpy, win, IconicState);

	XSelectInput(dpy, root, ra.your_event_mask);
	XSelectInput(dpy, win, ca.your_event_mask);
}
