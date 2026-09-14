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
            let st = collect(&xh, &cfg, &mut cpu, &mut vol);
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
            if handle_event(&ev, &xh, &cfg, &rend, &mut vol) {
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
fn handle_event(
    ev: &xlib::XEvent,
    xh: &x::X,
    cfg: &Config,
    rend: &Renderer,
    vol: &mut modules::VolumeCache,
) -> bool {
    unsafe {
        match ev.get_type() {
            xlib::Expose => true,
            xlib::ConfigureNotify => {
                let c = ev.configure;
                c.window == xh.win
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
                        let _ = std::process::Command::new("sh")
                            .arg("-c")
                            .arg(&cfg.clock.on_click)
                            .spawn();
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
) -> BarState {
    let a = &xh.atoms;

    let ndesk = xh
        .cardinal(xh.root, a.number_of_desktops)
        .unwrap_or(9)
        .clamp(1, 31) as usize;
    let current = xh.cardinal(xh.root, a.current_desktop).unwrap_or(0).max(0) as usize;

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
