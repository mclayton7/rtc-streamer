import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';
import type { KlvField, KlvFieldWithChange, Stats, StatusKind } from '../types';
import { useSignaling } from './useSignaling';

export interface UseWebRTCOptions {
  /** WebSocket signaling URL. Defaults to ws(s)://current-host/signal */
  signalingUrl?: string;
  /** Auto-reconnect on unexpected close. Default: true */
  reconnect?: boolean;
  /** Initial reconnect delay in ms. Default: 1000 */
  reconnectInitialDelay?: number;
  /** Maximum reconnect delay in ms. Default: 30000 */
  reconnectMaxDelay?: number;
  /** ICE servers used when the server doesn't send a config message */
  fallbackIceServers?: RTCIceServer[];
}

export interface WebRTCState {
  statusText: string;
  statusKind: StatusKind;
  connState: string;
  loading: boolean;
  streamOnline: boolean;
  fields: KlvFieldWithChange[];
  metaUpdatedAt: string | null;
  stats: Stats;
  isConnected: boolean;
  connect: () => void;
  disconnect: () => void;
  videoRef: RefObject<HTMLVideoElement | null>;
}

function defaultSignalingUrl(): string {
  if (typeof window === 'undefined') return '/signal';
  const proto = location.protocol === 'https:' ? 'wss://' : 'ws://';
  return `${proto}${window.location.host}/signal`;
}

export function useWebRTC(opts: UseWebRTCOptions = {}): WebRTCState {
  const optsRef = useRef(opts);
  optsRef.current = opts;

  // ── mutable instance state (refs – no re-render on change) ──────────
  const pcRef = useRef<RTCPeerConnection | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const iceServersRef = useRef<RTCIceServer[]>([]);
  const prevBytesRef = useRef(0);
  const prevStatsTimeRef = useRef<number | null>(null);
  const prevMetaRef = useRef<Record<string, string>>({});
  // Always points to the latest send() from useSignaling
  const sendRef = useRef<(data: string | object) => void>(() => {});

  // ── UI state ────────────────────────────────────────────────────────
  const [statusText, setStatusText] = useState('Disconnected');
  const [statusKind, setStatusKind] = useState<StatusKind>('disconnected');
  const [connState, setConnState] = useState('disconnected');
  const [loading, setLoading] = useState(false);
  const [streamOnline, setStreamOnline] = useState(false);
  const [fields, setFields] = useState<KlvFieldWithChange[]>([]);
  const [metaUpdatedAt, setMetaUpdatedAt] = useState<string | null>(null);
  const [bitrateKbps, setBitrateKbps] = useState(0);
  const [packetsLost, setPacketsLost] = useState(0);
  const [latencyMs, setLatencyMs] = useState<number | null>(null);
  const [isConnected, setIsConnected] = useState(false);

  // ── stats polling: runs while connected ─────────────────────────────
  useEffect(() => {
    if (!isConnected) return;

    prevBytesRef.current = 0;
    prevStatsTimeRef.current = null;

    const id = setInterval(async () => {
      const currentPc = pcRef.current;
      if (!currentPc) return;
      try {
        const stats = await currentPc.getStats();
        const now = Date.now();
        let bytes = 0;
        let lost = 0;
        let rtt: number | null = null;

        stats.forEach((r: RTCStats) => {
          if (r.type === 'inbound-rtp') {
            const s = r as unknown as {
              kind: string;
              bytesReceived: number;
              packetsLost: number;
            };
            if (s.kind === 'video') {
              bytes = s.bytesReceived ?? 0;
              lost = s.packetsLost ?? 0;
            }
          }
          if (r.type === 'candidate-pair') {
            const s = r as unknown as { state: string; currentRoundTripTime?: number };
            if (s.state === 'succeeded' && s.currentRoundTripTime != null) {
              rtt = s.currentRoundTripTime;
            }
          }
        });

        if (prevStatsTimeRef.current !== null) {
          const elapsed = (now - prevStatsTimeRef.current) / 1000;
          const delta = bytes - prevBytesRef.current;
          setBitrateKbps(elapsed > 0 ? Math.round((delta * 8) / (elapsed * 1000)) : 0);
        }
        prevBytesRef.current = bytes;
        prevStatsTimeRef.current = now;
        setPacketsLost(lost);
        if (rtt !== null) setLatencyMs(Math.round(rtt * 1000));
      } catch {
        // Stats errors are non-fatal
      }
    }, 1000);

    return () => clearInterval(id);
  }, [isConnected]);

  // ── helpers ──────────────────────────────────────────────────────────

  function cleanupPeer() {
    pcRef.current?.close();
    pcRef.current = null;
    if (videoRef.current) videoRef.current.srcObject = null;
    setLoading(false);
    setIsConnected(false);
    setStreamOnline(false);
    setConnState('disconnected');
    setBitrateKbps(0);
    setPacketsLost(0);
    setLatencyMs(null);
    prevMetaRef.current = {};
    setFields([]);
    setMetaUpdatedAt(null);
  }

  async function handleOffer(sdp: string) {
    const fallback = optsRef.current.fallbackIceServers ?? [
      { urls: 'stun:stun.l.google.com:19302' },
    ];
    const iceServers = iceServersRef.current.length > 0 ? iceServersRef.current : fallback;
    const newPc = new RTCPeerConnection({ iceServers });
    pcRef.current = newPc;

    newPc.ontrack = (event) => {
      if (event.track.kind === 'video' && videoRef.current) {
        videoRef.current.srcObject = event.streams[0];
        setLoading(false);
        setStatusText('Connected');
        setStatusKind('connected');
        setIsConnected(true);
      }
    };

    newPc.onicecandidate = (event) => {
      if (event.candidate) {
        sendRef.current({ type: 'ice-candidate', candidate: event.candidate.candidate });
      }
    };

    newPc.onconnectionstatechange = () => {
      setConnState(newPc.connectionState);
      if (
        newPc.connectionState === 'failed' ||
        newPc.connectionState === 'disconnected'
      ) {
        setStatusText('Connection lost');
        setStatusKind('error');
      }
    };

    await newPc.setRemoteDescription(new RTCSessionDescription({ type: 'offer', sdp }));
    const answer = await newPc.createAnswer();
    await newPc.setLocalDescription(answer);
    sendRef.current({ type: 'answer', sdp: answer.sdp! });
  }

  function handleMetadata(newFields: KlvField[]) {
    if (!newFields?.length) return;
    const prev = prevMetaRef.current;
    const newPrev: Record<string, string> = {};
    const withChange: KlvFieldWithChange[] = newFields.map((f) => {
      const key = String(f.tag);
      newPrev[key] = f.value;
      return { ...f, changed: key in prev && prev[key] !== f.value };
    });
    prevMetaRef.current = newPrev;
    setFields(withChange);
    setMetaUpdatedAt(new Date().toLocaleTimeString());
    setTimeout(() => {
      setFields((curr) =>
        curr === withChange ? curr.map((f) => ({ ...f, changed: false })) : curr,
      );
    }, 800);
  }

  async function handleMessage(event: MessageEvent) {
    const msg = JSON.parse(event.data as string) as Record<string, unknown>;
    switch (msg.type) {
      case 'config':
        iceServersRef.current = (msg.ice_servers as string[]).map((url) => ({ urls: url }));
        break;
      case 'offer':
        await handleOffer(msg.sdp as string);
        break;
      case 'ice-candidate':
        if (pcRef.current) {
          try {
            await pcRef.current.addIceCandidate(
              new RTCIceCandidate({ candidate: msg.candidate as string }),
            );
          } catch {
            // ICE candidate errors are non-fatal
          }
        }
        break;
      case 'error':
        setStatusText(`Error: ${msg.message as string}`);
        setStatusKind('error');
        setLoading(false);
        break;
      case 'metadata':
        handleMetadata(msg.fields as KlvField[]);
        break;
      case 'stream-status':
        setStreamOnline(msg.online as boolean);
        break;
    }
  }

  // ── signaling ────────────────────────────────────────────────────────

  const { send, connect: sigConnect, disconnect: sigDisconnect } = useSignaling({
    url: opts.signalingUrl ?? defaultSignalingUrl(),
    reconnect: opts.reconnect ?? true,
    reconnectInitialDelay: opts.reconnectInitialDelay ?? 1000,
    reconnectMaxDelay: opts.reconnectMaxDelay ?? 30_000,
    onOpen: () => {
      setStatusText('Connecting...');
      setStatusKind('connecting');
      setLoading(true);
      sendRef.current({ type: 'watch' });
    },
    onMessage: (e) => {
      void handleMessage(e);
    },
    onClose: (userInitiated) => {
      cleanupPeer();
      if (userInitiated) {
        setStatusText('Disconnected');
        setStatusKind('disconnected');
      }
      // If not user-initiated, onReconnecting will update the status text.
    },
    onReconnecting: (delay) => {
      setStatusText(`Reconnecting in ${Math.round(delay / 1000)}s...`);
      setStatusKind('connecting');
    },
  });

  // Keep sendRef current so async handlers (onicecandidate, etc.) always use the latest.
  sendRef.current = send;

  const connect = useCallback(() => {
    setStatusText('Connecting...');
    setStatusKind('connecting');
    setLoading(true);
    sigConnect();
  }, [sigConnect]);

  const disconnect = useCallback(() => sigDisconnect(), [sigDisconnect]);

  return {
    statusText,
    statusKind,
    connState,
    loading,
    streamOnline,
    fields,
    metaUpdatedAt,
    stats: { bitrateKbps, packetsLost, latencyMs },
    isConnected,
    connect,
    disconnect,
    videoRef,
  };
}
