#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod placement;
mod platform_windows;
mod state;

use std::{
    cell::{Cell, RefCell},
    error::Error,
    rc::Rc,
    time::{Duration, Instant},
};

use placement::{Point, constrain, docked, edge_position, popup};
use slint::{
    ComponentHandle, Model, ModelRc, PhysicalPosition, SharedString, Timer, TimerMode, VecModel,
};
use state::{Mode, Preferences};

slint::include_modules!();

const NAMES: [&str; 3] = ["Codex", "Claude Code", "WorkBuddy"];
const SUBTITLES: [&str; 3] = ["OpenAI · ChatGPT", "Anthropic · Claude", "Credits · 积分"];
// Used percentages (never remaining percentages). Explicitly marked demo in the UI.
const VALUES: [i32; 3] = [13, 24, 2];

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
        0 => slint::Color::from_rgb_u8(34, 197, 94),
        1 => slint::Color::from_rgb_u8(255, 138, 76),
        _ => slint::Color::from_rgb_u8(139, 92, 246),
    };
    let used = VALUES[index] * 10;
    Service {
        source_index: index as i32,
        label: NAMES[index].into(),
        subtitle: SUBTITLES[index].into(),
        percent: VALUES[index],
        used: used.to_string().into(),
        total: "1,000".into(),
        remaining: (1000 - used).to_string().into(),
        reset: "2026/10/10 00:00".into(),
        accent: tint,
        arc: arc_svg(VALUES[index]),
        visible,
    }
}

fn visible_service_indices(shown: &[bool; 3]) -> impl Iterator<Item = usize> + '_ {
    // Match the reference's Claude / OpenAI / purple order, keeping stable IDs.
    [1, 0, 2].into_iter().filter(|&index| shown[index])
}

fn refresh_model(
    model: &Rc<VecModel<Service>>,
    capsule: &CapsuleWindow,
    settings: &SettingsWindow,
    prefs: &Preferences,
) {
    while model.row_count() > 0 {
        model.remove(model.row_count() - 1);
    }
    for i in visible_service_indices(&prefs.shown) {
        model.push(service(i, true));
    }
    capsule.set_visible_count(model.row_count() as i32);
    settings.set_show_codex(prefs.shown[0]);
    settings.set_show_claude(prefs.shown[1]);
    settings.set_show_workbuddy(prefs.shown[2]);
    settings.set_pinned_mode(prefs.mode == Mode::Pinned);
    capsule.set_is_pinned(prefs.mode == Mode::Pinned);
    capsule.set_animated(prefs.animations_enabled);
    capsule.set_surface_opacity(prefs.opacity_percent.clamp(65, 100) as f32 / 100.0);
    settings.set_animations_on(prefs.animations_enabled);
    settings.set_opacity_percent(prefs.opacity_percent.clamp(65, 100) as f32);
    settings.set_edge_hide_on(prefs.edge_hide);
    settings.set_dock_side(if prefs.dock_left { 1 } else { 0 });
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
                docked(
                    size,
                    platform_windows::work_area_at(platform_windows::cursor()),
                    prefs.dock_left,
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

fn show_settings(settings: &SettingsWindow) {
    settings.set_startup_on(platform_windows::startup_enabled());
    let _ = settings.show();
    platform_windows::activate(settings.window());
}

fn position_detail(detail: &DetailWindow, capsule: &CapsuleWindow) {
    let cp = capsule.window().position();
    let cs = capsule.window().size();
    let ds = detail.window().size();
    let area = platform_windows::work_area_at(Point {
        x: cp.x + cs.width as i32 / 2,
        y: cp.y,
    });
    let margin = (8.0 * capsule.window().scale_factor()).round() as i32;
    let right = cp.x + cs.width as i32 - margin;
    let x = if right + ds.width as i32 <= area.right {
        right
    } else {
        cp.x - ds.width as i32 + margin
    };
    capsule.set_panel_on_left(x < cp.x);
    let dest = constrain(
        Point { x, y: cp.y },
        (ds.width as i32, ds.height as i32),
        area,
    );
    detail
        .window()
        .set_position(PhysicalPosition::new(dest.x, dest.y));
}

fn show_detail(detail: &DetailWindow, capsule: &CapsuleWindow, index: usize, prefs: &Preferences) {
    if !prefs.shown[index] {
        return;
    }
    detail.set_selected_service(service(index, true));
    detail.set_animated(prefs.animations_enabled);
    capsule.set_selected_index(index as i32);
    detail.set_expanded(false);
    detail.set_selected_period(0);
    let _ = detail.show();
    position_detail(detail, capsule);
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
    #[cfg(debug_assertions)]
    let mut initial = initial;
    #[cfg(debug_assertions)]
    let qa_view = std::env::var("WACU_QA_VIEW").unwrap_or_default();
    #[cfg(debug_assertions)]
    if matches!(
        qa_view.as_str(),
        "capsule" | "quick" | "detail" | "settings"
    ) {
        initial = Preferences {
            mode: Mode::Pinned,
            ..Preferences::default()
        };
        let count = std::env::var("WACU_QA_COUNT")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(3)
            .min(3);
        initial.shown = [false; 3];
        for &index in [1, 0, 2].iter().take(count) {
            initial.shown[index] = true;
        }
    }
    let preferences = Rc::new(RefCell::new(initial.clone()));
    let edge_origin = Rc::new(Cell::new(None::<Point>));
    let last_popup_open = Rc::new(Cell::new(Instant::now() - Duration::from_secs(3)));

    let capsule = CapsuleWindow::new()?;
    let settings = SettingsWindow::new()?;
    let detail = DetailWindow::new()?;
    let tray = WacuTray::new()?;
    tray.set_tray_icon(make_tray_icon());

    let model = Rc::new(VecModel::from(
        visible_service_indices(&initial.shown)
            .map(|i| service(i, true))
            .collect::<Vec<_>>(),
    ));
    capsule.set_services(ModelRc::from(model.clone()));
    refresh_model(&model, &capsule, &settings, &initial);

    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        let last_open = last_popup_open.clone();
        let origin = edge_origin.clone();
        tray.on_tray_clicked(move || {
            #[cfg(debug_assertions)]
            if std::env::var_os("WACU_QA").is_some() {
                eprintln!("WACU_QA tray_clicked mode={:?}", pref.borrow().mode);
            }
            if let Some(c) = cap.upgrade() {
                origin.set(None);
                if !c.window().is_visible() {
                    last_open.set(Instant::now());
                }
                toggle_capsule(&c, &pref.borrow());
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        let origin = edge_origin.clone();
        tray.on_switch_mode({
            let settings = settings.as_weak();
            let model = model.clone();
            move || {
                if let (Some(c), Some(s)) = (cap.upgrade(), settings.upgrade()) {
                    origin.set(None);
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
                show_settings(&s);
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
                show_settings(&s);
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
        let cap = capsule.as_weak();
        detail.on_close_detail(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
            if let Some(c) = cap.upgrade() {
                c.set_selected_index(-1);
            }
        });
    }
    {
        let weak = detail.as_weak();
        detail.on_open_dashboard(move || {
            if let Some(d) = weak.upgrade() {
                let url = match d.get_selected_service().source_index {
                    0 => "https://chatgpt.com/codex/settings/usage",
                    1 => "https://claude.ai/settings/usage",
                    _ => "https://www.codebuddy.cn/work/",
                };
                if let Err(error) = platform_windows::open_url(url) {
                    eprintln!("Opening provider website: {error}");
                }
            }
        });
    }
    {
        let weak = detail.as_weak();
        let cap = capsule.as_weak();
        detail.on_expand_detail(move || {
            if let (Some(detail), Some(capsule)) = (weak.upgrade(), cap.upgrade()) {
                detail.set_expanded(true);
                let d = detail.as_weak();
                let c = capsule.as_weak();
                Timer::single_shot(Duration::from_millis(45), move || {
                    if let (Some(d), Some(c)) = (d.upgrade(), c.upgrade()) {
                        position_detail(&d, &c);
                    }
                });
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let settings_weak = settings.as_weak();
        let pref = preferences.clone();
        let model = model.clone();
        let origin = edge_origin.clone();
        settings.on_set_pinned(move |enabled| {
            if let (Some(c), Some(s)) = (cap.upgrade(), settings_weak.upgrade()) {
                origin.set(None);
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
        let details = detail.as_weak();
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
                        if c.get_selected_index() == index && !enabled {
                            c.set_selected_index(-1);
                            if let Some(d) = details.upgrade() {
                                let _ = d.hide();
                            }
                        }
                    }
                }
            }
        });
    }
    {
        let weak = settings.as_weak();
        settings.on_close_settings(move || {
            if let Some(s) = weak.upgrade() {
                let _ = s.hide();
            }
        });
    }
    {
        let weak = settings.as_weak();
        settings.on_start_settings_drag(move || {
            if let Some(s) = weak.upgrade() {
                platform_windows::begin_drag(s.window());
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        let settings_weak = settings.as_weak();
        let detail_weak = detail.as_weak();
        settings.on_set_animations(move |enabled| {
            let mut value = pref.borrow_mut();
            value.animations_enabled = enabled;
            if let Err(error) = state::save(&value) {
                eprintln!("Saving animations: {error}");
            }
            if let Some(c) = cap.upgrade() {
                c.set_animated(enabled);
            }
            if let Some(s) = settings_weak.upgrade() {
                s.set_animations_on(enabled);
            }
            if let Some(d) = detail_weak.upgrade() {
                d.set_animated(enabled);
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        let settings_weak = settings.as_weak();
        settings.on_set_opacity(move |percent| {
            let percent = percent.round().clamp(65.0, 100.0) as u8;
            let mut value = pref.borrow_mut();
            value.opacity_percent = percent;
            if let Err(error) = state::save(&value) {
                eprintln!("Saving opacity: {error}");
            }
            if let Some(c) = cap.upgrade() {
                c.set_surface_opacity(percent as f32 / 100.0);
            }
            if let Some(s) = settings_weak.upgrade() {
                s.set_opacity_percent(percent as f32);
            }
        });
    }
    {
        let weak = settings.as_weak();
        settings.on_set_startup(move |enabled| {
            if let Some(s) = weak.upgrade() {
                match platform_windows::set_startup(enabled) {
                    Ok(()) => {
                        s.set_startup_on(platform_windows::startup_enabled());
                        s.set_status_message("".into());
                    }
                    Err(error) => {
                        s.set_status_message(error.into());
                    }
                }
            }
        });
    }
    {
        let weak = settings.as_weak();
        settings.on_open_link(move |index| {
            let url = match index {
                1 => "https://github.com/zhenyueyue/Wacu/issues",
                2 => "https://github.com/zhenyueyue/Wacu#readme",
                3 => "https://github.com/zhenyueyue/Wacu/releases",
                _ => "https://github.com/zhenyueyue/Wacu",
            };
            if let Some(s) = weak.upgrade() {
                s.set_status_message(match platform_windows::open_url(url) {
                    Ok(()) => "".into(),
                    Err(error) => error.into(),
                });
            }
        });
    }
    {
        let pref = preferences.clone();
        let weak = settings.as_weak();
        settings.on_set_edge_hide(move |enabled| {
            let mut value = pref.borrow_mut();
            value.edge_hide = enabled;
            if let Some(s) = weak.upgrade() {
                s.set_edge_hide_on(enabled);
                s.set_status_message(match state::save(&value) {
                    Ok(()) => "".into(),
                    Err(error) => error.into(),
                });
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        let weak = settings.as_weak();
        let origin = edge_origin.clone();
        settings.on_set_dock_side(move |index| {
            let mut value = pref.borrow_mut();
            value.dock_left = index == 1;
            value.saved_position = None;
            origin.set(None);
            if let Some(c) = cap.upgrade() {
                if value.mode == Mode::Pinned {
                    place_capsule(&c, &value, None);
                }
            }
            if let Some(s) = weak.upgrade() {
                s.set_dock_side(if value.dock_left { 1 } else { 0 });
                s.set_status_message(match state::save(&value) {
                    Ok(()) => "".into(),
                    Err(error) => error.into(),
                });
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        let origin = edge_origin.clone();
        settings.on_reset_position(move || {
            if let Some(c) = cap.upgrade() {
                let mut value = pref.borrow_mut();
                value.saved_position = None;
                origin.set(None);
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

    // Debug-only visual QA; does not exist in the Release application.
    #[cfg(debug_assertions)]
    if !qa_view.is_empty() {
        if qa_view == "capsule" {
            let selected = std::env::var("WACU_QA_SELECTED")
                .ok()
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(-1);
            capsule.set_selected_index(selected);
        }
        match qa_view.as_str() {
            "quick" | "detail" => {
                show_detail(&detail, &capsule, 1, &initial);
                if qa_view == "detail" {
                    detail.set_expanded(true);
                    detail.set_selected_period(
                        std::env::var("WACU_QA_PERIOD")
                            .ok()
                            .and_then(|value| value.parse::<i32>().ok())
                            .unwrap_or(0)
                            .clamp(0, 3),
                    );
                    let d = detail.as_weak();
                    let c = capsule.as_weak();
                    Timer::single_shot(Duration::from_millis(150), move || {
                        if let (Some(d), Some(c)) = (d.upgrade(), c.upgrade()) {
                            position_detail(&d, &c);
                        }
                    });
                }
            }
            "settings" => {
                let page = std::env::var("WACU_QA_PAGE")
                    .ok()
                    .and_then(|page| page.parse::<i32>().ok())
                    .unwrap_or(0)
                    .clamp(0, 4);
                settings.set_selected_page(page);
                show_settings(&settings);
            }
            _ => {}
        }
    }

    #[cfg(debug_assertions)]
    if matches!(
        qa_view.as_str(),
        "capsule" | "quick" | "detail" | "settings"
    ) {
        let mode = qa_view.clone();
        let c = capsule.as_weak();
        let d = detail.as_weak();
        let s = settings.as_weak();
        Timer::single_shot(Duration::from_millis(900), move || {
            let snapshot = match mode.as_str() {
                "capsule" => c
                    .upgrade()
                    .and_then(|window| window.window().take_snapshot().ok()),
                "quick" | "detail" => d
                    .upgrade()
                    .and_then(|window| window.window().take_snapshot().ok()),
                "settings" => s
                    .upgrade()
                    .and_then(|window| window.window().take_snapshot().ok()),
                _ => None,
            };
            if let Some(snapshot) = snapshot {
                let result = (|| -> Result<(), Box<dyn Error>> {
                    let mut encoder = png::Encoder::new(
                        std::fs::File::create(format!("target/wacu-qa-{mode}.png"))?,
                        snapshot.width(),
                        snapshot.height(),
                    );
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()?
                        .write_image_data(snapshot.as_bytes())?;
                    Ok(())
                })();
                if let Err(error) = result {
                    eprintln!("Wacu preview snapshot failed: {error}");
                }
            } else {
                eprintln!("Wacu preview snapshot unavailable");
            }
            if std::env::var_os("WACU_QA_EXIT").is_some() {
                let _ = slint::quit_event_loop();
            }
        });
    }

    // Low-frequency pointer/focus checks. No animation frames or network polling.
    let housekeeping = Timer::default();
    {
        let cap = capsule.as_weak();
        let details = detail.as_weak();
        let prefs = preferences.clone();
        let popup_open = last_popup_open.clone();
        let last_pos = Rc::new(RefCell::new(None::<(i32, i32)>));
        let origin = edge_origin.clone();
        let last_save = Rc::new(Cell::new(Instant::now()));
        let edge_revealed = Rc::new(Cell::new(Instant::now() - Duration::from_secs(1)));
        housekeeping.start(TimerMode::Repeated, Duration::from_millis(200), move || {
            let Some(c) = cap.upgrade() else { return };
            #[cfg(debug_assertions)]
            if std::env::var_os("WACU_QA_VIEW").is_some() {
                return;
            }
            let mut detail_active = false;
            if let Some(d) = details.upgrade() {
                detail_active = d.window().is_visible()
                    && (platform_windows::is_foreground(d.window())
                        || platform_windows::cursor_is_over(d.window()));
                if d.window().is_visible()
                    && !platform_windows::is_foreground(d.window())
                    && !platform_windows::is_foreground(c.window())
                    && !platform_windows::cursor_is_over(d.window())
                    && !platform_windows::cursor_is_over(c.window())
                {
                    let _ = d.hide();
                    c.set_selected_index(-1);
                }
            }
            if !c.window().is_visible() {
                return;
            }
            if prefs.borrow().mode == Mode::Popup {
                if origin.take().is_some() {
                    place_capsule(&c, &prefs.borrow(), None);
                }
                // Windows may temporarily retain focus on the taskbar overflow/menu.
                // Give a newly opened popup time to receive focus and let pointer hover
                // prevent accidental dismissal when SetForegroundWindow is denied.
                if popup_open.get().elapsed() > Duration::from_millis(1500)
                    && !platform_windows::is_foreground(c.window())
                    && !platform_windows::cursor_is_over(c.window())
                    && !detail_active
                {
                    c.set_revealed(false);
                    let _ = c.hide();
                }
            } else {
                if let Some(original) = origin.get() {
                    if !prefs.borrow().edge_hide
                        || platform_windows::cursor_is_over(c.window())
                        || detail_active
                    {
                        c.window()
                            .set_position(PhysicalPosition::new(original.x, original.y));
                        origin.set(None);
                        // The edge target lies outside the restored window; give the
                        // pointer time to cross the desktop margin into the capsule.
                        edge_revealed.set(Instant::now());
                    } else {
                        return;
                    }
                }
                let pos = c.window().position();
                let point = Point { x: pos.x, y: pos.y };
                let size = capsule_dimensions(&c);
                let area = platform_windows::work_area_at(Point {
                    x: pos.x + size.0 / 2,
                    y: pos.y + size.1 / 2,
                });
                if prefs.borrow().edge_hide
                    && !detail_active
                    && !platform_windows::cursor_is_over(c.window())
                    && edge_revealed.get().elapsed() > Duration::from_millis(800)
                {
                    if let Some(hidden) =
                        edge_position(point, size, area, c.window().scale_factor())
                    {
                        // Persist the visible position, never the off-screen hidden position.
                        let mut value = prefs.borrow_mut();
                        if value.saved_position != Some((point.x, point.y)) {
                            value.saved_position = Some((point.x, point.y));
                            let _ = state::save(&value);
                        }
                        origin.set(Some(point));
                        c.window()
                            .set_position(PhysicalPosition::new(hidden.x, hidden.y));
                        return;
                    }
                }
                let current = (pos.x, pos.y);
                if last_pos.borrow().as_ref() != Some(&current)
                    && last_save.get().elapsed() >= Duration::from_secs(1)
                {
                    last_save.set(Instant::now());
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

    #[test]
    fn filtered_services_keep_their_original_ids() {
        assert_eq!(
            visible_service_indices(&[false, true, true]).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            visible_service_indices(&[true, false, true]).collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(visible_service_indices(&[false, false, false]).count(), 0);
        assert_eq!(service(2, true).source_index, 2);
        assert_eq!(
            visible_service_indices(&[true; 3]).collect::<Vec<_>>(),
            vec![1, 0, 2]
        );
    }

    #[test]
    fn demo_usage_and_remaining_are_consistent() {
        for index in 0..3 {
            let item = service(index, true);
            let used: i32 = item.used.parse().unwrap();
            let remaining: i32 = item.remaining.parse().unwrap();
            assert_eq!(used + remaining, 1000);
            assert_eq!(item.percent, used / 10);
        }
    }
}
