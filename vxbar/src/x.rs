#![allow(non_upper_case_globals)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_long, c_uchar, c_ulong};
use std::ptr;
use x11::xlib;

use crate::config::{Config, Position};

pub struct Atoms {
    pub wm_window_type: xlib::Atom,
    pub wm_window_type_dock: xlib::Atom,
    pub wm_strut: xlib::Atom,
    pub wm_strut_partial: xlib::Atom,
    pub wm_desktop: xlib::Atom,
    pub current_desktop: xlib::Atom,
    pub number_of_desktops: xlib::Atom,
    pub desktop_names: xlib::Atom,
    pub client_list: xlib::Atom,
    pub active_window: xlib::Atom,
    pub wm_name: xlib::Atom,
    pub utf8_string: xlib::Atom,
    pub wm_state: xlib::Atom,
    pub wm_state_above: xlib::Atom,
    pub wm_state_sticky: xlib::Atom,
}

impl Atoms {
    unsafe fn new(dpy: *mut xlib::Display) -> Self {
        let intern = |name: &str| -> xlib::Atom {
            let c = CString::new(name).unwrap();
            xlib::XInternAtom(dpy, c.as_ptr(), xlib::False)
        };
        Self {
            wm_window_type: intern("_NET_WM_WINDOW_TYPE"),
            wm_window_type_dock: intern("_NET_WM_WINDOW_TYPE_DOCK"),
            wm_strut: intern("_NET_WM_STRUT"),
            wm_strut_partial: intern("_NET_WM_STRUT_PARTIAL"),
            wm_desktop: intern("_NET_WM_DESKTOP"),
            current_desktop: intern("_NET_CURRENT_DESKTOP"),
            number_of_desktops: intern("_NET_NUMBER_OF_DESKTOPS"),
            desktop_names: intern("_NET_DESKTOP_NAMES"),
            client_list: intern("_NET_CLIENT_LIST"),
            active_window: intern("_NET_ACTIVE_WINDOW"),
            wm_name: intern("_NET_WM_NAME"),
            utf8_string: intern("UTF8_STRING"),
            wm_state: intern("_NET_WM_STATE"),
            wm_state_above: intern("_NET_WM_STATE_ABOVE"),
            wm_state_sticky: intern("_NET_WM_STATE_STICKY"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

pub struct X {
    pub dpy: *mut xlib::Display,
    pub root: xlib::Window,
    pub win: xlib::Window,
    pub screen: c_int,
    pub visual: *mut xlib::Visual,
    #[allow(dead_code)]
    pub depth: c_int,
    #[allow(dead_code)]
    pub colormap: xlib::Colormap,
    pub atoms: Atoms,
    pub geom: Rect,
    pub mon: Rect,
}

impl X {
    pub fn open(cfg: &Config) -> Result<Self, String> {
        unsafe {
            let dpy = xlib::XOpenDisplay(ptr::null());
            if dpy.is_null() {
                return Err("не открывается DISPLAY".into());
            }
            let screen = xlib::XDefaultScreen(dpy);
            let root = xlib::XRootWindow(dpy, screen);
            let atoms = Atoms::new(dpy);

            // 32-битный ARGB-визуал нужен, чтобы скруглённые углы плавающего
            // бара были прозрачными, а не чёрными. Если такого нет -- падаем
            // на визуал по умолчанию, бар просто будет полностью непрозрачным.
            let mut vinfo: xlib::XVisualInfo = std::mem::zeroed();
            let argb = xlib::XMatchVisualInfo(
                dpy,
                screen,
                32,
                xlib::TrueColor,
                &mut vinfo,
            ) != 0;
            let (visual, depth) = if argb {
                (vinfo.visual, vinfo.depth)
            } else {
                (
                    xlib::XDefaultVisual(dpy, screen),
                    xlib::XDefaultDepth(dpy, screen),
                )
            };
            let colormap = xlib::XCreateColormap(dpy, root, visual, xlib::AllocNone);

            let mon = monitor_rect(dpy, screen, cfg.bar.monitor);
            let geom = bar_rect(cfg, &mon);

            let mut swa: xlib::XSetWindowAttributes = std::mem::zeroed();
            swa.override_redirect = xlib::False; // док должен быть виден WM, иначе струты никто не прочитает
            swa.colormap = colormap;
            swa.background_pixel = 0;
            swa.border_pixel = 0;
            swa.event_mask = xlib::ExposureMask
                | xlib::ButtonPressMask
                | xlib::PointerMotionMask
                | xlib::LeaveWindowMask
                | xlib::StructureNotifyMask;

            let win = xlib::XCreateWindow(
                dpy,
                root,
                geom.x,
                geom.y,
                geom.w,
                geom.h,
                0,
                depth,
                xlib::InputOutput as u32,
                visual,
                xlib::CWOverrideRedirect
                    | xlib::CWBackPixel
                    | xlib::CWBorderPixel
                    | xlib::CWColormap
                    | xlib::CWEventMask,
                &mut swa,
            );

            let mut x = X {
                dpy,
                root,
                win,
                screen,
                visual,
                depth,
                colormap,
                atoms,
                geom,
                mon,
            };
            x.set_window_props(cfg);
            xlib::XSelectInput(dpy, root, xlib::PropertyChangeMask);
            xlib::XMapRaised(dpy, win);
            xlib::XFlush(dpy);
            Ok(x)
        }
    }

    unsafe fn set_window_props(&mut self, cfg: &Config) {
        let d = self.dpy;
        let a = &self.atoms;

        let dock = a.wm_window_type_dock;
        xlib::XChangeProperty(
            d,
            self.win,
            a.wm_window_type,
            xlib::XA_ATOM,
            32,
            xlib::PropModeReplace,
            &dock as *const xlib::Atom as *const c_uchar,
            1,
        );

        let states = [a.wm_state_above, a.wm_state_sticky];
        xlib::XChangeProperty(
            d,
            self.win,
            a.wm_state,
            xlib::XA_ATOM,
            32,
            xlib::PropModeReplace,
            states.as_ptr() as *const c_uchar,
            2,
        );

        // 0xFFFFFFFF -- "на всех рабочих столах"
        let all: c_long = -1;
        xlib::XChangeProperty(
            d,
            self.win,
            a.wm_desktop,
            xlib::XA_CARDINAL,
            32,
            xlib::PropModeReplace,
            &all as *const c_long as *const c_uchar,
            1,
        );

        let name = CString::new("vxbar").unwrap();
        xlib::XChangeProperty(
            d,
            self.win,
            a.wm_name,
            a.utf8_string,
            8,
            xlib::PropModeReplace,
            name.as_ptr() as *const c_uchar,
            5,
        );
        let mut class = xlib::XClassHint {
            res_name: name.as_ptr() as *mut _,
            res_class: name.as_ptr() as *mut _,
        };
        xlib::XSetClassHint(d, self.win, &mut class);

        self.set_struts(cfg);
    }

    /// Струт резервирует полосу от края ЭКРАНА, а не монитора, поэтому при
    /// нескольких мониторах надо считать от границы всего экрана.
    pub unsafe fn set_struts(&self, cfg: &Config) {
        let screen_h = xlib::XDisplayHeight(self.dpy, self.screen) as i32;
        let mut strut: [c_long; 12] = [0; 12];

        let reserve = (cfg.bar.height as i32 + cfg.bar.margin_edge.max(0)) as c_long;
        match cfg.bar.position {
            Position::Top => {
                // top = нижняя граница бара относительно верха экрана
                strut[2] = (self.mon.y + cfg.bar.margin_edge.max(0) + cfg.bar.height as i32) as c_long;
                strut[8] = self.geom.x as c_long;
                strut[9] = (self.geom.x + self.geom.w as i32 - 1) as c_long;
            }
            Position::Bottom => {
                let mon_bottom = self.mon.y + self.mon.h as i32;
                strut[3] = (screen_h - mon_bottom) as c_long + reserve;
                strut[10] = self.geom.x as c_long;
                strut[11] = (self.geom.x + self.geom.w as i32 - 1) as c_long;
            }
        }

        xlib::XChangeProperty(
            self.dpy,
            self.win,
            self.atoms.wm_strut_partial,
            xlib::XA_CARDINAL,
            32,
            xlib::PropModeReplace,
            strut.as_ptr() as *const c_uchar,
            12,
        );
        // _NET_WM_STRUT -- для WM, не знающих partial-версию
        xlib::XChangeProperty(
            self.dpy,
            self.win,
            self.atoms.wm_strut,
            xlib::XA_CARDINAL,
            32,
            xlib::PropModeReplace,
            strut.as_ptr() as *const c_uchar,
            4,
        );
    }

    /// Применить новый конфиг к уже существующему окну: пересчитать геометрию,
    /// подвинуть окно и переписать струты.
    pub fn reconfigure(&mut self, cfg: &Config) {
        unsafe {
            self.mon = monitor_rect(self.dpy, self.screen, cfg.bar.monitor);
            self.geom = bar_rect(cfg, &self.mon);
            xlib::XMoveResizeWindow(
                self.dpy,
                self.win,
                self.geom.x,
                self.geom.y,
                self.geom.w,
                self.geom.h,
            );
            self.set_struts(cfg);
            xlib::XFlush(self.dpy);
        }
    }

    pub fn cardinal(&self, win: xlib::Window, atom: xlib::Atom) -> Option<i64> {
        self.cardinals(win, atom, 1).and_then(|v| v.first().copied())
    }

    pub fn cardinals(&self, win: xlib::Window, atom: xlib::Atom, len: c_long) -> Option<Vec<i64>> {
        unsafe {
            let mut actual_type: xlib::Atom = 0;
            let mut actual_format: c_int = 0;
            let mut nitems: c_ulong = 0;
            let mut bytes_after: c_ulong = 0;
            let mut prop: *mut c_uchar = ptr::null_mut();
            let rc = xlib::XGetWindowProperty(
                self.dpy,
                win,
                atom,
                0,
                len,
                xlib::False,
                xlib::AnyPropertyType as xlib::Atom,
                &mut actual_type,
                &mut actual_format,
                &mut nitems,
                &mut bytes_after,
                &mut prop,
            );
            if rc != xlib::Success as c_int || prop.is_null() || actual_format != 32 {
                if !prop.is_null() {
                    xlib::XFree(prop as *mut _);
                }
                return None;
            }
            let slice = std::slice::from_raw_parts(prop as *const c_long, nitems as usize);
            let out = slice.iter().map(|v| *v as i64).collect();
            xlib::XFree(prop as *mut _);
            Some(out)
        }
    }

    /// _NET_WM_NAME (UTF8), с откатом на WM_NAME для древних клиентов.
    pub fn window_title(&self, win: xlib::Window) -> Option<String> {
        if win == 0 {
            return None;
        }
        unsafe {
            if let Some(s) = self.text_prop(win, self.atoms.wm_name) {
                if !s.is_empty() {
                    return Some(s);
                }
            }
            self.text_prop(win, xlib::XA_WM_NAME)
        }
    }

    unsafe fn text_prop(&self, win: xlib::Window, atom: xlib::Atom) -> Option<String> {
        let mut tp: xlib::XTextProperty = std::mem::zeroed();
        if xlib::XGetTextProperty(self.dpy, win, &mut tp, atom) == 0 || tp.nitems == 0 {
            return None;
        }
        let out = if tp.encoding == xlib::XA_STRING {
            CStr::from_ptr(tp.value as *const _).to_string_lossy().into_owned()
        } else {
            let mut list: *mut *mut i8 = ptr::null_mut();
            let mut count: c_int = 0;
            let mut s = String::new();
            if xlib::Xutf8TextPropertyToTextList(self.dpy, &mut tp, &mut list, &mut count)
                >= xlib::Success as c_int
                && count > 0
                && !list.is_null()
            {
                s = CStr::from_ptr(*list).to_string_lossy().into_owned();
                xlib::XFreeStringList(list);
            }
            s
        };
        xlib::XFree(tp.value as *mut _);
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// Список имён рабочих столов из _NET_DESKTOP_NAMES (UTF8, NUL-разделённый).
    pub fn desktop_names(&self) -> Option<Vec<String>> {
        unsafe {
            let mut tp: xlib::XTextProperty = std::mem::zeroed();
            if xlib::XGetTextProperty(self.dpy, self.root, &mut tp, self.atoms.desktop_names) == 0
                || tp.nitems == 0
            {
                return None;
            }
            let mut list: *mut *mut i8 = ptr::null_mut();
            let mut count: c_int = 0;
            let mut out = Vec::new();
            if xlib::Xutf8TextPropertyToTextList(self.dpy, &mut tp, &mut list, &mut count)
                >= xlib::Success as c_int
                && !list.is_null()
            {
                for i in 0..count as isize {
                    out.push(CStr::from_ptr(*list.offset(i)).to_string_lossy().into_owned());
                }
                xlib::XFreeStringList(list);
            }
            xlib::XFree(tp.value as *mut _);
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
    }

    /// Просим WM переключить рабочий стол. vxwm понимает это только с патчем
    /// externalbar -- ванильный dwm-подобный код глотает root-сообщения.
    pub fn request_desktop(&self, idx: usize) {
        unsafe {
            let mut e: xlib::XEvent = std::mem::zeroed();
            let ev = &mut e.client_message;
            ev.type_ = xlib::ClientMessage;
            ev.window = self.root;
            ev.message_type = self.atoms.current_desktop;
            ev.format = 32;
            ev.data.set_long(0, idx as c_long);
            ev.data.set_long(1, xlib::CurrentTime as c_long);
            xlib::XSendEvent(
                self.dpy,
                self.root,
                xlib::False,
                xlib::SubstructureNotifyMask | xlib::SubstructureRedirectMask,
                &mut e,
            );
            xlib::XFlush(self.dpy);
        }
    }
}

pub fn monitor_rect(dpy: *mut xlib::Display, screen: c_int, idx: usize) -> Rect {
    unsafe {
        let full = Rect {
            x: 0,
            y: 0,
            w: xlib::XDisplayWidth(dpy, screen) as u32,
            h: xlib::XDisplayHeight(dpy, screen) as u32,
        };
        let mut n: c_int = 0;
        let si = x11::xinerama::XineramaQueryScreens(dpy, &mut n);
        if si.is_null() || n <= 0 {
            return full;
        }
        let list = std::slice::from_raw_parts(si, n as usize);
        let pick = list.get(idx).or_else(|| list.first()).copied();
        xlib::XFree(si as *mut _);
        match pick {
            Some(s) => Rect {
                x: s.x_org as i32,
                y: s.y_org as i32,
                w: s.width as u32,
                h: s.height as u32,
            },
            None => full,
        }
    }
}

pub fn bar_rect(cfg: &Config, mon: &Rect) -> Rect {
    let side = cfg.bar.margin_side.max(0);
    let edge = cfg.bar.margin_edge.max(0);
    let w = (mon.w as i32 - 2 * side).max(1) as u32;
    let x = mon.x + side;
    let y = match cfg.bar.position {
        Position::Top => mon.y + edge,
        Position::Bottom => mon.y + mon.h as i32 - cfg.bar.height as i32 - edge,
    };
    Rect {
        x,
        y,
        w,
        h: cfg.bar.height.max(1),
    }
}
