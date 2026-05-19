use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub struct OsUtils;

impl OsUtils {
    pub const SUPPORTED_VIDEO_EXTENSIONS: [&'static str; 5] = ["mp4", "mov", "mkv", "avi", "webm"];

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
            path.replace(':', "\\:").replace('\'', "\\'")
        }
    }

    /// Returns a platform-agnostic temporary directory using Tauri APIs.
    pub fn get_temp_dir(app: &AppHandle) -> PathBuf {
        if let Ok(runtime) = crate::runtime_paths::RuntimePaths::from_app(app) {
            let dir = runtime.temp_dir();
            let _ = crate::runtime_paths::ensure_dir_exists(&dir);
            return dir;
        }
        app.path()
            .temp_dir()
            .unwrap_or_else(|_| std::env::temp_dir())
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

    /// Sanitizes a string for use as a single path component (folder or filename).
    /// Replaces all non-alphanumeric characters (except _) with _.
    pub fn sanitize_path_component(input: &str) -> String {
        let sanitized: String = input
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        // Trim redundant underscores
        let mut result = String::new();
        let mut last_was_underscore = false;
        for c in sanitized.chars() {
            if c == '_' {
                if !last_was_underscore {
                    result.push(c);
                    last_was_underscore = true;
                }
            } else {
                result.push(c);
                last_was_underscore = false;
            }
        }
        result.to_lowercase().trim_matches('_').to_string()
    }

    /// Returns true when the path has a supported video extension.
    pub fn has_supported_video_extension(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                OsUtils::SUPPORTED_VIDEO_EXTENSIONS
                    .iter()
                    .any(|supported| ext.eq_ignore_ascii_case(supported))
            })
            .unwrap_or(false)
    }
}
