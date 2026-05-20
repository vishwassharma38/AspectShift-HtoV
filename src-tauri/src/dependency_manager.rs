use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::sync::RwLock;
use chrono::Utc;
use tauri_plugin_shell::ShellExt;
use tokio::time::{timeout, Duration};
use log::{info, warn, error};

use crate::runtime_paths::RuntimePaths;
use crate::video::types::VideoError;

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DependencyId {
    WhisperBinary,
    WhisperModel,
    Ffmpeg,
    Ffprobe,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyScanStatus {
    NotScanned,
    Scanning,
    ScanCompleted,
    Error,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DependencyStatus {
    Missing,
    Installed,
    Invalid { message: String },
    Corrupted { message: String },
    Ready,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct DependencyReport {
    pub id: DependencyId,
    pub name: String,
    pub status: DependencyStatus,
    pub version: Option<String>,
    pub path: Option<String>,
    pub description: String,
    pub last_checked: String,
    
    // Future-proofing for manifest/downloader
    pub expected_version: Option<String>,
    pub expected_sha256: Option<String>,
    pub expected_filename: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppDepsState {
    pub scan_status: DependencyScanStatus,
    pub dependencies: HashMap<DependencyId, DependencyReport>,
    pub all_ready: bool,
    pub last_updated: String,
}

#[derive(Clone)]
pub struct DepsManager {
    state: Arc<RwLock<AppDepsState>>,
}

impl DepsManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AppDepsState {
                scan_status: DependencyScanStatus::NotScanned,
                dependencies: HashMap::new(),
                all_ready: false,
                last_updated: Utc::now().to_rfc3339(),
            })),
        }
    }

    pub async fn get_state(&self) -> AppDepsState {
        self.state.read().await.clone()
    }

    pub async fn refresh(&self, app: &AppHandle) -> Result<AppDepsState, VideoError> {
        info!("Dependency refresh started");
        
        {
            let mut state_guard = self.state.write().await;
            state_guard.scan_status = DependencyScanStatus::Scanning;
            state_guard.last_updated = Utc::now().to_rfc3339();
        }

        let paths = match RuntimePaths::from_app(app) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to resolve runtime paths during dependency scan: {}", e);
                let mut state_guard = self.state.write().await;
                state_guard.scan_status = DependencyScanStatus::Error;
                return Err(e);
            }
        };

        let mut deps = HashMap::new();

        // 1. Whisper Binary
        deps.insert(
            DependencyId::WhisperBinary,
            self.check_whisper_binary(&paths).await,
        );

        // 2. Whisper Model
        deps.insert(
            DependencyId::WhisperModel,
            self.check_whisper_model(&paths).await,
        );

        // 3. FFmpeg (Sidecar)
        deps.insert(
            DependencyId::Ffmpeg,
            self.check_sidecar_executable(app, "ffmpeg", DependencyId::Ffmpeg).await,
        );

        // 4. FFprobe (Sidecar)
        deps.insert(
            DependencyId::Ffprobe,
            self.check_sidecar_executable(app, "ffprobe", DependencyId::Ffprobe).await,
        );

        let all_ready = deps.values().all(|d| d.status == DependencyStatus::Ready);
        
        let new_state = AppDepsState {
            scan_status: DependencyScanStatus::ScanCompleted,
            dependencies: deps,
            all_ready,
            last_updated: Utc::now().to_rfc3339(),
        };

        let mut state_guard = self.state.write().await;
        *state_guard = new_state.clone();

        info!("Dependency refresh completed. All ready: {}", all_ready);
        Ok(new_state)
    }

    async fn check_whisper_binary(&self, paths: &RuntimePaths) -> DependencyReport {
        let path = paths.whisper_binary_path();
        
        let status = if !path.exists() {
            warn!("Whisper binary missing at {}", path.display());
            DependencyStatus::Missing
        } else if !path.is_file() {
            warn!("Whisper path {} exists but is not a file", path.display());
            DependencyStatus::Invalid { message: "Path exists but is not a file".to_string() }
        } else {
            match std::fs::metadata(&path) {
                Ok(metadata) => {
                    if metadata.len() > 0 {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            if metadata.permissions().mode() & 0o111 != 0 {
                                DependencyStatus::Ready
                            } else {
                                warn!("Whisper binary at {} is not executable", path.display());
                                DependencyStatus::Invalid { message: "Binary is not executable".to_string() }
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            DependencyStatus::Ready
                        }
                    } else {
                        warn!("Whisper binary at {} is empty", path.display());
                        DependencyStatus::Corrupted { message: "Binary file is empty".to_string() }
                    }
                }
                Err(e) => {
                    error!("Failed to read metadata for whisper binary: {}", e);
                    DependencyStatus::Invalid { message: format!("Metadata error: {}", e) }
                }
            }
        };

        DependencyReport {
            id: DependencyId::WhisperBinary,
            name: "Whisper CLI".to_string(),
            status,
            version: None,
            path: Some(path.to_string_lossy().to_string()),
            description: "High-performance GGUF-based Whisper transcription engine.".to_string(),
            last_checked: Utc::now().to_rfc3339(),
            expected_version: None,
            expected_sha256: None,
            expected_filename: Some("whisper-cli".to_string()),
            source_url: None,
        }
    }

    async fn check_whisper_model(&self, paths: &RuntimePaths) -> DependencyReport {
        let path = paths.whisper_model_path();
        
        let status = if !path.exists() {
            warn!("Whisper model missing at {}", path.display());
            DependencyStatus::Missing
        } else if !path.is_file() {
            warn!("Whisper model path {} exists but is not a file", path.display());
            DependencyStatus::Invalid { message: "Path exists but is not a file".to_string() }
        } else {
            match std::fs::metadata(&path) {
                Ok(metadata) if metadata.len() > 0 => {
                    DependencyStatus::Ready
                }
                Ok(metadata) if metadata.len() == 0 => {
                    warn!("Whisper model at {} is empty", path.display());
                    DependencyStatus::Corrupted { message: "Model file is empty".to_string() }
                }
                Err(e) => {
                    error!("Failed to read metadata for whisper model: {}", e);
                    DependencyStatus::Invalid { message: format!("Metadata error: {}", e) }
                }
                _ => {
                    DependencyStatus::Corrupted { message: "Validation failed".to_string() }
                }
            }
        };

        DependencyReport {
            id: DependencyId::WhisperModel,
            name: "Whisper Model".to_string(),
            status,
            version: None,
            path: Some(path.to_string_lossy().to_string()),
            description: "AI model for transcription.".to_string(),
            last_checked: Utc::now().to_rfc3339(),
            expected_version: None,
            expected_sha256: None,
            expected_filename: Some("ggml-model.bin".to_string()),
            source_url: None,
        }
    }

    async fn check_sidecar_executable(&self, app: &AppHandle, name: &str, id: DependencyId) -> DependencyReport {
        let mut path_str = None;
        let mut version = None;

        let (display_name, description) = match id {
            DependencyId::Ffmpeg => ("FFmpeg".to_string(), "Universal media converter for video processing.".to_string()),
            DependencyId::Ffprobe => ("FFprobe".to_string(), "Media analysis tool for detecting video properties.".to_string()),
            _ => (name.to_string(), String::new()),
        };

        let status = match app.shell().sidecar(name) {
            Ok(sidecar) => {
                if let Ok(p) = app.path().resolve(format!("bin/{}", name), tauri::path::BaseDirectory::Resource) {
                    path_str = Some(p.to_string_lossy().to_string());
                }

                match timeout(Duration::from_secs(2), async {
                    let cmd = sidecar.args(["-version"]);
                    cmd.output().await
                }).await {
                    Ok(Ok(output)) => {
                        if output.status.success() {
                            let out_str = String::from_utf8_lossy(&output.stdout);
                            if let Some(first_line) = out_str.lines().next() {
                                version = Some(first_line.to_string());
                            }
                            DependencyStatus::Ready
                        } else {
                            let err_msg = String::from_utf8_lossy(&output.stderr);
                            error!("Sidecar {} failed with exit code {:?}: {}", name, output.status, err_msg);
                            DependencyStatus::Corrupted { message: format!("Executable failed: {}", err_msg) }
                        }
                    }
                    Ok(Err(e)) => {
                        error!("Failed to execute sidecar {}: {}", name, e);
                        DependencyStatus::Invalid { message: format!("Execution failed: {}", e) }
                    }
                    Err(_) => {
                        error!("Sidecar {} validation timed out after 2 seconds", name);
                        DependencyStatus::Invalid { message: "Validation timed out".to_string() }
                    }
                }
            }
            Err(e) => {
                warn!("Sidecar {} not found in Tauri resources: {}", name, e);
                DependencyStatus::Missing
            }
        };

        DependencyReport {
            id,
            name: display_name,
            status,
            version,
            path: path_str,
            description,
            last_checked: Utc::now().to_rfc3339(),
            expected_version: None,
            expected_sha256: None,
            expected_filename: Some(name.to_string()),
            source_url: None,
        }
    }
}
