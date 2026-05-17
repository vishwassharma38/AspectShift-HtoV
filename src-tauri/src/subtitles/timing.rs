use crate::subtitles::{SubtitleSegment, WordTiming};

pub struct TimingConfig {
    pub max_words_per_segment: usize,
    pub max_segment_duration_ms: u64,
    pub min_segment_duration_ms: u64,
    pub silence_threshold_ms: u64,
    pub max_gap_ms: u64,
    pub merge_gap_ms: u64,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            max_words_per_segment: 8,
            max_segment_duration_ms: 4000,
            min_segment_duration_ms: 400,
            silence_threshold_ms: 300,
            max_gap_ms: 1000,
            merge_gap_ms: 120,
        }
    }
}

pub fn optimize_segments(
    segments: Vec<SubtitleSegment>,
    config: &TimingConfig,
) -> Vec<SubtitleSegment> {
    let mut words = Vec::new();

    // Flatten segments into words if word timing is missing
    // or just collect all words for re-chunking
    for seg in segments {
        if seg.words.is_empty() {
            // If no word timing, try to infer it (crude but better than nothing)
            let text_words: Vec<&str> = seg.text.split_whitespace().collect();
            if text_words.is_empty() { continue; }
            
            let duration = seg.end_ms - seg.start_ms;
            let word_duration = duration / (text_words.len() as u64);
            
            for (i, w) in text_words.into_iter().enumerate() {
                words.push(WordTiming {
                    word: w.to_string(),
                    start_ms: seg.start_ms + (i as u64 * word_duration),
                    end_ms: seg.start_ms + ((i + 1) as u64 * word_duration),
                });
            }
        } else {
            words.extend(seg.words);
        }
    }

    if words.is_empty() { return Vec::new(); }

    let mut result = Vec::new();
    let mut current_words: Vec<WordTiming> = Vec::new();

    for word in words {
        let should_split = if let Some(last) = current_words.last() {
            let gap = word.start_ms.saturating_sub(last.end_ms);
            let duration = word.end_ms.saturating_sub(current_words[0].start_ms);
            
            gap > config.silence_threshold_ms 
                || current_words.len() >= config.max_words_per_segment
                || duration > config.max_segment_duration_ms
                || hard_sentence_end(&last.word)
        } else {
            false
        };

        if should_split && !current_words.is_empty() {
            result.push(create_segment(current_words));
            current_words = Vec::new();
        }

        current_words.push(word);
    }

    if !current_words.is_empty() {
        result.push(create_segment(current_words));
    }

    normalize_segments(result, config)
}

fn hard_sentence_end(text: &str) -> bool {
    text.ends_with('.') || text.ends_with('!') || text.ends_with('?')
}

fn create_segment(words: Vec<WordTiming>) -> SubtitleSegment {
    let start_ms = words[0].start_ms;
    let end_ms = words.last().unwrap().end_ms;
    let text = words
        .iter()
        .map(|w| w.word.clone())
        .collect::<Vec<_>>()
        .join(" ");

    let sanitized_text = crate::subtitles::sanitize_subtitle_text(&text);

    SubtitleSegment {
        start_ms,
        end_ms,
        text: sanitized_text,
        words,
    }
}

fn normalize_segments(segments: Vec<SubtitleSegment>, config: &TimingConfig) -> Vec<SubtitleSegment> {
    let mut normalized: Vec<SubtitleSegment> = Vec::new();

    for mut seg in segments {
        if seg.words.is_empty() {
            continue;
        }

        // Anchor segment boundaries to spoken words so subtitles don't appear early.
        if let Some(first) = seg.words.first() {
            seg.start_ms = first.start_ms;
        }
        if let Some(last) = seg.words.last() {
            seg.end_ms = last.end_ms;
        }

        if seg.end_ms <= seg.start_ms {
            continue;
        }

        if let Some(prev) = normalized.last_mut() {
            let gap = seg.start_ms.saturating_sub(prev.end_ms);
            let prev_duration = prev.end_ms.saturating_sub(prev.start_ms);

            // Merge ultra-short fragments created by tokenization noise.
            if gap <= config.merge_gap_ms && prev_duration < config.min_segment_duration_ms {
                prev.end_ms = seg.end_ms;
                prev.words.extend(seg.words.into_iter());
                prev.text = crate::subtitles::sanitize_subtitle_text(
                    &prev
                        .words
                        .iter()
                        .map(|w| w.word.clone())
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                continue;
            }
        }

        normalized.push(seg);
    }

    normalized
}
