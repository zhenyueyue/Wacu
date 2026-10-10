// Prevents an extra console window when launched on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    wacu_lib::run()
}
