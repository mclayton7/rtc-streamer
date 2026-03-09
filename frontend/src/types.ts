export type StatusKind = 'connected' | 'connecting' | 'disconnected' | 'error';

export interface KlvField {
  tag: number;
  name: string;
  value: string;
  unit?: string;
}

export interface KlvFieldWithChange extends KlvField {
  changed: boolean;
}

export interface Stats {
  bitrateKbps: number;
  packetsLost: number;
  latencyMs: number | null;
}
