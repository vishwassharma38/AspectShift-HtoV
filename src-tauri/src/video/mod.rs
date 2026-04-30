pub mod batch_processor;
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
pub mod types;
pub mod validation;

pub use presets::{delete_preset, get_all_presets, save_preset};
use tauri::{AppHandle, State};
use types::{
    BatchJobSettings, BatchProgress, ConversionRequest, ConversionRequestDTO, ConversionResult,
    FileReadiness, OrientationInfo, StructuredError, VideoError,
};

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
        ratio,
        options,
        platform_config,
    } = request.into();

    convert::convert_to_ratio(
        &app,
        "single-job".to_string(),
        input,
        output_dir,
        ratio,
        options,
        platform_config,
        None,
        None,
        None,
    )
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
    batch_processor::cancel_batch(manager).await.map_err(|e| StructuredError {
        code: "operation_failed".to_string(),
        message: e,
    })
}

#[tauri::command]
pub async fn get_batch_status(
    manager: State<'_, queue::BatchManager>,
) -> Result<BatchProgress, StructuredError> {
    batch_processor::get_batch_status(manager).await.map_err(|e| StructuredError {
        code: "operation_failed".to_string(),
        message: e,
    })
}

#[tauri::command]
pub async fn clear_batch(manager: State<'_, queue::BatchManager>) -> Result<(), StructuredError> {
    batch_processor::clear_batch(manager).await.map_err(|e| StructuredError {
        code: "operation_failed".to_string(),
        message: e,
    })
}

#[tauri::command]
pub async fn check_file_ready(app: AppHandle, path: String) -> Result<FileReadiness, StructuredError> {
    probe::check_file_ready(&app, &path)
        .await
        .map_err(StructuredError::from)
}

#[tauri::command]
pub async fn open_output_folder(app: AppHandle, path: String) -> Result<(), StructuredError> {
    use tauri_plugin_shell::ShellExt;
    app.shell().open(path, None).map_err(|e| StructuredError {
        code: "operation_failed".to_string(),
        message: e.to_string(),
    })
}


