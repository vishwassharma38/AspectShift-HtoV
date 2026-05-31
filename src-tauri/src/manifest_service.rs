use std::collections::HashMap;
use std::sync::Arc;

use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tokio::sync::RwLock;

use crate::runtime_paths::RuntimePaths;
use crate::video::types::VideoError;

const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/vishwassharma38/AspectShift-HtoV-Assets/main/manifest.json";
const MANIFEST_CACHE_FILENAME: &str = "manifest.json";

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct DependencyManifest {
    pub manifest_version: u32,
    pub generated_at: String,
    pub dependencies: HashMap<String, ManifestDependency>,
    pub models: HashMap<String, ManifestModelGroup>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManifestDependency {
    pub version: String,
    pub windows: Option<ManifestPlatformDependency>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPlatformDependency {
    pub binary: ManifestArtifact,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManifestArtifact {
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManifestModelGroup {
    pub default: String,
    pub files: HashMap<String, ManifestModelFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManifestModelFile {
    pub filename: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManifestDependencyInfo {
    pub id: String,
    pub version: String,
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone)]
pub struct ManifestService {
    client: reqwest::Client,
    cached_manifest: Arc<RwLock<Option<DependencyManifest>>>,
}

impl ManifestService {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            cached_manifest: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_manifest(&self, app: &AppHandle) -> Option<DependencyManifest> {
        if let Some(existing) = self.cached_manifest.read().await.clone() {
            return Some(existing);
        }

        match self.fetch_remote_manifest(app).await {
            Ok(manifest) => {
                if let Err(e) = self.save_manifest_cache(app, &manifest).await {
                    warn!("Failed saving manifest cache: {}", e);
                }
                *self.cached_manifest.write().await = Some(manifest.clone());
                Some(manifest)
            }
            Err(e) => {
                warn!(
                    "Remote manifest fetch failed: {}. Trying cache fallback.",
                    e
                );
                match self.load_cached_manifest(app).await {
                    Ok(manifest) => {
                        *self.cached_manifest.write().await = Some(manifest.clone());
                        Some(manifest)
                    }
                    Err(cache_err) => {
                        error!(
                            "Manifest unavailable. Remote error: {}. Cache error: {}.",
                            e, cache_err
                        );
                        None
                    }
                }
            }
        }
    }

    pub async fn fetch_remote_manifest(
        &self,
        app: &AppHandle,
    ) -> Result<DependencyManifest, VideoError> {
        let url = self.manifest_url(app)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| VideoError::InvalidInput(format!("Manifest fetch failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(VideoError::InvalidInput(format!(
                "Manifest fetch returned HTTP {}",
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| VideoError::InvalidInput(format!("Manifest read failed: {}", e)))?;
        let manifest = self.parse_and_validate(&body)?;
        info!("Manifest fetched and validated");
        Ok(manifest)
    }

    pub async fn load_cached_manifest(
        &self,
        app: &AppHandle,
    ) -> Result<DependencyManifest, VideoError> {
        let cache_path = self.manifest_cache_path(app)?;
        let content = std::fs::read_to_string(cache_path)?;
        self.parse_and_validate(&content)
    }

    pub async fn save_manifest_cache(
        &self,
        app: &AppHandle,
        manifest: &DependencyManifest,
    ) -> Result<(), VideoError> {
        let cache_path = self.manifest_cache_path(app)?;
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_string_pretty(manifest)?;
        std::fs::write(cache_path, serialized)?;
        Ok(())
    }

    pub fn get_dependency_info(
        &self,
        manifest: &DependencyManifest,
        id: &str,
    ) -> Option<ManifestDependencyInfo> {
        match id {
            "whisper_binary" => {
                let dep = manifest.dependencies.get("whisper")?;
                let binary = dep.windows.as_ref()?.binary.clone();
                Some(ManifestDependencyInfo {
                    id: id.to_string(),
                    version: dep.version.clone(),
                    filename: binary.filename,
                    url: binary.url,
                    sha256: binary.sha256,
                    size: binary.size,
                })
            }
            "whisper_model" => {
                let models = manifest.models.get("whisper")?;
                let default_id = models.default.as_str();
                let file = models.files.get(default_id)?;
                Some(ManifestDependencyInfo {
                    id: id.to_string(),
                    version: file.version.clone(),
                    filename: file.filename.clone(),
                    url: file.url.clone(),
                    sha256: file.sha256.clone(),
                    size: file.size,
                })
            }
            _ => None,
        }
    }

    fn parse_and_validate(&self, raw: &str) -> Result<DependencyManifest, VideoError> {
        let manifest: DependencyManifest = serde_json::from_str(raw)?;
        self.validate_manifest(&manifest)?;
        Ok(manifest)
    }

    fn validate_manifest(&self, manifest: &DependencyManifest) -> Result<(), VideoError> {
        if manifest.manifest_version == 0 {
            return Err(VideoError::InvalidInput(
                "manifestVersion must be >= 1".to_string(),
            ));
        }
        if manifest.generated_at.trim().is_empty() {
            return Err(VideoError::InvalidInput(
                "generatedAt must be present".to_string(),
            ));
        }

        for (dep_id, dep) in &manifest.dependencies {
            if dep_id.trim().is_empty() {
                return Err(VideoError::InvalidInput(
                    "dependency id cannot be empty".to_string(),
                ));
            }
            if dep.version.trim().is_empty() {
                return Err(VideoError::InvalidInput(format!(
                    "dependency version missing for {}",
                    dep_id
                )));
            }
            if cfg!(target_os = "windows") {
                let windows = dep.windows.as_ref().ok_or_else(|| {
                    VideoError::InvalidInput(format!(
                        "windows artifact block missing for {}",
                        dep_id
                    ))
                })?;
                self.validate_artifact(&windows.binary, dep_id)?;
            }
        }

        for (model_id, model_group) in &manifest.models {
            if model_id.trim().is_empty() {
                return Err(VideoError::InvalidInput(
                    "model id cannot be empty".to_string(),
                ));
            }
            if model_group.default.trim().is_empty() {
                return Err(VideoError::InvalidInput(format!(
                    "models.{}.default is required",
                    model_id
                )));
            }
            if !model_group.files.contains_key(&model_group.default) {
                return Err(VideoError::InvalidInput(format!(
                    "models.{}.default references missing file key",
                    model_id
                )));
            }
            for (file_id, file) in &model_group.files {
                if file_id.trim().is_empty() {
                    return Err(VideoError::InvalidInput(format!(
                        "models.{}.files has empty key",
                        model_id
                    )));
                }
                if file.version.trim().is_empty() {
                    return Err(VideoError::InvalidInput(format!(
                        "models.{}.files.{}.version is required",
                        model_id, file_id
                    )));
                }
                self.validate_artifact(
                    &ManifestArtifact {
                        filename: file.filename.clone(),
                        url: file.url.clone(),
                        sha256: file.sha256.clone(),
                        size: file.size,
                    },
                    &format!("model:{}", model_id),
                )?;
            }
        }

        Ok(())
    }

    fn validate_artifact(
        &self,
        artifact: &ManifestArtifact,
        context: &str,
    ) -> Result<(), VideoError> {
        if artifact.filename.trim().is_empty() {
            return Err(VideoError::InvalidInput(format!(
                "filename missing for {}",
                context
            )));
        }
        if artifact.url.trim().is_empty() {
            return Err(VideoError::InvalidInput(format!(
                "url missing for {}",
                context
            )));
        }
        if reqwest::Url::parse(&artifact.url).is_err() {
            return Err(VideoError::InvalidInput(format!(
                "url invalid for {}",
                context
            )));
        }
        if artifact.sha256.len() != 64 || !artifact.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(VideoError::InvalidInput(format!(
                "sha256 invalid for {}",
                context
            )));
        }
        if artifact.size == 0 {
            return Err(VideoError::InvalidInput(format!(
                "size must be > 0 for {}",
                context
            )));
        }
        Ok(())
    }

    fn manifest_cache_path(&self, app: &AppHandle) -> Result<std::path::PathBuf, VideoError> {
        let runtime = RuntimePaths::from_app(app)?;
        Ok(runtime.cache_dir().join(MANIFEST_CACHE_FILENAME))
    }

    fn manifest_url(&self, _: &AppHandle) -> Result<String, VideoError> {
        Ok(DEFAULT_MANIFEST_URL.to_string())
    }
}
