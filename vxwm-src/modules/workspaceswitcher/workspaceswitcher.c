#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <time.h>
#include <unistd.h>
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <X11/extensions/Xcomposite.h>
#include <cairo/cairo.h>
#include <cairo/cairo-xlib.h>

#include "workspaceswitcher.h"

#define WS_PREV_W 100
#define WS_PREV_H 56

static cairo_surface_t *ws_snapshots[32] = { NULL };
static unsigned int ws_snapshot_valid = 0;

static void
ws_cairo_rounded_rect(cairo_t *cr, double x, double y, double w, double h, double r)
{
	cairo_new_sub_path(cr);
	cairo_arc(cr, x + w - r, y + r, r, -M_PI / 2.0, 0);
	cairo_arc(cr, x + w - r, y + h - r, r, 0, M_PI / 2.0);
	cairo_arc(cr, x + r, y + h - r, r, M_PI / 2.0, M_PI);
	cairo_arc(cr, x + r, y + r, r, M_PI, 3.0 * M_PI / 2.0);
	cairo_close_path(cr);
}

static const char *
ws_get_icon(int i, const char *name)
{
	static const char *default_icons[] = {
		"", "󰈹", "", "󰈙", "󰚩", "", "󰒋", "󰒋", "󰒋"
	};
	if (strstr(name, "web") || strstr(name, "net") || strstr(name, "surf"))
		return "󰈹";
	if (strstr(name, "dev") || strstr(name, "code") || strstr(name, "src"))
		return "";
	if (strstr(name, "sys") || strstr(name, "term") || strstr(name, "cli"))
		return "";
	if (strstr(name, "doc") || strstr(name, "file") || strstr(name, "text"))
		return "󰈙";
	if (strstr(name, "ai")  || strstr(name, "bot")  || strstr(name, "llm"))
		return "󰚩";
	if (strstr(name, "media") || strstr(name, "mus") || strstr(name, "video"))
		return "";
	if (strstr(name, "chat") || strstr(name, "msg") || strstr(name, "talk"))
		return "󰍡";
	if (strstr(name, "game"))
		return "󰊴";
	if (i >= 0 && i < (int)(sizeof(default_icons)/sizeof(default_icons[0])))
		return default_icons[i];
	return "󰒋";
}

static void
ws_draw_vector_window(cairo_t *cr, Client *c, double wx, double wy, double ww, double wh)
{
	ws_cairo_rounded_rect(cr, wx, wy, ww, wh, 2.5);
	cairo_set_source_rgba(cr, 0.08, 0.09, 0.13, 0.96);
	cairo_fill(cr);

	/* Mini titlebar with micro traffic lights */
	if (wh >= 14 && ww >= 20) {
		ws_cairo_rounded_rect(cr, wx, wy, ww, 4.0, 2.0);
		cairo_set_source_rgba(cr, 0.15, 0.16, 0.22, 0.9);
		cairo_fill(cr);

		if (ww >= 26) {
			cairo_arc(cr, wx + 3.5, wy + 2.0, 0.8, 0, 2 * M_PI);
			cairo_set_source_rgb(cr, 0.95, 0.35, 0.35);
			cairo_fill(cr);
			cairo_arc(cr, wx + 6.0, wy + 2.0, 0.8, 0, 2 * M_PI);
			cairo_set_source_rgb(cr, 0.95, 0.75, 0.25);
			cairo_fill(cr);
			cairo_arc(cr, wx + 8.5, wy + 2.0, 0.8, 0, 2 * M_PI);
			cairo_set_source_rgb(cr, 0.25, 0.85, 0.45);
			cairo_fill(cr);
		}
	}

	/* Micro content lines */
	char class_name[32] = "";
	XClassHint ch = { NULL, NULL };
	if (XGetClassHint(dpy, c->win, &ch)) {
		if (ch.res_class) {
			strncpy(class_name, ch.res_class, sizeof(class_name) - 1);
			XFree(ch.res_class);
		}
		if (ch.res_name) XFree(ch.res_name);
	}
	for (char *p = class_name; *p; p++)
		if (*p >= 'A' && *p <= 'Z') *p = *p + ('a' - 'A');

	if (strstr(class_name, "kit") || strstr(class_name, "term") || strstr(class_name, "alacritty")) {
		cairo_set_source_rgba(cr, 0.35, 0.85, 0.55, 0.8);
		cairo_rectangle(cr, wx + 3, wy + 6, 4, 1.5);
		cairo_fill(cr);
		cairo_set_source_rgba(cr, 0.7, 0.7, 0.8, 0.35);
		if (ww > 14) cairo_rectangle(cr, wx + 9, wy + 6, ww - 12, 1.5);
		if (wh > 12) cairo_rectangle(cr, wx + 3, wy + 9.5, ww - 8, 1.5);
		if (wh > 16) cairo_rectangle(cr, wx + 3, wy + 13.0, (ww - 8) * 0.7, 1.5);
		cairo_fill(cr);
	} else if (strstr(class_name, "fire") || strstr(class_name, "chrom") || strstr(class_name, "web")) {
		cairo_set_source_rgba(cr, 0.20, 0.22, 0.30, 0.8);
		cairo_rectangle(cr, wx + 3, wy + 5, ww - 6, 2.5);
		cairo_fill(cr);
		if (wh > 14) {
			cairo_set_source_rgba(cr, 0.12, 0.13, 0.18, 0.7);
			cairo_rectangle(cr, wx + 3, wy + 9, ww - 6, wh - 11);
			cairo_fill(cr);
		}
	} else {
		cairo_set_font_size(cr, 9.0);
		cairo_set_source_rgba(cr, 0.65, 0.70, 0.80, 0.75);
		cairo_text_extents_t te;
		cairo_text_extents(cr, "󰍹", &te);
		cairo_move_to(cr, wx + (ww - te.width) / 2.0, wy + (wh + te.height) / 2.0);
		cairo_show_text(cr, "󰍹");
	}
}

void
workspaceswitcher_invalidate(unsigned int tagmask)
{
	ws_snapshot_valid &= ~tagmask;
}

void
workspaceswitcher_snapshot(void)
{
	if (!selmon || selmon->mw <= 0 || selmon->mh <= 0)
		return;

	int cur = 0;
	int ntags = (int)LENGTH(tags);
	for (int i = 0; i < ntags; i++) {
		if (selmon->tagset[selmon->seltags] & (1 << i)) {
			cur = i;
			break;
		}
	}
	if (cur < 0 || cur >= 32)
		return;

	if (!ws_snapshots[cur])
		ws_snapshots[cur] = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, WS_PREV_W, WS_PREV_H);

	cairo_t *scr = cairo_create(ws_snapshots[cur]);

	/* Clear */
	cairo_set_operator(scr, CAIRO_OPERATOR_CLEAR);
	cairo_paint(scr);
	cairo_set_operator(scr, CAIRO_OPERATOR_OVER);

	/* Stylized wallpaper background */
	cairo_pattern_t *pat = cairo_pattern_create_linear(0, 0, WS_PREV_W, WS_PREV_H);
	cairo_pattern_add_color_stop_rgb(pat, 0.0, 0.04, 0.05, 0.07);
	cairo_pattern_add_color_stop_rgb(pat, 1.0, 0.07, 0.05, 0.09);
	cairo_set_source(scr, pat);
	cairo_paint(scr);
	cairo_pattern_destroy(pat);

	/* Check if XComposite is available */
	int ev_base, err_base;
	int has_composite = XCompositeQueryExtension(dpy, &ev_base, &err_base);

	double sx = (double)(WS_PREV_W - 4) / (double)selmon->mw;
	double sy = (double)(WS_PREV_H - 4) / (double)selmon->mh;

	for (Client *c = selmon->clients; c; c = c->next) {
		if ((c->tags & (1 << cur)) && c->ismapped) {
			double wx = 2.0 + (c->x - selmon->mx) * sx;
			double wy = 2.0 + (c->y - selmon->my) * sy;
			double ww = c->w * sx;
			double wh = c->h * sy;
			if (ww < 10) ww = 10;
			if (wh < 8) wh = 8;

			int drawn = 0;
			if (has_composite) {
				XSetErrorHandler(xerrordummy);
				Pixmap px = XCompositeNameWindowPixmap(dpy, c->win);
				XSync(dpy, False);
				XSetErrorHandler(xerror);

				if (px) {
					XWindowAttributes wa;
					if (XGetWindowAttributes(dpy, c->win, &wa)) {
						cairo_surface_t *cs = cairo_xlib_surface_create(dpy, px, wa.visual, wa.width, wa.height);
						if (cairo_surface_status(cs) == CAIRO_STATUS_SUCCESS) {
							cairo_save(scr);
							ws_cairo_rounded_rect(scr, wx, wy, ww, wh, 2.5);
							cairo_clip(scr);
							cairo_translate(scr, wx, wy);
							cairo_scale(scr, ww / (double)wa.width, wh / (double)wa.height);
							cairo_set_source_surface(scr, cs, 0, 0);
							cairo_paint(scr);
							cairo_restore(scr);
							drawn = 1;
						}
						cairo_surface_destroy(cs);
					}
					XFreePixmap(dpy, px);
				}
			}

			if (!drawn)
				ws_draw_vector_window(scr, c, wx, wy, ww, wh);

			/* Window border */
			ws_cairo_rounded_rect(scr, wx, wy, ww, wh, 2.5);
			if (c == selmon->sel)
				cairo_set_source_rgba(scr, 0.40, 0.78, 0.98, 0.95);
			else
				cairo_set_source_rgba(scr, 0.25, 0.25, 0.32, 0.85);
			cairo_set_line_width(scr, 0.8);
			cairo_stroke(scr);
		}
	}

	cairo_destroy(scr);
	ws_snapshot_valid |= (1 << cur);
}

static void
ws_draw(cairo_t *cr, cairo_surface_t *surf, int win_w, int win_h, int ntags,
        int sel, int current_tag, int mod_is_super,
        const char tag_names[][32], const char tag_icons[][16],
        const char tag_apps[][64], const int tag_counts[])
{
	cairo_push_group(cr);

	/* 1. Clear transparent canvas */
	cairo_set_operator(cr, CAIRO_OPERATOR_CLEAR);
	cairo_paint(cr);

	/* 2. Main obsidian HUD container with subtle glass border */
	cairo_set_operator(cr, CAIRO_OPERATOR_OVER);
	ws_cairo_rounded_rect(cr, 1.0, 1.0, (double)win_w - 2.0, (double)win_h - 2.0, 16.0);
	cairo_set_source_rgba(cr, 0.045, 0.045, 0.055, 0.97);
	cairo_fill_preserve(cr);
	cairo_set_source_rgba(cr, 0.22, 0.22, 0.28, 0.90);
	cairo_set_line_width(cr, 1.5);
	cairo_stroke(cr);

	/* 3. Header: Title and navigation hint */
	cairo_select_font_face(cr, "JetBrainsMono Nerd Font", CAIRO_FONT_SLANT_NORMAL, CAIRO_FONT_WEIGHT_BOLD);
	cairo_set_font_size(cr, 11.0);
	cairo_set_source_rgb(cr, 0.55, 0.55, 0.62);
	cairo_move_to(cr, 24.0, 27.0);
	cairo_show_text(cr, "󰍹  WORKSPACE CAMERA VIEW");

	cairo_set_font_size(cr, 10.0);
	cairo_set_source_rgb(cr, 0.38, 0.38, 0.44);
	const char *hint = mod_is_super
		? "Tab / 󰁔  navigate   •   Release Super to switch   •   Esc to cancel"
		: "Tab / 󰁔  navigate   •   Release Alt to switch   •   Esc to cancel";
	cairo_text_extents_t he;
	cairo_text_extents(cr, hint, &he);
	cairo_move_to(cr, (double)win_w - he.width - 24.0, 27.0);
	cairo_show_text(cr, hint);

	/* 4. Render workspace cards */
	int pad = 16;
	int gap = 10;
	int avail_w = win_w - pad * 2 - (ntags - 1) * gap;
	int card_w = avail_w / ntags;
	int card_h = 142;
	int card_y = 42;

	for (int i = 0; i < ntags; i++) {
		int cx = pad + i * (card_w + gap);
		int is_sel = (i == sel);
		int is_cur = (i == current_tag);
		int is_empty = (tag_counts[i] == 0);

		/* Card background */
		ws_cairo_rounded_rect(cr, (double)cx, (double)card_y, (double)card_w, (double)card_h, 12.0);
		if (is_sel) {
			/* Selected workspace: elevated card with luminous glow outline */
			cairo_set_source_rgba(cr, 0.14, 0.15, 0.20, 0.98);
			cairo_fill_preserve(cr);
			cairo_set_source_rgba(cr, 0.95, 0.96, 1.0, 1.0);
			cairo_set_line_width(cr, 2.0);
			cairo_stroke(cr);

			/* Top glowing indicator pill */
			ws_cairo_rounded_rect(cr, (double)(cx + 18), (double)(card_y + 4), (double)(card_w - 36), 3.0, 1.5);
			cairo_set_source_rgb(cr, 0.58, 0.72, 0.98);
			cairo_fill(cr);
		} else if (!is_empty) {
			/* Occupied workspace */
			cairo_set_source_rgba(cr, 0.08, 0.08, 0.11, 0.88);
			cairo_fill_preserve(cr);
			cairo_set_source_rgba(cr, 0.22, 0.22, 0.28, 0.85);
			cairo_set_line_width(cr, 1.0);
			cairo_stroke(cr);
		} else {
			/* Empty workspace */
			cairo_set_source_rgba(cr, 0.05, 0.05, 0.07, 0.55);
			cairo_fill_preserve(cr);
			cairo_set_source_rgba(cr, 0.14, 0.14, 0.18, 0.50);
			cairo_set_line_width(cr, 1.0);
			cairo_stroke(cr);
		}

		/* Current workspace cyan indicator dot */
		if (is_cur) {
			cairo_arc(cr, (double)(cx + 12), (double)(card_y + 14), 3.0, 0, 2.0 * M_PI);
			cairo_set_source_rgb(cr, 0.38, 0.78, 0.98);
			cairo_fill(cr);
		}

		/* 1. Header: Icon + Tag Number + Name */
		char title_buf[48];
		if (strcmp(tag_names[i], "7") == 0 || strcmp(tag_names[i], "8") == 0 || strcmp(tag_names[i], "9") == 0)
			snprintf(title_buf, sizeof(title_buf), "%s  %d", tag_icons[i], i + 1);
		else
			snprintf(title_buf, sizeof(title_buf), "%s  %d • %s", tag_icons[i], i + 1, tag_names[i]);

		cairo_set_font_size(cr, 10.0);
		if (is_sel)
			cairo_set_source_rgb(cr, 0.98, 0.82, 0.38); /* Warm gold */
		else if (!is_empty)
			cairo_set_source_rgb(cr, 0.88, 0.89, 0.93);
		else
			cairo_set_source_rgb(cr, 0.35, 0.35, 0.40);

		cairo_text_extents_t te;
		cairo_text_extents(cr, title_buf, &te);
		cairo_move_to(cr, cx + (card_w - te.width) / 2.0, card_y + 23.0);
		cairo_show_text(cr, title_buf);

		/* 2. Middle: Camera View Mini-Display (16:9 Aspect Ratio) */
		int prev_w = card_w - 14;
		int prev_h = (prev_w * 9) / 16;
		int prev_x = cx + 7;
		int prev_y = card_y + 32;

		/* Bezel */
		ws_cairo_rounded_rect(cr, (double)prev_x, (double)prev_y, (double)prev_w, (double)prev_h, 5.0);
		cairo_set_source_rgba(cr, 0.02, 0.02, 0.03, 0.95);
		cairo_fill_preserve(cr);
		if (is_sel) {
			cairo_set_source_rgba(cr, 0.45, 0.75, 1.0, 0.95);
			cairo_set_line_width(cr, 1.4);
		} else {
			cairo_set_source_rgba(cr, 0.20, 0.22, 0.28, 0.80);
			cairo_set_line_width(cr, 0.8);
		}
		cairo_stroke(cr);

		/* Screen area */
		cairo_save(cr);
		ws_cairo_rounded_rect(cr, (double)(prev_x + 1), (double)(prev_y + 1), (double)(prev_w - 2), (double)(prev_h - 2), 4.0);
		cairo_clip(cr);

		if (!is_empty) {
			if ((ws_snapshot_valid & (1 << i)) && ws_snapshots[i]) {
				/* Live Captured Camera View */
				cairo_save(cr);
				cairo_translate(cr, prev_x + 1, prev_y + 1);
				cairo_scale(cr, (double)(prev_w - 2) / (double)WS_PREV_W, (double)(prev_h - 2) / (double)WS_PREV_H);
				cairo_set_source_surface(cr, ws_snapshots[i], 0, 0);
				cairo_paint(cr);
				cairo_restore(cr);
			} else {
				/* Vector Miniature Window Layout */
				cairo_pattern_t *pat = cairo_pattern_create_linear(prev_x, prev_y, prev_x + prev_w, prev_y + prev_h);
				cairo_pattern_add_color_stop_rgb(pat, 0.0, 0.04, 0.05, 0.07);
				cairo_pattern_add_color_stop_rgb(pat, 1.0, 0.07, 0.05, 0.09);
				cairo_set_source(cr, pat);
				cairo_paint(cr);
				cairo_pattern_destroy(pat);

				double sx = (double)(prev_w - 6) / (double)selmon->mw;
				double sy = (double)(prev_h - 6) / (double)selmon->mh;

				for (Client *c = selmon->clients; c; c = c->next) {
					if (c->tags & (1 << i)) {
						double wx = (double)prev_x + 3.0 + (c->x - selmon->mx) * sx;
						double wy = (double)prev_y + 3.0 + (c->y - selmon->my) * sy;
						double ww = c->w * sx;
						double wh = c->h * sy;
						if (ww < 10) ww = 10;
						if (wh < 8) wh = 8;

						ws_draw_vector_window(cr, c, wx, wy, ww, wh);

						ws_cairo_rounded_rect(cr, wx, wy, ww, wh, 2.5);
						cairo_set_source_rgba(cr, 0.35, 0.40, 0.50, 0.85);
						cairo_set_line_width(cr, 0.8);
						cairo_stroke(cr);
					}
				}
			}
		} else {
			/* Empty Workspace */
			cairo_set_source_rgba(cr, 0.04, 0.04, 0.06, 0.7);
			cairo_paint(cr);

			cairo_set_font_size(cr, 13.0);
			cairo_set_source_rgba(cr, 0.20, 0.20, 0.25, 0.5);
			cairo_text_extents_t ie;
			cairo_text_extents(cr, "󰍹", &ie);
			cairo_move_to(cr, prev_x + (prev_w - ie.width) / 2.0, prev_y + prev_h / 2.0 + 4.0);
			cairo_show_text(cr, "󰍹");
		}
		cairo_restore(cr);

		/* 3. Bottom Application Badge Pill */
		cairo_set_font_size(cr, 9.0);
		if (!is_empty) {
			char app_buf[64];
			if (tag_counts[i] > 1)
				snprintf(app_buf, sizeof(app_buf), "%s +%d", tag_apps[i], tag_counts[i] - 1);
			else
				snprintf(app_buf, sizeof(app_buf), "%s", tag_apps[i]);

			cairo_text_extents(cr, app_buf, &te);
			double pill_w = te.width + 12.0;
			if (pill_w > card_w - 8.0)
				pill_w = card_w - 8.0;
			double pill_h = 16.0;
			double pill_x = cx + (card_w - pill_w) / 2.0;
			double pill_y = card_y + card_h - 24.0;

			ws_cairo_rounded_rect(cr, pill_x, pill_y, pill_w, pill_h, 5.0);
			if (is_sel) {
				cairo_set_source_rgba(cr, 0.25, 0.26, 0.38, 0.95);
				cairo_fill(cr);
				cairo_set_source_rgb(cr, 0.96, 0.96, 1.0);
			} else {
				cairo_set_source_rgba(cr, 0.12, 0.17, 0.15, 0.85);
				cairo_fill(cr);
				cairo_set_source_rgb(cr, 0.45, 0.88, 0.62);
			}

			cairo_save(cr);
			ws_cairo_rounded_rect(cr, pill_x, pill_y, pill_w, pill_h, 5.0);
			cairo_clip(cr);
			cairo_move_to(cr, pill_x + (pill_w - te.width) / 2.0, pill_y + 11.5);
			cairo_show_text(cr, app_buf);
			cairo_restore(cr);
		} else {
			cairo_set_source_rgb(cr, 0.22, 0.22, 0.27);
			cairo_text_extents(cr, "empty", &te);
			cairo_move_to(cr, cx + (card_w - te.width) / 2.0, card_y + card_h - 13.0);
			cairo_show_text(cr, "empty");
		}
	}

	cairo_pop_group_to_source(cr);
	cairo_paint(cr);
	cairo_surface_flush(surf);
	XFlush(dpy);
}

void
workspaceswitcher(const Arg *arg)
{
	int ntags = (int)LENGTH(tags);
	if (ntags <= 0 || !selmon)
		return;

	/* Find current active tag */
	int current_tag = 0;
	for (int i = 0; i < ntags; i++) {
		if (selmon->tagset[selmon->seltags] & (1 << i)) {
			current_tag = i;
			break;
		}
	}

	/* Capture live camera snapshot of current tag before opening switcher */
	workspaceswitcher_snapshot();

	/* Find previous tag for Windows-style MRU switching */
	int prev_tag = -1;
	unsigned int prev_mask = selmon->tagset[selmon->seltags ^ 1] & TAGMASK;
	for (int i = 0; i < ntags; i++) {
		if (prev_mask & (1 << i)) {
			prev_tag = i;
			break;
		}
	}

	/* Initial target tag */
	int step = (arg && arg->i != 0) ? arg->i : 1;
	int sel;
	if (step > 0 && prev_tag >= 0 && prev_tag != current_tag)
		sel = prev_tag;
	else
		sel = (current_tag + step + ntags * 10) % ntags;

	/* Query initial modifier state to detect Alt vs Super */
	Window r_ret, c_ret;
	int rx, ry, wx, wy;
	unsigned int init_mask = 0;
	XQueryPointer(dpy, root, &r_ret, &c_ret, &rx, &ry, &wx, &wy, &init_mask);
	int mod_is_super = (init_mask & Mod4Mask) != 0;
	int mod_is_alt = (init_mask & Mod1Mask) != 0;
	if (!mod_is_super && !mod_is_alt)
		mod_is_alt = 1;

	/* Gather tag metadata and occupied clients */
	char tag_names[32][32];
	char tag_icons[32][16];
	char tag_apps[32][64];
	int tag_counts[32];

	for (int i = 0; i < ntags && i < 32; i++) {
		tag_counts[i] = 0;
		tag_apps[i][0] = '\0';

		const char *colon = strchr(tags[i], ':');
		if (colon)
			snprintf(tag_names[i], sizeof(tag_names[i]), "%s", colon + 1);
		else
			snprintf(tag_names[i], sizeof(tag_names[i]), "%s", tags[i]);

		const char *icon = ws_get_icon(i, tag_names[i]);
		snprintf(tag_icons[i], sizeof(tag_icons[i]), "%s", icon);
	}

	for (Client *c = selmon->clients; c; c = c->next) {
		for (int i = 0; i < ntags && i < 32; i++) {
			if (c->tags & (1 << i)) {
				tag_counts[i]++;
				if (tag_apps[i][0] == '\0') {
					XClassHint ch = { NULL, NULL };
					if (XGetClassHint(dpy, c->win, &ch)) {
						if (ch.res_class && ch.res_class[0]) {
							strncpy(tag_apps[i], ch.res_class, sizeof(tag_apps[i]) - 1);
							tag_apps[i][sizeof(tag_apps[i]) - 1] = '\0';
						} else if (ch.res_name && ch.res_name[0]) {
							strncpy(tag_apps[i], ch.res_name, sizeof(tag_apps[i]) - 1);
							tag_apps[i][sizeof(tag_apps[i]) - 1] = '\0';
						}
						if (ch.res_class) XFree(ch.res_class);
						if (ch.res_name) XFree(ch.res_name);
					}
					if (tag_apps[i][0] == '\0' && c->name[0]) {
						strncpy(tag_apps[i], c->name, sizeof(tag_apps[i]) - 1);
						tag_apps[i][sizeof(tag_apps[i]) - 1] = '\0';
					}
					for (char *p = tag_apps[i]; *p; p++) {
						if (*p >= 'A' && *p <= 'Z')
							*p = *p + ('a' - 'A');
					}
				}
			}
		}
	}

	/* HUD Dimensions & Positioning centered on active monitor */
	int win_w = 1160;
	int win_h = 205;
	if (win_w > selmon->mw - 40)
		win_w = selmon->mw - 40;
	int win_x = selmon->mx + (selmon->mw - win_w) / 2;
	int win_y = selmon->my + (selmon->mh - win_h) / 2;

	/* 32-bit ARGB Visual selection for antialiased transparency */
	XVisualInfo vinfo;
	Visual *visual = DefaultVisual(dpy, screen);
	int depth = DefaultDepth(dpy, screen);
	Colormap colormap = DefaultColormap(dpy, screen);
	int is_argb = 0;

	if (XMatchVisualInfo(dpy, screen, 32, TrueColor, &vinfo)) {
		visual = vinfo.visual;
		depth = 32;
		colormap = XCreateColormap(dpy, root, visual, AllocNone);
		is_argb = 1;
	}

	XSetWindowAttributes wa = {
		.override_redirect = True,
		.colormap = colormap,
		.border_pixel = 0,
		.background_pixel = 0,
		.event_mask = ExposureMask | KeyPressMask | KeyReleaseMask |
		              ButtonPressMask | PointerMotionMask | StructureNotifyMask
	};

	Window win = XCreateWindow(dpy, root, win_x, win_y, win_w, win_h, 0,
	                          depth, InputOutput, visual,
	                          CWOverrideRedirect | CWColormap | CWBorderPixel | CWBackPixel | CWEventMask,
	                          &wa);

	XClassHint wch = { "vxwm-switcher", "vxwm" };
	XSetClassHint(dpy, win, &wch);

	XMapRaised(dpy, win);
	XSync(dpy, False);

	cairo_surface_t *surf = cairo_xlib_surface_create(dpy, win, visual, win_w, win_h);
	cairo_t *cr = cairo_create(surf);

	/* Grab Keyboard & Pointer */
	for (int i = 0; i < 100; i++) {
		if (XGrabKeyboard(dpy, win, True, GrabModeAsync, GrabModeAsync, CurrentTime) == GrabSuccess)
			break;
		usleep(1000);
	}
	XGrabPointer(dpy, win, True, ButtonPressMask | PointerMotionMask,
	             GrabModeAsync, GrabModeAsync, None, None, CurrentTime);

	/* Initial draw */
	ws_draw(cr, surf, win_w, win_h, ntags, sel, current_tag, mod_is_super,
	        (const char (*)[32])tag_names, (const char (*)[16])tag_icons,
	        (const char (*)[64])tag_apps, tag_counts);

	/* If the modifier was already released before the modal loop (instant tap) */
	unsigned int quick_mask = 0;
	XQueryPointer(dpy, root, &r_ret, &c_ret, &rx, &ry, &wx, &wy, &quick_mask);
	int done = 0;
	int confirm = 0;

	if ((mod_is_alt && !(quick_mask & Mod1Mask)) || (mod_is_super && !(quick_mask & Mod4Mask))) {
		confirm = 1;
		done = 1;
	}

	int pad = 16;
	int gap = 10;
	int avail_w = win_w - pad * 2 - (ntags - 1) * gap;
	int card_w = avail_w / ntags;
	int card_h = 142;
	int card_y = 42;

	while (!done) {
		XEvent ev;
		XNextEvent(dpy, &ev);

		switch (ev.type) {
		case Expose:
			if (ev.xexpose.count == 0) {
				ws_draw(cr, surf, win_w, win_h, ntags, sel, current_tag, mod_is_super,
				        (const char (*)[32])tag_names, (const char (*)[16])tag_icons,
				        (const char (*)[64])tag_apps, tag_counts);
			}
			break;

		case KeyPress: {
			KeySym ks = XLookupKeysym(&ev.xkey, 0);
			unsigned int state = ev.xkey.state;

			if (ks == XK_Tab) {
				if (state & (ShiftMask | ControlMask))
					sel = (sel - 1 + ntags) % ntags;
				else
					sel = (sel + 1) % ntags;
				ws_draw(cr, surf, win_w, win_h, ntags, sel, current_tag, mod_is_super,
				        (const char (*)[32])tag_names, (const char (*)[16])tag_icons,
				        (const char (*)[64])tag_apps, tag_counts);
			} else if (ks == XK_Right || ks == XK_l || ks == XK_Down || ks == XK_j) {
				sel = (sel + 1) % ntags;
				ws_draw(cr, surf, win_w, win_h, ntags, sel, current_tag, mod_is_super,
				        (const char (*)[32])tag_names, (const char (*)[16])tag_icons,
				        (const char (*)[64])tag_apps, tag_counts);
			} else if (ks == XK_Left || ks == XK_h || ks == XK_Up || ks == XK_k) {
				sel = (sel - 1 + ntags) % ntags;
				ws_draw(cr, surf, win_w, win_h, ntags, sel, current_tag, mod_is_super,
				        (const char (*)[32])tag_names, (const char (*)[16])tag_icons,
				        (const char (*)[64])tag_apps, tag_counts);
			} else if (ks >= XK_1 && ks <= XK_9) {
				int idx = (int)(ks - XK_1);
				if (idx < ntags) {
					sel = idx;
					ws_draw(cr, surf, win_w, win_h, ntags, sel, current_tag, mod_is_super,
					        (const char (*)[32])tag_names, (const char (*)[16])tag_icons,
					        (const char (*)[64])tag_apps, tag_counts);
				}
			} else if (ks >= XK_KP_1 && ks <= XK_KP_9) {
				int idx = (int)(ks - XK_KP_1);
				if (idx < ntags) {
					sel = idx;
					ws_draw(cr, surf, win_w, win_h, ntags, sel, current_tag, mod_is_super,
					        (const char (*)[32])tag_names, (const char (*)[16])tag_icons,
					        (const char (*)[64])tag_apps, tag_counts);
				}
			} else if (ks == XK_Return || ks == XK_KP_Enter || ks == XK_space) {
				confirm = 1;
				done = 1;
			} else if (ks == XK_Escape || ks == XK_q) {
				confirm = 0;
				done = 1;
			}
			break;
		}

		case KeyRelease: {
			KeySym ks = XLookupKeysym(&ev.xkey, 0);
			if (ks == XK_Alt_L || ks == XK_Alt_R ||
			    ks == XK_Super_L || ks == XK_Super_R ||
			    ks == XK_Meta_L || ks == XK_Meta_R) {
				unsigned int cur_mask = 0;
				XQueryPointer(dpy, root, &r_ret, &c_ret, &rx, &ry, &wx, &wy, &cur_mask);
				if (mod_is_super && !(cur_mask & Mod4Mask)) {
					confirm = 1;
					done = 1;
				} else if (mod_is_alt && !(cur_mask & Mod1Mask)) {
					confirm = 1;
					done = 1;
				} else if (!mod_is_super && !mod_is_alt) {
					confirm = 1;
					done = 1;
				}
			}
			break;
		}

		case MotionNotify: {
			int mx = ev.xmotion.x;
			int my = ev.xmotion.y;
			if (my >= card_y && my <= card_y + card_h) {
				for (int i = 0; i < ntags; i++) {
					int cx = pad + i * (card_w + gap);
					if (mx >= cx && mx <= cx + card_w) {
						if (sel != i) {
							sel = i;
							ws_draw(cr, surf, win_w, win_h, ntags, sel, current_tag, mod_is_super,
							        (const char (*)[32])tag_names, (const char (*)[16])tag_icons,
							        (const char (*)[64])tag_apps, tag_counts);
						}
						break;
					}
				}
			}
			break;
		}

		case ButtonPress: {
			int bx = ev.xbutton.x;
			int by = ev.xbutton.y;
			if (by >= card_y && by <= card_y + card_h) {
				for (int i = 0; i < ntags; i++) {
					int cx = pad + i * (card_w + gap);
					if (bx >= cx && bx <= cx + card_w) {
						sel = i;
						confirm = 1;
						done = 1;
						break;
					}
				}
			} else {
				confirm = 0;
				done = 1;
			}
			break;
		}

		default:
			break;
		}
	}

	/* Cleanup grabs and Cairo */
	XUngrabKeyboard(dpy, CurrentTime);
	XUngrabPointer(dpy, CurrentTime);

	cairo_destroy(cr);
	cairo_surface_destroy(surf);
	XDestroyWindow(dpy, win);
	if (is_argb && colormap != DefaultColormap(dpy, screen))
		XFreeColormap(dpy, colormap);
	XSync(dpy, False);

	/* Switch workspace */
	if (confirm && sel != current_tag) {
		Arg a = { .ui = 1 << sel };
		view(&a);
	}
	restack(selmon);
}
