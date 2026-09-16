//! Выдвижные виджеты: по клику на модуль рядом с баром открывается маленькое
//! окно с подробностями. Это отдельное override-redirect окно, а не часть
//! бара: бар -- док с фиксированной толщиной, растягивать его под календарь
//! нельзя, WM тут же пересчитал бы струты и раскладку.

use cairo::Context;
use pango::FontDescription;

use crate::config::{parse_color, Config, Position};
use crate::modules::{self, ProcSampler, Volume};
use crate::render::Hit;
use crate::x::X;
use std::os::raw::{c_int, c_uchar};
use x11::xlib;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Clock,
    Volume,
    Cpu,
    Ram,
}

impl Kind {
    pub fn from_hit(hit: Hit) -> Option<Self> {
        match hit {
            Hit::Clock => Some(Kind::Clock),
            Hit::Volume => Some(Kind::Volume),
            Hit::Cpu => Some(Kind::Cpu),
            Hit::Ram => Some(Kind::Ram),
            _ => None,
        }
    }
}

/// Что показать в виджете. Собирается в main вместе с остальным состоянием:
/// popup сам в /proc не лазает, чтобы не дублировать источники данных.
pub struct PopupData {
    pub volume: Volume,
    pub cpu: u32,
    pub mem_used: f64,
    pub mem_total: f64,
    pub procs: Vec<modules::Proc>,
}

/// Строка содержимого. Слайдер -- единственный виджет со своей геометрией,
/// остальное это пары «слева подпись, справа значение».
enum Item {
    Title(String),
    /// Подсказка мелким приглушённым шрифтом внизу виджета.
    Hint(String),
    Pair(String, String),
    Slider(f64),
    Calendar,
    Gap(f64),
}

const ROW_H: f64 = 22.0;
const SLIDER_H: f64 = 26.0;
const CAL_ROW_H: f64 = 22.0;
const CAL_ROWS: f64 = 7.0; // строка с днями недели + шесть недель

pub struct Popup {
    dpy: *mut xlib::Display,
    pub win: xlib::Window,
    raw: *mut cairo_sys::cairo_surface_t,
    surface: cairo::Surface,
    ctx: Context,
    pango: pango::Context,
    font: FontDescription,
    pub kind: Option<Kind>,
    w: f64,
    h: f64,
    /// Зона слайдера громкости в координатах окна -- по ней считается клик.
    slider: Option<(f64, f64, f64)>,
}

impl Popup {
    pub fn new(x: &X, cfg: &Config) -> Result<Self, String> {
        unsafe {
            let mut swa: xlib::XSetWindowAttributes = std::mem::zeroed();
            // override_redirect: виджет -- не окно приложения, WM не должен
            // ни давать ему рамку, ни тайлить его.
            swa.override_redirect = xlib::True;
            swa.colormap = x.colormap;
            swa.background_pixel = 0;
            swa.border_pixel = 0;
            swa.event_mask = xlib::ExposureMask | xlib::ButtonPressMask;
            let win = xlib::XCreateWindow(
                x.dpy,
                x.root,
                -1,
                -1,
                1,
                1,
                0,
                x.depth,
                xlib::InputOutput as u32,
                x.visual,
                xlib::CWOverrideRedirect
                    | xlib::CWBackPixel
                    | xlib::CWBorderPixel
                    | xlib::CWColormap
                    | xlib::CWEventMask,
                &mut swa,
            );
            // Тип DOCK, чтобы композитор не пытался анимировать виджет как
            // обычное окно и не рисовал ему тень поверх скруглённых углов.
            let dock = x.atoms.wm_window_type_dock;
            xlib::XChangeProperty(
                x.dpy,
                win,
                x.atoms.wm_window_type,
                xlib::XA_ATOM,
                32,
                xlib::PropModeReplace,
                &dock as *const xlib::Atom as *const c_uchar,
                1,
            );

            let raw = cairo_sys::cairo_xlib_surface_create(
                x.dpy as *mut _,
                win as _,
                x.visual as *mut _,
                1,
                1,
            );
            if raw.is_null() {
                return Err("cairo: не создаётся поверхность виджета".into());
            }
            let surface =
                cairo::Surface::from_raw_full(raw).map_err(|e| format!("cairo surface: {e}"))?;
            let ctx = Context::new(&surface).map_err(|e| format!("cairo context: {e}"))?;
            let pango = pangocairo::functions::create_context(&ctx);
            Ok(Self {
                dpy: x.dpy,
                win,
                raw,
                surface,
                ctx,
                pango,
                font: FontDescription::from_string(&cfg.style.font),
                kind: None,
                w: 1.0,
                h: 1.0,
                slider: None,
            })
        }
    }

    /// Размер окна виджета -- по нему main решает, попал ли клик внутрь.
    pub fn size(&self) -> (f64, f64) {
        (self.w, self.h)
    }

    pub fn visible(&self) -> bool {
        self.kind.is_some()
    }

    pub fn reload_font(&mut self, cfg: &Config) {
        self.font = FontDescription::from_string(&cfg.style.font);
    }

    /// Открыть виджет напротив модуля. `region` -- границы модуля вдоль оси
    /// бара, по ним виджет выравнивается с тем, что открыло.
    pub fn open(&mut self, x: &X, cfg: &Config, kind: Kind, region: (f64, f64), data: &PopupData) {
        let items = build(cfg, kind, data);
        self.w = cfg.popups.width.max(120) as f64;
        self.h = self.height_of(cfg, &items);
        let (px, py) = self.place(x, cfg, region);
        unsafe {
            xlib::XMoveResizeWindow(self.dpy, self.win, px, py, self.w as u32, self.h as u32);
            cairo_sys::cairo_xlib_surface_set_size(self.raw, self.w as i32, self.h as i32);
            xlib::XMapRaised(self.dpy, self.win);
            // Пока виджет открыт, пойманный на root указатель даёт нам клики
            // мимо окна -- иначе закрыть его можно было бы только повторным
            // попаданием по модулю.
            xlib::XGrabPointer(
                self.dpy,
                x.root,
                xlib::True,
                (xlib::ButtonPressMask | xlib::ButtonReleaseMask) as u32,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                0,
                0,
                xlib::CurrentTime,
            );
        }
        self.kind = Some(kind);
        self.draw(cfg, &items);
    }

    pub fn close(&mut self) {
        if self.kind.is_none() {
            return;
        }
        self.kind = None;
        self.slider = None;
        unsafe {
            xlib::XUngrabPointer(self.dpy, xlib::CurrentTime);
            xlib::XUnmapWindow(self.dpy, self.win);
            xlib::XFlush(self.dpy);
        }
    }

    /// Перерисовать открытый виджет свежими данными (тик таймера, смена громкости).
    pub fn refresh(&mut self, cfg: &Config, data: &PopupData) {
        let kind = match self.kind {
            Some(k) => k,
            None => return,
        };
        let items = build(cfg, kind, data);
        self.draw(cfg, &items);
    }

    /// Клик внутри виджета. Возвращает true, если что-то поменялось и бар
    /// стоит перерисовать.
    pub fn click(&mut self, wx: f64, wy: f64) -> bool {
        if let (Some(Kind::Volume), Some((sx, sy, sw))) = (self.kind, self.slider) {
            if wy >= sy && wy <= sy + SLIDER_H {
                let frac = ((wx - sx) / sw).clamp(0.0, 1.0);
                modules::set_volume_abs((frac * 100.0).round() as u32);
                return true;
            }
        }
        false
    }

    /// Левый верхний угол виджета: вплотную к кромке бара, выровнен по модулю
    /// и зажат в границы монитора -- у края экрана иначе уезжает за него.
    fn place(&self, x: &X, cfg: &Config, region: (f64, f64)) -> (i32, i32) {
        let g = &x.geom;
        let gap = cfg.popups.gap;
        let center = (region.0 + region.1) / 2.0;
        if cfg.bar.position.vertical() {
            let px = if cfg.bar.position == Position::Left {
                g.x + g.w as i32 + gap
            } else {
                g.x - self.w as i32 - gap
            };
            let py = (g.y + center as i32 - self.h as i32 / 2)
                .clamp(x.mon.y + 4, x.mon.y + x.mon.h as i32 - self.h as i32 - 4);
            (px, py)
        } else {
            let py = if cfg.bar.position == Position::Top {
                g.y + g.h as i32 + gap
            } else {
                g.y - self.h as i32 - gap
            };
            let px = (g.x + center as i32 - self.w as i32 / 2)
                .clamp(x.mon.x + 4, x.mon.x + x.mon.w as i32 - self.w as i32 - 4);
            (px, py)
        }
    }

    fn set_color(&self, spec: &str) {
        let (r, g, b, a) = parse_color(spec);
        self.ctx.set_source_rgba(r, g, b, a);
    }

    fn layout(&self, text: &str, scale: f64) -> pango::Layout {
        let l = pango::Layout::new(&self.pango);
        let mut f = self.font.clone();
        if scale != 1.0 {
            f.set_size((f.size() as f64 * scale) as i32);
        }
        l.set_font_description(Some(&f));
        l.set_text(text);
        l
    }

    fn text(&self, text: &str, x: f64, y: f64, color: &str, scale: f64) -> f64 {
        let l = self.layout(text, scale);
        let (w, _) = l.pixel_size();
        self.set_color(color);
        self.ctx.move_to(x, y);
        pangocairo::functions::show_layout(&self.ctx, &l);
        w as f64
    }

    fn text_right(&self, text: &str, right: f64, y: f64, color: &str) {
        let l = self.layout(text, 1.0);
        let (w, _) = l.pixel_size();
        self.set_color(color);
        self.ctx.move_to(right - w as f64, y);
        pangocairo::functions::show_layout(&self.ctx, &l);
    }

    fn rounded(&self, x: f64, y: f64, w: f64, h: f64, r: f64) {
        let c = &self.ctx;
        if r <= 0.0 {
            c.rectangle(x, y, w, h);
            return;
        }
        let r = r.min(w / 2.0).min(h / 2.0);
        let pi = std::f64::consts::PI;
        c.new_sub_path();
        c.arc(x + w - r, y + r, r, -pi / 2.0, 0.0);
        c.arc(x + w - r, y + h - r, r, 0.0, pi / 2.0);
        c.arc(x + r, y + h - r, r, pi / 2.0, pi);
        c.arc(x + r, y + r, r, pi, 1.5 * pi);
        c.close_path();
    }

    /// Подсказка -- единственная строка, которая не влезает в одну линию:
    /// переносим её по ширине виджета, иначе хвост уезжает за кромку окна.
    fn hint_layout(&self, cfg: &Config, text: &str) -> pango::Layout {
        let l = self.layout(text, 0.85);
        let w = (self.w - 2.0 * cfg.popups.padding).max(1.0);
        l.set_width((w * pango::SCALE as f64) as i32);
        l.set_wrap(pango::WrapMode::WordChar);
        l
    }

    /// Высота окна виджета. Считается методом, а не свободной функцией, потому
    /// что перенос подсказки зависит от уже выставленной ширины `self.w`.
    fn height_of(&self, cfg: &Config, items: &[Item]) -> f64 {
        let body: f64 = items
            .iter()
            .map(|it| match it {
                Item::Title(_) | Item::Pair(_, _) => ROW_H,
                Item::Hint(t) => hint_h(&self.hint_layout(cfg, t)),
                Item::Slider(_) => SLIDER_H,
                Item::Calendar => CAL_ROW_H * CAL_ROWS,
                Item::Gap(g) => *g,
            })
            .sum();
        body + 2.0 * cfg.popups.padding
    }

    fn draw(&mut self, cfg: &Config, items: &[Item]) {
        self.slider = None;
        let pad = cfg.popups.padding;
        let c = &self.ctx;
        c.set_operator(cairo::Operator::Source);
        c.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        c.paint().ok();
        self.set_color(&cfg.style.background);
        self.rounded(0.0, 0.0, self.w, self.h, cfg.popups.radius);
        c.fill().ok();
        // Кромка в цвет активного тега: на тёмных обоях без неё виджет
        // сливается с фоном и выглядит дырой.
        self.set_color(&cfg.tags.active_bg);
        self.ctx.set_line_width(1.0);
        self.rounded(0.5, 0.5, self.w - 1.0, self.h - 1.0, cfg.popups.radius);
        self.ctx.stroke().ok();
        self.ctx.set_operator(cairo::Operator::Over);

        let mut y = pad;
        for it in items {
            match it {
                Item::Title(t) => {
                    self.text(t, pad, y, &cfg.style.accent, 1.0);
                    y += ROW_H;
                }
                Item::Hint(t) => {
                    let l = self.hint_layout(cfg, t);
                    self.set_color(&cfg.style.muted);
                    self.ctx.move_to(pad, y);
                    pangocairo::functions::show_layout(&self.ctx, &l);
                    y += hint_h(&l);
                }
                Item::Pair(l, r) => {
                    self.text(l, pad, y, &cfg.style.foreground, 1.0);
                    self.text_right(r, self.w - pad, y, &cfg.style.muted);
                    y += ROW_H;
                }
                Item::Gap(g) => y += g,
                Item::Slider(frac) => {
                    let w = self.w - 2.0 * pad;
                    let track_y = y + SLIDER_H / 2.0 - 4.0;
                    self.set_color(&cfg.tags.active_bg);
                    self.rounded(pad, track_y, w, 8.0, 4.0);
                    self.ctx.fill().ok();
                    self.set_color(&cfg.style.accent);
                    self.rounded(pad, track_y, (w * frac).max(8.0), 8.0, 4.0);
                    self.ctx.fill().ok();
                    self.slider = Some((pad, y, w));
                    y += SLIDER_H;
                }
                Item::Calendar => {
                    self.calendar(cfg, y);
                    y += CAL_ROW_H * CAL_ROWS;
                }
            }
        }
        self.surface.flush();
        unsafe { xlib::XFlush(self.dpy) };
    }

    fn calendar(&self, cfg: &Config, top: f64) {
        let pad = cfg.popups.padding;
        let cw = (self.w - 2.0 * pad) / 7.0;
        let n = modules::now();
        let first_wd = weekday(n.year, n.month, 1); // 0 -- понедельник
        let days = days_in_month(n.year, n.month);

        for (i, wd) in ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"]
            .iter()
            .enumerate()
        {
            let l = self.layout(wd, 0.85);
            let (w, _) = l.pixel_size();
            self.set_color(&cfg.style.muted);
            self.ctx
                .move_to(pad + cw * i as f64 + (cw - w as f64) / 2.0, top);
            pangocairo::functions::show_layout(&self.ctx, &l);
        }

        for day in 1..=days {
            let idx = first_wd + day - 1;
            let col = (idx % 7) as f64;
            let row = (idx / 7) as f64 + 1.0;
            let y = top + row * CAL_ROW_H;
            let s = day.to_string();
            let l = self.layout(&s, 1.0);
            let (tw, th) = l.pixel_size();
            let cx = pad + cw * col;
            if day == n.day {
                self.set_color(&cfg.style.accent);
                self.rounded(cx + 1.0, y - 1.0, cw - 2.0, CAL_ROW_H, 5.0);
                self.ctx.fill().ok();
            }
            let color = if day == n.day {
                &cfg.style.background
            } else {
                &cfg.style.foreground
            };
            self.set_color(color);
            self.ctx.move_to(
                cx + (cw - tw as f64) / 2.0,
                y + (CAL_ROW_H - th as f64) / 2.0 - 1.0,
            );
            pangocairo::functions::show_layout(&self.ctx, &l);
        }
    }
}

/// Высота подсказки с небольшим воздухом снизу.
fn hint_h(l: &pango::Layout) -> f64 {
    l.pixel_size().1 as f64 + 4.0
}

fn build(cfg: &Config, kind: Kind, d: &PopupData) -> Vec<Item> {
    match kind {
        Kind::Clock => {
            let n = modules::now();
            vec![
                Item::Title(format!("{} {}", month_name(n.month), n.year)),
                Item::Gap(6.0),
                Item::Calendar,
                Item::Gap(4.0),
                Item::Pair(modules::clock("%H:%M:%S"), modules::clock("%d.%m.%Y")),
            ]
        }
        Kind::Volume => {
            let mut v = vec![
                Item::Title("Громкость".into()),
                Item::Gap(4.0),
                Item::Slider(d.volume.percent as f64 / 100.0),
                Item::Pair(
                    if d.volume.muted {
                        "Без звука".into()
                    } else {
                        "Вывод".into()
                    },
                    format!("{}%", d.volume.percent),
                ),
            ];
            v.push(Item::Hint(
                "Клик по полосе — уровень, колесо на баре — ±5%".into(),
            ));
            v
        }
        Kind::Cpu | Kind::Ram => {
            let mut v = Vec::new();
            if kind == Kind::Cpu {
                v.push(Item::Title(format!("Процессор — {}%", d.cpu)));
            } else {
                v.push(Item::Title(format!(
                    "Память — {:.1} из {:.0} ГБ",
                    d.mem_used, d.mem_total
                )));
            }
            v.push(Item::Gap(6.0));
            if d.procs.is_empty() {
                // У cpu первый срез уходит на базу для дельты, так что пустой
                // список здесь -- нормальный шаг, а не отсутствие данных.
                let msg = if kind == Kind::Cpu {
                    "Считаю загрузку…"
                } else {
                    "Нет данных"
                };
                v.push(Item::Pair(msg.into(), String::new()));
            }
            for p in &d.procs {
                let val = if kind == Kind::Cpu {
                    format!("{:.0}%", p.value)
                } else {
                    format!("{:.2} ГБ", p.value)
                };
                v.push(Item::Pair(trim_name(&p.name), val));
            }
            // Список процессов перетасовывается на каждом тике, и вместе с ним
            // менялось бы число строк. Окно виджета после открытия не
            // переразмеряется, поэтому добиваем высоту до постоянной -- иначе
            // короткий список рисовался бы в окне от длинного.
            let filled = d.procs.len().max(1);
            if cfg.popups.proc_rows > filled {
                v.push(Item::Gap(ROW_H * (cfg.popups.proc_rows - filled) as f64));
            }
            v
        }
    }
}

fn trim_name(s: &str) -> String {
    if s.chars().count() <= 20 {
        return s.to_string();
    }
    s.chars().take(19).collect::<String>() + "…"
}

fn month_name(m: i32) -> &'static str {
    [
        "Январь",
        "Февраль",
        "Март",
        "Апрель",
        "Май",
        "Июнь",
        "Июль",
        "Август",
        "Сентябрь",
        "Октябрь",
        "Ноябрь",
        "Декабрь",
    ]
    .get((m - 1).clamp(0, 11) as usize)
    .copied()
    .unwrap_or("")
}

fn leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// День недели по формуле Целлера, 0 -- понедельник: неделя в календаре
/// начинается с него, а не с воскресенья, как в struct tm.
fn weekday(y: i32, m: i32, d: i32) -> i32 {
    let (y, m) = if m < 3 { (y - 1, m + 12) } else { (y, m) };
    let k = y % 100;
    let j = y / 100;
    let h = (d + 13 * (m + 1) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    // h: 0 -- суббота, 1 -- воскресенье, 2 -- понедельник...
    (h + 5) % 7
}

/// Пойман ли клик окном виджета. Указатель захвачен на root, поэтому окно
/// события -- единственный способ отличить «внутри» от «мимо».
pub fn is_popup_window(popup: &Popup, w: xlib::Window) -> bool {
    popup.win == w
}

/// Координаты клика внутри виджета из события, пришедшего на root.
pub fn local_coords(ev: &xlib::XButtonEvent, x: &X, popup: &Popup) -> (f64, f64) {
    unsafe {
        let mut rx: c_int = 0;
        let mut ry: c_int = 0;
        let mut child: xlib::Window = 0;
        if xlib::XTranslateCoordinates(
            x.dpy, x.root, popup.win, ev.x_root, ev.y_root, &mut rx, &mut ry, &mut child,
        ) == 0
        {
            return (-1.0, -1.0);
        }
        (rx as f64, ry as f64)
    }
}

/// Нужен main, чтобы собрать данные только когда виджет реально открыт.
pub fn procs_for(kind: Kind, cfg: &Config, sampler: &mut ProcSampler) -> Vec<modules::Proc> {
    match kind {
        Kind::Cpu => sampler.top_cpu(cfg.popups.proc_rows),
        Kind::Ram => sampler.top_mem(cfg.popups.proc_rows),
        _ => Vec::new(),
    }
}
