use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

use crate::video::types::VideoError;

const RUNTIME_ROOT_DIR: &str = "runtime";
const DEPENDENCIES_DIR: &str = "dependencies";
const MODELS_DIR: &str = "models";
const CACHE_DIR: &str = "cache";
const TEMP_DIR: &str = "temp";
const LOGS_DIR: &str = "logs";
const EXPORTS_DIR: &str = "exports";
const USER_ASSETS_DIR: &str = "user-assets";
const GENERATED_DIR: &str = "generated-assets";
const SUBTITLES_DIR: &str = "subtitles";
const THUMBNAILS_DIR: &str = "thumbnails";
const CURRENT_DIR: &str = "current";
const VERSIONS_DIR: &str = "versions";

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    root: PathBuf,
}

impl RuntimePaths {
    pub fn from_app(app: &AppHandle) -> Result<Self, VideoError> {
        let app_data = app.path().app_data_dir().map_err(VideoError::TauriError)?;
        let root = normalize(app_data.join(RUNTIME_ROOT_DIR));
        Ok(Self { root })
    }

    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn dependencies_root(&self) -> PathBuf {
        self.root.join(DEPENDENCIES_DIR)
    }

    pub fn dependency_dir(&self, name: &str) -> PathBuf {
        self.dependencies_root().join(sanitize_component(name))
    }

    pub fn models_root(&self) -> PathBuf {
        self.root.join(MODELS_DIR)
    }

    pub fn model_dir(&self, name: &str) -> PathBuf {
        self.models_root().join(sanitize_component(name))
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join(CACHE_DIR)
    }

    pub fn temp_dir(&self) -> PathBuf {
        self.root.join(TEMP_DIR)
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join(LOGS_DIR)
    }

    pub fn exports_dir(&self) -> PathBuf {
        self.root.join(EXPORTS_DIR)
    }

    pub fn user_assets_dir(&self) -> PathBuf {
        self.root.join(USER_ASSETS_DIR)
    }

    pub fn generated_assets_dir(&self) -> PathBuf {
        self.root.join(GENERATED_DIR)
    }

    pub fn subtitle_temp_dir(&self) -> PathBuf {
        self.temp_dir().join(SUBTITLES_DIR)
    }

    pub fn thumbnail_cache_dir(&self) -> PathBuf {
        self.cache_dir().join(THUMBNAILS_DIR)
    }

    pub fn whisper_binary_default_filename() -> &'static str {
        if cfg!(target_os = "windows") {
            "whisper-x86_64-pc-windows-msvc.exe"
        } else if cfg!(target_os = "macos") {
            "whisper-x86_64-apple-darwin"
        } else {
            "whisper-x86_64-unknown-linux-gnu"
        }
    }

    pub fn whisper_model_default_filename() -> &'static str {
        "ggml-medium.en.bin"
    }

    pub fn whisper_binary_path(&self) -> PathBuf {
        self.dependency_current_dir("whisper")
            .join(Self::whisper_binary_default_filename())
    }

    pub fn whisper_model_path(&self) -> PathBuf {
        self.model_current_dir("whisper")
            .join(Self::whisper_model_default_filename())
    }

    pub fn dependency_current_dir(&self, name: &str) -> PathBuf {
        self.dependency_dir(name).join(CURRENT_DIR)
    }

    pub fn dependency_versions_dir(&self, name: &str) -> PathBuf {
        self.dependency_dir(name).join(VERSIONS_DIR)
    }

    pub fn model_current_dir(&self, name: &str) -> PathBuf {
        self.model_dir(name).join(CURRENT_DIR)
    }

    pub fn model_versions_dir(&self, name: &str) -> PathBuf {
        self.model_dir(name).join(VERSIONS_DIR)
    }

    pub fn ensure_runtime_tree(&self) -> Result<(), VideoError> {
        let dirs = [
            self.root(),
            self.dependencies_root(),
            self.models_root(),
            self.cache_dir(),
            self.temp_dir(),
            self.logs_dir(),
            self.exports_dir(),
            self.user_assets_dir(),
            self.generated_assets_dir(),
            self.subtitle_temp_dir(),
            self.thumbnail_cache_dir(),
            self.dependency_dir("whisper"),
            self.dependency_current_dir("whisper"),
            self.dependency_versions_dir("whisper"),
            self.model_dir("whisper"),
            self.model_current_dir("whisper"),
            self.model_versions_dir("whisper"),
        ];

        for dir in dirs {
            ensure_dir_exists(&dir)?;
        }
        Ok(())
    }
}

pub fn ensure_dir_exists(path: &Path) -> Result<(), VideoError> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

pub fn normalize(path: PathBuf) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        canonical
    } else {
        path
    }
}

fn sanitize_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_lowercase()
}
