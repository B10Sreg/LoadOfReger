#pragma once

/* Поддержка внешних панелей (vxbar): окна типа _NET_WM_WINDOW_TYPE_DOCK не
 * тайлятся и резервируют место через _NET_WM_STRUT_PARTIAL / _NET_WM_STRUT,
 * плюс приём _NET_CURRENT_DESKTOP с root-окна для переключения тегов кликом. */

#define EB_MAX_DOCKS 8

typedef struct {
	Window win;
	int x, y, w, h;
	/* left, right, top, bottom -- в пикселях от края экрана */
	int strut[4];
} Dock;

enum { EbStrutLeft, EbStrutRight, EbStrutTop, EbStrutBottom };
enum { EbWindowTypeDock, EbStrut, EbStrutPartial, EbVertical, EbLast };

static void externalbar_setup(void);
static int isdock(Window w);
static Dock *wintodock(Window w);
static int updatedockstrut(Dock *d);
static void managedock(Window w, XWindowAttributes *wa);
static void unmanagedock(Window w);
static void applystruts(Monitor *m);
static void scandocks(void);
static void refreshstruts(void);
static void raisedocks(void);
static int dockvertical(void);
static int ebclientmessage(XClientMessageEvent *cme);
static int ebpropertynotify(XPropertyEvent *ev);
