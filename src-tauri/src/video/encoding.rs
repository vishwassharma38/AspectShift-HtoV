//! Authoritative encoding-resolution layer.
//!
//! Conceptual hierarchy:
//!
//! ```text
//! Built-in preset baseline (canonical JSON)
//!       ↓
//! Frontend selected baseline + transient EncodingOverrides (user intent)
//!       ↓
//! Rust render boundary: validate + resolve_effective_encoding()
//!       ↓
//! Final resolved EncodingProfile + derived re-encode intent
//!       ↓
//! ResolvedJob (via ResolvedJob::resolve_for_render)
//!       ↓
//! FFmpeg builder (receives resolved CRF, never maps quality names itself)
//! ```
//!
//! Rust is the final authority for the effective encoding. The frontend
//! expresses intent (selected baseline plus transient overrides); it never
//! supplies a pre-resolved answer that the backend trusts. `EncodingOverrides`
//! crosses the IPC boundary; `OutputJob.force_reencode` is legacy intake that
//! render construction recomputes via [`has_explicit_encoding_intent`].
//!
//! ## Quality → CRF mapping
//!
//! Single authoritative source. The frontend mirrors these values in
//! `src/App.tsx` (`QUALITY_LEVELS`); keep the two in sync.
//!
//! Ten levels give meaningful coverage across the CRF 0–51 slider instead of
//! three widely separated choices. Anchors consolidate values the repo
//! already used:
//! - `high → 18` (all `aspect_ratio_presets.json` entries, YouTube/Shorts
//!   presets; x264 visually-transparent neighborhood)
//! - `standard → 23` (`EncodingProfile::standard()`, x264 default)
//! - `lossless → 0` (x264 lossless mode, covers the slider floor)
//! - `very_high → 14` (archive grade), `good → 21` (high-quality streaming
//!   step), `balanced → 28` (smaller files above the default),
//!   `low → 33`, `very_low → 38` (degradation ladder)
//! The bottom two levels carry deliberate semantics rather than a label swap
//! of the old `poor → 44` / `draft → 51` pair:
//! - `draft → 41`: deliberately low-quality temporary/preview render —
//!   watchable but rough, just below Very Low.
//! - `poor → 48`: the poorest level, near the slider floor; its band owns
//!   the extreme values including CRF 51. (Keeping `poor → 44` would strand
//!   the representative at the bottom edge of a 43–51 band, far from the
//!   floor it must represent.)
//!
//! ## CRF → quality bands
//!
//! A Quality Preset represents a *range*; its mapped CRF is the representative
//! value applied on explicit selection. The reverse mapping is *derived* from
//! this same table — never a second hard-coded table: levels are ordered by
//! increasing CRF and each boundary is the integer midpoint
//! `floor((rep_i + rep_{i+1}) / 2)` between consecutive representatives. The
//! first band starts at 0 and the last ends at 51, so every slider value
//! belongs to exactly one band deterministically:
//!
//! ```text
//! lossless  0–7   (rep 0)      very_high 8–16  (rep 14)
//! high      17–19 (rep 18)      good      20–22 (rep 21)
//! standard  23–25 (rep 23)      balanced  26–30 (rep 28)
//! low       31–35 (rep 33)      very_low  36–39 (rep 38)
//! draft     40–44 (rep 41)      poor      45–51 (rep 48)
//! ```
//!
//! ## Authority
//!
//! `quality_preset` and `crf` are two controls over one quality dimension.
//! Exactly one may be authoritative at a time. The most recently
//! explicitly interacted-with control wins. A dropdown value shown while
//! `ManualCrf` holds authority is a *derived* band label, not an override.

use crate::video::types::{EncodingProfile, OutputFormat, VideoError};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Authoritative Quality Preset → representative CRF table, ordered by
/// increasing CRF (decreasing quality). Canonical names are lowercase
/// snake_case; lookups are case-insensitive.
pub const QUALITY_LEVELS: &[(&str, u8)] = &[
    ("lossless", 0),
    ("very_high", 14),
    ("high", 18),
    ("good", 21),
    ("standard", 23),
    ("balanced", 28),
    ("low", 33),
    ("very_low", 38),
    ("draft", 41),
    ("poor", 48),
];

/// Authoritative x264-style speed preset allowlist. Single source of truth;
/// `video::validation` delegates to [`validate_speed_name`].
pub const SPEED_PRESETS: &[&str] = &[
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
];

/// Canonical audio-bitrate options offered by the UI. Validation accepts any
/// `<n>k` value in 32–512; this list is the curated set exposed via metadata.
pub const AUDIO_BITRATE_CANDIDATES: &[&str] = &[
    "64k", "96k", "128k", "160k", "192k", "256k", "320k", "384k",
];

/// Maps a quality preset name to its representative CRF.
///
/// Returns `None` for unknown names (callers fall back to the preset's own
/// baseline CRF rather than inventing a value).
pub fn crf_for_quality_preset(quality_preset: &str) -> Option<u8> {
    let needle = quality_preset.trim().to_ascii_lowercase();
    QUALITY_LEVELS
        .iter()
        .find(|(name, _)| *name == needle)
        .map(|(_, crf)| *crf)
}

/// Derives the quality band containing a CRF value from [`QUALITY_LEVELS`].
///
/// Boundary between consecutive levels is the integer midpoint of their
/// representatives, so each representative maps back to its own level and
/// every value in 0–51 resolves deterministically. Values above 51 (outside
/// the slider range) resolve to the lowest-quality band.
pub fn quality_for_crf(crf: u8) -> &'static str {
    let crf_u16 = crf as u16;
    for i in 0..QUALITY_LEVELS.len() {
        let is_last = i + 1 == QUALITY_LEVELS.len();
        if is_last {
            return QUALITY_LEVELS[i].0;
        }
        let upper_mid = (QUALITY_LEVELS[i].1 as u16 + QUALITY_LEVELS[i + 1].1 as u16) / 2;
        if crf_u16 <= upper_mid {
            return QUALITY_LEVELS[i].0;
        }
    }
    // Unreachable: the loop always returns on the last level.
    QUALITY_LEVELS[QUALITY_LEVELS.len() - 1].0
}

/// Which quality control currently governs the effective CRF.
///
/// - `Baseline`: no session quality override; the built-in preset's own CRF
///   (or its quality's mapping at selection time) applies.
/// - `QualityPreset`: the user explicitly changed Quality Preset last; the
///   effective CRF is the mapping for the selected quality name.
/// - `ManualCrf`: the user explicitly moved the CRF slider last; the
///   effective CRF is the stored manual value regardless of the displayed
///   quality name.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Type)]
#[serde(rename_all = "camelCase")]
pub enum QualityAuthority {
    /// No explicit quality interaction this session.
    #[default]
    Baseline,
    /// Quality Preset dropdown was most recently interacted with.
    QualityPreset,
    /// CRF slider was most recently interacted with.
    ManualCrf,
}

/// Transient session-level encoding overrides (user intent).
///
/// Not persisted; not part of saved presets. Crosses the IPC boundary inside
/// `OutputJob.encoding_overrides` so Rust can resolve the effective encoding
/// itself. The frontend never sends a pre-resolved profile as authority.
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct EncodingOverrides {
    pub quality_preset: Option<String>,
    pub crf: Option<u8>,
    pub speed_preset: Option<String>,
    pub audio_bitrate: Option<String>,
    pub quality_authority: QualityAuthority,
}

impl EncodingOverrides {
    /// No overrides — pure preset baseline.
    pub fn baseline() -> Self {
        Self::default()
    }
}

/// Rejects an unknown quality name.
pub fn validate_quality_name(name: &str) -> Result<(), VideoError> {
    let needle = name.trim().to_ascii_lowercase();
    if QUALITY_LEVELS.iter().any(|(known, _)| *known == needle) {
        Ok(())
    } else {
        Err(VideoError::InvalidInput(format!(
            "Unsupported qualityPreset: {name}"
        )))
    }
}

/// Rejects an unknown speed preset name.
pub fn validate_speed_name(name: &str) -> Result<(), VideoError> {
    let needle = name.trim().to_ascii_lowercase();
    if SPEED_PRESETS.contains(&needle.as_str()) {
        Ok(())
    } else {
        Err(VideoError::InvalidInput(format!(
            "Unsupported speedPreset: {name}"
        )))
    }
}

/// Rejects a CRF outside the 0–51 slider range.
pub fn validate_crf_value(crf: u8) -> Result<(), VideoError> {
    if crf > 51 {
        return Err(VideoError::InvalidInput(
            "encoding.crf must be between 0 and 51".to_string(),
        ));
    }
    Ok(())
}

/// Rejects a malformed audio bitrate (`<n>k`, 32–512).
pub fn validate_audio_bitrate_value(bitrate: &str) -> Result<(), VideoError> {
    let raw = bitrate.trim().to_ascii_lowercase();
    let numeric = raw.strip_suffix('k').ok_or_else(|| {
        VideoError::InvalidInput(
            "encoding.audioBitrate must use 'k' suffix, e.g. 128k".to_string(),
        )
    })?;
    let parsed = numeric.parse::<u32>().map_err(|_| {
        VideoError::InvalidInput(
            "encoding.audioBitrate must be numeric, e.g. 128k".to_string(),
        )
    })?;
    if !(32..=512).contains(&parsed) {
        return Err(VideoError::InvalidInput(
            "encoding.audioBitrate must be between 32k and 512k".to_string(),
        ));
    }
    Ok(())
}

/// Validates a full baseline profile's value domains.
///
/// Preset-specific baselines intentionally keep their own tuned CRF even when
/// it differs from the global representative for the stored quality name, so
/// this checks membership/bounds only — never CRF↔name consistency.
pub fn validate_baseline_profile(encoding: &EncodingProfile) -> Result<(), VideoError> {
    validate_crf_value(encoding.crf)?;
    validate_quality_name(&encoding.quality_preset)?;
    validate_speed_name(&encoding.speed_preset)?;
    validate_audio_bitrate_value(&encoding.audio_bitrate)?;
    Ok(())
}

/// Validates override values and authority combinations.
///
/// Invalid combinations fail here so [`resolve_effective_encoding`] can never
/// produce a valid-looking profile from invalid intent.
pub fn validate_encoding_overrides(overrides: &EncodingOverrides) -> Result<(), VideoError> {
    if let Some(q) = &overrides.quality_preset {
        if !q.trim().is_empty() {
            validate_quality_name(q)?;
        } else if overrides.quality_authority != QualityAuthority::Baseline {
            return Err(VideoError::InvalidInput(
                "encoding override qualityPreset cannot be empty".to_string(),
            ));
        }
    }
    if let Some(crf) = overrides.crf {
        validate_crf_value(crf)?;
    }
    if let Some(speed) = &overrides.speed_preset {
        validate_speed_name(speed)?;
    }
    if let Some(bitrate) = &overrides.audio_bitrate {
        validate_audio_bitrate_value(bitrate)?;
    }
    match overrides.quality_authority {
        QualityAuthority::QualityPreset => {
            if overrides
                .quality_preset
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(VideoError::InvalidInput(
                    "qualityAuthority is qualityPreset but no qualityPreset override was provided"
                        .to_string(),
                ));
            }
        }
        QualityAuthority::ManualCrf => {
            if overrides.crf.is_none() {
                return Err(VideoError::InvalidInput(
                    "qualityAuthority is manualCrf but no crf override was provided".to_string(),
                ));
            }
        }
        QualityAuthority::Baseline => {
            let has_quality_override = overrides
                .quality_preset
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if overrides.crf.is_some() || has_quality_override {
                return Err(VideoError::InvalidInput(
                    "quality overrides require qualityAuthority qualityPreset or manualCrf"
                        .to_string(),
                ));
            }
        }
    }
    Ok(())
}

/// Resolves the final render-time [`EncodingProfile`] from a preset baseline
/// plus session overrides, failing on invalid input instead of silently
/// falling through.
///
/// This is the single authoritative resolution. All render-boundary paths
/// (platform, custom, aspect-ratio, single, batch, preview) must use this —
/// never a bare clone of the baseline.
///
/// Quality dimension follows the authority flag; speed/audio dimensions are
/// independent last-value-wins overrides. Unknown quality names fail (the
/// caller falls back only by not sending the override, never by inventing a
/// value). A `ManualCrf` override without an explicit display name derives
/// the band label from the CRF so the stored profile stays consistent with
/// what the UI shows, even though the builder only consumes `crf`.
pub fn resolve_effective_encoding(
    baseline: &EncodingProfile,
    overrides: &EncodingOverrides,
) -> Result<EncodingProfile, VideoError> {
    validate_baseline_profile(baseline)?;
    validate_encoding_overrides(overrides)?;

    let mut effective = baseline.clone();

    // Independent dimensions.
    if let Some(speed) = &overrides.speed_preset {
        effective.speed_preset = speed.clone();
    }
    if let Some(bitrate) = &overrides.audio_bitrate {
        effective.audio_bitrate = bitrate.clone();
    }

    // Quality dimension — exactly one authority.
    match overrides.quality_authority {
        QualityAuthority::ManualCrf => {
            // Validated present above.
            let crf = overrides.crf.unwrap_or(effective.crf);
            effective.crf = crf;
            if let Some(q) = &overrides.quality_preset {
                if !q.trim().is_empty() {
                    effective.quality_preset = q.clone();
                } else {
                    effective.quality_preset = quality_for_crf(crf).to_string();
                }
            } else {
                effective.quality_preset = quality_for_crf(crf).to_string();
            }
        }
        QualityAuthority::QualityPreset => {
            // Validated present above.
            if let Some(q) = &overrides.quality_preset {
                effective.quality_preset = q.clone();
                // The mapping is authoritative; a stale manual CRF (if any)
                // must NOT leak through — quality change discards it.
                match crf_for_quality_preset(q) {
                    Some(mapped) => effective.crf = mapped,
                    None => {
                        return Err(VideoError::InvalidInput(format!(
                            "Unsupported qualityPreset: {q}"
                        )))
                    }
                }
            }
        }
        QualityAuthority::Baseline => {
            // No quality override: keep the preset's own values untouched,
            // including intentionally preset-specific CRFs that differ from
            // the global representative for the stored quality name.
        }
    }

    validate_baseline_profile(&effective)?;
    Ok(effective)
}

/// Whether the session overrides contain explicit encoding intent that
/// requires re-encoding and therefore prohibits the `-c copy` passthrough.
///
/// Covers at minimum (§14): Quality Preset override, manual CRF override,
/// speed preset override, audio bitrate override. Derived from override
/// *presence + authority*, never by comparing values against magic numbers
/// like `crf != 18` (which would confuse baseline config with user intent).
pub fn has_explicit_encoding_intent(overrides: &EncodingOverrides) -> bool {
    match overrides.quality_authority {
        QualityAuthority::QualityPreset | QualityAuthority::ManualCrf => return true,
        QualityAuthority::Baseline => {}
    }
    overrides.speed_preset.is_some() || overrides.audio_bitrate.is_some()
}

/// Pure passthrough-eligibility predicate (Route B, §13–§15).
///
/// Returns `true` only when the output can genuinely be produced by stream
/// copy *and* the user has not requested encoding changes (`force_reencode`
/// carries [`has_explicit_encoding_intent`] across IPC).
#[allow(clippy::too_many_arguments)]
pub fn is_passthrough_allowed(
    is_vertical: bool,
    ratio_diff: f32,
    background_effect_enabled: bool,
    remove_audio_enabled: bool,
    burn_subtitles_enabled: bool,
    text_overlay_enabled: bool,
    has_logo: bool,
    has_transform: bool,
    force_reencode: bool,
) -> bool {
    if force_reencode {
        return false;
    }
    is_vertical
        && ratio_diff < 0.02
        && !background_effect_enabled
        && !remove_audio_enabled
        && !burn_subtitles_enabled
        && !text_overlay_enabled
        && !has_logo
        && !has_transform
}

/// One quality level with its representative CRF and the CRF band it owns.
///
/// Bands are derived from [`QUALITY_LEVELS`] with midpoint boundaries
/// `floor((rep_i + rep_{i+1}) / 2)` — the same rule as [`quality_for_crf`] —
/// so metadata can never disagree with resolution.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct QualityLevelMeta {
    pub name: String,
    pub representative_crf: u8,
    pub min_crf: u8,
    pub max_crf: u8,
}

/// Per-container encoder capabilities backing the FFmpeg builder's
/// conditional flags (`supports_crf`/`supports_preset`).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct CodecCapability {
    pub output_format: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub supports_crf: bool,
    pub supports_preset: bool,
}

/// Canonical encoding semantics for UI consumers.
///
/// The frontend must render controls from this instead of maintaining its own
/// copies of quality levels, speed lists, or bitrate lists.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct EncodingMetadata {
    pub quality_levels: Vec<QualityLevelMeta>,
    pub speed_presets: Vec<String>,
    pub audio_bitrate_options: Vec<String>,
    pub codec_capabilities: Vec<CodecCapability>,
    pub default_encoding: EncodingProfile,
}

/// Builds the canonical metadata from the single sources of truth above.
pub fn build_encoding_metadata() -> EncodingMetadata {
    let mut quality_levels = Vec::with_capacity(QUALITY_LEVELS.len());
    for (i, (name, rep)) in QUALITY_LEVELS.iter().enumerate() {
        let min_crf = if i == 0 {
            0
        } else {
            ((QUALITY_LEVELS[i - 1].1 as u16 + *rep as u16) / 2 + 1) as u8
        };
        let max_crf = if i + 1 == QUALITY_LEVELS.len() {
            51
        } else {
            ((*rep as u16 + QUALITY_LEVELS[i + 1].1 as u16) / 2) as u8
        };
        quality_levels.push(QualityLevelMeta {
            name: name.to_string(),
            representative_crf: *rep,
            min_crf,
            max_crf,
        });
    }
    EncodingMetadata {
        quality_levels,
        speed_presets: SPEED_PRESETS.iter().map(|s| s.to_string()).collect(),
        audio_bitrate_options: AUDIO_BITRATE_CANDIDATES
            .iter()
            .map(|s| s.to_string())
            .collect(),
        codec_capabilities: vec![
            CodecCapability {
                output_format: OutputFormat::Mp4.get_extension().to_string(),
                video_codec: "libx264".to_string(),
                audio_codec: "aac".to_string(),
                supports_crf: true,
                supports_preset: true,
            },
            CodecCapability {
                output_format: OutputFormat::Mov.get_extension().to_string(),
                video_codec: "libx264".to_string(),
                audio_codec: "aac".to_string(),
                supports_crf: true,
                supports_preset: true,
            },
            CodecCapability {
                output_format: OutputFormat::Webm.get_extension().to_string(),
                video_codec: "libvpx-vp9".to_string(),
                audio_codec: "libopus".to_string(),
                supports_crf: true,
                supports_preset: false,
            },
        ],
        default_encoding: EncodingProfile::standard(),
    }
}

/// Preview request: user intent plus the render settings that affect control
/// applicability (output format, audio removal).
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct EncodingPreviewRequest {
    pub baseline: EncodingProfile,
    pub overrides: EncodingOverrides,
    #[serde(default)]
    pub output_format: Option<String>,
    #[serde(default)]
    pub remove_audio: Option<bool>,
}

/// Preview response: effective values plus non-fatal applicability warnings.
///
/// Warnings never fail the render; they explain why a control has no effect.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct EncodingPreviewResponse {
    pub effective: EncodingProfile,
    pub warnings: Vec<String>,
}

/// Resolves a display preview with the exact production resolution rules.
pub fn preview_effective_encoding(
    request: &EncodingPreviewRequest,
) -> Result<EncodingPreviewResponse, VideoError> {
    let effective = resolve_effective_encoding(&request.baseline, &request.overrides)?;
    let mut warnings = Vec::new();
    let format = request
        .output_format
        .as_deref()
        .unwrap_or("mp4")
        .trim()
        .to_ascii_lowercase();
    if format == "webm" {
        warnings.push(
            "Speed preset is not applicable to VP9/WebM and will not affect encoding."
                .to_string(),
        );
    }
    if request.remove_audio.unwrap_or(false) {
        warnings.push(
            "Audio bitrate has no effect while Remove Audio is enabled.".to_string(),
        );
    }
    Ok(EncodingPreviewResponse {
        effective,
        warnings,
    })
}

/// Tauri command: canonical encoding metadata for UI consumers.
#[tauri::command]
pub fn get_encoding_metadata() -> EncodingMetadata {
    build_encoding_metadata()
}

/// Tauri command: preview resolution using production rules.
///
/// The frontend displays `effective` without implementing resolution itself.
#[tauri::command]
pub fn resolve_encoding_preview(
    request: EncodingPreviewRequest,
) -> Result<EncodingPreviewResponse, String> {
    preview_effective_encoding(&request).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::ffmpeg_args_builder::build_ffmpeg_args;
    use crate::video::preset_adapter::RenderPlan;
    use crate::video::types::{
        AspectRatio, SubtitleOverlaySettings, TextOverlaySettings, VideoEffectsSettings,
    };

    fn baseline_high() -> EncodingProfile {
        EncodingProfile {
            crf: 18,
            quality_preset: "high".to_string(),
            speed_preset: "slow".to_string(),
            audio_bitrate: "192k".to_string(),
        }
    }

    fn plain_effects() -> VideoEffectsSettings {
        VideoEffectsSettings {
            blur: None,
            white_background: None,
            overlays: None,
            subtitles: None,
            color_filter: None,
            blur_sigma: None,
            remove_audio: None,
            export_subtitles: None,
            burn_subtitles: None,
            skip_existing: None,
            output_format: None,
            logo: None,
            text_overlay: TextOverlaySettings::default(),
            subtitle_overlay: SubtitleOverlaySettings::default(),
            transform: None,
        }
    }

    fn plan_with(encoding: EncodingProfile, effects: VideoEffectsSettings) -> RenderPlan {
        RenderPlan {
            ratio: AspectRatio::Ratio9x16,
            encoding,
            effects,
            platform_config: None,
            logo: None,
        }
    }

    fn must_resolve(
        baseline: &EncodingProfile,
        overrides: &EncodingOverrides,
    ) -> EncodingProfile {
        resolve_effective_encoding(baseline, overrides).expect("test encoding must resolve")
    }

    // --- §22 Baseline ---

    #[test]
    fn baseline_without_overrides_uses_preset_values() {
        let effective = must_resolve(&baseline_high(), &EncodingOverrides::baseline());
        assert_eq!(effective.crf, 18);
        assert_eq!(effective.quality_preset, "high");
        assert_eq!(effective.speed_preset, "slow");
        assert!(!has_explicit_encoding_intent(&EncodingOverrides::baseline()));
    }

    // --- §22 Quality override ---

    #[test]
    fn quality_override_maps_to_mapped_crf_with_quality_authority() {
        let overrides = EncodingOverrides {
            quality_preset: Some("balanced".to_string()),
            quality_authority: QualityAuthority::QualityPreset,
            ..Default::default()
        };
        let effective = must_resolve(&baseline_high(), &overrides);
        assert_eq!(effective.crf, 28);
        assert_eq!(effective.quality_preset, "balanced");
        assert!(has_explicit_encoding_intent(&overrides));
    }

    // --- §22 Manual CRF override ---

    #[test]
    fn manual_crf_override_wins_over_quality_display() {
        let overrides = EncodingOverrides {
            quality_preset: Some("high".to_string()),
            crf: Some(51),
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        let effective = must_resolve(&baseline_high(), &overrides);
        assert_eq!(effective.crf, 51);
        assert!(has_explicit_encoding_intent(&overrides));
    }

    // --- Authority switching: High → Draft → manual 48 → High ---

    #[test]
    fn most_recent_quality_control_wins_and_stale_manual_is_discarded() {
        // Step 1: user selects draft (preview) quality.
        let step1 = EncodingOverrides {
            quality_preset: Some("draft".to_string()),
            quality_authority: QualityAuthority::QualityPreset,
            ..Default::default()
        };
        let e1 = must_resolve(&baseline_high(), &step1);
        assert_eq!((e1.crf, e1.quality_preset.as_str()), (41, "draft"));

        // Step 2: user moves CRF slider to 48 (derived band: poor).
        let step2 = EncodingOverrides {
            quality_preset: Some("poor".to_string()),
            crf: Some(48),
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        let e2 = must_resolve(&baseline_high(), &step2);
        assert_eq!((e2.crf, e2.quality_preset.as_str()), (48, "poor"));

        // Step 3: user changes Quality back to high — stale 51 discarded.
        // Frontend clears `crf` on quality change; resolution must not leak it
        // even if a stale value were still present.
        let step3_stale = EncodingOverrides {
            quality_preset: Some("high".to_string()),
            crf: Some(51),
            quality_authority: QualityAuthority::QualityPreset,
            ..Default::default()
        };
        let e3 = must_resolve(&baseline_high(), &step3_stale);
        assert_eq!((e3.crf, e3.quality_preset.as_str()), (18, "high"));
    }

    // --- §22 Ratio target uses same resolution ---

    #[test]
    fn ratio_baseline_plus_manual_override_produces_manual_crf() {
        // Aspect-ratio baseline (mirrors aspect_ratio_presets.json 9:16).
        let ratio_baseline = EncodingProfile {
            crf: 18,
            quality_preset: "high".to_string(),
            speed_preset: "slow".to_string(),
            audio_bitrate: "192k".to_string(),
        };
        let overrides = EncodingOverrides {
            crf: Some(51),
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        let effective = must_resolve(&ratio_baseline, &overrides);
        assert_eq!(effective.crf, 51);
    }

    // --- Mapping ---

    #[test]
    fn every_quality_preset_has_a_representative_crf_in_order() {
        let expected: &[(&str, u8)] = &[
            ("lossless", 0),
            ("very_high", 14),
            ("high", 18),
            ("good", 21),
            ("standard", 23),
            ("balanced", 28),
            ("low", 33),
            ("very_low", 38),
            ("draft", 41),
            ("poor", 48),
        ];
        assert_eq!(QUALITY_LEVELS, expected);
        // Strictly increasing: each representative must map back to itself.
        for window in QUALITY_LEVELS.windows(2) {
            assert!(window[0].1 < window[1].1, "levels must be ordered");
        }
        for (name, crf) in QUALITY_LEVELS {
            assert_eq!(crf_for_quality_preset(name), Some(*crf));
            assert_eq!(quality_for_crf(*crf), *name, "rep must round-trip");
        }
    }

    #[test]
    fn quality_mapping_is_case_insensitive_and_rejects_unknown() {
        assert_eq!(crf_for_quality_preset("high"), Some(18));
        assert_eq!(crf_for_quality_preset("High"), Some(18));
        assert_eq!(crf_for_quality_preset("STANDARD"), Some(23));
        assert_eq!(crf_for_quality_preset("Very_High"), Some(14));
        assert_eq!(crf_for_quality_preset("draft"), Some(41));
        assert_eq!(crf_for_quality_preset("Poor"), Some(48));
        assert_eq!(crf_for_quality_preset("ultra"), None);
    }

    #[test]
    fn draft_is_not_the_poorest_level_poor_is_lowest() {
        // Semantic ordering: Draft (preview) sits above Poor (lowest).
        let draft = crf_for_quality_preset("draft").expect("draft");
        let poor = crf_for_quality_preset("poor").expect("poor");
        assert!(draft < poor, "draft ({draft}) must be higher quality than poor ({poor})");
        // Poor owns the slider floor; Draft does not.
        assert_eq!(quality_for_crf(51), "poor");
        assert_eq!(quality_for_crf(48), "poor");
        assert_eq!(quality_for_crf(41), "draft");
        // Boundary between the two bands is deterministic.
        assert_eq!(quality_for_crf(44), "draft");
        assert_eq!(quality_for_crf(45), "poor");
        // Full ordering check, best → worst.
        let ordered: Vec<&str> = QUALITY_LEVELS.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            ordered,
            vec![
                "lossless",
                "very_high",
                "high",
                "good",
                "standard",
                "balanced",
                "low",
                "very_low",
                "draft",
                "poor"
            ]
        );
    }

    #[test]
    fn crf_to_quality_covers_the_full_slider_range_deterministically() {
        // Documented bands; boundaries are midpoints of consecutive reps.
        let bands: &[(&str, u8, u8)] = &[
            ("lossless", 0, 7),
            ("very_high", 8, 16),
            ("high", 17, 19),
            ("good", 20, 22),
            ("standard", 23, 25),
            ("balanced", 26, 30),
            ("low", 31, 35),
            ("very_low", 36, 39),
            ("draft", 40, 44),
            ("poor", 45, 51),
        ];
        for (name, lo, hi) in bands {
            for crf in *lo..=*hi {
                assert_eq!(quality_for_crf(crf), *name, "crf {crf}");
            }
        }
        // Spot-checks: the floor belongs to Poor, Draft keeps its preview
        // band, and long-standing anchors are unchanged.
        assert_eq!(quality_for_crf(51), "poor");
        assert_eq!(quality_for_crf(41), "draft");
        assert_eq!(quality_for_crf(18), "high");
        assert_eq!(quality_for_crf(0), "lossless");
    }

    #[test]
    fn intermediate_crf_keeps_exact_value_with_derived_band() {
        // High=18, Good=21 (boundary at 19): manual 20 stays exactly 20
        // while the derived band reports Good ("in the Good range, manually
        // refined"); manual 19 stays 19 with band High.
        assert_eq!(quality_for_crf(20), "good");
        assert_eq!(quality_for_crf(19), "high");
        let overrides = EncodingOverrides {
            quality_preset: Some(quality_for_crf(20).to_string()),
            crf: Some(20),
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        let effective = must_resolve(&baseline_high(), &overrides);
        assert_eq!(effective.crf, 20);
        assert_eq!(effective.quality_preset, "good");
        assert!(has_explicit_encoding_intent(&overrides));
    }

    #[test]
    fn derived_band_update_does_not_change_authority_or_intent() {
        // Manual CRF 48 with the *derived* Poor label is still a manual
        // override: authority stays ManualCrf and intent stays true. Only an
        // explicit dropdown selection may switch authority to QualityPreset.
        let derived = EncodingOverrides {
            quality_preset: Some(quality_for_crf(48).to_string()),
            crf: Some(48),
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        assert_eq!(derived.quality_authority, QualityAuthority::ManualCrf);
        assert!(has_explicit_encoding_intent(&derived));
        let effective = must_resolve(&baseline_high(), &derived);
        assert_eq!((effective.crf, effective.quality_preset.as_str()), (48, "poor"));

        // Explicitly selecting that same Poor label flips authority and
        // applies the representative CRF (48 here — same value, new owner).
        let explicit = EncodingOverrides {
            quality_preset: Some("poor".to_string()),
            quality_authority: QualityAuthority::QualityPreset,
            ..Default::default()
        };
        assert_eq!(explicit.quality_authority, QualityAuthority::QualityPreset);
        let e2 = must_resolve(&baseline_high(), &explicit);
        assert_eq!(e2.crf, 48);
    }

    #[test]
    fn switching_sequences_end_with_most_recent_explicit_control() {
        // Quality → CRF → Quality ends with QualityPreset authority.
        let q1 = EncodingOverrides {
            quality_preset: Some("good".to_string()),
            quality_authority: QualityAuthority::QualityPreset,
            ..Default::default()
        };
        assert_eq!(must_resolve(&baseline_high(), &q1).crf, 21);
        let c = EncodingOverrides {
            quality_preset: Some(quality_for_crf(27).to_string()),
            crf: Some(27),
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        let ec = must_resolve(&baseline_high(), &c);
        assert_eq!((ec.crf, ec.quality_preset.as_str()), (27, "balanced"));
        let q2 = EncodingOverrides {
            quality_preset: Some("high".to_string()),
            quality_authority: QualityAuthority::QualityPreset,
            ..Default::default()
        };
        let eq2 = must_resolve(&baseline_high(), &q2);
        assert_eq!((eq2.crf, eq2.quality_preset.as_str()), (18, "high"));

        // CRF → Quality → CRF ends with ManualCrf authority.
        let c2 = EncodingOverrides {
            quality_preset: Some(quality_for_crf(33).to_string()),
            crf: Some(33),
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        let ec2 = must_resolve(&baseline_high(), &c2);
        assert_eq!((ec2.crf, ec2.quality_preset.as_str()), (33, "low"));
    }

    // --- §22 Passthrough ---

    #[test]
    fn passthrough_allowed_when_compatible_and_no_intent() {
        assert!(is_passthrough_allowed(
            true, 0.0, false, false, false, false, false, false, false
        ));
    }

    #[test]
    fn each_explicit_override_blocks_passthrough() {
        // Quality intent.
        let q = EncodingOverrides {
            quality_preset: Some("draft".to_string()),
            quality_authority: QualityAuthority::QualityPreset,
            ..Default::default()
        };
        assert!(has_explicit_encoding_intent(&q));
        assert!(!is_passthrough_allowed(
            true, 0.0, false, false, false, false, false, false, true
        ));
        // Manual CRF intent.
        let c = EncodingOverrides {
            crf: Some(51),
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        assert!(has_explicit_encoding_intent(&c));
        // Speed intent.
        let s = EncodingOverrides {
            speed_preset: Some("ultrafast".to_string()),
            ..Default::default()
        };
        assert!(has_explicit_encoding_intent(&s));
        // Audio intent.
        let a = EncodingOverrides {
            audio_bitrate: Some("64k".to_string()),
            ..Default::default()
        };
        assert!(has_explicit_encoding_intent(&a));
    }

    #[test]
    fn passthrough_still_blocked_by_geometry_and_effects() {
        // Horizontal input can never passthrough even without intent.
        assert!(!is_passthrough_allowed(
            false, 0.0, false, false, false, false, false, false, false
        ));
        // Ratio mismatch.
        assert!(!is_passthrough_allowed(
            true, 0.5, false, false, false, false, false, false, false
        ));
        // Effects.
        assert!(!is_passthrough_allowed(
            true, 0.0, true, false, false, false, false, false, false
        ));
        assert!(!is_passthrough_allowed(
            true, 0.0, false, true, false, false, false, false, false
        ));
    }

    // --- §22 FFmpeg args reflect resolved config ---

    #[test]
    fn ffmpeg_args_contain_resolved_crf_preset_and_audio_bitrate() {
        let resolved = EncodingProfile {
            crf: 51,
            quality_preset: "high".to_string(),
            speed_preset: "ultrafast".to_string(),
            audio_bitrate: "64k".to_string(),
        };
        let plan = plan_with(resolved, plain_effects());
        let args = build_ffmpeg_args("in.mp4", "out.mp4", "null", &plan, None, None, None, None, None);
        let pos = |flag: &str| args.iter().position(|a| a == flag).expect(flag);
        assert_eq!(args[pos("-crf") + 1], "51");
        assert_eq!(args[pos("-preset") + 1], "ultrafast");
        assert_eq!(args[pos("-b:a") + 1], "64k");
    }

    // --- §22 Audio removal ---

    #[test]
    fn remove_audio_produces_an_without_audio_bitrate() {
        let mut effects = plain_effects();
        effects.remove_audio = Some(true);
        let plan = plan_with(baseline_high(), effects);
        let args = build_ffmpeg_args("in.mp4", "out.mp4", "null", &plan, None, None, None, None, None);
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"-b:a".to_string()));
        assert!(!args.contains(&"-c:a".to_string()));
    }

    // --- Issue #7: invalid input fails before any ResolvedJob can exist ---

    #[test]
    fn unknown_quality_override_fails_resolution() {
        let overrides = EncodingOverrides {
            quality_preset: Some("ultra".to_string()),
            quality_authority: QualityAuthority::QualityPreset,
            ..Default::default()
        };
        assert!(resolve_effective_encoding(&baseline_high(), &overrides).is_err());
    }

    #[test]
    fn unknown_speed_override_fails_resolution() {
        let overrides = EncodingOverrides {
            speed_preset: Some("ludicrous".to_string()),
            ..Default::default()
        };
        assert!(resolve_effective_encoding(&baseline_high(), &overrides).is_err());
    }

    #[test]
    fn malformed_audio_override_fails_resolution() {
        for bad in ["loud", "64", "8k", "1024k"] {
            let overrides = EncodingOverrides {
                audio_bitrate: Some(bad.to_string()),
                ..Default::default()
            };
            assert!(
                resolve_effective_encoding(&baseline_high(), &overrides).is_err(),
                "bitrate {bad}"
            );
        }
    }

    #[test]
    fn mismatched_authority_combinations_fail_resolution() {
        // QualityPreset authority without a quality name.
        let no_quality = EncodingOverrides {
            quality_authority: QualityAuthority::QualityPreset,
            ..Default::default()
        };
        assert!(resolve_effective_encoding(&baseline_high(), &no_quality).is_err());
        // ManualCrf authority without a CRF value.
        let no_crf = EncodingOverrides {
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        assert!(resolve_effective_encoding(&baseline_high(), &no_crf).is_err());
        // Baseline authority carrying quality intent.
        let stray_quality = EncodingOverrides {
            quality_preset: Some("high".to_string()),
            ..Default::default()
        };
        assert!(resolve_effective_encoding(&baseline_high(), &stray_quality).is_err());
    }

    #[test]
    fn invalid_baseline_fails_resolution() {
        let mut bad = baseline_high();
        bad.crf = 99;
        assert!(resolve_effective_encoding(&bad, &EncodingOverrides::baseline()).is_err());
        let mut unknown = baseline_high();
        unknown.quality_preset = "ultra".to_string();
        assert!(
            resolve_effective_encoding(&unknown, &EncodingOverrides::baseline()).is_err()
        );
    }

    #[test]
    fn manual_crf_without_display_name_derives_band() {
        let overrides = EncodingOverrides {
            crf: Some(51),
            quality_authority: QualityAuthority::ManualCrf,
            ..Default::default()
        };
        let effective = must_resolve(&baseline_high(), &overrides);
        assert_eq!((effective.crf, effective.quality_preset.as_str()), (51, "poor"));
    }

    // --- Issue #2: metadata agrees with resolution ---

    #[test]
    fn metadata_bands_cover_full_slider_and_match_derivation() {
        let meta = build_encoding_metadata();
        assert_eq!(meta.quality_levels.len(), QUALITY_LEVELS.len());
        assert_eq!(
            meta.speed_presets,
            SPEED_PRESETS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
        assert_eq!(
            meta.audio_bitrate_options,
            AUDIO_BITRATE_CANDIDATES
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(meta.default_encoding, EncodingProfile::standard());
        // Bands tile 0–51 without gaps and each representative resolves back.
        let mut cursor = 0u8;
        for level in &meta.quality_levels {
            assert_eq!(level.min_crf, cursor, "gap before {}", level.name);
            assert!(level.min_crf <= level.representative_crf);
            assert!(level.representative_crf <= level.max_crf);
            assert_eq!(quality_for_crf(level.representative_crf), level.name);
            for crf in level.min_crf..=level.max_crf {
                assert_eq!(quality_for_crf(crf), level.name, "crf {crf}");
            }
            cursor = level.max_crf.saturating_add(1);
        }
        assert_eq!(cursor, 52, "bands must end exactly at 51");
        // Codec capabilities mirror the builder's conditionals.
        let webm = meta
            .codec_capabilities
            .iter()
            .find(|c| c.output_format == "webm")
            .expect("webm capability");
        assert!(!webm.supports_preset);
        assert!(webm.supports_crf);
        let mp4 = meta
            .codec_capabilities
            .iter()
            .find(|c| c.output_format == "mp4")
            .expect("mp4 capability");
        assert!(mp4.supports_preset);
    }

    // --- Issues #5/#6: preview warnings ---

    #[test]
    fn preview_uses_production_resolution_and_warns_on_inapplicable_controls() {
        let req = EncodingPreviewRequest {
            baseline: baseline_high(),
            overrides: EncodingOverrides {
                quality_preset: Some("balanced".to_string()),
                quality_authority: QualityAuthority::QualityPreset,
                ..Default::default()
            },
            output_format: Some("mp4".to_string()),
            remove_audio: Some(false),
        };
        let res = preview_effective_encoding(&req).expect("preview must resolve");
        assert_eq!((res.effective.crf, res.effective.quality_preset.as_str()), (28, "balanced"));
        assert!(res.warnings.is_empty());

        let webm = EncodingPreviewRequest {
            output_format: Some("webm".to_string()),
            ..req.clone()
        };
        let res = preview_effective_encoding(&webm).expect("preview must resolve");
        assert_eq!(res.effective.crf, 28);
        assert!(res.warnings.iter().any(|w| w.contains("VP9/WebM")));

        let muted = EncodingPreviewRequest {
            remove_audio: Some(true),
            ..req.clone()
        };
        let res = preview_effective_encoding(&muted).expect("preview must resolve");
        assert!(res.warnings.iter().any(|w| w.contains("Remove Audio")));

        let invalid = EncodingPreviewRequest {
            overrides: EncodingOverrides {
                quality_preset: Some("ultra".to_string()),
                quality_authority: QualityAuthority::QualityPreset,
                ..Default::default()
            },
            ..req.clone()
        };
        assert!(preview_effective_encoding(&invalid).is_err());
    }
}
