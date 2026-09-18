import type { BandId } from "./types";

const TAU = Math.PI * 2;

export interface Tone {
  offsetHz: number;
  amp: number;
  /** Drift in Hz per second added to offset. */
  drift: number;
  kind: "cw" | "pulse" | "burst";
}

export interface Scene {
  description: string;
  tones: Tone[];
  noise: number;
}

export function sceneFor(band: BandId): Scene {
  switch (band) {
    case "es1090":
      return {
        description:
          "DEMO FIXTURE · 1090 MHz window · pulsed carriers and seeded noise. UNKNOWN.",
        tones: [
          { offsetHz: -180_000, amp: 0.16, drift: 40, kind: "cw" },
          { offsetHz: 90_000, amp: 0.22, drift: 0, kind: "pulse" },
          { offsetHz: 310_000, amp: 0.28, drift: 0, kind: "burst" },
          { offsetHz: -420_000, amp: 0.07, drift: -20, kind: "cw" },
        ],
        noise: 0.045,
      };
    case "noaa":
      return {
        description:
          "DEMO FIXTURE · NOAA VHF window · two tones plus a fading neighbor. UNKNOWN.",
        tones: [
          { offsetHz: -80_000, amp: 0.24, drift: 8, kind: "cw" },
          { offsetHz: 125_000, amp: 0.11, drift: 0, kind: "cw" },
          { offsetHz: 410_000, amp: 0.18, drift: 0, kind: "pulse" },
        ],
        noise: 0.035,
      };
    case "fm":
      return {
        description:
          "DEMO FIXTURE · FM broadcast window · wide occupancy, no demodulation. UNKNOWN.",
        tones: [
          { offsetHz: -320_000, amp: 0.22, drift: 0, kind: "cw" },
          { offsetHz: -40_000, amp: 0.18, drift: 12, kind: "cw" },
          { offsetHz: 210_000, amp: 0.14, drift: 0, kind: "pulse" },
          { offsetHz: 540_000, amp: 0.2, drift: -6, kind: "cw" },
        ],
        noise: 0.05,
      };
    case "ism433":
      return {
        description:
          "DEMO FIXTURE · 433 MHz ISM window · short bursts over noise. UNKNOWN.",
        tones: [
          { offsetHz: -55_000, amp: 0.26, drift: 0, kind: "burst" },
          { offsetHz: 140_000, amp: 0.12, drift: 90, kind: "cw" },
          { offsetHz: 380_000, amp: 0.19, drift: 0, kind: "pulse" },
        ],
        noise: 0.04,
      };
    default:
      return {
        description:
          "DEMO FIXTURE · deterministic synthetic IQ · three drifting/burst carriers plus seeded noise. No aircraft, protocols, or identities.",
        tones: [
          { offsetHz: -410_000, amp: 0.2, drift: 130, kind: "cw" },
          { offsetHz: 240_000, amp: 0.08, drift: 0, kind: "pulse" },
          { offsetHz: 610_000, amp: 0.3, drift: 0, kind: "burst" },
        ],
        noise: 0.04,
      };
  }
}

export interface Seed {
  v: number;
}

export function nextU32(seed: Seed): number {
  seed.v = (Math.imul(seed.v, 1664525) + 1013904223) >>> 0;
  return seed.v;
}

function toneOn(kind: Tone["kind"], time: number): boolean {
  if (kind === "cw") return true;
  if (kind === "pulse") return Math.floor(time * 2) % 3 !== 0;
  const cycle = time % 8;
  return cycle > 1.3 && cycle < 2.4;
}

/** Unsigned interleaved I/Q matching airwav-dsp conversion `(x-127.5)/128`. */
export function generateIq(
  firstSample: number,
  count: number,
  sampleRate: number,
  scene: Scene,
  seed: Seed,
  time0?: number,
): Uint8Array {
  const bytes = new Uint8Array(count * 2);
  const inv = 1 / sampleRate;
  const tBase = time0 ?? firstSample * inv;
  for (let i = 0; i < count; i++) {
    const time = tBase + i * inv;
    let re = 0;
    let im = 0;
    for (const tone of scene.tones) {
      if (!toneOn(tone.kind, time)) continue;
      const hz = tone.offsetHz + tone.drift * time;
      const phase = TAU * hz * time;
      re += tone.amp * Math.cos(phase);
      im += tone.amp * Math.sin(phase);
    }
    re += ((nextU32(seed) >>> 16) / 65536 - 0.5) * scene.noise;
    im += ((nextU32(seed) >>> 16) / 65536 - 0.5) * scene.noise;
    bytes[i * 2] = Math.round(Math.min(255, Math.max(0, 127.5 + re * 128)));
    bytes[i * 2 + 1] = Math.round(Math.min(255, Math.max(0, 127.5 + im * 128)));
  }
  return bytes;
}
