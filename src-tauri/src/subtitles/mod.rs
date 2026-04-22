pub mod positioning;
pub mod srt_writer;
pub mod whisper_runner;

#[derive(Debug, Clone)]
pub struct SubtitleSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}
