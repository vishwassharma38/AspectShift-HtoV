pub mod batch_processor;
pub mod convert;
pub mod ffmpeg;
pub mod ffmpeg_args_builder;
pub mod filter_builder;
pub mod lock;
pub mod preset_adapter;
pub mod presets;
pub mod probe;
pub mod queue;
pub mod types;

pub use presets::{delete_preset, get_all_presets, save_preset};
use tauri::{AppHandle, State};
use types::{
    AspectRatio, BatchJobSettings, BatchProgress, ConversionOptions, ConversionResult,
    FileReadiness, OrientationInfo, PlatformConfig,
};

#[tauri::command]
pub async fn detect_orientation(
    app: AppHandle,
    file_path: String,
) -> Result<OrientationInfo, String> {
    probe::detect_orientation(&app, &file_path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn convert_to_ratio(
    app: AppHandle,
    input: String,
    output_dir: String,
    ratio: AspectRatio,
    options: ConversionOptions,
    platform_config: Option<PlatformConfig>,
) -> Result<ConversionResult, String> {
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
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_batch(
    app: AppHandle,
    manager: State<'_, queue::BatchManager>,
    files: Vec<String>,
    settings: BatchJobSettings,
) -> Result<(), String> {
    batch_processor::start_batch(app, manager, files, settings).await
}

#[tauri::command]
pub async fn cancel_batch(manager: State<'_, queue::BatchManager>) -> Result<(), String> {
    batch_processor::cancel_batch(manager).await
}

#[tauri::command]
pub async fn get_batch_status(
    manager: State<'_, queue::BatchManager>,
) -> Result<BatchProgress, String> {
    batch_processor::get_batch_status(manager).await
}

#[tauri::command]
pub async fn clear_batch(manager: State<'_, queue::BatchManager>) -> Result<(), String> {
    batch_processor::clear_batch(manager).await
}

#[tauri::command]
pub async fn check_file_ready(app: AppHandle, path: String) -> Result<FileReadiness, String> {
    probe::check_file_ready(&app, &path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_output_folder(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_shell::ShellExt;
    app.shell().open(path, None).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn release_processing_lock(input_path: String, output_dir: String) -> Result<(), String> {
    lock::release_processing_lock(&input_path, &output_dir).map_err(|e| e.to_string())
}
