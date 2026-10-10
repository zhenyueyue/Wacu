mod placement;

use std::{fs, sync::Mutex};

use placement::{Bounds, Point, clamp_to_work_area, default_pinned_position, popup_position};
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewWindow, WindowEvent,
};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum DisplayMode {
    #[default]
    Popup,
    Pinned,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct WindowPreferences {
    mode: DisplayMode,
    pinned_position: Option<SavedPosition>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct SavedPosition {
    x: i32,
    y: i32,
}

impl From<SavedPosition> for Point {
    fn from(value: SavedPosition) -> Self {
        Self { x: value.x, y: value.y }
    }
}

impl From<Point> for SavedPosition {
    fn from(value: Point) -> Self {
        Self { x: value.x, y: value.y }
    }
}

struct WindowState {
    prefs: Mutex<WindowPreferences>,
    selected_service: Mutex<String>,
}

fn config_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("display-mode.json"))
        .map_err(|error| error.to_string())
}

fn load_preferences(app: &AppHandle) -> WindowPreferences {
    config_path(app)
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn persist_preferences(app: &AppHandle, prefs: WindowPreferences) -> Result<(), String> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, serde_json::to_vec(&prefs).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn preferences(app: &AppHandle) -> WindowPreferences {
    *app.state::<WindowState>().prefs.lock().expect("prefs mutex")
}

fn current_mode(app: &AppHandle) -> DisplayMode {
    preferences(app).mode
}

fn monitor_for_point(window: &WebviewWindow, point: Point) -> Option<tauri::window::Monitor> {
    window
        .available_monitors()
        .ok()?
        .into_iter()
        .find(|monitor| {
            let pos = monitor.position();
            let size = monitor.size();
            Bounds {
                left: pos.x, top: pos.y,
                width: size.width as i32, height: size.height as i32,
            }
            .contains(point)
        })
}

fn preferred_monitor(window: &WebviewWindow, point: Option<Point>) -> Option<tauri::window::Monitor> {
    point
        .and_then(|position| monitor_for_point(window, position))
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| window.current_monitor().ok().flatten())
}

fn work_area(monitor: &tauri::window::Monitor) -> Bounds {
    let work = monitor.work_area();
    Bounds {
        left: work.position.x,
        top: work.position.y,
        width: work.size.width as i32,
        height: work.size.height as i32,
    }
}

fn position_capsule(app: &AppHandle, anchor: Option<Point>) {
    let Some(window) = app.get_webview_window("capsule") else { return };
    let Ok(size) = window.outer_size() else { return };
    let dimensions = (size.width as i32, size.height as i32);
    let prefs = preferences(app);
    let saved = prefs.pinned_position.map(Point::from);
    let target = if prefs.mode == DisplayMode::Pinned {
        saved.map(|position| Point {
            x: position.x.saturating_add(dimensions.0 / 2),
            y: position.y.saturating_add(dimensions.1 / 2),
        })
    } else {
        anchor
    };
    let Some(monitor) = preferred_monitor(&window, target) else { return };
    let work = work_area(&monitor);
    let position = match prefs.mode {
        DisplayMode::Popup => popup_position(work, dimensions, anchor),
        DisplayMode::Pinned => saved.map(|position| clamp_to_work_area(position, dimensions, work))
            .unwrap_or_else(|| default_pinned_position(work, dimensions)),
    };
    let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
}

fn toggle_capsule_for(app: &AppHandle, anchor: Option<Point>) {
    if let Some(window) = app.get_webview_window("capsule") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            if current_mode(app) == DisplayMode::Popup {
                position_capsule(app, anchor);
            }
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn show_settings_for(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn get_display_mode(app: AppHandle) -> DisplayMode {
    current_mode(&app)
}

#[tauri::command]
fn set_display_mode(app: AppHandle, mode: DisplayMode) -> Result<(), String> {
    let mut next = preferences(&app);
    next.mode = mode;
    persist_preferences(&app, next)?;
    *app.state::<WindowState>().prefs.lock().map_err(|error| error.to_string())? = next;
    if mode == DisplayMode::Pinned {
        position_capsule(&app, None);
        if let Some(window) = app.get_webview_window("capsule") {
            let _ = window.show();
        }
    } else if let Some(window) = app.get_webview_window("capsule") {
        let _ = window.hide();
    }
    if let Some(detail) = app.get_webview_window("detail") {
        let _ = detail.hide();
    }
    app.emit("wacu:mode-changed", mode).map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_capsule_position(app: AppHandle) -> Result<(), String> {
    let mut next = preferences(&app);
    next.pinned_position = None;
    persist_preferences(&app, next)?;
    *app.state::<WindowState>().prefs.lock().map_err(|error| error.to_string())? = next;
    position_capsule(&app, None);
    Ok(())
}

#[tauri::command]
fn toggle_capsule(app: AppHandle) {
    toggle_capsule_for(&app, None);
}

#[tauri::command]
fn show_settings(app: AppHandle) {
    show_settings_for(&app);
}

#[tauri::command]
fn show_detail(app: AppHandle, state: State<'_, WindowState>, service_id: String) -> Result<(), String> {
    if !matches!(service_id.as_str(), "codex" | "claude" | "workbuddy") {
        return Err("unknown service".into());
    }
    *state.selected_service.lock().map_err(|error| error.to_string())? = service_id.clone();
    app.emit("wacu:detail-service", service_id).map_err(|error| error.to_string())?;
    if let (Some(capsule), Some(detail)) = (
        app.get_webview_window("capsule"),
        app.get_webview_window("detail"),
    ) {
        if let (Ok(origin), Ok(detail_size), Ok(Some(monitor))) =
            (capsule.outer_position(), detail.outer_size(), capsule.current_monitor())
        {
            let work = work_area(&monitor);
            let width = detail_size.width as i32;
            let right_of_capsule = origin.x + capsule.outer_size().map(|s| s.width as i32).unwrap_or(114) + 12;
            let left_of_capsule = origin.x - width - 12;
            let target_x = if left_of_capsule >= work.left {
                left_of_capsule
            } else {
                right_of_capsule
            };
            let position = clamp_to_work_area(
                Point { x: target_x, y: origin.y },
                (width, detail_size.height as i32),
                work,
            );
            let _ = detail.set_position(PhysicalPosition::new(position.x, position.y));
        }
        let _ = detail.show();
        let _ = detail.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn get_selected_service(state: State<'_, WindowState>) -> String {
    state.selected_service.lock().expect("service mutex").clone()
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(WindowState {
                prefs: Mutex::new(load_preferences(app.handle())),
                selected_service: Mutex::new("codex".into()),
            });

            // Popup windows must never appear until the user opens the tray entry.
            let _ = app.get_webview_window("capsule").map(|window| window.hide());
            if current_mode(app.handle()) == DisplayMode::Pinned {
                position_capsule(app.handle(), None);
                if let Some(capsule) = app.get_webview_window("capsule") {
                    let _ = capsule.show();
                }
            }

            let toggle = MenuItem::with_id(app, "toggle", "显示 / 隐藏 Wacu", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let reset = MenuItem::with_id(app, "reset", "重置悬浮位置", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 Wacu", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle, &settings, &reset, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("bundled icon").clone())
                .tooltip("Wacu · AI 用量守望")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "toggle" => toggle_capsule_for(app, None),
                    "settings" => show_settings_for(app),
                    "reset" => { let _ = reset_capsule_position(app.clone()); }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        position,
                        ..
                    } = event
                    {
                        toggle_capsule_for(
                            tray.app_handle(),
                            Some(Point { x: position.x.round() as i32, y: position.y.round() as i32 }),
                        );
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                WindowEvent::Moved(position) if window.label() == "capsule" => {
                    if current_mode(window.app_handle()) == DisplayMode::Pinned
                        && window.is_visible().unwrap_or(false)
                    {
                        let app = window.app_handle();
                        let mut prefs = preferences(app);
                        let next = SavedPosition { x: position.x, y: position.y };
                        if prefs.pinned_position.map(|p| (p.x, p.y)) != Some((next.x, next.y)) {
                            prefs.pinned_position = Some(next);
                            *app.state::<WindowState>().prefs.lock().expect("prefs mutex") = prefs;
                            let _ = persist_preferences(app, prefs);
                        }
                    }
                }
                WindowEvent::Focused(false) => {
                    if (window.label() == "capsule" && current_mode(window.app_handle()) == DisplayMode::Popup)
                        || window.label() == "detail"
                    {
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_display_mode,
            set_display_mode,
            reset_capsule_position,
            toggle_capsule,
            show_settings,
            show_detail,
            get_selected_service,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Wacu");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_remain_readable() {
        let prefs: WindowPreferences = serde_json::from_str(r#"{ "mode": "pinned" }"#).unwrap();
        assert_eq!(prefs.mode, DisplayMode::Pinned);
        assert!(prefs.pinned_position.is_none());
    }
}
