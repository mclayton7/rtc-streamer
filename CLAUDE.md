# RTC Streamer - Project Context for Claude

## Project Overview

**MPEG-TS to WebRTC Streaming Service** - A high-performance Rust application that receives MPEG-TS video streams over UDP/RTP and delivers them to web browsers via WebRTC with ultra-low latency (<500ms).

### Purpose
Enable sub-500ms glass-to-glass video streaming to web browsers without plugins. Designed for 1-10 concurrent viewers with focus on efficiency and minimal processing overhead.

### Status: ✅ COMPLETE & READY FOR TESTING

All planned phases implemented. Project compiles successfully in both debug and release modes. Ready for real-world testing with MPEG-TS streams.

## Architecture

```
UDP/RTP MPEG-TS → Demuxer → Frame Queue → WebRTC Broadcast → Multiple Browsers
                                ↓
                          Signaling Server (WebSocket)
```

### Data Flow
1. **UDP Receiver** (port 5004) receives packets
2. **RTP Depacketizer** extracts MPEG-TS payload (handles both RTP and raw)
3. **MPEG-TS Demuxer** parses transport stream and extracts H.264 frames
4. **Media Pipeline** broadcasts frames to all connected viewers via `tokio::sync::broadcast`
5. **WebRTC Peers** packetize H.264 into RTP and send to browsers
6. **Signaling Server** handles WebSocket SDP exchange

## Key Implementation Decisions

### 1. Simplified MPEG-TS Demuxer
- **What**: Custom parser instead of using `mpeg2ts-reader` library
- **Why**: Library API changes made integration complex; simplified parser meets MVP needs
- **Trade-off**: Currently assumes video on PID 256 (not dynamic PAT/PMT parsing)
- **Location**: `src/ingest/mpegts_demuxer.rs`
- **Future**: Can add full PAT/PMT parsing for production

### 2. Audio Deferred
- **What**: Audio transcoding (AAC→Opus) not implemented
- **Why**: `opus` crate has CMake build dependency issues
- **Impact**: Video-only streaming currently
- **Placeholder**: `src/media/audio_transcoder.rs` (stub implementation)
- **Future**: Uncomment dependencies in Cargo.toml and implement when needed

### 3. Manual RTP Packetization
- **What**: Direct RTP packet construction in `src/webrtc/track_sender.rs`
- **Why**: Full control over packet structure and timing for low latency
- **Features**:
  - FU-A fragmentation for NAL units > MTU (1200 bytes)
  - SPS/PPS injection before keyframes
  - Proper marker bit handling

### 4. Broadcast Architecture
- **What**: Single demuxer → broadcast channel → multiple viewers
- **Why**: Extremely efficient - decode once, distribute to all
- **Benefit**: Can handle 10+ viewers with minimal CPU overhead
- **Trade-off**: All viewers get same stream (no per-viewer customization)

## Project Structure

```
rtc-streamer/
├── Cargo.toml              # Dependencies (webrtc, axum, tokio, bytes, etc.)
├── config.toml             # Runtime config (ports, buffer size, max viewers)
├── README.md               # User documentation
├── IMPLEMENTATION.md       # Detailed implementation notes
├── CLAUDE.md              # This file
│
├── src/
│   ├── main.rs            # Entry point - wires all components together
│   ├── config.rs          # Config loading and structs
│   ├── error.rs           # Unified error types
│   │
│   ├── ingest/            # INPUT PIPELINE
│   │   ├── mod.rs
│   │   ├── udp_receiver.rs       # Binds UDP socket, receives packets
│   │   ├── rtp_depacketizer.rs   # Extracts RTP payload
│   │   └── mpegts_demuxer.rs     # Parses MPEG-TS, extracts H.264 frames
│   │
│   ├── media/             # FRAME HANDLING
│   │   ├── mod.rs
│   │   ├── frame.rs              # VideoFrame and AudioFrame structs
│   │   ├── h264_parser.rs        # NAL unit parsing, SPS/PPS extraction
│   │   ├── pipeline.rs           # Broadcast hub (tokio::sync::broadcast)
│   │   └── audio_transcoder.rs   # Placeholder for AAC→Opus (not impl)
│   │
│   ├── webrtc/            # OUTPUT PIPELINE
│   │   ├── mod.rs
│   │   ├── session_manager.rs    # Manages peer connections, enforces max viewers
│   │   ├── peer.rs               # RTCPeerConnection wrapper
│   │   └── track_sender.rs       # RTP packetization, FU-A fragmentation
│   │
│   ├── signaling/         # WEBSOCKET SERVER
│   │   ├── mod.rs
│   │   ├── server.rs             # Axum HTTP/WebSocket server
│   │   ├── messages.rs           # JSON message types for SDP exchange
│   │   └── handlers.rs           # WebSocket message handlers
│   │
│   └── monitoring/        # METRICS
│       ├── mod.rs
│       └── metrics.rs            # Performance tracking
│
└── static/                # WEB CLIENT
    ├── index.html         # Player UI
    ├── player.js          # WebRTC client logic
    └── style.css          # Modern styling
```

## Critical Files to Understand

### `src/main.rs` (151 lines)
Entry point. Creates pipeline: UDP → RTP → Demuxer → Broadcast → WebRTC.
- Loads config
- Creates media pipeline
- Spawns ingest tasks
- Starts signaling server

### `src/media/pipeline.rs` (45 lines)
The heart of the system. Uses `tokio::sync::broadcast` to distribute frames.
- `video_sender()` - Get sender for demuxer
- `subscribe_video()` - Get receiver for viewers
- `h264_parser()` - Access shared H.264 parameter cache

### `src/webrtc/track_sender.rs` (230 lines)
Most complex file. Handles RTP packetization:
- Reads from broadcast channel
- Splits H.264 frames into NAL units
- Injects SPS/PPS before keyframes
- Fragments large NALs using FU-A
- Writes RTP packets to WebRTC track

### `src/ingest/mpegts_demuxer.rs` (155 lines)
Simplified MPEG-TS parser:
- Looks for sync byte (0x47)
- Extracts PID from packet header
- Assumes video on PID 256 ⚠️
- Detects keyframes by looking for NAL type 5 (IDR)

### `static/player.js` (230 lines)
WebRTC browser client:
- Connects WebSocket to `/signal`
- Handles SDP offer/answer
- Manages ICE candidates
- Displays video and stats

## Configuration

### `config.toml`
```toml
[network]
udp_bind = "0.0.0.0:5004"      # Where to receive MPEG-TS

[media]
max_buffer_frames = 10          # Broadcast channel size (balance latency vs stability)
target_latency_ms = 100         # Target latency (informational)

[webrtc]
max_viewers = 10                # Concurrent viewer limit
stun_servers = ["stun:stun.l.google.com:19302"]

[signaling]
http_bind = "0.0.0.0:8080"      # Where to serve web UI
static_dir = "./static"         # Web files location
```

### Environment Variables
- `RUST_LOG=debug` - Enable debug logging (use `info` for production)

## How to Build & Run

### Development
```bash
cargo build
RUST_LOG=info cargo run
```

### Production
```bash
cargo build --release
./target/release/rtc-streamer
```

The release build is **heavily optimized** (LTO enabled, opt-level 3).

## Testing

### 1. Start Server
```bash
cargo run --release
```

Look for: `Ready to receive streams!`

### 2. Send Test Stream

**From file:**
```bash
ffmpeg -re -i video.mp4 -c:v libx264 -preset ultrafast -tune zerolatency \
  -profile:v baseline -pix_fmt yuv420p -f mpegts udp://127.0.0.1:5004
```

**From webcam (macOS):**
```bash
ffmpeg -f avfoundation -i "0" -c:v libx264 -preset ultrafast -tune zerolatency \
  -profile:v baseline -pix_fmt yuv420p -f mpegts udp://127.0.0.1:5004
```

**Critical ffmpeg flags:**
- `-tune zerolatency` - Essential for low latency
- `-profile:v baseline` - Maximum browser compatibility
- `-f mpegts` - Output format

### 3. Open Browser
Navigate to `http://localhost:8080` and click "Connect"

### 4. Verify Latency
Add timestamp to stream:
```bash
ffmpeg -re -i video.mp4 \
  -vf "drawtext=fontfile=/System/Library/Fonts/Supplemental/Arial.ttf:text='%{localtime\:%T}':x=10:y=10:fontsize=48:fontcolor=white" \
  -c:v libx264 -preset ultrafast -tune zerolatency \
  -f mpegts udp://127.0.0.1:5004
```

Compare displayed time with system clock. Should be <500ms.

## Known Issues & Limitations

### 1. Audio Not Implemented ⚠️
- **Status**: Placeholder exists but not functional
- **Workaround**: Video-only streaming
- **Fix**: Uncomment `symphonia` and `opus` in Cargo.toml, implement transcoding

### 2. Fixed Video PID ⚠️
- **Issue**: Assumes video on PID 256 (see `mpegts_demuxer.rs:82`)
- **Impact**: Won't work with streams using different PIDs
- **Fix**: Add PAT/PMT parsing to discover PIDs dynamically
- **Location**: `src/ingest/mpegts_demuxer.rs` - add PAT (PID 0) and PMT parsing

### 3. No TURN Server
- **Issue**: May not work through strict NATs
- **Impact**: Some networks may block connections
- **Fix**: Add TURN server configuration to `config.toml` and `session_manager.rs`

### 4. H.264 Only
- **Issue**: No H.265/VP9/AV1 support
- **Impact**: Limited to H.264 streams
- **Fix**: Add codec detection and additional RTP packetizers

## Common Debugging Scenarios

### "No video appears in browser"

1. **Check UDP stream is arriving:**
   ```bash
   netstat -an | grep 5004
   ```

2. **Enable debug logging:**
   ```bash
   RUST_LOG=debug cargo run
   ```

3. **Look for these log messages:**
   - `UDP socket bound successfully`
   - `Processed X MPEG-TS packets`
   - `WebSocket connection established`
   - `Streaming started for session`

4. **Check browser console:**
   - Open DevTools → Console
   - Look for WebRTC errors
   - Check Network tab for WebSocket connection

### "High latency (>1 second)"

1. **Check ffmpeg flags:**
   - Must include `-tune zerolatency`
   - Use `-preset ultrafast` or `-preset veryfast`

2. **Reduce buffer size:**
   - Edit `config.toml`: `max_buffer_frames = 5`

3. **Check network:**
   - Ensure no packet loss (`dmesg` on Linux, Console.app on macOS)

### "Connection fails immediately"

1. **STUN server issue:**
   - Check firewall allows UDP to stun.l.google.com:19302
   - Try alternative STUN server in `config.toml`

2. **Browser compatibility:**
   - Test with Chrome or Firefox first
   - Safari has stricter WebRTC requirements

## Future Enhancements (Priority Order)

### High Priority
1. **Audio Support** - Implement AAC→Opus transcoding
2. **Dynamic PID Discovery** - Parse PAT/PMT tables
3. **Error Recovery** - Handle stream interruptions gracefully
4. **TURN Support** - Better NAT traversal

### Medium Priority
5. **H.265 Support** - Add HEVC codec
6. **Recording** - DVR functionality
7. **Authentication** - Secure viewer access
8. **Multiple Streams** - Support multiple input streams

### Low Priority
9. **Adaptive Bitrate** - Multiple quality levels
10. **Load Balancing** - Edge server distribution
11. **Advanced Stats** - Detailed WebRTC metrics in UI

## Development Tips

### Adding New Features

1. **New endpoint**: Add route in `src/signaling/server.rs`
2. **New config option**: Update `src/config.rs` and `config.toml`
3. **New codec**: Extend `src/webrtc/track_sender.rs`
4. **New metric**: Add to `src/monitoring/metrics.rs`

### Code Style
- Use `tracing::info!()` for important events
- Use `tracing::debug!()` for verbose logging
- Use `tracing::warn!()` for recoverable errors
- Use `tracing::error!()` for critical failures

### Testing Approach
1. Unit tests for parsers (H.264, MPEG-TS)
2. Integration tests with sample files
3. Load testing with multiple browsers
4. Latency measurement with timestamps

## Performance Characteristics

### Expected Metrics (10 viewers)
- **CPU**: 20-40% on modern CPU
- **Memory**: 100-150MB total
- **Latency**: 200-400ms end-to-end
- **Throughput**: Input bitrate × viewer count

### Optimization Points
- Buffer size: `config.toml` → `max_buffer_frames`
- Broadcast channel: `src/media/pipeline.rs:19`
- RTP MTU: `src/webrtc/track_sender.rs:10` (currently 1200)

## Important Constants

```rust
// MPEG-TS
MPEG_TS_PACKET_SIZE = 188   // Standard transport packet size
SYNC_BYTE = 0x47            // MPEG-TS sync byte
VIDEO_PID = 256             // Assumed video PID ⚠️

// RTP
MTU = 1200                  // Conservative MTU for fragmentation
PAYLOAD_TYPE = 96           // H.264 RTP payload type

// NAL Units
NALU_TYPE_SPS = 7           // Sequence Parameter Set
NALU_TYPE_PPS = 8           // Picture Parameter Set
NALU_TYPE_IDR = 5           // Keyframe (Instantaneous Decoder Refresh)
NALU_TYPE_FU_A = 28         // Fragmentation Unit
```

## API Endpoints

- `GET /` → Serve index.html (web player)
- `GET /signal` → WebSocket upgrade (signaling)
- `GET /api/health` → Health check ("OK")
- `GET /api/stats` → JSON with active viewer count

## Dependencies

### Core
- `webrtc = "0.11"` - Pure Rust WebRTC implementation
- `tokio = "1.41"` - Async runtime
- `bytes = "1.8"` - Zero-copy buffer management

### HTTP/WebSocket
- `axum = "0.7"` - Web framework
- `tower-http = "0.5"` - Static file serving

### Utilities
- `serde`, `serde_json` - Serialization
- `tracing`, `tracing-subscriber` - Logging
- `uuid = "1.11"` - Session IDs
- `futures = "0.3"` - Async utilities

## Quick Reference Commands

```bash
# Build
cargo build --release

# Run with logging
RUST_LOG=info cargo run --release

# Test with ffmpeg
ffmpeg -re -i video.mp4 -c:v libx264 -tune zerolatency -f mpegts udp://127.0.0.1:5004

# Check UDP port
netstat -an | grep 5004

# Check HTTP port
curl http://localhost:8080/api/health

# Monitor logs
tail -f /path/to/logfile  # if you add file logging

# Build size
ls -lh target/release/rtc-streamer
```

## Project Health

- ✅ Compiles without errors
- ✅ All planned features implemented
- ✅ Release build optimized
- ✅ Documentation complete
- ⏱️ Pending: Real-world testing
- ⏱️ Pending: Performance validation

## Contact Points for Issues

### If video doesn't play:
1. Check `src/ingest/mpegts_demuxer.rs` - PID detection
2. Check `src/webrtc/track_sender.rs` - RTP packetization
3. Check browser console - WebRTC errors

### If latency is too high:
1. Verify `-tune zerolatency` in ffmpeg
2. Reduce `max_buffer_frames` in config.toml
3. Check network conditions

### If connection fails:
1. Check `src/signaling/handlers.rs` - SDP exchange
2. Verify STUN server accessibility
3. Check firewall rules

---

**Last Updated**: 2026-02-13
**Status**: Ready for Testing
**Next Steps**: Test with real MPEG-TS streams, measure latency, validate performance
