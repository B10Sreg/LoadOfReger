use std::time::{Duration, Instant};

use cairo::Context;
use pango::FontDescription;

use crate::config::{parse_color, Config};
use crate::modules::{
    Battery, Disk, Mic, Music, Net, Temps, Volume, ICON_CPU, ICON_POWER, ICON_RAM,
};
use crate::x::X;

/// Зазор между строками модуля у вертикального бара: значок, под ним
/// значение. Вплотную они слипаются в одно пятно.
const LINE_GAP: f64 = 1.0;

/// Поля по бокам прямой строки у вертикального бара: вплотную к кромке текст
/// выглядит обрезанным, даже когда помещается целиком.
const UPRIGHT_PAD: f64 = 3.0;

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
    Disk,
    Battery,
    Uptime,
    Mic,
    Power,
}

/// Зона клика задаётся координатами вдоль оси бара: у горизонтального это X,
/// у вертикального Y. Поперёк оси бар узкий, и попасть мимо там нельзя.
#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub a0: f64,
    pub a1: f64,
    pub hit: Hit,
}

/// Готовый к отрисовке модуль: строки и признак поворота. У горизонтального
/// бара строка всегда одна и не поворачивается.
struct Piece {
    lines: Vec<String>,
    rotate: bool,
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
    /// None -- путь из [disk] не читается: раздел отмонтировали или он не
    /// существует. Модуль тогда исчезает, как и сеть.
    pub disk: Option<Disk>,
    /// None -- батареи в системе нет (десктоп).
    pub battery: Option<Battery>,
    pub mic: Mic,
}

/// Переезд плашки активного тега. Направление никуда не записано: плашка
/// едет из прошлого положения в новое, а оси задаёт сам бар -- у
/// горизонтального это влево-вправо, у вертикального вверх-вниз.
struct TagAnim {
    /// Начало и размер плашки вдоль оси бара.
    from: (f64, f64),
    to: (f64, f64),
    /// Тег, с которого уехали: пока плашка в пути, его подпись гаснет.
    prev: usize,
    start: Instant,
    dur: Duration,
}

impl TagAnim {
    fn progress(&self) -> f64 {
        let t = (self.start.elapsed().as_secs_f64() / self.dur.as_secs_f64()).clamp(0.0, 1.0);
        // Ease-in-out: плашка трогается и подъезжает мягко. Чистый ease-out
        // укладывал половину пути в первую пятую времени -- на длинном
        // переезде вертикального бара (через полколонки тегов) это читалось
        // как прыжок с последующим доползанием.
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    }

    fn done(&self) -> bool {
        self.start.elapsed() >= self.dur
    }

    fn value(&self) -> (f64, f64) {
        let t = self.progress();
        let lerp = |a: f64, b: f64| a + (b - a) * t;
        (lerp(self.from.0, self.to.0), lerp(self.from.1, self.to.1))
    }
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
    /// Активный тег на прошлом кадре и целевая геометрия его плашки вдоль оси.
    active: Option<usize>,
    active_box: Option<(f64, f64)>,
    anim: Option<TagAnim>,
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
            active: None,
            active_box: None,
            anim: None,
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

    /// Цвет между двумя: подсветка тега перетекает со старого на новый, пока
    /// плашка едет. Без этого на полпути ярких тегов было бы два.
    fn set_color_mix(&self, from: &str, to: &str, t: f64) {
        let (r0, g0, b0, a0) = parse_color(from);
        let (r1, g1, b1, a1) = parse_color(to);
        let t = t.clamp(0.0, 1.0);
        let m = |a: f64, b: f64| a + (b - a) * t;
        self.ctx
            .set_source_rgba(m(r0, r1), m(g0, g1), m(b0, b1), m(a0, a1));
    }

    fn layout_for(&self, text: &str) -> pango::Layout {
        let layout = pango::Layout::new(&self.pango);
        layout.set_font_description(Some(&self.font));
        layout.set_text(text);
        layout
    }

    /// Поворачивается ли модуль у вертикального бара. Прямой текст влезает в
    /// колонку шириной с бар только коротким: заголовок окна и название трека
    /// там не помещаются никак, поэтому на бок ложатся только они. Всё
    /// остальное стоит прямо -- повёрнутые проценты и часы читать невозможно.
    fn rotated(name: &str) -> bool {
        matches!(name, "title" | "music")
    }

    /// Строки модуля для вертикального бара. Горизонтальная подпись
    /// разбирается на значок и значение: в столбик они занимают ширину
    /// иконки, а не всей строки.
    fn module_lines(&self, cfg: &Config, st: &BarState, name: &str) -> Vec<String> {
        match name {
            "cpu" => vec![ICON_CPU.to_string(), st.cpu.to_string()],
            "ram" => vec![ICON_RAM.to_string(), format!("{:.1}", st.mem_used)],
            "volume" => st.volume.lines(),
            "temp" => st.temps.lines(),
            "net" => st.net.as_ref().map(|n| n.lines()).unwrap_or_default(),
            "disk" => st.disk.as_ref().map(|d| d.lines()).unwrap_or_default(),
            "battery" => st.battery.as_ref().map(|b| b.lines()).unwrap_or_default(),
            "mic" => st.mic.lines(),
            "uptime" => crate::modules::uptime_lines(),
            // Часы разбираются по двоеточию и пробелу: «16 Sep 11:20» встаёт
            // в четыре узкие строки вместо одной, которая в колонку не лезет.
            "clock" => st
                .clock
                .split([':', ' '])
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect(),
            _ => {
                let t = self.module_text(cfg, st, name);
                if t.is_empty() {
                    Vec::new()
                } else {
                    vec![t]
                }
            }
        }
    }

    /// Что и как рисует модуль в текущей ориентации.
    fn piece(&self, cfg: &Config, st: &BarState, name: &str, vert: bool) -> Piece {
        let rotate = vert && Self::rotated(name);
        if vert && !rotate {
            Piece {
                lines: self.module_lines(cfg, st, name),
                rotate: false,
            }
        } else {
            let t = self.module_text(cfg, st, name);
            Piece {
                lines: if t.is_empty() { Vec::new() } else { vec![t] },
                rotate,
            }
        }
    }

    /// Размер куска вдоль оси бара. Прямые строки складываются высотами,
    /// повёрнутая (и любая горизонтальная) меряется длиной.
    fn piece_size(&self, p: &Piece, vert: bool) -> f64 {
        if p.lines.is_empty() {
            return 0.0;
        }
        if vert && !p.rotate {
            let mut sum = LINE_GAP * (p.lines.len() - 1) as f64;
            for l in &p.lines {
                sum += self.layout_for(l).pixel_size().1 as f64;
            }
            sum
        } else {
            self.layout_for(&p.lines[0]).pixel_size().0 as f64
        }
    }

    /// Кладёт кусок, начиная с координаты `a` вдоль оси бара, центрируя его
    /// поперёк. Возвращает занятый размер вдоль оси.
    fn draw_piece(&self, p: &Piece, a: f64, across: f64, color: &str, vert: bool) -> f64 {
        if p.lines.is_empty() {
            return 0.0;
        }
        self.set_color(color);
        if !vert {
            let layout = self.layout_for(&p.lines[0]);
            let (tw, th) = layout.pixel_size();
            self.ctx.move_to(a, (across - th as f64) / 2.0);
            pangocairo::functions::show_layout(&self.ctx, &layout);
            return tw as f64;
        }
        if p.rotate {
            let layout = self.layout_for(&p.lines[0]);
            let (tw, th) = layout.pixel_size();
            self.show_rotated(&layout, a, across, th as f64);
            return tw as f64;
        }
        let mut y = a;
        for (i, line) in p.lines.iter().enumerate() {
            if i > 0 {
                y += LINE_GAP;
            }
            let layout = self.upright_layout(line, across);
            let (tw, th) = layout.pixel_size();
            self.ctx.move_to((across - tw as f64) / 2.0, y);
            pangocairo::functions::show_layout(&self.ctx, &layout);
            y += th as f64;
        }
        y - a
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
        // Кадр собирается в отдельной группе и выкладывается в окно одним
        // блитом. Без этого композитор успевал поймать окно на полпути --
        // с погашенным фоном и плашкой, нарисованной уже на новом месте.
        self.ctx.push_group();
        let vert = cfg.bar.position.vertical();
        // Ось бара: вдоль неё раскладываются модули, поперёк -- центрируются.
        let (along, across) = if vert {
            (height, width)
        } else {
            (width, height)
        };

        let c = &self.ctx;
        self.set_color(&cfg.style.background);
        self.rounded_rect(0.0, 0.0, width, height, cfg.style.radius);
        c.fill().ok();

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
        let tail_items: Vec<(&String, Piece, f64)> = cfg
            .modules
            .right
            .iter()
            .filter_map(|n| {
                let p = self.piece(cfg, st, n, vert);
                if p.lines.is_empty() {
                    return None;
                }
                let size = self.piece_size(&p, vert);
                Some((n, p, size))
            })
            .collect();
        let tail_total: f64 = tail_items.iter().map(|(_, _, s)| s).sum::<f64>()
            + gap * tail_items.len().saturating_sub(1) as f64;
        let mut ta = along - pad - tail_total;
        let tail_start = ta;
        for (name, piece, size) in tail_items.iter() {
            self.draw_piece(piece, ta, across, self.module_color(cfg, name), vert);
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
                // Заголовок режем по свободному месту вдоль оси. Он всегда
                // одна строка -- прямая у горизонтального бара, повёрнутая у
                // вертикального, -- поэтому вдоль оси его меряет длина.
                layout.set_ellipsize(pango::EllipsizeMode::End);
                layout.set_width((avail.max(1.0) * pango::SCALE as f64) as i32);
                let (tw, th) = layout.pixel_size();
                let size = tw as f64;
                let ca = ((along - size) / 2.0).max(head_end + gap);
                self.set_color(self.module_color(cfg, name));
                if vert {
                    self.show_rotated(&layout, ca, across, th as f64);
                } else {
                    self.show_upright(&layout, ca, across, vert);
                }
                if let Some(hit) = module_hit(name) {
                    self.regions.push(Region {
                        a0: ca,
                        a1: ca + size,
                        hit,
                    });
                }
            }
        }

        // Готовый кадр кладём поверх окна целиком и операцией Source:
        // за пределами скруглённого фона должна остаться прозрачность, а не
        // то, что было в окне прошлым кадром.
        let c = &self.ctx;
        c.pop_group_to_source().ok();
        c.set_operator(cairo::Operator::Source);
        c.paint().ok();
        c.set_operator(cairo::Operator::Over);
        self.surface.flush();
    }

    /// Рисует модуль по имени, центрируя поперёк оси бара. Возвращает
    /// занятый размер вдоль оси.
    fn draw_named(
        &self,
        cfg: &Config,
        st: &BarState,
        name: &str,
        a: f64,
        across: f64,
        vert: bool,
    ) -> f64 {
        let p = self.piece(cfg, st, name, vert);
        self.draw_piece(&p, a, across, self.module_color(cfg, name), vert)
    }

    /// Кладёт строку на бок: она читается сверху вниз и занимает колонку
    /// шириной в высоту шрифта. Так рисуются только длинные модули.
    ///
    /// После rotate(+90°) ось X раскладки смотрит вниз экрана, ось Y -- влево.
    /// Значит начало строки задаёт Y = a, а её высота уходит влево от точки
    /// переноса -- отсюда сдвиг на (across + th) / 2.
    fn show_rotated(&self, layout: &pango::Layout, a: f64, across: f64, th: f64) {
        let c = &self.ctx;
        c.save().ok();
        c.translate((across + th) / 2.0, a);
        c.rotate(std::f64::consts::FRAC_PI_2);
        c.move_to(0.0, 0.0);
        pangocairo::functions::show_layout(c, layout);
        c.restore().ok();
    }

    /// Раскладка прямой строки для вертикального бара: шире панели она быть
    /// не может, иначе кромка срезала бы хвост без всякого знака, что текст
    /// продолжается. Многоточие -- такой знак.
    fn upright_layout(&self, text: &str, across: f64) -> pango::Layout {
        let layout = self.layout_for(text);
        let avail = (across - 2.0 * UPRIGHT_PAD).max(1.0);
        if layout.pixel_size().0 as f64 > avail {
            layout.set_ellipsize(pango::EllipsizeMode::End);
            layout.set_width((avail * pango::SCALE as f64) as i32);
        }
        layout
    }

    /// Одна прямая строка, отцентрованная поперёк оси бара. У вертикального
    /// центрируется по ширине панели, у горизонтального -- по её высоте.
    fn show_upright(&self, layout: &pango::Layout, a: f64, across: f64, vert: bool) {
        let (tw, th) = layout.pixel_size();
        if vert {
            self.ctx.move_to((across - tw as f64) / 2.0, a);
        } else {
            self.ctx.move_to(a, (across - th as f64) / 2.0);
        }
        pangocairo::functions::show_layout(&self.ctx, layout);
    }

    fn module_color<'a>(&self, cfg: &'a Config, name: &str) -> &'a str {
        match name {
            "clock" => &cfg.style.accent,
            "music" => &cfg.style.accent,
            // Кнопка питания -- единственный модуль, который что-то делает, а
            // не показывает: акцентом она читается как кнопка, а не как цифры.
            "power" => &cfg.style.accent,
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
            // Диска и батареи нет -- модуля тоже нет, как и у сети.
            "disk" => st.disk.as_ref().map(|d| d.label()).unwrap_or_default(),
            "battery" => st.battery.as_ref().map(|b| b.label()).unwrap_or_default(),
            "mic" => st.mic.label(),
            "uptime" => crate::modules::uptime_label(),
            // Кнопка: только значок, значения у неё нет.
            "power" => ICON_POWER.to_string(),
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
            let size = self.draw_named(cfg, st, name, a, across, vert);
            if size <= 0.0 {
                return 0.0;
            }
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
        // Сначала геометрия всех плашек, потом рисование: плашка активного
        // тега едет из прошлого положения в новое, и для этого нужно знать
        // цель до того, как начнём что-то класть на поверхность.
        let mut boxes: Vec<(usize, f64, f64)> = Vec::new();
        let mut ca = a;
        for (i, tag) in st.tags.iter().enumerate() {
            if cfg.tags.hide_empty && !tag.occupied && !tag.active {
                continue;
            }
            let label = tag_label(&tag.label, vert);
            let layout = self.layout_for(label);
            // Плашка растёт вдоль оси на поля с обеих сторон, а поперёк
            // прижимается к кромкам бара с зазором в 2px. Подпись тега короткая
            // и стоит прямо в обеих ориентациях, поэтому у вертикального бара
            // вдоль оси её меряет высота строки.
            let (lw, lh) = layout.pixel_size();
            let label_a = if vert { lh as f64 } else { lw as f64 };
            let box_a = label_a + 2.0 * ip;
            boxes.push((i, ca, box_a));
            ca += box_a;
        }

        let active = st.tags.iter().position(|t| t.active);
        let target = active.and_then(|ai| {
            boxes
                .iter()
                .find(|(i, _, _)| *i == ai)
                .map(|(_, a0, size)| (*a0, *size))
        });
        self.sync_anim(cfg, active, target);

        // Плашка одна и она едет; рисуем её до подписей, чтобы текст остался
        // сверху.
        if let Some((ba, bsize)) = self.visual_box() {
            let (bx, by, bw, bh) = if vert {
                (2.0, ba, across - 4.0, bsize)
            } else {
                (ba, 2.0, bsize, across - 4.0)
            };
            self.set_color(&cfg.tags.active_bg);
            self.rounded_rect(bx, by, bw, bh, cfg.style.radius.max(4.0));
            self.ctx.fill().ok();
        }

        let (t, moved_from) = match &self.anim {
            Some(an) => (an.progress(), Some(an.prev)),
            None => (1.0, None),
        };
        for (i, a0, box_a) in boxes {
            let tag = &st.tags[i];
            let rest = if tag.occupied {
                &cfg.tags.occupied_fg
            } else {
                &cfg.tags.empty_fg
            };
            let (from, to) = if tag.active {
                (rest, &cfg.tags.active_fg)
            } else if Some(i) == moved_from {
                (&cfg.tags.active_fg, rest)
            } else {
                (rest, rest)
            };
            self.set_color_mix(from, to, t);
            let layout = if vert {
                self.upright_layout(tag_label(&tag.label, vert), across)
            } else {
                self.layout_for(&tag.label)
            };
            self.show_upright(&layout, a0 + ip, across, vert);
            self.regions.push(Region {
                a0,
                a1: a0 + box_a,
                hit: Hit::Tag(i),
            });
        }
        ca - a
    }

    /// Заводит переезд плашки, когда активный тег сменился, и подтягивает уже
    /// начатый к новой цели.
    fn sync_anim(&mut self, cfg: &Config, active: Option<usize>, target: Option<(f64, f64)>) {
        let base = Duration::from_millis(cfg.tags.anim_ms);
        if active != self.active {
            // Едем не от целевой плашки прошлого тега, а оттуда, где застали
            // предыдущую анимацию: иначе быстрые переключения дёргают картинку.
            let from = self.visual_box();
            self.anim = match (self.active, from, target) {
                (Some(prev), Some(from), Some(to)) if !base.is_zero() && from != to => {
                    Some(TagAnim {
                        from,
                        to,
                        prev,
                        start: Instant::now(),
                        dur: travel_dur(base, from, to),
                    })
                }
                _ => None,
            };
            self.active = active;
        }
        self.active_box = target;
        // Цель могла сдвинуться на ходу: сменился шрифт, спрятались пустые
        // теги. Дотягиваем к новой, а не к устаревшей.
        if let (Some(an), Some(to)) = (self.anim.as_mut(), target) {
            an.to = to;
        }
        if self.anim.as_ref().is_some_and(|an| an.done()) {
            self.anim = None;
        }
    }

    /// Где плашка активного тега находится сейчас: на цели или на полпути.
    fn visual_box(&self) -> Option<(f64, f64)> {
        match &self.anim {
            Some(an) => Some(an.value()),
            None => self.active_box,
        }
    }

    /// Идёт ли переезд плашки. Основной цикл по этому флагу перерисовывает бар
    /// кадрами, а не по тику опроса модулей.
    pub fn animating(&self) -> bool {
        self.anim.is_some()
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

/// Сколько едет плашка. anim_ms -- это переезд на соседний тег; через полколонки
/// расстояние вчетверо больше, и за те же 160 мс плашка не едет, а телепортируется.
/// Растим по корню из отношения и не больше чем втрое: линейный рост превратил бы
/// прыжок с первого тега на девятый в долгую поездку, которую ждёшь.
fn travel_dur(base: Duration, from: (f64, f64), to: (f64, f64)) -> Duration {
    let hop = to.1.max(1.0);
    let k = ((to.0 - from.0).abs() / hop).max(1.0).sqrt().min(3.0);
    base.mul_f64(k)
}

/// Подпись тега под ориентацию. Метки вида «1:dev» в колонку шириной с бар не
/// влезают, а номер там и не нужен: порядок тегов виден и так, сверху вниз.
/// Поэтому у вертикального бара остаётся только название.
fn tag_label(label: &str, vert: bool) -> &str {
    match label.split_once(':') {
        Some((_, name)) if vert && !name.is_empty() => name,
        _ => label,
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
        "disk" => Some(Hit::Disk),
        "battery" => Some(Hit::Battery),
        "uptime" => Some(Hit::Uptime),
        "mic" => Some(Hit::Mic),
        "power" => Some(Hit::Power),
        _ => None,
    }
}
