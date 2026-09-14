use cairo::Context;
use pango::FontDescription;

use crate::config::{parse_color, Config};
use crate::modules::{Volume, ICON_CPU, ICON_RAM};
use crate::x::X;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Tag(usize),
    Volume,
    Clock,
    Title,
}

#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub x0: f64,
    pub x1: f64,
    pub hit: Hit,
}

#[derive(Debug, Clone)]
pub struct TagInfo {
    pub label: String,
    pub active: bool,
    pub occupied: bool,
}

pub struct BarState {
    pub tags: Vec<TagInfo>,
    pub title: String,
    pub layout: String,
    pub cpu: u32,
    pub mem_used: f64,
    pub mem_total: f64,
    pub volume: Volume,
    pub clock: String,
}

pub struct Renderer {
    surface: cairo::Surface,
    /// Тот же объект, что и `surface`, но сырым указателем: cairo-rs 0.20 не
    /// экспортирует безопасный XlibSurface, а set_size есть только в C-API.
    raw: *mut cairo_sys::cairo_surface_t,
    ctx: Context,
    pango: pango::Context,
    font: FontDescription,
    pub regions: Vec<Region>,
}

impl Renderer {
    pub fn new(x: &X, cfg: &Config) -> Result<Self, String> {
        let raw = unsafe {
            cairo_sys::cairo_xlib_surface_create(
                x.dpy as *mut _,
                x.win as _,
                x.visual as *mut _,
                x.geom.w as i32,
                x.geom.h as i32,
            )
        };
        if raw.is_null() {
            return Err("cairo: не создаётся xlib-поверхность".into());
        }
        let surface = unsafe { cairo::Surface::from_raw_full(raw) }
            .map_err(|e| format!("cairo surface: {e}"))?;
        let ctx = Context::new(&surface).map_err(|e| format!("cairo context: {e}"))?;
        let pango = pangocairo::functions::create_context(&ctx);
        let font = FontDescription::from_string(&cfg.style.font);
        Ok(Self {
            surface,
            raw,
            ctx,
            pango,
            font,
            regions: Vec::new(),
        })
    }

    pub fn resize(&mut self, x: &X, cfg: &Config) {
        unsafe {
            cairo_sys::cairo_xlib_surface_set_size(self.raw, x.geom.w as i32, x.geom.h as i32);
        }
        self.font = FontDescription::from_string(&cfg.style.font);
    }

    fn set_color(&self, spec: &str) {
        let (r, g, b, a) = parse_color(spec);
        self.ctx.set_source_rgba(r, g, b, a);
    }

    /// Ширина текста в пикселях -- нужна и для right-группы (её надо разложить
    /// справа налево), и для центрирования.
    fn text_width(&self, text: &str) -> f64 {
        let layout = pango::Layout::new(&self.pango);
        layout.set_font_description(Some(&self.font));
        layout.set_text(text);
        layout.pixel_size().0 as f64
    }

    fn draw_text(&self, text: &str, x: f64, height: f64, color: &str) -> f64 {
        let layout = pango::Layout::new(&self.pango);
        layout.set_font_description(Some(&self.font));
        layout.set_text(text);
        let (w, h) = layout.pixel_size();
        self.set_color(color);
        self.ctx.move_to(x, (height - h as f64) / 2.0);
        pangocairo::functions::show_layout(&self.ctx, &layout);
        w as f64
    }

    fn rounded_rect(&self, x: f64, y: f64, w: f64, h: f64, r: f64) {
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

    pub fn draw(&mut self, cfg: &Config, st: &BarState, width: f64, height: f64) {
        self.regions.clear();

        let c = &self.ctx;
        // Сначала гасим всё окно в полную прозрачность, иначе за пределами
        // скруглённого фона останется мусор от прошлого кадра.
        c.set_operator(cairo::Operator::Source);
        c.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        c.paint().ok();
        self.set_color(&cfg.style.background);
        self.rounded_rect(0.0, 0.0, width, height, cfg.style.radius);
        c.fill().ok();
        c.set_operator(cairo::Operator::Over);

        let pad = cfg.style.padding;
        let gap = cfg.style.module_gap;

        // --- левая группа: слева направо
        let mut x = pad;
        for (i, name) in cfg.modules.left.iter().enumerate() {
            if i > 0 {
                x += gap;
            }
            x += self.draw_module(cfg, st, name, x, height);
        }
        let left_end = x;

        // --- правая группа: считаем ширины заранее и кладём от правого края
        let right_items: Vec<(String, f64)> = cfg
            .modules
            .right
            .iter()
            .map(|n| {
                let t = self.module_text(cfg, st, n);
                let w = self.text_width(&t);
                (t, w)
            })
            .collect();
        let right_total: f64 = right_items.iter().map(|(_, w)| w).sum::<f64>()
            + gap * right_items.len().saturating_sub(1) as f64;
        let mut rx = width - pad - right_total;
        let right_start = rx;
        for (i, (text, w)) in right_items.iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            let name = &cfg.modules.right[i];
            self.draw_text(text, rx, height, self.module_color(cfg, name));
            match name.as_str() {
                "volume" => self.regions.push(Region {
                    x0: rx,
                    x1: rx + w,
                    hit: Hit::Volume,
                }),
                "clock" => self.regions.push(Region {
                    x0: rx,
                    x1: rx + w,
                    hit: Hit::Clock,
                }),
                _ => {}
            }
            rx += w + gap;
        }

        // --- центр: то, что осталось между группами; заголовок режем многоточием
        let avail = right_start - left_end - 2.0 * gap;
        if avail > 20.0 {
            for name in cfg.modules.center.iter() {
                let text = self.module_text(cfg, st, name);
                if text.is_empty() {
                    continue;
                }
                let layout = pango::Layout::new(&self.pango);
                layout.set_font_description(Some(&self.font));
                layout.set_text(&text);
                layout.set_ellipsize(pango::EllipsizeMode::End);
                layout.set_width((avail * pango::SCALE as f64) as i32);
                let (tw, th) = layout.pixel_size();
                let cx = (width - tw as f64) / 2.0;
                let cx = cx.max(left_end + gap);
                self.set_color(self.module_color(cfg, name));
                self.ctx.move_to(cx, (height - th as f64) / 2.0);
                pangocairo::functions::show_layout(&self.ctx, &layout);
                if name == "title" {
                    self.regions.push(Region {
                        x0: cx,
                        x1: cx + tw as f64,
                        hit: Hit::Title,
                    });
                }
            }
        }

        self.surface.flush();
    }

    fn module_color<'a>(&self, cfg: &'a Config, name: &str) -> &'a str {
        match name {
            "clock" => &cfg.style.accent,
            _ => &cfg.style.foreground,
        }
    }

    /// Текстовое представление модуля. Теги рисуются отдельно (у них плашки),
    /// поэтому здесь возвращают пустую строку.
    fn module_text(&self, cfg: &Config, st: &BarState, name: &str) -> String {
        match name {
            "cpu" => format!("{} {}%", ICON_CPU, st.cpu),
            "ram" => format!(
                "{} {:.1}/{:.0}G",
                ICON_RAM, st.mem_used, st.mem_total
            ),
            "volume" => st.volume.label(),
            "clock" => st.clock.clone(),
            "title" => st.title.clone(),
            "layout" => st.layout.clone(),
            "tags" => String::new(),
            other => {
                let _ = cfg;
                format!("?{other}")
            }
        }
    }

    /// Возвращает занятую ширину. Теги -- единственный модуль со своей
    /// геометрией и кликабельными зонами.
    fn draw_module(&mut self, cfg: &Config, st: &BarState, name: &str, x: f64, height: f64) -> f64 {
        if name != "tags" {
            let text = self.module_text(cfg, st, name);
            if text.is_empty() {
                return 0.0;
            }
            return self.draw_text(&text, x, height, self.module_color(cfg, name));
        }

        let ip = cfg.tags.item_padding;
        let mut cx = x;
        for (i, tag) in st.tags.iter().enumerate() {
            if cfg.tags.hide_empty && !tag.occupied && !tag.active {
                continue;
            }
            let tw = self.text_width(&tag.label);
            let box_w = tw + 2.0 * ip;
            if tag.active {
                self.set_color(&cfg.tags.active_bg);
                self.rounded_rect(cx, 2.0, box_w, height - 4.0, cfg.style.radius.max(4.0));
                self.ctx.fill().ok();
            }
            let color = if tag.active {
                &cfg.tags.active_fg
            } else if tag.occupied {
                &cfg.tags.occupied_fg
            } else {
                &cfg.tags.empty_fg
            };
            self.draw_text(&tag.label, cx + ip, height, color);
            self.regions.push(Region {
                x0: cx,
                x1: cx + box_w,
                hit: Hit::Tag(i),
            });
            cx += box_w;
        }
        cx - x
    }

    pub fn hit_test(&self, x: f64) -> Option<Hit> {
        self.regions
            .iter()
            .find(|r| x >= r.x0 && x <= r.x1)
            .map(|r| r.hit)
    }
}
