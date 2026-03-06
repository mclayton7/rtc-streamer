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
    /// Capacity of the channel between the UDP receiver and the MPEG-TS demuxer.
    /// Keep this small (same order as max_buffer_frames) to avoid hidden latency.
    #[serde(default = "default_udp_channel_capacity")]
    pub udp_channel_capacity: usize,
}

fn default_udp_channel_capacity() -> usize {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct MediaConfig {
    pub max_buffer_frames: usize,
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
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.media.max_buffer_frames == 0 {
            anyhow::bail!("media.max_buffer_frames must be > 0");
        }
        if self.webrtc.max_viewers == 0 {
            anyhow::bail!("webrtc.max_viewers must be > 0");
        }
        if self.network.udp_channel_capacity == 0 {
            anyhow::bail!("network.udp_channel_capacity must be > 0");
        }
        if !Path::new(&self.signaling.static_dir).exists() {
            anyhow::bail!(
                "signaling.static_dir '{}' does not exist",
                self.signaling.static_dir
            );
        }
        Ok(())
    }
}
