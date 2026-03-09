use thiserror::Error;

#[derive(Error, Debug)]
pub enum StreamError {
    #[error("Network error: {0}")]
    Network(#[from] std::io::Error),

    #[error("WebRTC error: {0}")]
    WebRtc(#[from] webrtc::Error),

    #[error("Max viewers reached")]
    MaxViewersReached,
}

pub type Result<T> = std::result::Result<T, StreamError>;
