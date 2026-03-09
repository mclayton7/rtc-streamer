import type { Stats } from '../types';

interface Props {
  connState: string;
  stats: Stats;
}

export function StatsPanel({ connState, stats }: Props) {
  const { bitrateKbps, packetsLost, latencyMs } = stats;
  return (
    <div className="stats">
      <h3>Connection Stats</h3>
      <div className="stats-grid">
        <div className="stat">
          <span className="stat-label">Status:</span>
          <span>{connState}</span>
        </div>
        <div className="stat">
          <span className="stat-label">Bitrate:</span>
          <span>{bitrateKbps} kbps</span>
        </div>
        <div className="stat">
          <span className="stat-label">Packets Lost:</span>
          <span>{packetsLost}</span>
        </div>
        <div className="stat">
          <span className="stat-label">Latency:</span>
          <span>{latencyMs !== null ? `${latencyMs} ms` : '— ms'}</span>
        </div>
      </div>
    </div>
  );
}
