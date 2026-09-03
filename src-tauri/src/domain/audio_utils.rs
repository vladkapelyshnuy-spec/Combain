//! Mock audio duration estimation utilities.
//!
//! This is a temporary solution for Phase 7 (Video Provider) that estimates
//! audio narration duration based on word count. Phase 8 (TTS Provider) will
//! replace this with real audio duration from synthesized speech.

/// Average speaking rate in words per second for typical narration.
const WORDS_PER_SECOND: f64 = 2.5;

/// Estimate audio duration in milliseconds from narration text.
///
/// Formula: `(word_count / 2.5 words_per_second) * 1000ms`
///
/// This is a rough approximation used to pass a target duration to the video
/// provider before Phase 8 TTS is implemented. Real audio duration will be
/// known once TTS synthesis happens.
pub fn estimate_audio_duration_ms(narration: &str) -> u64 {
    let word_count = narration.split_whitespace().count() as f64;
    if word_count == 0.0 {
        return 0;
    }
    ((word_count / WORDS_PER_SECOND) * 1000.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_duration_for_typical_narration() {
        // 10 words at 2.5 words/sec = 4 seconds = 4000ms
        let text = "This is a typical narration with exactly ten words here.";
        let duration = estimate_audio_duration_ms(text);
        assert_eq!(duration, 4000);
    }

    #[test]
    fn handles_empty_string() {
        let duration = estimate_audio_duration_ms("");
        assert_eq!(duration, 0);
    }

    #[test]
    fn handles_single_word() {
        // 1 word at 2.5 words/sec = 0.4 sec = 400ms
        let duration = estimate_audio_duration_ms("Hello");
        assert_eq!(duration, 400);
    }

    #[test]
    fn handles_long_text() {
        // 25 words at 2.5 words/sec = 10 seconds = 10000ms
        let text = "a ".repeat(25);
        let duration = estimate_audio_duration_ms(&text);
        assert_eq!(duration, 10000);
    }
}
