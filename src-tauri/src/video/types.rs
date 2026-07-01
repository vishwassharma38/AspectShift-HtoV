use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize};
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
    #[serde(default)]
    pub manual_position: bool,
    #[serde(default = "default_text_overlay_position")]
    pub x: f32,
    #[serde(default = "default_text_overlay_position")]
    pub y: f32,
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
            manual_position: false,
            x: default_text_overlay_position(),
            y: default_text_overlay_position(),
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

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TextFontStyle {
    #[default]
    Clean,
    Minimal,
    Caption,
    Meme,
    Creator,
    Gaming,
    Cyberpunk,
    Cinematic,
    Retro,
    Handwritten,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct TextLayerSettings {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_text_overlay_text")]
    pub text: String,
    #[serde(default)]
    pub font_style: TextFontStyle,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strikethrough: bool,
    #[serde(default = "default_text_overlay_font_size")]
    pub font_size: i32,
    #[serde(default = "default_text_overlay_color")]
    pub color: String,
    #[serde(default = "default_text_overlay_opacity")]
    pub opacity: f32,
    #[serde(default = "default_text_overlay_position")]
    pub x: f32,
    #[serde(default = "default_text_overlay_position")]
    pub y: f32,
    #[serde(default = "default_text_overlay_outline_enabled")]
    pub outline_enabled: bool,
    #[serde(default = "default_text_overlay_outline_color")]
    pub outline_color: String,
    #[serde(default = "default_text_overlay_outline_width")]
    pub outline_width: i32,
}

fn default_text_overlay_text() -> String {
    "Add Text".to_string()
}

fn default_text_overlay_font_size() -> i32 {
    48
}

fn default_text_overlay_color() -> String {
    "#ffffff".to_string()
}

fn default_text_overlay_opacity() -> f32 {
    1.0
}

fn default_text_overlay_position() -> f32 {
    0.5
}

fn default_text_overlay_outline_enabled() -> bool {
    true
}

fn default_text_overlay_outline_color() -> String {
    "#000000".to_string()
}

fn default_text_overlay_outline_width() -> i32 {
    3
}

fn default_subtitle_overlay_bold() -> bool {
    true
}

fn default_subtitle_overlay_color() -> String {
    "#ffffff".to_string()
}

fn default_subtitle_overlay_opacity() -> f32 {
    1.0
}

fn default_subtitle_overlay_outline_enabled() -> bool {
    true
}

fn default_subtitle_overlay_outline_color() -> String {
    "#000000".to_string()
}

fn default_subtitle_overlay_position_y() -> f32 {
    0.86
}

impl Default for TextLayerSettings {
    fn default() -> Self {
        Self {
            id: String::new(),
            enabled: true,
            text: default_text_overlay_text(),
            font_style: TextFontStyle::default(),
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            font_size: default_text_overlay_font_size(),
            color: default_text_overlay_color(),
            opacity: default_text_overlay_opacity(),
            x: default_text_overlay_position(),
            y: default_text_overlay_position(),
            outline_enabled: default_text_overlay_outline_enabled(),
            outline_color: default_text_overlay_outline_color(),
            outline_width: default_text_overlay_outline_width(),
        }
    }
}

#[derive(Debug, Serialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct TextOverlaySettings {
    #[serde(default)]
    pub panel_open: bool,
    #[serde(default)]
    pub layers: Vec<TextLayerSettings>,
    #[serde(default)]
    pub selected_layer_ids: Vec<String>,
}

impl Default for TextOverlaySettings {
    fn default() -> Self {
        Self {
            panel_open: false,
            layers: Vec::new(),
            selected_layer_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextOverlayContainerWire {
    #[serde(default)]
    panel_open: bool,
    #[serde(default)]
    layers: Vec<TextLayerSettings>,
    #[serde(default)]
    selected_layer_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyTextOverlayWire {
    #[serde(default)]
    enabled: bool,
    #[serde(default = "default_text_overlay_text")]
    text: String,
    #[serde(default)]
    font_style: TextFontStyle,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    italic: bool,
    #[serde(default)]
    underline: bool,
    #[serde(default)]
    strikethrough: bool,
    #[serde(default = "default_text_overlay_font_size")]
    font_size: i32,
    #[serde(default = "default_text_overlay_color")]
    color: String,
    #[serde(default = "default_text_overlay_opacity")]
    opacity: f32,
    #[serde(default = "default_text_overlay_position")]
    x: f32,
    #[serde(default = "default_text_overlay_position")]
    y: f32,
    #[serde(default = "default_text_overlay_outline_enabled")]
    outline_enabled: bool,
    #[serde(default = "default_text_overlay_outline_color")]
    outline_color: String,
    #[serde(default = "default_text_overlay_outline_width")]
    outline_width: i32,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TextOverlayWire {
    Container(TextOverlayContainerWire),
    Legacy(LegacyTextOverlayWire),
}

fn fallback_text_layer_id(index: usize) -> String {
    if index == 0 {
        "legacy-text-overlay".to_string()
    } else {
        format!("text-layer-{}", index + 1)
    }
}

impl TextOverlaySettings {
    fn normalized(mut self) -> Self {
        use std::collections::HashSet;

        let mut seen = HashSet::new();
        for (index, layer) in self.layers.iter_mut().enumerate() {
            if layer.id.trim().is_empty() {
                layer.id = fallback_text_layer_id(index);
            }
            if !seen.insert(layer.id.clone()) {
                layer.id = format!("{}-{}", layer.id, index + 1);
                seen.insert(layer.id.clone());
            }
        }
        let ids: HashSet<String> = self.layers.iter().map(|layer| layer.id.clone()).collect();
        self.selected_layer_ids
            .retain(|selected_id| ids.contains(selected_id));
        self
    }
}

impl<'de> Deserialize<'de> for TextOverlaySettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let is_container = value.get("layers").is_some()
            || value.get("panelOpen").is_some()
            || value.get("selectedLayerIds").is_some();
        let wire = if is_container {
            TextOverlayWire::Container(serde_json::from_value(value).map_err(D::Error::custom)?)
        } else {
            TextOverlayWire::Legacy(serde_json::from_value(value).map_err(D::Error::custom)?)
        };
        let overlay = match wire {
            TextOverlayWire::Container(container) => TextOverlaySettings {
                panel_open: container.panel_open,
                layers: container.layers,
                selected_layer_ids: container.selected_layer_ids,
            },
            TextOverlayWire::Legacy(legacy) => {
                if !legacy.enabled || legacy.text.trim().is_empty() {
                    TextOverlaySettings::default()
                } else {
                    let layer = TextLayerSettings {
                        id: fallback_text_layer_id(0),
                        enabled: true,
                        text: legacy.text,
                        font_style: legacy.font_style,
                        bold: legacy.bold,
                        italic: legacy.italic,
                        underline: legacy.underline,
                        strikethrough: legacy.strikethrough,
                        font_size: legacy.font_size,
                        color: legacy.color,
                        opacity: legacy.opacity,
                        x: legacy.x,
                        y: legacy.y,
                        outline_enabled: legacy.outline_enabled,
                        outline_color: legacy.outline_color,
                        outline_width: legacy.outline_width,
                    };
                    TextOverlaySettings {
                        panel_open: true,
                        selected_layer_ids: vec![layer.id.clone()],
                        layers: vec![layer],
                    }
                }
            }
        };
        Ok(overlay.normalized())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleOverlaySettings {
    #[serde(default)]
    pub font_style: TextFontStyle,
    #[serde(default = "default_subtitle_overlay_bold")]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub font_size: Option<i32>,
    #[serde(default = "default_subtitle_overlay_color")]
    pub color: String,
    #[serde(default = "default_subtitle_overlay_opacity")]
    pub opacity: f32,
    #[serde(default = "default_subtitle_overlay_outline_enabled")]
    pub outline_enabled: bool,
    #[serde(default = "default_subtitle_overlay_outline_color")]
    pub outline_color: String,
    #[serde(default)]
    pub outline_width: Option<i32>,
    #[serde(default)]
    pub manual_position: bool,
    #[serde(default = "default_text_overlay_position")]
    pub x: f32,
    #[serde(default = "default_subtitle_overlay_position_y")]
    pub y: f32,
}

impl Default for SubtitleOverlaySettings {
    fn default() -> Self {
        Self {
            font_style: TextFontStyle::default(),
            bold: default_subtitle_overlay_bold(),
            italic: false,
            font_size: None,
            color: default_subtitle_overlay_color(),
            opacity: default_subtitle_overlay_opacity(),
            outline_enabled: default_subtitle_overlay_outline_enabled(),
            outline_color: default_subtitle_overlay_outline_color(),
            outline_width: None,
            manual_position: false,
            x: default_text_overlay_position(),
            y: default_subtitle_overlay_position_y(),
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

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoEffectsSettings {
    pub blur: Option<bool>,
    pub white_background: Option<bool>,
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
    #[serde(default)]
    pub text_overlay: TextOverlaySettings,
    #[serde(default)]
    pub subtitle_overlay: SubtitleOverlaySettings,
    pub transform: Option<VideoTransform>,
}

impl VideoEffectsSettings {
    pub fn blur_enabled(&self) -> bool {
        self.blur.unwrap_or(false) && !self.white_background_enabled()
    }

    pub fn white_background_enabled(&self) -> bool {
        self.white_background.unwrap_or(false)
    }

    pub fn background_effect_enabled(&self) -> bool {
        self.blur_enabled() || self.white_background_enabled()
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

    pub fn text_overlay_enabled(&self) -> bool {
        self.text_overlay
            .layers
            .iter()
            .any(|layer| layer.enabled && !layer.text.trim().is_empty())
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
    pub logo_manual_position: Option<bool>,
    pub logo_x: Option<f32>,
    pub logo_y: Option<f32>,
    pub text_overlay: Option<TextOverlaySettings>,
    pub subtitle_overlay: Option<SubtitleOverlaySettings>,
    pub blur: Option<bool>,
    pub white_background: Option<bool>,
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
    pub subtitle_fonts_dir: Option<std::path::PathBuf>,
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
    pub white_background: bool,
    pub logo: bool,
    pub text: bool,
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
        if self.white_background {
            tags.push("white_bg".to_string());
        }
        if self.logo {
            tags.push("logo".to_string());
        }
        if self.text {
            tags.push("text".to_string());
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
    pub manual_position: bool,
    pub x: f32,
    pub y: f32,
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

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, OutputTags, SubtitleOverlaySettings, TextLayerSettings, TextOverlaySettings,
        VideoEffectsSettings,
    };

    #[test]
    fn old_effects_without_text_overlay_receive_safe_defaults() {
        let effects: VideoEffectsSettings =
            serde_json::from_str(r#"{"blur":false}"#).expect("old effects should load");
        assert_eq!(effects.text_overlay, TextOverlaySettings::default());
        assert_eq!(effects.subtitle_overlay, SubtitleOverlaySettings::default());
        assert!(!effects.subtitle_overlay.manual_position);
        assert!(effects.subtitle_overlay.font_size.is_none());
    }

    #[test]
    fn old_text_overlay_without_formatting_flags_receives_safe_defaults() {
        let overlay: TextOverlaySettings = serde_json::from_str(
            r##"{"enabled":true,"text":"Hello","fontStyle":"clean","fontSize":48,"color":"#ffffff","opacity":1.0,"x":0.5,"y":0.5,"outlineEnabled":true,"outlineColor":"#000000","outlineWidth":3}"##,
        )
        .expect("old text overlay should load");

        assert_eq!(overlay.layers.len(), 1);
        assert_eq!(overlay.selected_layer_ids, vec!["legacy-text-overlay"]);
        let layer = &overlay.layers[0];
        assert!(!layer.bold);
        assert!(!layer.italic);
        assert!(!layer.underline);
        assert!(!layer.strikethrough);
    }

    #[test]
    fn every_font_and_style_combination_survives_save_and_reload() {
        let combinations = [
            (false, false, false, false),
            (true, false, false, false),
            (false, true, false, false),
            (true, true, false, false),
            (false, false, true, false),
            (false, false, false, true),
            (true, true, true, true),
        ];
        for font_style in [
            super::TextFontStyle::Clean,
            super::TextFontStyle::Minimal,
            super::TextFontStyle::Caption,
            super::TextFontStyle::Meme,
            super::TextFontStyle::Creator,
            super::TextFontStyle::Gaming,
            super::TextFontStyle::Cyberpunk,
            super::TextFontStyle::Cinematic,
            super::TextFontStyle::Retro,
            super::TextFontStyle::Handwritten,
        ] {
            for (bold, italic, underline, strikethrough) in combinations {
                let layer = TextLayerSettings {
                    id: "layer-1".to_string(),
                    font_style: font_style.clone(),
                    bold,
                    italic,
                    underline,
                    strikethrough,
                    ..TextLayerSettings::default()
                };
                let overlay = TextOverlaySettings {
                    panel_open: true,
                    layers: vec![layer],
                    selected_layer_ids: vec!["layer-1".to_string()],
                };
                let saved = serde_json::to_string(&overlay).expect("overlay should serialize");
                let reloaded: TextOverlaySettings =
                    serde_json::from_str(&saved).expect("overlay should deserialize");
                assert_eq!(reloaded, overlay);
            }
        }
    }

    #[test]
    fn old_app_config_without_text_overlay_still_deserializes() {
        let json = r#"{
            "lastInputDir": null,
            "lastOutputDir": null,
            "lastPresetId": null,
            "selectedRatioIds": [],
            "selectedPresetIds": [],
            "logoPath": null,
            "logoOpacity": null,
            "logoPosition": null,
            "blur": null,
            "whiteBackground": null,
            "blurSigma": null,
            "enableSubfolders": null,
            "previewVolume": null
        }"#;
        let config: AppConfig = serde_json::from_str(json).expect("old config should load");
        assert!(config.text_overlay.is_none());
        assert!(config.subtitle_overlay.is_none());
    }

    #[test]
    fn subtitle_overlay_survives_save_and_reload_without_text_decoration_fields() {
        let overlay = SubtitleOverlaySettings {
            font_style: super::TextFontStyle::Gaming,
            bold: false,
            italic: true,
            font_size: Some(72),
            color: "#12abef".to_string(),
            opacity: 0.8,
            outline_enabled: true,
            outline_color: "#010203".to_string(),
            outline_width: Some(6),
            manual_position: true,
            x: 0.25,
            y: 0.75,
        };
        let saved = serde_json::to_string(&overlay).expect("overlay should serialize");
        assert!(!saved.contains("underline"));
        assert!(!saved.contains("strikethrough"));
        let reloaded: SubtitleOverlaySettings =
            serde_json::from_str(&saved).expect("overlay should deserialize");
        assert_eq!(reloaded, overlay);
    }

    #[test]
    fn output_suffix_distinguishes_text_overlay_renders() {
        let tags = OutputTags {
            ratio: "9x16".to_string(),
            platform: None,
            blur: false,
            white_background: false,
            logo: false,
            text: true,
            subtitles: false,
            no_audio: false,
        };
        assert_eq!(tags.to_suffix(), "9x16_text");
    }
}
