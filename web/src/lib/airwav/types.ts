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
  | "frames"
  | null;

export type ProtocolId = "UNKNOWN" | "MODE_S" | "ADS_B" | "ACARS" | "POCSAG" | "APRS" | "SAME";

export type DecoderKey = Exclude<ProtocolId, "UNKNOWN" | "ADS_B">;

export const DECODER_KEYS: DecoderKey[] = ["MODE_S", "ACARS", "POCSAG", "APRS", "SAME"];

export const DECODER_LABEL: Record<DecoderKey, string> = {
  MODE_S: "Mode S",
  ACARS: "ACARS",
  POCSAG: "POCSAG",
  APRS: "APRS",
  SAME: "SAME",
};


export const PROTOCOL_TAG: Record<ProtocolId, string> = {
  UNKNOWN: "UNK",
  MODE_S: "MS",
  ADS_B: "ADS-B",
  ACARS: "ACARS",
  POCSAG: "POCS",
  APRS: "APRS",
  SAME: "SAME",
};

export type BandId = "vhf-air" | "es1090" | "noaa" | "fm" | "ism433" | "aprs" | "acars";

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
    note: "ACARS decoder armed in 118–138 MHz.",
  },
  {
    id: "acars",
    label: "ACARS",
    centerHz: 131_550_000,
    note: "VHF ACARS. Odd parity + block checksum.",
  },
  {
    id: "aprs",
    label: "APRS",
    centerHz: 144_390_000,
    note: "AX.25 1200 baud. CRC-16 required.",
  },
  {
    id: "es1090",
    label: "1090 MHz",
    centerHz: 1_090_000_000,
    note: "Mode S / 1090ES. CRC-24 required.",
  },
  {
    id: "noaa",
    label: "NOAA VHF",
    centerHz: 162_400_000,
    note: "SAME header decoder armed.",
  },
  {
    id: "fm",
    label: "FM broadcast",
    centerHz: 98_500_000,
    note: "Listening only. No RDS decoder.",
  },
  {
    id: "ism433",
    label: "433 MHz ISM",
    centerHz: 433_920_000,
    note: "POCSAG BCH decoder armed.",
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
  state: "LIVE" | "FADING" | "UNKNOWN" | "FADING / UNKNOWN";
  protocol: ProtocolId;
  verified: boolean;
}

export interface DecodedFrame {
  id: string;
  protocol: ProtocolId;
  atSample: number;
  frequencyHz: number;
  verified: boolean;
  confidence: "verified" | "crc-fail" | "candidate";
  fields: Record<string, string>;
  rawHex: string;
  warnings: string[];
  evidence: string;
  islandId: number | null;
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
  decodedFrames: number;
  verifiedFrames: number;
}

export interface Snapshot {
  timestampNs: number;
  receiver: ReceiverConfig;
  spectrum: Spectrum;
  islands: SignalIsland[];
  metrics: Metrics;
  frames: DecodedFrame[];
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
  protocol: ProtocolId;
  hex?: string;
}

export interface LogEntry {
  t: number;
  level: "info" | "warn" | "event";
  message: string;
}

export type SourceKind = "synthetic" | "file";

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

export function activityOf(state: string): "LIVE" | "FADING" {
  return state.startsWith("FADING") ? "FADING" : "LIVE";
}

export function protocolOf(island: { protocol?: string }): ProtocolId {
  const p = island.protocol ?? "UNKNOWN";
  if (
    p === "MODE_S" ||
    p === "ADS_B" ||
    p === "ACARS" ||
    p === "POCSAG" ||
    p === "APRS" ||
    p === "SAME"
  ) {
    return p;
  }
  return "UNKNOWN";
}
