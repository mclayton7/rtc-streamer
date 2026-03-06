use crate::config::SignalingConfig;
use crate::error::Result;
use crate::monitoring::Metrics;
use crate::signaling::handlers::handle_websocket;
use crate::webrtc::SessionManager;
use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tracing::info;

#[derive(Clone)]
struct AppState {
    session_manager: Arc<SessionManager>,
    metrics: Arc<Metrics>,
}

pub struct SignalingServer {
    config: SignalingConfig,
    session_manager: Arc<SessionManager>,
    metrics: Arc<Metrics>,
}

impl SignalingServer {
    pub fn new(
        config: SignalingConfig,
        session_manager: Arc<SessionManager>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            config,
            session_manager,
            metrics,
        }
    }

    pub async fn start(self) -> Result<()> {
        let state = AppState {
            session_manager: self.session_manager,
            metrics: self.metrics,
        };

        let app = Router::new()
            .route("/signal", get(websocket_handler))
            .route("/api/health", get(health_handler))
            .route("/api/stats", get(stats_handler))
            .nest_service("/", ServeDir::new(&self.config.static_dir))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(&self.config.http_bind).await?;

        info!("Signaling server listening on {}", self.config.http_bind);

        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_websocket(socket, state.session_manager))
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let uptime_secs = state.metrics.get_uptime().as_secs();
    Json(json!({
        "status": "ok",
        "uptime_secs": uptime_secs
    }))
}

async fn stats_handler(State(state): State<AppState>) -> impl IntoResponse {
    let active_peers = state.session_manager.active_count().await;
    let max_viewers = state.session_manager.max_viewers();
    let frames_received = state.metrics.get_frames_processed();
    let frames_dropped = state.metrics.get_frames_dropped();
    let bytes_received = state.metrics.get_bytes_received();

    Json(json!({
        "active_peers": active_peers,
        "max_viewers": max_viewers,
        "available_slots": max_viewers.saturating_sub(active_peers),
        "frames_received": frames_received,
        "frames_dropped": frames_dropped,
        "bytes_received": bytes_received,
        "stream_active": frames_received > 0
    }))
}
