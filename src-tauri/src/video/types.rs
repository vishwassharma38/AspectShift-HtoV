use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Type)]
#[serde(rename_all = "snake_case")]
pub enum AspectRatio {
    Ratio9x16,
    Ratio1x1,
    Ratio4x5,
    Ratio2x3,
    Ratio16x9,
}

impl AspectRatio {
    pub fn get_ratio(&self) -> f32 {
        match self {
            AspectRatio::Ratio9x16 => 9.0 / 16.0,
            AspectRatio::Ratio1x1 => 1.0,
            AspectRatio::Ratio4x5 => 4.0 / 5.0,
            AspectRatio::Ratio2x3 => 2.0 / 3.0,
            AspectRatio::Ratio16x9 => 16.0 / 9.0,
        }
    }

    pub fn get_tag(&self) -> &'static str {
        match self {
            AspectRatio::Ratio9x16 => "9:16",
            AspectRatio::Ratio1x1 => "1:1",
            AspectRatio::Ratio4x5 => "4:5",
            AspectRatio::Ratio2x3 => "2:3",
            AspectRatio::Ratio16x9 => "16:9",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct EncodingProfile {
    pub crf: u8,
    pub quality_preset: String,
    pub speed_preset: String,
    pub audio_bitrate: String,
}

impl EncodingProfile {
    pub fn standard() -> Self {
        Self {
            crf: 23,
            quality_preset: "standard".to_string(),
            speed_preset: "medium".to_string(),
            audio_bitrate: "128k".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlatformConfig {
    pub target_width: u32,
    pub target_height: u32,
    pub enforce_dimensions: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct AspectRatioTarget {
    pub id: String,
    pub ratio: AspectRatio,
    pub encoding: EncodingProfile,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPreset {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub ratio: AspectRatio,
    pub encoding: EncodingProfile,
    pub platform_config: Option<PlatformConfig>,
    pub is_builtin: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomPreset {
    pub id: String,
    pub name: String,
    pub ratio: AspectRatio,
    pub encoding: EncodingProfile,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Type)]
pub struct VideoTransform {
    #[serde(default)]
    pub rotate: i32,
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct LogoOptions {
    #[serde(default)]
    pub enabled: bool,
    pub position: LogoPosition,
    pub opacity: f32,
    pub gap: u32,
    pub scale: f32,
    pub path: Option<String>,
}

impl Default for LogoOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            position: LogoPosition::BottomRight,
            opacity: 1.0,
            gap: 20,
            scale: 0.15,
            path: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "snake_case")]
pub enum LogoPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Mp4,
    Mov,
    Webm,
}

impl OutputFormat {
    pub fn get_extension(&self) -> &'static str {
        match self {
            OutputFormat::Mp4 => "mp4",
            OutputFormat::Mov => "mov",
            OutputFormat::Webm => "webm",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoEffectsSettings {
    pub blur: Option<bool>,
    pub overlays: Option<Vec<String>>,
    pub subtitles: Option<String>,
    pub color_filter: Option<String>,
    pub blur_sigma: Option<f32>,
    pub remove_audio: Option<bool>,
    pub export_subtitles: Option<bool>,
    pub burn_subtitles: Option<bool>,
    pub skip_existing: Option<bool>,
    pub output_format: Option<OutputFormat>,
    pub logo: Option<LogoOptions>,
    pub transform: Option<VideoTransform>,
}

impl VideoEffectsSettings {
    pub fn blur_enabled(&self) -> bool {
        self.blur.unwrap_or(false)
    }

    pub fn blur_sigma_value(&self) -> f32 {
        self.blur_sigma.unwrap_or(20.0)
    }

    pub fn remove_audio_enabled(&self) -> bool {
        self.remove_audio.unwrap_or(false)
    }

    pub fn export_subtitles_enabled(&self) -> bool {
        self.export_subtitles.unwrap_or(false)
    }

    pub fn burn_subtitles_enabled(&self) -> bool {
        self.burn_subtitles.unwrap_or(false)
    }

    pub fn skip_existing_enabled(&self) -> bool {
        self.skip_existing.unwrap_or(false)
    }

    pub fn output_format_value(&self) -> OutputFormat {
        self.output_format.clone().unwrap_or(OutputFormat::Mp4)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub last_input_dir: Option<String>,
    pub last_output_dir: Option<String>,
    pub last_preset_id: Option<String>, // Deprecated, kept for migration
    pub selected_ratio_ids: Vec<AspectRatio>,
    pub selected_preset_ids: Vec<String>,
    pub logo_path: Option<String>,
    pub logo_opacity: Option<f32>,
    pub logo_position: Option<LogoPosition>,
    pub blur: Option<bool>,
    pub blur_sigma: Option<f32>,
    pub enable_subfolders: Option<bool>,
    pub preview_volume: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VideoPresetDTO {
    Platform(PlatformPreset),
    Custom(CustomPreset),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Type)]
#[serde(rename_all = "camelCase")]
pub enum TargetType {
    AspectRatio,
    Platform,
    Custom,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectionMetadata {
    pub source_type: TargetType,
    pub source_id: String,
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct OutputJob {
    pub id: String,
    pub ratio: AspectRatio,
    pub encoding: EncodingProfile,
    pub effects: VideoEffectsSettings,
    pub platform_config: Option<PlatformConfig>,
    pub selection: SelectionMetadata,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLayoutRequest {
    pub ratio: AspectRatio,
    pub target_aspect_ratio: Option<f32>,
    pub effects: VideoEffectsSettings,
    pub platform_config: Option<PlatformConfig>,
}

#[derive(Debug, Clone)]
pub struct ResolvedJob {
    pub id: String,
    pub session_id: String,
    pub input_path: String,
    pub output_path: String,
    pub alt_output_path: Option<String>,
    pub ratio: AspectRatio,
    pub encoding: EncodingProfile,
    pub effects: VideoEffectsSettings,
    pub platform_config: Option<PlatformConfig>,
    pub subtitle_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct OutputTarget {
    pub id: String,
    pub label: String,
    pub target_type: TargetType,
    pub job: OutputJob,
}

impl OutputTarget {
    // CENTRALIZED SANITIZATION:
    // All labels MUST pass through this function exactly once.
    pub fn sanitize_label(label: &str) -> String {
        label
            // Step 1: replace ratio colons first
            .replace(':', "x")
            // Step 2: convert word boundaries into underscores
            .replace(
                |c: char| c == ' ' || c == '/' || c == '-' || c == '(' || c == ')',
                "_",
            )
            // Step 3: remove invalid characters
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "")
            // Step 4: collapse repeated underscores
            .split('_')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_")
            .to_lowercase()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchJob {
    pub id: String,
    pub input_path: String,
    pub output: OutputJob,
    pub resolved_output_path: String,
    pub alt_output_path: Option<String>,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversionRequestDTO {
    pub input: String,
    pub output_dir: String,
    pub job: OutputJob,
}

#[derive(Debug, Clone)]
pub struct ConversionRequest {
    pub input: String,
    pub output_dir: String,
    pub job: OutputJob,
}

impl From<ConversionRequestDTO> for ConversionRequest {
    fn from(dto: ConversionRequestDTO) -> Self {
        Self {
            input: dto.input,
            output_dir: dto.output_dir,
            job: dto.job,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchJobSettings {
    pub targets: Vec<OutputJob>,
    pub output_dir: String,
    #[serde(default)]
    pub enable_subfolders: bool,
}

pub struct OutputTags {
    pub ratio: String,
    pub platform: Option<String>,
    pub blur: bool,
    pub logo: bool,
    pub subtitles: bool,
    pub no_audio: bool,
}

impl OutputTags {
    pub fn to_suffix(&self) -> String {
        let mut tags = Vec::new();
        tags.push(self.ratio.clone());
        if let Some(platform) = &self.platform {
            tags.push(platform.clone());
        }
        if self.blur {
            tags.push("blur".to_string());
        }
        if self.logo {
            tags.push("logo".to_string());
        }
        if self.subtitles {
            tags.push("subtitles".to_string());
        }
        if self.no_audio {
            tags.push("no_audio".to_string());
        }
        tags.join("_")
    }

    pub fn get_output_filename(&self, stem: &str, extension: &str) -> String {
        format!("{}_{}.{}", stem, self.to_suffix(), extension)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Pending,
    Processing,
    Completed,
    #[serde(rename = "error")]
    Failed(String),
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileProgress {
    pub session_id: String,
    pub job_id: String,
    pub file_path: String,
    pub ratio: AspectRatio,
    pub progress: f32,
    pub status: JobStatus,
    pub thumbnail_path: Option<String>,
    pub duration_secs: f64,
    pub selection: SelectionMetadata,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub enum BatchStatus {
    Idle,
    Processing,
    Cancelled,
    Completed,
    Failed,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchProgress {
    pub session_id: Option<String>,
    pub total_jobs: usize,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub percentage: f32,
    pub status: BatchStatus,
    pub current_job_id: Option<String>,
    pub queue: Vec<FileProgress>,
    pub eta_seconds: Option<f64>,
    pub speed: f32,
    pub total_duration_secs: f64,
    pub processed_duration_secs: f64,
    pub current_stage_id: Option<String>,
    pub current_stage_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrientationInfo {
    pub width: u32,
    pub height: u32,
    pub rotation: i32,
    pub is_vertical: bool,
    pub display_width: u32,
    pub display_height: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConversionResult {
    pub output_path: String,
    pub ratio: AspectRatio,
    pub skipped: bool,
}

#[derive(Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileReadiness {
    pub exists: bool,
    pub is_readable: bool,
    pub file_size_bytes: u64,
    pub is_locked: bool,
    pub estimated_duration_secs: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct LogoPreset {
    pub path: String,
    pub position: LogoPosition,
    pub opacity: f32,
    pub gap: u32,
    pub scale: f32,
}

#[derive(Error, Debug)]
pub enum VideoError {
    #[error("FFmpeg not found")]
    FfmpegNotFound,
    #[error("FFprobe not found")]
    FfprobeNotFound,
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("File is locked: {0}")]
    FileLocked(String),
    #[error("Already processing: {0}")]
    AlreadyProcessing(String),
    #[error("Processing failed: {stderr}")]
    ProcessingFailed { stderr: String },
    #[error("Whisper binary not found")]
    WhisperNotFound,
    #[error("Whisper model not found")]
    WhisperModelNotFound,
    #[error("Whisper processing failed: {stderr}")]
    WhisperFailed { stderr: String },
    #[error("Subtitle parse error: {0}")]
    SubtitleParseError(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Lock error: {0}")]
    LockError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Tauri error: {0}")]
    TauriError(#[from] tauri::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct StructuredError {
    pub code: String,
    pub message: String,
}

impl From<VideoError> for StructuredError {
    fn from(error: VideoError) -> Self {
        let code = match &error {
            VideoError::InvalidInput(_) => "invalid_config",
            VideoError::FileNotFound(_) => "file_not_found",
            VideoError::FileLocked(_) => "file_locked",
            VideoError::AlreadyProcessing(_) => "already_processing",
            VideoError::FfmpegNotFound => "ffmpeg_not_found",
            VideoError::FfprobeNotFound => "ffprobe_not_found",
            VideoError::WhisperNotFound => "whisper_not_found",
            VideoError::WhisperModelNotFound => "whisper_model_not_found",
            VideoError::WhisperFailed { .. } => "subtitle_generation_failed",
            VideoError::SubtitleParseError(_) => "subtitle_parse_error",
            VideoError::ProcessingFailed { .. } => "processing_failed",
            VideoError::LockError(_) => "lock_error",
            VideoError::IoError(_) => "io_error",
            VideoError::JsonError(_) => "json_error",
            VideoError::TauriError(_) => "tauri_error",
        }
        .to_string();

        Self {
            code,
            message: error.to_string(),
        }
    }
}

impl From<VideoError> for String {
    fn from(error: VideoError) -> Self {
        error.to_string()
    }
}
