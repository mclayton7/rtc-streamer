use crate::error::Result;
use crate::media::{AudioFrame, H264Parser, VideoFrame};
use crate::monitoring::Metrics;
use bytes::{Buf, Bytes, BytesMut};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

const MPEG_TS_PACKET_SIZE: usize = 188;
const SYNC_BYTE: u8 = 0x47;
const PES_MAX_SIZE: usize = 1024 * 1024; // 1MB cap to prevent memory exhaustion
const PID_PAT: u16 = 0;

/// State machine for PID discovery before streaming begins.
#[derive(Debug)]
enum DemuxState {
    /// Waiting to parse the PAT (Program Association Table) at PID 0.
    NeedPAT,
    /// PAT found; waiting to parse the PMT (Program Map Table) at this PID.
    NeedPMT { pmt_pid: u16 },
    /// Video PID discovered; actively demuxing video frames.
    Streaming { video_pid: u16 },
}

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
        let mut state = DemuxState::NeedPAT;

        info!("MPEG-TS demuxer started, waiting for PAT");

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
                    if offset >= packet_data.len() {
                        continue;
                    }
                    let adaptation_len = packet_data[offset] as usize;
                    offset += 1 + adaptation_len;
                }

                if offset >= packet_data.len() {
                    continue;
                }

                let ts_payload = &packet_data[offset..];

                match &state {
                    DemuxState::NeedPAT => {
                        // PAT/PMT sections always begin at payload_unit_start_indicator; skip continuations
                        if pid == PID_PAT && payload_start {
                            if let Some(pmt_pid) = Self::parse_pat(ts_payload) {
                                info!("PAT parsed, found PMT PID: {}", pmt_pid);
                                state = DemuxState::NeedPMT { pmt_pid };
                            }
                        }
                    }
                    DemuxState::NeedPMT { pmt_pid } => {
                        let pmt_pid = *pmt_pid;
                        if pid == pmt_pid && payload_start {
                            if let Some(video_pid) = Self::parse_pmt(ts_payload) {
                                info!("PMT parsed, streaming video from PID: {}", video_pid);
                                state = DemuxState::Streaming { video_pid };
                            }
                        }
                    }
                    DemuxState::Streaming { video_pid } => {
                        let video_pid = *video_pid;
                        if pid == video_pid {
                            if payload_start && !video_pes_buffer.is_empty() {
                                // Flush completed PES unit before starting a new one
                                self.process_video_pes(&video_pes_buffer, &mut fallback_pts);
                                video_pes_buffer.clear();
                            }

                            // Guard against memory exhaustion from malformed streams
                            if video_pes_buffer.len() + ts_payload.len() > PES_MAX_SIZE {
                                warn!(
                                    "PES buffer exceeded {} bytes; discarding and resetting",
                                    PES_MAX_SIZE
                                );
                                video_pes_buffer.clear();
                            } else {
                                video_pes_buffer.extend_from_slice(ts_payload);
                            }
                        }
                    }
                }
            }
        }

        // Flush any remaining PES data when the input stream ends
        if !video_pes_buffer.is_empty() {
            self.process_video_pes(&video_pes_buffer, &mut fallback_pts);
        }

        info!("MPEG-TS demuxer stopped after {} packets", packet_count);
        Ok(())
    }

    /// Parse a PAT section (PID 0) to extract the first program's PMT PID.
    ///
    /// The payload begins with a pointer field (1 byte) when `payload_unit_start_indicator` is set.
    fn parse_pat(payload: &[u8]) -> Option<u16> {
        if payload.is_empty() {
            return None;
        }
        let pointer = payload[0] as usize;
        // Need: pointer(1) + table_id(1) + section_length(2) + ts_id(2) + version_etc(3) = 9 + pointer
        if 1 + pointer + 8 > payload.len() {
            return None;
        }
        let table = &payload[1 + pointer..];

        if table[0] != 0x00 {
            // Not a PAT table_id
            return None;
        }

        let section_length = (((table[1] & 0x0F) as usize) << 8) | table[2] as usize;
        // Program entries start at byte 8 of table; CRC occupies the last 4 bytes of the section
        let programs_end = std::cmp::min(3 + section_length.saturating_sub(4), table.len());

        let mut i = 8;
        while i + 4 <= programs_end {
            let program_num = ((table[i] as u16) << 8) | table[i + 1] as u16;
            let pid = (((table[i + 2] & 0x1F) as u16) << 8) | table[i + 3] as u16;
            if program_num != 0 {
                // program_num 0 is the NIT pointer; return first real program's PMT PID
                return Some(pid);
            }
            i += 4;
        }

        None
    }

    /// Parse a PMT section to extract the first video elementary stream PID.
    ///
    /// Recognises H.264 (0x1B), H.265 (0x24), and MPEG-1/2 video (0x01/0x02).
    fn parse_pmt(payload: &[u8]) -> Option<u16> {
        if payload.is_empty() {
            return None;
        }
        let pointer = payload[0] as usize;
        // Need: pointer(1) + table_id(1) + section_length(2) + program_number(2) + version_etc(3) + pcr_pid(2) + program_info_length(2) = 13 + pointer
        if 1 + pointer + 12 > payload.len() {
            return None;
        }
        let table = &payload[1 + pointer..];

        if table[0] != 0x02 {
            // Not a PMT table_id
            return None;
        }

        let section_length = (((table[1] & 0x0F) as usize) << 8) | table[2] as usize;
        let program_info_length = (((table[10] & 0x0F) as usize) << 8) | table[11] as usize;

        let mut i = 12 + program_info_length;
        let streams_end = std::cmp::min(3 + section_length.saturating_sub(4), table.len());

        while i + 5 <= streams_end {
            let stream_type = table[i];
            let elementary_pid = (((table[i + 1] & 0x1F) as u16) << 8) | table[i + 2] as u16;
            let es_info_length = (((table[i + 3] & 0x0F) as usize) << 8) | table[i + 4] as usize;

            if matches!(stream_type, 0x01 | 0x02 | 0x1B | 0x24) {
                return Some(elementary_pid);
            }

            i += 5 + es_info_length;
        }

        None
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
            // Fallback: synthesise PTS at ~30fps (3000 ticks in 90kHz clock)
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
            warn!("Frame dropped — no active video subscribers: {}", e);
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
    /// Passes through unchanged if the packet does not look like RTP.
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
        if has_extension {
            if offset + 4 > packet.len() {
                return &[];
            }
            let ext_len =
                ((packet[offset + 2] as usize) << 8 | packet[offset + 3] as usize) * 4;
            // Bounds check: guard against malformed extension length field
            if offset + 4 + ext_len > packet.len() {
                return &[];
            }
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
