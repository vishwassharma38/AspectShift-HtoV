use std::path::Path;
use tauri::AppHandle;
use crate::video::types::{AspectRatio, ConversionOptions, ConversionResult, VideoError, OrientationInfo};
use crate::video::probe::{detect_orientation, check_file_ready};
use crate::video::ffmpeg::run_ffmpeg;
use crate::video::lock::ProcessingLock;

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
    input: String,
    output_dir: String,
    ratio: AspectRatio,
    options: ConversionOptions
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

    // 6. Passthrough Check
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
         run_ffmpeg(app, &args, &file_label, &ratio_label, duration)?;
         return Ok(ConversionResult {
             output_path,
             ratio,
             skipped: false,
         });
    }

    // 7. Filter Construction
    let max_height = 1920;
    let th = orientation.display_height.min(max_height);
    let th = (th as f32 / 2.0).round() as u32 * 2;
    let tw = ((th as f32 * target_ratio) / 2.0).round() as u32 * 2;

    let filter = if options.blur_background {
        format!(
            "[0:v]split[bg][fg];\
             [bg]scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th},gblur=sigma={sigma}[bg_blurred];\
             [fg]scale=w={tw}:h={th}:force_original_aspect_ratio=decrease[fg_scaled];\
             [bg_blurred][fg_scaled]overlay=(main_w-overlay_w)/2:(main_h-overlay_h)/2",
            tw = tw, th = th, sigma = options.blur_sigma
        )
    } else {
        format!(
            "scale=w={tw}:h={th}:force_original_aspect_ratio=increase,crop={tw}:{th}",
            tw = tw, th = th
        )
    };

    // 8. FFmpeg Command
    let mut args = vec![
        "-i", &input,
        "-vf", &filter,
    ];

    if options.remove_audio {
        args.push("-an");
    } else {
        args.extend_from_slice(&["-c:a", "aac", "-b:a", "128k"]);
    }

    let quality_args = options.quality.get_ffmpeg_args();
    for arg in quality_args {
        args.push(arg);
    }

    args.extend_from_slice(&["-y", &output_path]);

    run_ffmpeg(app, &args, &file_label, &ratio_label, duration)?;

    Ok(ConversionResult {
        output_path,
        ratio,
        skipped: false,
    })
}
