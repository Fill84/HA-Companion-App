//! Release-mode file logger.
//!
//! Writes to `%APPDATA%\com.ha-companion.desktop\app.log` on Windows.
//! Falls back to `<cwd>/app.log` if `APPDATA` isn't set (CI / Linux dev).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};

const APP_DATA_DIR: &str = "com.ha-companion.desktop";
const LOG_FILE_NAME: &str = "app.log";
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
const BACKUPS: usize = 3;

/// Compute the on-disk log file path. Uses `%APPDATA%` on Windows.
pub fn log_file_path() -> std::io::Result<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs"));
    #[cfg(target_os = "linux")]
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")));
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    let base = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"));
    let base = base
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "App data directory unavailable"))?;
    Ok(base.join(APP_DATA_DIR).join(LOG_FILE_NAME))
}

struct RotatingWriter {
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
    limit: u64,
}

impl RotatingWriter {
    fn new(path: &Path, limit: u64) -> io::Result<Self> {
        let file = make_log_file(path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            path: path.to_path_buf(),
            file: Some(file),
            bytes,
            limit,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        drop(self.file.take());
        for index in (1..BACKUPS).rev() {
            let from = self.path.with_extension(format!("log.{index}"));
            let to = self.path.with_extension(format!("log.{}", index + 1));
            if from.exists() {
                if to.exists() {
                    fs::remove_file(&to)?;
                }
                fs::rename(from, to)?;
            }
        }
        if self.path.exists() {
            fs::rename(&self.path, self.path.with_extension("log.1"))?;
        }
        self.file = Some(make_log_file(&self.path)?);
        self.bytes = 0;
        Ok(())
    }
}

impl Write for RotatingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.bytes > 0 && self.bytes.saturating_add(buffer.len() as u64) > self.limit {
            self.rotate()?;
        }
        let written = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("Log file unavailable"))?
            .write(buffer)?;
        self.bytes += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("Log file unavailable"))?
            .flush()
    }
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
    let file = RotatingWriter::new(path, MAX_LOG_BYTES)?;
    let config = ConfigBuilder::new().set_time_format_rfc3339().build();
    WriteLogger::init(LevelFilter::Info, config, file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn make_log_file_creates_parent_dirs() {
        let base = std::env::temp_dir().join(format!("ha-companion-test-{}", std::process::id()));
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

    #[test]
    fn rotating_writer_bounds_files_and_preserves_new_entries() {
        let base = std::env::temp_dir().join(format!("ha-companion-rotate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let path = base.join("app.log");
        let mut writer = RotatingWriter::new(&path, 5).unwrap();
        writer.write_all(b"one\n").unwrap();
        writer.write_all(b"two\n").unwrap();
        writer.flush().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two\n");
        assert_eq!(fs::read(path.with_extension("log.1")).unwrap(), b"one\n");
        drop(writer);
        let _ = fs::remove_dir_all(&base);
    }
}
