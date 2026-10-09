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
    /// Clamp unexpectedly high automatic output frame rates without upsampling
    /// lower-frame-rate sources. Omitted means preserve the existing FPS policy.
    #[serde(default)]
    pub max_frame_rate: Option<u32>,
    /// Optional codec-specific VBV ceiling. Applied only to H.264 outputs so
    /// CRF remains the quality control while limiting short-term bitrate peaks.
    #[serde(default)]
    pub video_max_rate: Option<String>,
    /// Required together with video_max_rate when a VBV ceiling is configured.
    #[serde(default)]
    pub video_buffer_size: Option<String>,
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

/// Crop region as fractions of the source image dimensions.
///
/// `x/y` is the top-left of the visible region, `width/height` its size.
/// Default `{0,0,1,1}` means the full source image is visible.
/// Crop is independent from transform (position/scale/rotation/flip):
/// `source -> crop -> flip -> scale -> rotation -> position -> frame clipping`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImageCrop {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default = "default_image_crop_dimension")]
    pub width: f32,
    #[serde(default = "default_image_crop_dimension")]
    pub height: f32,
}

impl Default for ImageCrop {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }
    }
}

fn default_image_crop_dimension() -> f32 {
    1.0
}

fn default_image_overlay_scale() -> f32 {
    0.25
}

fn default_image_overlay_opacity() -> f32 {
    1.0
}

/// A single independent image object on the video canvas.
///
/// Canonical geometry is unbounded video-space: `x/y` is the center anchor
/// (any finite value, including outside `0..1`; the frame only clips
/// visibility), `scale` is the width as a fraction of the video width,
/// `rotation` is degrees. `path` is the source file (static image or GIF;
/// GIFs are image overlays, not a separate domain).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImageOverlay {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default = "default_text_overlay_position")]
    pub x: f32,
    #[serde(default = "default_text_overlay_position")]
    pub y: f32,
    #[serde(default = "default_image_overlay_scale")]
    pub scale: f32,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default = "default_image_overlay_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub flip_horizontal: bool,
    #[serde(default)]
    pub flip_vertical: bool,
    #[serde(default)]
    pub crop: ImageCrop,
}

impl Default for ImageOverlay {
    fn default() -> Self {
        Self {
            id: String::new(),
            path: String::new(),
            x: default_text_overlay_position(),
            y: default_text_overlay_position(),
            scale: default_image_overlay_scale(),
            rotation: 0.0,
            opacity: default_image_overlay_opacity(),
            flip_horizontal: false,
            flip_vertical: false,
            crop: ImageCrop::default(),
        }
    }
}

/// Image-overlay collection with single selection.
///
/// `selected_overlay_id` is a single ID (not a list): clicking an image
/// selects it, clicking another switches selection, the panel edits the
/// selected image. No image list lives in the panel; the canvas is the
/// object browser.
#[derive(Debug, Serialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImageOverlaySettings {
    #[serde(default)]
    pub panel_open: bool,
    #[serde(default)]
    pub overlays: Vec<ImageOverlay>,
    #[serde(default)]
    pub selected_overlay_id: Option<String>,
}

impl Default for ImageOverlaySettings {
    fn default() -> Self {
        Self {
            panel_open: false,
            overlays: Vec::new(),
            selected_overlay_id: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageOverlayContainerWire {
    #[serde(default)]
    panel_open: bool,
    #[serde(default)]
    overlays: Vec<ImageOverlay>,
    #[serde(default)]
    selected_overlay_id: Option<String>,
}

fn fallback_image_overlay_id(index: usize) -> String {
    format!("image-{}", index + 1)
}

impl ImageOverlaySettings {
    fn normalized(mut self) -> Self {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for (index, overlay) in self.overlays.iter_mut().enumerate() {
            if overlay.id.trim().is_empty() {
                overlay.id = fallback_image_overlay_id(index);
            }
            if !seen.insert(overlay.id.clone()) {
                overlay.id = format!("{}-{}", overlay.id, index + 1);
                seen.insert(overlay.id.clone());
            }
        }
        let ids: HashSet<String> = self.overlays.iter().map(|o| o.id.clone()).collect();
        if let Some(selected) = self.selected_overlay_id.clone() {
            if !ids.contains(&selected) {
                self.selected_overlay_id = None;
            }
        }
        self
    }
}

impl<'de> Deserialize<'de> for ImageOverlaySettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let container =
            ImageOverlayContainerWire::deserialize(deserializer)?;
        Ok(ImageOverlaySettings {
            panel_open: container.panel_open,
            overlays: container.overlays,
            selected_overlay_id: container.selected_overlay_id,
        }
        .normalized())
    }
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
    #[serde(default)]
    pub rotation: f32,
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
            rotation: 0.0,
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
        // Container-vs-legacy rule: an object carrying ANY container key
        // (`layers`, `panelOpen`, `selectedLayerIds`) is a container, even
        // with zero layers. This must stay aligned with the frontend mirror
        // (`src/utils/textOverlay.ts`, `isTextOverlayContainer`).
        // Contract: frontend normalization is edit-time convenience;
        // backend validation (`video::validation`) is render authority.
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
                        rotation: 0.0,
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
    pub background_color: Option<String>,
    pub overlays: Option<Vec<String>>,
    pub subtitles: Option<String>,
    pub color_filter: Option<String>,
    pub blur_sigma: Option<f32>,
    pub remove_audio: Option<bool>,
    pub export_subtitles: Option<bool>,
    pub burn_subtitles: Option<bool>,
    pub skip_existing: Option<bool>,
    pub output_format: Option<OutputFormat>,
    #[serde(default)]
    pub image_overlay: ImageOverlaySettings,
    #[serde(default)]
    pub text_overlay: TextOverlaySettings,
    #[serde(default)]
    pub subtitle_overlay: SubtitleOverlaySettings,
    pub transform: Option<VideoTransform>,
}

fn default_background_color() -> String {
    "#000000".to_string()
}

fn is_background_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

impl VideoEffectsSettings {
    pub fn blur_enabled(&self) -> bool {
        self.blur.unwrap_or(false) && !self.white_background_enabled()
    }

    /// Background Color enabled flag.
    ///
    /// Backed by the legacy `whiteBackground` boolean for persisted-config
    /// and IPC compatibility; `true` means "fill the letterbox with
    /// `background_color_value()`" (default black).
    pub fn white_background_enabled(&self) -> bool {
        self.white_background.unwrap_or(false)
    }

    /// Selected background color in `#RRGGBB`. Falls back to black when
    /// unset or malformed.
    pub fn background_color_value(&self) -> String {
        match &self.background_color {
            Some(color) if is_background_hex_color(color) => color.clone(),
            _ => default_background_color(),
        }
    }

    /// `background_color_value()` in FFmpeg `color=` syntax (`0xRRGGBB`).
    /// The `#` form is avoided: `#` starts a comment inside filtergraphs.
    pub fn background_color_ffmpeg(&self) -> String {
        format!("0x{}", self.background_color_value().trim_start_matches('#'))
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

    pub fn image_overlay_enabled(&self) -> bool {
        self.image_overlay
            .overlays
            .iter()
            .any(|overlay| !overlay.path.trim().is_empty())
    }

    pub fn image_overlays(&self) -> Vec<ImageOverlay> {
        self.image_overlay
            .overlays
            .iter()
            .filter(|overlay| !overlay.path.trim().is_empty())
            .cloned()
            .collect()
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
    pub image_overlay: Option<ImageOverlaySettings>,
    pub text_overlay: Option<TextOverlaySettings>,
    pub subtitle_overlay: Option<SubtitleOverlaySettings>,
    pub blur: Option<bool>,
    pub white_background: Option<bool>,
    pub background_color: Option<String>,
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
    /// Canonical baseline encoding (the selected preset's tuned values).
    ///
    /// This is NOT the effective render encoding. Rust resolves the effective
    /// profile at the render boundary via
    /// [`crate::video::encoding::resolve_effective_encoding`] using
    /// `encoding_overrides` below.
    pub encoding: EncodingProfile,
    /// Transient session intent. Empty means pure preset baseline.
    #[serde(default)]
    pub encoding_overrides: crate::video::encoding::EncodingOverrides,
    pub effects: VideoEffectsSettings,
    pub platform_config: Option<PlatformConfig>,
    pub selection: SelectionMetadata,
    /// Legacy intake, ignored by render construction.
    ///
    /// Re-encode intent is derived in Rust from `encoding_overrides` via
    /// [`crate::video::encoding::has_explicit_encoding_intent`]. This field is
    /// retained so older payloads still deserialize; nothing may trust it.
    #[serde(default)]
    pub force_reencode: bool,
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
    /// Derived in Rust from the resolved `EncodingOverrides` (never trusted
    /// from frontend intake). When `true`, the `-c copy` passthrough is
    /// prohibited even if the input geometry would otherwise allow it.
    pub force_reencode: bool,
    /// FFmpeg `-threads` override, exposed as a *capability* only.
    ///
    /// Production always sets this to `None` (architecture_fix Stage 1/2/6):
    /// the parallel-architecture rework removed the per-job thread hint from
    /// `ConcurrencyPlan`, so batch renders run with FFmpeg's own AUTO threading
    /// and `-threads` is never emitted. The `Option` remains so non-batch paths
    /// (e.g. single-video `convert_to_ratio`) and any future hardware-specific
    /// path can still force a value without changing the builder's shape.
    pub threads_per_job: Option<usize>,
}

impl ResolvedJob {
    /// Authoritative render-boundary constructor (Issues #1/#4/#7).
    ///
    /// Runs `validate → resolve effective encoding → derive re-encode intent`
    /// so no render job can reach the pipeline without Rust-side resolution.
    /// The incoming `OutputJob.force_reencode` is deliberately ignored and
    /// recomputed from `encoding_overrides`.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_for_render(
        job_id: String,
        session_id: String,
        input_path: String,
        output_path: String,
        alt_output_path: Option<String>,
        output: &OutputJob,
        subtitle_path: Option<std::path::PathBuf>,
        subtitle_fonts_dir: Option<std::path::PathBuf>,
        threads_per_job: Option<usize>,
    ) -> Result<Self, VideoError> {
        crate::video::validation::validate_output_job(output)?;
        let effective = crate::video::encoding::resolve_effective_encoding(
            &output.encoding,
            &output.encoding_overrides,
        )?;
        let force_reencode =
            crate::video::encoding::has_explicit_encoding_intent(&output.encoding_overrides);
        Ok(Self {
            id: job_id,
            session_id,
            input_path,
            output_path,
            alt_output_path,
            ratio: output.ratio.clone(),
            encoding: effective,
            effects: output.effects.clone(),
            platform_config: output.platform_config.clone(),
            subtitle_path,
            subtitle_fonts_dir,
            force_reencode,
            threads_per_job,
        })
    }

    /// Layout-only constructor for preview/subtitle measurement.
    ///
    /// Never rendered: no validation, no resolution, `force_reencode` is
    /// always `false`. Keeps layout paths from reusing render construction.
    pub fn for_layout(
        id: String,
        input_path: String,
        ratio: AspectRatio,
        encoding: EncodingProfile,
        effects: VideoEffectsSettings,
        platform_config: Option<PlatformConfig>,
    ) -> Self {
        Self {
            id,
            session_id: String::new(),
            input_path,
            output_path: String::new(),
            alt_output_path: None,
            ratio,
            encoding,
            effects,
            platform_config,
            subtitle_path: None,
            subtitle_fonts_dir: None,
            force_reencode: false,
            threads_per_job: None,
        }
    }
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
    pub image: bool,
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
        if self.image {
            tags.push("image".to_string());
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

/// Resolved image for the render pipeline (internal, not IPC).
///
/// Carries the canonical transform through to FFmpeg:
/// `source -> crop -> flip -> scale -> rotation -> position -> frame clipping`.
/// `is_gif` marks animated image overlays (GIFs are image overlays, not a
/// separate domain); the renderer loops them as an animation layer while the
/// video continues underneath.
#[derive(Debug, Clone)]
pub struct ImagePreset {
    pub path: String,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub crop: ImageCrop,
    pub is_gif: bool,
}

impl ImagePreset {
    pub fn is_gif_path(path: &str) -> bool {
        path.trim().to_ascii_lowercase().ends_with(".gif")
    }
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
    fn panel_open_without_layers_is_an_empty_container_not_a_legacy_layer() {
        // Must stay aligned with the frontend `isTextOverlayContainer`
        // (`src/utils/textOverlay.ts`): `{ panelOpen: true }` with no
        // `layers` is a container with zero layers, not a legacy single
        // layer. The old frontend rule (layers-array presence only)
        // materialized a phantom "Add Text" layer here.
        let overlay: TextOverlaySettings = serde_json::from_str(r#"{"panelOpen":true}"#)
            .expect("panel-only overlay should load");
        assert!(overlay.layers.is_empty());
        assert!(overlay.panel_open);
        assert!(overlay.selected_layer_ids.is_empty());
    }

    #[test]
    fn empty_object_and_bare_text_carry_no_legacy_layer() {
        // Mirrors the frontend `resolveTextOverlay` legacy branch: the
        // legacy wire defaults `enabled` to false, so shapes without an
        // explicit `enabled: true` plus non-empty `text` collapse to empty.
        for raw in [r#"{}"#, r#"{"text":"Hi"}"#, r#"{"enabled":false,"text":"Hi"}"#] {
            let overlay: TextOverlaySettings =
                serde_json::from_str(raw).expect("shape should load");
            assert!(overlay.layers.is_empty(), "{raw}");
        }
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
        // Legacy logo keys are unknown fields and must be ignored (Logo ->
        // ImageOverlay migration): old configs still load.
        let config: AppConfig = serde_json::from_str(json).expect("old config should load");
        assert!(config.text_overlay.is_none());
        assert!(config.subtitle_overlay.is_none());
        assert!(config.image_overlay.is_none());
    }

    #[test]
    fn old_effects_with_legacy_logo_field_still_deserializes_without_logo() {
        let effects: VideoEffectsSettings = serde_json::from_str(
            r#"{"blur":false,"logo":{"enabled":true,"position":"bottom_right","opacity":1.0,"gap":20,"scale":0.15,"path":"logo.png"}}"#,
        )
        .expect("old effects with logo should load");
        assert!(!effects.image_overlay_enabled());
        assert!(effects.image_overlay.overlays.is_empty());
    }

    #[test]
    fn image_overlay_survives_save_and_reload_with_unbounded_geometry() {
        use super::{ImageCrop, ImageOverlay, ImageOverlaySettings};
        let overlay = ImageOverlaySettings {
            panel_open: true,
            overlays: vec![
                ImageOverlay {
                    id: "img-1".to_string(),
                    path: "a.png".to_string(),
                    x: -0.4,
                    y: 1.2,
                    scale: 0.35,
                    rotation: 15.0,
                    opacity: 0.8,
                    flip_horizontal: true,
                    flip_vertical: false,
                    crop: ImageCrop {
                        x: 0.1,
                        y: 0.1,
                        width: 0.8,
                        height: 0.8,
                    },
                },
                ImageOverlay {
                    id: "img-2".to_string(),
                    path: "b.gif".to_string(),
                    x: 2.0,
                    y: 0.5,
                    scale: 0.5,
                    rotation: -30.0,
                    opacity: 1.0,
                    flip_horizontal: false,
                    flip_vertical: true,
                    crop: ImageCrop::default(),
                },
            ],
            selected_overlay_id: Some("img-2".to_string()),
        };
        let saved = serde_json::to_string(&overlay).expect("overlay should serialize");
        let reloaded: ImageOverlaySettings =
            serde_json::from_str(&saved).expect("overlay should deserialize");
        assert_eq!(reloaded, overlay);
    }

    #[test]
    fn image_overlay_dangling_selection_is_cleared() {
        use super::ImageOverlaySettings;
        let overlay: ImageOverlaySettings = serde_json::from_str(
            r#"{"panelOpen":true,"overlays":[],"selectedOverlayId":"missing"}"#,
        )
        .expect("overlay should deserialize");
        assert!(overlay.selected_overlay_id.is_none());
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
    fn render_boundary_resolves_baseline_plus_overrides_and_derives_intent() {
        use super::{EncodingProfile, OutputJob, ResolvedJob, SelectionMetadata};
        use crate::video::encoding::{EncodingOverrides, QualityAuthority};

        let baseline = EncodingProfile {
            crf: 18,
            quality_preset: "high".to_string(),
            speed_preset: "slow".to_string(),
            audio_bitrate: "192k".to_string(),
        };
        let output = OutputJob {
            id: "out-1".to_string(),
            ratio: super::AspectRatio::Ratio9x16,
            encoding: baseline,
            encoding_overrides: EncodingOverrides {
                crf: Some(28),
                quality_preset: Some("balanced".to_string()),
                quality_authority: QualityAuthority::ManualCrf,
                ..Default::default()
            },
            effects: serde_json::from_str("{}").expect("default effects"),
            platform_config: None,
            selection: SelectionMetadata {
                source_type: super::TargetType::AspectRatio,
                source_id: "ratio9x16".to_string(),
                label: "9:16".to_string(),
            },
            // Legacy intake must be ignored: overrides carry intent, so the
            // derived value is true even though this says false.
            force_reencode: false,
        };
        let resolved = ResolvedJob::resolve_for_render(
            "job-1".to_string(),
            "session-1".to_string(),
            "in.mp4".to_string(),
            "out.mp4".to_string(),
            None,
            &output,
            None,
            None,
            None,
        )
        .expect("valid render request must resolve");
        assert_eq!(resolved.encoding.crf, 28);
        assert_eq!(resolved.encoding.quality_preset, "balanced");
        assert_eq!(resolved.encoding.speed_preset, "slow");
        assert!(resolved.force_reencode);
    }

    #[test]
    fn render_boundary_without_overrides_keeps_baseline_and_passthrough() {
        use super::{EncodingProfile, OutputJob, ResolvedJob, SelectionMetadata};
        use crate::video::encoding::EncodingOverrides;

        let output = OutputJob {
            id: "out-2".to_string(),
            ratio: super::AspectRatio::Ratio9x16,
            encoding: EncodingProfile::standard(),
            encoding_overrides: EncodingOverrides::baseline(),
            effects: serde_json::from_str("{}").expect("default effects"),
            platform_config: None,
            selection: SelectionMetadata {
                source_type: super::TargetType::AspectRatio,
                source_id: "ratio9x16".to_string(),
                label: "9:16".to_string(),
            },
            // A stale `true` here must not force re-encode on its own.
            force_reencode: true,
        };
        let resolved = ResolvedJob::resolve_for_render(
            "job-2".to_string(),
            "session-1".to_string(),
            "in.mp4".to_string(),
            "out.mp4".to_string(),
            None,
            &output,
            None,
            None,
            None,
        )
        .expect("baseline request must resolve");
        assert_eq!(resolved.encoding, EncodingProfile::standard());
        assert!(!resolved.force_reencode);
    }

    #[test]
    fn render_boundary_rejects_invalid_overrides_before_any_job() {
        use super::{EncodingProfile, OutputJob, ResolvedJob, SelectionMetadata};
        use crate::video::encoding::{EncodingOverrides, QualityAuthority};

        let output = OutputJob {
            id: "out-3".to_string(),
            ratio: super::AspectRatio::Ratio9x16,
            encoding: EncodingProfile::standard(),
            encoding_overrides: EncodingOverrides {
                quality_preset: Some("ultra".to_string()),
                quality_authority: QualityAuthority::QualityPreset,
                ..Default::default()
            },
            effects: serde_json::from_str("{}").expect("default effects"),
            platform_config: None,
            selection: SelectionMetadata {
                source_type: super::TargetType::AspectRatio,
                source_id: "ratio9x16".to_string(),
                label: "9:16".to_string(),
            },
            force_reencode: false,
        };
        assert!(ResolvedJob::resolve_for_render(
            "job-3".to_string(),
            "session-1".to_string(),
            "in.mp4".to_string(),
            "out.mp4".to_string(),
            None,
            &output,
            None,
            None,
            None,
        )
        .is_err());
    }

    #[test]
    fn legacy_output_job_without_overrides_still_deserializes() {
        use super::OutputJob;
        let job: OutputJob = serde_json::from_str(
            r#"{"id":"o","ratio":"ratio9x16","encoding":{"crf":18,"qualityPreset":"high","speedPreset":"slow","audioBitrate":"192k"},"effects":{},"platformConfig":null,"selection":{"sourceType":"platform","sourceId":"youtube","label":"YouTube"}}"#,
        )
        .expect("legacy job should load");
        assert_eq!(job.encoding.crf, 18);
        assert!(!job.force_reencode);
    }

    #[test]
    fn output_suffix_distinguishes_text_overlay_renders() {
        let tags = OutputTags {
            ratio: "9x16".to_string(),
            platform: None,
            blur: false,
            white_background: false,
            image: false,
            text: true,
            subtitles: false,
            no_audio: false,
        };
        assert_eq!(tags.to_suffix(), "9x16_text");
    }

    #[test]
    fn output_suffix_distinguishes_image_overlay_renders() {
        let tags = OutputTags {
            ratio: "9x16".to_string(),
            platform: None,
            blur: false,
            white_background: false,
            image: true,
            text: false,
            subtitles: false,
            no_audio: false,
        };
        assert_eq!(tags.to_suffix(), "9x16_image");
    }
}
