use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tokio::time;
use tracing::info;

pub struct Metrics {
    frames_processed: Arc<AtomicU64>,
    frames_dropped: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    last_frame_at: Arc<AtomicU64>,
    start_time: SystemTime,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            frames_processed: Arc::new(AtomicU64::new(0)),
            frames_dropped: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            last_frame_at: Arc::new(AtomicU64::new(0)),
            start_time: SystemTime::now(),
        }
    }

    pub fn record_frame(&self) {
        self.frames_processed.fetch_add(1, Ordering::Relaxed);
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_frame_at.store(now_ms, Ordering::Relaxed);
    }

    pub fn record_frames_dropped(&self, n: u64) {
        self.frames_dropped.fetch_add(n, Ordering::Relaxed);
    }

    pub fn stream_active(&self, timeout_ms: u64) -> bool {
        let last = self.last_frame_at.load(Ordering::Relaxed);
        if last == 0 {
            return false;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now.saturating_sub(last) < timeout_ms
    }

    pub fn record_bytes(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn get_frames_processed(&self) -> u64 {
        self.frames_processed.load(Ordering::Relaxed)
    }

    pub fn get_frames_dropped(&self) -> u64 {
        self.frames_dropped.load(Ordering::Relaxed)
    }

    pub fn get_bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    pub fn get_uptime(&self) -> Duration {
        self.start_time.elapsed().unwrap_or(Duration::from_secs(0))
    }

    /// Run periodic metrics reporting until the shutdown signal fires.
    pub async fn start_periodic_reporting(
        self: Arc<Self>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let mut interval = time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let frames = self.get_frames_processed();
                    let dropped = self.get_frames_dropped();
                    let bytes = self.get_bytes_received();
                    let uptime = self.get_uptime();

                    info!(
                        "Metrics — uptime: {:?}, frames: {}, dropped: {}, received: {} MB",
                        uptime,
                        frames,
                        dropped,
                        bytes / (1024 * 1024)
                    );
                }
                Ok(()) = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    }
}
