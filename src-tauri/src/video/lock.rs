use crate::video::types::VideoError;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

pub struct ProcessingLock {
    lock_file: PathBuf,
}

impl ProcessingLock {
    pub fn acquire(app: &AppHandle, input_path: &str) -> Result<Self, VideoError> {
        let input_stem = Path::new(input_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| VideoError::InvalidInput("Invalid input filename".into()))?;

        let lock_dir = get_lock_dir(app)?;
        if !lock_dir.exists() {
            fs::create_dir_all(&lock_dir)?;
        }

        let lock_file = lock_dir.join(format!("{}.processing", input_stem));

        if lock_file.exists() {
            return Err(VideoError::AlreadyProcessing(input_stem.to_string()));
        }

        fs::write(&lock_file, "1")?;

        Ok(Self { lock_file })
    }

    pub fn release(self) -> Result<(), VideoError> {
        if self.lock_file.exists() {
            fs::remove_file(&self.lock_file)?;
        }
        Ok(())
    }
}

impl Drop for ProcessingLock {
    fn drop(&mut self) {
        if self.lock_file.exists() {
            let _ = fs::remove_file(&self.lock_file);
        }
    }
}

fn get_lock_dir(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let runtime = crate::runtime_paths::RuntimePaths::from_app(app)?;
    Ok(runtime.temp_dir().join("locks"))
}

/// Automatically cleans up all stale .processing lock files.
/// Should be called during application startup.
pub fn cleanup_stale_locks(app: &AppHandle) -> Result<(), VideoError> {
    let lock_dir = get_lock_dir(app)?;
    if !lock_dir.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(lock_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("processing") {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}
