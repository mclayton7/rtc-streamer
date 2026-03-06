use crate::error::{Result, StreamError};
use crate::media::AudioFrame;

pub struct AudioTranscoder;

impl AudioTranscoder {
    pub fn new() -> Self {
        Self
    }

    // Placeholder for AAC to Opus transcoding
    // This requires symphonia for decoding and opus for encoding
    // For MVP, we can pass through audio or skip it
    pub fn transcode_aac_to_opus(&self, _frame: &AudioFrame) -> Result<Vec<u8>> {
        // TODO: Implement actual transcoding
        // 1. Use symphonia to decode AAC
        // 2. Resample to 48kHz if needed
        // 3. Encode to Opus
        Err(StreamError::Audio(
            "Audio transcoding not yet implemented".to_string(),
        ))
    }
}
