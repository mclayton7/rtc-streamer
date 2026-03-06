use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub network: NetworkConfig,
    pub media: MediaConfig,
    pub webrtc: WebRtcConfig,
    pub signaling: SignalingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfig {
    pub udp_bind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaConfig {
    pub max_buffer_frames: usize,
    pub target_latency_ms: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebRtcConfig {
    pub max_viewers: usize,
    pub stun_servers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignalingConfig {
    pub http_bind: String,
    pub static_dir: String,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }
}
