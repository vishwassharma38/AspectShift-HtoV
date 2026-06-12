use chrono::{DateTime, Utc};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Mutex, RwLock};

use crate::download_manager::DownloadManager;
use crate::manifest_service::{ManifestDependencyInfo, ManifestService};
use crate::runtime_paths::RuntimePaths;
use crate::video::queue::BatchManager;
use crate::video::types::VideoError;

const WEEKLY_SCAN_INTERVAL_SECS: i64 = 7 * 24 * 60 * 60;
const DEPENDENCY_HEALTH_FILENAME: &str = "dependency-health.json";
const DEPENDENCY_HEALTH_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DependencyId {
    WhisperBinary,
    WhisperModel,
    Ffmpeg,
    Ffprobe,
}

pub const MANAGED_DEPENDENCY_IDS: &[DependencyId] =
    &[DependencyId::WhisperBinary, DependencyId::WhisperModel];

pub fn is_managed_dependency(id: &DependencyId) -> bool {
    matches!(id, DependencyId::WhisperBinary | DependencyId::WhisperModel)
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyScanStatus {
    NotScanned,
    Scanning,
    ScanCompleted,
    Error,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyScanSource {
    FirstLaunch,
    Manual,
    Weekly,
    PostDownload,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyHealthStatus {
    Unknown,
    Healthy,
    Degraded,
}

impl Default for DependencyHealthStatus {
    fn default() -> Self {
        DependencyHealthStatus::Unknown
    }
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

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyLifecycleStatus {
    Idle,
    Checking,
    Missing,
    Downloading,
    Verifying,
    Extracting,
    Installed,
    Failed,
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
    pub sha256_verified: Option<bool>,

    // Future-proofing for manifest/downloader
    pub expected_version: Option<String>,
    pub expected_sha256: Option<String>,
    pub expected_filename: Option<String>,
    pub source_url: Option<String>,
    pub update_available: Option<bool>,
    pub installed_version: Option<String>,
    pub lifecycle: DependencyLifecycleStatus,
    pub lifecycle_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppDepsState {
    #[serde(default)]
    pub validation_revision: u32,
    pub scan_status: DependencyScanStatus,
    #[serde(default)]
    pub scan_source: Option<DependencyScanSource>,
    #[serde(default)]
    pub health_status: DependencyHealthStatus,
    pub dependencies: HashMap<DependencyId, DependencyReport>,
    pub all_ready: bool,
    pub last_updated: String,
    #[serde(default)]
    pub last_full_scan_at: Option<String>,
    #[serde(default)]
    pub last_manual_scan_at: Option<String>,
    #[serde(default)]
    pub last_weekly_scan_at: Option<String>,
}

impl Default for AppDepsState {
    fn default() -> Self {
        Self {
            scan_status: DependencyScanStatus::NotScanned,
            validation_revision: DEPENDENCY_HEALTH_SCHEMA_VERSION,
            scan_source: None,
            health_status: DependencyHealthStatus::Unknown,
            dependencies: HashMap::new(),
            all_ready: false,
            last_updated: Utc::now().to_rfc3339(),
            last_full_scan_at: None,
            last_manual_scan_at: None,
            last_weekly_scan_at: None,
        }
    }
}

#[derive(Clone)]
pub struct DepsManager {
    state: Arc<RwLock<AppDepsState>>,
    manifest_service: ManifestService,
    scan_gate: Arc<Mutex<()>>,
}

impl DepsManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AppDepsState::default())),
            manifest_service: ManifestService::new(),
            scan_gate: Arc::new(Mutex::new(())),
        }
    }

    pub fn bootstrap(&self, app: &AppHandle) {
        if let Err(e) = self.load_persisted_state(app) {
            warn!("Failed to load persisted dependency state: {}", e);
        }

        let app_handle = app.clone();
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            manager.maybe_run_startup_scan(&app_handle).await;
        });

        let app_handle = app.clone();
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            manager.maybe_run_weekly_scan(&app_handle).await;
        });
    }

    pub async fn get_state(&self) -> AppDepsState {
        self.state.read().await.clone()
    }

    pub async fn refresh(
        &self,
        app: &AppHandle,
        scan_source: DependencyScanSource,
    ) -> Result<AppDepsState, VideoError> {
        self.scan_full_health(app, scan_source).await
    }

    async fn maybe_run_startup_scan(&self, app: &AppHandle) {
        let has_state_file = self.has_persisted_state(app);
        if has_state_file && !self.should_refresh_persisted_state().await {
            return;
        }
        let scan_source = if has_state_file {
            DependencyScanSource::Manual
        } else {
            DependencyScanSource::FirstLaunch
        };
        if let Err(e) = self.scan_full_health(app, scan_source).await {
            error!("Startup dependency scan failed: {}", e);
        }
    }

    async fn should_refresh_persisted_state(&self) -> bool {
        let state = self.get_state().await;
        if state.validation_revision < DEPENDENCY_HEALTH_SCHEMA_VERSION {
            return true;
        }
        if state.scan_status != DependencyScanStatus::ScanCompleted {
            return true;
        }

        MANAGED_DEPENDENCY_IDS.iter().any(|id| {
            state
                .dependencies
                .get(id)
                .map(|report| report.status != DependencyStatus::Ready)
                .unwrap_or(true)
        })
    }

    pub async fn maybe_run_weekly_scan(&self, app: &AppHandle) {
        if !self.should_run_weekly_scan(app).await {
            return;
        }
        if let Err(e) = self
            .scan_full_health(app, DependencyScanSource::Weekly)
            .await
        {
            error!("Weekly dependency scan failed: {}", e);
        }
    }

    async fn should_run_weekly_scan(&self, app: &AppHandle) -> bool {
        let state = self.get_state().await;
        let Some(last_full_scan_at) = state.last_full_scan_at.as_deref() else {
            return false;
        };

        let Ok(last_scan) = DateTime::parse_from_rfc3339(last_full_scan_at) else {
            return true;
        };

        let age = Utc::now().signed_duration_since(last_scan.with_timezone(&Utc));
        if age.num_seconds() < WEEKLY_SCAN_INTERVAL_SECS {
            return false;
        }

        if self.is_app_busy(app).await {
            return false;
        }

        true
    }

    async fn is_app_busy(&self, app: &AppHandle) -> bool {
        if let Some(download_manager) = app.try_state::<DownloadManager>() {
            if download_manager.is_busy() {
                return true;
            }
        }

        if let Some(batch_manager) = app.try_state::<BatchManager>() {
            if batch_manager.is_processing().await {
                return true;
            }
        }

        false
    }

    async fn scan_full_health(
        &self,
        app: &AppHandle,
        scan_source: DependencyScanSource,
    ) -> Result<AppDepsState, VideoError> {
        let is_weekly_scan = matches!(scan_source, DependencyScanSource::Weekly);
        let _scan_guard = if is_weekly_scan {
            let Ok(scan_guard) = self.scan_gate.try_lock() else {
                return Ok(self.get_state().await);
            };
            scan_guard
        } else {
            self.scan_gate.lock().await
        };

        if is_weekly_scan && self.is_app_busy(app).await {
            return Ok(self.get_state().await);
        }

        info!("Dependency health scan started: {:?}", scan_source);

        self.set_scan_in_progress(&scan_source).await;
        self.emit_state(app).await;

        let paths = match RuntimePaths::from_app(app) {
            Ok(p) => p,
            Err(e) => {
                error!(
                    "Failed to resolve runtime paths during dependency scan: {}",
                    e
                );
                let mut state_guard = self.state.write().await;
                state_guard.scan_status = DependencyScanStatus::Error;
                state_guard.last_updated = Utc::now().to_rfc3339();
                drop(state_guard);
                self.emit_state(app).await;
                return Err(e);
            }
        };

        let manifest = self.manifest_service.get_manifest(app).await;
        let mut deps = HashMap::new();

        let whisper_binary_meta = manifest.as_ref().and_then(|m| {
            self.manifest_service
                .get_dependency_info(m, "whisper_binary")
        });
        deps.insert(
            DependencyId::WhisperBinary,
            self.check_whisper_binary(&paths, whisper_binary_meta).await,
        );

        let whisper_model_meta = manifest.as_ref().and_then(|m| {
            self.manifest_service
                .get_dependency_info(m, "whisper_model")
        });
        deps.insert(
            DependencyId::WhisperModel,
            self.check_whisper_model(&paths, whisper_model_meta).await,
        );

        let all_ready = deps.values().all(|d| d.status == DependencyStatus::Ready);
        let health_status = if deps.is_empty() {
            DependencyHealthStatus::Unknown
        } else if all_ready {
            DependencyHealthStatus::Healthy
        } else {
            DependencyHealthStatus::Degraded
        };

        let now = Utc::now().to_rfc3339();
        let mut next_state = AppDepsState {
            scan_status: DependencyScanStatus::ScanCompleted,
            validation_revision: DEPENDENCY_HEALTH_SCHEMA_VERSION,
            scan_source: Some(scan_source.clone()),
            health_status,
            dependencies: deps,
            all_ready,
            last_updated: now.clone(),
            last_full_scan_at: Some(now.clone()),
            last_manual_scan_at: None,
            last_weekly_scan_at: None,
        };

        {
            let previous = self.state.read().await.clone();
            next_state.last_manual_scan_at = previous.last_manual_scan_at.clone();
            next_state.last_weekly_scan_at = previous.last_weekly_scan_at.clone();
            if matches!(scan_source, DependencyScanSource::Manual) {
                next_state.last_manual_scan_at = Some(now.clone());
            } else if matches!(scan_source, DependencyScanSource::Weekly) {
                next_state.last_weekly_scan_at = Some(now.clone());
            }
        }

        {
            let mut state_guard = self.state.write().await;
            *state_guard = next_state.clone();
        }

        if let Err(e) = self.save_persisted_state(app, &next_state) {
            error!("Failed to persist dependency health state: {}", e);
        }

        self.emit_state(app).await;
        info!("Dependency health scan completed. All ready: {}", all_ready);
        Ok(next_state)
    }

    async fn set_scan_in_progress(&self, scan_source: &DependencyScanSource) {
        let mut state_guard = self.state.write().await;
        state_guard.scan_status = DependencyScanStatus::Scanning;
        state_guard.scan_source = Some(scan_source.clone());
        state_guard.last_updated = Utc::now().to_rfc3339();
    }

    fn has_persisted_state(&self, app: &AppHandle) -> bool {
        self.state_file_path(app)
            .map(|path| path.exists())
            .unwrap_or(false)
    }

    fn state_file_path(&self, app: &AppHandle) -> Result<PathBuf, VideoError> {
        let runtime = RuntimePaths::from_app(app)?;
        Ok(runtime.cache_dir().join(DEPENDENCY_HEALTH_FILENAME))
    }

    fn load_persisted_state(&self, app: &AppHandle) -> Result<(), VideoError> {
        let path = self.state_file_path(app)?;
        if !path.exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&path)?;
        let mut state: AppDepsState = serde_json::from_str(&content)?;

        if state.last_full_scan_at.is_none()
            && state.scan_status == DependencyScanStatus::ScanCompleted
        {
            state.last_full_scan_at = Some(state.last_updated.clone());
        }

        if state.health_status == DependencyHealthStatus::Unknown && !state.dependencies.is_empty()
        {
            state.health_status = if state.all_ready {
                DependencyHealthStatus::Healthy
            } else {
                DependencyHealthStatus::Degraded
            };
        }

        if state.validation_revision < DEPENDENCY_HEALTH_SCHEMA_VERSION {
            info!(
                "Invalidating persisted dependency health state revision {} -> {}",
                state.validation_revision, DEPENDENCY_HEALTH_SCHEMA_VERSION
            );
            state.validation_revision = DEPENDENCY_HEALTH_SCHEMA_VERSION;
        }
        filter_unmanaged_dependencies(&mut state);

        let mut guard = self.state.blocking_write();
        *guard = state;
        Ok(())
    }

    fn save_persisted_state(
        &self,
        app: &AppHandle,
        state: &AppDepsState,
    ) -> Result<(), VideoError> {
        let path = self.state_file_path(app)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut state = state.clone();
        filter_unmanaged_dependencies(&mut state);
        let serialized = serde_json::to_string_pretty(&state)?;
        std::fs::write(path, serialized)?;
        Ok(())
    }

    async fn check_whisper_binary(
        &self,
        paths: &RuntimePaths,
        expected: Option<ManifestDependencyInfo>,
    ) -> DependencyReport {
        let path = paths.whisper_binary_path();

        let status = if !path.exists() {
            warn!("Whisper binary missing at {}", path.display());
            DependencyStatus::Missing
        } else if !path.is_file() {
            warn!("Whisper path {} exists but is not a file", path.display());
            DependencyStatus::Invalid {
                message: "Path exists but is not a file".to_string(),
            }
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
                                DependencyStatus::Invalid {
                                    message: "Binary is not executable".to_string(),
                                }
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            DependencyStatus::Ready
                        }
                    } else {
                        warn!("Whisper binary at {} is empty", path.display());
                        DependencyStatus::Corrupted {
                            message: "Binary file is empty".to_string(),
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read metadata for whisper binary: {}", e);
                    DependencyStatus::Invalid {
                        message: format!("Metadata error: {}", e),
                    }
                }
            }
        };

        let mut sha256_verified = None;
        let mut status = status;
        if let (DependencyStatus::Ready, Some(meta)) = (&status, expected.as_ref()) {
            match calculate_sha256_hex(&path) {
                Ok(actual_sha) => {
                    let matches = actual_sha == meta.sha256.to_lowercase();
                    sha256_verified = Some(matches);
                    if !matches {
                        status = DependencyStatus::Corrupted {
                            message: "Binary checksum mismatch".to_string(),
                        };
                    }
                }
                Err(e) => {
                    warn!("Failed to hash whisper binary {}: {}", path.display(), e);
                    sha256_verified = Some(false);
                    status = DependencyStatus::Corrupted {
                        message: "Binary checksum could not be verified".to_string(),
                    };
                }
            }
        }

        let installed = status == DependencyStatus::Ready;
        let expected_version = expected.as_ref().map(|m| m.version.clone());
        let expected_sha256 = expected.as_ref().map(|m| m.sha256.clone());
        let expected_filename = expected.as_ref().map(|m| m.filename.clone());
        let source_url = expected.as_ref().map(|m| m.url.clone());
        let installed_version = if installed {
            expected_version.clone()
        } else {
            None
        };
        let update_available = match (&installed_version, &expected_version) {
            (Some(installed_v), Some(expected_v)) => Some(installed_v != expected_v),
            _ => None,
        };

        DependencyReport {
            id: DependencyId::WhisperBinary,
            name: "Whisper CLI".to_string(),
            status,
            version: None,
            path: Some(path.to_string_lossy().to_string()),
            description: "High-performance GGUF-based Whisper transcription engine.".to_string(),
            last_checked: Utc::now().to_rfc3339(),
            sha256_verified,
            expected_version,
            expected_sha256,
            expected_filename,
            source_url,
            update_available,
            installed_version,
            lifecycle: if installed {
                DependencyLifecycleStatus::Installed
            } else {
                DependencyLifecycleStatus::Missing
            },
            lifecycle_message: None,
        }
    }

    async fn check_whisper_model(
        &self,
        paths: &RuntimePaths,
        expected: Option<ManifestDependencyInfo>,
    ) -> DependencyReport {
        let path = paths.whisper_model_path();

        let status = if !path.exists() {
            warn!("Whisper model missing at {}", path.display());
            DependencyStatus::Missing
        } else if !path.is_file() {
            warn!(
                "Whisper model path {} exists but is not a file",
                path.display()
            );
            DependencyStatus::Invalid {
                message: "Path exists but is not a file".to_string(),
            }
        } else {
            match std::fs::metadata(&path) {
                Ok(metadata) if metadata.len() > 0 => DependencyStatus::Ready,
                Ok(metadata) if metadata.len() == 0 => {
                    warn!("Whisper model at {} is empty", path.display());
                    DependencyStatus::Corrupted {
                        message: "Model file is empty".to_string(),
                    }
                }
                Err(e) => {
                    error!("Failed to read metadata for whisper model: {}", e);
                    DependencyStatus::Invalid {
                        message: format!("Metadata error: {}", e),
                    }
                }
                _ => DependencyStatus::Corrupted {
                    message: "Validation failed".to_string(),
                },
            }
        };

        let mut sha256_verified = None;
        let mut status = status;
        if let (DependencyStatus::Ready, Some(meta)) = (&status, expected.as_ref()) {
            match calculate_sha256_hex(&path) {
                Ok(actual_sha) => {
                    let matches = actual_sha == meta.sha256.to_lowercase();
                    sha256_verified = Some(matches);
                    if !matches {
                        status = DependencyStatus::Corrupted {
                            message: "Model checksum mismatch".to_string(),
                        };
                    }
                }
                Err(e) => {
                    warn!("Failed to hash whisper model {}: {}", path.display(), e);
                    sha256_verified = Some(false);
                    status = DependencyStatus::Corrupted {
                        message: "Model checksum could not be verified".to_string(),
                    };
                }
            }
        }

        let installed = status == DependencyStatus::Ready;
        let expected_version = expected.as_ref().map(|m| m.version.clone());
        let expected_sha256 = expected.as_ref().map(|m| m.sha256.clone());
        let expected_filename = expected.as_ref().map(|m| m.filename.clone());
        let source_url = expected.as_ref().map(|m| m.url.clone());
        let installed_version = if installed {
            expected_version.clone()
        } else {
            None
        };
        let update_available = match (&installed_version, &expected_version) {
            (Some(installed_v), Some(expected_v)) => Some(installed_v != expected_v),
            _ => None,
        };

        DependencyReport {
            id: DependencyId::WhisperModel,
            name: "Whisper Model".to_string(),
            status,
            version: None,
            path: Some(path.to_string_lossy().to_string()),
            description: "AI model for transcription.".to_string(),
            last_checked: Utc::now().to_rfc3339(),
            sha256_verified,
            expected_version,
            expected_sha256,
            expected_filename,
            source_url,
            update_available,
            installed_version,
            lifecycle: if installed {
                DependencyLifecycleStatus::Installed
            } else {
                DependencyLifecycleStatus::Missing
            },
            lifecycle_message: None,
        }
    }

    async fn emit_state(&self, app: &AppHandle) {
        let state = self.get_state().await;
        let _ = app.emit("deps://state", state);
    }
}

fn filter_unmanaged_dependencies(state: &mut AppDepsState) {
    state.dependencies.retain(|id, _| is_managed_dependency(id));
    state.all_ready = !state.dependencies.is_empty()
        && MANAGED_DEPENDENCY_IDS.iter().all(|id| {
            state
                .dependencies
                .get(id)
                .map(|report| report.status == DependencyStatus::Ready)
                .unwrap_or(false)
        });
    state.health_status = if state.dependencies.is_empty() {
        DependencyHealthStatus::Unknown
    } else if state.all_ready {
        DependencyHealthStatus::Healthy
    } else {
        DependencyHealthStatus::Degraded
    };
}

fn calculate_sha256_hex(path: &std::path::Path) -> Result<String, std::io::Error> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}
