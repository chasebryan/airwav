import {
  acarsToBits,
  ax25BitStream,
  encodeAcars,
  encodeAprs,
  encodeModeSDf11,
  encodeModeSDf17Ident,
  encodeModeSDf17Velocity,
  encodePocsag,
  modeSChips,
} from "./decode";

const TAU = Math.PI * 2;

export interface Tone {
  offsetHz: number;
  amp: number;
  drift: number;
  kind: "cw" | "pulse" | "burst";
}

export type DigitalKind = "mode-s" | "acars" | "pocsag" | "aprs" | "same";

export interface DigitalBurst {
  kind: DigitalKind;
  offsetHz: number;
  amp: number;
  periodSec: number;
  phaseSec: number;
}

export interface Scene {
  description: string;
  tones: Tone[];
  digital: DigitalBurst[];
  noise: number;
}

export interface PhaseState {
  pocsag: number;
  aprs: number;
  same: number;
}

export function newPhaseState(): PhaseState {
  return { pocsag: 0, aprs: 0, same: 0 };
}

export interface Seed {
  v: number;
}

export function sceneForHz(centerHz: number): Scene {
  if (Math.abs(centerHz - 1_090_000_000) < 1_200_000) {
    return {
      description: "1090ES · Mode S PPM bursts with CRC-24. ICAO is CRC evidence, not a guess.",
      tones: [
        { offsetHz: -420_000, amp: 0.07, drift: -18, kind: "cw" },
        { offsetHz: 310_000, amp: 0.05, drift: 12, kind: "pulse" },
      ],
      digital: [{ kind: "mode-s", offsetHz: 0, amp: 0.72, periodSec: 0.09, phaseSec: 0.0002 }],
      noise: 0.035,
    };
  }
  if (Math.abs(centerHz - 131_550_000) < 20_000 || (centerHz >= 118e6 && centerHz <= 137e6)) {
    const acars = Math.abs(centerHz - 131_550_000) < 80_000;
    return {
      description: acars
        ? "VHF ACARS · 2400 baud MSK on AM. Block checksum required."
        : "VHF air · AM occupancy plus ACARS bursts in-window.",
      tones: [
        { offsetHz: -410_000, amp: 0.18, drift: 90, kind: "cw" },
        { offsetHz: 240_000, amp: 0.08, drift: 0, kind: "pulse" },
        { offsetHz: 610_000, amp: 0.22, drift: 0, kind: "burst" },
      ],
      digital: [{ kind: "acars", offsetHz: acars ? 0 : -80_000, amp: 0.55, periodSec: 0.85, phaseSec: 0.02 }],
      noise: 0.038,
    };
  }
  if (Math.abs(centerHz - 144_390_000) < 20_000) {
    return {
      description: "APRS · Bell 202 AFSK, AX.25 HDLC CRC-16.",
      tones: [{ offsetHz: 40_000, amp: 0.06, drift: 0, kind: "cw" }],
      digital: [{ kind: "aprs", offsetHz: 0, amp: 0.62, periodSec: 1.15, phaseSec: 0.04 }],
      noise: 0.04,
    };
  }
  if (centerHz >= 162_380_000 && centerHz <= 162_560_000) {
    return {
      description: "NOAA VHF · SAME AFSK header plus carrier.",
      tones: [
        { offsetHz: 0, amp: 0.22, drift: 4, kind: "cw" },
        { offsetHz: 125_000, amp: 0.09, drift: 0, kind: "cw" },
      ],
      digital: [{ kind: "same", offsetHz: 0, amp: 0.48, periodSec: 1.8, phaseSec: 0.08 }],
      noise: 0.032,
    };
  }
  if (centerHz >= 88e6 && centerHz <= 108e6) {
    return {
      description: "FM broadcast occupancy. No RDS/identification decoder.",
      tones: [
        { offsetHz: -320_000, amp: 0.22, drift: 0, kind: "cw" },
        { offsetHz: -40_000, amp: 0.18, drift: 8, kind: "cw" },
        { offsetHz: 210_000, amp: 0.14, drift: 0, kind: "pulse" },
        { offsetHz: 540_000, amp: 0.2, drift: -6, kind: "cw" },
      ],
      digital: [],
      noise: 0.05,
    };
  }
  if (Math.abs(centerHz - 433_920_000) < 250_000) {
    return {
      description: "433 MHz ISM · POCSAG FSK with BCH(31,21).",
      tones: [{ offsetHz: 140_000, amp: 0.08, drift: 40, kind: "cw" }],
      digital: [{ kind: "pocsag", offsetHz: 0, amp: 0.58, periodSec: 1.2, phaseSec: 0.04 }],
      noise: 0.04,
    };
  }
  return {
    description: "Untargeted window · drifting carriers. No decoder is armed.",
    tones: [
      { offsetHz: -410_000, amp: 0.2, drift: 130, kind: "cw" },
      { offsetHz: 240_000, amp: 0.08, drift: 0, kind: "pulse" },
      { offsetHz: 610_000, amp: 0.3, drift: 0, kind: "burst" },
    ],
    digital: [],
    noise: 0.04,
  };
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

function modeSFrame(t: number): Uint8Array {
  const n = Math.floor(t * 11) % 3;
  if (n === 0) return encodeModeSDf17Ident(0xa1b2c3, "AIRWAV1");
  if (n === 1) return encodeModeSDf17Velocity(0xa1b2c3, 420, 275);
  return encodeModeSDf11(0xa1b2c3);
}

function mixCarrier(re: number, im: number, amp: number, phase: number, env: number): [number, number] {
  const a = amp * env;
  return [re + a * Math.cos(phase), im + a * Math.sin(phase)];
}

function digitalEnv(kind: DigitalKind, tRel: number, periodIndex: number): number {
  if (tRel < 0) return 0;
  if (kind === "mode-s") {
    const chips = modeSChips(modeSFrame(periodIndex));
    const idx = Math.floor(tRel / 0.5e-6);
    if (idx < 0 || idx >= chips.length) return 0;
    return chips[idx]!;
  }
  if (kind === "acars") {
    const bits = acarsToBits(
      encodeAcars({ mode: "2", addr: "N17XX", label: "Q0", text: "AIRWAV TEST" }),
    );
    const baud = 2400;
    const bi = Math.floor(tRel * baud);
    if (bi < 0 || bi >= bits.length) return 0;
    const freq = bits[bi] ? 2400 : 1200;
    return 0.55 + 0.45 * Math.cos(TAU * freq * tRel);
  }
  if (kind === "pocsag") {
    const words = encodePocsag(0x1a2b3c, "AIRWAV");
    const baud = 1200;
    const bitIndex = Math.floor(tRel * baud);
    const totalBits = words.length * 32;
    if (bitIndex < 0 || bitIndex >= totalBits) return 0;
    const w = words[(bitIndex / 32) | 0]!;
    const b = (w >> (31 - (bitIndex % 32))) & 1;
    return b ? 1 : -1;
  }
  if (kind === "aprs") {
    return 1;
  }
  if (kind === "same") {
    const header = "ZCZC-WXR-RWT-000000+0015-1230000-AIRWAV-";
    const bits: number[] = [];
    for (const ch of header) {
      const c = ch.charCodeAt(0);
      for (let i = 0; i < 8; i++) bits.push((c >> i) & 1);
    }
    const baud = 520.83;
    const bi = Math.floor(tRel * baud);
    if (bi < 0 || bi >= bits.length) return 0;
    const freq = bits[bi] ? 1562.5 : 2083.3;
    return 0.5 + 0.5 * Math.cos(TAU * freq * tRel);
  }
  return 0;
}

const APRS_NRZI: number[] = (() => {
  const bits = ax25BitStream(encodeAprs("!0000.00N/00000.00W# AIRWAV TEST"));
  let level = 1;
  const nrziBits: number[] = [];
  for (const bit of bits) {
    if (bit === 0) level ^= 1;
    nrziBits.push(level);
  }
  return nrziBits;
})();

function burstActive(burst: DigitalBurst, time: number): number {
  const t = time - burst.phaseSec;
  if (t < 0) return -1;
  const periodIndex = Math.floor(t / burst.periodSec);
  const tRel = t - periodIndex * burst.periodSec;
  const dur =
    burst.kind === "mode-s"
      ? 0.00014
      : burst.kind === "acars"
        ? 0.45
        : burst.kind === "aprs"
          ? 0.9
          : burst.kind === "pocsag"
            ? 0.7
            : 0.6;
  if (tRel > dur) return -1;
  return tRel;
}

/** Unsigned interleaved I/Q matching airwav-dsp conversion `(x-127.5)/128`. */
export function generateIq(
  firstSample: number,
  count: number,
  sampleRate: number,
  scene: Scene,
  seed: Seed,
  time0?: number,
  ppm = 0,
  gainScale = 1,
  phase: PhaseState = newPhaseState(),
): Uint8Array {
  const bytes = new Uint8Array(count * 2);
  const inv = 1 / sampleRate;
  const tBase = time0 ?? firstSample * inv;
  const ppmScale = 1 + ppm / 1e6;
  const dt = inv;
  for (let i = 0; i < count; i++) {
    const time = tBase + i * inv;
    let re = 0;
    let im = 0;
    for (const tone of scene.tones) {
      if (!toneOn(tone.kind, time)) continue;
      const hz = (tone.offsetHz + tone.drift * time) * ppmScale;
      const ph = TAU * hz * time;
      re += tone.amp * gainScale * Math.cos(ph);
      im += tone.amp * gainScale * Math.sin(ph);
    }
    for (const burst of scene.digital) {
      const tRel = burstActive(burst, time);
      if (tRel < 0) continue;
      const periodIndex = Math.floor((time - burst.phaseSec) / burst.periodSec);
      const env = digitalEnv(burst.kind, tRel, periodIndex);
      const hz = burst.offsetHz * ppmScale;
      if (burst.kind === "pocsag") {
        phase.pocsag += TAU * (hz + env * 4500) * dt;
        re += burst.amp * gainScale * Math.cos(phase.pocsag);
        im += burst.amp * gainScale * Math.sin(phase.pocsag);
      } else if (burst.kind === "aprs") {
        const bi = Math.floor(tRel * 1200);
        const mark = (APRS_NRZI[bi] ?? 1) ? 1200 : 2200;
        phase.aprs += TAU * (hz + mark) * dt;
        re += burst.amp * gainScale * Math.cos(phase.aprs);
        im += burst.amp * gainScale * Math.sin(phase.aprs);
      } else {
        const ph = TAU * hz * time;
        [re, im] = mixCarrier(re, im, burst.amp * gainScale, ph, Math.max(0, env));
      }
    }
    re += ((nextU32(seed) >>> 16) / 65536 - 0.5) * scene.noise;
    im += ((nextU32(seed) >>> 16) / 65536 - 0.5) * scene.noise;
    bytes[i * 2] = Math.round(Math.min(255, Math.max(0, 127.5 + re * 128)));
    bytes[i * 2 + 1] = Math.round(Math.min(255, Math.max(0, 127.5 + im * 128)));
  }
  return bytes;
}
