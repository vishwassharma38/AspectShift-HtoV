use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum AspectRatio {
    Ratio9x16,
    Ratio1x1,
    Ratio4x5,
    Ratio2x3,
}

impl AspectRatio {
    pub fn get_ratio(&self) -> f32 {
        match self {
            AspectRatio::Ratio9x16 => 9.0 / 16.0,
            AspectRatio::Ratio1x1 => 1.0 / 1.0,
            AspectRatio::Ratio4x5 => 4.0 / 5.0,
            AspectRatio::Ratio2x3 => 2.0 / 3.0,
        }
    }

    pub fn get_tag(&self) -> &'static str {
        match self {
            AspectRatio::Ratio9x16 => "9:16",
            AspectRatio::Ratio1x1 => "1:1",
            AspectRatio::Ratio4x5 => "4:5",
            AspectRatio::Ratio2x3 => "2:3",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConversionOptions {
    pub blur_background: bool,
    pub blur_sigma: f32,
    pub remove_audio: bool,
    pub skip_existing: bool,
    pub quality: QualityPreset,
    pub output_format: OutputFormat,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            blur_background: false,
            blur_sigma: 20.0,
            remove_audio: false,
            skip_existing: true,
            quality: QualityPreset::Standard,
            output_format: OutputFormat::Mp4,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchConversionResult {
    pub results: Vec<Result<ConversionResult, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileReadiness {
    pub exists: bool,
    pub is_readable: bool,
    pub file_size_bytes: u64,
    pub is_locked: bool,
    pub estimated_duration_secs: f64,
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

impl From<VideoError> for String {
    fn from(error: VideoError) -> Self {
        error.to_string()
    }
}
