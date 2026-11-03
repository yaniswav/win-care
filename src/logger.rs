use crate::types::{LogEntry, LogLevel, WorkerEvent};
use chrono::Local;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

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
        let desktop = std::env::var("USERPROFILE")
            .map(|p| PathBuf::from(p).join("Desktop"))
            .unwrap_or_else(|_| PathBuf::from("."));

        let log_dir = if desktop.exists() {
            desktop
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        let log_path = log_dir.join(format!("WinCare_{timestamp}.log"));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();

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

        if let Ok(mut guard) = self.log_file.lock() {
            if let Some(f) = guard.as_mut() {
                let _ = writeln!(f, "{formatted}");
                let _ = f.flush();
            }
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
