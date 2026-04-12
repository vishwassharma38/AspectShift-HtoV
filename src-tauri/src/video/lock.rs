use std::fs;
use std::path::{Path, PathBuf};
use crate::video::types::VideoError;

pub struct ProcessingLock {
    lock_file: PathBuf,
}

impl ProcessingLock {
    pub fn acquire(input_path: &str, output_dir: &str) -> Result<Self, VideoError> {
        let input_stem = Path::new(input_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| VideoError::InvalidInput("Invalid input filename".into()))?;

        let lock_file = Path::new(output_dir).join(format!("{}.processing", input_stem));

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

pub fn release_processing_lock(input_path: &str, output_dir: &str) -> Result<(), VideoError> {
    let input_stem = Path::new(input_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| VideoError::InvalidInput("Invalid input filename".into()))?;

    let lock_file = Path::new(output_dir).join(format!("{}.processing", input_stem));
    if lock_file.exists() {
        fs::remove_file(lock_file)?;
    }
    Ok(())
}
