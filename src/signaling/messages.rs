use crate::media::KlvField;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SignalingMessage {
    Watch,
    /// Server → client: ICE server configuration. Sent before the offer so the
    /// client can construct RTCPeerConnection with the correct ICE servers.
    Config {
        ice_servers: Vec<String>,
    },
    Offer {
        sdp: String,
    },
    Answer {
        sdp: String,
    },
    IceCandidate {
        candidate: String,
    },
    Error {
        message: String,
    },
    /// Server → client: decoded MISB ST 0601 KLV metadata fields.
    Metadata {
        timestamp: u64,
        fields: Vec<KlvField>,
    },
    /// Server → client: whether the UDP source is actively sending frames.
    StreamStatus {
        online: bool,
    },
}
