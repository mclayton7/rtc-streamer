use crate::signaling::messages::SignalingMessage;
use crate::webrtc::SessionManager;
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time;
use tracing::{error, info, warn};
use uuid::Uuid;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

const PING_INTERVAL: Duration = Duration::from_secs(30);
const PONG_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_MESSAGE_BYTES: usize = 64 * 1024; // 64 KB is generous for signaling messages

pub async fn handle_websocket(socket: WebSocket, session_manager: Arc<SessionManager>) {
    let session_id = Uuid::new_v4().to_string();
    info!("WebSocket connection established: {}", session_id);

    let (mut sender, mut receiver) = socket.split();

    // Delay first ping until after the interval so we don't ping right away
    let ping_start = tokio::time::Instant::now() + PING_INTERVAL;
    let mut ping_interval = time::interval_at(ping_start, PING_INTERVAL);
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if text.len() > MAX_MESSAGE_BYTES {
                            warn!(
                                "Oversized signaling message ({} bytes) from session {}, closing",
                                text.len(),
                                session_id
                            );
                            break;
                        }
                        match serde_json::from_str::<SignalingMessage>(&text) {
                            Ok(signal) => {
                                match handle_signaling_message(
                                    signal,
                                    &session_id,
                                    &session_manager,
                                    &mut sender,
                                )
                                .await
                                {
                                    Ok(()) => {}
                                    Err(e) => {
                                        error!("Error handling signaling message: {}", e);
                                        let error_msg = SignalingMessage::Error {
                                            message: e.to_string(),
                                        };
                                        if let Ok(json) = serde_json::to_string(&error_msg) {
                                            let _ = sender.send(Message::Text(json)).await;
                                        }
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse signaling message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_pong = Instant::now();
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("WebSocket closed by client: {}", session_id);
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error for session {}: {}", session_id, e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            _ = ping_interval.tick() => {
                if last_pong.elapsed() > PONG_TIMEOUT {
                    warn!("WebSocket ping timeout for session: {}", session_id);
                    break;
                }
                if sender.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
        }
    }

    // Cleanup
    session_manager.remove_peer(&session_id).await;
    info!("WebSocket connection closed: {}", session_id);
}

async fn handle_signaling_message(
    message: SignalingMessage,
    session_id: &str,
    session_manager: &SessionManager,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> anyhow::Result<()> {
    match message {
        SignalingMessage::Watch => {
            info!("Client requesting to watch stream: {}", session_id);

            // Send ICE server configuration before the offer so the client can
            // construct RTCPeerConnection with the correct STUN/TURN servers
            let config_msg = SignalingMessage::Config {
                ice_servers: session_manager.ice_server_urls(),
            };
            let json = serde_json::to_string(&config_msg)?;
            sender.send(Message::Text(json)).await?;

            // Create peer connection
            let peer = session_manager.create_peer(session_id.to_string()).await?;

            // Create offer
            let offer = peer.create_offer().await?;

            // Send offer to client
            let offer_msg = SignalingMessage::Offer {
                sdp: offer.sdp.clone(),
            };
            let json = serde_json::to_string(&offer_msg)?;
            sender.send(Message::Text(json)).await?;

            info!("Sent config + offer to client: {}", session_id);
        }

        SignalingMessage::Answer { sdp } => {
            info!("Received answer from client: {}", session_id);

            if let Some(peer) = session_manager.get_peer(session_id).await {
                let answer = RTCSessionDescription::answer(sdp)?;
                peer.set_remote_description(answer).await?;

                // Start streaming
                let pipeline = session_manager.pipeline();
                peer.start_streaming(pipeline).await?;

                info!("Streaming started for session: {}", session_id);
            } else {
                warn!("Peer not found for session: {}", session_id);
            }
        }

        SignalingMessage::IceCandidate { candidate } => {
            info!("Received ICE candidate from client: {}", session_id);

            if let Some(peer) = session_manager.get_peer(session_id).await {
                peer.add_ice_candidate(candidate).await?;
            } else {
                warn!("Peer not found for session: {}", session_id);
            }
        }

        _ => {
            warn!("Unexpected message type received from session {}", session_id);
        }
    }

    Ok(())
}
