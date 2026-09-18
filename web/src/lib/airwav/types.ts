export const MAX_SAMPLE_RATE = 2_560_000;
export const BLOCK_SAMPLES = 4096;
export const WATERFALL_ROWS = 220;
export const ISLAND_BUDGET = 256;

export type ThemeName = "Midnight" | "Radar" | "Arctic" | "Ember" | "Studio";

export const THEME_NAMES: ThemeName[] = [
  "Midnight",
  "Radar",
  "Arctic",
  "Ember",
  "Studio",
];

export type OverlayView =
  | "help"
  | "diagnostics"
  | "events"
  | "evidence"
  | "settings"
  | "log"
  | null;

export type BandId = "vhf-air" | "es1090" | "noaa" | "fm" | "ism433";

export interface Band {
  id: BandId;
  label: string;
  centerHz: number;
  note: string;
}

export const BANDS: Band[] = [
  {
    id: "vhf-air",
    label: "VHF air",
    centerHz: 136_000_000,
    note: "Observation window only. No aviation decoder is enabled.",
  },
  {
    id: "es1090",
    label: "1090 MHz",
    centerHz: 1_090_000_000,
    note: "Observation window only. Mode S / 1090ES is not decoded.",
  },
  {
    id: "noaa",
    label: "NOAA VHF",
    centerHz: 162_400_000,
    note: "Observation window only. Weather-radio audio is not monitored.",
  },
  {
    id: "fm",
    label: "FM broadcast",
    centerHz: 98_500_000,
    note: "Observation window only. No demodulation or identification.",
  },
  {
    id: "ism433",
    label: "433 MHz ISM",
    centerHz: 433_920_000,
    note: "Observation window only. No protocol is established.",
  },
];

export interface ReceiverConfig {
  centerHz: number;
  sampleRate: number;
  gainTenthDb: number | null;
  ppm: number;
  biasTee: boolean;
}

export interface SignalIsland {
  id: number;
  firstSample: number;
  lastSample: number;
  centerHz: number;
  bandwidthHz: number;
  peakDbfs: number;
  snrDb: number;
  observations: number;
  state: "UNKNOWN" | "FADING / UNKNOWN";
}

export interface Spectrum {
  firstSample: number;
  binHz: number;
  startHz: number;
  powerDbfs: Float32Array;
  noiseDbfs: number;
}

export interface Metrics {
  receivedSamples: number;
  queueDroppedSamples: number;
  processedSamples: number;
  discontinuities: number;
  dspUs: number;
  ringBytes: number;
  ringCapacityBytes: number;
  storageDroppedSnapshots: number;
  storageDroppedIqSamples: number;
  islandCandidatesOmitted: number;
  frames: number;
}

export interface Snapshot {
  timestampNs: number;
  receiver: ReceiverConfig;
  spectrum: Spectrum;
  islands: SignalIsland[];
  metrics: Metrics;
}

export interface CapturedEvent {
  id: string;
  atNs: number;
  label: string;
  centerHz: number;
  islandId: number | null;
  peakDbfs: number;
  snrDb: number;
  samples: number;
  spectrum: number[];
  note: string;
}

export interface LogEntry {
  t: number;
  level: "info" | "warn" | "event";
  message: string;
}

export function formatMhz(hz: number, digits = 6): string {
  return (hz / 1e6).toFixed(digits);
}

export function formatKhz(hz: number, digits = 2): string {
  return (hz / 1e3).toFixed(digits);
}

export function formatSamples(n: number): string {
  if (n >= 1e6) return `${(n / 1e6).toFixed(2)} M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)} k`;
  return String(n);
}
