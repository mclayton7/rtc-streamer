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
}
