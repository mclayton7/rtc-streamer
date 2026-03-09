# rtc-streamer-ui — Integration Guide

React hooks and components for connecting to an rtc-streamer backend over WebRTC.
Handles signaling, ICE negotiation, auto-reconnect, MISB KLV metadata, and map display.

## Building the library

```bash
# From the frontend/ directory:
npm run build:lib
```

Outputs to `frontend/dist/`:
```
dist/
  rtc-streamer.es.js    # ESM bundle
  rtc-streamer.cjs.js   # CJS bundle
  index.d.ts            # TypeScript declarations
```

## Installation

```bash
# From a local build:
npm install ../rtc-streamer/frontend

# Or after packing:
npm pack  # creates rtc-streamer-ui-0.1.0.tgz
npm install rtc-streamer-ui-0.1.0.tgz
```

**Peer dependencies** (must be installed in the consuming app):

```bash
npm install react react-dom leaflet
```

Import Leaflet CSS once near your app root:

```ts
import 'leaflet/dist/leaflet.css';
```

---

## Quick start

```tsx
import { useWebRTC, VideoPlayer } from 'rtc-streamer-ui';

export function StreamPlayer() {
  const { videoRef, statusText, statusKind, loading, streamOnline, connect, disconnect } =
    useWebRTC({ signalingUrl: 'wss://your-server/signal' });

  return (
    <>
      <div className={`status ${statusKind}`}>{statusText}</div>
      <VideoPlayer
        videoRef={videoRef}
        loading={loading}
        streamOnline={streamOnline}
        statusKind={statusKind}
      />
      <button onClick={connect}  disabled={statusKind !== 'disconnected'}>Connect</button>
      <button onClick={disconnect} disabled={statusKind === 'disconnected'}>Disconnect</button>
    </>
  );
}
```

---

## `useWebRTC(opts?)`

The main hook. Manages the full WebRTC session: WebSocket signaling, SDP exchange, ICE,
stats polling, KLV metadata, and auto-reconnect.

### Options (`UseWebRTCOptions`)

| Option | Type | Default | Description |
|---|---|---|---|
| `signalingUrl` | `string` | `ws(s)://current-host/signal` | WebSocket signaling endpoint |
| `reconnect` | `boolean` | `true` | Auto-reconnect on unexpected close |
| `reconnectInitialDelay` | `number` | `1000` | First reconnect delay in ms |
| `reconnectMaxDelay` | `number` | `30000` | Max reconnect delay in ms (exponential backoff) |
| `fallbackIceServers` | `RTCIceServer[]` | Google STUN | Used when server doesn't send a `config` message |

### Return value (`WebRTCState`)

| Field | Type | Description |
|---|---|---|
| `videoRef` | `RefObject<HTMLVideoElement \| null>` | Attach to `<video ref={videoRef}>` |
| `statusText` | `string` | Human-readable status (`"Connecting..."`, `"Connected"`, `"Reconnecting in 2s..."`, …) |
| `statusKind` | `StatusKind` | `"connected" \| "connecting" \| "disconnected" \| "error"` |
| `connState` | `string` | Raw `RTCPeerConnection.connectionState` |
| `loading` | `boolean` | True while waiting for first video frame |
| `streamOnline` | `boolean` | True when the server reports the UDP source is active |
| `fields` | `KlvFieldWithChange[]` | Latest decoded MISB KLV fields |
| `metaUpdatedAt` | `string \| null` | Locale time string of last metadata update |
| `stats` | `Stats` | `{ bitrateKbps, packetsLost, latencyMs }` — updated every second |
| `isConnected` | `boolean` | True once video is flowing |
| `connect` | `() => void` | Open the connection |
| `disconnect` | `() => void` | Close the connection |

### Using all pre-built components

```tsx
import {
  useWebRTC,
  VideoPlayer,
  StatsPanel,
  MetadataPanel,
  PlatformMap,
} from 'rtc-streamer-ui';
import 'rtc-streamer-ui/dist/rtc-streamer.es.js'; // CSS is inlined; import leaflet/dist/leaflet.css separately

export function FullPlayer() {
  const rtc = useWebRTC({ signalingUrl: 'wss://your-server/signal' });

  return (
    <>
      <VideoPlayer
        videoRef={rtc.videoRef}
        loading={rtc.loading}
        streamOnline={rtc.streamOnline}
        statusKind={rtc.statusKind}
      />
      <StatsPanel connState={rtc.connState} stats={rtc.stats} />
      <MetadataPanel fields={rtc.fields} updatedAt={rtc.metaUpdatedAt} />
      <PlatformMap fields={rtc.fields} />
    </>
  );
}
```

---

## `useSignaling(opts)`

Lower-level hook if you need WebSocket + auto-reconnect without the WebRTC layer
(e.g. custom protocol, non-video signaling).

### Options (`UseSignalingOptions`)

| Option | Type | Default | Description |
|---|---|---|---|
| `url` | `string` | — | WebSocket URL (required) |
| `reconnect` | `boolean` | `true` | Auto-reconnect on unexpected close |
| `reconnectInitialDelay` | `number` | `1000` | First reconnect delay in ms |
| `reconnectMaxDelay` | `number` | `30000` | Max reconnect delay in ms |
| `onOpen` | `() => void` | — | Called when connection opens |
| `onMessage` | `(event: MessageEvent) => void` | — | Called for each incoming message |
| `onClose` | `(userInitiated: boolean) => void` | — | Called when connection closes |
| `onReconnecting` | `(delayMs: number) => void` | — | Called just before a reconnect attempt |

### Return value (`UseSignalingReturn`)

| Field | Type | Description |
|---|---|---|
| `isOpen` | `boolean` | Whether the WebSocket is currently open |
| `send` | `(data: string \| object) => void` | Send a message (objects are JSON-serialised) |
| `connect` | `() => void` | Open the connection (resets backoff delay) |
| `disconnect` | `() => void` | Close the connection (suppresses reconnect) |

### Example

```tsx
import { useSignaling } from 'rtc-streamer-ui';

function TelemetryFeed() {
  const [messages, setMessages] = useState<string[]>([]);

  const { isOpen, send, connect, disconnect } = useSignaling({
    url: 'wss://your-server/telemetry',
    onOpen: () => send({ type: 'subscribe', channel: 'uav-1' }),
    onMessage: (e) => setMessages((prev) => [...prev, e.data as string]),
    onClose: (userInitiated) => {
      if (!userInitiated) console.log('Reconnecting...');
    },
    onReconnecting: (delay) => console.log(`Retry in ${delay}ms`),
  });

  return (
    <>
      <button onClick={isOpen ? disconnect : connect}>
        {isOpen ? 'Disconnect' : 'Connect'}
      </button>
      {messages.map((m, i) => <div key={i}>{m}</div>)}
    </>
  );
}
```

---

## Components

### `<VideoPlayer>`

| Prop | Type | Description |
|---|---|---|
| `videoRef` | `RefObject<HTMLVideoElement \| null>` | From `useWebRTC()` |
| `loading` | `boolean` | Shows a spinner overlay |
| `streamOnline` | `boolean` | Hides the "Waiting for stream source" banner when true |
| `statusKind` | `StatusKind` | Banner only shown when not `"disconnected"` |

Requires the CSS classes from `src/index.css` (or equivalent): `.video-container`, `.loading`, `.spinner`, `.stream-offline-banner`.

### `<StatsPanel>`

| Prop | Type | Description |
|---|---|---|
| `connState` | `string` | Raw RTCPeerConnection state string |
| `stats` | `Stats` | `{ bitrateKbps: number, packetsLost: number, latencyMs: number \| null }` |

### `<MetadataPanel>`

| Prop | Type | Description |
|---|---|---|
| `fields` | `KlvFieldWithChange[]` | From `useWebRTC().fields` |
| `updatedAt` | `string \| null` | From `useWebRTC().metaUpdatedAt` |

Highlights fields that changed in the last 800 ms with a green flash.

### `<PlatformMap>`

| Prop | Type | Description |
|---|---|---|
| `fields` | `KlvFieldWithChange[]` | From `useWebRTC().fields` |

Reads MISB ST 0601 tags directly from `fields`:
- Tag 13/14 — sensor latitude/longitude (UAV marker)
- Tag 23/24 — frame center latitude/longitude (crosshair marker + FOV sector)
- Tag 5 — platform heading (marker rotation)
- Tag 16 — horizontal FOV (sector width)

Requires Leaflet CSS to be imported by the consuming app.

---

## Geo utilities

Pure math functions, no dependencies.

```ts
import { bearing, haversineKm, destinationPoint, buildFovSector } from 'rtc-streamer-ui';

// Bearing in degrees from point A to point B
bearing(lat1, lon1, lat2, lon2): number

// Great-circle distance in km
haversineKm(lat1, lon1, lat2, lon2): number

// Point at given bearing and distance from origin
destinationPoint(lat, lon, bearingDeg, distKm): [lat, lon]

// Polygon vertices for a camera FOV sector (for use with Leaflet L.polygon)
buildFovSector(sensorLat, sensorLon, frameLat, frameLon, hfovDeg): [lat, lon][]
```

---

## TypeScript types

```ts
import type {
  StatusKind,        // 'connected' | 'connecting' | 'disconnected' | 'error'
  KlvField,          // { tag, name, value, unit? }
  KlvFieldWithChange,// KlvField & { changed: boolean }
  Stats,             // { bitrateKbps, packetsLost, latencyMs }
  UseWebRTCOptions,
  WebRTCState,
  UseSignalingOptions,
  UseSignalingReturn,
} from 'rtc-streamer-ui';
```
