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
            AspectRatio::Ratio1x1 => 1.0 / 1.0,
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
pub struct PlatformConfig {
    pub target_width: u32,
    pub target_height: u32,
    pub enforce_dimensions: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct ConversionRequestDTO {
    pub input: String,
    pub output_dir: String,
    pub ratio: AspectRatio,
    pub options: PartialConversionOptions,
    #[serde(default)]
    pub platform_config: Option<PlatformConfig>,
}

#[derive(Debug, Clone)]
pub struct ConversionRequest {
    pub input: String,
    pub output_dir: String,
    pub ratio: AspectRatio,
    pub options: ConversionOptions,
    pub platform_config: Option<PlatformConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct ConversionOptions {
    pub blur_background: bool,
    pub blur_sigma: f32,
    pub remove_audio: bool,
    #[serde(default)]
    pub generate_subtitles: bool,
    #[serde(default)]
    pub burn_subtitles: bool,
    pub skip_existing: bool,
    pub quality: QualityPreset,
    pub output_format: OutputFormat,
    pub logo: Option<LogoOptions>,
    pub custom_encoding_enabled: bool,
    pub crf: Option<u8>,
    pub preset: Option<String>,
    pub audio_bitrate: Option<String>,
    #[serde(default)]
    pub transform: Option<VideoTransform>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Type)]
pub struct VideoTransform {
    #[serde(default)]
    pub rotate: i32, // 0, 90, 180, 270
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatchJob {
    pub id: String,
    pub input_path: String,
    pub target_ratio: AspectRatio,
    pub target_preset: Option<String>,
    pub active_effects: ConversionOptions,
    pub platform_config: Option<PlatformConfig>,
    pub resolved_output_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct JobTarget {
    #[serde(default)]
    pub ratio: Option<AspectRatio>,
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub overrides: Option<PartialConversionOptions>,
    // Backward-compat legacy payload fields (frontend-resolved config).
    #[serde(default)]
    pub options: Option<ConversionOptions>,
    #[serde(default)]
    pub platform_config: Option<PlatformConfig>,
    #[serde(default)]
    pub preset_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, Type)]
pub struct PartialConversionOptions {
    pub blur_background: Option<bool>,
    pub blur_sigma: Option<f32>,
    pub remove_audio: Option<bool>,
    pub generate_subtitles: Option<bool>,
    pub burn_subtitles: Option<bool>,
    pub skip_existing: Option<bool>,
    pub quality: Option<QualityPreset>,
    pub output_format: Option<OutputFormat>,
    pub logo: Option<LogoOptions>,
    pub custom_encoding_enabled: Option<bool>,
    pub crf: Option<u8>,
    pub preset: Option<String>,
    pub audio_bitrate: Option<String>,
    pub transform: Option<VideoTransform>,
}

impl From<PartialConversionOptions> for ConversionOptions {
    fn from(partial: PartialConversionOptions) -> Self {
        let mut options = ConversionOptions::default();
        if let Some(v) = partial.blur_background { options.blur_background = v; }
        if let Some(v) = partial.blur_sigma { options.blur_sigma = v; }
        if let Some(v) = partial.remove_audio { options.remove_audio = v; }
        if let Some(v) = partial.generate_subtitles { options.generate_subtitles = v; }
        if let Some(v) = partial.burn_subtitles { options.burn_subtitles = v; }
        if let Some(v) = partial.skip_existing { options.skip_existing = v; }
        if let Some(v) = partial.quality { options.quality = v; }
        if let Some(v) = partial.output_format { options.output_format = v; }
        if let Some(v) = partial.logo { options.logo = Some(v); }
        if let Some(v) = partial.custom_encoding_enabled { options.custom_encoding_enabled = v; }
        if let Some(v) = partial.crf { options.crf = Some(v); }
        if let Some(v) = partial.preset { options.preset = Some(v); }
        if let Some(v) = partial.audio_bitrate { options.audio_bitrate = Some(v); }
        if let Some(v) = partial.transform { options.transform = Some(v); }
        options
    }
}

impl From<ConversionRequestDTO> for ConversionRequest {
    fn from(dto: ConversionRequestDTO) -> Self {
        Self {
            input: dto.input,
            output_dir: dto.output_dir,
            ratio: dto.ratio,
            options: dto.options.into(),
            platform_config: dto.platform_config,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct BatchJobSettings {
    pub targets: Vec<JobTarget>,
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

        // Ratio is ALWAYS first and ALWAYS present
        tags.push(self.ratio.clone());

        // Platform tag only included if preset is active
        if let Some(platform) = &self.platform {
            tags.push(platform.clone());
        }

        // Effect tags included ONLY when enabled
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

        // Tag order is FIXED and NEVER dynamically reordered
        // Tags are appended in a deterministic chain
        tags.join("_")
    }

    pub fn get_output_filename(&self, stem: &str, extension: &str) -> String {
        format!("{}_{}.{}", stem, self.to_suffix(), extension)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Processing,
    Completed,
    #[serde(rename = "error")]
    Failed(String),
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct FileProgress {
    pub job_id: String,
    pub file_path: String,
    pub ratio: AspectRatio,
    pub progress: f32,
    pub status: JobStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct BatchProgress {
    pub total_jobs: usize,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub percentage: f32,
    pub current_job_id: Option<String>,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            blur_background: false,
            blur_sigma: 20.0,
            remove_audio: false,
            generate_subtitles: false,
            burn_subtitles: false,
            skip_existing: true,
            quality: QualityPreset::Standard,
            output_format: OutputFormat::Mp4,
            logo: None,
            custom_encoding_enabled: false,
            crf: Some(18),
            preset: Some("medium".to_string()),
            audio_bitrate: Some("128k".to_string()),
            transform: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "snake_case")]
pub enum QualityPreset {
    Draft,
    Standard,
    High,
}

impl QualityPreset {
    pub fn get_ffmpeg_args(&self) -> Vec<&'static str> {
        match self {
            QualityPreset::Draft => vec!["-preset", "veryfast", "-crf", "28"],
            QualityPreset::Standard => vec!["-preset", "medium", "-crf", "23"],
            QualityPreset::High => vec!["-preset", "slow", "-crf", "18"],
        }
    }
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

#[derive(Debug, Serialize, Deserialize, Type)]
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
pub struct FileReadiness {
    pub exists: bool,
    pub is_readable: bool,
    pub file_size_bytes: u64,
    pub is_locked: bool,
    pub estimated_duration_secs: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct VideoPreset {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub ratio: AspectRatio,
    pub options: ConversionOptions,
    pub logo_path: Option<String>,
    pub platform_config: Option<PlatformConfig>,
    pub is_builtin: bool,
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
