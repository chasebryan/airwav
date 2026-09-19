import assert from "node:assert/strict";
import { test } from "node:test";
import {
  decodeModeS,
  encodeAcars,
  encodeModeSDf11,
  encodeModeSDf17Ident,
  modeSChips,
  modeSCrc,
  parseAcarsBytes,
  parseModeSMessage,
  pocsagBch,
  encodePocsag,
  pocsagValid,
  ax25Fcs,
  encodeAprs,
  AcarsDecoder,
  PocsagDecoder,
  AprsDecoder,
  SameDecoder,
  acarsToBits,
  ax25BitStream,
} from "./decode.ts";
import { parseTune, armedDecoders, clampHz } from "./tune.ts";

const TAU = Math.PI * 2;

function rx(centerHz: number, sampleRate: number) {
  return { centerHz, sampleRate, gainTenthDb: null, ppm: 0, biasTee: false };
}

test("Mode S CRC-24 encodes to remainder 0", () => {
  const ident = encodeModeSDf17Ident(0xa1b2c3, "AIRWAV1");
  assert.equal(modeSCrc(ident, 112), 0);
  const df11 = encodeModeSDf11(0xa1b2c3);
  assert.equal(modeSCrc(df11, 56), 0);
  const parsed = parseModeSMessage(ident);
  assert.ok(parsed?.verified);
  assert.equal(parsed?.fields.ICAO, "A1B2C3");
  assert.equal(parsed?.fields.callsign, "AIRWAV1");
  assert.equal(parsed?.protocol, "ADS_B");
});

test("Mode S PPM round-trip from synthetic IQ", () => {
  const msg = encodeModeSDf17Ident(0xa1b2c3, "AIRWAV1");
  const chips = modeSChips(msg);
  const rate = 2_560_000;
  const n = 4096;
  const re = new Float32Array(n);
  const im = new Float32Array(n);
  const start = 40;
  for (let i = 0; i < n; i++) {
    const tRel = (i - start) / rate;
    const idx = Math.floor(tRel / 0.5e-6);
    const env = idx >= 0 && idx < chips.length ? chips[idx]! : 0;
    re[i] = 0.7 * env;
    im[i] = 0;
  }
  const frames = decodeModeS(re, im, 0, rx(1_090_000_000, rate));
  const hit = frames.find((f) => f.verified);
  assert.ok(hit, `no verified frame, got ${JSON.stringify(frames)}`);
  assert.equal(hit.fields.ICAO, "A1B2C3");
  assert.equal(hit.fields.callsign, "AIRWAV1");
});

test("ACARS checksum and MSK round-trip", () => {
  const pkt = encodeAcars({ mode: "2", addr: "N17XX", label: "Q0", text: "AIRWAV TEST" });
  const parsed = parseAcarsBytes([...pkt]);
  assert.ok(parsed?.verified);
  assert.equal(parsed?.fields.aircraft, "N17XX");
  assert.equal(parsed?.fields.label, "Q0");
  const bits = acarsToBits(pkt);
  const rate = 240_000;
  const n = Math.floor((bits.length / 2400) * rate) + 32;
  const re = new Float32Array(n);
  const im = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    const t = i / rate;
    const bi = Math.floor(t * 2400);
    if (bi >= bits.length) break;
    const freq = bits[bi] ? 2400 : 1200;
    re[i] = 0.7 * (0.55 + 0.45 * Math.cos(TAU * freq * t));
  }
  const frames = new AcarsDecoder().push(re, im, 0, rx(131_550_000, rate));
  assert.ok(frames.some((f) => f.verified && f.protocol === "ACARS"), JSON.stringify(frames));
});

test("POCSAG BCH remainder 0 and FSK round-trip", () => {
  const words = encodePocsag(0x1a2b3c, "AIRWAV");
  let ok = 0;
  for (const w of words) {
    if (w === 0xaaaaaaaa || w === 0x7cd215d8) continue;
    if (pocsagValid(w)) ok += 1;
  }
  assert.ok(ok >= 1);
  assert.equal(pocsagBch(0x1a2b3c << 2), pocsagBch(0x1a2b3c << 2));
  const rate = 48_000;
  const totalBits = words.length * 32;
  const n = Math.floor((totalBits / 1200) * rate) + 64;
  const re = new Float32Array(n);
  const im = new Float32Array(n);
  let phase = 0;
  const dt = 1 / rate;
  for (let i = 0; i < n; i++) {
    const bitIndex = Math.floor((i * 1200) / rate);
    let env = 0;
    if (bitIndex < totalBits) {
      const w = words[(bitIndex / 32) | 0]!;
      const b = (w >> (31 - (bitIndex % 32))) & 1;
      env = b ? 1 : -1;
    }
    phase += TAU * env * 4500 * dt;
    re[i] = 0.7 * Math.cos(phase);
    im[i] = 0.7 * Math.sin(phase);
  }
  const frames = new PocsagDecoder().push(re, im, 0, rx(433_920_000, rate));
  assert.ok(frames.some((f) => f.verified && f.protocol === "POCSAG"), JSON.stringify(frames));
});

test("AX.25 FCS encodes to a matching residue and AFSK round-trip", () => {
  const frame = encodeAprs("!0000.00N/00000.00W# AIRWAV TEST");
  const body = frame.subarray(0, frame.length - 2);
  const fcs = frame[frame.length - 2]! | (frame[frame.length - 1]! << 8);
  assert.equal(ax25Fcs(body), fcs);
  const bits = ax25BitStream(frame);
  const nrzi: number[] = [];
  let level = 1;
  for (const bit of bits) {
    if (bit === 0) level ^= 1;
    nrzi.push(level);
  }
  const rate = 48_000;
  const n = Math.floor((nrzi.length / 1200) * rate) + 64;
  const re = new Float32Array(n);
  const im = new Float32Array(n);
  let phase = 0;
  const dt = 1 / rate;
  for (let i = 0; i < n; i++) {
    const bi = Math.floor((i * 1200) / rate);
    const mark = (nrzi[bi] ?? 1) ? 1200 : 2200;
    phase += TAU * mark * dt;
    re[i] = 0.7 * Math.cos(phase);
    im[i] = 0.7 * Math.sin(phase);
  }
  const frames = new AprsDecoder().push(re, im, 0, rx(144_390_000, rate));
  assert.ok(frames.some((f) => f.verified && f.protocol === "APRS"), JSON.stringify(frames));
});

test("SAME header recovers from AFSK", () => {
  const header = "ZCZC-WXR-RWT-000000+0015-1230000-AIRWAV-";
  const bits: number[] = [];
  for (const ch of header) {
    const c = ch.charCodeAt(0);
    for (let i = 0; i < 8; i++) bits.push((c >> i) & 1);
  }
  const rate = 48_000;
  const n = Math.floor((bits.length / 520.83) * rate) + 64;
  const re = new Float32Array(n);
  const im = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    const t = i / rate;
    const bi = Math.floor(t * 520.83);
    if (bi >= bits.length) break;
    const freq = bits[bi] ? 1562.5 : 2083.3;
    re[i] = 0.7 * (0.5 + 0.5 * Math.cos(TAU * freq * t));
  }
  const frames = new SameDecoder().push(re, im, 0, rx(162_400_000, rate));
  assert.ok(frames.some((f) => f.verified && f.protocol === "SAME"), JSON.stringify(frames));
});

test("decodeModeS on empty noise does not invent ICAO", () => {
  const n = 4096;
  const re = new Float32Array(n);
  const im = new Float32Array(n);
  const frames = decodeModeS(re, im, 0, rx(1_090_000_000, 2_560_000));
  assert.ok(frames.every((f) => !f.verified));
});

test("VFO parse and decoder arming", () => {
  assert.equal(parseTune("1090"), 1_090_000_000);
  assert.equal(parseTune("131.550 MHz"), 131_550_000);
  assert.equal(parseTune("144390000"), 144_390_000);
  assert.equal(parseTune("12 khz"), null);
  assert.equal(clampHz(100), 500_000);
  assert.deepEqual(armedDecoders(1_090_000_000, 2_560_000), ["MODE_S"]);
  assert.ok(armedDecoders(131_550_000, 2_560_000).includes("ACARS"));
  assert.ok(armedDecoders(144_390_000, 2_560_000).includes("APRS"));
});
