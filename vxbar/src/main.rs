mod config;
mod modules;
mod popup;
mod render;
mod x;

use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use x11::xlib;

use config::Config;
use popup::{Popup, PopupData};
use render::{BarState, Hit, Renderer, TagInfo};

fn main() {
    let path = Config::path();
    if let Err(e) = Config::write_default_if_missing(&path) {
        eprintln!("vxbar: не создаётся {}: {e}", path.display());
    }
    let mut cfg = Config::load(&path);

    // Ставим до первого запроса к X: дальше любая гонка с исчезающим окном
    // прилетит в наш обработчик, а не в дефолтный, который убивает процесс.
    unsafe { x::install_error_handler() };

    // Дети (wpctl, команда по клику на часы) нам не нужны: просим ядро
    // хоронить их само, иначе за сессию накапливаются зомби.
    unsafe { libc::signal(libc::SIGCHLD, libc::SIG_IGN) };

    let mut xh = match x::X::open(&cfg) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("vxbar: {e}");
            std::process::exit(1);
        }
    };
    let mut rend = match Renderer::new(&xh, &cfg) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("vxbar: {e}");
            std::process::exit(1);
        }
    };

    // SIGUSR1 -- перечитать конфиг. Обработчик только взводит флаг: всё
    // остальное делается в основном цикле, где можно спокойно трогать X.
    let reload = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGUSR1, Arc::clone(&reload))
        .expect("не ставится обработчик SIGUSR1");
    // SIGUSR2 -- показать/скрыть бар. Как и с USR1, обработчик только взводит
    // флаг: разматывать это внутри сигнала нельзя, там нельзя трогать Xlib.
    let toggle = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGUSR2, Arc::clone(&toggle))
        .expect("не ставится обработчик SIGUSR2");
    let mut hidden = false;

    let quit = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(sig, Arc::clone(&quit)).ok();
    }

    let pop = match Popup::new(&xh, &cfg) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("vxbar: {e}");
            std::process::exit(1);
        }
    };

    let mut rt = Runtime {
        cpu: modules::CpuMeter::default(),
        mic: modules::MicCache::new(),
        disk: DiskCache::default(),
        temps: modules::TempSensors::discover(),
        net: modules::NetMeter::new(),
        music: modules::MusicCache::new(),
        vol: modules::VolumeCache::new(),
        procs: modules::ProcSampler::default(),
        pop,
        keeper: GeometryKeeper::default(),
        watched: 0,
    };
    rt.cpu.sample(); // первый замер -- база для дельты, значение отбрасываем

    let xfd = unsafe { xlib::XConnectionNumber(xh.dpy) };
    let mut last_tick = Instant::now() - Duration::from_secs(60);
    let mut dirty = true;
    // Последнее собранное состояние: кадры анимации рисуются по нему.
    // Пересобирать его на каждый кадр нельзя -- collect() обходит все окна
    // отдельными запросами к X, а раз в полсекунды ещё и форкает wpctl. На
    // переезде плашки это стоило пары кадров ровно там, где она быстрее всего.
    let mut state: Option<BarState> = None;

    loop {
        if quit.load(Ordering::Relaxed) {
            break;
        }

        if reload.swap(false, Ordering::Relaxed) {
            cfg = Config::load(&path);
            // Геометрия и шрифт поменялись -- открытый виджет всё равно висел
            // бы не на месте и старым кеглем.
            rt.pop.close();
            xh.reconfigure(&cfg);
            rend.resize(&xh, &cfg);
            rt.pop.reload_font(&cfg);
            if hidden {
                // reconfigure переставил струты -- у спрятанного бара их быть не должно
                xh.hide();
            }
            dirty = !hidden;
        }

        if toggle.swap(false, Ordering::Relaxed) {
            hidden = !hidden;
            if hidden {
                rt.pop.close();
                xh.hide();
            } else {
                xh.show(&cfg);
            }
            dirty = !hidden;
        }

        let interval = Duration::from_millis(cfg.bar.interval_ms.max(100));
        if last_tick.elapsed() >= interval {
            last_tick = Instant::now();
            dirty = true;
        }

        // Кадр нужен либо когда данные протухли, либо пока едет плашка тега.
        let frame = !hidden && (dirty || rend.animating());
        if frame {
            if dirty || state.is_none() {
                state = Some(collect(&xh, &cfg, &mut rt));
            }
            let st = state.as_ref().expect("состояние собрано выше");
            rend.draw(&cfg, st, xh.geom.w as f64, xh.geom.h as f64);
            unsafe { xlib::XFlush(xh.dpy) };
            // Виджету свежие данные нужны только на тике: тегов он не
            // показывает, и перерисовывать его между кадрами анимации незачем.
            if dirty {
                if let Some(kind) = rt.pop.kind {
                    let data = popup_data(&cfg, kind, st, &mut rt.procs);
                    rt.pop.refresh(&cfg, &data);
                }
            }
            dirty = false;
        }

        // Ждём либо X-событие, либо истечения интервала. Без этого пришлось бы
        // крутить busy-loop или спать фиксированно, теряя отзывчивость кликов.
        let mut wait = interval
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_millis(0));
        // Пока плашка тега едет, кадры нужны чаще опроса модулей: интервал там
        // секунда, и анимация из одного кадра не состоит.
        if rend.animating() && !hidden {
            wait = wait.min(FRAME);
        }
        let pending = unsafe { xlib::XPending(xh.dpy) };
        if pending == 0 && !wait.is_zero() {
            wait_fd(xfd, wait);
        }

        while unsafe { xlib::XPending(xh.dpy) } > 0 {
            let mut ev: xlib::XEvent = unsafe { std::mem::zeroed() };
            unsafe { xlib::XNextEvent(xh.dpy, &mut ev) };
            if handle_event(&ev, &xh, &cfg, &rend, &mut rt) {
                dirty = true;
            }
        }
    }

    rt.pop.close();
    unsafe {
        xlib::XDestroyWindow(xh.dpy, rt.pop.win);
        xlib::XDestroyWindow(xh.dpy, xh.win);
        xlib::XCloseDisplay(xh.dpy);
    }
}

/// Шаг анимации: ~60 кадров в секунду. Дольше держать плашку между тегами
/// незачем, а чаще -- бессмысленно даже на 144 Гц: сама анимация занимает
/// десятые доли секунды.
const FRAME: Duration = Duration::from_millis(16);

/// poll(2) на дескрипторе X-соединения с таймаутом.
fn wait_fd(fd: c_int, dur: Duration) {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ms = dur.as_millis().min(i32::MAX as u128) as c_int;
    unsafe { libc::poll(&mut pfd, 1, ms) };
}

/// Живое состояние бара между событиями: сэмплеры модулей, открытый виджет и
/// сторож геометрии. Собрано в одну структуру, потому что обработчик событий
/// трогает почти всё это разом, а десяток отдельных ссылок в сигнатуре читать
/// невозможно.
struct Runtime {
    cpu: modules::CpuMeter,
    vol: modules::VolumeCache,
    mic: modules::MicCache,
    disk: DiskCache,
    /// Датчики ищутся один раз: нумерация hwmon в пределах сессии не меняется.
    temps: modules::TempSensors,
    net: modules::NetMeter,
    music: modules::MusicCache,
    procs: modules::ProcSampler,
    pop: Popup,
    keeper: GeometryKeeper,
    /// Окно, на свойства которого мы сейчас подписаны ради заголовка.
    watched: xlib::Window,
}

/// Свободное место меряется реже остального: statvfs на сетевой или уснувший
/// диск умеет задуматься на десятки миллисекунд, а цифра там меняется не
/// быстрее, чем что-то успевает записаться.
#[derive(Default)]
struct DiskCache {
    value: Option<modules::Disk>,
    fetched: Option<Instant>,
}

impl DiskCache {
    fn get(&mut self, path: &str) -> Option<modules::Disk> {
        let stale = self.fetched.is_none_or(|t| t.elapsed() > Duration::from_secs(5));
        if stale {
            self.value = modules::disk(path);
            self.fetched = Some(Instant::now());
        }
        self.value.as_ref().map(|d| modules::Disk {
            free_gb: d.free_gb,
            total_gb: d.total_gb,
        })
    }
}

/// Сколько раз подряд бар возвращает себя на место, прежде чем сдаться.
/// Одиночный перетаск мышью -- это одна-две поправки; сотни подряд означают,
/// что бар воюет с WM, который тайлит его как обычное окно. В такой войне
/// выиграть нельзя, а мигать окнами она будет бесконечно.
const MAX_FIXES: u32 = 8;
const FIX_WINDOW: Duration = Duration::from_secs(2);
const BACKOFF: Duration = Duration::from_secs(30);

struct GeometryKeeper {
    fixes: u32,
    since: Instant,
    quiet_until: Option<Instant>,
    warned: bool,
}

impl Default for GeometryKeeper {
    fn default() -> Self {
        Self {
            fixes: 0,
            since: Instant::now(),
            quiet_until: None,
            warned: false,
        }
    }
}

impl GeometryKeeper {
    fn keep(&mut self, xh: &x::X) {
        if let Some(t) = self.quiet_until {
            if Instant::now() < t {
                return;
            }
            self.quiet_until = None;
            self.fixes = 0;
            self.since = Instant::now();
        }
        if !xh.enforce_geometry() {
            return;
        }
        if self.since.elapsed() > FIX_WINDOW {
            self.fixes = 0;
            self.since = Instant::now();
        }
        self.fixes += 1;
        if self.fixes <= MAX_FIXES {
            return;
        }
        self.quiet_until = Some(Instant::now() + BACKOFF);
        if !self.warned {
            self.warned = true;
            eprintln!(
                "vxbar: окно бара двигает WM -- похоже, он не знает про \
                 _NET_WM_WINDOW_TYPE_DOCK и тайлит бар как обычное окно; \
                 перестаю возвращать его на место"
            );
        }
    }
}

/// Возвращает true, если бар надо перерисовать.
fn handle_event(
    ev: &xlib::XEvent,
    xh: &x::X,
    cfg: &Config,
    rend: &Renderer,
    rt: &mut Runtime,
) -> bool {
    unsafe {
        match ev.get_type() {
            xlib::Expose => true,
            xlib::ConfigureNotify => {
                let c = ev.configure;
                if c.window != xh.win {
                    return false;
                }
                // Бар -- док, его никто не должен двигать. WM без поддержки
                // доков тащит его как обычное окно: возвращаем на место и
                // перерисовываем, потому что размер мог измениться.
                rt.keeper.keep(xh);
                true
            }
            xlib::PropertyNotify => {
                let p = ev.property;
                // Нас интересуют только смены состояния WM на root-окне:
                // активный тег, список клиентов, фокус, имена столов.
                p.window == xh.root
                    && (p.atom == xh.atoms.current_desktop
                        || p.atom == xh.atoms.client_list
                        || p.atom == xh.atoms.active_window
                        || p.atom == xh.atoms.desktop_names
                        || p.atom == xh.atoms.number_of_desktops)
                    || (p.window != xh.root && p.atom == xh.atoms.wm_name)
            }
            xlib::ButtonPress => {
                let b = ev.button;
                // Пока виджет открыт, указатель захвачен на root: клик мимо
                // бара и мимо самого виджета его закрывает.
                if rt.pop.visible() && b.window != xh.win {
                    // Указатель захвачен на root, поэтому клик по самому
                    // виджету может прийти и с его окном, и с root-координатами.
                    let (lx, ly) = if popup::is_popup_window(&rt.pop, b.window) {
                        (b.x as f64, b.y as f64)
                    } else {
                        popup::local_coords(&b, xh, &rt.pop)
                    };
                    let (pw, ph) = rt.pop.size();
                    if lx >= 0.0 && ly >= 0.0 && lx < pw && ly < ph {
                        match rt.pop.click(lx, ly) {
                            popup::Click::Changed => {
                                rt.vol.invalidate();
                                return true;
                            }
                            // Кнопку питания закрываем до запуска: команда
                            // уводит сессию в сон или в перезагрузку, и
                            // висящий поверх виджет остался бы последним, что
                            // человек увидит на экране.
                            popup::Click::Run(cmd) => {
                                rt.pop.close();
                                modules::spawn_shell(&cmd);
                                return true;
                            }
                            popup::Click::Ignored => return false,
                        }
                    }
                    rt.pop.close();
                    return true;
                }
                if b.window != xh.win {
                    return false;
                }
                // Вдоль оси бара: у вертикального модули разложены по Y.
                let along = if cfg.bar.position.vertical() {
                    b.y as f64
                } else {
                    b.x as f64
                };
                let hit = rend.hit_test(along);
                match (hit, b.button) {
                    (Some(Hit::Tag(i)), 1) => {
                        xh.request_desktop(i);
                        true
                    }
                    // Средняя кнопка и колесо на громкости остаются быстрыми
                    // действиями: открывать ради них окно было бы лишним шагом.
                    (Some(Hit::Volume), 2) | (Some(Hit::Volume), 3) => {
                        modules::toggle_mute();
                        rt.vol.invalidate();
                        true
                    }
                    (Some(Hit::Volume), 4) => {
                        modules::set_volume_step(5);
                        rt.vol.invalidate();
                        true
                    }
                    (Some(Hit::Volume), 5) => {
                        modules::set_volume_step(-5);
                        rt.vol.invalidate();
                        true
                    }
                    // Музыка живёт теми же жестами, что и громкость: клик --
                    // пауза, колесо -- соседний трек. Виджета ей не нужно,
                    // всё содержимое и так в баре.
                    (Some(Hit::Music), 1) | (Some(Hit::Music), 2) => {
                        modules::music_play_pause();
                        rt.music.invalidate();
                        true
                    }
                    (Some(Hit::Music), 4) => {
                        modules::music_next();
                        rt.music.invalidate();
                        true
                    }
                    (Some(Hit::Music), 5) => {
                        modules::music_prev();
                        rt.music.invalidate();
                        true
                    }
                    // Микрофон живёт теми же жестами, что и громкость.
                    (Some(Hit::Mic), 1) | (Some(Hit::Mic), 2) | (Some(Hit::Mic), 3) => {
                        modules::toggle_mic_mute();
                        rt.mic.invalidate();
                        true
                    }
                    (Some(Hit::Mic), 4) => {
                        modules::set_mic_step(5);
                        rt.mic.invalidate();
                        true
                    }
                    (Some(Hit::Mic), 5) => {
                        modules::set_mic_step(-5);
                        rt.mic.invalidate();
                        true
                    }
                    (Some(Hit::Clock), 1) => {
                        // Заданная вручную команда важнее встроенного
                        // календаря: её просили явно.
                        let cmd = cfg.clock.on_click.trim();
                        if !cmd.is_empty() {
                            let _ = std::process::Command::new("sh").arg("-c").arg(cmd).spawn();
                            return false;
                        }
                        toggle_popup(xh, cfg, rend, rt, Hit::Clock)
                    }
                    (Some(h), 1) if popup::Kind::from_hit(h).is_some() => {
                        toggle_popup(xh, cfg, rend, rt, h)
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

/// Клик по модулю открывает его виджет, повторный клик по тому же -- закрывает.
fn toggle_popup(xh: &x::X, cfg: &Config, rend: &Renderer, rt: &mut Runtime, hit: Hit) -> bool {
    if !cfg.popups.enabled {
        return false;
    }
    let kind = match popup::Kind::from_hit(hit) {
        Some(k) => k,
        None => return false,
    };
    if rt.pop.kind == Some(kind) {
        rt.pop.close();
        return true;
    }
    let region = match rend.region_of(hit) {
        Some(r) => r,
        None => return false,
    };
    let st = collect(xh, cfg, rt);
    let data = popup_data(cfg, kind, &st, &mut rt.procs);
    rt.pop.open(xh, cfg, kind, region, &data);
    false
}

fn popup_data(
    cfg: &Config,
    kind: popup::Kind,
    st: &BarState,
    procs: &mut modules::ProcSampler,
) -> PopupData {
    PopupData {
        volume: st.volume,
        cpu: st.cpu,
        mem_used: st.mem_used,
        mem_total: st.mem_total,
        procs: popup::procs_for(kind, cfg, procs),
        mounts: popup::mounts_for(kind),
    }
}

fn collect(xh: &x::X, cfg: &Config, rt: &mut Runtime) -> BarState {
    let a = &xh.atoms;

    let ndesk = xh
        .cardinal(xh.root, a.number_of_desktops)
        .unwrap_or(9)
        .clamp(1, 31) as usize;
    // WM может отдать индекс за пределами числа столов (например, сразу после
    // смены их количества) -- без зажима активного тега просто не было бы.
    let current =
        (xh.cardinal(xh.root, a.current_desktop).unwrap_or(0).max(0) as usize).min(ndesk - 1);

    // Занятые теги: у каждого клиента из _NET_CLIENT_LIST читаем _NET_WM_DESKTOP.
    let mut occupied = vec![false; ndesk];
    if let Some(list) = xh.cardinals(xh.root, a.client_list, 1024) {
        for w in list {
            if let Some(d) = xh.cardinal(w as xlib::Window, a.wm_desktop) {
                if d >= 0 && (d as usize) < ndesk {
                    occupied[d as usize] = true;
                }
            }
        }
    }

    let names = if !cfg.tags.labels.is_empty() {
        cfg.tags.labels.clone()
    } else {
        xh.desktop_names().unwrap_or_default()
    };

    let tags = (0..ndesk)
        .map(|i| TagInfo {
            label: names.get(i).cloned().unwrap_or_else(|| (i + 1).to_string()),
            active: i == current,
            occupied: occupied[i],
        })
        .collect();

    let active = xh
        .cardinal(xh.root, a.active_window)
        .filter(|v| *v > 0)
        .map(|v| v as xlib::Window);
    // Подписываемся на свойства активного окна: без этого заголовок в баре
    // менялся бы только на тике таймера, с задержкой до interval_ms.
    let active_win = active.unwrap_or(0);
    xh.watch_title(rt.watched, active_win);
    rt.watched = active_win;
    let title = active.and_then(|w| xh.window_title(w)).unwrap_or_default();

    let m = modules::mem();
    BarState {
        tags,
        title,
        layout: String::new(),
        cpu: rt.cpu.sample(),
        mem_used: m.used_gb,
        mem_total: m.total_gb,
        volume: rt.vol.get(),
        clock: modules::clock(&cfg.clock.format),
        temps: rt.temps.read(),
        net: rt.net.sample(),
        music: rt.music.get(),
        disk: rt.disk.get(&cfg.disk.path),
        battery: modules::battery(),
        mic: rt.mic.get(),
    }
}
