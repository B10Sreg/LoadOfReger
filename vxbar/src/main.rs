mod config;
mod modules;
mod render;
mod x;

use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use x11::xlib;

use config::Config;
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

    let mut cpu = modules::CpuMeter::default();
    let mut vol = modules::VolumeCache::new();
    cpu.sample(); // первый замер -- база для дельты, значение отбрасываем

    let xfd = unsafe { xlib::XConnectionNumber(xh.dpy) };
    // Окно, на свойства которого мы сейчас подписаны ради заголовка.
    let mut watched: xlib::Window = 0;
    let mut keeper = GeometryKeeper::default();
    let mut last_tick = Instant::now() - Duration::from_secs(60);
    let mut dirty = true;

    loop {
        if quit.load(Ordering::Relaxed) {
            break;
        }

        if reload.swap(false, Ordering::Relaxed) {
            cfg = Config::load(&path);
            xh.reconfigure(&cfg);
            rend.resize(&xh, &cfg);
            if hidden {
                // reconfigure переставил струты -- у спрятанного бара их быть не должно
                xh.hide();
            }
            dirty = !hidden;
        }

        if toggle.swap(false, Ordering::Relaxed) {
            hidden = !hidden;
            if hidden {
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

        if dirty && !hidden {
            let st = collect(&xh, &cfg, &mut cpu, &mut vol, &mut watched);
            rend.draw(&cfg, &st, xh.geom.w as f64, xh.geom.h as f64);
            unsafe { xlib::XFlush(xh.dpy) };
            dirty = false;
        }

        // Ждём либо X-событие, либо истечения интервала. Без этого пришлось бы
        // крутить busy-loop или спать фиксированно, теряя отзывчивость кликов.
        let wait = interval
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_millis(0));
        let pending = unsafe { xlib::XPending(xh.dpy) };
        if pending == 0 && !wait.is_zero() {
            wait_fd(xfd, wait);
        }

        while unsafe { xlib::XPending(xh.dpy) } > 0 {
            let mut ev: xlib::XEvent = unsafe { std::mem::zeroed() };
            unsafe { xlib::XNextEvent(xh.dpy, &mut ev) };
            if handle_event(&ev, &xh, &cfg, &rend, &mut vol, &mut keeper) {
                dirty = true;
            }
        }
    }

    unsafe {
        xlib::XDestroyWindow(xh.dpy, xh.win);
        xlib::XCloseDisplay(xh.dpy);
    }
}

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

/// Возвращает true, если бар надо перерисовать.
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

fn handle_event(
    ev: &xlib::XEvent,
    xh: &x::X,
    cfg: &Config,
    rend: &Renderer,
    vol: &mut modules::VolumeCache,
    keeper: &mut GeometryKeeper,
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
                keeper.keep(xh);
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
                let hit = rend.hit_test(b.x as f64);
                match (hit, b.button) {
                    (Some(Hit::Tag(i)), 1) => {
                        xh.request_desktop(i);
                        true
                    }
                    (Some(Hit::Volume), 1) => {
                        modules::toggle_mute();
                        vol.invalidate();
                        true
                    }
                    (Some(Hit::Volume), 4) => {
                        modules::set_volume_step(5);
                        vol.invalidate();
                        true
                    }
                    (Some(Hit::Volume), 5) => {
                        modules::set_volume_step(-5);
                        vol.invalidate();
                        true
                    }
                    (Some(Hit::Clock), 1) => {
                        let cmd = cfg.clock.on_click.trim();
                        if !cmd.is_empty() {
                            let _ = std::process::Command::new("sh")
                                .arg("-c")
                                .arg(cmd)
                                .spawn();
                        }
                        false
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

fn collect(
    xh: &x::X,
    cfg: &Config,
    cpu: &mut modules::CpuMeter,
    vol: &mut modules::VolumeCache,
    watched: &mut xlib::Window,
) -> BarState {
    let a = &xh.atoms;

    let ndesk = xh
        .cardinal(xh.root, a.number_of_desktops)
        .unwrap_or(9)
        .clamp(1, 31) as usize;
    // WM может отдать индекс за пределами числа столов (например, сразу после
    // смены их количества) -- без зажима активного тега просто не было бы.
    let current = (xh.cardinal(xh.root, a.current_desktop).unwrap_or(0).max(0) as usize)
        .min(ndesk - 1);

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
            label: names
                .get(i)
                .cloned()
                .unwrap_or_else(|| (i + 1).to_string()),
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
    xh.watch_title(*watched, active_win);
    *watched = active_win;
    let title = active.and_then(|w| xh.window_title(w)).unwrap_or_default();

    let m = modules::mem();
    BarState {
        tags,
        title,
        layout: String::new(),
        cpu: cpu.sample(),
        mem_used: m.used_gb,
        mem_total: m.total_gb,
        volume: vol.get(),
        clock: modules::clock(&cfg.clock.format),
    }
}
