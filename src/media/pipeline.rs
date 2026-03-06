use crate::media::{AudioFrame, H264Parser, VideoFrame};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

pub struct MediaPipeline {
    video_tx: broadcast::Sender<VideoFrame>,
    audio_tx: broadcast::Sender<AudioFrame>,
    h264_parser: Arc<H264Parser>,
}

impl MediaPipeline {
    pub fn new(buffer_size: usize) -> Self {
        let (video_tx, _) = broadcast::channel(buffer_size);
        let (audio_tx, _) = broadcast::channel(buffer_size);
        let h264_parser = Arc::new(H264Parser::new());

        info!(
            "Media pipeline created with buffer size: {}",
            buffer_size
        );

        Self {
            video_tx,
            audio_tx,
            h264_parser,
        }
    }

    pub fn video_sender(&self) -> broadcast::Sender<VideoFrame> {
        self.video_tx.clone()
    }

    pub fn audio_sender(&self) -> broadcast::Sender<AudioFrame> {
        self.audio_tx.clone()
    }

    pub fn subscribe_video(&self) -> broadcast::Receiver<VideoFrame> {
        self.video_tx.subscribe()
    }

    pub fn subscribe_audio(&self) -> broadcast::Receiver<AudioFrame> {
        self.audio_tx.subscribe()
    }

    pub fn h264_parser(&self) -> Arc<H264Parser> {
        self.h264_parser.clone()
    }

    pub fn viewer_count(&self) -> usize {
        self.video_tx.receiver_count()
    }
}
