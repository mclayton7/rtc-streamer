use crate::error::Result;
use crate::media::{AudioFrame, H264Parser, VideoFrame};
use crate::monitoring::Metrics;
use bytes::{Buf, Bytes, BytesMut};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info};

const MPEG_TS_PACKET_SIZE: usize = 188;
const SYNC_BYTE: u8 = 0x47;

pub struct MpegTsDemuxer {
    video_tx: broadcast::Sender<VideoFrame>,
    audio_tx: broadcast::Sender<AudioFrame>,
    metrics: Arc<Metrics>,
}

impl MpegTsDemuxer {
    pub fn new(
        video_tx: broadcast::Sender<VideoFrame>,
        audio_tx: broadcast::Sender<AudioFrame>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            video_tx,
            audio_tx,
            metrics,
        }
    }

    pub async fn start(self, mut rx: mpsc::Receiver<Bytes>) -> Result<()> {
        let mut buffer = BytesMut::new();
        let mut packet_count = 0u64;
        let mut video_pes_buffer = BytesMut::new();
        let mut fallback_pts = 0u64;

        info!("MPEG-TS demuxer started");

        while let Some(data) = rx.recv().await {
            // Strip RTP header if present, then append MPEG-TS payload
            let payload = Self::strip_rtp_header(&data);
            buffer.extend_from_slice(payload);

            // Process complete MPEG-TS packets
            while buffer.len() >= MPEG_TS_PACKET_SIZE {
                // Find sync byte
                if buffer[0] != SYNC_BYTE {
                    if let Some(pos) = buffer.iter().position(|&b| b == SYNC_BYTE) {
                        buffer.advance(pos);
                        continue;
                    } else {
                        buffer.clear();
                        break;
                    }
                }

                let packet_data = buffer.split_to(MPEG_TS_PACKET_SIZE);
                packet_count += 1;

                if packet_count % 1000 == 0 {
                    debug!("Processed {} MPEG-TS packets", packet_count);
                }

                // Parse packet header
                let pid = (((packet_data[1] & 0x1F) as u16) << 8) | (packet_data[2] as u16);
                let payload_start = (packet_data[1] & 0x40) != 0;
                let has_adaptation = (packet_data[3] & 0x20) != 0;
                let has_payload = (packet_data[3] & 0x10) != 0;

                if !has_payload {
                    continue;
                }

                let mut offset = 4;

                // Skip adaptation field
                if has_adaptation {
                    let adaptation_len = packet_data[offset] as usize;
                    offset += 1 + adaptation_len;
                }

                if offset >= packet_data.len() {
                    continue;
                }

                let payload = &packet_data[offset..];

                // Simplified: Assume video on PID 256
                // In production, parse PAT/PMT for dynamic PID discovery
                if pid == 256 {
                    if payload_start && !video_pes_buffer.is_empty() {
                        // Process accumulated PES packet
                        self.process_video_pes(&video_pes_buffer, &mut fallback_pts);
                        video_pes_buffer.clear();
                    }
                    video_pes_buffer.extend_from_slice(payload);
                }
            }
        }

        Ok(())
    }

    fn process_video_pes(&self, data: &[u8], fallback_pts: &mut u64) {
        // Minimum PES header: start code (3) + stream_id (1) + length (2) + flags (3) = 9 bytes
        if data.len() < 9 {
            return;
        }

        // Verify PES start code prefix
        if data[0..3] != [0, 0, 1] {
            return;
        }

        // Byte 7: PTS/DTS flags (bits 7-6)
        let pts_dts_flags = (data[7] >> 6) & 0x03;

        // Byte 8: PES header data length (number of bytes following this field before payload)
        let pes_header_data_len = data[8] as usize;
        let payload_start = 9 + pes_header_data_len;

        if payload_start >= data.len() {
            return;
        }

        // Extract PTS from PES header if present (flags >= 2 means PTS is encoded)
        let pts = if pts_dts_flags >= 2 && data.len() >= 14 {
            Self::extract_pts(&data[9..14])
        } else {
            // Fallback: synthesize PTS at ~30fps (3000 ticks in 90kHz clock)
            *fallback_pts += 3000;
            *fallback_pts
        };

        // Elementary stream data starts after PES header
        let es_data = &data[payload_start..];
        if es_data.is_empty() {
            return;
        }

        let is_keyframe = H264Parser::is_keyframe(es_data);

        self.metrics.record_frame();

        let frame = VideoFrame {
            data: Arc::new(es_data.to_vec()),
            timestamp: pts,
            dts: pts,
            is_keyframe,
        };

        if let Err(e) = self.video_tx.send(frame) {
            debug!("No video subscribers: {}", e);
        }
    }

    /// Extract 33-bit PTS value from 5 PES header bytes.
    /// PTS is encoded across 5 bytes with marker bits interspersed.
    fn extract_pts(data: &[u8]) -> u64 {
        (((data[0] as u64 >> 1) & 0x07) << 30)
            | ((data[1] as u64) << 22)
            | (((data[2] as u64 >> 1) & 0x7F) << 15)
            | ((data[3] as u64) << 7)
            | ((data[4] as u64 >> 1) & 0x7F)
    }

    /// Strip RTP header from a UDP packet if present, returning the MPEG-TS payload.
    /// Passes through unchanged if the packet is not RTP (e.g., raw MPEG-TS over UDP).
    fn strip_rtp_header(packet: &[u8]) -> &[u8] {
        // RTP requires at least 12 bytes and version field must be 2
        if packet.len() < 12 || (packet[0] >> 6) != 2 {
            return packet;
        }

        let cc = (packet[0] & 0x0F) as usize;
        let has_extension = (packet[0] & 0x10) != 0;
        let has_padding = (packet[0] & 0x20) != 0;

        let mut offset = 12 + cc * 4; // Fixed header + CSRC list

        // Skip RTP header extension
        if has_extension && offset + 4 <= packet.len() {
            let ext_len =
                ((packet[offset + 2] as usize) << 8 | (packet[offset + 3] as usize)) * 4;
            offset += 4 + ext_len;
        }

        if offset >= packet.len() {
            return &[];
        }

        let mut end = packet.len();

        // Remove RTP padding
        if has_padding {
            let padding_len = packet[end - 1] as usize;
            if padding_len <= end - offset {
                end -= padding_len;
            }
        }

        &packet[offset..end]
    }
}
