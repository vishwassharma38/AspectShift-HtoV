use aspectshift_htov_lib::video::{
    ffmpeg::VideoProgress,
    types::{
        AppConfig, AspectRatio, AspectRatioTarget, BatchJobSettings, BatchProgress, ConversionRequestDTO,
        CustomPreset, EncodingProfile, FileProgress, FileReadiness, JobStatus, LogoOptions,
        LogoPosition, OrientationInfo, OutputFormat, OutputJob, PlatformConfig, PlatformPreset,
        StructuredError, VideoEffectsSettings, VideoTransform,
    },
};
use specta::TypeCollection;
use specta_typescript::{BigIntExportBehavior, Typescript};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let types = TypeCollection::default()
        .register::<AspectRatio>()
        .register::<EncodingProfile>()
        .register::<OutputFormat>()
        .register::<LogoPosition>()
        .register::<PlatformConfig>()
        .register::<VideoTransform>()
        .register::<LogoOptions>()
        .register::<VideoEffectsSettings>()
        .register::<AppConfig>()
        .register::<OutputJob>()
        .register::<AspectRatioTarget>()
        .register::<PlatformPreset>()
        .register::<CustomPreset>()
        .register::<ConversionRequestDTO>()
        .register::<BatchJobSettings>()
        .register::<JobStatus>()
        .register::<FileProgress>()
        .register::<BatchProgress>()
        .register::<OrientationInfo>()
        .register::<FileReadiness>()
        .register::<StructuredError>()
        .register::<VideoProgress>();

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
