use aspectshift_htov_lib::video::{
    ffmpeg::VideoProgress,
    types::{
        AspectRatio, BatchJobSettings, BatchProgress, ConversionOptions, ConversionRequestDTO,
        FileProgress, FileReadiness, JobStatus, JobTarget, LogoOptions, LogoPosition,
        OrientationInfo, OutputFormat, PartialConversionOptions, PlatformConfig, QualityPreset,
        StructuredError, VideoPreset, VideoTransform,
    },
};
use std::path::PathBuf;
use specta::TypeCollection;
use specta_typescript::{BigIntExportBehavior, Typescript};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let types = TypeCollection::default()
        .register::<AspectRatio>()
        .register::<QualityPreset>()
        .register::<OutputFormat>()
        .register::<LogoPosition>()
        .register::<PlatformConfig>()
        .register::<VideoTransform>()
        .register::<LogoOptions>()
        .register::<ConversionOptions>()
        .register::<PartialConversionOptions>()
        .register::<ConversionRequestDTO>()
        .register::<JobTarget>()
        .register::<BatchJobSettings>()
        .register::<JobStatus>()
        .register::<FileProgress>()
        .register::<BatchProgress>()
        .register::<OrientationInfo>()
        .register::<FileReadiness>()
        .register::<VideoPreset>()
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
