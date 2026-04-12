pub mod types;
pub mod ffmpeg;
pub mod probe;
pub mod convert;
pub mod batch;
pub mod lock;

use tauri::AppHandle;
use types::{AspectRatio, ConversionOptions, ConversionResult, OrientationInfo, FileReadiness, VideoError};

#[tauri::command]
pub async fn detect_orientation(app: AppHandle, file_path: String) -> Result<OrientationInfo, String> {
    probe::detect_orientation(&app, &file_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn convert_to_ratio(
    app: AppHandle,
    input: String,
    output_dir: String,
    ratio: AspectRatio,
    options: ConversionOptions
) -> Result<ConversionResult, String> {
    convert::convert_to_ratio(&app, input, output_dir, ratio, options).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn batch_convert(
    app: AppHandle,
    input: String,
    output_dir: String,
    ratios: Vec<AspectRatio>,
    options: ConversionOptions
) -> Result<types::BatchConversionResult, String> {
    batch::batch_convert(app, input, output_dir, ratios, options).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_file_ready(app: AppHandle, path: String) -> Result<FileReadiness, String> {
    probe::check_file_ready(&app, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn release_processing_lock(input_path: String, output_dir: String) -> Result<(), String> {
    lock::release_processing_lock(&input_path, &output_dir).map_err(|e| e.to_string())
}
