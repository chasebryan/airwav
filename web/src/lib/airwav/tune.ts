import type { DecoderKey } from "./types";

export const TUNE_MIN_HZ = 500_000;
export const TUNE_MAX_HZ = 1_766_000_000;

export const TUNE_STEPS: { hz: number; label: string }[] = [
  { hz: 1_000, label: "1 kHz" },
  { hz: 5_000, label: "5 kHz" },
  { hz: 8_333, label: "8.33 kHz" },
  { hz: 12_500, label: "12.5 kHz" },
  { hz: 25_000, label: "25 kHz" },
  { hz: 100_000, label: "100 kHz" },
  { hz: 1_000_000, label: "1 MHz" },
];

export const BOOKMARKS: { label: string; hz: number; note: string }[] = [
  { label: "VHF air", hz: 136_000_000, note: "AM voice / ACARS window" },
  { label: "ACARS", hz: 131_550_000, note: "VHF ACARS primary" },
  { label: "APRS", hz: 144_390_000, note: "1200 baud AX.25" },
  { label: "NOAA", hz: 162_400_000, note: "Weather radio / SAME" },
  { label: "FM", hz: 98_500_000, note: "Broadcast FM" },
  { label: "433 ISM", hz: 433_920_000, note: "POCSAG / ISM" },
  { label: "1090 ES", hz: 1_090_000_000, note: "Mode S / ADS-B" },
];

export function clampHz(hz: number): number {
  return Math.round(Math.min(TUNE_MAX_HZ, Math.max(TUNE_MIN_HZ, hz)));
}

/** Parse an operator frequency. Bare values ≥ 1e6 are Hz, ≥ 3000 are kHz, otherwise MHz. */
export function parseTune(input: string): number | null {
  const s = input.trim().toLowerCase().replace(/,/g, "");
  if (!s) return null;
  const m = s.match(/^([+-]?\d*\.?\d+)\s*(hz|khz|mhz|ghz)?$/i);
  if (!m) return null;
  const n = Number(m[1]);
  if (!Number.isFinite(n) || n <= 0) return null;
  const unit = (m[2] ?? (n >= 1e6 ? "hz" : n >= 3000 ? "khz" : "mhz")).toLowerCase();
  const hz =
    unit === "ghz" ? n * 1e9 : unit === "mhz" ? n * 1e6 : unit === "khz" ? n * 1e3 : n;
  if (hz < TUNE_MIN_HZ || hz > TUNE_MAX_HZ) return null;
  return Math.round(hz);
}

export function stepHz(center: number, step: number, dir: -1 | 1): number {
  const next = center + dir * step;
  return clampHz(next);
}

export function formatTune(hz: number): string {
  if (hz >= 1e9) return `${(hz / 1e9).toFixed(6)} GHz`;
  return `${(hz / 1e6).toFixed(6)} MHz`;
}

export function scanList(centerHz: number): number[] {
  if (centerHz >= 118e6 && centerHz <= 137e6) {
    const list: number[] = [];
    for (let f = 118_000_000; f <= 136_975_000; f += 25_000) list.push(f);
    list.push(129_125_000, 130_025_000, 131_550_000, 136_000_000);
    return [...new Set(list)].sort((a, b) => a - b);
  }
  if (centerHz >= 144e6 && centerHz <= 148e6) {
    return [144_390_000, 145_000_000, 146_520_000];
  }
  if (centerHz >= 162.3e6 && centerHz <= 162.6e6) {
    return [162_400_000, 162_425_000, 162_450_000, 162_475_000, 162_500_000, 162_525_000, 162_550_000];
  }
  if (centerHz >= 88e6 && centerHz <= 108e6) {
    const list: number[] = [];
    for (let f = 88_100_000; f <= 107_900_000; f += 200_000) list.push(f);
    return list;
  }
  if (Math.abs(centerHz - 1_090_000_000) < 5e6) return [1_090_000_000];
  if (Math.abs(centerHz - 433.92e6) < 2e6) return [433_920_000, 433_000_000, 434_000_000];
  return [centerHz];
}

export function protocolHint(centerHz: number): string {
  if (Math.abs(centerHz - 1_090_000_000) < 1.2e6) return "Mode S / 1090ES window";
  if (centerHz >= 118e6 && centerHz <= 138e6) return "VHF air / ACARS window";
  if (Math.abs(centerHz - 144_390_000) < 15e3) return "APRS 1200 baud window";
  if (centerHz >= 162.4e6 && centerHz <= 162.55e6) return "NOAA SAME window";
  if (centerHz >= 88e6 && centerHz <= 108e6) return "FM broadcast window";
  if (Math.abs(centerHz - 433.92e6) < 2e5) return "ISM / POCSAG window";
  return "Observation window";
}

export function armedDecoders(centerHz: number, sampleRate: number): DecoderKey[] {
  const out: DecoderKey[] = [];
  if (Math.abs(centerHz - 1_090_000_000) < sampleRate) out.push("MODE_S");
  if (centerHz >= 118e6 && centerHz <= 138e6) out.push("ACARS");
  if (
    Math.abs(centerHz - 433.92e6) < 3e5 ||
    (centerHz >= 150e6 && centerHz <= 174e6 && Math.abs(centerHz - 162.4e6) > 2e5)
  ) {
    out.push("POCSAG");
  }
  if (centerHz >= 144e6 && centerHz <= 148e6) out.push("APRS");
  if (centerHz >= 162.3e6 && centerHz <= 162.6e6) out.push("SAME");
  return out;
}
