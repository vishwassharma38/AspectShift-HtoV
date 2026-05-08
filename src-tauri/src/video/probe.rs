use crate::os_utils::OsUtils;
use crate::video::ffmpeg::run_ffprobe;
use crate::video::types::{FileReadiness, OrientationInfo, VideoError};
use serde_json::Value;
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

pub async fn detect_orientation(
    app: &AppHandle,
    file_path: &str,
) -> Result<OrientationInfo, VideoError> {
    let input_path = Path::new(file_path);
    // Validate early so ffprobe is never invoked for folders/unsupported paths.
    if !input_path.exists() {
        return Err(VideoError::FileNotFound(file_path.to_string()));
    }
    if !input_path.is_file() {
        return Err(VideoError::InvalidInput(format!(
            "Input path is not a file: {}",
            file_path
        )));
    }
    if !OsUtils::has_supported_video_extension(input_path) {
        return Err(VideoError::InvalidInput(format!(
            "Unsupported video file type: {}",
            file_path
        )));
    }

    let output = run_ffprobe(
        app,
        &[
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            file_path,
        ],
    )
    .await?;

    let json: Value = serde_json::from_str(&output.stdout)?;

    let streams = json["streams"]
        .as_array()
        .ok_or_else(|| VideoError::InvalidInput("No streams found".into()))?;
    let video_stream = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .ok_or_else(|| VideoError::InvalidInput("No video stream found".into()))?;

    let width = video_stream["width"].as_u64().unwrap_or(0) as u32;
    let height = video_stream["height"].as_u64().unwrap_or(0) as u32;

    let mut rotation = 0;

    // Rotation can be in tags or side_data
    if let Some(tags) = video_stream["tags"].as_object() {
        if let Some(rotate) = tags.get("rotate") {
            rotation = rotate
                .as_str()
                .and_then(|r| r.parse::<i32>().ok())
                .unwrap_or(0);
        }
    }

    if rotation == 0 {
        if let Some(side_data) = video_stream["side_data_list"].as_array() {
            for data in side_data {
                if let Some(rot) = data["rotation"].as_i64() {
                    rotation = rot as i32;
                    break;
                }
            }
        }
    }

    let (display_width, display_height) =
        if rotation == 90 || rotation == 270 || rotation == -90 || rotation == -270 {
            (height, width)
        } else {
            (width, height)
        };

    let is_vertical = display_width < display_height;

    Ok(OrientationInfo {
        width,
        height,
        rotation,
        is_vertical,
        display_width,
        display_height,
    })
}

pub async fn check_file_ready(app: &AppHandle, path: &str) -> Result<FileReadiness, VideoError> {
    let path_buf = Path::new(path);

    // Validate early so ffprobe is never invoked for folders/unsupported paths.
    if !path_buf.exists() {
        return Err(VideoError::FileNotFound(path.to_string()));
    }
    if !path_buf.is_file() {
        return Err(VideoError::InvalidInput(format!(
            "Input path is not a file: {}",
            path
        )));
    }
    if !OsUtils::has_supported_video_extension(path_buf) {
        return Err(VideoError::InvalidInput(format!(
            "Unsupported video file type: {}",
            path
        )));
    }

    let metadata = std::fs::metadata(path)?;
    let file_size_bytes = metadata.len();

    // Check if readable by attempting to open it
    let is_readable = std::fs::File::open(path).is_ok();

    // Check if locked using OS-specific logic
    let is_locked = OsUtils::is_file_locked(path_buf);

    // Get duration via ffprobe
    let output = run_ffprobe(
        app,
        &[
            "-v",
            "quiet",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path,
        ],
    )
    .await?;

    let estimated_duration_secs = output.stdout.trim().parse::<f64>().unwrap_or(0.0);

    Ok(FileReadiness {
        exists: true,
        is_readable,
        file_size_bytes,
        is_locked,
        estimated_duration_secs,
    })
}

pub async fn generate_thumbnail(
    app: &AppHandle,
    input_path: &str,
    output_path: &str,
) -> Result<String, VideoError> {
    let input = Path::new(input_path);
    if !input.exists() {
        return Err(VideoError::FileNotFound(input_path.to_string()));
    }

    // Extract frame at 1s, or 0s if very short. 
    // -vf "thumbnail,scale=320:-1" to make it small and representative
    let args = [
        "-ss", "00:00:01",
        "-i", input_path,
        "-vframes", "1",
        "-vf", "scale=320:-1",
        "-f", "image2",
        "-y",
        output_path,
    ];

    let sidecar = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|_| VideoError::FfmpegNotFound)?
        .args(args);

    let output: tauri_plugin_shell::process::Output = sidecar.output().await.map_err(|e| VideoError::ProcessingFailed {
        stderr: format!("Failed to generate thumbnail: {e}"),
    })?;

    if !output.status.success() {
        // Try at 0s if 1s failed (video might be < 1s)
        let args_fallback = [
            "-i", input_path,
            "-vframes", "1",
            "-vf", "scale=320:-1",
            "-f", "image2",
            "-y",
            output_path,
        ];
        let sidecar_fallback = app
            .shell()
            .sidecar("ffmpeg")
            .map_err(|_| VideoError::FfmpegNotFound)?
            .args(args_fallback);
        
        let output_fallback: tauri_plugin_shell::process::Output = sidecar_fallback.output().await.map_err(|e| VideoError::ProcessingFailed {
            stderr: format!("Failed to generate thumbnail fallback: {e}"),
        })?;

        if !output_fallback.status.success() {
            return Err(VideoError::ProcessingFailed {
                stderr: String::from_utf8_lossy(&output_fallback.stderr).to_string(),
            });
        }
    }

    Ok(output_path.to_string())
}
