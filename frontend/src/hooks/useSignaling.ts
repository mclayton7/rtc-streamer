import { useCallback, useEffect, useRef, useState } from 'react';

export interface UseSignalingOptions {
  url: string;
  /** Whether to auto-reconnect on unexpected close. Default: true */
  reconnect?: boolean;
  /** Initial reconnect delay in ms. Default: 1000 */
  reconnectInitialDelay?: number;
  /** Maximum reconnect delay in ms. Default: 30000 */
  reconnectMaxDelay?: number;
  /** Called when the WebSocket connection opens */
  onOpen?: () => void;
  /** Called for each incoming message */
  onMessage?: (event: MessageEvent) => void;
  /**
   * Called when the WebSocket closes.
   * `userInitiated` is true when closed by calling disconnect().
   */
  onClose?: (userInitiated: boolean) => void;
  /** Called just before a reconnect attempt, with the delay in ms */
  onReconnecting?: (delayMs: number) => void;
}

export interface UseSignalingReturn {
  isOpen: boolean;
  send: (data: string | object) => void;
  connect: () => void;
  disconnect: () => void;
}

export function useSignaling(opts: UseSignalingOptions): UseSignalingReturn {
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectDelayRef = useRef(opts.reconnectInitialDelay ?? 1000);
  const isUserDisconnectRef = useRef(false);

  // Stable fn refs so timer callbacks always invoke the latest version.
  const connectFnRef = useRef<() => void>(() => {});
  const disconnectFnRef = useRef<() => void>(() => {});

  // Keep opts current across renders without needing them as effect deps.
  const optsRef = useRef(opts);
  optsRef.current = opts;

  const [isOpen, setIsOpen] = useState(false);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      isUserDisconnectRef.current = true;
      disconnectFnRef.current();
    };
  }, []);

  function scheduleReconnect() {
    const delay = reconnectDelayRef.current;
    optsRef.current.onReconnecting?.(delay);
    reconnectTimerRef.current = setTimeout(() => {
      reconnectTimerRef.current = null;
      connectFnRef.current();
    }, delay);
    reconnectDelayRef.current = Math.min(delay * 2, optsRef.current.reconnectMaxDelay ?? 30_000);
  }

  function doConnect() {
    isUserDisconnectRef.current = false;
    if (reconnectTimerRef.current !== null) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }

    try {
      const ws = new WebSocket(optsRef.current.url);
      wsRef.current = ws;

      ws.onopen = () => {
        setIsOpen(true);
        // Reset backoff on successful open
        reconnectDelayRef.current = optsRef.current.reconnectInitialDelay ?? 1000;
        optsRef.current.onOpen?.();
      };

      ws.onmessage = (e) => optsRef.current.onMessage?.(e);

      ws.onerror = () => {
        // Will always be followed by onclose — handle reconnect there.
      };

      ws.onclose = () => {
        wsRef.current = null;
        setIsOpen(false);
        optsRef.current.onClose?.(isUserDisconnectRef.current);
        if (!isUserDisconnectRef.current && (optsRef.current.reconnect ?? true)) {
          scheduleReconnect();
        }
      };
    } catch {
      if (!isUserDisconnectRef.current && (optsRef.current.reconnect ?? true)) {
        scheduleReconnect();
      }
    }
  }

  function doDisconnect() {
    isUserDisconnectRef.current = true;
    if (reconnectTimerRef.current !== null) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
    // Null the ref first; onclose will still fire but will be a no-op for reconnect.
    const ws = wsRef.current;
    wsRef.current = null;
    ws?.close();
    setIsOpen(false);
  }

  // Keep fn refs pointing to current implementations every render.
  connectFnRef.current = doConnect;
  disconnectFnRef.current = doDisconnect;

  // Public connect resets the backoff delay so manual reconnects start fresh.
  const connect = useCallback(() => {
    reconnectDelayRef.current = optsRef.current.reconnectInitialDelay ?? 1000;
    connectFnRef.current();
  }, []);

  const disconnect = useCallback(() => disconnectFnRef.current(), []);

  const send = useCallback((data: string | object) => {
    const ws = wsRef.current;
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(typeof data === 'string' ? data : JSON.stringify(data));
    }
  }, []);

  return { isOpen, send, connect, disconnect };
}
