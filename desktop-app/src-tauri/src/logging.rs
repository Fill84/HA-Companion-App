//! Release-mode file logger.
//!
//! Writes to `%APPDATA%\com.ha-companion.desktop\app.log` on Windows.
//! Falls back to `<cwd>/app.log` if `APPDATA` isn't set (CI / Linux dev).

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};

const APP_DATA_DIR: &str = "com.ha-companion.desktop";
const LOG_FILE_NAME: &str = "app.log";

/// Compute the on-disk log file path. Uses `%APPDATA%` on Windows.
pub fn log_file_path() -> std::io::Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "APPDATA not set")
    })?;
    Ok(Path::new(&appdata).join(APP_DATA_DIR).join(LOG_FILE_NAME))
}

/// Open (or create) the log file in append mode, creating parent directories
/// as needed. Exposed so tests can verify file/dir creation independently of
/// global logger initialisation (which can only run once per process).
pub fn make_log_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// Initialise the global logger to write to `path`. Returns Err if a logger
/// is already installed or the file can't be opened.
pub fn init_logger(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = make_log_file(path)?;
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .build();
    WriteLogger::init(LevelFilter::Info, config, file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn make_log_file_creates_parent_dirs() {
        let base = std::env::temp_dir()
            .join(format!("ha-companion-test-{}", std::process::id()));
        let log = base.join("nested").join("a").join("app.log");
        let _ = fs::remove_dir_all(&base);

        let file = make_log_file(&log).expect("create");
        drop(file);
        assert!(log.exists(), "log file must exist at {:?}", log);

        fs::write(&log, b"existing\n").expect("seed content");
        let _file2 = make_log_file(&log).expect("re-open");
        let content = fs::read_to_string(&log).expect("read");
        assert!(content.contains("existing"), "must append, not truncate");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn log_file_path_under_appdata_uses_app_identifier() {
        let prev = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", r"C:\Users\test\AppData\Roaming");

        let path = log_file_path().expect("must compute path");
        assert!(
            path.ends_with(r"com.ha-companion.desktop\app.log"),
            "unexpected path: {:?}",
            path
        );

        match prev {
            Some(v) => std::env::set_var("APPDATA", v),
            None => std::env::remove_var("APPDATA"),
        }
    }
}
