use crate::video::types::VideoError;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

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

pub async fn run_ffmpeg(
    app: &AppHandle,
    args: &[&str],
    job_id: &str,
    file_label: &str,
    ratio_label: &str,
    duration_secs: f64,
    cancel_token: Option<tokio_util::sync::CancellationToken>
) -> Result<FfmpegOutput, VideoError> {
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

    let sidecar = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|_| VideoError::FfmpegNotFound)?
        .args(final_args);
    let (mut rx, child) = sidecar.spawn().map_err(|e| VideoError::ProcessingFailed {
        stderr: format!("Failed to spawn ffmpeg sidecar: {e}"),
    })?;
    let mut child = Some(child);

    let mut stdout_str = String::new();
    let mut stderr_str = String::new();
    let mut exit_code = -1;
    let app_handle = app.clone();
    let job_id = job_id.to_string();
    let file_label = file_label.to_string();
    let ratio_label = ratio_label.to_string();

    // Read progress in real-time
    loop {
        // Check for cancellation
        if let Some(ref token) = cancel_token {
            if token.is_cancelled() {
                if let Some(child) = child.take() {
                    let _ = child.kill();
                }
                return Err(VideoError::ProcessingFailed { stderr: "Cancelled by user".to_string() });
            }
        }

        let Some(event) = rx.recv().await else {
            break;
        };

        match event {
            CommandEvent::Stdout(line_bytes) => {
                let line = String::from_utf8_lossy(&line_bytes).to_string();
                stdout_str.push_str(&line);
                stdout_str.push('\n');
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
            CommandEvent::Stderr(line_bytes) => {
                let line = String::from_utf8_lossy(&line_bytes).to_string();
                stderr_str.push_str(&line);
                stderr_str.push('\n');
            }
            CommandEvent::Error(err) => {
                if !stderr_str.is_empty() && !stderr_str.ends_with('\n') {
                    stderr_str.push('\n');
                }
                stderr_str.push_str(&err);
            }
            CommandEvent::Terminated(payload) => {
                exit_code = payload.code.unwrap_or(-1);
                break;
            }
            _ => {}
        }
    }

    if exit_code == 0 {
        Ok(FfmpegOutput { stdout: stdout_str, stderr: stderr_str, exit_code })
    } else {
        let stderr = if stderr_str.trim().is_empty() {
            format!("ffmpeg exited with code {exit_code}")
        } else {
            stderr_str
        };
        Err(VideoError::ProcessingFailed { stderr })
    }
}

pub async fn run_ffprobe(app: &AppHandle, args: &[&str]) -> Result<FfmpegOutput, VideoError> {
    let output = app.shell()
        .sidecar("ffprobe")
        .map_err(|_| VideoError::FfprobeNotFound)?
        .args(args)
        .output()
        .await
        .map_err(|e| VideoError::ProcessingFailed {
            stderr: format!("Failed to execute ffprobe sidecar: {e}"),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    if output.status.success() {
        Ok(FfmpegOutput { stdout, stderr, exit_code })
    } else {
        let stderr = if stderr.trim().is_empty() {
            format!("ffprobe exited with code {exit_code}")
        } else {
            stderr
        };
        Err(VideoError::ProcessingFailed { stderr })
    }
}
