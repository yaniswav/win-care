use crate::types::{LogEntry, LogLevel, WorkerEvent};
use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// Détermine et crée le dossier dédié aux fichiers journaux (`logs/`).
/// - Si l'exécutable se trouve dans un dossier dédié (hors `Desktop` et hors `target\`), utilise `<dossier_exe>\logs`.
/// - Sinon, utilise `%LOCALAPPDATA%\WinCare\logs`.
pub fn resolve_logs_dir() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let parent_str = parent.to_string_lossy().to_lowercase();
            let is_desktop = parent_str.ends_with("\\desktop") || parent_str.ends_with("/desktop");
            let is_cargo_target =
                parent_str.contains("\\target\\debug") || parent_str.contains("\\target\\release");

            if !is_desktop && !is_cargo_target {
                let dir = parent.join("logs");
                if fs::create_dir_all(&dir).is_ok() {
                    return dir;
                }
            }
        }
    }

    let base = std::env::var("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("WinCare").join("logs"))
        .unwrap_or_else(|_| PathBuf::from("logs"));
    let _ = fs::create_dir_all(&base);
    base
}

/// Conserve uniquement les `max_files` fichiers `.log` les plus récents dans `log_dir`.
fn rotate_old_logs(log_dir: & std::path::Path, max_files: usize) {
    let Ok(entries) = fs::read_dir(log_dir) else {
        return;
    };
    let mut logs: Vec<(PathBuf, std::time::SystemTime)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                let mtime = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                Some((path, mtime))
            } else {
                None
            }
        })
        .collect();

    if logs.len() > max_files {
        logs.sort_by_key(|(_, t)| *t);
        let to_remove = logs.len() - max_files;
        for (old_path, _) in logs.into_iter().take(to_remove) {
            let _ = fs::remove_file(old_path);
        }
    }
}

#[derive(Clone)]
pub struct Logger {
    tx: Option<Sender<WorkerEvent>>,
    log_file: Arc<Mutex<Option<File>>>,
    pub log_path: PathBuf,
    console_echo: bool,
}

impl Logger {
    pub fn new(tx: Option<Sender<WorkerEvent>>, console_echo: bool) -> Self {
        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let log_dir = resolve_logs_dir();
        let log_path = log_dir.join(format!("WinCare_{timestamp}.log"));

        let file = if cfg!(test) {
            None
        } else {
            rotate_old_logs(&log_dir, 25);
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .ok()
        };

        Self {
            tx,
            log_file: Arc::new(Mutex::new(file)),
            log_path,
            console_echo,
        }
    }

    pub fn send_event(&self, event: WorkerEvent) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(event);
        }
    }

    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        let msg = message.into();
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let formatted = format!("[{timestamp}] [{}] {msg}", level.as_str());

        if self.console_echo {
            let _ = writeln!(std::io::stdout(), "{formatted}");
        }

        let mut guard = self.log_file.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(f) = guard.as_mut() {
            let _ = writeln!(f, "{formatted}");
            let _ = f.flush();
        }

        let entry = LogEntry {
            timestamp,
            level,
            message: msg,
        };
        self.send_event(WorkerEvent::Log(entry));
    }

    pub fn info(&self, msg: impl Into<String>) {
        self.log(LogLevel::Info, msg);
    }

    pub fn success(&self, msg: impl Into<String>) {
        self.log(LogLevel::Success, msg);
    }

    pub fn warn(&self, msg: impl Into<String>) {
        self.log(LogLevel::Warning, msg);
    }

    pub fn error(&self, msg: impl Into<String>) {
        self.log(LogLevel::Error, msg);
    }

    pub fn section(&self, title: impl Into<String>) {
        let t = title.into();
        self.log(LogLevel::Section, format!("========== {t} =========="));
    }

    pub fn progress(&self, current: usize, total: usize, label: impl Into<String>) {
        self.send_event(WorkerEvent::Progress {
            current,
            total,
            label: label.into(),
        });
    }
}
