use cairo::Context;
use pango::FontDescription;

use crate::config::{parse_color, Config};
use crate::modules::{Music, Net, Temps, Volume, ICON_CPU, ICON_RAM};
use crate::x::X;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Tag(usize),
    Volume,
    Clock,
    Title,
    Cpu,
    Ram,
    Temp,
    Net,
    Music,
}

/// Зона клика задаётся координатами вдоль оси бара: у горизонтального это X,
/// у вертикального Y. Поперёк оси бар узкий, и попасть мимо там нельзя.
#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub a0: f64,
    pub a1: f64,
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
    pub temps: Temps,
    /// None -- ни одного поднятого физического интерфейса.
    pub net: Option<Net>,
    pub music: Music,
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

    fn layout_for(&self, text: &str) -> pango::Layout {
        let layout = pango::Layout::new(&self.pango);
        layout.set_font_description(Some(&self.font));
        layout.set_text(text);
        layout
    }

    /// Размер текста в пикселях. Нужен и для групп, которые раскладываются от
    /// дальнего края, и для центрирования.
    fn text_size(&self, text: &str) -> (f64, f64) {
        let (w, h) = self.layout_for(text).pixel_size();
        (w as f64, h as f64)
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
        let vert = cfg.bar.position.vertical();
        // Ось бара: вдоль неё раскладываются модули, поперёк -- центрируются.
        let (along, across) = if vert {
            (height, width)
        } else {
            (width, height)
        };

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

        // --- первая группа: от начала оси (слева / сверху)
        let mut a = pad;
        for (i, name) in cfg.modules.left.iter().enumerate() {
            if i > 0 {
                a += gap;
            }
            a += self.draw_module(cfg, st, name, a, across, vert);
        }
        let head_end = a;

        // --- последняя группа: считаем размеры заранее и кладём от дальнего
        // края. Пустые модули выкидываем сразу: иначе их зазор всё равно
        // попадал бы в общий размер и вся группа уезжала бы от края на лишний
        // gap.
        let tail_items: Vec<(&String, String, f64)> = cfg
            .modules
            .right
            .iter()
            .filter_map(|n| {
                let t = self.module_text(cfg, st, n);
                if t.is_empty() {
                    return None;
                }
                let (w, _) = self.text_size(&t);
                Some((n, t, w))
            })
            .collect();
        let tail_total: f64 = tail_items.iter().map(|(_, _, s)| s).sum::<f64>()
            + gap * tail_items.len().saturating_sub(1) as f64;
        let mut ta = along - pad - tail_total;
        let tail_start = ta;
        for (name, text, size) in tail_items.iter() {
            self.draw_text(text, ta, across, self.module_color(cfg, name), vert);
            if let Some(hit) = module_hit(name) {
                self.regions.push(Region {
                    a0: ta,
                    a1: ta + size,
                    hit,
                });
            }
            ta += size + gap;
        }

        // --- центр: то, что осталось между группами; заголовок режем многоточием
        let avail = tail_start - head_end - 2.0 * gap;
        if avail > 20.0 {
            for name in cfg.modules.center.iter() {
                let text = self.module_text(cfg, st, name);
                if text.is_empty() {
                    continue;
                }
                let layout = self.layout_for(&text);
                // Заголовок режем по свободному месту вдоль оси -- и у
                // горизонтального бара, и у вертикального, где строка
                // повёрнута и тоже тянется вдоль оси.
                layout.set_ellipsize(pango::EllipsizeMode::End);
                layout.set_width((avail.max(1.0) * pango::SCALE as f64) as i32);
                let (tw, th) = layout.pixel_size();
                let size = tw as f64;
                let ca = ((along - size) / 2.0).max(head_end + gap);
                self.set_color(self.module_color(cfg, name));
                self.show_layout(&layout, ca, across, th as f64, vert);
                if let Some(hit) = module_hit(name) {
                    self.regions.push(Region {
                        a0: ca,
                        a1: ca + size,
                        hit,
                    });
                }
            }
        }

        self.surface.flush();
    }

    /// Рисует текст, центрируя его поперёк оси бара. Возвращает размер вдоль оси.
    fn draw_text(&self, text: &str, a: f64, across: f64, color: &str, vert: bool) -> f64 {
        let layout = self.layout_for(text);
        let (w, h) = layout.pixel_size();
        self.set_color(color);
        self.show_layout(&layout, a, across, h as f64, vert);
        w as f64
    }

    /// Кладёт готовый layout так, чтобы строка шла вдоль оси бара, начинаясь с
    /// координаты `a`, и была отцентрована поперёк неё.
    ///
    /// У вертикального бара строку приходится поворачивать: в 28 пикселей
    /// ширины не влезает ни время, ни заголовок окна, а раскладывать модули
    /// столбиком из горизонтальных строк -- значит обрезать каждый из них.
    /// Поворот на 90 градусов по часовой выбран, чтобы текст читался сверху
    /// вниз, в том же направлении, в котором идут сами модули.
    fn show_layout(&self, layout: &pango::Layout, a: f64, across: f64, th: f64, vert: bool) {
        let c = &self.ctx;
        if !vert {
            c.move_to(a, (across - th) / 2.0);
            pangocairo::functions::show_layout(c, layout);
            return;
        }
        c.save().ok();
        // После поворота локальная ось X смотрит вниз экрана, а локальная Y --
        // влево, поэтому начало координат сдвигаем на полвысоты строки вправо
        // от центра бара.
        c.translate((across + th) / 2.0, a);
        c.rotate(std::f64::consts::FRAC_PI_2);
        c.move_to(0.0, 0.0);
        pangocairo::functions::show_layout(c, layout);
        c.restore().ok();
    }

    fn module_color<'a>(&self, cfg: &'a Config, name: &str) -> &'a str {
        match name {
            "clock" => &cfg.style.accent,
            "music" => &cfg.style.accent,
            _ => &cfg.style.foreground,
        }
    }

    /// Текстовое представление модуля. Теги рисуются отдельно (у них плашки),
    /// поэтому здесь возвращают пустую строку.
    fn module_text(&self, cfg: &Config, st: &BarState, name: &str) -> String {
        match name {
            "cpu" => format!("{} {}%", ICON_CPU, st.cpu),
            "ram" => format!("{} {:.1}/{:.0}G", ICON_RAM, st.mem_used, st.mem_total),
            "volume" => st.volume.label(),
            "clock" => st.clock.clone(),
            "title" => st.title.clone(),
            "temp" => st.temps.label(),
            // Сети нет -- модуль исчезает, а не показывает прочерк: на
            // десктопе это состояние редкое и заметное само по себе.
            "net" => st.net.as_ref().map(|n| n.label()).unwrap_or_default(),
            "music" => st.music.label(cfg.music.max_chars),
            "layout" => st.layout.clone(),
            "tags" => String::new(),
            other => format!("?{other}"),
        }
    }

    /// Возвращает занятый размер вдоль оси. Теги -- единственный модуль со
    /// своей геометрией и кликабельными зонами.
    fn draw_module(
        &mut self,
        cfg: &Config,
        st: &BarState,
        name: &str,
        a: f64,
        across: f64,
        vert: bool,
    ) -> f64 {
        if name != "tags" {
            let text = self.module_text(cfg, st, name);
            if text.is_empty() {
                return 0.0;
            }
            let size = self.draw_text(&text, a, across, self.module_color(cfg, name), vert);
            if let Some(hit) = module_hit(name) {
                self.regions.push(Region {
                    a0: a,
                    a1: a + size,
                    hit,
                });
            }
            return size;
        }

        let ip = cfg.tags.item_padding;
        let mut ca = a;
        for (i, tag) in st.tags.iter().enumerate() {
            if cfg.tags.hide_empty && !tag.occupied && !tag.active {
                continue;
            }
            let (tw, _) = self.text_size(&tag.label);
            // Плашка растёт вдоль оси на поля с обеих сторон, а поперёк
            // прижимается к кромкам бара с зазором в 2px.
            let box_a = tw + 2.0 * ip;
            if tag.active {
                let (bx, by, bw, bh) = if vert {
                    (2.0, ca, across - 4.0, box_a)
                } else {
                    (ca, 2.0, box_a, across - 4.0)
                };
                self.set_color(&cfg.tags.active_bg);
                self.rounded_rect(bx, by, bw, bh, cfg.style.radius.max(4.0));
                self.ctx.fill().ok();
            }
            let color = if tag.active {
                &cfg.tags.active_fg
            } else if tag.occupied {
                &cfg.tags.occupied_fg
            } else {
                &cfg.tags.empty_fg
            };
            self.draw_text(&tag.label, ca + ip, across, color, vert);
            self.regions.push(Region {
                a0: ca,
                a1: ca + box_a,
                hit: Hit::Tag(i),
            });
            ca += box_a;
        }
        ca - a
    }

    /// pos -- координата вдоль оси бара: X у горизонтального, Y у вертикального.
    pub fn hit_test(&self, pos: f64) -> Option<Hit> {
        self.regions
            .iter()
            .find(|r| pos >= r.a0 && pos <= r.a1)
            .map(|r| r.hit)
    }

    /// Границы зоны модуля вдоль оси -- по ним попап выравнивается с модулем.
    pub fn region_of(&self, hit: Hit) -> Option<(f64, f64)> {
        self.regions
            .iter()
            .find(|r| r.hit == hit)
            .map(|r| (r.a0, r.a1))
    }
}

/// Кликабельные модули: по ним открываются выдвижные виджеты.
fn module_hit(name: &str) -> Option<Hit> {
    match name {
        "volume" => Some(Hit::Volume),
        "clock" => Some(Hit::Clock),
        "cpu" => Some(Hit::Cpu),
        "ram" => Some(Hit::Ram),
        "title" => Some(Hit::Title),
        "temp" => Some(Hit::Temp),
        "net" => Some(Hit::Net),
        "music" => Some(Hit::Music),
        _ => None,
    }
}
