import React from 'react';
import type { StatusKind } from '../types';

interface Props {
  videoRef: React.RefObject<HTMLVideoElement | null>;
  loading: boolean;
  streamOnline: boolean;
  statusKind: StatusKind;
}

export function VideoPlayer({ videoRef, loading, streamOnline, statusKind }: Props) {
  return (
    <div className="video-container">
      <video ref={videoRef as React.RefObject<HTMLVideoElement>} autoPlay playsInline controls muted />

      {loading && (
        <div className="loading">
          <div className="spinner" />
          <p>Connecting...</p>
        </div>
      )}

      {/* Only show the offline banner while an active session is in progress */}
      {statusKind !== 'disconnected' && !streamOnline && (
        <div className="stream-offline-banner">
          <p>Waiting for stream source...</p>
        </div>
      )}
    </div>
  );
}
