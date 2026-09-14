use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Конфиг целиком сериализуем в обе стороны: vxbar его читает, vxbar-settings
/// перезаписывает. Поэтому каждое поле имеет #[serde(default)] -- частичный
/// TOML, написанный руками, не должен ронять бар.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub bar: Bar,
    pub style: Style,
    pub tags: Tags,
    pub modules: Modules,
    pub clock: Clock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Bar {
    pub position: Position,
    pub height: u32,
    /// Отступ от края экрана вдоль оси бара и поперёк неё. Ненулевой margin_edge
    /// делает бар "плавающим": струт всё равно резервирует height + margin_edge.
    pub margin_edge: i32,
    pub margin_side: i32,
    /// Индекс монитора в порядке Xinerama; 0 -- первый.
    pub monitor: usize,
    /// Период обновления «медленных» модулей (cpu/ram/часы), в миллисекундах.
    pub interval_ms: u64,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            position: Position::Top,
            height: 28,
            margin_edge: 0,
            margin_side: 0,
            monitor: 0,
            interval_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    pub background: String,
    pub foreground: String,
    pub accent: String,
    pub muted: String,
    pub font: String,
    /// Скругление углов бара. Имеет смысл в основном при ненулевых margin.
    pub radius: f64,
    /// Внутренние поля бара слева/справа.
    pub padding: f64,
    /// Расстояние между соседними модулями.
    pub module_gap: f64,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            background: "#1e1e20".into(),
            foreground: "#8a8a8e".into(),
            accent: "#c0c0c0".into(),
            muted: "#5a5a5e".into(),
            font: "JetBrainsMono Nerd Font 10".into(),
            radius: 0.0,
            padding: 10.0,
            module_gap: 14.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Tags {
    /// Подписи тегов. Пусто -- берём _NET_DESKTOP_NAMES с root-окна.
    pub labels: Vec<String>,
    pub active_bg: String,
    pub active_fg: String,
    pub occupied_fg: String,
    pub empty_fg: String,
    /// Прятать теги, на которых нет окон и которые не активны.
    pub hide_empty: bool,
    pub item_padding: f64,
}

impl Default for Tags {
    fn default() -> Self {
        Self {
            labels: Vec::new(),
            active_bg: "#2a2a2d".into(),
            active_fg: "#c0c0c0".into(),
            occupied_fg: "#8a8a8e".into(),
            empty_fg: "#5a5a5e".into(),
            hide_empty: false,
            item_padding: 9.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Modules {
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
}

impl Default for Modules {
    fn default() -> Self {
        Self {
            left: vec!["tags".into()],
            center: vec!["title".into()],
            right: vec![
                "cpu".into(),
                "ram".into(),
                "volume".into(),
                "clock".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Clock {
    pub format: String,
    /// Команда по клику -- всплывающий календарь и т.п.
    pub on_click: String,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            format: "%a %d %b  %H:%M".into(),
            on_click: String::new(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("VXBAR_CONFIG") {
            return PathBuf::from(p);
        }
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".config")
            });
        base.join("vxbar").join("config.toml")
    }

    /// Отсутствующий или битый конфиг -- не повод падать: бар должен подняться
    /// на дефолтах, иначе кривая правка из редактора оставит сессию без панели.
    pub fn load(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("vxbar: не читается {}: {e}", path.display());
                }
                return Self::default();
            }
        };
        match toml::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("vxbar: ошибка в {}: {e}", path.display());
                eprintln!("vxbar: поднимаюсь на дефолтном конфиге");
                Self::default()
            }
        }
    }

    pub fn write_default_if_missing(path: &Path) -> std::io::Result<()> {
        if path.exists() {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(&Self::default())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, text)
    }
}

/// "#rrggbb" / "#rgb" / "#rrggbbaa" -> (r, g, b, a) в диапазоне 0..1.
pub fn parse_color(s: &str) -> (f64, f64, f64, f64) {
    let h = s.trim().trim_start_matches('#');
    let n = |i: usize, len: usize| -> f64 {
        let sl = &h[i..i + len];
        let v = u8::from_str_radix(&sl.repeat(3 - len), 16).unwrap_or(0);
        v as f64 / 255.0
    };
    match h.len() {
        3 => (n(0, 1), n(1, 1), n(2, 1), 1.0),
        6 => (n(0, 2), n(2, 2), n(4, 2), 1.0),
        8 => (n(0, 2), n(2, 2), n(4, 2), n(6, 2)),
        _ => (1.0, 0.0, 1.0, 1.0), // заведомо заметная маджента вместо тихого чёрного
    }
}
