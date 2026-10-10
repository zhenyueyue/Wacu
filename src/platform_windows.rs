//! Small Windows-specific window integration. Rendering and tray are handled by Slint.
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    UI::Input::KeyboardAndMouse::ReleaseCapture,
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        HTCAPTION, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, WM_NCLBUTTONDOWN, WS_EX_APPWINDOW,
        WS_EX_TOOLWINDOW,
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

pub fn begin_drag(window: &slint::Window) {
    let Some(hwnd) = native_window(window) else {
        return;
    };
    unsafe {
        ReleaseCapture();
        SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
    }
}
