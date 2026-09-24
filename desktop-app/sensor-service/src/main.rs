#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod driver_install;
#[cfg(windows)]
mod service;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--install") => service::install(false),
        Some("--install-with-driver") => service::install(true),
        Some("--refresh-existing") => service::refresh_existing(),
        Some("--uninstall") => service::uninstall(),
        _ => Ok(service::run()?),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The integrated sensor service is Windows-only");
}
