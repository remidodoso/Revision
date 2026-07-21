//! Where the log file lives.
//!
//! Resolved from the environment rather than from a crate: the three platform
//! conventions are four lines each, and the dependency-free posture (coding
//! standard) is worth more here than the handful of edge cases a directories
//! crate would additionally cover.

use std::path::PathBuf;

use crate::error::LogError;

/// The application-data directory for Revision, created if absent.
///
/// - Windows: `%LOCALAPPDATA%\Revision`
/// - macOS: `~/Library/Application Support/Revision`
/// - Linux and other unix: `$XDG_DATA_HOME/revision`, else `~/.local/share/revision`
pub fn data_directory() -> Result<PathBuf, LogError> {
    let directory = platform_directory()?;
    std::fs::create_dir_all(&directory).map_err(|source| LogError::Directory {
        path: directory.display().to_string(),
        source,
    })?;
    Ok(directory)
}

/// The default log file: `<data directory>/observation.revlog`.
///
/// The extension is ours rather than `.sqlite` so a user who finds the file
/// learns what it is, and so that "delete my logs" cannot glob a project.
pub fn default_log_path() -> Result<PathBuf, LogError> {
    Ok(data_directory()?.join("observation.revlog"))
}

#[cfg(windows)]
fn platform_directory() -> Result<PathBuf, LogError> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or(LogError::NoHome("LOCALAPPDATA is unset"))?;
    Ok(PathBuf::from(base).join("Revision"))
}

#[cfg(target_os = "macos")]
fn platform_directory() -> Result<PathBuf, LogError> {
    let home = std::env::var_os("HOME").ok_or(LogError::NoHome("HOME is unset"))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Revision"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_directory() -> Result<PathBuf, LogError> {
    // XDG first, and only when absolute — the specification says a relative
    // value is to be ignored as invalid.
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Ok(path.join("revision"));
        }
    }
    let home = std::env::var_os("HOME").ok_or(LogError::NoHome("HOME is unset"))?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("revision"))
}
