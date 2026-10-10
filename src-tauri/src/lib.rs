use std::{fs, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, State, WindowEvent,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum DisplayMode {
    Popup,
    Pinned,
}

struct WindowState {
    mode: Mutex<DisplayMode>,
    selected_service: Mutex<String>,
}

fn mode_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("display-mode.json"))
        .map_err(|error| error.to_string())
}

fn load_mode(app: &AppHandle) -> DisplayMode {
    #[derive(Deserialize)]
    struct SavedMode {
        mode: DisplayMode,
    }

    mode_path(app)
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|json| serde_json::from_str::<SavedMode>(&json).ok())
        .map(|saved| saved.mode)
        .unwrap_or(DisplayMode::Popup)
}

fn persist_mode(app: &AppHandle, mode: DisplayMode) -> Result<(), String> {
    let path = mode_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        path,
        serde_json::json!({ "mode": mode }).to_string(),
    )
    .map_err(|error| error.to_string())
}

fn current_mode(app: &AppHandle) -> DisplayMode {
    *app.state::<WindowState>().mode.lock().expect("mode mutex")
}

fn position_capsule(app: &AppHandle) {
    let Some(window) = app.get_webview_window("capsule") else {
        return;
    };
    if let (Ok(Some(monitor)), Ok(size)) = (window.current_monitor(), window.outer_size()) {
        let screen = monitor.size();
        let origin = monitor.position();
        let x = origin.x + screen.width as i32 - size.width as i32 - 22;
        let y = match current_mode(app) {
            DisplayMode::Popup => origin.y + screen.height as i32 - size.height as i32 - 86,
            DisplayMode::Pinned => origin.y + (screen.height as i32 - size.height as i32) / 2,
        };
        let _ = window.set_position(PhysicalPosition::new(x.max(origin.x), y.max(origin.y)));
    }
}

fn toggle_capsule_for(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("capsule") {
        let visible = window.is_visible().unwrap_or(false);
        if visible {
            let _ = window.hide();
        } else {
            if current_mode(app) == DisplayMode::Popup {
                position_capsule(app);
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
fn set_display_mode(app: AppHandle, state: State<'_, WindowState>, mode: DisplayMode) -> Result<(), String> {
    persist_mode(&app, mode)?;
    *state.mode.lock().map_err(|error| error.to_string())? = mode;
    position_capsule(&app);
    app.emit("wacu:mode-changed", mode).map_err(|error| error.to_string())?;
    if mode == DisplayMode::Pinned {
        if let Some(window) = app.get_webview_window("capsule") {
            let _ = window.show();
        }
    }
    Ok(())
}

#[tauri::command]
fn toggle_capsule(app: AppHandle) {
    toggle_capsule_for(&app);
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
            let edge = monitor.position().x + monitor.size().width as i32;
            let x = if origin.x - detail_size.width as i32 - 14 < monitor.position().x {
                (origin.x + 130).min(edge - detail_size.width as i32 - 12)
            } else {
                origin.x - detail_size.width as i32 - 14
            };
            let _ = detail.set_position(PhysicalPosition::new(x, origin.y));
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
                mode: Mutex::new(load_mode(app.handle())),
                selected_service: Mutex::new("codex".into()),
            });
            position_capsule(app.handle());

            let toggle = MenuItem::with_id(app, "toggle", "显示 / 隐藏 Wacu", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 Wacu", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle, &settings, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("bundled icon").clone())
                .tooltip("Wacu · AI 用量守望")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "toggle" => toggle_capsule_for(app),
                    "settings" => show_settings_for(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_capsule_for(tray.app_handle());
                    }
                })
                .build(app)?;

            if current_mode(app.handle()) == DisplayMode::Pinned {
                if let Some(capsule) = app.get_webview_window("capsule") {
                    let _ = capsule.show();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            if let WindowEvent::Focused(false) = event {
                if (window.label() == "capsule" && current_mode(window.app_handle()) == DisplayMode::Popup)
                    || window.label() == "detail"
                {
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_display_mode,
            set_display_mode,
            toggle_capsule,
            show_settings,
            show_detail,
            get_selected_service,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Wacu");
}
