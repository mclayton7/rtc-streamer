use crate::error::{Result, StreamError};
use crate::media::{H264Parser, MediaPipeline};
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use webrtc::rtp::header::Header;
use webrtc::rtp::packet::Packet as RtpPacket;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocalWriter;

const MTU: usize = 1200; // Conservative MTU for RTP
const NALU_TYPE_FU_A: u8 = 28;
const MAX_CONSECUTIVE_ERRORS: u32 = 5;

pub struct TrackSender {
    session_id: String,
    track: Arc<TrackLocalStaticRTP>,
    pipeline: Arc<MediaPipeline>,
}

impl TrackSender {
    pub fn new(
        session_id: String,
        track: Arc<TrackLocalStaticRTP>,
        pipeline: Arc<MediaPipeline>,
    ) -> Self {
        Self {
            session_id,
            track,
            pipeline,
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut video_rx = self.pipeline.subscribe_video();
        let track = self.track.clone();
        let session_id = self.session_id.clone();
        let h264_parser = self.pipeline.h264_parser();

        tokio::spawn(async move {
            info!("Track sender started for session: {}", session_id);
            let mut sequence_number: u16 = 0;
            let mut frame_count = 0u64;
            let mut waiting_for_keyframe = false;
            let mut consecutive_errors: u32 = 0;

            loop {
                match video_rx.recv().await {
                    Ok(frame) => {
                        // After lag, skip non-keyframes to avoid decoder corruption
                        if waiting_for_keyframe {
                            if !frame.is_keyframe {
                                continue;
                            }
                            waiting_for_keyframe = false;
                            info!("Recovered: found keyframe for session {}", session_id);
                        }

                        frame_count += 1;

                        if frame_count % 100 == 0 {
                            debug!(
                                "Sent {} frames to session: {}",
                                frame_count, session_id
                            );
                        }

                        // Extract H.264 parameter sets on keyframes
                        if frame.is_keyframe {
                            h264_parser.update_params(&frame.data);
                        }

                        match Self::send_h264_frame(
                            &track,
                            &frame.data,
                            frame.timestamp,
                            frame.is_keyframe,
                            &mut sequence_number,
                            &h264_parser,
                        )
                        .await
                        {
                            Ok(()) => {
                                consecutive_errors = 0;
                            }
                            Err(e) => {
                                consecutive_errors += 1;
                                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                    error!(
                                        "Too many consecutive send errors for session {} ({}), closing: {}",
                                        session_id, consecutive_errors, e
                                    );
                                    break;
                                }
                                warn!(
                                    "Transient send error for session {} (attempt {}/{}): {}",
                                    session_id, consecutive_errors, MAX_CONSECUTIVE_ERRORS, e
                                );
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(
                            "Session {} lagged by {} frames, seeking to next keyframe",
                            session_id, n
                        );
                        waiting_for_keyframe = true;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Video channel closed for session: {}", session_id);
                        break;
                    }
                }
            }

            info!("Track sender stopped for session: {}", session_id);
        });

        Ok(())
    }

    async fn send_h264_frame(
        track: &Arc<TrackLocalStaticRTP>,
        data: &[u8],
        timestamp: u64,
        is_keyframe: bool,
        sequence_number: &mut u16,
        h264_parser: &H264Parser,
    ) -> Result<()> {
        let nals = H264Parser::find_nal_units(data);

        // Inject SPS/PPS before keyframes
        if is_keyframe {
            if let Some(params) = h264_parser.get_params() {
                Self::send_nal_unit(track, &params.sps, timestamp, sequence_number, false).await?;
                Self::send_nal_unit(track, &params.pps, timestamp, sequence_number, false).await?;
            }
        }

        let nal_count = nals.len();
        for (i, nal) in nals.iter().enumerate() {
            if nal.is_empty() {
                continue;
            }
            let is_last = i == nal_count - 1;
            Self::send_nal_unit(track, nal, timestamp, sequence_number, is_last).await?;
        }

        Ok(())
    }

    async fn send_nal_unit(
        track: &Arc<TrackLocalStaticRTP>,
        nal: &[u8],
        timestamp: u64,
        sequence_number: &mut u16,
        is_last_nal_in_au: bool,
    ) -> Result<()> {
        if nal.is_empty() {
            return Ok(());
        }

        // PTS is already in 90kHz clock units — use directly as RTP timestamp
        let rtp_timestamp = timestamp as u32;

        if nal.len() <= MTU {
            // Single NAL unit mode
            let rtp_packet = RtpPacket {
                header: Header {
                    version: 2,
                    marker: is_last_nal_in_au,
                    payload_type: 96,
                    sequence_number: *sequence_number,
                    timestamp: rtp_timestamp,
                    ..Default::default()
                },
                payload: Bytes::copy_from_slice(nal),
            };

            track
                .write_rtp(&rtp_packet)
                .await
                .map_err(StreamError::WebRtc)?;

            *sequence_number = sequence_number.wrapping_add(1);
        } else {
            // FU-A fragmentation for NAL units exceeding MTU
            let nal_header = nal[0];
            let nal_type = nal_header & 0x1F;
            let nri = nal_header & 0x60;

            let data = &nal[1..];
            let mut offset = 0;

            while offset < data.len() {
                let end = std::cmp::min(offset + MTU - 2, data.len());
                let is_first = offset == 0;
                let is_last_fragment = end >= data.len();

                // FU indicator: NRI from original header + FU-A type
                let fu_indicator = nri | NALU_TYPE_FU_A;

                // FU header: start/end bits + original NAL type
                let mut fu_header = nal_type;
                if is_first {
                    fu_header |= 0x80; // Start bit
                }
                if is_last_fragment {
                    fu_header |= 0x40; // End bit
                }

                let mut packet_payload = Vec::with_capacity(2 + (end - offset));
                packet_payload.push(fu_indicator);
                packet_payload.push(fu_header);
                packet_payload.extend_from_slice(&data[offset..end]);

                // Marker bit only on the last fragment of the last NAL in the access unit
                let marker = is_last_fragment && is_last_nal_in_au;

                let rtp_packet = RtpPacket {
                    header: Header {
                        version: 2,
                        marker,
                        payload_type: 96,
                        sequence_number: *sequence_number,
                        timestamp: rtp_timestamp,
                        ..Default::default()
                    },
                    payload: packet_payload.into(),
                };

                track
                    .write_rtp(&rtp_packet)
                    .await
                    .map_err(StreamError::WebRtc)?;

                *sequence_number = sequence_number.wrapping_add(1);
                offset = end;
            }
        }

        Ok(())
    }
}
