// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let dev_mode = std::env::args().any(|a| a == "--dev");
    ha_companion_lib::run(dev_mode);
}
