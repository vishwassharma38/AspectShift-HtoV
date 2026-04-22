use std::path::{Path, PathBuf};
use tauri::{path::BaseDirectory, AppHandle, Manager};
use tauri_plugin_shell::ShellExt;
use tracing::{error, info};
use uuid::Uuid;

use crate::subtitles::SubtitleSegment;
use crate::video::types::VideoError;

fn get_whisper_binary_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let _sidecar = app.shell()
        .sidecar("whisper")
        .map_err(|e| {
            error!("Failed to create whisper sidecar: {}", e);
            VideoError::WhisperNotFound
        })?;
    Ok(PathBuf::from("whisper"))
}

fn get_whisper_model_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    // Try resolving as a resource in the "resources" subdirectory
    let path = app
        .path()
        .resolve("resources/ggml-small.en.bin", BaseDirectory::Resource)
        .or_else(|_| app.path().resolve("ggml-small.en.bin", BaseDirectory::Resource))
        .map_err(|e| {
            error!("Failed to resolve whisper model path: {}", e);
            VideoError::WhisperModelNotFound
        })?;
    
    info!("Checking whisper model at: {}", path.display());
    if path.exists() {
        Ok(path)
    } else {
        // Fallback: try relative to executable directory in dev mode
        let exe_path = std::env::current_exe().unwrap_or_default();
        if let Some(exe_dir) = exe_path.parent() {
            let fallback = exe_dir.join("resources/ggml-small.en.bin");
            info!("Trying fallback whisper model at: {}", fallback.display());
            if fallback.exists() {
                return Ok(fallback);
            }
        }

        error!("Whisper model file does not exist at: {}", path.display());
        Err(VideoError::WhisperModelNotFound)
    }
}

fn parse_time_to_ms(timestamp: &str) -> Option<u64> {
    let mut parts = timestamp.split(':');
    let hours = parts.next()?.trim().parse::<u64>().ok()?;
    let minutes = parts.next()?.trim().parse::<u64>().ok()?;
    let sec_ms = parts.next()?.trim();
    let mut sec_parts = sec_ms.split('.');
    let seconds = sec_parts.next()?.trim().parse::<u64>().ok()?;
    let fraction_raw = sec_parts.next().unwrap_or("0").trim();
    let mut fraction = fraction_raw.to_string();
    if fraction.len() > 3 {
        fraction.truncate(3);
    }
    while fraction.len() < 3 {
        fraction.push('0');
    }
    let millis = fraction.parse::<u64>().ok()?;

    Some((hours * 3_600_000) + (minutes * 60_000) + (seconds * 1_000) + millis)
}

fn parse_whisper_output(stdout: &str) -> Vec<SubtitleSegment> {
    let mut segments = Vec::new();

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if !line.starts_with('[') {
            continue;
        }
        let Some(closing_idx) = line.find(']') else {
            continue;
        };
        let timestamp_part = &line[1..closing_idx];
        let Some((start_raw, end_raw)) = timestamp_part.split_once("-->") else {
            continue;
        };

        let Some(start_ms) = parse_time_to_ms(start_raw.trim()) else {
            continue;
        };
        let Some(end_ms) = parse_time_to_ms(end_raw.trim()) else {
            continue;
        };
        if end_ms <= start_ms {
            continue;
        }

        let text = line[(closing_idx + 1)..].trim();
        if text.is_empty() {
            continue;
        }

        segments.push(SubtitleSegment {
            start_ms,
            end_ms,
            text: text.to_string(),
        });
    }

    segments
}

async fn extract_audio_for_whisper(
    app: &AppHandle,
    video_path: &Path,
) -> Result<PathBuf, VideoError> {
    let temp_dir = std::env::temp_dir();
    let wav_filename = format!("whisper_audio_{}.wav", Uuid::new_v4());
    let wav_path = temp_dir.join(wav_filename);

    info!("Extracting audio for Whisper transcription: {}", video_path.display());
    info!("Temporary audio destination: {}", wav_path.display());

    let output = app.shell()
        .sidecar("ffmpeg")
        .map_err(|_| VideoError::FfmpegNotFound)?
        .args(&[
            "-y",
            "-i",
            &video_path.to_string_lossy(),
            "-ar", "16000",
            "-ac", "1",
            "-c:a", "pcm_s16le",
            &wav_path.to_string_lossy(),
        ])
        .output()
        .await
        .map_err(|e| VideoError::ProcessingFailed {
            stderr: format!("Failed to execute ffmpeg for audio extraction: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        error!("FFmpeg audio extraction failed: {}", stderr);
        // Attempt cleanup if file was created despite failure
        let _ = std::fs::remove_file(&wav_path);
        return Err(VideoError::ProcessingFailed { stderr });
    }

    Ok(wav_path)
}

pub async fn transcribe_to_segments(
    app: &AppHandle,
    input_path: &Path,
) -> Result<Vec<SubtitleSegment>, VideoError> {
    let _ = get_whisper_binary_path(app)?;
    let model_path = get_whisper_model_path(app)?;

    // 1. Extract 16kHz mono WAV audio required by Whisper.cpp
    let wav_path = extract_audio_for_whisper(app, input_path).await?;

    info!(
        "Starting whisper transcription for {}",
        input_path.display()
    );

    // 2. Execute Whisper with the extracted WAV file
    let whisper_result = app.shell()
        .sidecar("whisper")
        .map_err(|_| VideoError::WhisperNotFound)?
        .arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(&wav_path)
        .arg("-l")
        .arg("auto")
        .arg("-otxt")
        .output()
        .await;

    // 3. Clean up the temporary WAV file
    let _ = std::fs::remove_file(&wav_path);

    // 4. Handle Whisper execution result
    let output = whisper_result.map_err(|e| VideoError::WhisperFailed {
        stderr: format!("Failed to execute whisper sidecar: {e}"),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        error!("Whisper failed for {}: {}", input_path.display(), stderr);
        return Err(VideoError::WhisperFailed { stderr });
    }

    let mut segments = parse_whisper_output(&stdout);
    if segments.is_empty() {
        segments = parse_whisper_output(&stderr);
    }
    if segments.is_empty() {
        error!(
            "Whisper produced no timestamped segments for {}",
            input_path.display()
        );
        return Err(VideoError::SubtitleParseError(
            "No timestamped subtitle segments were produced by whisper output".to_string(),
        ));
    }

    info!(
        "Whisper transcription completed for {} with {} segments",
        input_path.display(),
        segments.len()
    );

    Ok(segments)
}
