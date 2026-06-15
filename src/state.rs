use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionConfigState {
    pub enabled: bool,
    pub position: String,
    pub size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompanionState {
    pub version: u32,
    #[serde(default)]
    pub sessions: Vec<SessionInfo>,
    #[serde(default)]
    pub config: Option<CompanionConfigState>,
    #[serde(default)]
    pub window_positions: BTreeMap<String, WindowPositionState>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct WindowPositionState {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub cwd: String,
    #[serde(default)]
    pub active_agents: Vec<String>,
    #[serde(default)]
    pub active_agent: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub pid: Option<u32>,
}

pub fn state_file_path() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local")
                .join("share")
        });
    base.join("opencode")
        .join("storage")
        .join("oh-my-opencode-slim")
        .join("companion-state.json")
}

pub fn read_state(path: &std::path::Path) -> CompanionState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_project_window_position(
    path: &std::path::Path,
    project: &str,
    position: WindowPositionState,
) -> std::io::Result<()> {
    if project.trim().is_empty() || !position.x.is_finite() || !position.y.is_finite() {
        return Ok(());
    }

    let mut state = read_state(path);
    state.window_positions.insert(project.to_string(), position);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string(&state).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// Starts a background thread that polls the state file for changes.
/// Returns a receiver that fires whenever the file content changes.
pub fn start_watcher(path: PathBuf) -> Receiver<()> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || poll_loop(path, tx));
    rx
}

fn poll_loop(path: PathBuf, tx: Sender<()>) {
    let mut last_mtime: Option<std::time::SystemTime> = None;
    loop {
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(mtime) = meta.modified() {
                if Some(mtime) != last_mtime {
                    last_mtime = Some(mtime);
                    if tx.send(()).is_err() {
                        return;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
