//! Small Windows-specific window integration. Rendering and tray are handled by Slint.
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
        RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW,
        RegSetValueExW,
    },
    UI::Input::KeyboardAndMouse::ReleaseCapture,
    UI::Shell::ShellExecuteW,
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        HTCAPTION, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, WM_NCLBUTTONDOWN, WS_EX_APPWINDOW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    },
};

use crate::placement::{Point, WorkArea};

pub fn cursor() -> Point {
    let mut point = POINT { x: 0, y: 0 };
    // Failure is non-fatal: default to the primary monitor.
    unsafe {
        GetCursorPos(&mut point);
    }
    Point {
        x: point.x,
        y: point.y,
    }
}

pub fn work_area_at(point: Point) -> WorkArea {
    let monitor = unsafe {
        MonitorFromPoint(
            POINT {
                x: point.x,
                y: point.y,
            },
            MONITOR_DEFAULTTONEAREST,
        )
    };
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    let ok = unsafe { GetMonitorInfoW(monitor, &mut info) };
    if ok != 0 {
        WorkArea {
            left: info.rcWork.left,
            top: info.rcWork.top,
            right: info.rcWork.right,
            bottom: info.rcWork.bottom,
        }
    } else {
        WorkArea {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1000,
        }
    }
}

pub fn native_window(window: &slint::Window) -> Option<HWND> {
    let binding = window.window_handle();
    let handle = binding.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
        _ => None,
    }
}

pub fn activate(window: &slint::Window) {
    if let Some(hwnd) = native_window(window) {
        unsafe {
            SetForegroundWindow(hwnd);
        }
    }
}

pub fn is_foreground(window: &slint::Window) -> bool {
    let Some(hwnd) = native_window(window) else {
        return false;
    };
    unsafe { GetForegroundWindow() == hwnd }
}

pub fn cursor_is_over(window: &slint::Window) -> bool {
    let Some(hwnd) = native_window(window) else {
        return false;
    };
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return false;
    }
    let point = cursor();
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

pub fn configure_capsule(window: &slint::Window) {
    let Some(hwnd) = native_window(window) else {
        return;
    };
    unsafe {
        let existing = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            (existing | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize),
        );
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );
    }
}

pub fn configure_transit(window: &slint::Window) {
    configure_capsule(window);
    if let Some(hwnd) = native_window(window) {
        unsafe {
            let existing = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                existing | WS_EX_NOACTIVATE as isize | WS_EX_TRANSPARENT as isize,
            );
        }
    }
}

pub fn begin_drag(window: &slint::Window) {
    let Some(hwnd) = native_window(window) else {
        return;
    };
    unsafe {
        ReleaseCapture();
        SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
    }
    release_drag_input(window);
}

pub fn release_drag_input(window: &slint::Window) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let cursor = cursor();
    let origin = window.position();
    let scale = window.scale_factor();
    let position = slint::LogicalPosition::new(
        (cursor.x - origin.x) as f32 / scale,
        (cursor.y - origin.y) as f32 / scale,
    );
    // The OS caption drag consumes mouse-up; release Slint's TouchArea grab too.
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved { position });
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

fn startup_command() -> Result<Vec<u16>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(wide(&format!("\"{}\"", exe.display())))
}

// Read the OS entry, rather than treating a saved checkbox as evidence of startup.
pub fn startup_enabled() -> bool {
    let Ok(expected) = startup_command() else {
        return false;
    };
    let mut key: HKEY = std::ptr::null_mut();
    unsafe {
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            KEY_READ,
            &mut key,
        ) != 0
        {
            return false;
        }
        let mut contents = vec![0u16; 32768];
        let mut size = (contents.len() * 2) as u32;
        let status = RegGetValueW(
            key,
            std::ptr::null(),
            wide("Wacu").as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            contents.as_mut_ptr().cast(),
            &mut size,
        );
        RegCloseKey(key);
        status == 0 && contents[..(size as usize / 2).min(contents.len())] == expected
    }
}

pub fn set_startup(enabled: bool) -> Result<(), String> {
    let mut key: HKEY = std::ptr::null_mut();
    let command = startup_command()?;
    unsafe {
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        );
        if status != 0 {
            return Err(format!("无法打开启动设置（Windows {status}）"));
        }
        let status = if enabled {
            RegSetValueExW(
                key,
                wide("Wacu").as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, wide("Wacu").as_ptr())
        };
        RegCloseKey(key);
        // ERROR_FILE_NOT_FOUND means the disabled entry is already absent.
        if status == 0 || (!enabled && status == 2) {
            Ok(())
        } else {
            Err(format!("无法修改开机启动（Windows {status}）"))
        }
    }
}

pub fn open_url(url: &str) -> Result<(), String> {
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            wide("open").as_ptr(),
            wide(url).as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err(format!("无法打开浏览器（Windows {result}）"))
    }
}
