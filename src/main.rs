#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod child_process;
mod claude;
mod claude_activity;
mod claude_schedule;
mod codex;
mod flow;
mod glow;
mod placement;
mod platform_windows;
mod quota;
mod state;

use std::{
    cell::{Cell, RefCell},
    error::Error,
    rc::Rc,
    time::{Duration, Instant},
};

use placement::{Point, constrain, docked, edge_position, pinned};
use slint::{
    ComponentHandle, Model, ModelRc, PhysicalPosition, SharedString, Timer, TimerMode, VecModel,
};
use state::{EntryView, Mode, Preferences};

slint::include_modules!();

const NAMES: [&str; 3] = ["Codex", "Claude Code", "WorkBuddy"];
const SUBTITLES: [&str; 3] = ["OpenAI · ChatGPT", "Anthropic · Claude", "Credits · 积分"];
// Demo inputs are used percentages; UI rings and headline values show remaining quota.
const VALUES: [i32; 3] = [13, 24, 2];

thread_local! {
    // UI-thread state; the worker sends only quota snapshots, never credentials.
    static CODEX_USAGE: RefCell<codex::UsageState> = RefCell::default();
    static CLAUDE_USAGE: RefCell<claude::UsageState> = RefCell::default();
}

fn arc_svg(percent: i32) -> SharedString {
    arc_percent(percent as f32)
}

fn arc_percent(percent: f32) -> SharedString {
    if !percent.is_finite() || percent <= 0.0 {
        return "".into();
    }
    if percent >= 100.0 {
        return "M 28 4 A 24 24 0 1 1 28 52 A 24 24 0 1 1 28 4 Z".into();
    }
    let f = (percent as f64 / 100.0).min(0.99998);
    let angle = (-90.0 + f * 360.0).to_radians();
    let end_x = 28.0 + 24.0 * angle.cos();
    let end_y = 28.0 + 24.0 * angle.sin();
    let large = if percent > 50.0 { 1 } else { 0 };
    format!("M 28 4 A 24 24 0 {large} 1 {end_x:.2} {end_y:.2}").into()
}

fn arc_trail(start: f32, span: f32) -> SharedString {
    if !start.is_finite() || !span.is_finite() || span <= 0.0 {
        return "".into();
    }
    let point = |percent: f64| {
        let angle = (percent * 3.6 - 90.0).to_radians();
        (28.0 + 24.0 * angle.cos(), 28.0 + 24.0 * angle.sin())
    };
    let (x, y) = point(start as f64);
    if span >= 100.0 {
        let (mx, my) = point(start as f64 + 50.0);
        return format!(
            "M {x:.2} {y:.2} A 24 24 0 0 1 {mx:.2} {my:.2} A 24 24 0 0 1 {x:.2} {y:.2} Z"
        )
        .into();
    }
    let (ex, ey) = point(start as f64 + span as f64);
    let large = if span > 50.0 { 1 } else { 0 };
    format!("M {x:.2} {y:.2} A 24 24 0 {large} 1 {ex:.2} {ey:.2}").into()
}

fn service(index: usize, visible: bool) -> Service {
    let tint = match index {
        0 => slint::Color::from_rgb_u8(34, 197, 94),
        1 => slint::Color::from_rgb_u8(255, 138, 76),
        _ => slint::Color::from_rgb_u8(139, 92, 246),
    };
    let used = VALUES[index] * 10;
    let mut item = Service {
        source_index: index as i32,
        label: NAMES[index].into(),
        subtitle: SUBTITLES[index].into(),
        percent: (100 - VALUES[index]) as f32,
        used: used.to_string().into(),
        total: "1,000".into(),
        remaining: (1000 - used).to_string().into(),
        reset: "2026/10/10 00:00".into(),
        accent: tint,
        arc: arc_svg(100 - VALUES[index]),
        glow: glow::make(100 - VALUES[index], tint),
        visible,
        percent_text: format!("{}%", 100 - VALUES[index]).into(),
        status: "演示数据".into(),
        window_label: "今日用量".into(),
        window_summary: format!("{used} 次").into(),
        quota_details: "".into(),
    };
    if index == 0 {
        CODEX_USAGE.with_borrow(|state| {
            fill_quota_service(
                &mut item,
                state.status(),
                state
                    .snapshot
                    .as_ref()
                    .and_then(codex::Snapshot::main_window),
                state.snapshot.as_ref().map(codex::Snapshot::details),
            );
        });
    } else if index == 1 {
        CLAUDE_USAGE.with_borrow(|state| {
            fill_quota_service(
                &mut item,
                state.status(),
                state
                    .snapshot
                    .as_ref()
                    .and_then(claude::Snapshot::main_window),
                state.snapshot.as_ref().map(claude::Snapshot::details),
            );
        });
        #[cfg(debug_assertions)]
        if std::env::var_os("WACU_QA_CLAUDE_FIXTURE").is_some() {
            item.status = format!("{}（离线样例）", item.status).into();
        }
    }
    item
}

fn fill_quota_service(
    item: &mut Service,
    status: String,
    window: Option<&quota::QuotaWindow>,
    details: Option<String>,
) {
    item.status = status.clone().into();
    item.percent = 0.0;
    item.percent_text = "--".into();
    item.arc = "".into();
    item.glow = slint::Image::default();
    item.used = "--".into();
    item.total = "--".into();
    item.remaining = "--".into();
    item.reset = "--".into();
    item.window_label = "额度窗口".into();
    item.window_summary = "暂不可用".into();
    item.quota_details = details.unwrap_or(status).into();
    if let Some(window) = window {
        if let Some(used) = window.percent() {
            let remaining = 100.0 - used;
            let remaining_text = if remaining == 0.0 || remaining == 100.0 {
                format!("{remaining:.0}%")
            } else {
                format!("{remaining:.1}%")
            };
            item.percent = remaining as f32;
            item.percent_text = remaining_text.clone().into();
            item.arc = arc_percent(remaining as f32);
            item.glow = glow::make(remaining.round() as i32, item.accent);
            item.used = format!("{used:.1}%").into();
            item.total = "100%".into();
            item.remaining = remaining_text.into();
            item.reset = window.reset().into();
            item.window_label = window.label().into();
            item.window_summary = format!("已用 {used:.1}%").into();
        }
    }
}

enum UsageResult {
    Codex(Result<codex::Snapshot, codex::FetchError>),
    Claude(Result<claude::Snapshot, claude::FetchError>),
}

fn set_usage_status(
    settings: &SettingsWindow,
    index: usize,
    status: Option<SharedString>,
    busy: bool,
) {
    if index == 0 {
        if let Some(status) = status {
            settings.set_codex_status(status);
        }
        settings.set_codex_refreshing(busy);
    } else {
        if let Some(status) = status {
            settings.set_claude_status(status);
        }
        settings.set_claude_refreshing(busy);
    }
}

fn start_usage_watcher(
    index: usize,
    model: Rc<VecModel<Service>>,
    settings: &SettingsWindow,
    detail: &DetailWindow,
    preferences: Rc<RefCell<Preferences>>,
) -> Timer {
    let timer = Timer::default();
    set_usage_status(settings, index, Some(service(index, true).status), false);
    if index == 1 {
        settings.set_claude_activity_connected(claude_activity::connected());
    }
    let qa_live = if index == 0 {
        "WACU_QA_CODEX_LIVE"
    } else {
        "WACU_QA_CLAUDE_LIVE"
    };
    if cfg!(debug_assertions)
        && std::env::var_os("WACU_QA_VIEW").is_some()
        && std::env::var_os(qa_live).is_none()
    {
        return timer;
    }
    let interval = if index == 0 {
        codex::REFRESH_INTERVAL
    } else {
        claude::REFRESH_INTERVAL
    };
    let (requests, incoming) = std::sync::mpsc::channel::<()>();
    let (results, outgoing) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut claude_account = None;
        while incoming.recv().is_ok() {
            let result = if index == 0 {
                UsageResult::Codex(codex::fetch())
            } else {
                let result = claude::fetch_for(claude_account.as_ref());
                if let Ok(snapshot) = &result {
                    claude_account = snapshot.account.clone();
                }
                UsageResult::Claude(result)
            };
            if results.send(result).is_err() {
                break;
            }
        }
    });
    let busy = Rc::new(Cell::new(false));
    let cooldown = Rc::new(Cell::new(Instant::now()));
    let schedule =
        (index == 1).then(|| Rc::new(RefCell::new(claude_schedule::Schedule::new(Instant::now()))));
    let monitor = (index == 1).then(claude_activity::Monitor::new);
    if index == 1 {
        settings.set_claude_activity_connected(claude_activity::connected());
        settings.set_claude_working(monitor.as_ref().is_some_and(|m| m.active()));
    }
    let request = {
        let busy = busy.clone();
        let cooldown = cooldown.clone();
        let schedule = schedule.clone();
        Rc::new(move |manual: bool| {
            if manual
                && schedule
                    .as_ref()
                    .is_some_and(|s| !s.borrow().due(Instant::now(), true, true, false))
            {
                return false;
            }
            if Instant::now() < cooldown.get() || busy.replace(true) {
                return false;
            }
            if requests.send(()).is_err() {
                busy.set(false);
                return false;
            }
            if let Some(schedule) = &schedule {
                schedule.borrow_mut().begin(Instant::now());
            }
            true
        })
    };
    set_usage_status(settings, index, Some(service(index, true).status), false);
    if preferences.borrow().shown[index]
        && monitor.as_ref().is_none_or(|m| m.active())
        && request(false)
    {
        set_usage_status(settings, index, None, true);
    }
    {
        let request = request.clone();
        let settings_weak = settings.as_weak();
        let refresh = move || {
            if request(true) {
                if let Some(settings) = settings_weak.upgrade() {
                    set_usage_status(&settings, index, None, true);
                }
            }
        };
        if index == 0 {
            settings.on_refresh_codex(refresh);
        } else {
            settings.on_refresh_claude(refresh);
        }
    }
    let settings = settings.as_weak();
    let detail = detail.as_weak();
    let mut next_refresh = Instant::now() + interval;
    let mut failures = 0u32;
    let mut was_enabled = preferences.borrow().shown[index];
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        if let Ok(result) = outgoing.try_recv() {
            let (success, retry) = match result {
                UsageResult::Codex(result) => {
                    let success = result.is_ok();
                    CODEX_USAGE.with_borrow_mut(|state| state.apply(result));
                    (success, None)
                }
                UsageResult::Claude(result) => {
                    let success = result.is_ok();
                    let retry = result.as_ref().err().and_then(|error| match error {
                        claude::FetchError::RateLimited(_) => Some(error.retry_after()),
                        _ => None,
                    });
                    CLAUDE_USAGE.with_borrow_mut(|state| state.apply(result));
                    (success, retry)
                }
            };
            failures = if success { 0 } else { (failures + 1).min(4) };
            let delay =
                retry.unwrap_or((interval * (1 << failures)).min(Duration::from_secs(1800)));
            next_refresh = Instant::now() + delay;
            if retry.is_some() {
                cooldown.set(next_refresh);
            }
            if let Some(schedule) = &schedule {
                schedule
                    .borrow_mut()
                    .complete(Instant::now(), delay, retry.is_some());
            }
            busy.set(false);
            let item = service(index, true);
            for row in 0..model.row_count() {
                if model
                    .row_data(row)
                    .is_some_and(|s| s.source_index == index as i32)
                {
                    model.set_row_data(row, item.clone());
                }
            }
            if let Some(detail) = detail.upgrade() {
                if detail.get_selected_service().source_index == index as i32 {
                    detail.set_selected_service(item.clone());
                }
            }
            if let Some(settings) = settings.upgrade() {
                set_usage_status(&settings, index, Some(item.status), false);
            }
        }
        let enabled = preferences.borrow().shown[index];
        let working = monitor.as_ref().is_none_or(|m| m.active());
        if let Some(settings) = settings.upgrade() {
            if index == 1 {
                settings.set_claude_working(working);
                settings.set_claude_activity_connected(claude_activity::connected());
                settings.set_claude_refresh_cooldown(
                    schedule
                        .as_ref()
                        .unwrap()
                        .borrow()
                        .cooldown_seconds(Instant::now()),
                );
            }
        }
        if enabled && !was_enabled {
            next_refresh = Instant::now();
        }
        was_enabled = enabled;
        let due = schedule
            .as_ref()
            .map(|s| s.borrow().due(Instant::now(), false, enabled, working))
            .unwrap_or(enabled && Instant::now() >= next_refresh);
        if !busy.get() && due && request(false) {
            if let Some(settings) = settings.upgrade() {
                set_usage_status(&settings, index, None, true);
            }
        }
    });
    timer
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

fn persist_entry_preferences() -> bool {
    #[cfg(debug_assertions)]
    if std::env::var_os("WACU_QA_VIEW").is_some() {
        return std::env::var_os("WACU_QA_ENTRY_SESSION").is_some();
    }
    true
}

fn remember_entry_view(prefs: &Rc<RefCell<Preferences>>, entry_view: EntryView) {
    let mut value = prefs.borrow_mut();
    if value.entry_view == entry_view {
        return;
    }
    value.entry_view = entry_view;
    if persist_entry_preferences() {
        if let Err(error) = state::save(&value) {
            eprintln!("Saving entry view: {error}");
        }
    }
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
    if !persist_entry_preferences() {
        return;
    }
    if let Err(error) = state::save(&value) {
        eprintln!("Saving position: {error}");
    }
}

// Mini and capsule have *independent* desktop positions. Switching
// visibility must never derive or overwrite one from the other's location.
fn default_mini_position(size: (i32, i32), area: placement::WorkArea, left: bool) -> Point {
    constrain(
        Point {
            x: if left {
                area.left + 15
            } else {
                area.right - size.0 - 15
            },
            y: area.bottom - size.1 - 15,
        },
        size,
        area,
    )
}

fn capsule_position(prefs: &Preferences, size: (i32, i32), default_anchor: Point) -> Point {
    if let Some((x, y)) = prefs.saved_position {
        let area = platform_windows::work_area_at(Point {
            x: x + size.0 / 2,
            y: y + size.1 / 2,
        });
        constrain(Point { x, y }, size, area)
    } else {
        pinned(size, platform_windows::work_area_at(default_anchor))
    }
}

fn remember_mini(mini: &MiniWindow, prefs: &Rc<RefCell<Preferences>>) {
    let pos = mini.window().position();
    let mut value = prefs.borrow_mut();
    if value.mini_position == Some((pos.x, pos.y)) {
        return;
    }
    value.mini_position = Some((pos.x, pos.y));
    if !persist_entry_preferences() {
        return;
    }
    if let Err(error) = state::save(&value) {
        eprintln!("Saving Mini position: {error}");
    }
}

// One generation covers both windows, so a reversed handoff cannot be hidden
// or re-revealed by timers left over from the previous direction.
#[derive(Clone)]
struct EntryMotion {
    generation: Rc<Cell<u64>>,
    active: Rc<Cell<bool>>,
    progress: Rc<Cell<f64>>,
    geometry: Rc<RefCell<Option<flow::Geometry>>>,
    layer: Rc<LineFlowWindow>,
}

impl EntryMotion {
    fn new() -> Result<Self, slint::PlatformError> {
        Ok(Self {
            generation: Rc::new(Cell::new(0)),
            active: Rc::new(Cell::new(false)),
            progress: Rc::new(Cell::new(0.0)),
            geometry: Rc::new(RefCell::new(None)),
            layer: Rc::new(LineFlowWindow::new()?),
        })
    }

    fn next(&self) -> u64 {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.active.set(false);
        let _ = self.layer.hide();
        self.layer.set_strokes(ModelRc::default());
        *self.geometry.borrow_mut() = None;
        generation
    }

    fn settle(&self, mini: &MiniWindow, capsule: &CapsuleWindow, open: bool) {
        self.next();
        let mini_animated = mini.get_animated();
        let capsule_animated = capsule.get_animated();
        mini.set_animated(false);
        capsule.set_animated(false);
        self.progress.set(if open { 1.0 } else { 0.0 });
        mini.set_flow_reveal(if open { 0.0 } else { 1.0 });
        capsule.set_flow_progress(if open { 1.0 } else { 0.0 });
        capsule.set_flow_reveal(if open { 1.0 } else { 0.0 });
        capsule.set_flow_visibility(if open { 1.0 } else { 0.0 });
        mini.set_revealed(!open);
        capsule.set_revealed(open);
        mini.set_flow_active(false);
        mini.set_external_rings(false);
        mini.set_rim_turn(0.0);
        mini.set_spin_highlight(0.0);
        capsule.set_flow_controlled(false);
        capsule.set_external_rings(false);
        capsule.set_flow_strokes(ModelRc::default());
        // Evaluate the terminal bindings with zero duration before restoring
        // animation: releasing external control must not start another tween
        // from the last sampled frame to the resting value.
        let _ = mini.get_reveal_progress();
        let _ = capsule.get_reveal_progress();
        let _ = capsule.get_ring_progress();
        mini.set_animated(mini_animated);
        capsule.set_animated(capsule_animated);
        if open {
            let _ = mini.hide();
            let _ = capsule.show();
        } else {
            let _ = capsule.hide();
            let _ = mini.show();
        }
    }

    fn frame(&self, mini: &MiniWindow, capsule: &CapsuleWindow, progress: f64) {
        self.progress.set(progress);
        let geometry = self.geometry.borrow().expect("Active flow geometry");
        if progress <= 0.0 {
            mini.set_flow_reveal(1.0);
            mini.set_external_rings(false);
            mini.set_rim_turn(flow::spin_turn(-progress) as f32);
            mini.set_spin_highlight(
                (std::f64::consts::PI * (-progress).clamp(0.0, 1.0)).sin() as f32 * 0.65,
            );
            capsule.set_flow_progress(0.0);
            capsule.set_flow_reveal(0.0);
            capsule.set_flow_visibility(0.0);
            capsule.set_flow_strokes(ModelRc::default());
            self.layer.set_strokes(ModelRc::default());
            let _ = self.layer.hide();
            if !capsule.get_revealed() {
                let _ = capsule.hide();
            }
            return;
        }
        mini.set_external_rings(true);
        mini.set_rim_turn(0.0);
        mini.set_spin_highlight(0.0);
        let rail = geometry.rail(progress);
        mini.set_flow_reveal(geometry.mini_reveal(progress) as f32);
        capsule.set_flow_progress(rail as f32);
        capsule.set_flow_reveal(flow::smooth(rail / 0.43) as f32);
        capsule.set_flow_visibility(geometry.visibility(progress) as f32);
        if geometry.mini_reveal(progress) == 0.0 && capsule.get_revealed() {
            let _ = mini.hide();
        }
        let services = capsule.get_services();
        let arrival = if geometry.spatial() {
            (330.0 + services.row_count().saturating_sub(1) as f64 * 18.0) / 720.0
        } else {
            0.10
        };
        let inside = progress >= arrival;
        if services.row_count() > 0 && !self.layer.window().is_visible() {
            self.show_layer(geometry);
        }
        let strokes = (0..services.row_count())
            .filter_map(|index| {
                let coil = geometry.coil(services.row_count(), index, progress);
                let strength = geometry.opacity(
                    index,
                    progress,
                    services.row_data(index).unwrap().percent as f64,
                ) * if inside {
                    1.0
                } else {
                    (1.0 + (capsule.get_surface_opacity() as f64 - 1.0)
                        * flow::smooth(progress / arrival)) as f32
                };
                if strength <= 0.001 {
                    return None;
                }
                use std::fmt::Write;
                let (x, y, scale) = if inside {
                    let p = capsule.window().position();
                    (p.x, p.y, capsule.window().scale_factor() as f64)
                } else {
                    let (x, y, _, _) = geometry.bounds();
                    (x, y, self.layer.window().scale_factor() as f64)
                };
                let mut path = String::new();
                for (i, point) in geometry
                    .points(
                        services.row_count(),
                        index,
                        progress,
                        services.row_data(index).unwrap().percent as f64,
                    )
                    .iter()
                    .enumerate()
                {
                    let _ = write!(
                        path,
                        "{} {:.2} {:.2} ",
                        if i == 0 { "M" } else { "L" },
                        (point.x - x as f64) / scale,
                        (point.y - y as f64) / scale
                    );
                }
                if coil == 1.0 {
                    path = geometry.terminal_path(
                        index,
                        services.row_data(index).unwrap().percent as f64,
                        flow::Vector {
                            x: x as f64,
                            y: y as f64,
                        },
                        scale,
                    );
                }
                Some(FlowStroke {
                    path: path.into(),
                    accent: services.row_data(index).unwrap().accent,
                    strength,
                    width: (geometry.width(services.row_count(), index, progress) / scale) as f32,
                    halo: (0.14 * (1.0 - coil)) as f32,
                })
            })
            .collect::<Vec<_>>();
        let strokes = ModelRc::new(VecModel::from(strokes));
        if inside {
            capsule.set_flow_strokes(strokes);
            self.layer.set_strokes(ModelRc::default());
        } else {
            self.layer.set_strokes(strokes);
            capsule.set_flow_strokes(ModelRc::default());
        }
    }

    fn show_layer(&self, geometry: flow::Geometry) {
        let (x, y, width, height) = geometry.bounds();
        let _ = self.layer.show();
        self.layer
            .window()
            .set_position(PhysicalPosition::new(x, y));
        let scale = self.layer.window().scale_factor();
        self.layer.set_canvas_width(width as f32 / scale);
        self.layer.set_canvas_height(height as f32 / scale);
        self.layer
            .window()
            .set_size(slint::PhysicalSize::new(width, height));
        platform_windows::configure_flow(self.layer.window());
    }

    fn run(&self, mini: &MiniWindow, capsule: &CapsuleWindow, open: bool) {
        let from = if self.active.get() {
            self.progress.get()
        } else if open {
            0.0
        } else {
            1.0
        };
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.active.set(true);
        let mini_pos = mini.window().position();
        let mini_size = mini.window().size();
        let cap_pos = capsule.window().position();
        let cap_size = capsule.window().size();
        let geometry = flow::Geometry {
            source: flow::Vector {
                x: mini_pos.x as f64 + mini_size.width as f64 / 2.0,
                y: mini_pos.y as f64 + mini_size.height as f64 / 2.0,
            },
            logo: flow::Vector {
                x: cap_pos.x as f64 + cap_size.width as f64 / 2.0,
                y: cap_pos.y as f64
                    + capsule.get_hero_y() as f64 * capsule.window().scale_factor() as f64,
            },
            source_scale: mini.window().scale_factor() as f64,
            target_scale: capsule.window().scale_factor() as f64,
            quota_offset: capsule.invoke_ring_offset(0) as f64
                * capsule.window().scale_factor() as f64,
            quota_step: capsule.get_item_height() as f64 * capsule.window().scale_factor() as f64,
            quota_diameter: capsule.get_ring_diameter() as f64
                * capsule.window().scale_factor() as f64,
            quota_width: capsule.get_quota_stroke_width() as f64
                * capsule.window().scale_factor() as f64,
            capsule_width: cap_size.width as f64,
            bottom: cap_pos.y as f64 + cap_size.height as f64,
        };
        *self.geometry.borrow_mut() = Some(geometry);
        mini.set_flow_active(true);
        mini.set_external_rings(true);
        capsule.set_flow_controlled(true);
        capsule.set_external_rings(true);
        mini.set_revealed(!open);
        capsule.set_revealed(open);
        self.frame(mini, capsule, from);
        let to = if open {
            1.0
        } else if capsule.get_services().row_count() == 0 {
            0.0
        } else {
            -1.0
        };
        let from_time = flow::motion_time_ms(geometry.spatial(), open, from);
        let to_time = flow::motion_time_ms(geometry.spatial(), open, to);
        let duration = Duration::from_secs_f64((to_time - from_time).abs() / 1000.0);
        fn step(
            motion: EntryMotion,
            mini: slint::Weak<MiniWindow>,
            capsule: slint::Weak<CapsuleWindow>,
            generation: u64,
            from: f64,
            to: f64,
            start: Instant,
            duration: Duration,
        ) {
            if motion.generation.get() != generation {
                return;
            }
            let (Some(m), Some(c)) = (mini.upgrade(), capsule.upgrade()) else {
                return;
            };
            let t = if duration.is_zero() {
                1.0
            } else {
                (start.elapsed().as_secs_f64() / duration.as_secs_f64()).min(1.0)
            };
            let spatial = motion.geometry.borrow().unwrap().spatial();
            let open = to == 1.0;
            let from_time = flow::motion_time_ms(spatial, open, from);
            let to_time = flow::motion_time_ms(spatial, open, to);
            let progress = if t >= 1.0 {
                to
            } else if t == 0.0 {
                from
            } else {
                flow::motion_progress(spatial, open, from_time + (to_time - from_time) * t)
            };
            motion.frame(&m, &c, progress);
            if t >= 1.0 {
                motion.settle(&m, &c, to == 1.0);
            } else {
                Timer::single_shot(Duration::from_millis(16), move || {
                    step(motion, mini, capsule, generation, from, to, start, duration)
                });
            }
        }
        step(
            self.clone(),
            mini.as_weak(),
            capsule.as_weak(),
            generation,
            from,
            to,
            Instant::now(),
            duration,
        );
    }
}

fn prepare_mini(mini: &MiniWindow, prefs: &Rc<RefCell<Preferences>>) {
    // Instantiate the HWND before reading its physical dimensions. Before the
    // first show(), Slint can report a zero size and place Mini off screen.
    mini.set_revealed(false);
    let _ = mini.show();
    let size = mini.window().size();
    let size = (size.width as i32, size.height as i32);
    let position = if let Some((x, y)) = prefs.borrow().mini_position {
        let area = platform_windows::work_area_at(Point { x, y });
        constrain(Point { x, y }, size, area)
    } else {
        default_mini_position(
            size,
            platform_windows::work_area_at(platform_windows::cursor()),
            prefs.borrow().dock_left,
        )
    };
    mini.window()
        .set_position(PhysicalPosition::new(position.x, position.y));
    remember_mini(mini, prefs); // Persist the initial Mini location once.
    platform_windows::configure_capsule(mini.window());
}

fn show_mini(mini: &MiniWindow, prefs: &Rc<RefCell<Preferences>>, motion: &EntryMotion) {
    let generation = motion.next();
    prepare_mini(mini, prefs);
    if !mini.get_animated() {
        platform_windows::configure_capsule(mini.window());
        mini.set_revealed(true);
        return;
    }
    let weak = mini.as_weak();
    let motion = motion.clone();
    Timer::single_shot(Duration::from_millis(25), move || {
        if motion.generation.get() == generation {
            if let Some(m) = weak.upgrade() {
                if m.window().is_visible() {
                    platform_windows::configure_capsule(m.window());
                    m.set_revealed(true);
                }
            }
        }
    });
}

fn expand_entry(
    mini: &MiniWindow,
    capsule: &CapsuleWindow,
    prefs: &Rc<RefCell<Preferences>>,
    motion: &EntryMotion,
) {
    let pos = mini.window().position();
    remember_mini(mini, prefs);
    remember_entry_view(prefs, EntryView::Capsule);
    // Keep a new target blank until the shared timeline has been installed.
    if !motion.active.get() {
        capsule.set_flow_controlled(true);
        capsule.set_flow_progress(0.0);
        capsule.set_flow_reveal(0.0);
        capsule.set_flow_visibility(0.0);
    }
    let _ = capsule.show();
    let target = capsule_position(
        &prefs.borrow(),
        capsule_dimensions(capsule),
        Point { x: pos.x, y: pos.y },
    );
    capsule
        .window()
        .set_position(PhysicalPosition::new(target.x, target.y));
    remember_position(capsule, prefs, None);
    platform_windows::configure_capsule(capsule.window());
    if !capsule.get_animated() {
        motion.settle(mini, capsule, true);
        return;
    }
    motion.run(mini, capsule, true);
}

fn collapse_entry(
    mini: &MiniWindow,
    capsule: &CapsuleWindow,
    prefs: &Rc<RefCell<Preferences>>,
    motion: &EntryMotion,
) {
    remember_position(capsule, prefs, None);
    remember_entry_view(prefs, EntryView::Mini);
    prepare_mini(mini, prefs);
    if !capsule.get_animated() {
        motion.settle(mini, capsule, false);
        return;
    }
    motion.run(mini, capsule, false);
}

fn show_entry_menu(menu: &EntryMenuWindow, owner: &slint::Window, x: f32, y: f32, animated: bool) {
    let scale = owner.scale_factor();
    let origin = owner.position();
    let anchor = Point {
        x: origin.x + (x * scale).round() as i32,
        y: origin.y + (y * scale).round() as i32,
    };
    let area = platform_windows::work_area_at(anchor);
    let target = constrain(
        Point {
            x: anchor.x - (6.0 * scale).round() as i32,
            y: anchor.y - (6.0 * scale).round() as i32,
        },
        (
            (128.0 * scale).round() as i32,
            (84.0 * scale).round() as i32,
        ),
        area,
    );
    menu.set_selected(-1);
    menu.set_animated(animated);
    menu.window()
        .set_position(PhysicalPosition::new(target.x, target.y));
    let _ = menu.show();
    let size = menu.window().size();
    let target = constrain(target, (size.width as i32, size.height as i32), area);
    menu.window()
        .set_position(PhysicalPosition::new(target.x, target.y));
    platform_windows::configure_capsule(menu.window());
    platform_windows::activate(menu.window());
    menu.invoke_focus_menu();
}

fn restore_entry(
    mini: &MiniWindow,
    capsule: &CapsuleWindow,
    prefs: &Rc<RefCell<Preferences>>,
    motion: &EntryMotion,
) {
    if prefs.borrow().entry_view == EntryView::Mini {
        show_mini(mini, prefs, motion);
        return;
    }
    capsule.set_revealed(false);
    let _ = capsule.show();
    let target = capsule_position(
        &prefs.borrow(),
        capsule_dimensions(capsule),
        platform_windows::cursor(),
    );
    capsule
        .window()
        .set_position(PhysicalPosition::new(target.x, target.y));
    remember_position(capsule, prefs, None);
    let weak = capsule.as_weak();
    Timer::single_shot(Duration::from_millis(16), move || {
        if let Some(c) = weak.upgrade() {
            platform_windows::configure_capsule(c.window());
            platform_windows::activate(c.window());
            c.set_revealed(true);
        }
    });
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
    desktop_entry: bool,
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
    if desktop_entry {
        return;
    }
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
    let dest = constrain(
        Point { x, y: cp.y },
        (ds.width as i32, ds.height as i32),
        area,
    );
    detail
        .window()
        .set_position(PhysicalPosition::new(dest.x, dest.y));
}

const DETAIL_FADE: Duration = Duration::from_millis(175);

#[derive(Clone, Default)]
struct DetailMotion {
    generation: Rc<Cell<u64>>,
    closing: Rc<Cell<bool>>,
}

impl DetailMotion {
    fn next(&self) -> u64 {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        generation
    }
}

// Drive the actual panel opacity/position properties, not just its child controls.
// The first frame must be drawn hidden before changing the revealed property.
fn reveal_detail_window(detail: &DetailWindow, motion: &DetailMotion) {
    let generation = motion.next();
    motion.closing.set(false);
    if detail.window().is_visible() {
        detail.set_revealed(true);
        return;
    }
    detail.set_revealed(false);
    let _ = detail.show();
    if !detail.get_animated() {
        detail.set_revealed(true);
        return;
    }
    let weak = detail.as_weak();
    let motion = motion.clone();
    Timer::single_shot(Duration::from_millis(20), move || {
        if motion.generation.get() == generation {
            if let Some(detail) = weak.upgrade() {
                if detail.window().is_visible() {
                    detail.set_revealed(true);
                }
            }
        }
    });
}

fn dismiss_detail(detail: &DetailWindow, motion: &DetailMotion) {
    if !detail.window().is_visible() || motion.closing.get() {
        return;
    }
    let generation = motion.next();
    motion.closing.set(true);
    detail.set_revealed(false);
    if !detail.get_animated() {
        let _ = detail.hide();
        motion.closing.set(false);
        return;
    }
    let weak = detail.as_weak();
    let motion = motion.clone();
    Timer::single_shot(DETAIL_FADE + Duration::from_millis(10), move || {
        if motion.generation.get() == generation {
            if let Some(detail) = weak.upgrade() {
                let _ = detail.hide();
            }
            motion.closing.set(false);
        }
    });
}

// Native Windows dragging blocks normal Slint timers. Keep the established
// immediate hide on mouse-down; after mouse-up, re-anchor and fade back in.
fn restore_detail_after_drag(
    detail: &DetailWindow,
    capsule: &CapsuleWindow,
    motion: &DetailMotion,
) {
    position_detail(detail, capsule);
    reveal_detail_window(detail, motion);
}

fn show_detail(
    detail: &DetailWindow,
    capsule: &CapsuleWindow,
    index: usize,
    prefs: &Preferences,
    motion: &DetailMotion,
) {
    if !prefs.shown[index] {
        return;
    }
    detail.set_selected_service(service(index, true));
    detail.set_animated(prefs.animations_enabled);
    capsule.set_selected_index(index as i32);
    detail.set_expanded(false);
    detail.set_selected_period(0);
    reveal_detail_window(detail, motion);
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
fn qa_detail_opacity(window: &slint::Window) -> u8 {
    let snapshot = window
        .take_snapshot()
        .expect("Cannot capture animated detail");
    let width = snapshot.width() as usize;
    let height = snapshot.height() as usize;
    // Transparent root window, with an opaque center panel at full reveal.
    let alpha_offset = ((height / 2) * width + width / 2) * 4 + 3;
    snapshot.as_bytes()[alpha_offset]
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

#[cfg(debug_assertions)]
thread_local! {
    // Encode after the animation: PNG compression must not delay live samples.
    static ENTRY_FRAMES: RefCell<Vec<(String, slint::SharedPixelBuffer<slint::Rgba8Pixel>)>> = const { RefCell::new(Vec::new()) };
}

#[cfg(debug_assertions)]
fn qa_write_entry_frame(frame: &slint::SharedPixelBuffer<slint::Rgba8Pixel>, name: &str) {
    let mut encoder = png::Encoder::new(
        std::fs::File::create(format!("target/qa-entry-{name}.png")).unwrap(),
        frame.width(),
        frame.height(),
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(frame.as_bytes())
        .unwrap();
}

#[cfg(debug_assertions)]
fn qa_entry_frame(window: &slint::Window, name: &str) -> usize {
    let frame = window.take_snapshot().expect("Entry animation snapshot");
    if std::env::var_os("WACU_QA_ENTRY_MOTION").is_some()
        || std::env::var_os("WACU_QA_ENTRY_HANDOFF").is_some()
    {
        ENTRY_FRAMES.with(|frames| frames.borrow_mut().push((name.into(), frame.clone())));
    } else {
        qa_write_entry_frame(&frame, name);
    }
    // Count the opaque silhouette, excluding transient glow and shadow pixels.
    let width = frame.width() as usize;
    (0..frame.height() as usize)
        .filter(|y| frame.as_bytes()[(*y * width + width / 2) * 4 + 3] > 180)
        .count()
}

#[cfg(debug_assertions)]
fn qa_entry_animation(
    mini: &MiniWindow,
    capsule: &CapsuleWindow,
    settings: &SettingsWindow,
    prefs: &Rc<RefCell<Preferences>>,
) {
    // Warm the renderer before measuring the resting silhouette. A hidden or
    // occluded Windows test surface may otherwise render its first frame only
    // when take_snapshot() is first called, starting from the reveal's zero.
    let warm = mini.as_weak();
    Timer::single_shot(Duration::from_millis(150), move || {
        if let Some(mini) = warm.upgrade() {
            // This suite measures handoffs, not the initial app launch. Finish
            // that reveal explicitly so an occluded startup cannot leave the
            // first GPU readback at the zero-opacity frame.
            mini.set_animated(false);
            mini.set_revealed(false);
            mini.set_revealed(true);
            platform_windows::activate(mini.window());
            let _ = mini.window().take_snapshot();
            mini.set_animated(true);
        }
    });
    let m = mini.as_weak();
    let c = capsule.as_weak();
    let s = settings.as_weak();
    let prefs = prefs.clone();
    Timer::single_shot(Duration::from_millis(400), move || {
        let mini = m.upgrade().unwrap();
        let capsule = c.upgrade().unwrap();
        // Measure the local ring timeline here. Spatial flight is checked by
        // the separate handoff suite with distant saved entry positions.
        let area = platform_windows::work_area_at(platform_windows::cursor());
        let scale = capsule.window().scale_factor();
        let cap_pos = Point {
            x: area.left + 150,
            y: area.top + 100,
        };
        let mini_pos = Point {
            x: cap_pos.x + capsule.window().size().width as i32 / 2
                - mini.window().size().width as i32 / 2,
            y: cap_pos.y + (capsule.get_hero_y() * scale).round() as i32
                - mini.window().size().height as i32 / 2,
        };
        mini.window()
            .set_position(PhysicalPosition::new(mini_pos.x, mini_pos.y));
        prefs.borrow_mut().saved_position = Some((cap_pos.x, cap_pos.y));
        let settled_mini = qa_entry_frame(mini.window(), "mini-rest");
        assert!(
            settled_mini > 0,
            "Mini baseline must contain visible pixels"
        );
        let mini_position = mini.window().position();
        let capsule_size = capsule.window().size();
        mini.invoke_toggle_expanded();
        assert!(capsule.window().is_visible());
        assert!(
            mini.window().is_visible(),
            "Exit must render before hiding Mini"
        );
        let heights = Rc::new(RefCell::new(Vec::new()));
        for delay in [40, 60, 100, 120, 160, 200, 260, 300, 340, 380, 420] {
            let c = c.clone();
            let m = m.clone();
            Timer::single_shot(Duration::from_millis(delay), move || {
                qa_entry_frame(c.upgrade().unwrap().window(), &format!("open-fx-{delay}"));
                if delay <= 100 {
                    qa_entry_frame(m.upgrade().unwrap().window(), &format!("mini-exit-{delay}"));
                }
            });
        }
        let open_rest = flow::duration_ms(false, true) + 110;
        for (delay, name) in [
            (30, "open-early"),
            (300, "open-mid"),
            (open_rest, "open-rest"),
        ] {
            let c = c.clone();
            let m = m.clone();
            let s = s.clone();
            let heights = heights.clone();
            Timer::single_shot(Duration::from_millis(delay), move || {
                let capsule = c.upgrade().unwrap();
                // Read the phase before GPU readback / PNG encoding. A busy
                // desktop may deliver a sampling timer after the rings settle.
                if delay == 300 && capsule.get_visible_count() > 1 {
                    let progress = capsule.get_ring_progress();
                    let first = capsule.invoke_ring_phase(0);
                    let last = capsule.invoke_ring_phase(capsule.get_visible_count() - 1);
                    assert!(first >= last, "Quota rings must follow their visible order");
                    if progress > 0.52 && progress < 0.86 {
                        assert!(first > last, "Active ring stages must be staggered");
                    }
                }
                let scale = capsule.window().scale_factor();
                assert!(
                    capsule.get_shell_height() >= 56.0
                        && capsule.get_shell_height()
                            <= capsule.window().size().height as f32 / scale - 24.0 + 0.5
                );
                for index in 0..capsule.get_visible_count() {
                    let spread = capsule.invoke_spread_phase(index);
                    assert!(
                        (0.0..=1.04).contains(&spread),
                        "Spring motion must remain within the window margins"
                    );
                }
                heights
                    .borrow_mut()
                    .push(qa_entry_frame(capsule.window(), name));
                assert_eq!(
                    capsule.window().size(),
                    capsule_size,
                    "Do not resize the HWND"
                );
                // Sample late enough for the brief expanding pulse to fade;
                // its outer ring can initially exceed the resting silhouette.
                if delay == 300 {
                    let exiting = qa_entry_frame(m.upgrade().unwrap().window(), "mini-exit");
                    assert!(
                        exiting < settled_mini,
                        "Mini must shrink in rendered pixels"
                    );
                }
                if delay == open_rest {
                    assert_eq!(
                        capsule.get_reveal_progress(),
                        1.0,
                        "Opening must finish within its assembly duration"
                    );
                    assert!(!m.upgrade().unwrap().window().is_visible());
                    let h = heights.borrow();
                    assert!(
                        h[0] < h[2] && h[0] <= h[1] && h[1] <= h[2],
                        "Shell must unfold in rendered frames: {h:?}"
                    );
                    eprintln!("WACU_QA entry_unfold_pixels_pass heights={h:?}");
                    let full_height = h[2];
                    Timer::single_shot(Duration::from_millis(60), move || {
                        let capsule = c.upgrade().unwrap();
                        qa_entry_frame(capsule.window(), "open-settled");
                        assert_eq!(
                            capsule.get_ring_progress(),
                            1.0,
                            "Quota rings must settle without an idle loop"
                        );
                        for index in 0..capsule.get_visible_count() {
                            assert_eq!(capsule.invoke_ring_phase(index), 1.0);
                        }
                        qa_entry_close(
                            &m.upgrade().unwrap(),
                            &capsule,
                            &s.upgrade().unwrap(),
                            mini_position,
                            full_height,
                        );
                    });
                }
            });
        }
    });
}

#[cfg(debug_assertions)]
fn qa_entry_close(
    mini: &MiniWindow,
    capsule: &CapsuleWindow,
    settings: &SettingsWindow,
    mini_position: PhysicalPosition,
    full_height: usize,
) {
    let m = mini.as_weak();
    let c = capsule.as_weak();
    let s = settings.as_weak();
    capsule.invoke_collapse_to_mini();
    assert!(
        !capsule.get_revealed(),
        "Collapsing must begin exit animation"
    );
    let heights = Rc::new(RefCell::new(vec![full_height]));
    for delay in [20, 50, 110, 130, 170, 230, 290] {
        let c = c.clone();
        let m = m.clone();
        Timer::single_shot(Duration::from_millis(delay), move || {
            qa_entry_frame(c.upgrade().unwrap().window(), &format!("close-fx-{delay}"));
            qa_entry_frame(
                m.upgrade().unwrap().window(),
                &format!("mini-return-{delay}"),
            );
        });
    }
    // Schedule against the actual collapse, so a busy desktop cannot make the
    // completion assertion run before a delayed click's hide timer.
    let close_mid = (flow::base_ms(false, false) as f64
        * (flow::time_position(false, 1.0) - flow::time_position(false, 0.50)))
    .ceil() as u64;
    for (delay, name) in [(30, "close-early"), (close_mid, "close-mid")] {
        let c = c.clone();
        let m = m.clone();
        let heights = heights.clone();
        Timer::single_shot(Duration::from_millis(delay), move || {
            heights
                .borrow_mut()
                .push(qa_entry_frame(c.upgrade().unwrap().window(), name));
            qa_entry_frame(
                m.upgrade().unwrap().window(),
                if delay == 30 {
                    "mini-return-early"
                } else {
                    "mini-return-mid"
                },
            );
        });
    }
    Timer::single_shot(
        Duration::from_millis(flow::duration_ms(false, false) + 100),
        move || {
            let mini = m.upgrade().unwrap();
            assert!(mini.window().is_visible());
            assert!(!c.upgrade().unwrap().window().is_visible());
            assert_eq!(mini.window().position(), mini_position);
            let h = heights.borrow();
            assert!(
                h[0] > h[2] && h[0] >= h[1] && h[1] >= h[2],
                "Shell must fold in rendered frames: {h:?}"
            );
            qa_entry_frame(mini.window(), "mini-return-rest");
            // Reverse both directions before pending show/hide timers finish.
            for (delay, expand) in [(0, true), (10, false), (20, true), (50, false), (80, true)] {
                let m = m.clone();
                let c = c.clone();
                Timer::single_shot(Duration::from_millis(delay), move || {
                    if expand {
                        m.upgrade().unwrap().invoke_toggle_expanded();
                    } else {
                        c.upgrade().unwrap().invoke_collapse_to_mini();
                    }
                });
            }
            Timer::single_shot(
                Duration::from_millis(flow::duration_ms(false, true) + 170),
                move || {
                    let mini = m.upgrade().unwrap();
                    let capsule = c.upgrade().unwrap();
                    let settings = s.upgrade().unwrap();
                    assert!(capsule.window().is_visible() && capsule.get_revealed());
                    assert!(!mini.window().is_visible());
                    settings.invoke_set_animations(false);
                    capsule.invoke_collapse_to_mini();
                    assert!(!capsule.window().is_visible());
                    assert!(mini.window().is_visible() && mini.get_revealed());
                    assert_eq!(mini.get_reveal_progress(), 1.0);
                    assert_eq!(capsule.get_ring_progress(), 0.0);
                    mini.invoke_toggle_expanded();
                    assert!(!mini.window().is_visible());
                    assert!(capsule.window().is_visible() && capsule.get_revealed());
                    assert_eq!(capsule.get_reveal_progress(), 1.0);
                    assert_eq!(capsule.get_ring_progress(), 1.0);
                    capsule.invoke_collapse_to_mini();
                    settings.invoke_set_animations(true);
                    eprintln!("WACU_QA entry_reversal_disabled_pass");
                    let _ = slint::quit_event_loop();
                },
            );
        },
    );
}

#[cfg(debug_assertions)]
fn qa_handoff_scene(
    mini: &MiniWindow,
    capsule: &CapsuleWindow,
    motion: &EntryMotion,
    start: Instant,
    scenes: &Rc<RefCell<Vec<serde_json::Value>>>,
) {
    let index = scenes.borrow().len();
    let mut windows = Vec::new();
    for (label, window) in [
        ("mini", mini.window()),
        ("capsule", capsule.window()),
        ("flow", motion.layer.window()),
    ] {
        if !window.is_visible() {
            continue;
        }
        assert!(
            platform_windows::utility_window_is_taskbar_excluded(window),
            "{label} must never become eligible for a taskbar button"
        );
        if label == "flow" {
            assert!(platform_windows::flow_is_passthrough(window));
            let geometry = motion.geometry.borrow().unwrap();
            let (x, y, width, height) = geometry.bounds();
            assert_eq!(window.position(), PhysicalPosition::new(x, y));
            assert_eq!(window.size(), slint::PhysicalSize::new(width, height));
            assert!(motion.layer.get_strokes().row_count() <= capsule.get_services().row_count());
            if motion.progress.get() > 0.12 {
                assert_eq!(
                    motion.layer.get_strokes().row_count() + capsule.get_flow_strokes().row_count(),
                    capsule.get_services().row_count(),
                    "Every line must remain visible until it becomes its quota arc"
                );
                for i in 0..motion.layer.get_strokes().row_count() {
                    assert!(
                        motion.layer.get_strokes().row_data(i).unwrap().strength
                            >= capsule.get_surface_opacity() - 0.001
                    );
                }
            }
        }
        let name = format!("handoff-{index:03}-{label}");
        let position = window.position();
        qa_entry_frame(window, &name);
        if label == "flow" && std::env::var_os("WACU_QA_FLOW_DESKTOP").is_some() && index == 10 {
            let geometry = motion.geometry.borrow().unwrap();
            let points: Vec<_> = (0..capsule.get_visible_count() as usize)
                .map(|i| {
                    geometry.points(
                        capsule.get_visible_count() as usize,
                        i,
                        motion.progress.get(),
                        capsule.get_services().row_data(i).unwrap().percent as f64,
                    )[32]
                })
                .collect();
            eprintln!(
                "WACU_QA desktop flow p={:.3} colors={:x?}",
                motion.progress.get(),
                platform_windows::desktop_colors(&points)
            );
        }
        windows.push(serde_json::json!({"name": name, "x": position.x, "y": position.y}));
    }
    scenes
        .borrow_mut()
        .push(serde_json::json!({"time":start.elapsed().as_millis(),"progress":motion.progress.get(),"active":motion.active.get(),"mini_turn":mini.get_rim_turn(),"mini_ids":(0..mini.get_services().row_count()).map(|i| mini.get_services().row_data(i).unwrap().source_index).collect::<Vec<_>>(),"mini_percent":(0..mini.get_services().row_count()).map(|i| mini.get_services().row_data(i).unwrap().percent).collect::<Vec<_>>(),"windows":windows}));
}

#[cfg(debug_assertions)]
fn qa_entry_handoff(
    mini: &MiniWindow,
    capsule: &CapsuleWindow,
    settings: &SettingsWindow,
    prefs: &Rc<RefCell<Preferences>>,
    motion: &EntryMotion,
) {
    let m = mini.as_weak();
    let c = capsule.as_weak();
    let s = settings.as_weak();
    let prefs = prefs.clone();
    let motion = motion.clone();
    Timer::single_shot(Duration::from_millis(350), move || {
        let mini = m.upgrade().unwrap();
        let capsule = c.upgrade().unwrap();
        let direction = std::env::var("WACU_QA_ENTRY_HANDOFF")
            .unwrap()
            .parse::<i32>()
            .unwrap();
        let area = platform_windows::work_area_at(platform_windows::cursor());
        let cap_pos = Point {
            x: area.left + if direction == 2 { 100 } else { 460 },
            y: area.top + 80,
        };
        let mini_pos = if direction == 0 {
            Point {
                x: cap_pos.x + capsule.window().size().width as i32 / 2
                    - mini.window().size().width as i32 / 2,
                y: cap_pos.y
                    + (capsule.get_hero_y() * capsule.window().scale_factor()).round() as i32
                    - mini.window().size().height as i32 / 2,
            }
        } else {
            Point {
                x: area.left + if direction == 2 { 460 } else { 100 },
                y: area.top + 280,
            }
        };
        mini.window()
            .set_position(PhysicalPosition::new(mini_pos.x, mini_pos.y));
        mini.set_animated(false);
        mini.set_revealed(true);
        let _ = mini.get_reveal_progress();
        mini.set_animated(true);
        {
            let mut value = prefs.borrow_mut();
            value.saved_position = Some((cap_pos.x, cap_pos.y));
            value.mini_position = Some((mini_pos.x, mini_pos.y));
        }
        let start = Instant::now();
        let scenes = Rc::new(RefCell::new(Vec::new()));
        qa_handoff_scene(&mini, &capsule, &motion, start, &scenes);
        mini.invoke_toggle_expanded();
        assert_eq!(
            capsule.get_services().row_count(),
            mini.get_services().row_count()
        );
        assert_eq!(
            capsule.get_visible_count() as usize,
            mini.get_services().row_count()
        );
        assert_eq!(
            capsule.get_visible_count(),
            std::env::var("WACU_QA_COUNT")
                .unwrap()
                .parse::<i32>()
                .unwrap()
        );
        assert!(
            capsule.get_revealed(),
            "Logo handoff must begin immediately"
        );
        assert!(platform_windows::utility_window_is_taskbar_excluded(
            mini.window()
        ));
        assert!(platform_windows::utility_window_is_taskbar_excluded(
            capsule.window()
        ));
        let open_rest = flow::duration_ms(direction != 0, true) + 180;
        let restart = open_rest + flow::duration_ms(direction != 0, false) + 180;
        let reversed_rest = flow::duration_ms(direction != 0, true) + 220;
        for delay in (30..=restart - 60).step_by(30) {
            let m = m.clone();
            let c = c.clone();
            let motion = motion.clone();
            let scenes = scenes.clone();
            Timer::single_shot(Duration::from_millis(delay), move || {
                assert_eq!(
                    m.upgrade().unwrap().window().position(),
                    PhysicalPosition::new(mini_pos.x, mini_pos.y)
                );
                assert_eq!(
                    c.upgrade().unwrap().window().position(),
                    PhysicalPosition::new(cap_pos.x, cap_pos.y)
                );
                qa_handoff_scene(
                    &m.upgrade().unwrap(),
                    &c.upgrade().unwrap(),
                    &motion,
                    start,
                    &scenes,
                )
            });
        }
        let cap = c.clone();
        let pref = prefs.clone();
        Timer::single_shot(Duration::from_millis(open_rest), move || {
            let capsule = cap.upgrade().unwrap();
            assert!(capsule.get_revealed() && capsule.get_ring_progress() == 1.0);
            assert_eq!(
                capsule.window().position(),
                PhysicalPosition::new(cap_pos.x, cap_pos.y)
            );
            assert_eq!(pref.borrow().mini_position, Some((mini_pos.x, mini_pos.y)));
            capsule.invoke_collapse_to_mini();
        });
        let m2 = m.clone();
        let c2 = c.clone();
        let pref = prefs.clone();
        Timer::single_shot(Duration::from_millis(restart), move || {
            let mini = m2.upgrade().unwrap();
            assert!(mini.get_revealed() && mini.window().is_visible());
            assert_eq!(
                mini.window().position(),
                PhysicalPosition::new(mini_pos.x, mini_pos.y)
            );
            assert!(!c2.upgrade().unwrap().window().is_visible());
            assert_eq!(pref.borrow().mini_position, Some((mini_pos.x, mini_pos.y)));
            assert_eq!(pref.borrow().saved_position, Some((cap_pos.x, cap_pos.y)));
            std::fs::write(
                "target/qa-entry-handoff-scenes.json",
                serde_json::to_vec(&*scenes.borrow()).unwrap(),
            )
            .unwrap();
            mini.invoke_toggle_expanded();
            c2.upgrade().unwrap().invoke_collapse_to_mini();
            mini.invoke_toggle_expanded();
            // Schedule from the actual reverse click, since framebuffer readbacks
            // can delay the restart callback on a busy test desktop. Keep reading
            // the window so an occluded surface cannot retain an old frame phase.
            for delay in (200..reversed_rest).step_by(250) {
                let cap = c.clone();
                Timer::single_shot(Duration::from_millis(delay), move || {
                    qa_entry_frame(
                        cap.upgrade().unwrap().window(),
                        &format!("handoff-reverse-{delay}"),
                    );
                });
            }
            Timer::single_shot(Duration::from_millis(reversed_rest), move || {
                let mini = m.upgrade().unwrap();
                let capsule = c.upgrade().unwrap();
                assert!(!motion.active.get(), "Reversed motion clock must settle");
                qa_entry_frame(capsule.window(), "handoff-reversed-rest");
                assert!(
                    capsule.get_revealed() && capsule.get_ring_progress() == 1.0,
                    "Reversed relay must settle: revealed={} progress={}",
                    capsule.get_revealed(),
                    capsule.get_ring_progress()
                );
                s.upgrade().unwrap().invoke_set_animations(false);
                capsule.invoke_collapse_to_mini();
                assert!(mini.get_revealed() && !capsule.window().is_visible());
                mini.invoke_toggle_expanded();
                assert!(capsule.get_revealed() && !mini.window().is_visible());
                // Every settings combination, including a lone green or purple
                // service, must keep its original identity in all three surfaces.
                let settings = s.upgrade().unwrap();
                let original_count = capsule.get_visible_count();
                for mask in 0..8 {
                    let shown = std::array::from_fn(|index| mask & (1 << index) != 0);
                    for (index, enabled) in shown.iter().enumerate() {
                        settings.invoke_toggle_service(index as i32, *enabled);
                    }
                    let expected = visible_service_indices(&shown).collect::<Vec<_>>();
                    assert_eq!(capsule.get_services().row_count(), expected.len());
                    assert_eq!(mini.get_services().row_count(), expected.len());
                    for (row, id) in expected.iter().enumerate() {
                        assert_eq!(
                            capsule.get_services().row_data(row).unwrap().source_index,
                            *id as i32
                        );
                    }
                    qa_entry_frame(capsule.window(), &format!("handoff-selection-{mask}"));
                }
                let mut shown = [false; 3];
                for &index in [1, 0, 2].iter().take(original_count as usize) {
                    shown[index] = true;
                }
                for (index, enabled) in shown.iter().enumerate() {
                    settings.invoke_toggle_service(index as i32, *enabled);
                }
                settings.invoke_set_animations(true);
                capsule.invoke_collapse_to_mini();
                mini.invoke_toggle_expanded();
                settings.invoke_set_animations(false);
                assert!(!motion.active.get() && !motion.layer.window().is_visible());
                assert!(!mini.get_flow_active() && !capsule.get_flow_controlled());
                assert!(platform_windows::utility_window_is_taskbar_excluded(
                    capsule.window()
                ));
                let cap = capsule.as_weak();
                Timer::single_shot(Duration::from_millis(420), move || {
                    let capsule = cap.upgrade().unwrap();
                    assert!(capsule.window().is_visible() && capsule.get_revealed());
                    eprintln!(
                        "WACU_QA entry_handoff_pass direction={direction} count={original_count} independent_positions separated_routes logo_handoff click_through taskbar_excluded reversal disabled all_service_combinations"
                    );
                    let _ = slint::quit_event_loop();
                });
            });
        });
    });
}

#[cfg(debug_assertions)]
fn qa_context_command(window: &slint::Window, menu: &EntryMenuWindow, x: f32, y: f32, row: usize) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let position = slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Right,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Right,
    });
    assert!(
        menu.window().is_visible(),
        "Right click must display the complete menu window"
    );
    assert!(platform_windows::utility_window_is_taskbar_excluded(
        menu.window()
    ));
    let pos = menu.window().position();
    let size = menu.window().size();
    let area = platform_windows::work_area_at(Point { x: pos.x, y: pos.y });
    assert!(
        pos.x >= area.left
            && pos.y >= area.top
            && pos.x + size.width as i32 <= area.right
            && pos.y + size.height as i32 <= area.bottom,
        "Menu must fit completely in the screen work area"
    );
    use slint::platform::Key;
    if row == 1 || (row == 2 && x == 33.0) {
        qa_click(menu.window(), 64.0, if row == 1 { 25.0 } else { 58.0 });
        return;
    }
    let keys = if row == 0 {
        vec![Key::Escape]
    } else {
        std::iter::repeat_n(Key::DownArrow, row)
            .chain([Key::Return])
            .collect()
    };
    for key in keys {
        menu.window()
            .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
        menu.window()
            .dispatch_event(WindowEvent::KeyReleased { text: key.into() });
    }
}

#[cfg(debug_assertions)]
fn qa_menu_actions<C: ComponentHandle + 'static>(
    entry: &C,
    menu: &EntryMenuWindow,
    settings: &SettingsWindow,
    point: (f32, f32),
    exits: Rc<Cell<i32>>,
    done: impl FnOnce() + 'static,
) {
    qa_context_command(entry.window(), menu, point.0, point.1, 1);
    let entry = entry.as_weak();
    let menu = menu.as_weak();
    let settings = settings.as_weak();
    Timer::single_shot(Duration::from_millis(50), move || {
        let view = entry.upgrade().unwrap();
        let settings_view = settings.upgrade().unwrap();
        assert!(
            settings_view.window().is_visible(),
            "First menu item must open settings"
        );
        assert!(platform_windows::utility_window_is_taskbar_excluded(
            settings_view.window()
        ));
        let _ = settings_view.hide();
        let before = exits.get();
        qa_context_command(view.window(), &menu.upgrade().unwrap(), point.0, point.1, 2);
        Timer::single_shot(Duration::from_millis(50), move || {
            let _ = entry.upgrade().unwrap();
            assert_eq!(exits.get(), before + 1, "Second menu item must invoke exit");
            let _ = menu.upgrade().unwrap().hide();
            done();
        });
    });
}

#[cfg(debug_assertions)]
fn qa_entry_controls(
    mini: &MiniWindow,
    capsule: &CapsuleWindow,
    settings: &SettingsWindow,
    menu: &EntryMenuWindow,
) {
    let exits = Rc::new(Cell::new(0));
    let captured = exits.clone();
    menu.on_exit_app(move || captured.set(captured.get() + 1));
    let menu = menu.as_weak();
    let m = mini.as_weak();
    let c = capsule.as_weak();
    let s = settings.as_weak();
    Timer::single_shot(Duration::from_millis(230), move || {
        let mini = m.upgrade().unwrap();
        let capsule = c.upgrade().unwrap();
        qa_click(mini.window(), 33.0, 33.0);
        assert!(
            capsule.window().is_visible(),
            "Left click must show the capsule synchronously"
        );
        assert!(
            !s.upgrade().unwrap().window().is_visible(),
            "Left click must not open settings"
        );
        Timer::single_shot(
            Duration::from_millis(flow::duration_ms(true, true) + 150),
            move || {
                let capsule = c.upgrade().unwrap();
                let settings = s.upgrade().unwrap();
                let x =
                    capsule.window().size().width as f32 / capsule.window().scale_factor() / 2.0;
                let m2 = m.clone();
                let c2 = c.clone();
                let s2 = s.clone();
                let exits2 = exits.clone();
                let menu2 = menu.clone();
                qa_menu_actions(
                    &capsule,
                    &menu.upgrade().unwrap(),
                    &settings,
                    (x, 100.0),
                    exits,
                    move || {
                        let capsule = c2.upgrade().unwrap();
                        qa_click(capsule.window(), x, 49.0);
                        assert!(
                            m2.upgrade().unwrap().window().is_visible(),
                            "Left click must show Mini synchronously"
                        );
                        Timer::single_shot(
                            Duration::from_millis(flow::duration_ms(true, false) + 150),
                            move || {
                                let mini = m2.upgrade().unwrap();
                                let settings = s2.upgrade().unwrap();
                                let mini3 = mini.as_weak();
                                let menu3 = menu2.clone();
                                qa_menu_actions(
                                    &mini,
                                    &menu2.upgrade().unwrap(),
                                    &settings,
                                    (33.0, 33.0),
                                    exits2,
                                    move || {
                                        let mini = mini3.upgrade().unwrap();
                                        mini.invoke_show_menu(33.0, 33.0);
                                        let menu = menu3.upgrade().unwrap();
                                        menu.window().dispatch_event(
                                            slint::platform::WindowEvent::WindowActiveChanged(true),
                                        );
                                        menu.window().dispatch_event(
                                            slint::platform::WindowEvent::WindowActiveChanged(
                                                false,
                                            ),
                                        );
                                        assert!(
                                            !menu.window().is_visible(),
                                            "Clicking away must dismiss the menu on focus loss"
                                        );
                                        eprintln!("WACU_QA entry_immediate_menu_pass");
                                    },
                                );
                            },
                        );
                    },
                );
            },
        );
    });
}

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::args().any(|arg| arg == "--claude-activity-status") {
        println!("{}", claude_activity::active_count());
        return Ok(());
    }
    if std::env::args().any(|arg| arg == claude_activity::HOOK_ARG) {
        claude_activity::handle_hook();
        return Ok(());
    }
    if std::env::args().any(|arg| {
        matches!(
            arg.as_str(),
            "--install-claude-activity" | "--uninstall-claude-activity"
        )
    }) {
        let enable = std::env::args().any(|arg| arg == "--install-claude-activity");
        let backup = claude_activity::install(&std::env::current_exe()?, enable)?;
        println!(
            "Wacu Claude activity {}. Backup: {:?}",
            if enable { "connected" } else { "disconnected" },
            backup
        );
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--codex-usage-check") {
        match codex::fetch() {
            Ok(snapshot) => println!("{}", serde_json::to_string_pretty(&snapshot)?),
            Err(error) => return Err(error.message().into()),
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--claude-usage-check") {
        match claude::fetch() {
            Ok(snapshot) => println!("{}", serde_json::to_string_pretty(&snapshot)?),
            Err(error) => return Err(error.message().into()),
        }
        return Ok(());
    }
    #[cfg(debug_assertions)]
    if std::env::var_os("WACU_QA_VIEW").is_some() {
        if let Ok(fixture) = std::env::var("WACU_QA_CLAUDE_FIXTURE") {
            let result = match fixture.as_str() {
                "unauthorized" => Err(claude::FetchError::Unauthorized),
                "rate-limited" => Err(claude::FetchError::RateLimited(900)),
                "ok" => claude::parse_snapshot(serde_json::json!({
                    "five_hour":{"utilization":12.5,"resets_at":"2026-10-12T04:00:00+08:00"},
                    "seven_day":{"utilization":33,"resets_at":"2026-10-18T12:00:00+08:00"},
                    "seven_day_sonnet":{"utilization":20,"resets_at":"2026-10-18T12:00:00+08:00"}
                })),
                "unused" | "half" | "exhausted" => claude::parse_snapshot(serde_json::json!({
                    "five_hour":{"utilization":match fixture.as_str() { "unused" => 0, "half" => 50, _ => 100 }}
                })),
                _ => return Err("Unknown Claude QA fixture".into()),
            };
            CLAUDE_USAGE.with_borrow_mut(|state| state.apply(result));
        }
    }
    let owner_handle = Rc::new(Cell::new(0isize));
    let hook_owner = owner_handle.clone();
    use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("femtovg".into())
        .with_winit_window_attributes_hook(move |attributes| {
            assert_ne!(hook_owner.get(), 0, "Utility owner must precede UI windows");
            attributes
                .with_owner_window(hook_owner.get() as _)
                .with_skip_taskbar(true)
                .with_active(false)
        })
        .select()?;
    // Let winit establish process DPI awareness before creating any HWND.
    let utility_owner = platform_windows::UtilityOwner::new()?;
    owner_handle.set(utility_owner.handle() as isize);
    let initial = state::load();
    #[cfg(not(debug_assertions))]
    let mini_active = true;
    #[cfg(debug_assertions)]
    let mini_active = std::env::var("WACU_QA_VIEW")
        .map(|view| view.is_empty() || view == "mini" || view == "entry-session")
        .unwrap_or(true);
    #[cfg(debug_assertions)]
    let mut initial = initial;
    #[cfg(debug_assertions)]
    let qa_view = std::env::var("WACU_QA_VIEW").unwrap_or_default();
    #[cfg(debug_assertions)]
    if std::env::var_os("WACU_QA_ENTRY_SESSION").is_some() {
        let expected = std::env::current_dir()?.join("target/qa-entry-session-profile");
        let actual = std::env::var("APPDATA")
            .unwrap_or_default()
            .replace('/', "\\");
        if qa_view != "entry-session"
            || !actual.eq_ignore_ascii_case(&expected.to_string_lossy().replace('/', "\\"))
        {
            return Err("Entry session QA requires an isolated APPDATA".into());
        }
    }
    #[cfg(debug_assertions)]
    if qa_view == "mini" {
        initial.entry_view = EntryView::Mini;
    }
    #[cfg(debug_assertions)]
    if qa_view == "menu" {
        let preview = EntryMenuWindow::new()?;
        preview.set_selected(
            std::env::var("WACU_QA_MENU_SELECTED")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(-1),
        );
        let weak = preview.as_weak();
        Timer::single_shot(Duration::from_millis(200), move || {
            qa_entry_frame(weak.upgrade().unwrap().window(), "menu-preview");
            std::fs::copy(
                "target/qa-entry-menu-preview.png",
                "target/wacu-qa-menu.png",
            )
            .unwrap();
            let _ = slint::quit_event_loop();
        });
        preview.run()?;
        return Ok(());
    }
    #[cfg(debug_assertions)]
    if std::env::var_os("WACU_QA_ENTRY_MOTION").is_some() {
        let size = std::env::var("WACU_QA_SIZE")?.parse::<u8>()?.min(2);
        let count = std::env::var("WACU_QA_COUNT")?.parse::<usize>()?.min(3);
        let expected =
            std::env::current_dir()?.join(format!("target/qa-entry-profile-{size}-{count}"));
        let actual = std::env::var("APPDATA")
            .unwrap_or_default()
            .replace('/', "\\");
        if qa_view != "mini"
            || !actual.eq_ignore_ascii_case(&expected.to_string_lossy().replace('/', "\\"))
        {
            return Err("Entry animation QA requires an isolated APPDATA".into());
        }
    }
    #[cfg(debug_assertions)]
    if let Ok(direction) = std::env::var("WACU_QA_ENTRY_HANDOFF") {
        let direction = direction.parse::<u8>()?;
        let count = std::env::var("WACU_QA_COUNT")?.parse::<usize>()?.min(3);
        let expected =
            std::env::current_dir()?.join(format!("target/qa-handoff-profile-{count}-{direction}"));
        let actual = std::env::var("APPDATA")
            .unwrap_or_default()
            .replace('/', "\\");
        if qa_view != "mini"
            || direction > 2
            || !actual.eq_ignore_ascii_case(&expected.to_string_lossy().replace('/', "\\"))
        {
            return Err("Entry handoff QA requires an isolated APPDATA".into());
        }
        initial.animations_enabled = true;
    }
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
    ) || (qa_view == "mini"
        && (std::env::var_os("WACU_QA_ENTRY_MOTION").is_some()
            || std::env::var_os("WACU_QA_ENTRY_HANDOFF").is_some()
            || std::env::var_os("WACU_QA_CLAUDE_FIXTURE").is_some()))
    {
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
    let detail_motion = DetailMotion::default();
    let entry_motion = EntryMotion::new()?;

    let capsule = CapsuleWindow::new()?;
    capsule.on_render_arc(arc_percent);
    capsule.on_render_trail(arc_trail);
    let mini = MiniWindow::new()?;
    mini.on_render_trail(arc_trail);
    mini.on_render_rim(|index, count, percent, turn, tip, x, y| {
        flow::mini_path(
            count.max(0) as usize,
            index.max(0) as usize,
            percent as f64,
            turn as f64,
            tip,
            flow::Vector {
                x: x as f64,
                y: y as f64,
            },
        )
        .into()
    });
    mini.set_animated(initial.animations_enabled);
    let menu = EntryMenuWindow::new()?;
    let settings = SettingsWindow::new()?;
    {
        let weak = settings.as_weak();
        settings.on_connect_claude_activity(move || {
            if let Some(settings) = weak.upgrade() {
                match std::env::current_exe()
                    .map_err(|e| e.to_string())
                    .and_then(|exe| claude_activity::install(&exe, true).map_err(|e| e.to_string()))
                {
                    Ok(_) => {
                        settings.set_claude_activity_connected(true);
                        settings.set_status_message(
                            "已连接本地工作状态。请新开 CLI 或 Desktop Code 会话。".into(),
                        );
                    }
                    Err(error) => settings.set_status_message(error.into()),
                }
            }
        });
    }
    let detail = DetailWindow::new()?;
    #[cfg(debug_assertions)]
    let tray = Some(WacuTray::new()?);
    #[cfg(not(debug_assertions))]
    let tray: Option<WacuTray> = None;

    let model = Rc::new(VecModel::from(
        visible_service_indices(&initial.shown)
            .map(|i| service(i, true))
            .collect::<Vec<_>>(),
    ));
    capsule.set_services(ModelRc::from(model.clone()));
    mini.set_services(ModelRc::from(model.clone()));
    refresh_model(&model, &capsule, &settings, &initial);
    let _codex_watcher =
        start_usage_watcher(0, model.clone(), &settings, &detail, preferences.clone());
    let _claude_watcher =
        start_usage_watcher(1, model.clone(), &settings, &detail, preferences.clone());

    {
        let view = menu.as_weak();
        menu.on_dismiss(move || {
            if let Some(menu) = view.upgrade() {
                let _ = menu.hide();
            }
        });
        let view = menu.as_weak();
        let settings = settings.as_weak();
        menu.on_open_settings(move || {
            if let Some(menu) = view.upgrade() {
                let _ = menu.hide();
            }
            if let Some(settings) = settings.upgrade() {
                show_settings(&settings);
            }
        });
        menu.on_exit_app(|| {
            let _ = slint::quit_event_loop();
        });
        let view = menu.as_weak();
        let owner = mini.as_weak();
        mini.on_request_menu(move |x, y| {
            if let (Some(menu), Some(owner)) = (view.upgrade(), owner.upgrade()) {
                show_entry_menu(&menu, owner.window(), x, y, owner.get_animated());
            }
        });
        let view = menu.as_weak();
        mini.on_dismiss_menu(move || {
            if let Some(menu) = view.upgrade() {
                let _ = menu.hide();
            }
        });
        let view = menu.as_weak();
        let owner = capsule.as_weak();
        capsule.on_request_menu(move |x, y| {
            if let (Some(menu), Some(owner)) = (view.upgrade(), owner.upgrade()) {
                show_entry_menu(&menu, owner.window(), x, y, owner.get_animated());
            }
        });
        let view = menu.as_weak();
        capsule.on_dismiss_menu(move || {
            if let Some(menu) = view.upgrade() {
                let _ = menu.hide();
            }
        });
    }

    // Restore the last desktop entry; the shared menu is shown on demand
    // in the same process, without background threads or Explorer integration.
    {
        let mini_weak = mini.as_weak();
        let capsule_weak = capsule.as_weak();
        let prefs = preferences.clone();
        let motion = entry_motion.clone();
        mini.on_toggle_expanded(move || {
            if let (Some(m), Some(c)) = (mini_weak.upgrade(), capsule_weak.upgrade()) {
                m.invoke_close_menu();
                c.invoke_close_menu();
                expand_entry(&m, &c, &prefs, &motion);
            }
        });
    }

    {
        let weak = mini.as_weak();
        let cap = capsule.as_weak();
        let motion = entry_motion.clone();
        let prefs = preferences.clone();
        mini.on_start_drag(move || {
            if motion.active.get() {
                if let (Some(m), Some(c)) = (weak.upgrade(), cap.upgrade()) {
                    motion.settle(&m, &c, false);
                }
            }
            let weak = weak.clone();
            let prefs = prefs.clone();
            Timer::single_shot(Duration::from_millis(15), move || {
                if let Some(m) = weak.upgrade() {
                    platform_windows::begin_drag(m.window());
                    remember_mini(&m, &prefs);
                    m.set_dragging(false);
                }
            });
        });
    }
    {
        let weak = settings.as_weak();
        mini.on_open_settings(move || {
            if let Some(s) = weak.upgrade() {
                show_settings(&s);
            }
        });
    }
    {
        let weak = mini.as_weak();
        let cap = capsule.as_weak();
        let detail = detail.as_weak();
        let prefs = preferences.clone();
        let motion = detail_motion.clone();
        let entry_motion = entry_motion.clone();
        capsule.on_collapse_to_mini(move || {
            if let (Some(c), Some(m)) = (cap.upgrade(), weak.upgrade()) {
                c.invoke_close_menu();
                if let Some(d) = detail.upgrade() {
                    motion.next();
                    motion.closing.set(false);
                    d.set_revealed(false);
                    let _ = d.hide();
                }
                c.set_selected_index(-1);
                // Moving the expanded capsule only changes saved_position.
                // The Mini will restore itself at its own mini_position.
                collapse_entry(&m, &c, &prefs, &entry_motion);
            }
        });
    }
    mini.on_exit_app(|| {
        let _ = slint::quit_event_loop();
    });
    capsule.on_exit_app(|| {
        let _ = slint::quit_event_loop();
    });
    {
        settings.on_exit_app(|| {
            let _ = slint::quit_event_loop();
        });
    }

    if let Some(tray) = tray.as_ref() {
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
                        set_mode(next, &pref, &model, &c, &s, &origin, false);
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
    }

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
        let detail_weak = detail.as_weak();
        let motion = detail_motion.clone();
        let mini_weak = mini.as_weak();
        let entry_motion = entry_motion.clone();
        capsule.on_start_drag(move || {
            if entry_motion.active.get() {
                if let (Some(m), Some(c)) = (mini_weak.upgrade(), weak.upgrade()) {
                    entry_motion.settle(&m, &c, true);
                }
            }
            preview.set(false);
            active_drag.set(true);
            if let Some(c) = weak.upgrade() {
                c.set_logo_dragging(true);
            }
            let cap = weak.clone();
            let prefs = prefs.clone();
            let active_drag = active_drag.clone();
            let last_open = last_open.clone();
            let detail_weak = detail_weak.clone();
            let motion = motion.clone();
            // Dragging always hides the detail instantly, including while an
            // earlier fade-out is still running. Restore only an open detail.
            let visible_detail = detail_weak.upgrade().filter(|d| d.window().is_visible());
            let restore_detail = visible_detail.is_some() && !motion.closing.get();
            if let Some(d) = visible_detail {
                motion.next(); // invalidate a pending reveal or dismiss timer
                motion.closing.set(false);
                d.set_revealed(false);
                let _ = d.hide();
            }
            // Allow the avatar's pressed state one frame before the native drag.
            Timer::single_shot(Duration::from_millis(35), move || {
                if let Some(c) = cap.upgrade() {
                    platform_windows::begin_drag(c.window());
                    // The Windows move loop has now ended (mouse released).
                    last_open.set(Instant::now());
                    remember_position(&c, &prefs, None);
                    c.set_logo_dragging(false);
                    if restore_detail {
                        if let Some(d) = detail_weak.upgrade() {
                            restore_detail_after_drag(&d, &c, &motion);
                        }
                    }
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
        let motion = detail_motion.clone();
        capsule.on_open_service(move |idx| {
            preview.set(false);
            if let (Some(c), Some(d)) = (cap.upgrade(), view.upgrade()) {
                if let Ok(index) = usize::try_from(idx) {
                    if index < 3 {
                        show_detail(&d, &c, index, &pref.borrow(), &motion);
                    }
                }
            }
        });
    }
    {
        let weak = detail.as_weak();
        let cap = capsule.as_weak();
        let motion = detail_motion.clone();
        detail.on_close_detail(move || {
            if let Some(d) = weak.upgrade() {
                dismiss_detail(&d, &motion);
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
                    mini_active,
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
        let mini_weak = mini.as_weak();
        let motion = entry_motion.clone();
        settings.on_toggle_service(move |index, enabled| {
            if let Ok(i) = usize::try_from(index) {
                if i < 3 {
                    let mut value = pref.borrow_mut();
                    if motion.active.get() {
                        if let (Some(m), Some(c)) = (mini_weak.upgrade(), cap.upgrade()) {
                            motion.settle(&m, &c, value.entry_view != EntryView::Mini);
                        }
                    }
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
        let mini_weak = mini.as_weak();
        let motion = entry_motion.clone();
        settings.on_set_animations(move |enabled| {
            let mut value = pref.borrow_mut();
            value.animations_enabled = enabled;
            if let Err(error) = state::save(&value) {
                eprintln!("Saving animations: {error}");
            }
            if let Some(c) = cap.upgrade() {
                c.set_animated(enabled);
            }
            if let Some(m) = mini_weak.upgrade() {
                m.set_animated(enabled);
            }
            if let Some(s) = settings_weak.upgrade() {
                s.set_animations_on(enabled);
            }
            if let Some(d) = detail_weak.upgrade() {
                d.set_animated(enabled);
            }
            if !enabled && mini_active {
                // Cancel handoff and collection timers immediately when motion
                // is disabled; the saved target view remains authoritative.
                if let (Some(c), Some(m)) = (cap.upgrade(), mini_weak.upgrade()) {
                    motion.settle(&m, &c, value.entry_view != EntryView::Mini);
                }
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
        let mini_weak = mini.as_weak();
        let motion = entry_motion.clone();
        settings.on_set_capsule_size(move |size| {
            let size = size.clamp(0, 2) as u8;
            let mut value = prefs.borrow_mut();
            if value.capsule_size == size {
                return;
            }
            if motion.active.get() {
                if let (Some(m), Some(c)) = (mini_weak.upgrade(), capsule_weak.upgrade()) {
                    motion.settle(&m, &c, value.entry_view != EntryView::Mini);
                }
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
                        if mini_active {
                            let pos = c.window().position();
                            let size = capsule_dimensions(&c);
                            let area = platform_windows::work_area_at(Point { x: pos.x, y: pos.y });
                            let target = constrain(Point { x: pos.x, y: pos.y }, size, area);
                            c.window()
                                .set_position(PhysicalPosition::new(target.x, target.y));
                            remember_position(&c, &prefs, None);
                        } else {
                            place_capsule(&c, &prefs.borrow());
                        }
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
        let mini_weak = mini.as_weak();
        let motion = entry_motion.clone();
        settings.on_set_dock_side(move |index| {
            let mut value = pref.borrow_mut();
            if motion.active.get() {
                if let (Some(m), Some(c)) = (mini_weak.upgrade(), cap.upgrade()) {
                    motion.settle(&m, &c, value.entry_view != EntryView::Mini);
                }
            }
            value.dock_left = index == 1;
            origin.set(None);
            if mini_active {
                // The side selector controls Mini only. The user's expanded
                // capsule stays at its independently chosen position.
                value.mini_position = None;
                if let Some(m) = mini_weak.upgrade() {
                    let sz = m.window().size();
                    let size = (sz.width as i32, sz.height as i32);
                    let current = m.window().position();
                    let area = platform_windows::work_area_at(Point {
                        x: current.x + size.0 / 2,
                        y: current.y + size.1 / 2,
                    });
                    let pos = default_mini_position(size, area, value.dock_left);
                    m.window().set_position(PhysicalPosition::new(pos.x, pos.y));
                    value.mini_position = Some((pos.x, pos.y));
                }
            } else if let Some(c) = cap.upgrade() {
                // Legacy debug/pinned view uses the original dock selector.
                value.saved_position = None;
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
        let pref = preferences.clone();
        let mini_weak = mini.as_weak();
        let cap = capsule.as_weak();
        let motion = entry_motion.clone();
        settings.on_reset_mini_position(move || {
            let Some(m) = mini_weak.upgrade() else { return };
            if motion.active.get() {
                if let Some(c) = cap.upgrade() {
                    motion.settle(&m, &c, pref.borrow().entry_view != EntryView::Mini);
                }
            }
            let size = m.window().size();
            let pos = m.window().position();
            let area = platform_windows::work_area_at(Point {
                x: pos.x + size.width as i32 / 2,
                y: pos.y + size.height as i32 / 2,
            });
            let mut value = pref.borrow_mut();
            let target = default_mini_position(
                (size.width as i32, size.height as i32),
                area,
                value.dock_left,
            );
            m.window()
                .set_position(PhysicalPosition::new(target.x, target.y));
            value.mini_position = Some((target.x, target.y));
            // Do not modify saved_position or move the expanded capsule.
            if let Err(error) = state::save(&value) {
                eprintln!("Resetting Mini position: {error}");
            }
        });
    }
    {
        let cap = capsule.as_weak();
        let pref = preferences.clone();
        let mini_weak = mini.as_weak();
        let detail_weak = detail.as_weak();
        let origin = edge_origin.clone();
        let motion = entry_motion.clone();
        settings.on_reset_capsule_position(move || {
            let Some(c) = cap.upgrade() else { return };
            if motion.active.get() {
                if let Some(m) = mini_weak.upgrade() {
                    motion.settle(&m, &c, pref.borrow().entry_view != EntryView::Mini);
                }
            }
            let anchor = if let Some((x, y)) = pref.borrow().saved_position {
                Point { x, y }
            } else if let Some(m) = mini_weak.upgrade() {
                let pos = m.window().position();
                Point { x: pos.x, y: pos.y }
            } else {
                platform_windows::cursor()
            };
            let mut value = pref.borrow_mut();
            origin.set(None);
            let target = pinned(
                capsule_dimensions(&c),
                platform_windows::work_area_at(anchor),
            );
            value.saved_position = Some((target.x, target.y));
            // Mini keeps its exact saved position.
            if c.window().is_visible() {
                c.window()
                    .set_position(PhysicalPosition::new(target.x, target.y));
                if let Some(d) = detail_weak.upgrade().filter(|d| d.window().is_visible()) {
                    position_detail(&d, &c);
                }
            }
            if let Err(error) = state::save(&value) {
                eprintln!("Resetting capsule position: {error}");
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

    // Production uses a real floating Mini window. No notification-area icon.
    #[cfg(not(debug_assertions))]
    let show_tray = false;
    #[cfg(debug_assertions)]
    let show_tray = !matches!(
        qa_view.as_str(),
        "capsule" | "quick" | "detail" | "settings" | "entry-session"
    );
    if show_tray {
        if let Some(tray) = tray.as_ref() {
            tray.show()?;
        }
    }
    startup_preview.set(!mini_active && initial.mode == Mode::Popup);
    last_popup_open.set(Instant::now());
    if mini_active {
        restore_entry(&mini, &capsule, &preferences, &entry_motion);
    } else {
        show_capsule(&capsule, &preferences);
    }

    // Debug-only visual QA; does not exist in the Release application.
    #[cfg(debug_assertions)]
    if !qa_view.is_empty() {
        if let Ok(action) = std::env::var("WACU_QA_ENTRY_SESSION") {
            let expected = match std::env::var("WACU_QA_ENTRY_EXPECT").as_deref() {
                Ok("mini") => EntryView::Mini,
                Ok("capsule") => EntryView::Capsule,
                _ => return Err("Entry session QA requires an expected view".into()),
            };
            let m = mini.as_weak();
            let c = capsule.as_weak();
            let s = settings.as_weak();
            let menu = menu.as_weak();
            let prefs = preferences.clone();
            let delay = if action == "verify-settings" {
                3700
            } else {
                350
            };
            Timer::single_shot(Duration::from_millis(delay), move || {
                let m = m.upgrade().unwrap();
                let c = c.upgrade().unwrap();
                let s = s.upgrade().unwrap();
                let menu = menu.upgrade().unwrap();
                assert_eq!(prefs.borrow().entry_view, expected);
                assert_eq!(m.window().is_visible(), expected == EntryView::Mini);
                assert_eq!(c.window().is_visible(), expected == EntryView::Capsule);
                if expected == EntryView::Mini {
                    assert!(m.get_revealed());
                    let p = m.window().position();
                    assert_eq!(prefs.borrow().mini_position, Some((p.x, p.y)));
                } else {
                    assert!(c.get_revealed());
                    let p = c.window().position();
                    assert_eq!(prefs.borrow().saved_position, Some((p.x, p.y)));
                }
                match action.as_str() {
                    "verify" => {}
                    "verify-settings" => {
                        show_settings(&s);
                        assert!(s.window().is_visible());
                        s.invoke_set_pinned(true);
                        s.invoke_set_pinned(false);
                        assert_eq!(prefs.borrow().entry_view, expected);
                        assert_eq!(m.window().is_visible(), expected == EntryView::Mini);
                        assert_eq!(c.window().is_visible(), expected == EntryView::Capsule);
                    }
                    "expand" => m.invoke_toggle_expanded(),
                    "collapse" => c.invoke_collapse_to_mini(),
                    "rapid-capsule" => {
                        m.invoke_toggle_expanded();
                        c.invoke_collapse_to_mini();
                        m.invoke_toggle_expanded();
                    }
                    "rapid-mini" => {
                        c.invoke_collapse_to_mini();
                        m.invoke_toggle_expanded();
                        c.invoke_collapse_to_mini();
                    }
                    _ => panic!("Unknown entry session QA action"),
                }
                eprintln!(
                    "WACU_QA entry_session_pass initial={expected:?} action={action} target={:?}",
                    prefs.borrow().entry_view
                );
                if action == "collapse" || action == "verify-settings" {
                    s.invoke_exit_app();
                } else {
                    menu.invoke_exit_app();
                }
            });
        }
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
            let tray = tray.as_ref().expect("Debug tray QA").as_weak();
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
                    c.invoke_open_settings();
                    assert!(s.window().is_visible(), "Settings callback was intercepted");
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
                    qa_click(s.window(), 250.0, 284.0);
                }
            });
            let s = settings.as_weak();
            Timer::single_shot(Duration::from_millis(350), move || {
                use slint::platform::{Key, PointerEventButton, WindowEvent};
                let s = s.upgrade().unwrap();
                let previous = s.get_opacity_percent();
                let start = slint::LogicalPosition::new(250.0, 284.0);
                let end = slint::LogicalPosition::new(300.0, 284.0);
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
                qa_click(s.window(), 250.0, 284.0);
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
                qa_click(s.window(), 250.0, 284.0);
            });
            let s = settings.as_weak();
            Timer::single_shot(Duration::from_millis(400), move || {
                if let Some(s) = s.upgrade() {
                    qa_click(s.window(), 325.0, 328.0);
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
            "mini" => {
                if std::env::var_os("WACU_QA_ENTRY_CONTROLS").is_some() {
                    qa_entry_controls(&mini, &capsule, &settings, &menu);
                }
                if std::env::var_os("WACU_QA_ENTRY_MOTION").is_some() {
                    qa_entry_animation(&mini, &capsule, &settings, &preferences);
                }
                if std::env::var_os("WACU_QA_ENTRY_HANDOFF").is_some() {
                    qa_entry_handoff(&mini, &capsule, &settings, &preferences, &entry_motion);
                }
                if std::env::var_os("WACU_QA_MINI_POSITIONS").is_some() {
                    let m = mini.as_weak();
                    let c = capsule.as_weak();
                    let settings = settings.as_weak();
                    let prefs = preferences.clone();
                    Timer::single_shot(Duration::from_millis(230), move || {
                        let m = m.upgrade().unwrap();
                        let c = c.upgrade().unwrap();
                        let settings = settings.upgrade().unwrap();
                        let area = platform_windows::work_area_at({
                            let p = m.window().position();
                            Point { x: p.x, y: p.y }
                        });
                        let mini_size = m.window().size();
                        let mini_pos = default_mini_position(
                            (mini_size.width as i32, mini_size.height as i32),
                            area,
                            false,
                        );
                        let capsule_pos = docked(capsule_dimensions(&c), area, false);
                        assert_ne!(mini_pos.y, capsule_pos.y);
                        m.window()
                            .set_position(PhysicalPosition::new(mini_pos.x, mini_pos.y));
                        {
                            let mut pref = prefs.borrow_mut();
                            pref.mini_position = Some((mini_pos.x, mini_pos.y));
                            pref.saved_position = Some((capsule_pos.x, capsule_pos.y));
                        }
                        m.invoke_toggle_expanded();
                        assert!(c.window().is_visible());
                        assert_eq!(c.window().position().x, capsule_pos.x);
                        assert_eq!(c.window().position().y, capsule_pos.y);
                        c.invoke_collapse_to_mini();
                        let m2 = m.as_weak();
                        let c2 = c.as_weak();
                        let prefs2 = prefs.clone();
                        let settings2 = settings.as_weak();
                        Timer::single_shot(Duration::from_millis(720), move || {
                            let m = m2.upgrade().unwrap();
                            let c = c2.upgrade().unwrap();
                            let settings = settings2.upgrade().unwrap();
                            assert!(m.window().is_visible());
                            assert_eq!(m.window().position().x, mini_pos.x);
                            assert_eq!(m.window().position().y, mini_pos.y);
                            m.invoke_toggle_expanded();
                            assert_eq!(c.window().position().y, capsule_pos.y);
                            // Simulate native drag to a distinctly different
                            // location and save it on collapse.
                            let moved = constrain(
                                Point {
                                    x: capsule_pos.x,
                                    y: capsule_pos.y + 65,
                                },
                                capsule_dimensions(&c),
                                area,
                            );
                            assert_ne!(capsule_pos.y, moved.y);
                            c.window()
                                .set_position(PhysicalPosition::new(moved.x, moved.y));
                            c.invoke_collapse_to_mini();
                            let mini3 = m.as_weak();
                            Timer::single_shot(Duration::from_millis(720), move || {
                                let m = mini3.upgrade().unwrap();
                                assert!(m.window().is_visible());
                                assert_eq!(m.window().position().y, mini_pos.y);
                                assert_eq!(
                                    prefs2.borrow().mini_position,
                                    Some((mini_pos.x, mini_pos.y))
                                );
                                assert_eq!(
                                    prefs2.borrow().saved_position,
                                    Some((moved.x, moved.y))
                                );
                                // Round-trip through the real JSON storage.
                                state::save(&prefs2.borrow()).unwrap();
                                let reloaded = state::load();
                                assert_eq!(reloaded.mini_position, Some((mini_pos.x, mini_pos.y)));
                                assert_eq!(reloaded.saved_position, Some((moved.x, moved.y)));
                                // Resetting either position may not reset the other.
                                settings.invoke_reset_mini_position();
                                assert_eq!(
                                    prefs2.borrow().saved_position,
                                    Some((moved.x, moved.y))
                                );
                                settings.invoke_reset_capsule_position();
                                assert_eq!(
                                    prefs2.borrow().mini_position,
                                    Some((mini_pos.x, mini_pos.y))
                                );
                                assert_eq!(
                                    prefs2.borrow().saved_position,
                                    Some((capsule_pos.x, capsule_pos.y))
                                );
                                eprintln!("WACU_QA mini_independent_positions_pass");
                            });
                        });
                    });
                }
                if std::env::var_os("WACU_QA_MINI_INPUT").is_some() {
                    let menu = menu.as_weak();
                    let m = mini.as_weak();
                    let c = capsule.as_weak();
                    let settings = settings.as_weak();
                    Timer::single_shot(Duration::from_millis(220), move || {
                        use slint::platform::{PointerEventButton, WindowEvent};
                        let m = m.upgrade().unwrap();
                        let c = c.upgrade().unwrap();
                        let settings = settings.upgrade().unwrap();
                        assert!(m.window().is_visible());
                        assert!(!c.window().is_visible());
                        m.invoke_toggle_expanded();
                        assert!(c.window().is_visible(), "Mini toggle must expand");
                        assert!(!m.get_revealed(), "Mini must begin its exit when expanded");
                        // Top avatar is a click target, but drag remains on >7px motion.
                        let clicking = c.as_weak();
                        Timer::single_shot(Duration::from_millis(60), move || {
                            clicking.upgrade().unwrap().invoke_collapse_to_mini();
                        });
                        let m2 = m.as_weak();
                        let c2 = c.as_weak();
                        Timer::single_shot(Duration::from_millis(800), move || {
                            let m = m2.upgrade().unwrap();
                            let c = c2.upgrade().unwrap();
                            assert!(
                                m.window().is_visible(),
                                "Second click must collapse to Mini"
                            );
                            assert!(!c.window().is_visible());
                            let calls = Rc::new(Cell::new(0));
                            let captured = calls.clone();
                            m.on_start_drag(move || captured.set(captured.get() + 1));
                            let start = slint::LogicalPosition::new(33.0, 33.0);
                            let end = slint::LogicalPosition::new(45.0, 33.0);
                            m.window()
                                .dispatch_event(WindowEvent::PointerMoved { position: start });
                            m.window().dispatch_event(WindowEvent::PointerPressed {
                                position: start,
                                button: PointerEventButton::Left,
                            });
                            m.window()
                                .dispatch_event(WindowEvent::PointerMoved { position: end });
                            m.window().dispatch_event(WindowEvent::PointerReleased {
                                position: end,
                                button: PointerEventButton::Left,
                            });
                            assert_eq!(calls.get(), 1, "Mini drag threshold failed");
                            assert!(m.window().is_visible(), "Drag must not expand the capsule");
                            m.set_dragging(false);
                            qa_context_command(m.window(), &menu.upgrade().unwrap(), 33.0, 33.0, 0);
                            assert!(
                                !settings.window().is_visible(),
                                "Right click must open the menu without opening settings"
                            );
                            assert!(m.window().is_visible(), "Right click must keep Mini open");
                            assert!(
                                !c.window().is_visible(),
                                "Right click must not expand capsule"
                            );
                            m.invoke_close_menu();
                            eprintln!("WACU_QA mini_toggle_drag_settings_pass");
                        });
                    });
                }
            }
            "quick" | "detail" => {
                let index = std::env::var("WACU_QA_SERVICE")
                    .ok()
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(1)
                    .min(2);
                show_detail(&detail, &capsule, index, &initial, &detail_motion);
                if std::env::var_os("WACU_QA_DETAIL_FADE").is_some() {
                    let fading = detail.as_weak();
                    Timer::single_shot(Duration::from_millis(85), move || {
                        let d = fading.upgrade().unwrap();
                        let mid = qa_detail_opacity(d.window());
                        let fading = d.as_weak();
                        Timer::single_shot(Duration::from_millis(145), move || {
                            let d = fading.upgrade().unwrap();
                            let full = qa_detail_opacity(d.window());
                            assert!(
                                full > mid.saturating_add(10),
                                "Detail opacity must visibly animate; mid={mid} full={full}"
                            );
                            eprintln!("WACU_QA detail_alpha_pass mid={mid} full={full}");
                        });
                    });
                    let cap = capsule.as_weak();
                    let det = detail.as_weak();
                    let motion = detail_motion.clone();
                    let prefs = initial.clone();
                    Timer::single_shot(Duration::from_millis(240), move || {
                        let c = cap.upgrade().unwrap();
                        let d = det.upgrade().unwrap();
                        assert!(d.window().is_visible() && d.get_revealed());
                        dismiss_detail(&d, &motion);
                        assert!(d.window().is_visible() && !d.get_revealed());
                        let cap = c.as_weak();
                        let det = d.as_weak();
                        let motion = motion.clone();
                        Timer::single_shot(Duration::from_millis(40), move || {
                            let c = cap.upgrade().unwrap();
                            let d = det.upgrade().unwrap();
                            show_detail(&d, &c, 1, &prefs, &motion);
                            assert!(d.window().is_visible() && d.get_revealed());
                            let cap = c.as_weak();
                            let det = d.as_weak();
                            let motion = motion.clone();
                            Timer::single_shot(Duration::from_millis(100), move || {
                                let c = cap.upgrade().unwrap();
                                let d = det.upgrade().unwrap();
                                assert!(
                                    d.window().is_visible(),
                                    "Old close timer hid reopened detail"
                                );
                                dismiss_detail(&d, &motion);
                                let cap = c.as_weak();
                                let det = d.as_weak();
                                let motion = motion.clone();
                                Timer::single_shot(Duration::from_millis(205), move || {
                                    let c = cap.upgrade().unwrap();
                                    let d = det.upgrade().unwrap();
                                    assert!(
                                        !d.window().is_visible(),
                                        "Fade-out failed to hide the detail"
                                    );
                                    d.set_animated(false);
                                    reveal_detail_window(&d, &motion);
                                    assert!(d.window().is_visible() && d.get_revealed());
                                    dismiss_detail(&d, &motion);
                                    assert!(
                                        !d.window().is_visible(),
                                        "Disabled animations should close immediately"
                                    );
                                    d.set_animated(true);
                                    show_detail(&d, &c, 1, &prefs, &motion);
                                    eprintln!("WACU_QA detail_fade_pass");
                                });
                            });
                        });
                    });
                }
                if std::env::var_os("WACU_QA_DRAG_DETAIL").is_some() {
                    let cap = capsule.as_weak();
                    let det = detail.as_weak();
                    let motion = detail_motion.clone();
                    Timer::single_shot(Duration::from_millis(280), move || {
                        let c = cap.upgrade().unwrap();
                        let d = det.upgrade().unwrap();
                        assert!(d.window().is_visible());
                        let before = c.window().position();
                        let old_detail = d.window().position();
                        let area = platform_windows::work_area_at(Point {
                            x: before.x,
                            y: before.y,
                        });
                        let capsule_size = capsule_dimensions(&c);
                        let dx = if before.x - area.left >= 90 { -75 } else { 75 };
                        let target = constrain(
                            Point {
                                x: before.x + dx,
                                y: before.y,
                            },
                            capsule_size,
                            area,
                        );
                        assert_ne!((before.x, before.y), (target.x, target.y));
                        let selection = c.get_selected_index();
                        let provider = d.get_selected_service().source_index;
                        let _ = d.hide();
                        assert!(
                            !d.window().is_visible(),
                            "Detail should disappear during drag"
                        );
                        c.window()
                            .set_position(PhysicalPosition::new(target.x, target.y));
                        restore_detail_after_drag(&d, &c, &motion);
                        assert!(
                            d.window().is_visible(),
                            "Detail should reopen after mouse-up"
                        );
                        assert_eq!(c.get_selected_index(), selection);
                        assert_eq!(d.get_selected_service().source_index, provider);
                        let after_detail = d.window().position();
                        assert_ne!(
                            (old_detail.x, old_detail.y),
                            (after_detail.x, after_detail.y),
                            "Detail remained at its original desktop position"
                        );
                        eprintln!("WACU_QA detail_hide_restore_pass");
                    });
                }
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
        "mini" | "capsule" | "quick" | "detail" | "settings"
    ) && std::env::var_os("WACU_QA_ENTRY_HANDOFF").is_none()
        && std::env::var_os("WACU_QA_ENTRY_MOTION").is_none()
    {
        let mode = qa_view.clone();
        let m = mini.as_weak();
        let c = capsule.as_weak();
        let d = detail.as_weak();
        let s = settings.as_weak();
        let snapshot_delay = if std::env::var_os("WACU_QA_CLAUDE_LIVE").is_some() {
            43000
        } else if std::env::var_os("WACU_QA_CODEX_LIVE").is_some() {
            7000
        } else if std::env::var_os("WACU_QA_ENTRY_CONTROLS").is_some() {
            4200 + flow::duration_ms(true, true) - flow::OPEN_MS + flow::duration_ms(true, false)
                - flow::CLOSE_MS
        } else if std::env::var_os("WACU_QA_DETAIL_FADE").is_some() {
            1250
        } else if std::env::var_os("WACU_QA_MINI_POSITIONS").is_some() {
            2300
        } else if std::env::var_os("WACU_QA_MINI_INPUT").is_some() {
            1900
        } else {
            900
        };
        Timer::single_shot(Duration::from_millis(snapshot_delay), move || {
            if std::env::var_os("WACU_QA_CLAUDE_LIVE").is_some()
                || std::env::var_os("WACU_QA_CLAUDE_FIXTURE").is_some()
            {
                if std::env::var_os("WACU_QA_CLAUDE_LIVE").is_some() {
                    CLAUDE_USAGE.with_borrow(|state| {
                        assert!(
                            state.error.is_none(),
                            "Live Claude quota read failed: {}",
                            state.status()
                        );
                        assert!(
                            state
                                .snapshot
                                .as_ref()
                                .and_then(claude::Snapshot::main_window)
                                .is_some()
                        );
                    });
                }
                let expected = service(1, true);
                let capsule = c.upgrade().unwrap();
                let actual = (0..capsule.get_services().row_count())
                    .filter_map(|i| capsule.get_services().row_data(i))
                    .find(|item| item.source_index == 1)
                    .unwrap();
                assert_eq!(actual.percent_text, expected.percent_text);
                let mini = m.upgrade().unwrap();
                let mini_actual = (0..mini.get_services().row_count())
                    .filter_map(|i| mini.get_services().row_data(i))
                    .find(|item| item.source_index == 1)
                    .unwrap();
                assert_eq!(mini_actual.percent, expected.percent);
                if mode == "mini" {
                    let count = std::env::var("WACU_QA_COUNT")
                        .ok()
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(3)
                        .min(3);
                    assert_eq!(mini.get_services().row_count(), count);
                }
                if let Ok(fixture) = std::env::var("WACU_QA_CLAUDE_FIXTURE") {
                    if fixture == "unused" || fixture == "exhausted" {
                        let remaining = if fixture == "unused" { 100.0 } else { 0.0 };
                        assert_eq!(actual.percent, remaining);
                        assert_eq!(
                            actual.percent_text,
                            if remaining == 100.0 { "100%" } else { "0%" }
                        );
                        assert_eq!(actual.arc, arc_percent(remaining));
                    }
                }
                if mode == "quick" || mode == "detail" {
                    let detail = d.upgrade().unwrap();
                    assert_eq!(detail.get_selected_service().status, expected.status);
                    assert_eq!(
                        detail.get_selected_service().percent_text,
                        expected.percent_text
                    );
                    assert_eq!(
                        detail.get_selected_service().quota_details,
                        expected.quota_details
                    );
                }
                eprintln!("WACU_QA claude_model_pass");
            }
            if std::env::var_os("WACU_QA_CODEX_LIVE").is_some() {
                CODEX_USAGE.with_borrow(|state| {
                    assert!(state.error.is_none(), "Live Codex quota read failed");
                    assert!(
                        state
                            .snapshot
                            .as_ref()
                            .and_then(codex::Snapshot::main_window)
                            .is_some()
                    );
                });
                let expected = service(0, true);
                let capsule = c.upgrade().unwrap();
                let actual = (0..capsule.get_services().row_count())
                    .filter_map(|i| capsule.get_services().row_data(i))
                    .find(|item| item.source_index == 0)
                    .unwrap();
                assert_eq!(actual.percent_text, expected.percent_text);
                if mode == "quick" || mode == "detail" {
                    let detail = d.upgrade().unwrap();
                    assert_eq!(
                        detail.get_selected_service().percent_text,
                        expected.percent_text
                    );
                    assert_eq!(
                        detail.get_selected_service().quota_details,
                        expected.quota_details
                    );
                }
                eprintln!("WACU_QA codex_live_model_pass");
            }
            let snapshot = match mode.as_str() {
                "mini" => m
                    .upgrade()
                    .and_then(|window| window.window().take_snapshot().ok()),
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
        let motion = detail_motion.clone();
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
                    && !motion.closing.get()
                    && d.get_revealed()
                    && !platform_windows::is_foreground(d.window())
                    && !platform_windows::is_foreground(c.window())
                    && !platform_windows::cursor_is_over(d.window())
                    && !platform_windows::cursor_is_over(c.window())
                {
                    dismiss_detail(&d, &motion);
                    c.set_selected_index(-1);
                }
            }
            if !c.window().is_visible() {
                return;
            }
            // Mini mode expands only on click; never auto-dismiss the capsule.
            if mini_active {
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
    #[cfg(debug_assertions)]
    ENTRY_FRAMES.with(|frames| {
        for (name, frame) in frames.borrow_mut().drain(..) {
            qa_write_entry_frame(&frame, &name);
        }
    });
    if mini_active && persist_entry_preferences() {
        // Use the logical view saved at the start of a switch: both native
        // windows can still be visible when quitting during the animation.
        if mini.window().is_visible() {
            remember_mini(&mini, &preferences);
        }
        remember_position(&capsule, &preferences, edge_origin.get());
        if let Err(error) = state::save(&preferences.borrow()) {
            eprintln!("Saving entry session: {error}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_and_capsule_defaults_are_independent() {
        let area = placement::WorkArea {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        let mini_right = default_mini_position((66, 66), area, false);
        let mini_left = default_mini_position((66, 66), area, true);
        let capsule = pinned((96, 411), area);
        assert_eq!(mini_right, Point { x: -81, y: 959 });
        assert_eq!(mini_left, Point { x: -1905, y: 959 });
        assert_eq!(capsule, Point { x: -111, y: 314 });
        assert_ne!(mini_left.x, capsule.x);
        assert_ne!(mini_right.y, capsule.y);
    }

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
    fn fractional_arc_preserves_smooth_sweep_and_boundaries() {
        assert!(arc_percent(0.0).is_empty());
        assert!(arc_percent(f32::NAN).is_empty());
        assert!(arc_percent(0.5).as_str().ends_with("28.75 4.01"));
        assert!(arc_percent(25.0).as_str().ends_with("52.00 28.00"));
        assert_eq!(arc_percent(100.0), arc_svg(100));
    }

    #[test]
    fn moving_ring_trail_wraps_and_retains_clockwise_geometry() {
        assert_eq!(
            arc_trail(25.0, 25.0).as_str(),
            "M 52.00 28.00 A 24 24 0 0 1 28.00 52.00"
        );
        assert_eq!(arc_trail(90.0, 20.0), arc_trail(-10.0, 20.0));
        assert!(arc_trail(10.0, 0.0).is_empty());
        assert!(arc_trail(f32::NAN, 9.0).is_empty());
        assert!(arc_trail(10.0, 100.0).as_str().ends_with('Z'));
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
        for index in 2..3 {
            let item = service(index, true);
            let used: i32 = item.used.parse().unwrap();
            let remaining: i32 = item.remaining.parse().unwrap();
            assert_eq!(used + remaining, 1000);
            assert_eq!(item.percent, (remaining / 10) as f32);
        }
    }

    #[test]
    fn codex_ui_uses_real_window_and_never_demo_counts() {
        CODEX_USAGE.with_borrow_mut(|state| *state = codex::UsageState::default());
        assert_eq!(service(0, true).percent_text, "--");
        assert!(service(0, true).arc.is_empty());
        CODEX_USAGE.with_borrow_mut(|state| {
            state.apply(codex::parse_snapshot(serde_json::json!({
                "rateLimits":{"primary":{"usedPercent":12.5,"windowDurationMins":300},
                    "secondary":{"usedPercent":32,"windowDurationMins":10080}}
            })))
        });
        let item = service(0, true);
        assert_eq!(item.percent_text, "87.5%");
        assert_eq!(item.percent, 87.5);
        assert_eq!(item.remaining, "87.5%");
        assert_eq!(item.window_label, "5小时额度");
        assert_eq!(item.arc, arc_percent(87.5));
        assert!(item.quota_details.contains("每周额度"));
        CODEX_USAGE.with_borrow_mut(|state| state.apply(Err(codex::FetchError::Connection)));
        assert_eq!(service(0, true).percent_text, "87.5%");
        assert!(service(0, true).status.contains("保留上次数据"));
        CODEX_USAGE.with_borrow_mut(|state| state.apply(Err(codex::FetchError::Unauthorized)));
        assert_eq!(service(0, true).percent_text, "--");
        assert!(service(0, true).arc.is_empty());
        CODEX_USAGE.with_borrow_mut(|state| *state = codex::UsageState::default());
    }

    #[test]
    fn claude_ui_keeps_its_own_real_snapshot_and_clears_on_logout() {
        CLAUDE_USAGE.with_borrow_mut(|state| *state = claude::UsageState::default());
        assert_eq!(service(1, true).percent_text, "--");
        CLAUDE_USAGE.with_borrow_mut(|state| {
            state.apply(claude::parse_snapshot(serde_json::json!({
                "five_hour":{"utilization":12.5}, "seven_day":{"utilization":34}
            })))
        });
        let item = service(1, true);
        assert_eq!(item.percent, 87.5);
        assert_eq!(item.remaining, "87.5%");
        assert_eq!(item.window_label, "5小时额度");
        assert_eq!(item.arc, arc_percent(87.5));
        assert!(item.quota_details.contains("每周额度"));
        CLAUDE_USAGE.with_borrow_mut(|state| state.apply(Err(claude::FetchError::Timeout)));
        assert_eq!(service(1, true).percent_text, "87.5%");
        assert!(service(1, true).status.contains("保留上次数据"));
        CLAUDE_USAGE.with_borrow_mut(|state| state.apply(Err(claude::FetchError::Unauthorized)));
        assert_eq!(service(1, true).percent_text, "--");
        assert!(service(1, true).arc.is_empty());
        CLAUDE_USAGE.with_borrow_mut(|state| *state = claude::UsageState::default());
    }

    #[test]
    fn quota_rings_show_remaining_and_keep_unknown_data_distinct() {
        let mut item = service(2, true);
        for (used, remaining, text) in [
            (0.0, 100.0, "100%"),
            (12.5, 87.5, "87.5%"),
            (100.0, 0.0, "0%"),
        ] {
            let window = quota::QuotaWindow {
                used_percent: Some(used),
                window_duration_mins: Some(300),
                resets_at: None,
            };
            fill_quota_service(&mut item, "已连接".into(), Some(&window), None);
            assert_eq!(item.percent, remaining);
            assert_eq!(item.percent_text, text);
            assert_eq!(item.remaining, text);
            assert_eq!(item.used, format!("{used:.1}%"));
            assert_eq!(item.arc, arc_percent(remaining));
            if used == 0.0 {
                assert!(item.arc.ends_with('Z'));
            }
            if used == 100.0 {
                assert!(item.arc.is_empty());
            }
        }
        for used in [None, Some(f64::NAN)] {
            let window = quota::QuotaWindow {
                used_percent: used,
                window_duration_mins: Some(300),
                resets_at: None,
            };
            fill_quota_service(&mut item, "等待额度".into(), Some(&window), None);
            assert_eq!(item.percent_text, "--");
            assert_eq!(item.remaining, "--");
            assert!(item.arc.is_empty());
        }
    }
}
