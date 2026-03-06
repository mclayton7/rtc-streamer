mod config;
mod error;
mod ingest;
mod media;
mod monitoring;
mod signaling;
mod webrtc;

use config::Config;
use ingest::{MpegTsDemuxer, UdpReceiver};
use media::MediaPipeline;
use monitoring::Metrics;
use signaling::SignalingServer;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use webrtc::SessionManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Starting RTC Streamer...");

    // Load and validate configuration
    let config = Config::from_file("config.toml")?;
    info!("Configuration loaded");

    // Create media pipeline
    let pipeline = Arc::new(MediaPipeline::new(config.media.max_buffer_frames));

    // Create and start metrics reporting
    let metrics = Arc::new(Metrics::new());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let metrics_clone = metrics.clone();
    tokio::spawn(async move {
        metrics_clone.start_periodic_reporting(shutdown_rx).await;
    });

    // Single channel: UDP receiver → demuxer (RTP depacketization is inlined)
    let (udp_tx, udp_rx) = mpsc::channel(config.network.udp_channel_capacity);

    // Start UDP receiver
    let udp_receiver = UdpReceiver::new(config.network.udp_bind.clone(), metrics.clone());
    tokio::spawn(async move {
        if let Err(e) = udp_receiver.start(udp_tx).await {
            error!("UDP receiver error: {}", e);
        }
    });

    // Start MPEG-TS demuxer (handles RTP depacketization internally)
    let demuxer = MpegTsDemuxer::new(
        pipeline.video_sender(),
        pipeline.audio_sender(),
        metrics.clone(),
    );
    tokio::spawn(async move {
        if let Err(e) = demuxer.start(udp_rx).await {
            error!("MPEG-TS demuxer error: {}", e);
        }
    });

    // Create WebRTC session manager
    let session_manager = Arc::new(SessionManager::new(config.webrtc.clone(), pipeline.clone())?);

    // Start signaling server
    let signaling_server =
        SignalingServer::new(config.signaling.clone(), session_manager, metrics);

    info!("All components initialized");
    info!("UDP listening on: {}", config.network.udp_bind);
    info!("HTTP server on: {}", config.signaling.http_bind);
    info!("Max viewers: {}", config.webrtc.max_viewers);
    info!("Ready to receive streams!");

    // Run until signaling server exits or shutdown signal received
    tokio::select! {
        result = signaling_server.start() => {
            if let Err(e) = result {
                error!("Signaling server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Shutdown signal received");
        }
    }

    // Signal metrics loop to stop
    let _ = shutdown_tx.send(true);

    info!("Shutting down...");
    Ok(())
}
