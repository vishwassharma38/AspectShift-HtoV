use std::process::{Command, Stdio};
use std::io::{BufReader, BufRead};
use crate::video::types::VideoError;
use tauri::{AppHandle, Manager, Emitter};
use std::path::PathBuf;
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct VideoProgress {
    pub job_id: String,
    pub file: String,
    pub ratio: String,
    pub percent: f32,
}

pub struct FfmpegOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn get_ffmpeg_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let resource_dir = app.path().resource_dir().map_err(|_| VideoError::FfmpegNotFound)?;
    let ffmpeg = if cfg!(target_os = "windows") { "ffmpeg.exe" } else { "ffmpeg" };
    let path = resource_dir.join(ffmpeg);
    if path.exists() {
        Ok(path)
    } else {
        Err(VideoError::FfmpegNotFound)
    }
}

pub fn get_ffprobe_path(app: &AppHandle) -> Result<PathBuf, VideoError> {
    let resource_dir = app.path().resource_dir().map_err(|_| VideoError::FfprobeNotFound)?;
    let ffprobe = if cfg!(target_os = "windows") { "ffprobe.exe" } else { "ffprobe" };
    let path = resource_dir.join(ffprobe);
    if path.exists() {
        Ok(path)
    } else {
        Err(VideoError::FfprobeNotFound)
    }
}

pub fn run_ffmpeg(
    app: &AppHandle,
    args: &[&str],
    job_id: &str,
    file_label: &str,
    ratio_label: &str,
    duration_secs: f64,
    cancel_token: Option<tokio_util::sync::CancellationToken>
) -> Result<FfmpegOutput, VideoError> {
    let ffmpeg_path = get_ffmpeg_path(app)?;
    
    // Progress flags will be added manually or ensure they are after input
    let mut final_args = vec![];
    final_args.extend_from_slice(args);
    
    // Ensure -progress pipe:1 is before the last argument (the output path)
    if final_args.len() > 1 {
        let output_path = final_args.pop().unwrap();
        final_args.push("-progress");
        final_args.push("pipe:1");
        final_args.push(output_path);
    }

    let mut child = Command::new(ffmpeg_path)
        .args(&final_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let app_handle = app.clone();
    let job_id = job_id.to_string();
    let file_label = file_label.to_string();
    let ratio_label = ratio_label.to_string();

    // Read progress in real-time
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let l = line.trim();
                if l.starts_with("out_time_ms=") {
                    let time_ms = l.replace("out_time_ms=", "").parse::<f64>().unwrap_or(0.0);
                    let current_secs = time_ms / 1_000_000.0;
                    if duration_secs > 0.0 {
                        let percent = (current_secs / duration_secs) * 100.0;
                        let _ = app_handle.emit("video://progress", VideoProgress {
                            job_id: job_id.clone(),
                            file: file_label.clone(),
                            ratio: ratio_label.clone(),
                            percent: percent as f32,
                        });
                    }
                }
                if l == "progress=end" {
                     let _ = app_handle.emit("video://progress", VideoProgress {
                        job_id: job_id.clone(),
                        file: file_label.clone(),
                        ratio: ratio_label.clone(),
                        percent: 100.0,
                    });
                }
            }
            Err(_) => break,
        }

        // Check for cancellation
        if let Some(ref token) = cancel_token {
            if token.is_cancelled() {
                let _ = child.kill();
                return Err(VideoError::ProcessingFailed { stderr: "Cancelled by user".to_string() });
            }
        }
    }

    let output = child.wait_with_output()?;
    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    if output.status.success() {
        Ok(FfmpegOutput { stdout: stdout_str, stderr: stderr_str, exit_code })
    } else {
        Err(VideoError::ProcessingFailed { stderr: stderr_str })
    }
}

pub fn run_ffprobe(app: &AppHandle, args: &[&str]) -> Result<FfmpegOutput, VideoError> {
    let ffprobe_path = get_ffprobe_path(app)?;
    
    let output = Command::new(ffprobe_path)
        .args(args)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    if output.status.success() {
        Ok(FfmpegOutput { stdout, stderr, exit_code })
    } else {
        Err(VideoError::ProcessingFailed { stderr })
    }
}
