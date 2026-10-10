#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod glow;
mod placement;
mod platform_windows;
mod state;

use std::{
    cell::{Cell, RefCell},
    error::Error,
    rc::Rc,
    time::{Duration, Instant},
};

use placement::{Point, constrain, docked, edge_position};
use slint::{
    ComponentHandle, Model, ModelRc, PhysicalPosition, SharedString, Timer, TimerMode, VecModel,
};
use state::{Mode, Preferences};

slint::include_modules!();

const NAMES: [&str; 3] = ["Codex", "Claude Code", "WorkBuddy"];
const SUBTITLES: [&str; 3] = ["OpenAI · ChatGPT", "Anthropic · Claude", "Credits · 积分"];
// Used percentages (never remaining percentages). Explicitly marked demo in the UI.
const VALUES: [i32; 3] = [13, 24, 2];

fn arc_svg(percent: i32) -> SharedString {
    if percent <= 0 {
        return "".into();
    }
    if percent >= 100 {
        return "M 28 4 A 24 24 0 1 1 28 52 A 24 24 0 1 1 28 4 Z".into();
    }
    let f = (percent.min(100) as f64 / 100.0).min(0.99998);
    let angle = (-90.0 + f * 360.0).to_radians();
    let end_x = 28.0 + 24.0 * angle.cos();
    let end_y = 28.0 + 24.0 * angle.sin();
    let large = if percent > 50 { 1 } else { 0 };
    format!("M 28 4 A 24 24 0 {large} 1 {end_x:.2} {end_y:.2}").into()
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
        glow: glow::make(VALUES[index], tint),
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
    capsule.set_size_mode(prefs.capsule_size.min(2) as i32);
    settings.set_capsule_size(prefs.capsule_size.min(2) as i32);
    settings.set_show_codex(prefs.shown[0]);
    settings.set_show_claude(prefs.shown[1]);
    settings.set_show_workbuddy(prefs.shown[2]);
    settings.set_pinned_mode(prefs.mode == Mode::Pinned);
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

fn place_capsule(capsule: &CapsuleWindow, prefs: &Preferences) {
    let size = capsule_dimensions(capsule);
    let position = if let Some((x, y)) = prefs.saved_position {
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
    };
    capsule
        .window()
        .set_position(PhysicalPosition::new(position.x, position.y));
}

fn remember_position(
    capsule: &CapsuleWindow,
    prefs: &Rc<RefCell<Preferences>>,
    visible_origin: Option<Point>,
) {
    if !capsule.window().is_visible() {
        return;
    }
    let current = capsule.window().position();
    let point = visible_origin.unwrap_or(Point {
        x: current.x,
        y: current.y,
    });
    let mut value = prefs.borrow_mut();
    if value.saved_position == Some((point.x, point.y)) {
        return;
    }
    value.saved_position = Some((point.x, point.y));
    // Visual previews update only their in-memory preferences.
    #[cfg(debug_assertions)]
    if std::env::var_os("WACU_QA_VIEW").is_some() {
        return;
    }
    if let Err(error) = state::save(&value) {
        eprintln!("Saving position: {error}");
    }
}

fn show_capsule(capsule: &CapsuleWindow, prefs: &Rc<RefCell<Preferences>>) {
    capsule.set_revealed(false);
    let _ = capsule.show();
    place_capsule(capsule, &prefs.borrow());
    remember_position(capsule, prefs, None);
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

fn toggle_capsule(capsule: &CapsuleWindow, prefs: &Rc<RefCell<Preferences>>) {
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
        show_capsule(capsule, prefs);
    }
}

fn should_hide_popup(
    elapsed: Duration,
    startup_preview: bool,
    foreground: bool,
    hovered: bool,
    detail_active: bool,
) -> bool {
    if hovered || detail_active {
        return false;
    }
    if startup_preview {
        elapsed >= Duration::from_secs(3)
    } else {
        elapsed > Duration::from_millis(1500) && !foreground
    }
}

fn set_mode(
    mode: Mode,
    prefs: &Rc<RefCell<Preferences>>,
    model: &Rc<VecModel<Service>>,
    capsule: &CapsuleWindow,
    settings: &SettingsWindow,
    edge_origin: &Cell<Option<Point>>,
) {
    remember_position(capsule, prefs, edge_origin.take());
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
        Mode::Pinned => show_capsule(capsule, prefs),
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

#[cfg(debug_assertions)]
fn qa_click(window: &slint::Window, x: f32, y: f32) {
    use slint::platform::WindowEvent;
    let position = slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
}

fn main() -> Result<(), Box<dyn Error>> {
    let initial = state::load();
    #[cfg(debug_assertions)]
    let mut initial = initial;
    #[cfg(debug_assertions)]
    let qa_view = std::env::var("WACU_QA_VIEW").unwrap_or_default();
    #[cfg(debug_assertions)]
    if let Ok(mode) = std::env::var("WACU_QA_STARTUP") {
        let expected = std::env::current_dir()?.join("target/qa-startup-profile");
        let actual = std::env::var("APPDATA")
            .unwrap_or_default()
            .replace('/', "\\");
        if qa_view != "capsule"
            || !matches!(mode.as_str(), "popup" | "pinned")
            || !actual.eq_ignore_ascii_case(&expected.to_string_lossy().replace('/', "\\"))
        {
            return Err("Startup QA requires an isolated APPDATA".into());
        }
    }
    #[cfg(debug_assertions)]
    if std::env::var_os("WACU_QA_POSITION").is_some() {
        let expected = std::env::current_dir()?.join("target/qa-position-profile");
        let actual = std::env::var("APPDATA")
            .unwrap_or_default()
            .replace('/', "\\");
        if qa_view != "capsule"
            || !actual.eq_ignore_ascii_case(&expected.to_string_lossy().replace('/', "\\"))
        {
            return Err("Position QA requires an isolated APPDATA".into());
        }
    }
    #[cfg(debug_assertions)]
    if matches!(
        qa_view.as_str(),
        "capsule" | "quick" | "detail" | "settings"
    ) {
        initial = Preferences {
            mode: Mode::Pinned,
            ..Preferences::default()
        };
        if std::env::var("WACU_QA_STARTUP").as_deref() == Ok("popup") {
            initial.mode = Mode::Popup;
        }
        initial.capsule_size = std::env::var("WACU_QA_SIZE")
            .ok()
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or(1)
            .min(2);
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
    let startup_preview = Rc::new(Cell::new(false));
    let dragging = Rc::new(Cell::new(false));

    let capsule = CapsuleWindow::new()?;
    let settings = SettingsWindow::new()?;
    let detail = DetailWindow::new()?;
    let tray = WacuTray::new()?;

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
        let preview = startup_preview.clone();
        tray.on_tray_clicked(move || {
            preview.set(false);
            #[cfg(debug_assertions)]
            if std::env::var_os("WACU_QA").is_some() {
                eprintln!("WACU_QA tray_clicked mode={:?}", pref.borrow().mode);
            }
            if let Some(c) = cap.upgrade() {
                remember_position(&c, &pref, origin.take());
                if !c.window().is_visible() {
                    last_open.set(Instant::now());
                }
                toggle_capsule(&c, &pref);
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        let origin = edge_origin.clone();
        let preview = startup_preview.clone();
        tray.on_switch_mode({
            let settings = settings.as_weak();
            let model = model.clone();
            move || {
                if let (Some(c), Some(s)) = (cap.upgrade(), settings.upgrade()) {
                    preview.set(false);
                    let next = if pref.borrow().mode == Mode::Popup {
                        Mode::Pinned
                    } else {
                        Mode::Popup
                    };
                    set_mode(next, &pref, &model, &c, &s, &origin);
                }
            }
        });
    }
    {
        let weak = settings.as_weak();
        let preview = startup_preview.clone();
        tray.on_show_settings(move || {
            preview.set(false);
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
        let preview = startup_preview.clone();
        capsule.on_open_settings(move || {
            preview.set(false);
            if let Some(s) = weak.upgrade() {
                show_settings(&s);
            }
        });
    }
    {
        let weak = capsule.as_weak();
        let prefs = preferences.clone();
        let preview = startup_preview.clone();
        let active_drag = dragging.clone();
        let last_open = last_popup_open.clone();
        capsule.on_start_drag(move || {
            preview.set(false);
            active_drag.set(true);
            if let Some(c) = weak.upgrade() {
                c.set_logo_dragging(true);
            }
            let cap = weak.clone();
            let prefs = prefs.clone();
            let active_drag = active_drag.clone();
            let last_open = last_open.clone();
            // Give Slint one frame to show the pressed avatar before native dragging.
            Timer::single_shot(Duration::from_millis(35), move || {
                if let Some(c) = cap.upgrade() {
                    platform_windows::begin_drag(c.window());
                    last_open.set(Instant::now());
                    remember_position(&c, &prefs, None);
                    c.set_logo_dragging(false);
                }
                active_drag.set(false);
            });
        });
    }
    {
        let cap = capsule.as_weak();
        let view = detail.as_weak();
        let pref = preferences.clone();
        let preview = startup_preview.clone();
        capsule.on_open_service(move |idx| {
            preview.set(false);
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
        let preview = startup_preview.clone();
        settings.on_set_pinned(move |enabled| {
            preview.set(false);
            if let (Some(c), Some(s)) = (cap.upgrade(), settings_weak.upgrade()) {
                set_mode(
                    if enabled { Mode::Pinned } else { Mode::Popup },
                    &pref,
                    &model,
                    &c,
                    &s,
                    &origin,
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
            let weak = weak.clone();
            Timer::single_shot(Duration::ZERO, move || {
                if let Some(s) = weak.upgrade() {
                    platform_windows::begin_drag(s.window());
                }
            });
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
        let prefs = preferences.clone();
        let capsule_weak = capsule.as_weak();
        let settings_weak = settings.as_weak();
        let detail_weak = detail.as_weak();
        let origin = edge_origin.clone();
        settings.on_set_capsule_size(move |size| {
            let size = size.clamp(0, 2) as u8;
            let mut value = prefs.borrow_mut();
            if value.capsule_size == size {
                return;
            }
            value.capsule_size = size;
            origin.set(None);
            if let Some(c) = capsule_weak.upgrade() {
                c.set_size_mode(size as i32);
            }
            if let Some(s) = settings_weak.upgrade() {
                s.set_capsule_size(size as i32);
                s.set_status_message(match state::save(&value) {
                    Ok(()) => "".into(),
                    Err(error) => error.into(),
                });
            } else if let Err(error) = state::save(&value) {
                eprintln!("Saving capsule size: {error}");
            }
            drop(value);
            let weak = capsule_weak.clone();
            let details = detail_weak.clone();
            let prefs = prefs.clone();
            Timer::single_shot(Duration::from_millis(40), move || {
                if let Some(c) = weak.upgrade() {
                    if c.window().is_visible() {
                        let preferences = prefs.borrow();
                        place_capsule(&c, &preferences);
                        if let Some(d) = details.upgrade() {
                            if d.window().is_visible() {
                                position_detail(&d, &c);
                            }
                        }
                    }
                }
            });
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
                if c.window().is_visible() || value.mode == Mode::Pinned {
                    place_capsule(&c, &value);
                    let position = c.window().position();
                    value.saved_position = Some((position.x, position.y));
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
                if c.window().is_visible() || value.mode == Mode::Pinned {
                    place_capsule(&c, &value);
                    let position = c.window().position();
                    value.saved_position = Some((position.x, position.y));
                }
                let _ = state::save(&value);
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
    #[cfg(not(debug_assertions))]
    let show_tray = true;
    #[cfg(debug_assertions)]
    let show_tray = !matches!(
        qa_view.as_str(),
        "capsule" | "quick" | "detail" | "settings"
    );
    if show_tray {
        tray.show()?;
    }
    startup_preview.set(initial.mode == Mode::Popup);
    last_popup_open.set(Instant::now());
    show_capsule(&capsule, &preferences);

    // Debug-only visual QA; does not exist in the Release application.
    #[cfg(debug_assertions)]
    if !qa_view.is_empty() {
        if std::env::var_os("WACU_QA_STARTUP").is_some() {
            let weak = capsule.as_weak();
            Timer::single_shot(Duration::from_millis(300), move || {
                let c = weak.upgrade().unwrap();
                assert!(c.window().is_visible() && c.get_revealed());
                // Keep the test window away from the actual mouse without moving it.
                let cursor = platform_windows::cursor();
                let area = platform_windows::work_area_at(cursor);
                let size = capsule_dimensions(&c);
                let point = Point {
                    x: if cursor.x < (area.left + area.right) / 2 {
                        area.right - size.0 - 20
                    } else {
                        area.left + 20
                    },
                    y: if cursor.y < (area.top + area.bottom) / 2 {
                        area.bottom - size.1 - 20
                    } else {
                        area.top + 20
                    },
                };
                c.window()
                    .set_position(PhysicalPosition::new(point.x, point.y));
                eprintln!("WACU_QA startup_shown");
            });
            let weak = capsule.as_weak();
            let mode = initial.mode;
            Timer::single_shot(Duration::from_millis(3700), move || {
                let c = weak.upgrade().unwrap();
                assert_eq!(c.window().is_visible(), mode == Mode::Pinned);
                eprintln!("WACU_QA startup_pass mode={mode:?}");
                let _ = slint::quit_event_loop();
            });
        }
        if std::env::var_os("WACU_QA_POSITION").is_some() {
            let cap = capsule.as_weak();
            let settings = settings.as_weak();
            let tray = tray.as_weak();
            let origin = edge_origin.clone();
            let prefs = preferences.clone();
            let detail = detail.as_weak();
            Timer::single_shot(Duration::from_millis(200), move || {
                let c = cap.upgrade().unwrap();
                let s = settings.upgrade().unwrap();
                let t = tray.upgrade().unwrap();
                let d = detail.upgrade().unwrap();
                let drag_calls = Rc::new(Cell::new(0));
                let calls = drag_calls.clone();
                // Check real Logo hit-testing without entering the OS mouse drag loop.
                c.on_start_drag(move || calls.set(calls.get() + 1));
                qa_click(c.window(), 44.0, 48.0);
                assert_eq!(drag_calls.get(), 1);
                let check_clicks = || {
                    let before = drag_calls.get();
                    for (index, y) in [(1, 109.0), (0, 179.0), (2, 249.0)] {
                        qa_click(c.window(), 44.0, y);
                        assert_eq!(
                            c.get_selected_index(),
                            index,
                            "Service click was intercepted"
                        );
                        assert!(d.window().is_visible());
                        assert_eq!(d.get_selected_service().source_index, index);
                        d.invoke_close_detail();
                    }
                    qa_click(c.window(), 44.0, 319.0);
                    assert!(s.window().is_visible(), "Settings click was intercepted");
                    s.invoke_close_settings();
                    assert_eq!(drag_calls.get(), before, "A button click triggered drag");
                };
                let check_drag_release = || {
                    use slint::platform::{PointerEventButton, WindowEvent};
                    let position = slint::LogicalPosition::new(44.0, 48.0);
                    c.window()
                        .dispatch_event(WindowEvent::PointerMoved { position });
                    c.window().dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    });
                    // Simulate the OS drag consuming mouse-up, then use the real recovery.
                    platform_windows::release_drag_input(c.window());
                    check_clicks();
                };
                check_clicks();
                check_drag_release();
                let size = capsule_dimensions(&c);
                let current = c.window().position();
                let area = platform_windows::work_area_at(Point {
                    x: current.x,
                    y: current.y,
                });
                let custom = constrain(
                    Point {
                        x: current.x - 60,
                        y: current.y + 22,
                    },
                    size,
                    area,
                );
                c.window()
                    .set_position(PhysicalPosition::new(custom.x, custom.y));
                let expect_position = |point: Point| {
                    let actual = c.window().position();
                    assert_eq!((actual.x, actual.y), (point.x, point.y));
                };
                // Capture a drag immediately, before the housekeeping save timer.
                s.invoke_set_pinned(false);
                t.invoke_tray_clicked();
                expect_position(custom);
                let before = drag_calls.get();
                qa_click(c.window(), 44.0, 48.0);
                assert_eq!(drag_calls.get(), before + 1);
                check_clicks();
                check_drag_release();
                s.invoke_set_pinned(true);
                expect_position(custom);
                // Switching while edge-hidden must retain the visible origin.
                origin.set(Some(custom));
                c.window()
                    .set_position(PhysicalPosition::new(area.right - 18, custom.y));
                s.invoke_set_pinned(false);
                t.invoke_tray_clicked();
                expect_position(custom);
                // Side selection applies immediately in Popup and survives switching.
                s.invoke_set_dock_side(1);
                let left = docked(size, area, true);
                expect_position(left);
                s.invoke_set_pinned(true);
                expect_position(left);
                s.invoke_set_pinned(false);
                t.invoke_tray_clicked();
                expect_position(left);
                assert_eq!(prefs.borrow().saved_position, Some((left.x, left.y)));
                assert_eq!(state::load().saved_position, Some((left.x, left.y)));
                eprintln!(
                    "WACU_QA position_pass logo, services, settings, lost_mouse_up, popup, pinned, edge-hidden, dock-left, saved"
                );
            });
        }
        if qa_view == "settings" && std::env::var_os("WACU_QA_INPUT").is_some() {
            // The capture script supplies an isolated APPDATA for this input case.
            let expected = std::env::current_dir()?
                .join("target")
                .join("qa-input-profile");
            let actual = std::env::var("APPDATA")
                .unwrap_or_default()
                .replace('/', "\\");
            if !actual.eq_ignore_ascii_case(&expected.to_string_lossy().replace('/', "\\")) {
                return Err(
                    "WACU_QA_INPUT requires the isolated target/qa-input-profile APPDATA".into(),
                );
            }
            let s = settings.as_weak();
            Timer::single_shot(Duration::from_millis(300), move || {
                if let Some(s) = s.upgrade() {
                    // Exercise title mouse capture recovery without an OS modal drag.
                    s.on_start_settings_drag(|| {});
                    use slint::platform::{PointerEventButton, WindowEvent};
                    let position = slint::LogicalPosition::new(120.0, 38.0);
                    s.window()
                        .dispatch_event(WindowEvent::PointerMoved { position });
                    s.window().dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    });
                    platform_windows::release_drag_input(s.window());
                    qa_click(s.window(), 250.0, 330.0);
                }
            });
            let s = settings.as_weak();
            Timer::single_shot(Duration::from_millis(350), move || {
                use slint::platform::{Key, PointerEventButton, WindowEvent};
                let s = s.upgrade().unwrap();
                let previous = s.get_opacity_percent();
                let start = slint::LogicalPosition::new(250.0, 330.0);
                let end = slint::LogicalPosition::new(300.0, 330.0);
                s.window().dispatch_event(WindowEvent::PointerPressed {
                    position: start,
                    button: PointerEventButton::Left,
                });
                s.window()
                    .dispatch_event(WindowEvent::PointerMoved { position: end });
                s.window().dispatch_event(WindowEvent::PointerReleased {
                    position: end,
                    button: PointerEventButton::Left,
                });
                assert!(s.get_opacity_percent() > previous, "Slider drag failed");
                qa_click(s.window(), 250.0, 330.0);
                let previous = s.get_opacity_percent();
                s.window().dispatch_event(WindowEvent::KeyPressed {
                    text: Key::RightArrow.into(),
                });
                s.window().dispatch_event(WindowEvent::KeyReleased {
                    text: Key::RightArrow.into(),
                });
                assert!(
                    s.get_opacity_percent() > previous,
                    "Slider keyboard focus failed"
                );
                qa_click(s.window(), 250.0, 330.0);
            });
            let s = settings.as_weak();
            Timer::single_shot(Duration::from_millis(400), move || {
                if let Some(s) = s.upgrade() {
                    qa_click(s.window(), 325.0, 374.0);
                }
            });
            let s = settings.as_weak();
            Timer::single_shot(Duration::from_millis(450), move || {
                if let Some(s) = s.upgrade() {
                    if let Ok(snapshot) = s.window().take_snapshot() {
                        let mut encoder = png::Encoder::new(
                            std::fs::File::create("target/qa-settings-popup.png").unwrap(),
                            snapshot.width(),
                            snapshot.height(),
                        );
                        encoder.set_color(png::ColorType::Rgba);
                        encoder.set_depth(png::BitDepth::Eight);
                        encoder
                            .write_header()
                            .unwrap()
                            .write_image_data(snapshot.as_bytes())
                            .unwrap();
                    }
                }
            });
            let s = settings.as_weak();
            Timer::single_shot(Duration::from_millis(500), move || {
                use slint::platform::{Key, WindowEvent};
                if let Some(s) = s.upgrade() {
                    for key in [Key::DownArrow, Key::Return] {
                        s.window()
                            .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
                        s.window()
                            .dispatch_event(WindowEvent::KeyReleased { text: key.into() });
                    }
                }
            });
            let prefs = preferences.clone();
            Timer::single_shot(Duration::from_millis(650), move || {
                let value = prefs.borrow();
                assert!(
                    (68..=75).contains(&value.opacity_percent),
                    "Native slider input was blocked: {}",
                    value.opacity_percent
                );
                assert!(value.dock_left, "Native ComboBox selection was blocked");
                eprintln!(
                    "WACU_QA input_pass opacity={} dock=left",
                    value.opacity_percent
                );
            });
        }
        if qa_view == "capsule" {
            capsule.set_logo_dragging(std::env::var_os("WACU_QA_LOGO_DRAG").is_some());
            capsule
                .set_preview_settings_pressed(std::env::var_os("WACU_QA_SETTINGS_PRESS").is_some());
            capsule.set_preview_pressed_service(
                std::env::var("WACU_QA_SERVICE_PRESS")
                    .ok()
                    .and_then(|value| value.parse::<i32>().ok())
                    .unwrap_or(-1),
            );
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
            if std::env::var_os("WACU_QA_EXIT").is_some()
                && std::env::var_os("WACU_QA_STARTUP").is_none()
            {
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
        let preview = startup_preview.clone();
        let active_drag = dragging.clone();
        let last_pos = Rc::new(RefCell::new(None::<(i32, i32)>));
        let origin = edge_origin.clone();
        let last_save = Rc::new(Cell::new(Instant::now()));
        let edge_revealed = Rc::new(Cell::new(Instant::now() - Duration::from_secs(1)));
        housekeeping.start(TimerMode::Repeated, Duration::from_millis(200), move || {
            let Some(c) = cap.upgrade() else { return };
            if active_drag.get() {
                return;
            }
            #[cfg(debug_assertions)]
            if std::env::var_os("WACU_QA_VIEW").is_some()
                && std::env::var_os("WACU_QA_STARTUP").is_none()
            {
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
                    place_capsule(&c, &prefs.borrow());
                }
                // Windows may temporarily retain focus on the taskbar overflow/menu.
                // Give a newly opened popup time to receive focus and let pointer hover
                // prevent accidental dismissal when SetForegroundWindow is denied.
                if should_hide_popup(
                    popup_open.get().elapsed(),
                    preview.get(),
                    platform_windows::is_foreground(c.window()),
                    platform_windows::cursor_is_over(c.window()),
                    detail_active,
                ) {
                    preview.set(false);
                    remember_position(&c, &prefs, None);
                    toggle_capsule(&c, &prefs);
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
    fn startup_preview_hides_after_three_seconds_even_with_focus() {
        assert!(!should_hide_popup(
            Duration::from_millis(2999),
            true,
            true,
            false,
            false
        ));
        assert!(should_hide_popup(
            Duration::from_secs(3),
            true,
            true,
            false,
            false
        ));
        assert!(!should_hide_popup(
            Duration::from_secs(4),
            true,
            true,
            true,
            false
        ));
        assert!(!should_hide_popup(
            Duration::from_secs(4),
            true,
            true,
            false,
            true
        ));
    }

    #[test]
    fn regular_popup_waits_for_focus_or_hover_to_leave() {
        assert!(!should_hide_popup(
            Duration::from_secs(4),
            false,
            true,
            false,
            false
        ));
        assert!(!should_hide_popup(
            Duration::from_millis(1500),
            false,
            false,
            false,
            false
        ));
        assert!(should_hide_popup(
            Duration::from_secs(2),
            false,
            false,
            false,
            false
        ));
        assert!(!should_hide_popup(
            Duration::from_secs(2),
            false,
            false,
            true,
            false
        ));
    }
    #[test]
    fn demo_arc_is_generated() {
        assert!(arc_svg(76).as_str().contains("A 24 24"));
        assert!(arc_svg(0).is_empty());
        assert!(arc_svg(100).as_str().ends_with('Z'));
        assert!(!arc_svg(76).as_str().contains("L 0 0"));
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
