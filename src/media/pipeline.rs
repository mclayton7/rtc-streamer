use crate::media::{H264Parser, MetadataFrame, VideoFrame};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

pub struct MediaPipeline {
    video_tx: broadcast::Sender<VideoFrame>,
    metadata_tx: broadcast::Sender<MetadataFrame>,
    h264_parser: Arc<H264Parser>,
}

impl MediaPipeline {
    pub fn new(buffer_size: usize) -> Self {
        let (video_tx, _) = broadcast::channel(buffer_size);
        let (metadata_tx, _) = broadcast::channel(10);
        let h264_parser = Arc::new(H264Parser::new());

        info!(
            "Media pipeline created with buffer size: {}",
            buffer_size
        );

        Self {
            video_tx,
            metadata_tx,
            h264_parser,
        }
    }

    pub fn video_sender(&self) -> broadcast::Sender<VideoFrame> {
        self.video_tx.clone()
    }

    pub fn metadata_sender(&self) -> broadcast::Sender<MetadataFrame> {
        self.metadata_tx.clone()
    }

    pub fn subscribe_video(&self) -> broadcast::Receiver<VideoFrame> {
        self.video_tx.subscribe()
    }

    pub fn subscribe_metadata(&self) -> broadcast::Receiver<MetadataFrame> {
        self.metadata_tx.subscribe()
    }

    pub fn h264_parser(&self) -> Arc<H264Parser> {
        self.h264_parser.clone()
    }
}
