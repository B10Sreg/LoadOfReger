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

/// Процесс в списке «топ» выдвижного виджета.
pub struct Proc {
    pub name: String,
    /// Проценты CPU либо гигабайты RSS -- смотря чем сортировали.
    pub value: f64,
}

/// Считает загрузку CPU по процессам между двумя замерами. Мгновенного
/// значения в /proc нет: там только накопленное время, поэтому без хранения
/// прошлого среза пришлось бы показывать среднее за всю жизнь процесса.
#[derive(Default)]
pub struct ProcSampler {
    prev: std::collections::HashMap<i32, u64>,
    prev_total: u64,
}

impl ProcSampler {
    /// Пустой список означает «замера ещё нет»: доля CPU считается только по
    /// разнице двух срезов, и на первом вызове показывать было бы нечего,
    /// кроме честных нулей у всех процессов сразу.
    pub fn top_cpu(&mut self, n: usize) -> Vec<Proc> {
        let first = self.prev.is_empty();
        let total = total_cpu_ticks();
        let dt = total.saturating_sub(self.prev_total);
        let mut cur = std::collections::HashMap::new();
        let mut out = Vec::new();
        for (pid, name, ticks) in proc_ticks() {
            cur.insert(pid, ticks);
            let prev = self.prev.get(&pid).copied().unwrap_or(ticks);
            if dt > 0 {
                let share = 100.0 * ticks.saturating_sub(prev) as f64 / dt as f64;
                // Умножаем на число ядер: /proc/stat суммирует их все, а
                // привычные «100% = одно ядро под нагрузкой» считаются от него.
                out.push(Proc {
                    name,
                    value: share * num_cpus() as f64,
                });
            }
        }
        self.prev = cur;
        self.prev_total = total;
        if first {
            return Vec::new();
        }
        out.sort_by(|a, b| b.value.total_cmp(&a.value));
        out.truncate(n);
        out
    }

    pub fn top_mem(&self, n: usize) -> Vec<Proc> {
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as f64;
        let mut out: Vec<Proc> = proc_dirs()
            .filter_map(|pid| {
                let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
                let rss: f64 = statm.split_whitespace().nth(1)?.parse().ok()?;
                Some(Proc {
                    name: proc_name(pid)?,
                    value: rss * page / 1024.0 / 1024.0 / 1024.0,
                })
            })
            .collect();
        out.sort_by(|a, b| b.value.total_cmp(&a.value));
        out.truncate(n);
        out
    }
}

fn proc_dirs() -> impl Iterator<Item = i32> {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse::<i32>().ok())
}

fn proc_name(pid: i32) -> Option<String> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let s = s.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// (pid, имя, utime+stime в тиках). Битые и исчезнувшие процессы пропускаем:
/// каталог /proc живёт своей жизнью, пока мы по нему идём.
fn proc_ticks() -> Vec<(i32, String, u64)> {
    proc_dirs()
        .filter_map(|pid| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            // Имя процесса в /proc/pid/stat заключено в скобки и может само
            // содержать пробелы, поэтому поля считаем после последней ')'.
            let rest = &stat[stat.rfind(')')? + 1..];
            let f: Vec<&str> = rest.split_whitespace().collect();
            let utime: u64 = f.get(11)?.parse().ok()?;
            let stime: u64 = f.get(12)?.parse().ok()?;
            Some((pid, proc_name(pid)?, utime + stime))
        })
        .collect()
}

fn total_cpu_ticks() -> u64 {
    let t = match std::fs::read_to_string("/proc/stat") {
        Ok(t) => t,
        Err(_) => return 0,
    };
    t.lines()
        .next()
        .map(|l| {
            l.split_whitespace()
                .skip(1)
                .take(8)
                .filter_map(|v| v.parse::<u64>().ok())
                .sum()
        })
        .unwrap_or(0)
}

fn num_cpus() -> usize {
    let n = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    if n > 0 {
        n as usize
    } else {
        1
    }
}

/// Абсолютная громкость в процентах.
pub fn set_volume_abs(percent: u32) {
    let _ = Command::new("wpctl")
        .args([
            "set-volume",
            "@DEFAULT_AUDIO_SINK@",
            &format!("{}%", percent.min(150)),
        ])
        .status();
}

/// Разложенное время: нужно календарю в выдвижном виджете.
pub struct Now {
    pub year: i32,
    pub month: i32,
    pub day: i32,
}

pub fn now() -> Now {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        Now {
            year: tm.tm_year + 1900,
            month: tm.tm_mon + 1,
            day: tm.tm_mday,
        }
    }
}

// ---------------------------------------------------------------------------
// Температуры
// ---------------------------------------------------------------------------

pub const ICON_TEMP: &str = "\u{f050f}";
pub const ICON_GPU: &str = "\u{f0379}";

/// Пути к датчикам ищем один раз на старте: нумерация hwmon зависит от порядка
/// загрузки модулей ядра и между загрузками гуляет, но внутри одной сессии
/// стоит на месте. Перебирать /sys на каждом тике ради этого незачем.
pub struct TempSensors {
    cpu: Option<String>,
    gpu: Option<String>,
}

pub struct Temps {
    pub cpu: Option<i32>,
    pub gpu: Option<i32>,
}

impl TempSensors {
    pub fn discover() -> Self {
        let mut cpu = None;
        let mut gpu = None;
        let dirs = match std::fs::read_dir("/sys/class/hwmon") {
            Ok(d) => d,
            Err(_) => return Self { cpu, gpu },
        };
        for e in dirs.flatten() {
            let dir = e.path();
            let name = std::fs::read_to_string(dir.join("name")).unwrap_or_default();
            let path = |n: &str| dir.join(n).to_string_lossy().into_owned();
            match name.trim() {
                // coretemp/k10temp: temp1 -- это «Package id 0», температура
                // всего кристалла. Отдельные ядра (temp2+) в баре не нужны.
                "coretemp" | "k10temp" | "zenpower" => cpu = Some(path("temp1_input")),
                // У amdgpu temp1 -- edge, температура кристалла; junction и
                // mem есть не на всех картах, поэтому берём то, что есть везде.
                "amdgpu" | "nouveau" | "radeon" => gpu = Some(path("temp1_input")),
                _ => {}
            }
        }
        Self { cpu, gpu }
    }

    pub fn read(&self) -> Temps {
        Temps {
            cpu: self.cpu.as_deref().and_then(read_millidegrees),
            gpu: self.gpu.as_deref().and_then(read_millidegrees),
        }
    }
}

fn read_millidegrees(path: &str) -> Option<i32> {
    let v: i32 = std::fs::read_to_string(path).ok()?.trim().parse().ok()?;
    Some(v / 1000)
}

impl Temps {
    /// Показываем то, что нашлось: на машине без дискретной карты половина
    /// строки просто не появится, а не станет прочерком.
    pub fn label(&self) -> String {
        match (self.cpu, self.gpu) {
            (Some(c), Some(g)) => format!("{ICON_TEMP} {c}° {ICON_GPU} {g}°"),
            (Some(c), None) => format!("{ICON_TEMP} {c}°"),
            (None, Some(g)) => format!("{ICON_GPU} {g}°"),
            (None, None) => String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Сеть
// ---------------------------------------------------------------------------

pub const ICON_WIFI: &str = "\u{f05a9}";
pub const ICON_WIFI_OFF: &str = "\u{f05aa}";
pub const ICON_ETH: &str = "\u{f0200}";
pub const ICON_DOWN: &str = "\u{f01da}";
pub const ICON_UP: &str = "\u{f0552}";

pub struct Net {
    pub wireless: bool,
    pub up: bool,
    pub rx_bps: f64,
    pub tx_bps: f64,
}

/// Скорость -- это дельта счётчиков, поэтому меряем от замера к замеру и
/// делим на реально прошедшее время, а не на период таймера: тик бара может
/// задержаться на перерисовке или на открытии виджета.
pub struct NetMeter {
    prev: Option<(u64, u64, Instant)>,
    iface: Option<String>,
}

impl NetMeter {
    pub fn new() -> Self {
        Self {
            prev: None,
            iface: None,
        }
    }

    pub fn sample(&mut self) -> Option<Net> {
        // Интерфейс переискиваем каждый раз, когда текущий лёг: кабель могли
        // выдернуть и уйти на wifi, и наоборот.
        let iface = match self.iface.clone().filter(|i| operstate(i) == "up") {
            Some(i) => i,
            None => {
                let found = pick_iface()?;
                // Смена интерфейса обнуляет отсчёт: счётчики у нового свои, и
                // разница со старыми дала бы выброс в гигабайты в секунду.
                self.prev = None;
                self.iface = Some(found.clone());
                found
            }
        };

        let rx = read_counter(&iface, "rx_bytes")?;
        let tx = read_counter(&iface, "tx_bytes")?;
        let now = Instant::now();

        let (rx_bps, tx_bps) = match self.prev {
            Some((prx, ptx, t)) => {
                let dt = now.duration_since(t).as_secs_f64();
                if dt > 0.05 {
                    (
                        rx.saturating_sub(prx) as f64 / dt,
                        tx.saturating_sub(ptx) as f64 / dt,
                    )
                } else {
                    (0.0, 0.0)
                }
            }
            None => (0.0, 0.0),
        };
        self.prev = Some((rx, tx, now));

        Some(Net {
            wireless: is_wireless(&iface),
            up: operstate(&iface) == "up",
            rx_bps,
            tx_bps,
        })
    }
}

/// Берём только физические интерфейсы: у lo, tun, tailscale и zerotier нет
/// каталога device. Иначе «сеть» в баре показывала бы трафик VPN поверх той
/// же самой карты и считала бы его дважды.
fn pick_iface() -> Option<String> {
    let mut candidates: Vec<String> = std::fs::read_dir("/sys/class/net")
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            e.path().join("device").exists().then_some(name)
        })
        .collect();
    // Проводной интерфейс предпочтительнее: если поднят и он, и wifi, трафик
    // почти наверняка идёт по кабелю.
    candidates.sort_by_key(|i| is_wireless(i));
    candidates.into_iter().find(|i| operstate(i) == "up")
}

fn operstate(iface: &str) -> String {
    std::fs::read_to_string(format!("/sys/class/net/{iface}/operstate"))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn is_wireless(iface: &str) -> bool {
    std::path::Path::new(&format!("/sys/class/net/{iface}/wireless")).exists()
}

fn read_counter(iface: &str, what: &str) -> Option<u64> {
    std::fs::read_to_string(format!("/sys/class/net/{iface}/statistics/{what}"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Компактно: до 1000 единиц -- целое, дальше на порядок вверх. В баре важнее
/// стабильная ширина, чем точность до байта.
fn human_bps(bps: f64) -> String {
    let kb = bps / 1024.0;
    if kb < 1000.0 {
        format!("{:.0}K", kb)
    } else {
        format!("{:.1}M", kb / 1024.0)
    }
}

impl Net {
    pub fn label(&self) -> String {
        if !self.up {
            return format!("{ICON_WIFI_OFF} нет сети");
        }
        let icon = if self.wireless { ICON_WIFI } else { ICON_ETH };
        format!(
            "{icon} {ICON_DOWN}{} {ICON_UP}{}",
            human_bps(self.rx_bps),
            human_bps(self.tx_bps)
        )
    }
}

// ---------------------------------------------------------------------------
// Музыка
// ---------------------------------------------------------------------------

pub const ICON_PLAY: &str = "\u{f040a}";
pub const ICON_PAUSE: &str = "\u{f03e4}";

#[derive(Clone, Default)]
pub struct Music {
    pub playing: bool,
    /// Пусто -- проигрывателя нет; модуль в этом случае не занимает места.
    pub text: String,
}

impl Music {
    pub fn label(&self, max_chars: usize) -> String {
        if self.text.is_empty() {
            return String::new();
        }
        let icon = if self.playing { ICON_PLAY } else { ICON_PAUSE };
        format!("{icon} {}", ellipsize(&self.text, max_chars))
    }
}

/// Название трека шире всего остального в баре и дёргается при каждой смене
/// позиции. Режем по символам, а не по байтам: в кириллице и в длинных тире
/// байт на символ больше одного, и обрезка по ним рвала бы строку посередине
/// символа.
fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// playerctl -- внешний процесс, поэтому кэш на секунду: трек меняется
/// минутами, а бар перерисовывается на каждое движение окна.
pub struct MusicCache {
    value: Music,
    fetched: Option<Instant>,
    ttl: Duration,
}

impl MusicCache {
    pub fn new() -> Self {
        Self {
            value: Music::default(),
            fetched: None,
            ttl: Duration::from_millis(1000),
        }
    }

    pub fn get(&mut self) -> Music {
        let stale = match self.fetched {
            Some(t) => t.elapsed() > self.ttl,
            None => true,
        };
        if stale {
            self.value = read_music();
            self.fetched = Some(Instant::now());
        }
        self.value.clone()
    }

    pub fn invalidate(&mut self) {
        self.fetched = None;
    }
}

fn read_music() -> Music {
    // Один вызов вместо двух: playerctl умеет форматировать статус и метаданные
    // разом, а каждый запуск процесса стоит дороже самого разбора.
    let out = match Command::new("playerctl")
        .args(["metadata", "--format", "{{status}}\t{{artist}} - {{title}}"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        // Ни одного проигрывателя -- playerctl выходит с ошибкой. Это не сбой,
        // а нормальное состояние: модуль просто исчезает из бара.
        _ => return Music::default(),
    };
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.trim();
    let (status, text) = match line.split_once('\t') {
        Some((a, b)) => (a, b.trim()),
        None => return Music::default(),
    };
    // Без метаданных playerctl отдаёт " - "; показывать такое незачем.
    let text = text.trim_matches('-').trim();
    if text.is_empty() {
        return Music::default();
    }
    Music {
        playing: status == "Playing",
        text: text.to_string(),
    }
}

pub fn music_play_pause() {
    let _ = Command::new("playerctl").arg("play-pause").status();
}

pub fn music_next() {
    let _ = Command::new("playerctl").arg("next").status();
}

pub fn music_prev() {
    let _ = Command::new("playerctl").arg("previous").status();
}
