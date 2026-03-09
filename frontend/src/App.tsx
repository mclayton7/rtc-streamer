import { useWebRTC } from './hooks/useWebRTC';
import { VideoPlayer } from './components/VideoPlayer';
import { StatsPanel } from './components/StatsPanel';
import { MetadataPanel } from './components/MetadataPanel';
import { PlatformMap } from './components/PlatformMap';

export default function App() {
  const {
    statusText,
    statusKind,
    connState,
    loading,
    streamOnline,
    fields,
    metaUpdatedAt,
    stats,
    connect,
    disconnect,
    videoRef,
  } = useWebRTC();

  return (
    <div className="container">
      <header>
        <h1>RTC Streamer</h1>
        <div className={`status ${statusKind}`}>{statusText}</div>
      </header>

      <main>
        <div className="main-grid">
          <div className="main-left">
            <VideoPlayer
              videoRef={videoRef}
              loading={loading}
              streamOnline={streamOnline}
              statusKind={statusKind}
            />

            <div className="controls">
              <button
                className="btn btn-primary"
                onClick={connect}
                disabled={statusKind !== 'disconnected'}
              >
                Connect
              </button>
              <button
                className="btn btn-secondary"
                onClick={disconnect}
                disabled={statusKind === 'disconnected'}
              >
                Disconnect
              </button>
            </div>

            <StatsPanel connState={connState} stats={stats} />
          </div>

          <MetadataPanel fields={fields} updatedAt={metaUpdatedAt} />
        </div>

        <PlatformMap fields={fields} />
      </main>

      <footer>
        <p>Ultra-low latency WebRTC streaming</p>
      </footer>
    </div>
  );
}
