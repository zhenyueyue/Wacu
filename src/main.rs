#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod placement;
mod platform_windows;
mod state;

use std::{cell::RefCell, error::Error, rc::Rc, time::Duration};

use placement::{Point, constrain, pinned, popup};
use slint::{
    ComponentHandle, Model, ModelRc, PhysicalPosition, SharedString, Timer, TimerMode, VecModel,
};
use state::{Mode, Preferences};

slint::include_modules!();

const NAMES: [&str; 3] = ["Codex", "Claude Code", "WorkBuddy"];
const SYMBOLS: [&str; 3] = ["◎", "✳", "w"];
const SUBTITLES: [&str; 3] = ["OpenAI · ChatGPT", "Anthropic · Claude", "Credits · 积分"];
const VALUES: [i32; 3] = [76, 43, 92];

fn make_tray_icon() -> slint::Image {
    // A small antialiased icon is rasterized just once; no WebView or OS image file is needed.
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(64, 64);
    let pixels = buffer.make_mut_bytes();
    for y in 0..64 {
        for x in 0..64 {
            let dx = x as f64 + 0.5 - 32.0;
            let dy = y as f64 + 0.5 - 32.0;
            let distance = dx.hypot(dy);
            let angle = dy.atan2(dx).to_degrees();
            let ring = (distance - 25.0).abs() < 3.1;
            let active = (angle + 90.0 + 360.0) % 360.0 < 275.0;
            let face = (dx / 13.5).powi(2) + (dy / 11.0).powi(2) < 1.0;
            let eye = dy > -5.0 && dy < 6.0 && ((dx > -7.0 && dx < -4.0) || (dx > 4.0 && dx < 7.0));
            let (r, g, b, a) = if distance < 30.0 {
                if eye {
                    (43, 50, 47, 255)
                } else if face {
                    (244, 236, 221, 255)
                } else if ring && active {
                    (142, 220, 183, 255)
                } else if ring {
                    (71, 84, 79, 255)
                } else {
                    (26, 29, 33, 255)
                }
            } else {
                (0, 0, 0, 0)
            };
            let i = (y * 64 + x) * 4;
            pixels[i] = r;
            pixels[i + 1] = g;
            pixels[i + 2] = b;
            pixels[i + 3] = a;
        }
    }
    slint::Image::from_rgba8(buffer)
}

fn arc_svg(percent: i32) -> SharedString {
    if percent <= 0 {
        return "M 0 0 L 0 0 M 56 56 L 56 56".into();
    }
    let f = (percent.min(100) as f64 / 100.0).min(0.99998);
    let angle = (-90.0 + f * 360.0).to_radians();
    let end_x = 28.0 + 24.0 * angle.cos();
    let end_y = 28.0 + 24.0 * angle.sin();
    let large = if percent > 50 { 1 } else { 0 };
    format!("M 0 0 L 0 0 M 56 56 L 56 56 M 28 4 A 24 24 0 {large} 1 {end_x:.2} {end_y:.2}").into()
}

fn service(index: usize, visible: bool) -> Service {
    let tint = match index {
        0 => slint::Color::from_rgb_u8(142, 216, 182),
        1 => slint::Color::from_rgb_u8(230, 159, 129),
        _ => slint::Color::from_rgb_u8(164, 154, 238),
    };
    let (remaining, reset) = match index {
        0 => ("5 小时额度", "预计重置：1 小时 42 分后"),
        1 => ("当前会话额度", "预计重置：3 小时 18 分后"),
        _ => ("积分余额 · 3,324 / 3,609", "积分余额以官方接口为准"),
    };
    Service {
        label: NAMES[index].into(),
        subtitle: SUBTITLES[index].into(),
        symbol: SYMBOLS[index].into(),
        percent: VALUES[index],
        remaining: remaining.into(),
        reset: reset.into(),
        accent: tint,
        arc: arc_svg(VALUES[index]),
        visible,
    }
}

fn refresh_model(
    model: &Rc<VecModel<Service>>,
    capsule: &CapsuleWindow,
    settings: &SettingsWindow,
    prefs: &Preferences,
) {
    for i in 0..3 {
        model.set_row_data(i, service(i, prefs.shown[i]));
    }
    let count = prefs.shown.iter().filter(|v| **v).count().max(1);
    capsule.set_visible_count(count as i32);
    settings.set_show_codex(prefs.shown[0]);
    settings.set_show_claude(prefs.shown[1]);
    settings.set_show_workbuddy(prefs.shown[2]);
    settings.set_pinned_mode(prefs.mode == Mode::Pinned);
    capsule.set_is_pinned(prefs.mode == Mode::Pinned);
}

fn capsule_dimensions(capsule: &CapsuleWindow) -> (i32, i32) {
    let size = capsule.window().size();
    (size.width as i32, size.height as i32)
}

fn place_capsule(capsule: &CapsuleWindow, prefs: &Preferences, anchor: Option<Point>) {
    let size = capsule_dimensions(capsule);
    let position = match prefs.mode {
        Mode::Popup => {
            let at = anchor.unwrap_or_else(platform_windows::cursor);
            popup(at, size, platform_windows::work_area_at(at))
        }
        Mode::Pinned => {
            if let Some((x, y)) = prefs.saved_position {
                let position = Point { x, y };
                let area = platform_windows::work_area_at(Point {
                    x: x + size.0 / 2,
                    y: y + size.1 / 2,
                });
                constrain(position, size, area)
            } else {
                pinned(
                    size,
                    platform_windows::work_area_at(platform_windows::cursor()),
                )
            }
        }
    };
    capsule
        .window()
        .set_position(PhysicalPosition::new(position.x, position.y));
}

fn show_capsule(capsule: &CapsuleWindow, prefs: &Preferences, anchor: Option<Point>) {
    capsule.set_revealed(false);
    let _ = capsule.show();
    place_capsule(capsule, prefs, anchor);
    // The native HWND may only become available after an event-loop iteration.
    let weak = capsule.as_weak();
    Timer::single_shot(Duration::from_millis(30), move || {
        if let Some(c) = weak.upgrade() {
            platform_windows::configure_capsule(c.window());
            platform_windows::activate(c.window());
            c.set_revealed(true);
        }
    });
}

fn toggle_capsule(capsule: &CapsuleWindow, prefs: &Preferences) {
    if capsule.window().is_visible() {
        capsule.set_revealed(false);
        let weak = capsule.as_weak();
        Timer::single_shot(Duration::from_millis(155), move || {
            if let Some(c) = weak.upgrade() {
                if !c.get_revealed() {
                    let _ = c.hide();
                }
            }
        });
    } else {
        show_capsule(capsule, prefs, Some(platform_windows::cursor()));
    }
}

fn set_mode(
    mode: Mode,
    prefs: &Rc<RefCell<Preferences>>,
    model: &Rc<VecModel<Service>>,
    capsule: &CapsuleWindow,
    settings: &SettingsWindow,
) {
    {
        let mut value = prefs.borrow_mut();
        value.mode = mode;
        if let Err(error) = state::save(&value) {
            eprintln!("Saving preferences: {error}");
        }
    }
    let current = prefs.borrow().clone();
    refresh_model(model, capsule, settings, &current);
    match mode {
        Mode::Popup => {
            capsule.set_revealed(false);
            let _ = capsule.hide();
        }
        Mode::Pinned => show_capsule(capsule, &current, None),
    }
}

fn show_detail(detail: &DetailWindow, capsule: &CapsuleWindow, index: usize, prefs: &Preferences) {
    if !prefs.shown[index] {
        return;
    }
    detail.set_selected_service(service(index, true));
    let _ = detail.show();
    let cp = capsule.window().position();
    let cs = capsule.window().size();
    let ds = detail.window().size();
    let area = platform_windows::work_area_at(Point {
        x: cp.x + cs.width as i32 / 2,
        y: cp.y,
    });
    let left = cp.x - ds.width as i32 - 13;
    let x = if left >= area.left {
        left
    } else {
        cp.x + cs.width as i32 + 13
    };
    let dest = constrain(
        Point { x, y: cp.y },
        (ds.width as i32, ds.height as i32),
        area,
    );
    detail
        .window()
        .set_position(PhysicalPosition::new(dest.x, dest.y));
    let weak = detail.as_weak();
    Timer::single_shot(Duration::from_millis(30), move || {
        if let Some(view) = weak.upgrade() {
            platform_windows::configure_capsule(view.window());
            platform_windows::activate(view.window());
        }
    });
}

fn main() -> Result<(), Box<dyn Error>> {
    let initial = state::load();
    let preferences = Rc::new(RefCell::new(initial.clone()));

    let capsule = CapsuleWindow::new()?;
    let settings = SettingsWindow::new()?;
    let detail = DetailWindow::new()?;
    let tray = WacuTray::new()?;
    tray.set_tray_icon(make_tray_icon());

    let model = Rc::new(VecModel::from(
        (0..3)
            .map(|i| service(i, initial.shown[i]))
            .collect::<Vec<_>>(),
    ));
    capsule.set_services(ModelRc::from(model.clone()));
    refresh_model(&model, &capsule, &settings, &initial);

    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        tray.on_tray_clicked(move || {
            #[cfg(debug_assertions)]
            if std::env::var_os("WACU_QA").is_some() {
                eprintln!("WACU_QA tray_clicked mode={:?}", pref.borrow().mode);
            }
            if let Some(c) = cap.upgrade() {
                toggle_capsule(&c, &pref.borrow());
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        tray.on_switch_mode({
            let settings = settings.as_weak();
            let model = model.clone();
            move || {
                if let (Some(c), Some(s)) = (cap.upgrade(), settings.upgrade()) {
                    let next = if pref.borrow().mode == Mode::Popup {
                        Mode::Pinned
                    } else {
                        Mode::Popup
                    };
                    set_mode(next, &pref, &model, &c, &s);
                }
            }
        });
    }
    {
        let weak = settings.as_weak();
        tray.on_show_settings(move || {
            if let Some(s) = weak.upgrade() {
                let _ = s.show();
            }
        });
    }
    tray.on_exit_app(|| {
        let _ = slint::quit_event_loop();
    });
    {
        let weak = settings.as_weak();
        capsule.on_open_settings(move || {
            if let Some(s) = weak.upgrade() {
                let _ = s.show();
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        let settings = settings.as_weak();
        let model = model.clone();
        capsule.on_toggle_mode(move || {
            if let (Some(c), Some(s)) = (cap.upgrade(), settings.upgrade()) {
                let next = if pref.borrow().mode == Mode::Popup {
                    Mode::Pinned
                } else {
                    Mode::Popup
                };
                set_mode(next, &pref, &model, &c, &s);
            }
        });
    }
    {
        let weak = capsule.as_weak();
        capsule.on_start_drag(move || {
            if let Some(c) = weak.upgrade() {
                platform_windows::begin_drag(c.window());
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let view = detail.as_weak();
        let pref = preferences.clone();
        capsule.on_open_service(move |idx| {
            if let (Some(c), Some(d)) = (cap.upgrade(), view.upgrade()) {
                if let Ok(index) = usize::try_from(idx) {
                    if index < 3 {
                        show_detail(&d, &c, index, &pref.borrow());
                    }
                }
            }
        });
    }
    {
        let weak = detail.as_weak();
        detail.on_close_detail(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    detail.on_refresh(|| {
        // The UI is deliberately in demo mode. No network requests or credentials.
    });
    {
        let cap = capsule.as_weak();
        let settings_weak = settings.as_weak();
        let pref = preferences.clone();
        let model = model.clone();
        settings.on_set_pinned(move |enabled| {
            if let (Some(c), Some(s)) = (cap.upgrade(), settings_weak.upgrade()) {
                set_mode(
                    if enabled { Mode::Pinned } else { Mode::Popup },
                    &pref,
                    &model,
                    &c,
                    &s,
                );
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let settings_weak = settings.as_weak();
        let pref = preferences.clone();
        let model = model.clone();
        settings.on_toggle_service(move |index, enabled| {
            if let Ok(i) = usize::try_from(index) {
                if i < 3 {
                    let mut value = pref.borrow_mut();
                    value.shown[i] = enabled;
                    if let Err(error) = state::save(&value) {
                        eprintln!("Saving preferences: {error}");
                    }
                    if let (Some(c), Some(s)) = (cap.upgrade(), settings_weak.upgrade()) {
                        refresh_model(&model, &c, &s, &value);
                    }
                }
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        settings.on_reset_position(move || {
            if let Some(c) = cap.upgrade() {
                let mut value = pref.borrow_mut();
                value.saved_position = None;
                let _ = state::save(&value);
                if value.mode == Mode::Pinned {
                    place_capsule(&c, &value, None);
                }
            }
        });
    }

    // Diagnostic hook only runs when explicitly requested on a debug build.
    #[cfg(debug_assertions)]
    if std::env::var_os("WACU_QA").is_some() {
        let weak = capsule.as_weak();
        Timer::single_shot(Duration::from_millis(1800), move || {
            if let Some(c) = weak.upgrade() {
                eprintln!(
                    "WACU_QA visible={} revealed={}",
                    c.window().is_visible(),
                    c.get_revealed()
                );
                match c.window().take_snapshot() {
                    Ok(snapshot) => {
                        let bytes = snapshot.as_bytes();
                        let opaque = bytes.chunks_exact(4).filter(|p| p[3] > 0).count();
                        let colorful = bytes
                            .chunks_exact(4)
                            .filter(|p| p[0] > 70 && p[1] > 100 && p[2] > 70 && p[3] > 0)
                            .count();
                        eprintln!(
                            "WACU_QA snapshot={}x{} opaque={} colorful={}",
                            snapshot.width(),
                            snapshot.height(),
                            opaque,
                            colorful
                        );
                    }
                    Err(error) => eprintln!("WACU_QA snapshot_error={error}"),
                }
            }
        });
    }

    // A visible native tray keeps Slint's event loop running while windows are hidden.
    tray.show()?;
    if initial.mode == Mode::Pinned {
        show_capsule(&capsule, &initial, None);
    }

    // Low-frequency housekeeping: no rendering or network polling during idle.
    let housekeeping = Timer::default();
    {
        let cap = capsule.as_weak();
        let details = detail.as_weak();
        let prefs = preferences.clone();
        let last_pos = Rc::new(RefCell::new(None::<(i32, i32)>));
        housekeeping.start(TimerMode::Repeated, Duration::from_millis(950), move || {
            let Some(c) = cap.upgrade() else { return };
            if let Some(d) = details.upgrade() {
                if d.window().is_visible()
                    && !platform_windows::is_foreground(d.window())
                    && !platform_windows::is_foreground(c.window())
                {
                    let _ = d.hide();
                }
            }
            if !c.window().is_visible() {
                return;
            }
            if prefs.borrow().mode == Mode::Popup {
                if !platform_windows::is_foreground(c.window()) {
                    // Allow a brief activation grace period after tray click.
                    c.set_revealed(false);
                    let _ = c.hide();
                }
            } else {
                let pos = c.window().position();
                let current = (pos.x, pos.y);
                if last_pos.borrow().as_ref() != Some(&current) {
                    *last_pos.borrow_mut() = Some(current);
                    let mut prefs = prefs.borrow_mut();
                    if prefs.saved_position != Some(current) {
                        prefs.saved_position = Some(current);
                        let _ = state::save(&prefs);
                    }
                }
            }
        });
    }
    slint::run_event_loop()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn demo_arc_is_generated() {
        assert!(arc_svg(76).as_str().contains("A 24 24"));
        assert!(arc_svg(0).as_str().contains("M 56 56"));
    }
}
