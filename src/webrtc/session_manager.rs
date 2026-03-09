use crate::config::WebRtcConfig;
use crate::error::{Result, StreamError};
use crate::media::MediaPipeline;
use crate::monitoring::Metrics;
use crate::webrtc::PeerConnection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;

pub struct SessionManager {
    config: WebRtcConfig,
    pipeline: Arc<MediaPipeline>,
    peers: Arc<RwLock<HashMap<String, Arc<PeerConnection>>>>,
    api: Arc<webrtc::api::API>,
    metrics: Arc<Metrics>,
}

impl SessionManager {
    pub fn new(config: WebRtcConfig, pipeline: Arc<MediaPipeline>, metrics: Arc<Metrics>) -> Result<Self> {
        let mut media_engine = MediaEngine::default();

        // Register H.264 codec
        media_engine.register_default_codecs()?;

        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media_engine)?;

        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build();

        Ok(Self {
            config,
            pipeline,
            peers: Arc::new(RwLock::new(HashMap::new())),
            api: Arc::new(api),
            metrics,
        })
    }

    pub async fn create_peer(&self, session_id: String) -> Result<Arc<PeerConnection>> {
        // Hold write lock for the entire operation to prevent TOCTOU race
        let mut peers = self.peers.write().await;

        if peers.len() >= self.config.max_viewers {
            warn!(
                "Max viewers ({}) reached, rejecting new connection",
                self.config.max_viewers
            );
            return Err(StreamError::MaxViewersReached);
        }

        info!("Creating peer connection for session: {}", session_id);

        let ice_servers = self
            .config
            .stun_servers
            .iter()
            .map(|url| RTCIceServer {
                urls: vec![url.clone()],
                ..Default::default()
            })
            .collect();

        let peer = PeerConnection::new(session_id.clone(), self.api.clone(), ice_servers).await?;

        let peer = Arc::new(peer);
        peers.insert(session_id, peer.clone());

        info!("Peer connection created. Active peers: {}", peers.len());

        Ok(peer)
    }

    pub async fn remove_peer(&self, session_id: &str) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.remove(session_id) {
            info!("Removing peer: {}", session_id);
            if let Err(e) = peer.close().await {
                warn!("Error closing peer {}: {}", session_id, e);
            }
        }
        info!("Active peers: {}", peers.len());
    }

    pub async fn get_peer(&self, session_id: &str) -> Option<Arc<PeerConnection>> {
        let peers = self.peers.read().await;
        peers.get(session_id).cloned()
    }

    pub async fn active_count(&self) -> usize {
        self.peers.read().await.len()
    }

    pub fn pipeline(&self) -> Arc<MediaPipeline> {
        self.pipeline.clone()
    }

    pub fn metrics(&self) -> Arc<Metrics> {
        self.metrics.clone()
    }

    pub fn ice_server_urls(&self) -> Vec<String> {
        self.config.stun_servers.clone()
    }

    pub fn max_viewers(&self) -> usize {
        self.config.max_viewers
    }
}
