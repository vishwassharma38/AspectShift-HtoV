use crate::video::types::VideoError;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

pub struct ProcessingLock {
    lock_file: PathBuf,
}

impl ProcessingLock {
    pub fn acquire(
        app: &AppHandle,
        input_path: &str,
        target_identifier: &str,
    ) -> Result<Self, VideoError> {
        let lock_dir = get_lock_dir(app)?;
        Self::acquire_in_dir(lock_dir, input_path, target_identifier)
    }

    fn acquire_in_dir(
        lock_dir: PathBuf,
        input_path: &str,
        target_identifier: &str,
    ) -> Result<Self, VideoError> {
        let identity = lock_identity(input_path, target_identifier)?;

        if !lock_dir.exists() {
            fs::create_dir_all(&lock_dir)?;
        }

        let lock_file = lock_dir.join(format!("{}.processing", identity));

        if lock_file.exists() {
            let display = Path::new(input_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(input_path);
            return Err(VideoError::AlreadyProcessing(display.to_string()));
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

fn lock_identity(input_path: &str, target_identifier: &str) -> Result<String, VideoError> {
    let canonical_input_path = fs::canonicalize(input_path).map_err(|e| {
        VideoError::LockError(format!(
            "Failed to resolve canonical path for '{}': {}",
            input_path, e
        ))
    })?;

    let mut hasher = Sha256::new();
    hasher.update(canonical_input_path.to_string_lossy().as_bytes());
    hasher.update(b"|");
    hasher.update(target_identifier.as_bytes());
    Ok(hex::encode(hasher.finalize()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_source(root: &Path, subdir: &str) -> PathBuf {
        let dir = root.join(subdir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("video.mp4");
        fs::write(&path, b"dummy").unwrap();
        path
    }

    fn fresh_root() -> PathBuf {
        std::env::temp_dir()
            .join("aspectshift_htov_lock_test")
            .join(Uuid::new_v4().to_string())
    }

    #[test]
    fn same_basename_in_different_dirs_produces_different_identities() {
        let root = fresh_root();
        let dir_a = make_source(&root, "dir_a");
        let dir_b = make_source(&root, "dir_b");

        let target = "/out/video_9x16.mp4";
        let id_a = lock_identity(dir_a.to_str().unwrap(), target).unwrap();
        let id_b = lock_identity(dir_b.to_str().unwrap(), target).unwrap();

        assert_ne!(id_a, id_b);
    }

    #[test]
    fn same_input_different_targets_produce_different_identities() {
        let root = fresh_root();
        let input = make_source(&root, "single");
        let input_str = input.to_str().unwrap();

        let id_9x16 = lock_identity(input_str, "/out/video_9x16.mp4").unwrap();
        let id_1x1 = lock_identity(input_str, "/out/video_1x1.mp4").unwrap();

        assert_ne!(id_9x16, id_1x1);
    }

    #[test]
    fn same_input_same_target_produces_same_identity() {
        let root = fresh_root();
        let input = make_source(&root, "dup");
        let input_str = input.to_str().unwrap();
        let target = "/out/video_9x16.mp4";

        let id_a = lock_identity(input_str, target).unwrap();
        let id_b = lock_identity(input_str, target).unwrap();

        assert_eq!(id_a, id_b);
    }

    #[test]
    fn acquire_release_and_crash_cleanup_still_work() {
        let root = fresh_root();
        let lock_dir = root.join("locks");
        let input = make_source(&root, "src");
        let input_str = input.to_str().unwrap();
        let target = "/out/video_9x16.mp4";

        let lock = ProcessingLock::acquire_in_dir(lock_dir.clone(), input_str, target).unwrap();
        let lock_file = lock.lock_file.clone();
        assert!(lock_file.exists());
        lock.release().unwrap();
        assert!(!lock_file.exists());

        let drop_lock =
            ProcessingLock::acquire_in_dir(lock_dir.clone(), input_str, target).unwrap();
        let drop_lock_file = drop_lock.lock_file.clone();
        assert!(drop_lock_file.exists());
        drop(drop_lock);
        assert!(!drop_lock_file.exists());

        let reacquire =
            ProcessingLock::acquire_in_dir(lock_dir.clone(), input_str, target).unwrap();
        let reacquire_file = reacquire.lock_file.clone();
        assert!(reacquire_file.exists());
        reacquire.release().unwrap();
        assert!(!reacquire_file.exists());
    }

    #[test]
    fn held_lock_still_blocks_genuine_duplicate() {
        let root = fresh_root();
        let lock_dir = root.join("locks");
        let input = make_source(&root, "src");
        let input_str = input.to_str().unwrap();
        let target = "/out/video_9x16.mp4";

        let lock = ProcessingLock::acquire_in_dir(lock_dir.clone(), input_str, target).unwrap();

        let blocked = ProcessingLock::acquire_in_dir(lock_dir.clone(), input_str, target);
        assert!(matches!(blocked, Err(VideoError::AlreadyProcessing(_))));

        drop(lock);
    }
}
