use std::path::{Path, PathBuf};

use log::error;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

use crate::dependency_manager::{DependencyId, DependencyLifecycleStatus};
use crate::manifest_service::{ManifestDependencyInfo, ManifestService};
use crate::runtime_paths::RuntimePaths;
use crate::video::types::VideoError;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct DependencyInstallEvent {
    pub id: DependencyId,
    pub lifecycle: DependencyLifecycleStatus,
    pub progress_percent: Option<f32>,
    pub message: Option<String>,
}

#[derive(Clone)]
pub struct DownloadManager {
    manifest_service: ManifestService,
    client: reqwest::Client,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            manifest_service: ManifestService::new(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn install_dependency(
        &self,
        app: &AppHandle,
        id: DependencyId,
    ) -> Result<(), VideoError> {
        self.emit(app, id.clone(), DependencyLifecycleStatus::Downloading, Some(0.0), None);

        let manifest = self
            .manifest_service
            .get_manifest(app)
            .await
            .ok_or_else(|| VideoError::InvalidInput("Manifest unavailable".to_string()))?;
        let info = self
            .manifest_service
            .get_dependency_info(&manifest, dependency_id_key(&id))
            .ok_or_else(|| VideoError::InvalidInput("Dependency metadata not found".to_string()))?;

        let runtime = RuntimePaths::from_app(app)?;
        let staging_root = runtime
            .temp_dir()
            .join("dependency-installs")
            .join(dependency_id_key(&id));
        std::fs::create_dir_all(&staging_root)?;
        let download_path = staging_root.join(&info.filename);

        if let Err(e) = self.download_file(app, &id, &info, &download_path).await {
            self.cleanup_staging(&staging_root);
            return Err(e);
        }

        self.emit(app, id.clone(), DependencyLifecycleStatus::Verifying, None, None);
        if let Err(e) = self.verify_checksum(&download_path, &info.sha256).await {
            let _ = std::fs::remove_file(&download_path);
            self.cleanup_staging(&staging_root);
            return Err(e);
        }

        self.emit(app, id.clone(), DependencyLifecycleStatus::Extracting, None, None);
        let install_target_path = self.install_target_path(&runtime, &id, &info.filename);
        if let Some(parent) = install_target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Err(e) = self.atomic_install(&download_path, &install_target_path) {
            self.cleanup_staging(&staging_root);
            return Err(e);
        }

        self.cleanup_staging(&staging_root);
        self.emit(
            app,
            id,
            DependencyLifecycleStatus::Installed,
            Some(100.0),
            Some("Install complete".to_string()),
        );
        Ok(())
    }

    async fn download_file(
        &self,
        app: &AppHandle,
        id: &DependencyId,
        info: &ManifestDependencyInfo,
        output_path: &Path,
    ) -> Result<(), VideoError> {
        let response = self
            .client
            .get(&info.url)
            .send()
            .await
            .map_err(|e| VideoError::InvalidInput(format!("Download failed: {}", e)))?;
        if !response.status().is_success() {
            return Err(VideoError::InvalidInput(format!(
                "Download returned HTTP {}",
                response.status()
            )));
        }

        let total = response.content_length().unwrap_or(info.size).max(1);
        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(output_path).await?;
        let mut downloaded = 0u64;
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk
                .map_err(|e| VideoError::InvalidInput(format!("Download stream failed: {}", e)))?;
            file.write_all(&bytes).await?;
            downloaded += bytes.len() as u64;
            let pct = ((downloaded as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32;
            self.emit(
                app,
                id.clone(),
                DependencyLifecycleStatus::Downloading,
                Some(pct),
                None,
            );
        }
        file.flush().await?;
        Ok(())
    }

    async fn verify_checksum(&self, path: &Path, expected_sha256: &str) -> Result<(), VideoError> {
        use sha2::{Digest, Sha256};
        let bytes = tokio::fs::read(path).await?;
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let computed = format!("{:x}", hasher.finalize());
        if computed != expected_sha256.to_lowercase() {
            error!("Checksum mismatch. expected={}, actual={}", expected_sha256, computed);
            return Err(VideoError::InvalidInput("Downloaded file integrity check failed".to_string()));
        }
        Ok(())
    }

    fn atomic_install(&self, staged_path: &Path, live_path: &Path) -> Result<(), VideoError> {
        let backup_path = live_path.with_extension("bak");
        if live_path.exists() {
            if backup_path.exists() {
                let _ = std::fs::remove_file(&backup_path);
            }
            std::fs::rename(live_path, &backup_path)?;
        }

        if let Err(e) = std::fs::rename(staged_path, live_path) {
            if backup_path.exists() {
                let _ = std::fs::rename(&backup_path, live_path);
            }
            return Err(VideoError::IoError(e));
        }

        if backup_path.exists() {
            let _ = std::fs::remove_file(backup_path);
        }
        Ok(())
    }

    fn install_target_path(
        &self,
        runtime: &RuntimePaths,
        id: &DependencyId,
        filename: &str,
    ) -> PathBuf {
        match id {
            DependencyId::WhisperBinary => runtime.dependency_current_dir("whisper").join(filename),
            DependencyId::WhisperModel => runtime.model_current_dir("whisper").join(filename),
            DependencyId::Ffmpeg | DependencyId::Ffprobe => runtime.dependency_current_dir("sidecars").join(filename),
        }
    }

    fn cleanup_staging(&self, staging_root: &Path) {
        if staging_root.exists() {
            let _ = std::fs::remove_dir_all(staging_root);
        }
    }

    fn emit(
        &self,
        app: &AppHandle,
        id: DependencyId,
        lifecycle: DependencyLifecycleStatus,
        progress_percent: Option<f32>,
        message: Option<String>,
    ) {
        let event = DependencyInstallEvent {
            id,
            lifecycle,
            progress_percent,
            message,
        };
        let _ = app.emit("deps://install-progress", event);
    }
}

fn dependency_id_key(id: &DependencyId) -> &'static str {
    match id {
        DependencyId::WhisperBinary => "whisper_binary",
        DependencyId::WhisperModel => "whisper_model",
        DependencyId::Ffmpeg => "ffmpeg",
        DependencyId::Ffprobe => "ffprobe",
    }
}
