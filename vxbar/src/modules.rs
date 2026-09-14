use std::process::Command;
use std::time::{Duration, Instant};

/// Иконки Nerd Font -- те же, что были в старом statusbar.sh.
pub const ICON_CPU: &str = "\u{f4bc}";
pub const ICON_RAM: &str = "\u{f035b}";
pub const ICON_VOL_MUTE: &str = "\u{f075f}";
pub const ICON_VOL_LOW: &str = "\u{f057f}";
pub const ICON_VOL_HIGH: &str = "\u{f057e}";

#[derive(Default)]
pub struct CpuMeter {
    prev_total: u64,
    prev_idle: u64,
}

impl CpuMeter {
    /// Первый вызов вернёт 0: процент считается как дельта между замерами,
    /// мгновенного значения в /proc/stat просто нет.
    pub fn sample(&mut self) -> u32 {
        let text = match std::fs::read_to_string("/proc/stat") {
            Ok(t) => t,
            Err(_) => return 0,
        };
        let line = match text.lines().next() {
            Some(l) if l.starts_with("cpu ") => l,
            _ => return 0,
        };
        let v: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|f| f.parse().ok())
            .collect();
        if v.len() < 8 {
            return 0;
        }
        let idle = v[3] + v[4];
        let total: u64 = v.iter().take(8).sum();
        let dt = total.saturating_sub(self.prev_total);
        let di = idle.saturating_sub(self.prev_idle);
        self.prev_total = total;
        self.prev_idle = idle;
        if dt == 0 {
            return 0;
        }
        ((100 * (dt - di)) / dt) as u32
    }
}

pub struct Mem {
    pub used_gb: f64,
    pub total_gb: f64,
}

pub fn mem() -> Mem {
    let mut total = 0u64;
    let mut avail = 0u64;
    if let Ok(t) = std::fs::read_to_string("/proc/meminfo") {
        for line in t.lines() {
            let mut it = line.split_whitespace();
            match (it.next(), it.next()) {
                (Some("MemTotal:"), Some(v)) => total = v.parse().unwrap_or(0),
                (Some("MemAvailable:"), Some(v)) => avail = v.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    let to_gb = |kb: u64| kb as f64 / 1024.0 / 1024.0;
    Mem {
        used_gb: to_gb(total.saturating_sub(avail)),
        total_gb: to_gb(total),
    }
}

#[derive(Clone, Copy)]
pub struct Volume {
    pub percent: u32,
    pub muted: bool,
}

impl Volume {
    pub fn icon(&self) -> &'static str {
        if self.muted {
            ICON_VOL_MUTE
        } else if self.percent >= 34 {
            ICON_VOL_HIGH
        } else {
            ICON_VOL_LOW
        }
    }

    pub fn label(&self) -> String {
        if self.muted {
            format!("{} mute", self.icon())
        } else {
            format!("{} {}%", self.icon(), self.percent)
        }
    }
}

/// wpctl -- внешний процесс, поэтому результат кэшируется: дёргать его на
/// каждую перерисовку (Expose, смена заголовка окна) слишком дорого.
pub struct VolumeCache {
    value: Volume,
    fetched: Option<Instant>,
    ttl: Duration,
}

impl VolumeCache {
    pub fn new() -> Self {
        Self {
            value: Volume {
                percent: 0,
                muted: false,
            },
            fetched: None,
            ttl: Duration::from_millis(500),
        }
    }

    pub fn get(&mut self) -> Volume {
        let stale = match self.fetched {
            Some(t) => t.elapsed() > self.ttl,
            None => true,
        };
        if stale {
            self.value = read_volume().unwrap_or(self.value);
            self.fetched = Some(Instant::now());
        }
        self.value
    }

    pub fn invalidate(&mut self) {
        self.fetched = None;
    }
}

fn read_volume() -> Option<Volume> {
    let out = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    // формат: "Volume: 0.65 [MUTED]"
    let muted = s.contains("MUTED");
    let v: f32 = s.split_whitespace().nth(1)?.parse().ok()?;
    Some(Volume {
        percent: (v * 100.0).round() as u32,
        muted,
    })
}

pub fn set_volume_step(delta: i32) {
    let arg = if delta >= 0 {
        format!("{}%+", delta)
    } else {
        format!("{}%-", -delta)
    };
    let _ = Command::new("wpctl")
        .args(["set-volume", "-l", "1.5", "@DEFAULT_AUDIO_SINK@", &arg])
        .status();
}

pub fn toggle_mute() {
    let _ = Command::new("wpctl")
        .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
        .status();
}

/// strftime через libc: тянуть chrono ради одной строки не хочется.
pub fn clock(format: &str) -> String {
    use std::ffi::CString;
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        let fmt = match CString::new(format) {
            Ok(f) => f,
            Err(_) => return String::new(),
        };
        let mut buf = vec![0u8; 256];
        let n = libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            fmt.as_ptr(),
            &tm,
        );
        buf.truncate(n);
        String::from_utf8_lossy(&buf).into_owned()
    }
}
