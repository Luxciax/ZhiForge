use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::State;

pub struct DiagnosticLog {
    path: PathBuf,
    lock: Mutex<()>,
}

impl DiagnosticLog {
    pub fn new_default() -> Result<Self, String> {
        let root = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "APPDATA is unavailable".to_string())?;
        Ok(Self {
            path: root.join("ZhiForge").join("logs").join("app.log"),
            lock: Mutex::new(()),
        })
    }

    pub fn record(&self, event: &str, detail: &str) {
        let Ok(_guard) = self.lock.lock() else {
            return;
        };
        let Some(parent) = self.path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.path) else {
            return;
        };
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let event = sanitize(event, 80);
        let detail = sanitize(detail, 600);
        let _ = writeln!(file, "{timestamp_ms}\t{event}\t{detail}");
    }

    pub fn path_string(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

fn sanitize(value: &str, max_chars: usize) -> String {
    value
        .replace(['\r', '\n', '\t'], " ")
        .chars()
        .take(max_chars)
        .collect()
}

#[tauri::command]
pub fn diagnostics_log_path(state: State<'_, DiagnosticLog>) -> String {
    state.path_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_keeps_log_single_line_and_bounded() {
        let text = sanitize("a\nb\tc", 4);
        assert_eq!(text, "a b ");
    }
}
