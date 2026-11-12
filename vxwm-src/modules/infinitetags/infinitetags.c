#include <math.h>

int
getcurrenttag(Monitor *m) {
    unsigned int i;
    for (i = 0; i < LENGTH(tags) && !(m->tagset[m->seltags] & (1 << i)); i++);
    return i < LENGTH(tags) ? i : 0;
}

void
homecanvas(const Arg *arg) {
    if (!selmon || selmon->lt[selmon->sellt]->arrange != NULL)
        return;

    Client *c;
    int tagidx = getcurrenttag(selmon);
    int cx = selmon->canvas[tagidx].cx;
    int cy = selmon->canvas[tagidx].cy;
    
    for (c = selmon->clients; c; c = c->next) {
        if (c->tags & (1 << tagidx)) {
            if (c->saved_cw > 0 && c->saved_ch > 0) {
                c->w = c->saved_cw;
                c->h = c->saved_ch;
            }
            c->x -= cx;
            c->y -= cy;
            XMoveResizeWindow(dpy, c->win, c->x, c->y, c->w, c->h);
            configure(c);
        }
    }
    
    selmon->canvas[tagidx].cx = 0;
    selmon->canvas[tagidx].cy = 0;
    selmon->canvas[tagidx].scale = 1.0f;
    drawbar(selmon); 
    XSync(dpy, False);
}

void
zoomcanvas(const Arg *arg)
{
    if (!selmon)
        return;

    /* Зум камеры работает ТОЛЬКО в режиме холста (плавающий режим, arrange == NULL) */
    if (selmon->lt[selmon->sellt]->arrange != NULL)
        return;

    if (selmon->sel && selmon->sel->isfullscreen)
        return;

    int tagidx = getcurrenttag(selmon);
    float old_scale = selmon->canvas[tagidx].scale;
    if (old_scale <= 0.05f)
        old_scale = 1.0f;

    float new_scale = old_scale;

    if (arg && arg->i > 0) {
        /* Zoom in (приближение камеры к холсту) */
        new_scale = old_scale * 1.25f;
    } else if (arg && arg->i < 0) {
        /* Zoom out (отдаление камеры от холста) */
        new_scale = old_scale / 1.25f;
    } else {
        /* Reset zoom to 100% (1.0) */
        new_scale = 1.0f;
    }

    /* Clamp zoom range between 0.25x and 3.0x */
    if (new_scale < 0.25f)
        new_scale = 0.25f;
    if (new_scale > 3.0f)
        new_scale = 3.0f;

    if (fabsf(new_scale - old_scale) < 0.005f)
        return;

    /* Pivot point: pointer position if on monitor, else center of screen */
    int ox = selmon->wx + selmon->ww / 2;
    int oy = selmon->wy + selmon->wh / 2;

    Window dummy;
    int px, py, di;
    unsigned int dui;
    if (XQueryPointer(dpy, root, &dummy, &dummy, &px, &py, &di, &di, &dui)) {
        if (px >= selmon->wx && px < selmon->wx + selmon->ww &&
            py >= selmon->wy && py < selmon->wy + selmon->wh) {
            ox = px;
            oy = py;
        }
    }

    float ratio = new_scale / old_scale;

    Client *c;
    for (c = selmon->clients; c; c = c->next) {
        if (ISVISIBLE(c)) {
            if (c->saved_cw <= 0 || c->saved_ch <= 0) {
                c->saved_cw = (int)roundf(c->w / old_scale);
                c->saved_ch = (int)roundf(c->h / old_scale);
                c->saved_cx = c->x + selmon->canvas[tagidx].cx;
                c->saved_cy = c->y + selmon->canvas[tagidx].cy;
                c->was_on_canvas = 1;
            }

            int win_cx = c->x + (c->w + 2 * c->bw) / 2;
            int win_cy = c->y + (c->h + 2 * c->bw) / 2;

            int n_cx = ox + (int)roundf((win_cx - ox) * ratio);
            int n_cy = oy + (int)roundf((win_cy - oy) * ratio);

            int nw, nh;
            if (fabsf(new_scale - 1.0f) < 0.01f) {
                nw = c->saved_cw;
                nh = c->saved_ch;
            } else {
                nw = (int)roundf(c->saved_cw * new_scale);
                nh = (int)roundf(c->saved_ch * new_scale);
            }

            if (nw < 60) nw = 60;
            if (nh < 40) nh = 40;

            int nx = n_cx - (nw + 2 * c->bw) / 2;
            int ny = n_cy - (nh + 2 * c->bw) / 2;

            c->x = nx;
            c->y = ny;
            c->w = nw;
            c->h = nh;

            XMoveResizeWindow(dpy, c->win, c->x, c->y, c->w, c->h);
            configure(c);
        }
    }

    selmon->canvas[tagidx].cx = ox + (int)roundf((selmon->canvas[tagidx].cx - ox) * ratio);
    selmon->canvas[tagidx].cy = oy + (int)roundf((selmon->canvas[tagidx].cy - oy) * ratio);
    selmon->canvas[tagidx].scale = new_scale;

    drawbar(selmon);
    XSync(dpy, False);
}

void
movecanvas(const Arg *arg)
{
	if (selmon->lt[selmon->sellt]->arrange != NULL)
		return;
  if (selmon->sel && selmon->sel->isfullscreen)
    return;

	int tagidx = getcurrenttag(selmon);
	int dx = 0, dy = 0;
	
#ifndef MOVE_CANVAS_STEP
#define MOVE_CANVAS_STEP 120
#endif

	switch(arg->i) {
    case 0: dx = -MOVE_CANVAS_STEP; break;
		case 1: dx =  MOVE_CANVAS_STEP; break;
    case 2: dy = -MOVE_CANVAS_STEP; break;
		case 3: dy =  MOVE_CANVAS_STEP; break;
	}

	selmon->canvas[tagidx].cx -= dx;
	selmon->canvas[tagidx].cy -= dy;

	Client *c;
	for (c = selmon->clients; c; c = c->next) {
		if (ISVISIBLE(c)) {
			c->x -= dx;
			c->y -= dy;
			XMoveWindow(dpy, c->win, c->x, c->y);
		}
	}

	drawbar(selmon);
}

void
manuallymovecanvas(const Arg *arg) {
    if (selmon->lt[selmon->sellt]->arrange != NULL)
      return;
    if (selmon->sel && selmon->sel->isfullscreen)
      return;
    int start_x, start_y;
    Window dummy;
    int di;
    unsigned int dui;
    int tagidx = getcurrenttag(selmon);
#if LOCK_MOVE_RESIZE_REFRESH_RATE
    Time lasttime = 0;
#endif

    if (!XQueryPointer(dpy, root, &dummy, &dummy, &start_x, &start_y, &di, &di, &dui))
        return;
    
    if (XGrabPointer(dpy, root, False, MOUSEMASK, GrabModeAsync, GrabModeAsync,
        None, cursor[CurMove]->cursor, CurrentTime) != GrabSuccess)
        return;
    
    XEvent ev;
    do {
        XMaskEvent(dpy, MOUSEMASK|ExposureMask|SubstructureRedirectMask, &ev);
        
        switch (ev.type) {
        case MotionNotify:
        {
#if LOCK_MOVE_RESIZE_REFRESH_RATE
            if ((ev.xmotion.time - lasttime) <= (1000 / refreshrate))
                continue;
            lasttime = ev.xmotion.time;
#endif
            int nx = ev.xmotion.x - start_x;
            int ny = ev.xmotion.y - start_y;
            
            for (Client *c = selmon->clients; c; c = c->next) {
                if (c->tags & (1 << tagidx)) {
                    c->x += nx;
                    c->y += ny;
                    XMoveWindow(dpy, c->win, c->x, c->y);
                }
            }
            
            selmon->canvas[tagidx].cx += nx;
            selmon->canvas[tagidx].cy += ny;
            drawbar(selmon); 
            start_x = ev.xmotion.x;
            start_y = ev.xmotion.y;
        }   break;
        }
    } while (ev.type != ButtonRelease);
    
    XUngrabPointer(dpy, CurrentTime);
}

void
save_canvas_positions(Monitor *m) {
    Client *c;
    int tagidx = getcurrenttag(m);
    
    m->canvas[tagidx].saved_cx = m->canvas[tagidx].cx;
    m->canvas[tagidx].saved_cy = m->canvas[tagidx].cy;

    float scale = m->canvas[tagidx].scale;
    if (scale <= 0.05f)
        scale = 1.0f;

    for (c = m->clients; c; c = c->next) {
        if (ISVISIBLE(c)) {
            c->saved_cx = c->x + m->canvas[tagidx].cx;
            c->saved_cy = c->y + m->canvas[tagidx].cy;
            if (fabsf(scale - 1.0f) < 0.01f || c->saved_cw <= 0) {
                c->saved_cw = (int)roundf(c->w / scale);
                c->saved_ch = (int)roundf(c->h / scale);
            }
            c->was_on_canvas = 1;
        }
    }
}

void
restore_canvas_positions(Monitor *m) {
    Client *c;
    int tagidx = getcurrenttag(m);

    m->canvas[tagidx].cx = m->canvas[tagidx].saved_cx;
    m->canvas[tagidx].cy = m->canvas[tagidx].saved_cy;

    float scale = m->canvas[tagidx].scale;
    if (scale <= 0.05f)
        scale = 1.0f;

    for (c = m->clients; c; c = c->next) {
        if (ISVISIBLE(c) && c->was_on_canvas) {
            c->isfloating = 1;
            
            int target_x = c->saved_cx - m->canvas[tagidx].cx;
            int target_y = c->saved_cy - m->canvas[tagidx].cy;

            c->x = target_x;
            c->y = target_y;
            if (c->saved_cw > 0 && c->saved_ch > 0) {
                if (fabsf(scale - 1.0f) < 0.01f) {
                    c->w = c->saved_cw;
                    c->h = c->saved_ch;
                } else {
                    c->w = (int)roundf(c->saved_cw * scale);
                    c->h = (int)roundf(c->saved_ch * scale);
                }
            } else {
                if (c->w <= 0) c->w = 640;
                if (c->h <= 0) c->h = 480;
                c->saved_cw = (int)roundf(c->w / scale);
                c->saved_ch = (int)roundf(c->h / scale);
            }

            if (c->w < 60) c->w = 60;
            if (c->h < 40) c->h = 40;

            XMoveResizeWindow(dpy, c->win, target_x, target_y, c->w, c->h);

            configure(c);
        }
    }
    XSync(dpy, False);
}

void
centerwindow(const Arg *arg)
{
    Client *c = (arg && arg->v) ? (Client *)arg->v : selmon->sel;

    if (!c || !c->mon || c->mon->lt[c->mon->sellt]->arrange != NULL)
        return;

    Monitor *m = c->mon;
    int tagidx = getcurrenttag(m);

    int screen_center_x = m->wx + (m->ww / 2);
    int screen_center_y = m->wy + (m->wh / 2);

    int win_center_x = c->x + (c->w + 2 * c->bw) / 2;
    int win_center_y = c->y + (c->h + 2 * c->bw) / 2;

    int dx = screen_center_x - win_center_x;
    int dy = screen_center_y - win_center_y;

    if (dx == 0 && dy == 0)
        return;

    Client *tmp;
    for (tmp = m->clients; tmp; tmp = tmp->next) {
        if (ISVISIBLE(tmp)) {
            tmp->x += dx;
            tmp->y += dy;
            XMoveWindow(dpy, tmp->win, tmp->x, tmp->y);
        }
    }

    m->canvas[tagidx].cx += dx;
    m->canvas[tagidx].cy += dy;

    drawbar(m);
}
