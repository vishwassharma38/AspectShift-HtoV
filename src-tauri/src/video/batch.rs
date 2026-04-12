use tauri::AppHandle;
use crate::video::types::{AspectRatio, ConversionOptions, ConversionResult, VideoError, BatchConversionResult};
use crate::video::convert::convert_to_ratio;
use tokio::task;

pub async fn batch_convert(
    app: AppHandle,
    input: String,
    output_dir: String,
    ratios: Vec<AspectRatio>,
    options: ConversionOptions
) -> Result<BatchConversionResult, VideoError> {
    let mut handles = Vec::new();

    for ratio in ratios {
        let app_clone = app.clone();
        let input_clone = input.clone();
        let output_dir_clone = output_dir.clone();
        let options_clone = options.clone();

        let handle = task::spawn_blocking(move || {
            convert_to_ratio(&app_clone, input_clone, output_dir_clone, ratio, options_clone)
                .map_err(|e| e.to_string())
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(res) => results.push(res),
            Err(e) => results.push(Err(format!("Task joined failed: {}", e))),
        }
    }

    Ok(BatchConversionResult { results })
}
