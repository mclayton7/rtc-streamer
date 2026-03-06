use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time;
use tracing::info;

pub struct Metrics {
    frames_processed: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    start_time: SystemTime,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            frames_processed: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            start_time: SystemTime::now(),
        }
    }

    pub fn record_frame(&self) {
        self.frames_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_bytes(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn get_frames_processed(&self) -> u64 {
        self.frames_processed.load(Ordering::Relaxed)
    }

    pub fn get_bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    pub fn get_uptime(&self) -> Duration {
        self.start_time.elapsed().unwrap_or(Duration::from_secs(0))
    }

    pub async fn start_periodic_reporting(self: Arc<Self>) {
        let mut interval = time::interval(Duration::from_secs(30));

        loop {
            interval.tick().await;

            let frames = self.get_frames_processed();
            let bytes = self.get_bytes_received();
            let uptime = self.get_uptime();

            info!(
                "Metrics - Uptime: {:?}, Frames: {}, Bytes: {} MB",
                uptime,
                frames,
                bytes / (1024 * 1024)
            );
        }
    }
}
