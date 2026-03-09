use crate::error::Result;
use crate::media::klv_parser;
use crate::media::{H264Parser, MetadataFrame, VideoFrame};
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
    /// Video (and optional KLV metadata) PID(s) discovered; actively demuxing.
    Streaming {
        video_pid: u16,
        klv_pid: Option<u16>,
    },
}

pub struct MpegTsDemuxer {
    video_tx: broadcast::Sender<VideoFrame>,
    metadata_tx: broadcast::Sender<MetadataFrame>,
    metrics: Arc<Metrics>,
}

impl MpegTsDemuxer {
    pub fn new(
        video_tx: broadcast::Sender<VideoFrame>,
        metadata_tx: broadcast::Sender<MetadataFrame>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            video_tx,
            metadata_tx,
            metrics,
        }
    }

    pub async fn start(self, mut rx: mpsc::Receiver<Bytes>) -> Result<()> {
        let mut buffer = BytesMut::new();
        let mut packet_count = 0u64;
        let mut video_pes_buffer = BytesMut::new();
        let mut klv_pes_buffer = BytesMut::new();
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
                            if let Some((video_pid, klv_pid)) = Self::parse_pmt(ts_payload) {
                                if let Some(kpid) = klv_pid {
                                    info!(
                                        "PMT parsed, streaming video from PID: {}, KLV from PID: {}",
                                        video_pid, kpid
                                    );
                                } else {
                                    info!("PMT parsed, streaming video from PID: {} (no KLV PID found)", video_pid);
                                }
                                state = DemuxState::Streaming { video_pid, klv_pid };
                            }
                        }
                    }
                    DemuxState::Streaming { video_pid, klv_pid } => {
                        let video_pid = *video_pid;
                        let klv_pid = *klv_pid;

                        if pid == video_pid {
                            if payload_start && !video_pes_buffer.is_empty() {
                                // Flush completed PES unit before starting a new one
                                self.process_video_pes(&video_pes_buffer, &mut fallback_pts);
                                video_pes_buffer.clear();
                            }

                            // Guard against memory exhaustion from malformed streams
                            if video_pes_buffer.len() + ts_payload.len() > PES_MAX_SIZE {
                                warn!(
                                    "Video PES buffer exceeded {} bytes; discarding and resetting",
                                    PES_MAX_SIZE
                                );
                                video_pes_buffer.clear();
                            } else {
                                video_pes_buffer.extend_from_slice(ts_payload);
                            }
                        } else if Some(pid) == klv_pid {
                            if payload_start && !klv_pes_buffer.is_empty() {
                                self.process_klv_pes(&klv_pes_buffer);
                                klv_pes_buffer.clear();
                            }

                            if klv_pes_buffer.len() + ts_payload.len() > PES_MAX_SIZE {
                                warn!(
                                    "KLV PES buffer exceeded {} bytes; discarding and resetting",
                                    PES_MAX_SIZE
                                );
                                klv_pes_buffer.clear();
                            } else {
                                klv_pes_buffer.extend_from_slice(ts_payload);
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
        if !klv_pes_buffer.is_empty() {
            self.process_klv_pes(&klv_pes_buffer);
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

    /// Parse a PMT section to extract the video PID and optional KLV metadata PID.
    ///
    /// Returns `Some((video_pid, klv_pid))` where `klv_pid` is `Some` if a private/metadata
    /// stream (`0x06` or `0x15`) was also found alongside the video stream.
    /// Returns `None` if no video stream was found.
    fn parse_pmt(payload: &[u8]) -> Option<(u16, Option<u16>)> {
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

        let mut video_pid: Option<u16> = None;
        let mut klv_pid: Option<u16> = None;

        while i + 5 <= streams_end {
            let stream_type = table[i];
            let elementary_pid = (((table[i + 1] & 0x1F) as u16) << 8) | table[i + 2] as u16;
            let es_info_length = (((table[i + 3] & 0x0F) as usize) << 8) | table[i + 4] as usize;

            match stream_type {
                // H.264, H.265, MPEG-1/2 video
                0x01 | 0x02 | 0x1B | 0x24 => {
                    if video_pid.is_none() {
                        video_pid = Some(elementary_pid);
                    }
                }
                // Private data PES (0x06) or Metadata PES (0x15) — used for MISB KLV
                0x06 | 0x15 => {
                    if klv_pid.is_none() {
                        klv_pid = Some(elementary_pid);
                    }
                }
                _ => {}
            }

            i += 5 + es_info_length;
        }

        video_pid.map(|vpid| (vpid, klv_pid))
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
            is_keyframe,
        };

        if let Err(e) = self.video_tx.send(frame) {
            warn!("Frame dropped — no active video subscribers: {}", e);
        }
    }

    fn process_klv_pes(&self, data: &[u8]) {
        // Minimum PES header: start code (3) + stream_id (1) + length (2) + flags (3) = 9 bytes
        if data.len() < 9 {
            return;
        }

        // Verify PES start code prefix
        if data[0..3] != [0, 0, 1] {
            return;
        }

        let pes_header_data_len = data[8] as usize;
        let payload_start = 9 + pes_header_data_len;

        if payload_start >= data.len() {
            return;
        }

        // Extract PTS for metadata timestamp (microseconds)
        let pts_dts_flags = (data[7] >> 6) & 0x03;
        let timestamp_us = if pts_dts_flags >= 2 && data.len() >= 14 {
            // Convert 90kHz PTS to microseconds
            Self::extract_pts(&data[9..14]) * 1_000_000 / 90_000
        } else {
            0
        };

        let klv_data = &data[payload_start..];
        if klv_data.is_empty() {
            return;
        }

        if let Some(fields) = klv_parser::parse_klv_packet(klv_data) {
            let frame = MetadataFrame {
                timestamp: timestamp_us,
                fields: Arc::new(fields),
            };
            if let Err(e) = self.metadata_tx.send(frame) {
                debug!("KLV metadata dropped — no active subscribers: {}", e);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- PAT ----

    fn make_pat(program_num: u16, pmt_pid: u16) -> Vec<u8> {
        // pointer=0, table_id=0x00, section_length=13
        // ts_id(2) + version(1) + sec_num(1) + last_sec_num(1) = 5 bytes header after length
        // 1 program entry = 4 bytes; CRC placeholder = 4 bytes → total section_length = 5+4+4 = 13
        let pmt_high = 0xE0 | ((pmt_pid >> 8) as u8);
        let pmt_low  = (pmt_pid & 0xFF) as u8;
        let prog_high = (program_num >> 8) as u8;
        let prog_low  = (program_num & 0xFF) as u8;
        vec![
            0x00,             // pointer
            0x00,             // table_id = PAT
            0xB0, 0x0D,       // section_length = 13
            0x00, 0x01,       // transport_stream_id
            0xC1,             // version + current_next
            0x00, 0x00,       // section / last section numbers
            prog_high, prog_low, pmt_high, pmt_low, // program entry
            0x00, 0x00, 0x00, 0x00, // CRC placeholder
        ]
    }

    #[test]
    fn parse_pat_valid() {
        let pat = make_pat(1, 0x1234);
        assert_eq!(MpegTsDemuxer::parse_pat(&pat), Some(0x1234));
    }

    #[test]
    fn parse_pat_nit_only() {
        // program_number=0 is the NIT pointer; should be skipped
        let pat = make_pat(0, 0x0010);
        assert_eq!(MpegTsDemuxer::parse_pat(&pat), None);
    }

    #[test]
    fn parse_pat_too_short() {
        assert_eq!(MpegTsDemuxer::parse_pat(&[]), None);
        assert_eq!(MpegTsDemuxer::parse_pat(&[0x00, 0x00, 0xB0]), None);
    }

    // ---- PMT ----

    fn make_pmt(streams: &[(u8, u16)]) -> Vec<u8> {
        // Build stream entries (5 bytes each, es_info_length=0)
        let mut entries: Vec<u8> = Vec::new();
        for &(stype, epid) in streams {
            let pid_high = 0xE0 | ((epid >> 8) as u8);
            let pid_low  = (epid & 0xFF) as u8;
            entries.extend_from_slice(&[stype, pid_high, pid_low, 0xF0, 0x00]);
        }
        // section_length = 5 (ts_id/ver/sec) + 4 (pcr_pid + prog_info_len) + entries.len() + 4 (CRC)
        let section_length = 9 + entries.len() + 4;
        let mut pmt = vec![
            0x00,             // pointer
            0x02,             // table_id = PMT
            0xB0, section_length as u8, // section_length
            0x00, 0x01,       // program_number
            0xC1,             // version + current_next
            0x00, 0x00,       // section / last section numbers
            0xE1, 0x00,       // PCR_PID
            0xF0, 0x00,       // program_info_length = 0
        ];
        pmt.extend_from_slice(&entries);
        pmt.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // CRC
        pmt
    }

    #[test]
    fn parse_pmt_video_and_klv() {
        let pmt = make_pmt(&[(0x1B, 0x0100), (0x06, 0x0200)]);
        assert_eq!(MpegTsDemuxer::parse_pmt(&pmt), Some((0x0100, Some(0x0200))));
    }

    #[test]
    fn parse_pmt_video_only() {
        let pmt = make_pmt(&[(0x1B, 0x0100)]);
        assert_eq!(MpegTsDemuxer::parse_pmt(&pmt), Some((0x0100, None)));
    }

    #[test]
    fn parse_pmt_no_video() {
        let pmt = make_pmt(&[(0x06, 0x0200)]);
        assert_eq!(MpegTsDemuxer::parse_pmt(&pmt), None);
    }

    // ---- strip_rtp_header ----

    #[test]
    fn strip_rtp_header_plain_mpegts() {
        // Starts with MPEG-TS sync byte 0x47; version bits = 0x47 >> 6 = 1 ≠ 2 → pass through
        let pkt: Vec<u8> = std::iter::once(0x47u8).chain(vec![0xAB; 20]).collect();
        assert_eq!(MpegTsDemuxer::strip_rtp_header(&pkt), pkt.as_slice());
    }

    #[test]
    fn strip_rtp_header_rtp_wrapped() {
        // 12-byte RTP header (version=2, CC=0, no extension) followed by MPEG-TS
        let mut pkt = vec![
            0x80, 0x60, 0x00, 0x01, // V=2, P=0, X=0, CC=0; M=0, PT=96; seq=1
            0x00, 0x00, 0x03, 0xE8, // timestamp
            0x12, 0x34, 0x56, 0x78, // SSRC
        ];
        pkt.push(0x47); // MPEG-TS sync byte
        pkt.extend_from_slice(&[0x00; 20]);
        let result = MpegTsDemuxer::strip_rtp_header(&pkt);
        assert_eq!(result[0], 0x47);
        assert_eq!(result.as_ptr(), pkt[12..].as_ptr());
    }

    #[test]
    fn strip_rtp_header_with_csrc() {
        // version=2, CC=2 → header = 12 + 2*4 = 20 bytes
        let mut pkt = vec![
            0x82, 0x60, 0x00, 0x01, // V=2, P=0, X=0, CC=2
            0x00, 0x00, 0x03, 0xE8, // timestamp
            0x12, 0x34, 0x56, 0x78, // SSRC
            0x00, 0x00, 0x00, 0x01, // CSRC 1
            0x00, 0x00, 0x00, 0x02, // CSRC 2
        ];
        pkt.push(0x47); // MPEG-TS sync byte
        pkt.extend_from_slice(&[0x00; 20]);
        let result = MpegTsDemuxer::strip_rtp_header(&pkt);
        assert_eq!(result[0], 0x47);
        assert_eq!(result.as_ptr(), pkt[20..].as_ptr());
    }
}
