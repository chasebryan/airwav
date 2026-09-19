import type { DecodedFrame, ProtocolId, ReceiverConfig } from "./types";

const TAU = Math.PI * 2;
const AIS = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ##### ###############0123456789######";

function bitAt(bytes: Uint8Array, i: number): number {
  return (bytes[i >> 3]! >> (7 - (i & 7))) & 1;
}

function setBit(bytes: Uint8Array, i: number, v: number): void {
  const mask = 1 << (7 - (i & 7));
  if (v) bytes[i >> 3]! |= mask;
  else bytes[i >> 3]! &= ~mask;
}

/** Mode S CRC-24, polynomial 0xFFF409. */
export function modeSCrc(msg: Uint8Array, bits: number): number {
  const g = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1];
  const b: number[] = [];
  for (let i = 0; i < bits; i++) b.push(bitAt(msg, i));
  for (let i = 0; i < bits - 24; i++) {
    if (!b[i]) continue;
    for (let j = 0; j < g.length; j++) b[i + j]! ^= g[j]!;
  }
  let crc = 0;
  for (let i = bits - 24; i < bits; i++) crc = (crc << 1) | b[i]!;
  return crc & 0xffffff;
}

export function modeSSetCrc(msg: Uint8Array, bits: number): void {
  for (let i = bits - 24; i < bits; i++) setBit(msg, i, 0);
  const crc = modeSCrc(msg, bits);
  for (let i = 0; i < 24; i++) setBit(msg, bits - 24 + i, (crc >> (23 - i)) & 1);
}

function icaoBytes(icao: number): [number, number, number] {
  return [(icao >> 16) & 0xff, (icao >> 8) & 0xff, icao & 0xff];
}

export function encodeModeSDf11(icao: number): Uint8Array {
  const msg = new Uint8Array(7);
  msg[0] = (11 << 3) | 5;
  const [a, b, c] = icaoBytes(icao);
  msg[1] = a;
  msg[2] = b;
  msg[3] = c;
  modeSSetCrc(msg, 56);
  return msg;
}

export function encodeCallsign(callsign: string): number[] {
  const s = callsign.toUpperCase().padEnd(8, " ").slice(0, 8);
  const six: number[] = [];
  for (const ch of s) {
    let idx = AIS.indexOf(ch);
    if (idx < 0) idx = 32;
    six.push(idx & 63);
  }
  return six;
}

export function encodeModeSDf17Ident(icao: number, callsign: string): Uint8Array {
  const msg = new Uint8Array(14);
  msg[0] = (17 << 3) | 5;
  const [a, b, c] = icaoBytes(icao);
  msg[1] = a;
  msg[2] = b;
  msg[3] = c;
  const chars = encodeCallsign(callsign);
  let me = 0n;
  me |= 4n << 51n;
  me |= 1n << 48n;
  for (let i = 0; i < 8; i++) me |= BigInt(chars[i]!) << BigInt(42 - i * 6);
  for (let i = 0; i < 7; i++) msg[4 + i] = Number((me >> BigInt(48 - 8 * i)) & 0xffn);
  modeSSetCrc(msg, 112);
  return msg;
}

export function encodeModeSDf17Velocity(icao: number, gsKt: number, headingDeg: number): Uint8Array {
  const msg = new Uint8Array(14);
  msg[0] = (17 << 3) | 5;
  const [a, b, c] = icaoBytes(icao);
  msg[1] = a;
  msg[2] = b;
  msg[3] = c;
  const heading = ((headingDeg % 360) + 360) % 360;
  const ew = Math.round(gsKt * Math.sin((heading * Math.PI) / 180));
  const ns = Math.round(gsKt * Math.cos((heading * Math.PI) / 180));
  const ewAbs = Math.min(1021, Math.abs(ew) + 1);
  const nsAbs = Math.min(1021, Math.abs(ns) + 1);
  let me = 0n;
  me |= 19n << 51n;
  me |= 1n << 48n;
  if (ew < 0) me |= 1n << 45n;
  me |= BigInt(ewAbs) << 35n;
  if (ns < 0) me |= 1n << 34n;
  me |= BigInt(nsAbs) << 24n;
  for (let i = 0; i < 7; i++) msg[4 + i] = Number((me >> BigInt(48 - 8 * i)) & 0xffn);
  modeSSetCrc(msg, 112);
  return msg;
}

export function modeSChips(msg: Uint8Array): number[] {
  const bits = msg.length === 14 ? 112 : 56;
  const chips = [1, 0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0];
  for (let i = 0; i < bits; i++) {
    if (bitAt(msg, i)) chips.push(1, 0);
    else chips.push(0, 1);
  }
  return chips;
}

function magAt(mag: Float32Array, rate: number, origin: number, us: number): number {
  const idx = origin + (us * 1e-6 * rate);
  const i0 = Math.floor(idx);
  const i1 = i0 + 1;
  if (i0 < 0 || i0 >= mag.length) return 0;
  const frac = idx - i0;
  const a = mag[i0]!;
  const b = i1 < mag.length ? mag[i1]! : a;
  return a + (b - a) * frac;
}

function meanMag(mag: Float32Array, rate: number, origin: number, us0: number, us1: number): number {
  const s0 = origin + us0 * 1e-6 * rate;
  const s1 = origin + us1 * 1e-6 * rate;
  const a = Math.max(0, Math.floor(s0));
  const b = Math.min(mag.length - 1, Math.ceil(s1));
  if (b < a) return 0;
  let sum = 0;
  let n = 0;
  for (let i = a; i <= b; i++) {
    sum += mag[i]!;
    n += 1;
  }
  return n ? sum / n : 0;
}

export function parseModeSMessage(msg: Uint8Array): Omit<DecodedFrame, "id" | "atSample" | "frequencyHz" | "islandId"> | null {
  const bits = msg.length >= 14 ? 112 : 56;
  if (msg.length < bits / 8) return null;
  const crc = modeSCrc(msg, bits);
  if (msg.every((b) => b === 0)) return null;
  const df = msg[0]! >> 3;
  if (![0, 4, 5, 11, 16, 17, 18, 20, 21].includes(df)) return null;
  const icao =
    df === 17 || df === 18 || df === 11
      ? ((msg[1]! << 16) | (msg[2]! << 8) | msg[3]!).toString(16).toUpperCase().padStart(6, "0")
      : "";
  const hex = Array.from(msg.subarray(0, bits / 8))
    .map((b) => b.toString(16).toUpperCase().padStart(2, "0"))
    .join("");
  const verified = crc === 0;
  const fields: Record<string, string> = {
    DF: String(df),
    bits: String(bits),
  };
  if (icao) fields.ICAO = icao;
  let protocol: ProtocolId = "MODE_S";
  const warnings: string[] = [];
  if (!verified) warnings.push("CRC-24 failed — identity is not established.");
  if (verified && df === 17) {
    protocol = "ADS_B";
    const tc = msg[4]! >> 3;
    fields.TC = String(tc);
    if (tc >= 1 && tc <= 4) {
      let call = "";
      let acc = 0n;
      for (let i = 0; i < 7; i++) acc = (acc << 8n) | BigInt(msg[4 + i]!);
      acc &= (1n << 48n) - 1n;
      const chars = acc & ((1n << 48n) - 1n);
      for (let i = 0; i < 8; i++) {
        const six = Number((chars >> BigInt(42 - i * 6)) & 63n);
        call += AIS[six] === "#" ? " " : AIS[six] ?? "?";
      }
      fields.callsign = call.trimEnd();
      fields.type = "identification";
    } else if (tc === 19) {
      fields.type = "airborne velocity";
    } else {
      fields.type = `ME type ${tc}`;
    }
  } else if (verified && df === 11) {
    fields.type = "all-call reply";
  } else {
    fields.type = `downlink format ${df}`;
  }
  return {
    protocol,
    verified,
    confidence: verified ? "verified" : "crc-fail",
    fields,
    rawHex: hex,
    warnings,
    evidence: verified
      ? `Mode S CRC-24 remainder 0 over ${bits} bits. Polynomial 0xFFF409.`
      : `Mode S CRC-24 remainder 0x${crc.toString(16).toUpperCase().padStart(6, "0")}.`,
  };
}

export function decodeModeS(
  re: Float32Array,
  im: Float32Array,
  firstSample: number,
  rx: ReceiverConfig,
): DecodedFrame[] {
  const n = re.length;
  const mag = new Float32Array(n);
  for (let i = 0; i < n; i++) mag[i] = Math.hypot(re[i]!, im[i]!);
  const rate = rx.sampleRate;
  const chip = 0.5e-6 * rate;
  if (chip < 0.6) return [];
  const out: DecodedFrame[] = [];
  const seen = new Set<string>();
  const end = n - Math.ceil(120e-6 * rate);
  const stride = Math.max(1, Math.floor(chip / 2));
  let best = { i: -1, score: 0 };
  const scores: { i: number; score: number }[] = [];
  for (let i = 0; i < end; i += stride) {
    const high =
      magAt(mag, rate, i, 0) +
      magAt(mag, rate, i, 1.0) +
      magAt(mag, rate, i, 3.5) +
      magAt(mag, rate, i, 4.5);
    const low =
      magAt(mag, rate, i, 0.5) +
      magAt(mag, rate, i, 1.5) +
      magAt(mag, rate, i, 2.5) +
      magAt(mag, rate, i, 3.0) +
      magAt(mag, rate, i, 5.0) +
      magAt(mag, rate, i, 5.5) +
      magAt(mag, rate, i, 6.5) +
      magAt(mag, rate, i, 7.5);
    const score = high - low;
    if (score > best.score) best = { i, score };
    if (score > 0.35) scores.push({ i, score });
  }
  const candidates = scores.length ? scores.sort((a, b) => b.score - a.score).slice(0, 8) : best.i >= 0 ? [best] : [];
  for (const c of candidates) {
    if (c.score < 0.18) continue;
    const tryBits = (bits: number) => {
      const msg = new Uint8Array(bits / 8);
      for (let b = 0; b < bits; b++) {
        const t0 = 8 + b;
        const a = magAt(mag, rate, c.i, t0 + 0.25);
        const d = magAt(mag, rate, c.i, t0 + 0.75);
        setBit(msg, b, a > d ? 1 : 0);
      }
      return parseModeSMessage(msg);
    };
    let parsed = tryBits(112);
    if (!parsed || !parsed.verified) {
      const short = tryBits(56);
      if (short && (short.verified || !parsed)) parsed = short;
    }
    if (!parsed || !parsed.verified) continue;
    if (seen.has(parsed.rawHex)) continue;
    seen.add(parsed.rawHex);
    out.push({
      id: `MS-${parsed.rawHex.slice(0, 8)}`,
      atSample: firstSample + c.i,
      frequencyHz: rx.centerHz,
      islandId: null,
      ...parsed,
    });
  }
  return out;
}

const ACARS_SOH = 0x01;
const ACARS_STX = 0x02;
const ACARS_ETX = 0x03;
const ACARS_DEL = 0x7f;

function oddParity(v: number): number {
  let x = v & 0x7f;
  x ^= x >> 4;
  x ^= x >> 2;
  x ^= x >> 1;
  return (x & 1) ^ 1;
}

export function encodeAcars(opts: { mode: string; addr: string; label: string; text: string }): Uint8Array {
  const addr = opts.addr.toUpperCase().padEnd(7, " ").slice(0, 7);
  const label = opts.label.toUpperCase().padEnd(2, " ").slice(0, 2);
  const body = [
    ACARS_SOH,
    opts.mode.charCodeAt(0) & 0x7f,
    ...[...addr].map((c) => c.charCodeAt(0) & 0x7f),
    0x15,
    ...[...label].map((c) => c.charCodeAt(0) & 0x7f),
    0x02,
    ACARS_STX,
    ...[...opts.text].map((c) => c.charCodeAt(0) & 0x7f),
    ACARS_ETX,
  ];
  let bcs = 0;
  for (let i = 1; i < body.length; i++) bcs ^= body[i]!;
  body.push(bcs & 0x7f, ACARS_DEL);
  return Uint8Array.from(body);
}

export function acarsToBits(chars: Uint8Array): number[] {
  const bits: number[] = [];
  for (const ch of chars) {
    const d = ch & 0x7f;
    for (let i = 0; i < 7; i++) bits.push((d >> i) & 1);
    bits.push(oddParity(d));
  }
  return bits;
}

function bitsToAcarsBytes(bits: number[]): number[] {
  const chars: number[] = [];
  for (let i = 0; i + 8 <= bits.length; i += 8) {
    let v = 0;
    for (let b = 0; b < 7; b++) v |= bits[i + b]! << b;
    const p = bits[i + 7]!;
    if (p !== oddParity(v)) continue;
    chars.push(v);
  }
  return chars;
}

export function parseAcarsBytes(chars: number[]): Omit<DecodedFrame, "id" | "atSample" | "frequencyHz" | "islandId"> | null {
  const soh = chars.indexOf(ACARS_SOH);
  if (soh < 0) return null;
  const etx = chars.indexOf(ACARS_ETX, soh);
  if (etx < 0 || etx + 1 >= chars.length) return null;
  const slice = chars.slice(soh, etx + 1);
  let bcs = 0;
  for (let i = 1; i < slice.length; i++) bcs ^= slice[i]!;
  const got = chars[etx + 1]! & 0x7f;
  const verified = (bcs & 0x7f) === got;
  const payload = slice.slice(1);
  const mode = String.fromCharCode(payload[0] ?? 32);
  const addr = String.fromCharCode(...payload.slice(1, 8)).trim();
  const label = String.fromCharCode(...payload.slice(9, 11)).trim();
  const stx = payload.indexOf(ACARS_STX);
  const text = stx >= 0 ? String.fromCharCode(...payload.slice(stx + 1, payload.length - 1)) : "";
  return {
    protocol: "ACARS",
    verified,
    confidence: verified ? "verified" : "crc-fail",
    fields: {
      mode,
      aircraft: addr,
      label,
      text: text.slice(0, 80),
    },
    rawHex: slice.map((c) => c.toString(16).padStart(2, "0")).join(""),
    warnings: verified ? [] : ["ACARS block checksum failed."],
    evidence: verified
      ? "ACARS odd-parity characters and XOR block checksum matched."
      : "ACARS framing found; block checksum did not match.",
  };
}

class ToneBitDecoder {
  phase = 0;
  osc = 0;
  mI = 0;
  mQ = 0;
  sI = 0;
  sQ = 0;

  pushEnvelope(env: Float32Array, sampleRate: number, markHz: number, spaceHz: number, baud: number): number[] {
    const markW = TAU * markHz / sampleRate;
    const spaceW = TAU * spaceHz / sampleRate;
    const spb = sampleRate / baud;
    const out: number[] = [];
    for (let i = 0; i < env.length; i++) {
      const x = env[i]!;
      const t = this.osc++;
      this.mI += x * Math.cos(markW * t);
      this.mQ += x * Math.sin(markW * t);
      this.sI += x * Math.cos(spaceW * t);
      this.sQ += x * Math.sin(spaceW * t);
      this.phase += 1;
      if (this.phase >= spb) {
        const mark = this.mI * this.mI + this.mQ * this.mQ;
        const space = this.sI * this.sI + this.sQ * this.sQ;
        out.push(mark > space ? 1 : 0);
        this.phase -= spb;
        this.mI = this.mQ = this.sI = this.sQ = 0;
      }
    }
    return out;
  }
}

class FmBitSlicer {
  acc = 0;
  n = 0;

  push(fm: Float32Array, spb: number, thresh = 0, lowIsOne = false): number[] {
    const bits: number[] = [];
    for (let i = 0; i < fm.length; i++) {
      this.acc += fm[i]!;
      this.n += 1;
      if (this.n >= spb) {
        const high = this.acc > thresh * this.n;
        bits.push(lowIsOne ? (high ? 0 : 1) : high ? 1 : 0);
        this.acc = 0;
        this.n -= spb;
      }
    }
    return bits;
  }
}


export class AcarsDecoder {
  private bits: number[] = [];
  private tones = new ToneBitDecoder();
  push(re: Float32Array, im: Float32Array, firstSample: number, rx: ReceiverConfig): DecodedFrame[] {
    const env = new Float32Array(re.length);
    let mean = 0;
    for (let i = 0; i < re.length; i++) {
      env[i] = Math.hypot(re[i]!, im[i]!);
      mean += env[i]!;
    }
    mean /= Math.max(1, re.length);
    for (let i = 0; i < env.length; i++) env[i] = env[i]! - mean;
    const bits = this.tones.pushEnvelope(env, rx.sampleRate, 2400, 1200, 2400);
    this.bits.push(...bits);
    if (this.bits.length > 8000) this.bits.splice(0, this.bits.length - 6000);
    const chars = bitsToAcarsBytes(this.bits);
    const parsed = parseAcarsBytes(chars);
    if (!parsed) return [];
    if (parsed.verified) this.bits = [];
    return [
      {
        id: `ACARS-${(firstSample & 0xffff).toString(16)}`,
        atSample: firstSample,
        frequencyHz: rx.centerHz,
        islandId: null,
        ...parsed,
      },
    ];
  }
}

/** POCSAG BCH(31,21) generator 0x769. */
export function pocsagBch(data21: number): number {
  let v = (data21 & 0x1fffff) << 10;
  const g = 0x769;
  for (let i = 30; i >= 10; i--) {
    if ((v >> i) & 1) v ^= g << (i - 10);
  }
  const code = ((data21 & 0x1fffff) << 11) | ((v & 0x3ff) << 1);
  let parity = 0;
  let x = code;
  while (x) {
    parity ^= x & 1;
    x >>>= 1;
  }
  return (code | (parity & 1)) >>> 0;
}

export function encodePocsag(address: number, text: string): Uint32Array {
  const words: number[] = [];
  for (let i = 0; i < 18; i++) words.push(0xaaaaaaaa);
  words.push(0x7cd215d8);
  const addrCw = pocsagBch(((address & 0x1ffffc) << 2) | 0);
  words.push(addrCw);
  const chars = text.toUpperCase().slice(0, 20);
  let acc = 0;
  let n = 0;
  const pushChar = (c: number) => {
    acc = (acc << 6) | (c & 63);
    n += 6;
    if (n >= 20) {
      const data = (acc >> (n - 20)) & 0xfffff;
      words.push(pocsagBch((1 << 20) | data));
      acc &= (1 << (n - 20)) - 1;
      n -= 20;
    }
  };
  for (const ch of chars) {
    const code = ch === " " ? 0x20 : ch.charCodeAt(0) & 63;
    pushChar(code);
  }
  if (n) {
    acc <<= 20 - n;
    words.push(pocsagBch((1 << 20) | (acc & 0xfffff)));
  }
  while (words.length % 16 !== 8) words.push(pocsagBch(0));
  return Uint32Array.from(words);
}

export function pocsagValid(cw: number): boolean {
  return pocsagBch(cw >>> 11) === (cw >>> 0);
}

function fmDemod(re: Float32Array, im: Float32Array): Float32Array {
  const out = new Float32Array(re.length);
  let pRe = re[0] ?? 1;
  let pIm = im[0] ?? 0;
  for (let i = 0; i < re.length; i++) {
    const r = re[i]!;
    const q = im[i]!;
    const nrm = pRe * pRe + pIm * pIm;
    out[i] = nrm > 1e-8 ? (q * pRe - r * pIm) / nrm : 0;
    pRe = r;
    pIm = q;
  }
  return out;
}

export class PocsagDecoder {
  private bits: number[] = [];
  private slicer = new FmBitSlicer();
  push(re: Float32Array, im: Float32Array, firstSample: number, rx: ReceiverConfig): DecodedFrame[] {
    const fm = fmDemod(re, im);
    const bits = this.slicer.push(fm, rx.sampleRate / 1200);
    this.bits.push(...bits);
    if (this.bits.length > 12000) this.bits.splice(0, this.bits.length - 8000);
    const sync = 0x7cd215d8;
    const stream = this.bits;
    for (let i = 0; i + 32 + 32 < stream.length; i++) {
      let v = 0;
      for (let b = 0; b < 32; b++) v = (v << 1) | stream[i + b]!;
      if (v !== sync) continue;
      const cws: number[] = [];
      let ok = 0;
      for (let w = 0; w < 8; w++) {
        const off = i + 32 + w * 32;
        if (off + 32 > stream.length) break;
        let cw = 0;
        for (let b = 0; b < 32; b++) cw = (cw << 1) | stream[off + b]!;
        cws.push(cw >>> 0);
        if (pocsagValid(cw >>> 0)) ok += 1;
      }
      if (ok < 1) continue;
      let text = "";
      for (const cw of cws) {
        if (!pocsagValid(cw)) continue;
        if (((cw >>> 31) & 1) === 0) continue;
        const data = (cw >>> 11) & 0xfffff;
        for (let k = 2; k >= 0; k--) {
          const six = (data >> (k * 6 + 2)) & 63;
          text += six === 0x20 || six === 0 ? " " : String.fromCharCode(six);
        }
      }
      this.bits = stream.slice(i + 32 + 256);
      return [
        {
          id: `POCSAG-${firstSample.toString(16)}`,
          protocol: "POCSAG",
          atSample: firstSample,
          frequencyHz: rx.centerHz,
          verified: ok >= 1,
          confidence: ok >= 1 ? "verified" : "crc-fail",
          fields: { text: text.trim(), validCodewords: String(ok) },
          rawHex: cws.map((c) => c.toString(16).padStart(8, "0")).join(""),
          warnings: [],
          evidence: `${ok} POCSAG codeword(s) with BCH(31,21)+parity remainder 0.`,
          islandId: null,
        },
      ];
    }
    return [];
  }
}

/** HDLC CRC-16-CCITT, poly 0x1021, init 0xFFFF, reflected via LSB-first bits. */
export function ax25Fcs(data: Uint8Array): number {
  let crc = 0xffff;
  for (const byte of data) {
    crc ^= byte;
    for (let i = 0; i < 8; i++) {
      if (crc & 1) crc = (crc >>> 1) ^ 0x8408;
      else crc >>>= 1;
    }
  }
  return ~crc & 0xffff;
}

function encodeAx25Address(call: string, ssid: number, last: boolean): Uint8Array {
  const c = call.toUpperCase().padEnd(6, " ").slice(0, 6);
  const out = new Uint8Array(7);
  for (let i = 0; i < 6; i++) out[i] = (c.charCodeAt(i) & 0x7f) << 1;
  out[6] = ((ssid & 0x0f) << 1) | 0x60 | (last ? 1 : 0);
  return out;
}

export function encodeAprs(info: string): Uint8Array {
  const dest = encodeAx25Address("APRS", 0, false);
  const src = encodeAx25Address("AIRWAV", 0, false);
  const rpt = encodeAx25Address("WIDE1", 1, true);
  const payload = new TextEncoder().encode(info);
  const body = new Uint8Array(7 * 3 + 2 + payload.length);
  body.set(dest, 0);
  body.set(src, 7);
  body.set(rpt, 14);
  body[21] = 0x03;
  body[22] = 0xf0;
  body.set(payload, 23);
  const fcs = ax25Fcs(body);
  const frame = new Uint8Array(body.length + 2);
  frame.set(body);
  frame[body.length] = fcs & 0xff;
  frame[body.length + 1] = (fcs >> 8) & 0xff;
  return frame;
}

export function ax25BitStream(frame: Uint8Array): number[] {
  const bits: number[] = [];
  let ones = 0;
  const pushByte = (b: number, stuff: boolean) => {
    for (let i = 0; i < 8; i++) {
      const bit = (b >> i) & 1;
      bits.push(bit);
      if (stuff) {
        if (bit) ones += 1;
        else ones = 0;
        if (ones === 5) {
          bits.push(0);
          ones = 0;
        }
      } else {
        ones = 0;
      }
    }
  };
  for (let i = 0; i < 20; i++) pushByte(0x7e, false);
  for (const b of frame) pushByte(b, true);
  pushByte(0x7e, false);
  pushByte(0x7e, false);
  return bits;
}

function nrzi(bits: number[]): number[] {
  const out: number[] = [];
  let level = 1;
  for (const b of bits) {
    if (b === 0) level ^= 1;
    out.push(level);
  }
  return out;
}

export class AprsDecoder {
  private bits: number[] = [];
  private slicer = new FmBitSlicer();
  private prevNrzi = 1;
  push(re: Float32Array, im: Float32Array, firstSample: number, rx: ReceiverConfig): DecodedFrame[] {
    const fm = fmDemod(re, im);
    const spb = rx.sampleRate / 1200;
    const mid = Math.sin((TAU * 1700) / rx.sampleRate);
    const nrziBits = this.slicer.push(fm, spb, mid, true);
    const bits: number[] = [];
    for (const level of nrziBits) {
      bits.push(level === this.prevNrzi ? 1 : 0);
      this.prevNrzi = level;
    }
    this.bits.push(...bits);
    if (this.bits.length > 16000) this.bits.splice(0, this.bits.length - 12000);
    const stream = this.bits;
    const flags: number[] = [];
    for (let i = 0; i + 8 <= stream.length; i++) {
      if (
        stream[i] === 0 &&
        stream[i + 1] === 1 &&
        stream[i + 2] === 1 &&
        stream[i + 3] === 1 &&
        stream[i + 4] === 1 &&
        stream[i + 5] === 1 &&
        stream[i + 6] === 1 &&
        stream[i + 7] === 0
      ) {
        flags.push(i);
        i += 7;
      }
    }
    for (let f = 0; f + 1 < flags.length; f++) {
      const start = flags[f]! + 8;
      const end = flags[f + 1]!;
      if (end <= start) continue;
      const destuffed: number[] = [];
      let ones = 0;
      for (const b of stream.slice(start, end)) {
        if (ones === 5 && b === 0) {
          ones = 0;
          continue;
        }
        if (b) {
          ones += 1;
          destuffed.push(1);
        } else {
          ones = 0;
          destuffed.push(0);
        }
      }
      if (destuffed.length < 18 * 8) continue;
      const bytes = new Uint8Array(Math.floor(destuffed.length / 8));
      for (let i = 0; i < bytes.length; i++) {
        let v = 0;
        for (let k = 0; k < 8; k++) v |= (destuffed[i * 8 + k]! & 1) << k;
        bytes[i] = v;
      }
      if (bytes.length < 18) continue;
      const body = bytes.subarray(0, bytes.length - 2);
      const fcs = bytes[bytes.length - 2]! | (bytes[bytes.length - 1]! << 8);
      if (ax25Fcs(body) !== fcs) continue;
      const call = (off: number) => {
        let s = "";
        for (let i = 0; i < 6; i++) s += String.fromCharCode(bytes[off + i]! >> 1);
        const ssid = (bytes[off + 6]! >> 1) & 0x0f;
        return `${s.trim()}-${ssid}`;
      };
      const info = new TextDecoder().decode(body.subarray(7 * 3 + 2));
      this.bits = [];
      return [
        {
          id: `APRS-${firstSample.toString(16)}`,
          protocol: "APRS",
          atSample: firstSample,
          frequencyHz: rx.centerHz,
          verified: true,
          confidence: "verified",
          fields: {
            from: call(7),
            to: call(0),
            info: info.slice(0, 80),
          },
          rawHex: Array.from(bytes)
            .map((x) => x.toString(16).padStart(2, "0"))
            .join(""),
          warnings: [],
          evidence: "AX.25 HDLC CRC-16-CCITT matched after destuff.",
          islandId: null,
        },
      ];
    }
    return [];
  }
}

export class SameDecoder {
  private bits: number[] = [];
  private tones = new ToneBitDecoder();
  push(re: Float32Array, im: Float32Array, firstSample: number, rx: ReceiverConfig): DecodedFrame[] {
    const env = new Float32Array(re.length);
    for (let i = 0; i < re.length; i++) env[i] = Math.hypot(re[i]!, im[i]!);
    let mean = 0;
    for (const v of env) mean += v;
    mean /= env.length || 1;
    for (let i = 0; i < env.length; i++) env[i] = env[i]! - mean;
    const bits = this.tones.pushEnvelope(env, rx.sampleRate, 1562.5, 2083.3, 520.83);
    this.bits.push(...bits);
    if (this.bits.length > 4000) this.bits.splice(0, this.bits.length - 3000);
    const bytes: number[] = [];
    for (let i = 0; i + 8 <= this.bits.length; i += 8) {
      let v = 0;
      for (let b = 0; b < 8; b++) v |= this.bits[i + b]! << b;
      bytes.push(v);
    }
    const text = String.fromCharCode(...bytes.filter((c) => c >= 32 && c < 127));
    const idx = text.indexOf("ZCZC-");
    if (idx < 0) return [];
    const end = text.indexOf("-", idx + 20);
    const header = text.slice(idx, end > idx ? end + 1 : idx + 40);
    this.bits = [];
    return [
      {
        id: `SAME-${firstSample.toString(16)}`,
        protocol: "SAME",
        atSample: firstSample,
        frequencyHz: rx.centerHz,
        verified: header.startsWith("ZCZC-"),
        confidence: header.startsWith("ZCZC-") ? "verified" : "candidate",
        fields: { header: header.slice(0, 64) },
        rawHex: header
          .split("")
          .map((c) => c.charCodeAt(0).toString(16).padStart(2, "0"))
          .join(""),
        warnings: [],
        evidence: "NOAA SAME preamble ZCZC recovered from 520.83 baud AFSK.",
        islandId: null,
      },
    ];
  }
}

export class DecoderHost {
  private acars = new AcarsDecoder();
  private pocsag = new PocsagDecoder();
  private aprs = new AprsDecoder();
  private same = new SameDecoder();
  private overlapRe = new Float32Array(0);
  private overlapIm = new Float32Array(0);
  private seq = 1;
  enabled: Record<Exclude<ProtocolId, "UNKNOWN">, boolean> = {
    MODE_S: true,
    ADS_B: true,
    ACARS: true,
    POCSAG: true,
    APRS: true,
    SAME: true,
  };

  reset(): void {
    this.acars = new AcarsDecoder();
    this.pocsag = new PocsagDecoder();
    this.aprs = new AprsDecoder();
    this.same = new SameDecoder();
    this.overlapRe = new Float32Array(0);
    this.overlapIm = new Float32Array(0);
  }

  push(bytes: Uint8Array, firstSample: number, rx: ReceiverConfig): DecodedFrame[] {
    const n = bytes.length / 2;
    const re = new Float32Array(n);
    const im = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      re[i] = (bytes[i * 2]! - 127.5) / 128;
      im[i] = (bytes[i * 2 + 1]! - 127.5) / 128;
    }
    const hold = Math.min(400, n);
    const reA = this.overlapRe.length ? concat(this.overlapRe, re) : re;
    const imA = this.overlapIm.length ? concat(this.overlapIm, im) : im;
    this.overlapRe = re.slice(n - hold);
    this.overlapIm = im.slice(n - hold);
    const origin = firstSample - (reA.length - n);
    const c = rx.centerHz;
    const out: DecodedFrame[] = [];
    if (this.enabled.MODE_S && Math.abs(c - 1_090_000_000) < rx.sampleRate) {
      out.push(...decodeModeS(reA, imA, origin, rx));
    }
    if (this.enabled.ACARS && c >= 118e6 && c <= 138e6) {
      out.push(...this.acars.push(re, im, firstSample, rx));
    }
    if (this.enabled.POCSAG && (Math.abs(c - 433.92e6) < 3e5 || (c >= 150e6 && c <= 174e6 && Math.abs(c - 162.4e6) > 2e5))) {
      out.push(...this.pocsag.push(re, im, firstSample, rx));
    }
    if (this.enabled.APRS && c >= 144e6 && c <= 148e6) {
      out.push(...this.aprs.push(re, im, firstSample, rx));
    }
    if (this.enabled.SAME && c >= 162.3e6 && c <= 162.6e6) {
      out.push(...this.same.push(re, im, firstSample, rx));
    }
    return out.map((f) => ({ ...f, id: `${f.protocol}-${this.seq++}` }));
  }
}

function concat(a: Float32Array, b: Float32Array): Float32Array {
  const o = new Float32Array(a.length + b.length);
  o.set(a);
  o.set(b, a.length);
  return o;
}

export function annotateIslands<T extends { id?: number; centerHz: number; bandwidthHz: number; protocol: string; verified: boolean }>(
  islands: T[],
  frames: DecodedFrame[],
): T[] {
  for (const island of islands) {
    const hit = frames.filter(
      (f) => f.verified && Math.abs(f.frequencyHz - island.centerHz) < Math.max(island.bandwidthHz, 80_000),
    );
    if (!hit.length) continue;
    const best = hit[hit.length - 1]!;
    island.protocol = best.protocol;
    island.verified = true;
    if (island.id != null) best.islandId = island.id;
  }
  return islands;
}

export { nrzi };
