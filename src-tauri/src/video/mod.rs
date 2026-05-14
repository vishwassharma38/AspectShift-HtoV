pub mod batch_processor;
pub mod config;
pub mod convert;
pub mod ffmpeg;
pub mod ffmpeg_args_builder;
pub mod filter_builder;
pub mod lock;
pub mod paths;
pub mod preset_adapter;
pub mod presets;
pub mod probe;
pub mod queue;
pub mod targets;
pub mod types;
pub mod validation;

pub use presets::{
    delete_preset, get_all_aspect_ratio_targets, get_builtin_platform_presets, save_preset,
};

use tauri::{AppHandle, State};
use types::{
    BatchJobSettings, BatchProgress, ConversionRequest, ConversionRequestDTO, ConversionResult,
    FileReadiness, OrientationInfo, StructuredError, VideoError,
};

#[tauri::command]
pub async fn get_all_presets(app: AppHandle) -> Result<Vec<types::VideoPresetDTO>, String> {
    let mut presets = Vec::new();
    for p in presets::get_builtin_presets() {
        presets.push(types::VideoPresetDTO::Platform(p));
    }
    let custom = presets::load_custom_presets(&app).map_err(|e| e.to_string())?;
    for c in custom {
        presets.push(types::VideoPresetDTO::Custom(c));
    }
    Ok(presets)
}

#[tauri::command]
pub async fn detect_orientation(
    app: AppHandle,
    file_path: String,
) -> Result<OrientationInfo, StructuredError> {
    probe::detect_orientation(&app, &file_path)
        .await
        .map_err(StructuredError::from)
}

#[tauri::command]
pub async fn convert_to_ratio(
    app: AppHandle,
    request: ConversionRequestDTO,
) -> Result<ConversionResult, StructuredError> {
    let ConversionRequest {
        input,
        output_dir,
        job,
    } = request.into();

    // RESOLUTION: Resolve Spec + Input into ResolvedJob
    let target = crate::video::targets::normalize_targets(&[job.clone()])
        .map_err(|e| StructuredError::from(VideoError::InvalidInput(e)))?
        .pop()
        .unwrap();

    let output_path = crate::video::paths::resolve_output_path(
        std::path::Path::new(&output_dir),
        std::path::Path::new(&input),
        &target,
        false,
    )
    .to_string_lossy()
    .to_string();

    let alt_output_path = crate::video::paths::resolve_output_path(
        std::path::Path::new(&output_dir),
        std::path::Path::new(&input),
        &target,
        true,
    )
    .to_string_lossy()
    .to_string();

    let resolved_job = crate::video::types::ResolvedJob {
        id: "single-job".to_string(),
        session_id: "single-session".to_string(),
        input_path: input,
        output_path,
        alt_output_path: Some(alt_output_path),
        ratio: job.ratio,
        encoding: job.encoding,
        effects: job.effects,
        platform_config: job.platform_config,
        subtitle_path: None,
    };

    convert::render_single(&app, resolved_job, None, None)
        .await
        .map_err(StructuredError::from)
}

#[tauri::command]
pub async fn start_batch(
    app: AppHandle,
    manager: State<'_, queue::BatchManager>,
    files: Vec<String>,
    settings: BatchJobSettings,
) -> Result<(), StructuredError> {
    batch_processor::start_batch(app, manager, files, settings)
        .await
        .map_err(|e| StructuredError::from(VideoError::InvalidInput(e)))
}

#[tauri::command]
pub async fn cancel_batch(manager: State<'_, queue::BatchManager>) -> Result<(), StructuredError> {
    batch_processor::cancel_batch(manager)
        .await
        .map_err(|e| StructuredError {
            code: "operation_failed".to_string(),
            message: e,
        })
}

#[tauri::command]
pub async fn get_batch_status(
    manager: State<'_, queue::BatchManager>,
) -> Result<BatchProgress, StructuredError> {
    batch_processor::get_batch_status(manager)
        .await
        .map_err(|e| StructuredError {
            code: "operation_failed".to_string(),
            message: e,
        })
}

#[tauri::command]
pub async fn clear_batch(manager: State<'_, queue::BatchManager>) -> Result<(), StructuredError> {
    batch_processor::clear_batch(manager)
        .await
        .map_err(|e| StructuredError {
            code: "operation_failed".to_string(),
            message: e,
        })
}

#[tauri::command]
pub async fn check_file_ready(
    app: AppHandle,
    path: String,
) -> Result<FileReadiness, StructuredError> {
    probe::check_file_ready(&app, &path)
        .await
        .map_err(StructuredError::from)
}

#[tauri::command]
pub async fn open_output_folder(app: AppHandle, path: String) -> Result<(), StructuredError> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| StructuredError {
            code: "operation_failed".to_string(),
            message: e.to_string(),
        })
}
