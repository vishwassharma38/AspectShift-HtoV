use std::fs;
use std::path::{Path, PathBuf};

use crate::subtitles::SubtitleSegment;
use crate::video::types::VideoError;

fn format_srt_timestamp(total_ms: u64) -> String {
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms % 3_600_000) / 60_000;
    let seconds = (total_ms % 60_000) / 1_000;
    let millis = total_ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

pub fn write_srt_for_input(
    input_path: &Path,
    output_dir: &Path,
    segments: &[SubtitleSegment],
) -> Result<PathBuf, VideoError> {
    if segments.is_empty() {
        return Err(VideoError::InvalidInput(
            "Cannot write SRT without subtitle segments".to_string(),
        ));
    }

    fs::create_dir_all(output_dir)?;
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("subtitle");
    let srt_path = output_dir.join(format!("{stem}.srt"));

    let mut body = String::new();
    for (idx, segment) in segments.iter().enumerate() {
        let start = format_srt_timestamp(segment.start_ms);
        let end = format_srt_timestamp(segment.end_ms);
        body.push_str(&(idx + 1).to_string());
        body.push('\n');
        body.push_str(&format!("{start} --> {end}\n"));
        body.push_str(segment.text.trim());
        body.push_str("\n\n");
    }

    fs::write(&srt_path, body)?;
    Ok(srt_path)
}
