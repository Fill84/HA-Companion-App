// The desktop app logs to a file, including in debug builds.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let dev_mode = std::env::args().any(|a| a == "--dev");
    ha_companion_lib::run(dev_mode);
}
