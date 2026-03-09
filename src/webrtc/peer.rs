use crate::error::Result;
use crate::media::MediaPipeline;
use crate::monitoring::Metrics;
use crate::webrtc::TrackSender;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
use webrtc::api::API;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocal;

pub struct PeerConnection {
    session_id: String,
    pc: Arc<RTCPeerConnection>,
    video_track: Arc<TrackLocalStaticRTP>,
    track_sender: Arc<Mutex<Option<TrackSender>>>,
}

impl PeerConnection {
    pub async fn new(
        session_id: String,
        api: Arc<API>,
        ice_servers: Vec<RTCIceServer>,
    ) -> Result<Self> {
        let config = RTCConfiguration {
            ice_servers,
            ..Default::default()
        };

        let pc = api.new_peer_connection(config).await?;

        // Create video track
        let video_track = Arc::new(TrackLocalStaticRTP::new(
            RTCRtpCodecCapability {
                mime_type: "video/H264".to_owned(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_owned(),
                rtcp_feedback: vec![],
            },
            "video".to_owned(),
            "webrtc-rs".to_owned(),
        ));

        let rtp_sender = pc
            .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
            .await?;

        // Read RTCP packets (required to keep the sender alive)
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            let mut rtcp_buf = vec![0u8; 1500];
            while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {
                // Process RTCP feedback if needed
            }
            info!("RTCP reader closed for session: {}", session_id_clone);
        });

        // Handle connection state changes
        let session_id_clone = session_id.clone();
        pc.on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
            let session_id = session_id_clone.clone();
            Box::pin(async move {
                info!(
                    "Peer connection state changed: {:?} (session: {})",
                    state, session_id
                );
                if state == RTCPeerConnectionState::Failed
                    || state == RTCPeerConnectionState::Disconnected
                {
                    info!("Peer disconnected: {}", session_id);
                }
            })
        }));

        Ok(Self {
            session_id,
            pc: Arc::new(pc),
            video_track,
            track_sender: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription> {
        let offer = self.pc.create_offer(None).await?;
        self.pc.set_local_description(offer.clone()).await?;
        Ok(offer)
    }

    pub async fn set_remote_description(&self, sdp: RTCSessionDescription) -> Result<()> {
        self.pc.set_remote_description(sdp).await?;
        Ok(())
    }

    pub async fn add_ice_candidate(&self, candidate: String) -> Result<()> {
        use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;

        let init = RTCIceCandidateInit {
            candidate,
            ..Default::default()
        };

        self.pc.add_ice_candidate(init).await?;
        Ok(())
    }

    pub async fn start_streaming(&self, pipeline: Arc<MediaPipeline>, metrics: Arc<Metrics>) -> Result<()> {
        info!("Starting streaming for session: {}", self.session_id);

        let sender = TrackSender::new(
            self.session_id.clone(),
            self.video_track.clone(),
            pipeline,
            metrics,
        );

        sender.start().await?;

        let mut track_sender = self.track_sender.lock().await;
        *track_sender = Some(sender);

        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        info!("Closing peer connection: {}", self.session_id);

        // Stop track sender
        let mut track_sender = self.track_sender.lock().await;
        *track_sender = None;

        self.pc.close().await?;
        Ok(())
    }

}
