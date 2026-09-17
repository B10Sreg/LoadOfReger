#include <time.h>

/* Дедлайны текущего перехода, монотонные миллисекунды; 0 -- дела нет.
 * Монитор не запоминаем намеренно: при перенастройке экранов Monitor могут
 * удалить у нас под ногами, а обход всех mons стоит копейки. */
static long ts_in_at;
static long ts_out_at;

/* Номер первого поднятого бита -- это и есть индекс тега. Своя функция, а не
 * getcurrenttag(): та живёт в infinitetags и считает по монитору, а здесь надо
 * уметь спросить про произвольный набор, в том числе про прошлый. */
int
tagslide_tagof(unsigned int tagset)
{
	unsigned int i;

	for (i = 0; i < LENGTH(tags); i++)
		if (tagset & (1 << i))
			return (int)i;
	return 0;
}

/* Ось берём у бара: вертикальный бар -- окна ездят по вертикали. Если бара нет
 * или он молчит, остаёмся на горизонтали. */
int
tagslide_axis(void)
{
#if EXTERNAL_BAR
	return dockvertical();
#else
	return 0;
#endif
}

long
tagslide_now(void)
{
	struct timespec ts;

	clock_gettime(CLOCK_MONOTONIC, &ts);
	return ts.tv_sec * 1000L + ts.tv_nsec / 1000000L;
}

/* Расставить видимые окна монитора со смещением off по оси и пометить их.
 * Позиции клиентов (c->x, c->y) не трогаем -- двигаем только сами окна,
 * поэтому раскладка после перехода остаётся ровно той, что посчитал arrange(). */
void
tagslide_shift(Monitor *m, int vertical, int off, int state)
{
	Client *c;

	for (c = m->clients; c; c = c->next)
		if (ISVISIBLE(c)) {
			XMoveWindow(dpy, c->win,
				c->x + (vertical ? 0 : off),
				c->y + (vertical ? off : 0));
			c->tsstate = state;
		}
	/* Flush, а не Sync: ответ сервера нам не нужен, а round-trip тут лишний --
	 * дальше всё равно возвращаемся в цикл событий. */
	XFlush(dpy);
}

/* Убрать уехавшее окно с глаз. При WINDOWMAP это размапливание с иконификацией
 * -- ровно то, что сделал бы showhide, не помешай мы ему меткой TS_OUT. Без
 * WINDOWMAP прятать нечем, кроме как увести окно совсем далеко за край. */
static void
tagslide_hide(Client *c)
{
#if WINDOWMAP
	if (c->ismapped) {
		window_unmap(dpy, c->win, root, 1);
		c->ismapped = 0;
	}
#else
	XMoveWindow(dpy, c->win, WIDTH(c) * -2, c->y);
#endif
}

/* Отъезд. Вызывается ДО смены тега, пока ISVISIBLE ещё показывает старый
 * набор. Экран листается в сторону sign, значит сами окна идут навстречу. */
void
tagslide_out(Monitor *m, int sign)
{
	int vertical;

	/* Предыдущий переход мог не доиграть -- переключение тега приходит и
	 * быстрее, чем за TAG_SLIDE_MS. Доводим его до конца одним махом, иначе
	 * метки TS_* перепутаются между двумя переходами. */
	tagslide_flush();

	vertical = tagslide_axis();
	tagslide_shift(m, vertical, -sign * (vertical ? m->mh : m->mw), TS_OUT);
	/* Окна уехали, но остались замаплены: пока picom рисует отъезд, они ему
	 * нужны живыми. Размапит их tagslide_tick(). */
	ts_out_at = tagslide_now() + TAG_SLIDE_MS;
}

/* Приезд. Вызывается ПОСЛЕ arrange(): окна нового тега уже замаплены и стоят
 * на своих местах -- отсюда и уводим их за край, чтобы приехать обратно. */
void
tagslide_in(Monitor *m, int sign)
{
	int vertical = tagslide_axis();

	tagslide_shift(m, vertical, sign * (vertical ? m->mh : m->mw), TS_IN);
	ts_in_at = tagslide_now() + TS_IN_DELAY_MS;
}

/* Доиграть всё немедленно: приехавшие -- на места, уехавшие -- размапить. */
void
tagslide_flush(void)
{
	Monitor *m;
	Client *c;

	if (!ts_in_at && !ts_out_at)
		return;

	for (m = mons; m; m = m->next)
		for (c = m->clients; c; c = c->next) {
			if (c->tsstate == TS_IN)
				XMoveWindow(dpy, c->win, c->x, c->y);
			else if (c->tsstate == TS_OUT && !ISVISIBLE(c))
				tagslide_hide(c);
			c->tsstate = TS_IDLE;
		}
	ts_in_at = ts_out_at = 0;
	XFlush(dpy);
}

/* Шаг планировщика: делаем то, чему подошёл срок. Вызывается из run() и ничего
 * не ждёт сам -- ожиданием заведует select() там же. */
void
tagslide_tick(void)
{
	Monitor *m;
	Client *c;
	long now = tagslide_now();
	int moved = 0;

	if (ts_in_at && now >= ts_in_at) {
		for (m = mons; m; m = m->next)
			for (c = m->clients; c; c = c->next)
				if (c->tsstate == TS_IN) {
					XMoveWindow(dpy, c->win, c->x, c->y);
					c->tsstate = TS_IDLE;
					moved = 1;
				}
		ts_in_at = 0;
	}

	if (ts_out_at && now >= ts_out_at) {
		for (m = mons; m; m = m->next)
			for (c = m->clients; c; c = c->next)
				if (c->tsstate == TS_OUT) {
					/* Тег могли переключить обратно, пока окно ехало: тогда
					 * оно снова видимо, его уже расставил arrange, и трогать
					 * его нельзя -- только снять метку. */
					if (!ISVISIBLE(c)) {
						tagslide_hide(c);
						moved = 1;
					}
					c->tsstate = TS_IDLE;
				}
		ts_out_at = 0;
	}

	if (moved)
		XFlush(dpy);
}

/* Сколько миллисекунд можно спать до ближайшего дела; -1 -- дел нет, можно
 * блокироваться на XNextEvent. */
long
tagslide_wait_ms(void)
{
	long now, wait = -1;

	if (!ts_in_at && !ts_out_at)
		return -1;

	now = tagslide_now();
	if (ts_in_at)
		wait = ts_in_at - now;
	if (ts_out_at && (wait < 0 || ts_out_at - now < wait))
		wait = ts_out_at - now;
	return wait < 0 ? 0 : wait;
}

/* Идёт ли переход. Нужно enternotify: проезжающие под курсором окна сыплют
 * EnterNotify, и без этой проверки фокус во время перехода скачет по тем
 * окнам, над которыми случайно оказалась мышь. */
int
tagslide_busy(void)
{
	return ts_in_at || ts_out_at;
}
