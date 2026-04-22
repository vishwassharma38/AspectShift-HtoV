use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub struct OsUtils;

impl OsUtils {
    /// Escapes a path for use inside FFmpeg filter strings (e.g., subtitles, drawtext).
    /// FFmpeg filters use ':' as a separator, which conflicts with Windows drive letters.
    pub fn escape_filter_path(path: &str) -> String {
        #[cfg(target_os = "windows")]
        {
            // Windows: Replace \ with / and escape the : in C:\
            path.replace('\\', "/")
                .replace(':', "\\:")
                .replace('\'', "\\'")
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Unix: Just escape : and '
            path.replace(':', "\\:")
                .replace('\'', "\\'")
        }
    }

    /// Returns a platform-agnostic temporary directory using Tauri APIs.
    pub fn get_temp_dir(app: &AppHandle) -> PathBuf {
        app.path().temp_dir().unwrap_or_else(|_| std::env::temp_dir())
    }

    /// Checks if a file is likely locked by another process (Windows-centric check).
    pub fn is_file_locked(path: &Path) -> bool {
        #[cfg(target_os = "windows")]
        {
            // On Windows, we try to open with write access to check for mandatory locks
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .is_err()
        }
        #[cfg(not(target_os = "windows"))]
        {
            // On Unix, file locking is advisory; usually just check if it's readable
            !path.exists() || std::fs::File::open(path).is_err()
        }
    }
}
