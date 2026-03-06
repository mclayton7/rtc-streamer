use thiserror::Error;

#[derive(Error, Debug)]
pub enum StreamError {
    #[error("Network error: {0}")]
    Network(#[from] std::io::Error),

    #[error("WebRTC error: {0}")]
    WebRtc(#[from] webrtc::Error),

    #[error("RTP error: {0}")]
    Rtp(String),

    #[error("MPEG-TS parsing error: {0}")]
    MpegTs(String),

    #[error("H.264 parsing error: {0}")]
    H264(String),

    #[error("Audio transcoding error: {0}")]
    Audio(String),

    #[error("Signaling error: {0}")]
    Signaling(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Max viewers reached")]
    MaxViewersReached,

    #[error("Stream not available")]
    StreamNotAvailable,
}

pub type Result<T> = std::result::Result<T, StreamError>;
