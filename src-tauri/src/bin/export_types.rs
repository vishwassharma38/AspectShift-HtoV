use aspectshift_htov_lib::auth::{
    auth_events::{AuthActivationFailedPayload, AuthStatusChangedPayload},
    auth_models::ActivationResult,
    state::auth_state::{AuthState, AuthStatus},
    state::license_tier::LicenseTier,
};
use aspectshift_htov_lib::dependency_manager::{
    AppDepsState, DependencyId, DependencyReport, DependencyScanStatus, DependencyStatus,
};
use aspectshift_htov_lib::subtitles::positioning::SubtitleLayoutMetrics;
use aspectshift_htov_lib::video::{
    encoding::{
        CodecCapability, EncodingMetadata, EncodingOverrides, EncodingPreviewRequest,
        EncodingPreviewResponse, QualityAuthority, QualityLevelMeta,
    },
    ffmpeg::VideoProgress,
    render_layout::{PreviewFitMode, PreviewRenderLayout},
    types::{
        AppConfig, AspectRatio, AspectRatioTarget, BatchJobSettings, BatchProgress,
        ConversionRequestDTO, CustomPreset, EncodingProfile, FileProgress, FileReadiness,
        ImageCrop, ImageOverlay, ImageOverlaySettings, JobStatus, OrientationInfo,
        OutputFormat, OutputJob, PlatformConfig, PlatformPreset, PreviewLayoutRequest,
        StructuredError, SubtitleOverlaySettings, TextFontStyle, TextLayerSettings,
        TextOverlaySettings, VideoEffectsSettings, VideoPresetDTO, VideoTransform,
    },
};
use specta::TypeCollection;
use specta_typescript::{BigIntExportBehavior, Typescript};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Contract caveats (intentional, do not "fix"):
    // - Rust `u8/u32/usize/u64/i32/f32/f64` all export as TS `number`
    //   (`BigIntExportBehavior::Number` below). Precision loss above 2^53
    //   is irrelevant for the counts/sizes crossing this boundary.
    // - `Option<T>` with `#[serde(default)]` exports as optional (`?`);
    //   missing and `null` both deserialize to `None`.
    // - `VideoProgress.ratio` (FFmpeg event payload) is a free `String`
    //   label, while `FileProgress.ratio` is the typed `AspectRatio` enum;
    //   the shared name is historical, not a type error.
    let types = TypeCollection::default()
        .register::<AspectRatio>()
        .register::<EncodingProfile>()
        .register::<QualityAuthority>()
        .register::<EncodingOverrides>()
        .register::<QualityLevelMeta>()
        .register::<CodecCapability>()
        .register::<EncodingMetadata>()
        .register::<EncodingPreviewRequest>()
        .register::<EncodingPreviewResponse>()
        .register::<OutputFormat>()
        .register::<PlatformConfig>()
        .register::<VideoTransform>()
        .register::<ImageCrop>()
        .register::<ImageOverlay>()
        .register::<ImageOverlaySettings>()
        .register::<TextFontStyle>()
        .register::<TextLayerSettings>()
        .register::<TextOverlaySettings>()
        .register::<SubtitleOverlaySettings>()
        .register::<VideoEffectsSettings>()
        .register::<AppConfig>()
        .register::<OutputJob>()
        .register::<AspectRatioTarget>()
        .register::<PlatformPreset>()
        .register::<CustomPreset>()
        .register::<VideoPresetDTO>()
        .register::<ConversionRequestDTO>()
        .register::<BatchJobSettings>()
        .register::<JobStatus>()
        .register::<FileProgress>()
        .register::<BatchProgress>()
        .register::<OrientationInfo>()
        .register::<PreviewFitMode>()
        .register::<SubtitleLayoutMetrics>()
        .register::<PreviewRenderLayout>()
        .register::<PreviewLayoutRequest>()
        .register::<FileReadiness>()
        .register::<StructuredError>()
        .register::<VideoProgress>()
        .register::<DependencyId>()
        .register::<DependencyScanStatus>()
        .register::<DependencyStatus>()
        .register::<DependencyReport>()
        .register::<aspectshift_htov_lib::download_manager::DependencyInstallEvent>()
        .register::<AppDepsState>()
        .register::<AuthStatus>()
        .register::<LicenseTier>()
        .register::<AuthState>()
        .register::<AuthStatusChangedPayload>()
        .register::<AuthActivationFailedPayload>()
        .register::<ActivationResult>()
        .register::<aspectshift_htov_lib::auth::contracts::ActivateRequest>()
        .register::<aspectshift_htov_lib::auth::contracts::ActivateResponse>()
        .register::<aspectshift_htov_lib::auth::contracts::ActivateErrorResponse>()
        .register::<aspectshift_htov_lib::auth::contracts::RefreshRequest>()
        .register::<aspectshift_htov_lib::auth::contracts::UpdateCheckRequest>()
        .register::<aspectshift_htov_lib::auth::contracts::UpdateCheckResponse>()
        .register::<aspectshift_htov_lib::auth::contracts::UpdateCheckErrorCode>()
        .register::<aspectshift_htov_lib::auth::contracts::UpdateCheckErrorResponse>()
        .register::<aspectshift_htov_lib::auth::contracts::UpdateEntitlement>()
        .register::<aspectshift_htov_lib::auth::contracts::LicenseTierWire>()
        .register::<aspectshift_htov_lib::auth::contracts::BuildChannel>()
        .register::<aspectshift_htov_lib::auth::auth_models::UpdateCheckAvailableResult>()
        .register::<aspectshift_htov_lib::auth::auth_models::UpdateEntitlementCheckStatus>()
        .register::<aspectshift_htov_lib::auth::auth_models::UpdateEntitlementCheckResult>();

    let output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("src")
        .join("types")
        .join("backend.ts");

    Typescript::default()
        .bigint(BigIntExportBehavior::Number)
        .export_to(output_path, &types)?;
    Ok(())
}
