use std::path::Path;
use serde_json::Value;
use tauri::AppHandle;
use crate::video::types::{OrientationInfo, VideoError, FileReadiness};
use crate::video::ffmpeg::run_ffprobe;

pub async fn detect_orientation(app: &AppHandle, file_path: &str) -> Result<OrientationInfo, VideoError> {
    let output = run_ffprobe(app, &[
        "-v", "quiet",
        "-print_format", "json",
        "-show_streams",
        file_path
    ]).await?;

    let json: Value = serde_json::from_str(&output.stdout)?;
    
    let streams = json["streams"].as_array().ok_or_else(|| VideoError::InvalidInput("No streams found".into()))?;
    let video_stream = streams.iter().find(|s| s["codec_type"] == "video")
        .ok_or_else(|| VideoError::InvalidInput("No video stream found".into()))?;

    let width = video_stream["width"].as_u64().unwrap_or(0) as u32;
    let height = video_stream["height"].as_u64().unwrap_or(0) as u32;
    
    let mut rotation = 0;
    
    // Rotation can be in tags or side_data
    if let Some(tags) = video_stream["tags"].as_object() {
        if let Some(rotate) = tags.get("rotate") {
            rotation = rotate.as_str().and_then(|r| r.parse::<i32>().ok()).unwrap_or(0);
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

    let (display_width, display_height) = if rotation == 90 || rotation == 270 || rotation == -90 || rotation == -270 {
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
    
    if !path_buf.exists() {
        return Err(VideoError::FileNotFound(path.to_string()));
    }

    let metadata = std::fs::metadata(path)?;
    let file_size_bytes = metadata.len();
    
    // Check if readable by attempting to open it
    let is_readable = std::fs::File::open(path).is_ok();

    // Check if locked by attempting to open (Windows behavior: read(true) is enough to check for some locks)
    let is_locked = std::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .is_err();

    // Get duration via ffprobe
    let output = run_ffprobe(app, &[
        "-v", "quiet",
        "-show_entries", "format=duration",
        "-of", "default=noprint_wrappers=1:nokey=1",
        path
    ]).await?;

    let estimated_duration_secs = output.stdout.trim().parse::<f64>().unwrap_or(0.0);

    Ok(FileReadiness {
        exists: true,
        is_readable,
        file_size_bytes,
        is_locked,
        estimated_duration_secs,
    })
}
