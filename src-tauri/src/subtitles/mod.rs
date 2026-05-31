pub mod ass_writer;
pub mod positioning;
pub mod srt_writer;
pub mod timing;
pub mod whisper_runner;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    #[serde(default)]
    pub words: Vec<WordTiming>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTiming {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Sanitizes subtitle text by removing invalid leading punctuation artifacts.
/// Preserves legitimate starts like speaker dashes, sound effect brackets, quotes, and ellipsis.
pub fn sanitize_subtitle_text(text: &str) -> String {
    let mut s = text.trim();

    loop {
        let initial_len = s.len();

        // Remove leading commas, semicolons, and colons
        if s.starts_with(',') || s.starts_with(';') || s.starts_with(':') {
            s = s[1..].trim_start();
        }
        // Remove leading dots if they aren't an ellipsis
        else if s.starts_with('.') && !s.starts_with("...") {
            s = s[1..].trim_start();
        }
        // Remove leading exclamation or question marks (usually artifacts at start of segment)
        else if s.starts_with('!') || s.starts_with('?') {
            s = s[1..].trim_start();
        }

        if s.len() == initial_len || s.is_empty() {
            break;
        }
    }

    s.to_string()
}
