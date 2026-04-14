use std::path::Path;
use tauri::AppHandle;
use crate::video::types::{AspectRatio, ConversionOptions, ConversionResult, VideoError};
use crate::video::probe::{detect_orientation, check_file_ready};
use crate::video::ffmpeg::run_ffmpeg;
use crate::video::lock::ProcessingLock;
use crate::video::preset_adapter::legacy_to_preset;
use crate::video::filter_builder::build_filter_graph;
use crate::video::ffmpeg_args_builder::build_ffmpeg_args;

pub fn check_already_processed(input: &str, output_dir: &str, ratio: &AspectRatio, options: &ConversionOptions) -> bool {
    if !options.skip_existing {
        return false;
    }

    let input_path = Path::new(input);
    let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    let ext = options.output_format.get_extension();
    let tag = ratio.get_tag().replace(':', "x");
    let output_name = format!("{}_{}.{}", stem, tag, ext);
    let output_path = Path::new(output_dir).join(output_name);

    output_path.exists()
}

pub fn convert_to_ratio(
    app: &AppHandle,
    job_id: String,
    input: String,
    output_dir: String,
    ratio: AspectRatio,
    options: ConversionOptions,
    cancel_token: Option<tokio_util::sync::CancellationToken>
) -> Result<ConversionResult, VideoError> {
    // 1. File Readiness Check
    let readiness = check_file_ready(app, &input)?;
    let duration = readiness.estimated_duration_secs;

    // 2. Already Processed Check
    if check_already_processed(&input, &output_dir, &ratio, &options) {
        let input_path = Path::new(&input);
        let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
        let ext = options.output_format.get_extension();
        let tag = ratio.get_tag().replace(':', "x");
        let output_name = format!("{}_{}.{}", stem, tag, ext);
        let output_path = Path::new(&output_dir).join(output_name).to_string_lossy().to_string();
        
        return Ok(ConversionResult {
            output_path,
            ratio,
            skipped: true,
        });
    }

    // 3. Acquire Lock
    let _lock = ProcessingLock::acquire(&input, &output_dir)?;

    // 4. Orientation Detection
    let orientation = detect_orientation(app, &input)?;

    // 5. Output Path
    let input_path = Path::new(&input);
    let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    let ext = options.output_format.get_extension();
    let tag = ratio.get_tag().replace(':', "x");
    let output_name = format!("{}_{}.{}", stem, tag, ext);
    let output_path_buf = Path::new(&output_dir).join(output_name);
    let output_path = output_path_buf.to_string_lossy().to_string();

    let file_label = stem.to_string();
    let ratio_label = ratio.get_tag().to_string();

    // 6. Bridge to Preset System
    let preset = legacy_to_preset(ratio.clone(), options.clone());

    // 7. Passthrough Check
    let current_ratio = orientation.display_width as f32 / orientation.display_height as f32;
    let target_ratio = ratio.get_ratio();
    let ratio_diff = (current_ratio - target_ratio).abs() / target_ratio;

    if orientation.is_vertical && ratio_diff < 0.02 && !options.blur_background && !options.remove_audio {
         let args = [
             "-i", &input,
             "-c", "copy",
             "-y",
             &output_path
         ];
         run_ffmpeg(app, &args, &job_id, &file_label, &ratio_label, duration, cancel_token)?;
         return Ok(ConversionResult {
             output_path,
             ratio,
             skipped: false,
         });
    }

    // 8. Filter Construction
    let filter = build_filter_graph(&preset, &orientation);

    // 9. FFmpeg Command Building
    let args_vec = build_ffmpeg_args(&input, &output_path, &filter, &preset);
    let args: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();

    run_ffmpeg(app, &args, &job_id, &file_label, &ratio_label, duration, cancel_token)?;

    Ok(ConversionResult {
        output_path,
        ratio,
        skipped: false,
    })
}
