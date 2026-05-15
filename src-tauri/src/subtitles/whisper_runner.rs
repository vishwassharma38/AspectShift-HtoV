use std::path::{Path, PathBuf};
use tauri::{path::BaseDirectory, AppHandle, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tracing::{error, info};
use uuid::Uuid;

use crate::subtitles::SubtitleSegment;
use crate::video::types::VideoError;

fn get_whisper_binary_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let _sidecar = app.shell().sidecar("whisper").map_err(|e| {
        error!("Failed to create whisper sidecar: {}", e);
        VideoError::WhisperNotFound
    })?;
    Ok(PathBuf::from("whisper"))
}

fn get_whisper_model_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    // Try resolving as a resource in the "resources" subdirectory
    let path = app
        .path()
        .resolve("resources/ggml-medium.en.bin", BaseDirectory::Resource)
        .or_else(|_| {
            app.path()
                .resolve("ggml-medium.en.bin", BaseDirectory::Resource)
        })
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
            let fallback = exe_dir.join("resources/ggml-medium.en.bin");
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

        let mut text = line[(closing_idx + 1)..].trim().to_string();
        let mut words = Vec::new();

        // Basic word timestamp extraction if whisper outputs them with <timestamp>word
        // e.g. [00:00:00.000 --> 00:00:00.000] <00:00:00.000> word1 <00:00:00.500> word2
        if text.contains('<') && text.contains('>') {
            let mut parts = Vec::new();
            let mut current_pos = 0;
            while let Some(start_bracket) = text[current_pos..].find('<') {
                let start_bracket = current_pos + start_bracket;
                if let Some(end_bracket) = text[start_bracket..].find('>') {
                    let end_bracket = start_bracket + end_bracket;
                    let timestamp = &text[start_bracket + 1..end_bracket];
                    if let Some(word_start_ms) = parse_time_to_ms(timestamp) {
                        parts.push((word_start_ms, start_bracket, end_bracket));
                    }
                    current_pos = end_bracket + 1;
                } else {
                    break;
                }
            }

            for i in 0..parts.len() {
                let (start_ms, _, end_bracket) = parts[i];
                let next_start = if i + 1 < parts.len() {
                    parts[i + 1].1
                } else {
                    text.len()
                };
                let word_text = text[end_bracket + 1..next_start].trim();
                if !word_text.is_empty() {
                    let word_end_ms = if i + 1 < parts.len() {
                        parts[i + 1].0
                    } else {
                        end_ms
                    };
                    words.push(crate::subtitles::WordTiming {
                        word: word_text.to_string(),
                        start_ms,
                        end_ms: word_end_ms,
                    });
                }
            }
            
            // Clean up text by removing timestamps for the final text field
            let mut cleaned_text = String::new();
            let mut last_pos = 0;
            for (_, start, end) in parts {
                cleaned_text.push_str(&text[last_pos..start]);
                last_pos = end + 1;
            }
            cleaned_text.push_str(&text[last_pos..]);
            text = cleaned_text.split_whitespace().collect::<Vec<_>>().join(" ");
        }

        segments.push(SubtitleSegment {
            start_ms,
            end_ms,
            text: crate::subtitles::sanitize_subtitle_text(&text),
            words,
        });
    }

    segments
}

use crate::os_utils::OsUtils;

async fn extract_audio_for_whisper(
    app: &AppHandle,
    video_path: &Path,
    source_duration_secs: f64,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    on_progress: Option<&(dyn Fn(f32) + Send + Sync)>,
) -> Result<PathBuf, VideoError> {
    let temp_dir = OsUtils::get_temp_dir(app);
    let wav_filename = format!("whisper_audio_{}.wav", Uuid::new_v4());
    let wav_path = temp_dir.join(wav_filename);

    info!(
        "Extracting audio for Whisper transcription: {}",
        video_path.display()
    );
    info!("Temporary audio destination: {}", wav_path.display());

    let sidecar = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|_| VideoError::FfmpegNotFound)?
        .args(&[
            "-y",
            "-i",
            &video_path.to_string_lossy(),
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            "-progress",
            "pipe:1",
            &wav_path.to_string_lossy(),
        ]);
    let (mut rx, child) = sidecar
        .spawn()
        .map_err(|e| VideoError::ProcessingFailed {
            stderr: format!("Failed to execute ffmpeg for audio extraction: {e}"),
        })?;
    let mut child = Some(child);
    let mut stderr = String::new();
    let mut exit_code = -1;

    loop {
        let event = if let Some(token) = &cancel_token {
            tokio::select! {
                _ = token.cancelled() => {
                    if let Some(child) = child.take() {
                        let _ = child.kill();
                    }
                    let _ = std::fs::remove_file(&wav_path);
                    return Err(VideoError::ProcessingFailed {
                        stderr: "Cancelled by user".to_string(),
                    });
                }
                evt = rx.recv() => evt,
            }
        } else {
            rx.recv().await
        };
        let Some(event) = event else { break; };
        match event {
            CommandEvent::Stdout(line_bytes) => {
                let line = String::from_utf8_lossy(&line_bytes).to_string();
                let l = line.trim();
                if let Some(raw) = l.strip_prefix("out_time_ms=") {
                    let time_ms = raw.parse::<f64>().unwrap_or(0.0);
                    let current_secs = time_ms / 1_000_000.0;
                    if source_duration_secs > 0.0 {
                        let pct = ((current_secs / source_duration_secs) * 100.0)
                            .clamp(0.0, 100.0) as f32;
                        if let Some(cb) = on_progress {
                            cb(pct);
                        }
                    }
                } else if l == "progress=end" {
                    if let Some(cb) = on_progress {
                        cb(100.0);
                    }
                }
            }
            CommandEvent::Stderr(line_bytes) => {
                let line = String::from_utf8_lossy(&line_bytes).to_string();
                stderr.push_str(&line);
                stderr.push('\n');
            }
            CommandEvent::Error(err) => {
                if !stderr.is_empty() && !stderr.ends_with('\n') {
                    stderr.push('\n');
                }
                stderr.push_str(&err);
            }
            CommandEvent::Terminated(payload) => {
                exit_code = payload.code.unwrap_or(-1);
                child = None;
                break;
            }
            _ => {}
        }
    }

    if let Some(child) = child.take() {
        let _ = child.kill();
    }

    if exit_code != 0 {
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
    source_duration_secs: f64,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    on_progress: Option<Box<dyn Fn(f32) + Send + Sync>>,
) -> Result<Vec<SubtitleSegment>, VideoError> {
    let _ = get_whisper_binary_path(app)?;
    let model_path = get_whisper_model_path(app)?;

    // 1. Extract 16kHz mono WAV audio required by Whisper.cpp
    let progress_cb = on_progress.as_ref().map(|cb| cb.as_ref());
    let wav_path = extract_audio_for_whisper(
        app,
        input_path,
        source_duration_secs,
        cancel_token.clone(),
        progress_cb.map(|cb| {
            let cb_ref: &(dyn Fn(f32) + Send + Sync) = cb;
            cb_ref
        }),
    )
    .await?;
    if let Some(cb) = progress_cb {
        cb(35.0);
    }

    info!(
        "Starting whisper transcription for {}",
        input_path.display()
    );

    // 2. Execute Whisper with the extracted WAV file
    let sidecar = app
        .shell()
        .sidecar("whisper")
        .map_err(|_| VideoError::WhisperNotFound)?
        .arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(&wav_path)
        .arg("-l")
        .arg("auto")
        .arg("-otxt");
    let (mut rx, child) = sidecar.spawn().map_err(|e| VideoError::WhisperFailed {
        stderr: format!("Failed to execute whisper sidecar: {e}"),
    })?;
    let mut child = Some(child);
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = -1;

    loop {
        let event = if let Some(token) = &cancel_token {
            tokio::select! {
                _ = token.cancelled() => {
                    if let Some(child) = child.take() {
                        let _ = child.kill();
                    }
                    let _ = std::fs::remove_file(&wav_path);
                    return Err(VideoError::ProcessingFailed {
                        stderr: "Cancelled by user".to_string(),
                    });
                }
                evt = rx.recv() => evt,
            }
        } else {
            rx.recv().await
        };
        let Some(event) = event else { break; };
        match event {
            CommandEvent::Stdout(line_bytes) => {
                let line = String::from_utf8_lossy(&line_bytes).to_string();
                stdout.push_str(&line);
                stdout.push('\n');
                if let Some(end_ms) = parse_whisper_line_end_ms(&line) {
                    if source_duration_secs > 0.0 {
                        let ratio = ((end_ms as f64 / 1000.0) / source_duration_secs).clamp(0.0, 1.0);
                        let mapped = 35.0 + ((ratio as f32) * 60.0);
                        if let Some(cb) = progress_cb {
                            cb(mapped);
                        }
                    }
                }
            }
            CommandEvent::Stderr(line_bytes) => {
                let line = String::from_utf8_lossy(&line_bytes).to_string();
                stderr.push_str(&line);
                stderr.push('\n');
                if let Some(end_ms) = parse_whisper_line_end_ms(&line) {
                    if source_duration_secs > 0.0 {
                        let ratio = ((end_ms as f64 / 1000.0) / source_duration_secs).clamp(0.0, 1.0);
                        let mapped = 35.0 + ((ratio as f32) * 60.0);
                        if let Some(cb) = progress_cb {
                            cb(mapped);
                        }
                    }
                }
            }
            CommandEvent::Error(err) => {
                if !stderr.is_empty() && !stderr.ends_with('\n') {
                    stderr.push('\n');
                }
                stderr.push_str(&err);
            }
            CommandEvent::Terminated(payload) => {
                exit_code = payload.code.unwrap_or(-1);
                child = None;
                break;
            }
            _ => {}
        }
    }

    // 3. Clean up the temporary WAV file
    let _ = std::fs::remove_file(&wav_path);

    // 4. Handle Whisper execution result
    if let Some(child) = child.take() {
        let _ = child.kill();
    }
    if exit_code != 0 {
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

    // 5. Optimize segments for professional timing
    let optimized = crate::subtitles::timing::optimize_segments(
        segments,
        &crate::subtitles::timing::TimingConfig::default(),
    );

    info!(
        "Optimized to {} segments for professional timing",
        optimized.len()
    );

    if let Some(cb) = progress_cb {
        cb(100.0);
    }

    Ok(optimized)
}

fn parse_whisper_line_end_ms(raw_line: &str) -> Option<u64> {
    let line = raw_line.trim();
    if !line.starts_with('[') {
        return None;
    }
    let closing_idx = line.find(']')?;
    let timestamp_part = &line[1..closing_idx];
    let (_, end_raw) = timestamp_part.split_once("-->")?;
    parse_time_to_ms(end_raw.trim())
}
