import {
  BLOCK_SAMPLES,
  ISLAND_BUDGET,
  type ReceiverConfig,
  type SignalIsland,
  type Spectrum,
} from "./types";

const TAU = Math.PI * 2;

export function fftRadix2(re: Float32Array, im: Float32Array): void {
  const n = re.length;
  for (let i = 1, j = 0; i < n; i++) {
    let bit = n >> 1;
    for (; j & bit; bit >>= 1) j ^= bit;
    j ^= bit;
    if (i < j) {
      const tr = re[i]!;
      re[i] = re[j]!;
      re[j] = tr;
      const ti = im[i]!;
      im[i] = im[j]!;
      im[j] = ti;
    }
  }
  for (let len = 2; len <= n; len <<= 1) {
    const ang = -TAU / len;
    const wlenRe = Math.cos(ang);
    const wlenIm = Math.sin(ang);
    const half = len >> 1;
    for (let i = 0; i < n; i += len) {
      let wRe = 1;
      let wIm = 0;
      for (let j = 0; j < half; j++) {
        const ur = re[i + j]!;
        const ui = im[i + j]!;
        const vr = re[i + j + half]! * wRe - im[i + j + half]! * wIm;
        const vi = re[i + j + half]! * wIm + im[i + j + half]! * wRe;
        re[i + j] = ur + vr;
        im[i + j] = ui + vi;
        re[i + j + half] = ur - vr;
        im[i + j + half] = ui - vi;
        const nwRe = wRe * wlenRe - wIm * wlenIm;
        wIm = wRe * wlenIm + wIm * wlenRe;
        wRe = nwRe;
      }
    }
  }
}

export function quantile(values: ArrayLike<number>, q: number): number {
  const n = values.length;
  if (n === 0) return -160;
  const sorted = Array.from(values);
  sorted.sort((a, b) => a - b);
  const i = Math.floor((n - 1) * q);
  return sorted[i] ?? -160;
}

export class SpectrumEngine {
  readonly size: number;
  readonly window: Float32Array;
  readonly normalization: number;
  pendingRe: number[] = [];
  pendingIm: number[] = [];
  pendingFirst = 0;
  expectedSample: number | null = null;
  discontinuities = 0;
  workRe: Float32Array;
  workIm: Float32Array;

  constructor(size: number) {
    if (size < 256 || size > 16384 || (size & (size - 1)) !== 0) {
      throw new Error("FFT size must be a power of two in 256..=16384");
    }
    this.size = size;
    this.window = new Float32Array(size);
    let sum = 0;
    for (let i = 0; i < size; i++) {
      const w = 0.5 - 0.5 * Math.cos((TAU * i) / size);
      this.window[i] = w;
      sum += w;
    }
    this.normalization = sum * sum;
    this.workRe = new Float32Array(size);
    this.workIm = new Float32Array(size);
  }

  reset(): void {
    this.pendingRe.length = 0;
    this.pendingIm.length = 0;
    this.expectedSample = null;
    this.discontinuities = 0;
  }

  push(
    bytes: Uint8Array,
    firstSample: number,
    receiver: ReceiverConfig,
  ): Spectrum | null {
    if (bytes.length % 2 !== 0) throw new Error("IQ requires complete I/Q pairs");
    if (receiver.sampleRate === 0) throw new Error("sample rate cannot be zero");
    if (this.expectedSample !== null && this.expectedSample !== firstSample) {
      this.pendingRe.length = 0;
      this.pendingIm.length = 0;
      this.discontinuities += 1;
    }
    const samples = bytes.length / 2;
    this.expectedSample = firstSample + samples;
    const size = this.size;
    const power = new Float32Array(size);
    let frames = 0;
    let first = firstSample;
    for (let i = 0; i < samples; i++) {
      if (this.pendingRe.length === 0) this.pendingFirst = firstSample + i;
      this.pendingRe.push((bytes[i * 2]! - 127.5) / 128);
      this.pendingIm.push((bytes[i * 2 + 1]! - 127.5) / 128);
      if (this.pendingRe.length === size) {
        if (frames === 0) first = this.pendingFirst;
        let meanRe = 0;
        let meanIm = 0;
        for (let k = 0; k < size; k++) {
          meanRe += this.pendingRe[k]!;
          meanIm += this.pendingIm[k]!;
        }
        meanRe /= size;
        meanIm /= size;
        for (let k = 0; k < size; k++) {
          const w = this.window[k]!;
          this.workRe[k] = (this.pendingRe[k]! - meanRe) * w;
          this.workIm[k] = (this.pendingIm[k]! - meanIm) * w;
        }
        fftRadix2(this.workRe, this.workIm);
        const half = size / 2;
        for (let k = 0; k < size; k++) {
          const src = (k + half) % size;
          const mag =
            this.workRe[src]! * this.workRe[src]! +
            this.workIm[src]! * this.workIm[src]!;
          power[k]! += mag / this.normalization;
        }
        this.pendingRe.length = 0;
        this.pendingIm.length = 0;
        frames += 1;
      }
    }
    if (frames === 0) return null;
    for (let k = 0; k < size; k++) {
      power[k] = 10 * Math.log10(Math.max(power[k]! / frames, 1e-16));
    }
    return {
      firstSample: first,
      binHz: receiver.sampleRate / size,
      startHz: receiver.centerHz - receiver.sampleRate / 2,
      powerDbfs: power,
      noiseDbfs: quantile(power, 0.3),
    };
  }
}

export class Detector {
  threshold: number;
  nextId = 1;
  tracked: SignalIsland[] = [];
  expirySamples: number;
  candidatesOmitted = 0;

  constructor(threshold: number, sampleRate: number) {
    this.threshold = threshold;
    this.expirySamples = sampleRate;
  }

  reset(threshold: number, sampleRate: number): void {
    this.threshold = threshold;
    this.expirySamples = sampleRate;
    this.nextId = 1;
    this.tracked = [];
    this.candidatesOmitted = 0;
  }

  update(spectrum: Spectrum): SignalIsland[] {
    const p = spectrum.powerDbfs;
    const n = p.length;
    if (n < 16) return [];
    const local: number[] = [];
    for (let i = 0; i < n; i += 128) {
      local.push(quantile(p.subarray(i, Math.min(i + 128, n)), 0.3));
    }
    const active = new Array<boolean>(n).fill(false);
    const guard = Math.max(2, Math.floor(n / 50));
    const dc = n / 2;
    for (let i = guard; i < n - guard; i++) {
      const noise = local[Math.floor(i / 128)]!;
      active[i] =
        p[i]! > noise + this.threshold && p[i]! > -100 && Math.abs(i - dc) > 1;
    }
    const original = active.slice();
    for (let i = 1; i < n - 1; i++) {
      if (original[i - 1] && original[i + 1]) active[i] = true;
    }
    const measured: SignalIsland[] = [];
    let i = 0;
    while (i < n) {
      if (!active[i]) {
        i += 1;
        continue;
      }
      const start = i;
      while (i < n && active[i]) i += 1;
      if (i - start < 2) continue;
      const end = i;
      let peak = -160;
      let total = 0;
      let centroid = 0;
      for (let j = start; j < end; j++) {
        const db = p[j]!;
        if (db > peak) peak = db;
        const lin = 10 ** (db / 10);
        total += lin;
        centroid += lin * j;
      }
      const center = spectrum.startHz + (centroid / total) * spectrum.binHz;
      const noise = local[Math.floor((start + end - 1) / 2 / 128)] ?? spectrum.noiseDbfs;
      measured.push({
        id: 0,
        firstSample: spectrum.firstSample,
        lastSample: spectrum.firstSample,
        centerHz: center,
        bandwidthHz: (end - start) * spectrum.binHz,
        peakDbfs: peak,
        snrDb: peak - noise,
        observations: 1,
        state: "LIVE",
        protocol: "UNKNOWN",
        verified: false,
      });
    }
    this.tracked = this.tracked.filter(
      (old) => spectrum.firstSample - old.lastSample <= this.expirySamples,
    );
    const used = new Array<boolean>(this.tracked.length).fill(false);
    for (const neu of measured) {
      let bestJ = -1;
      let bestDist = Infinity;
      for (let j = 0; j < this.tracked.length; j++) {
        if (used[j]) continue;
        const old = this.tracked[j]!;
        const tol = Math.max(old.bandwidthHz, neu.bandwidthHz) / 2 + 3 * spectrum.binHz;
        const dist = Math.abs(old.centerHz - neu.centerHz);
        if (dist < tol && dist < bestDist) {
          bestDist = dist;
          bestJ = j;
        }
      }
      if (bestJ >= 0) {
        const old = this.tracked[bestJ]!;
        neu.id = old.id;
        neu.firstSample = old.firstSample;
        neu.observations = old.observations + 1;
        neu.protocol = old.protocol;
        neu.verified = old.verified;
        used[bestJ] = true;
      } else {
        neu.id = this.nextId++;
      }
    }
    for (let j = 0; j < this.tracked.length; j++) {
      if (!used[j]) {
        measured.push({ ...this.tracked[j]!, state: "FADING" });
      }
    }
    measured.sort((a, b) => b.lastSample - a.lastSample || b.peakDbfs - a.peakDbfs);
    this.candidatesOmitted += Math.max(0, measured.length - ISLAND_BUDGET);
    measured.length = Math.min(measured.length, ISLAND_BUDGET);
    measured.sort((a, b) => a.centerHz - b.centerHz);
    this.tracked = measured;
    return measured;
  }
}

export class IqRing {
  blocks: { firstSample: number; bytes: Uint8Array }[] = [];
  bytes = 0;
  capacity: number;

  constructor(capacity: number) {
    this.capacity = capacity - (capacity % 2);
  }

  push(firstSample: number, data: Uint8Array): void {
    if (this.capacity === 0) return;
    const last = this.blocks[this.blocks.length - 1];
    if (last && last.firstSample + last.bytes.length / 2 !== firstSample) {
      this.blocks = [];
      this.bytes = 0;
    }
    this.bytes += data.length;
    this.blocks.push({ firstSample, bytes: data });
    while (this.bytes > this.capacity && this.blocks.length) {
      const front = this.blocks.shift()!;
      const excess = this.bytes - this.capacity;
      if (front.bytes.length <= excess) {
        this.bytes -= front.bytes.length;
      } else {
        this.bytes -= excess;
        this.blocks.unshift({
          firstSample: front.firstSample + excess / 2,
          bytes: front.bytes.subarray(excess),
        });
      }
    }
  }
}

export { BLOCK_SAMPLES };
