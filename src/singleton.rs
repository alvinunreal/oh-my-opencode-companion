use std::path::PathBuf;

fn lock_path() -> PathBuf {
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
        .join("companion.pid")
}

/// Returns true if this process should continue running.
/// Returns false if another companion instance is already alive.
pub fn acquire() -> bool {
    let path = lock_path();

    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            if pid != std::process::id() && is_alive(pid) {
                return false;
            }
        }
    }

    let _ = std::fs::write(&path, std::process::id().to_string());
    true
}

#[cfg(unix)]
fn is_alive(pid: u32) -> bool {
    // kill -0 checks if the process exists without sending a signal
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn is_alive(_pid: u32) -> bool {
    false
}
