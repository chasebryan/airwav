import { create } from "zustand";
import {
  BANDS,
  BLOCK_SAMPLES,
  THEME_NAMES,
  WATERFALL_ROWS,
  type BandId,
  type CapturedEvent,
  type DecodedFrame,
  type DecoderKey,
  type LogEntry,
  type Metrics,
  type OverlayView,
  type ProtocolId,
  type ReceiverConfig,
  type Snapshot,
  type SourceKind,
  type ThemeName,
  activityOf,
} from "./types";
import { Detector, IqRing, SpectrumEngine } from "./dsp";
import { DecoderHost, annotateIslands } from "./decode";
import { generateIq, newPhaseState, sceneForHz, type PhaseState, type Seed } from "./fixture";
import { clampHz, parseTune, protocolHint, scanList, stepHz } from "./tune";

const LIBRARY_KEY = "airwav.library.v1";
const THEME_KEY = "airwav.theme";
const TUNE_KEY = "airwav.centerHz";
const SNAPSHOT_HZ = 12;

function loadLibrary(): CapturedEvent[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(LIBRARY_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as CapturedEvent[];
    return Array.isArray(parsed) ? parsed.slice(-64) : [];
  } catch {
    return [];
  }
}

function persistLibrary(events: CapturedEvent[]): void {
  try {
    localStorage.setItem(LIBRARY_KEY, JSON.stringify(events.slice(-64)));
  } catch {
    /* quota */
  }
}

function loadTheme(): ThemeName {
  if (typeof localStorage === "undefined") return "Midnight";
  const t = localStorage.getItem(THEME_KEY);
  return THEME_NAMES.includes(t as ThemeName) ? (t as ThemeName) : "Midnight";
}

function loadCenter(): number {
  if (typeof localStorage === "undefined") return 136_000_000;
  const n = Number(localStorage.getItem(TUNE_KEY));
  return Number.isFinite(n) ? clampHz(n) : 136_000_000;
}

interface Engine {
  fft: SpectrumEngine;
  detector: Detector;
  ring: IqRing;
  decoder: DecoderHost;
  seed: Seed;
  phase: PhaseState;
  sample: number;
  lastTick: number;
  acc: number;
  sceneTime: number;
  fileCursor: number;
}

function applyEnabled(
  enabled?: Record<DecoderKey, boolean>,
): Engine["decoder"]["enabled"] | undefined {
  if (!enabled) return undefined;
  return {
    MODE_S: enabled.MODE_S,
    ADS_B: enabled.MODE_S,
    ACARS: enabled.ACARS,
    POCSAG: enabled.POCSAG,
    APRS: enabled.APRS,
    SAME: enabled.SAME,
  };
}

function makeEngine(
  fftSize: number,
  snr: number,
  sampleRate: number,
  ringBytes: number,
  enabled?: Record<DecoderKey, boolean>,
): Engine {
  const decoder = new DecoderHost();
  const mapped = applyEnabled(enabled);
  if (mapped) decoder.enabled = mapped;
  return {
    fft: new SpectrumEngine(fftSize),
    detector: new Detector(snr, sampleRate),
    ring: new IqRing(ringBytes),
    decoder,
    seed: { v: 7 },
    phase: newPhaseState(),
    sample: 0,
    lastTick: 0,
    acc: 0,
    sceneTime: 0,
    fileCursor: 0,
  };
}

export interface AirwavState {
  booted: boolean;
  theme: ThemeName;
  band: BandId;
  centerHz: number;
  sampleRate: number;
  gainTenthDb: number | null;
  ppm: number;
  tuneStepHz: number;
  scanning: boolean;
  fftSize: number;
  snrDb: number;
  peakHold: boolean;
  source: string;
  sourceKind: SourceKind;
  status: string;
  recording: boolean;
  recordStartedAt: number | null;
  captureActive: boolean;
  captureUntilSample: number | null;
  paused: boolean;
  demo: boolean;
  overlay: OverlayView;
  selected: number;
  zoom: number;
  pan: number;
  hoverHz: number | null;
  hoverDbfs: number | null;
  hoverFrac: number;
  dragStart: number | null;
  lockedHz: number | null;
  audioActive: boolean;
  audioMode: number;
  audioVolume: number;
  sortSnr: boolean;
  hideFading: boolean;
  quitArmedAt: number | null;
  snapshot: Snapshot | null;
  history: Float32Array[];
  peak: Float32Array | null;
  events: CapturedEvent[];
  frames: DecodedFrame[];
  decoderEnabled: Record<DecoderKey, boolean>;
  logs: LogEntry[];
  replay: boolean;
  replayIndex: number;
  replayBuffer: Snapshot[];
  speed: number;
  running: boolean;
  focused: number;
  engine: Engine | null;
  fileIq: Uint8Array | null;
  fileName: string | null;
  boot: () => void;
  start: () => void;
  stop: () => void;
  tick: (now: number) => void;
  setTheme: (name: ThemeName) => void;
  cycleTheme: () => void;
  setBand: (id: BandId) => void;
  setCenter: (hz: number, reason?: string) => void;
  stepTune: (dir: -1 | 1) => void;
  setTuneStep: (hz: number) => void;
  parseAndTune: (input: string) => boolean;
  tuneToCursor: () => void;
  tuneToSelected: () => void;
  toggleScan: () => void;
  setGain: (v: number | null) => void;
  setPpm: (v: number) => void;
  setSampleRate: (v: number) => void;
  loadIqFile: (name: string, data: ArrayBuffer) => void;
  clearFile: () => void;
  toggleDecoder: (id: DecoderKey) => void;
  setFftSize: (n: number) => void;
  setSnr: (n: number) => void;
  setPeakHold: (v: boolean) => void;
  setOverlay: (v: OverlayView) => void;
  select: (delta: number) => void;
  selectIndex: (i: number) => void;
  jumpVisible: (delta: number | "home" | "end") => void;
  setZoom: (z: number) => void;
  setPan: (p: number) => void;
  setHover: (hz: number | null, dbfs: number | null, frac?: number) => void;
  setDragStart: (frac: number | null) => void;
  applyDrag: (frac: number) => void;
  selectNearest: (hz: number) => void;
  zoomAt: (factor: number, frac: number) => void;
  zoomToSelected: () => void;
  toggleAudio: () => void;
  cycleAudioMode: () => void;
  setAudioVolume: (v: number) => void;
  toggleSort: () => void;
  toggleHideFading: () => void;
  toggleLock: () => void;
  toggleRecord: () => void;
  captureIq: () => void;
  togglePause: () => void;
  toggleDemo: () => void;
  toggleReplay: () => void;
  setSpeed: (s: number) => void;
  stepReplay: () => void;
  jumpEvent: (dir: -1 | 1) => void;
  setFocused: (n: number) => void;
  cycleFocus: (dir?: 1 | -1) => void;
  requestQuit: () => void;
  log: (level: LogEntry["level"], message: string) => void;
  resetSession: () => void;
}

function receiverOf(s: { centerHz: number; sampleRate: number; gainTenthDb: number | null; ppm: number }): ReceiverConfig {
  return {
    centerHz: s.centerHz,
    sampleRate: s.sampleRate,
    gainTenthDb: s.gainTenthDb,
    ppm: s.ppm,
    biasTee: false,
  };
}

function ringBytes(sampleRate: number): number {
  return BLOCK_SAMPLES * 2 * SNAPSHOT_HZ * 5;
}

function bandFor(hz: number): BandId {
  const hit = BANDS.reduce(
    (best, b) => (Math.abs(b.centerHz - hz) < Math.abs(best.centerHz - hz) ? b : best),
    BANDS[0]!,
  );
  return Math.abs(hit.centerHz - hz) < 2e6 ? hit.id : "vhf-air";
}

let raf = 0;
let scanAcc = 0;

export const useAirwav = create<AirwavState>((set, get) => ({
  booted: false,
  theme: loadTheme(),
  band: bandFor(loadCenter()),
  centerHz: loadCenter(),
  sampleRate: 2_560_000,
  gainTenthDb: null,
  ppm: 0,
  tuneStepHz: 25_000,
  scanning: false,
  fftSize: 2048,
  snrDb: 12,
  peakHold: true,
  source: "SYNTHETIC IQ",
  sourceKind: "synthetic",
  status: "Waiting for IQ…",
  recording: false,
  recordStartedAt: null,
  captureActive: false,
  captureUntilSample: null,
  paused: false,
  demo: false,
  overlay: null,
  selected: 0,
  zoom: 1,
  pan: 0.5,
  hoverHz: null,
  hoverDbfs: null,
  hoverFrac: 0.5,
  dragStart: null,
  lockedHz: null,
  audioActive: false,
  audioMode: 0,
  audioVolume: 50,
  sortSnr: false,
  hideFading: false,
  quitArmedAt: null,
  snapshot: null,
  history: [],
  peak: null,
  events: loadLibrary(),
  frames: [],
  decoderEnabled: {
    MODE_S: true,
    ACARS: true,
    POCSAG: true,
    APRS: true,
    SAME: true,
  },
  logs: [
    {
      t: Date.now(),
      level: "info",
      message: "AIRWAV 0.2 · VFO tuner + CRC-gated decoders. Observe first. Conclude second.",
    },
    {
      t: Date.now(),
      level: "info",
      message: "Type a frequency, step the VFO, or Shift-click the spectrum to retune.",
    },
  ],
  replay: false,
  replayIndex: 0,
  replayBuffer: [],
  speed: 1,
  running: false,
  focused: 0,
  engine: null,
  fileIq: null,
  fileName: null,

  boot: () => set({ booted: true }),

  start: () => {
    const s = get();
    if (s.running) return;
    const eng = makeEngine(s.fftSize, s.snrDb, s.sampleRate, ringBytes(s.sampleRate), s.decoderEnabled);
    set({
      running: true,
      engine: eng,
      status: `Observing ${protocolHint(s.centerHz)}`,
    });
    const loop = (now: number) => {
      get().tick(now);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
  },

  stop: () => {
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
    set({ running: false });
  },

  tick: (now: number) => {
    const s = get();
    if (!s.running || !s.engine) return;
    if (s.paused && !s.replay) {
      s.engine.lastTick = now;
      return;
    }
    if (s.replay) {
      if (s.engine.lastTick === 0) s.engine.lastTick = now;
      const dt = (now - s.engine.lastTick) * s.speed;
      if (dt < 1000 / SNAPSHOT_HZ) return;
      s.engine.lastTick = now;
      if (!s.replayBuffer.length) return;
      const idx = s.replayIndex % s.replayBuffer.length;
      const snap = s.replayBuffer[idx]!;
      applySnapshot(set, get, snap, false);
      set({ replayIndex: idx + 1, status: `Replay ${s.speed.toFixed(2)}× · frame ${idx + 1}` });
      return;
    }
    const eng = s.engine;
    if (eng.lastTick === 0) eng.lastTick = now;
    const dt = Math.min(0.08, (now - eng.lastTick) / 1000);
    eng.lastTick = now;
    eng.acc += dt;
    const interval = 1 / SNAPSHOT_HZ;
    if (eng.acc < interval) return;
    eng.acc = 0;

    if (s.scanning) {
      scanAcc += interval;
      if (scanAcc >= 1.15) {
        scanAcc = 0;
        const list = scanList(s.centerHz);
        const i = list.indexOf(s.centerHz);
        const next = list[(i + 1) % list.length] ?? s.centerHz;
        get().setCenter(next, "scan");
        return;
      }
    }

    const rx = receiverOf(s);
    const scene = sceneForHz(s.centerHz);
    const t0 = performance.now();
    const gainScale = s.gainTenthDb === null ? 1 : Math.min(1.4, Math.max(0.15, s.gainTenthDb / 400));
    let bytes: Uint8Array;
    if (s.sourceKind === "file" && s.fileIq && s.fileIq.length >= BLOCK_SAMPLES * 2) {
      const need = BLOCK_SAMPLES * 2;
      if (eng.fileCursor + need > s.fileIq.length) eng.fileCursor = 0;
      bytes = s.fileIq.subarray(eng.fileCursor, eng.fileCursor + need);
      eng.fileCursor += need;
    } else {
      bytes = generateIq(
        eng.sample,
        BLOCK_SAMPLES,
        rx.sampleRate,
        scene,
        eng.seed,
        eng.sceneTime,
        rx.ppm,
        gainScale,
        eng.phase,
      );
    }
    eng.sceneTime += interval;
    eng.ring.push(eng.sample, bytes);
    const spectrum = eng.fft.push(bytes, eng.sample, rx);
    const decoded = eng.decoder.push(bytes, eng.sample, rx);
    eng.sample += BLOCK_SAMPLES;
    if (!spectrum) return;
    const islands = annotateIslands(eng.detector.update(spectrum), decoded);
    for (const f of decoded) {
      const near = islands.find((isl) => Math.abs(isl.centerHz - f.frequencyHz) < Math.max(isl.bandwidthHz, 80_000));
      if (near) f.islandId = near.id;
    }
    const dspUs = Math.round((performance.now() - t0) * 1000);
    const verified = decoded.filter((f) => f.verified).length;
    const metrics: Metrics = {
      receivedSamples: eng.sample,
      queueDroppedSamples: 0,
      processedSamples: eng.sample,
      discontinuities: eng.fft.discontinuities,
      dspUs,
      ringBytes: eng.ring.bytes,
      ringCapacityBytes: eng.ring.capacity,
      storageDroppedSnapshots: 0,
      storageDroppedIqSamples: 0,
      islandCandidatesOmitted: eng.detector.candidatesOmitted,
      frames: (s.snapshot?.metrics.frames ?? 0) + 1,
      decodedFrames: (s.snapshot?.metrics.decodedFrames ?? 0) + decoded.length,
      verifiedFrames: (s.snapshot?.metrics.verifiedFrames ?? 0) + verified,
    };
    const snap: Snapshot = {
      timestampNs: Math.round(now * 1e6),
      receiver: rx,
      spectrum,
      islands,
      metrics,
      frames: decoded,
    };
    applySnapshot(set, get, snap, true);

    if (decoded.length) {
      const known = new Set(s.frames.map((f) => f.rawHex));
      const frames = [...s.frames];
      for (const f of decoded) {
        if (known.has(f.rawHex)) continue;
        known.add(f.rawHex);
        frames.push(f);
        if (f.verified) {
          get().log(
            "event",
            `${f.protocol} verified · ${Object.entries(f.fields)
              .slice(0, 3)
              .map(([k, v]) => `${k} ${v}`)
              .join(" · ")}`,
          );
        }
      }
      set({ frames: frames.slice(-128) });
    }

    const proto = islands.find((i) => i.verified)?.protocol ?? "UNKNOWN";
    let status = `${formatElapsed(eng.sample, rx.sampleRate)} · ${(rx.centerHz / 1e6).toFixed(6)} MHz · ${scene.description}`;
    if (proto !== "UNKNOWN") status += ` · ${proto}`;
    if (s.scanning) status = `SCAN · ${status}`;
    if (s.recording && s.recordStartedAt) {
      status = `Recording ${((Date.now() - s.recordStartedAt) / 1000).toFixed(1)}s · ${status}`;
    }
    if (s.captureActive && s.captureUntilSample !== null) {
      if (eng.sample >= s.captureUntilSample) {
        set({ captureActive: false, captureUntilSample: null });
        get().log("event", "Post-trigger IQ collection complete.");
      } else {
        status = "EVENT IQ · COLLECTING post-roll";
      }
    }
    set({
      status,
      source: s.sourceKind === "file" ? `FILE ${s.fileName ?? "iq"}` : "SYNTHETIC IQ",
    });
  },

  setTheme: (name) => {
    try {
      localStorage.setItem(THEME_KEY, name);
    } catch {
      /* ignore */
    }
    set({ theme: name });
  },
  cycleTheme: () => {
    const i = THEME_NAMES.indexOf(get().theme);
    get().setTheme(THEME_NAMES[(i + 1) % THEME_NAMES.length]!);
  },
  setBand: (id) => {
    const band = BANDS.find((b) => b.id === id);
    if (!band) return;
    set({ band: id });
    get().setCenter(band.centerHz, band.label);
  },
  setCenter: (hz, reason = "tune") => {
    const centerHz = clampHz(hz);
    try {
      localStorage.setItem(TUNE_KEY, String(centerHz));
    } catch {
      /* ignore */
    }
    const s = get();
    const eng = makeEngine(s.fftSize, s.snrDb, s.sampleRate, ringBytes(s.sampleRate), s.decoderEnabled);
    set({
      centerHz,
      band: bandFor(centerHz),
      engine: eng,
      history: [],
      peak: null,
      snapshot: null,
      selected: 0,
      lockedHz: null,
      zoom: 1,
      pan: 0.5,
      audioActive: false,
      status: `Retuned ${(centerHz / 1e6).toFixed(6)} MHz · new epoch · ${protocolHint(centerHz)}`,
    });
    get().log("info", `Tuner ${reason}: ${(centerHz / 1e6).toFixed(6)} MHz. DSP/decoder state reset.`);
  },
  stepTune: (dir) => {
    const s = get();
    get().setCenter(stepHz(s.centerHz, s.tuneStepHz, dir), dir === 1 ? "step+" : "step-");
  },
  setTuneStep: (hz) => set({ tuneStepHz: hz }),
  parseAndTune: (input) => {
    const hz = parseTune(input);
    if (hz === null) {
      get().log("warn", `Cannot parse frequency "${input}".`);
      return false;
    }
    get().setCenter(hz, "entry");
    return true;
  },
  tuneToCursor: () => {
    const hz = get().hoverHz;
    if (hz === null) return;
    get().setCenter(hz, "cursor");
  },
  tuneToSelected: () => {
    const s = get();
    const island = s.snapshot?.islands[s.selected];
    if (!island) return;
    get().setCenter(island.centerHz, "island");
  },
  toggleScan: () => {
    const scanning = !get().scanning;
    scanAcc = 0;
    set({ scanning });
    get().log("info", scanning ? "Scan started." : "Scan stopped.");
  },
  setGain: (v) => {
    set({ gainTenthDb: v });
    get().log("info", v === null ? "Gain AUTO" : `Gain ${(v / 10).toFixed(1)} dB`);
  },
  setPpm: (v) => set({ ppm: Math.max(-200, Math.min(200, Math.round(v))) }),
  setSampleRate: (v) => {
    const s = get();
    const eng = makeEngine(s.fftSize, s.snrDb, v, ringBytes(v), s.decoderEnabled);
    set({ sampleRate: v, engine: eng, history: [], peak: null });
    get().log("info", `Sample rate ${(v / 1e6).toFixed(3)} MS/s`);
  },
  loadIqFile: (name, data) => {
    const bytes = new Uint8Array(data);
    if (bytes.length < 16) {
      get().log("warn", "IQ file too small.");
      return;
    }
    const s = get();
    const eng = makeEngine(s.fftSize, s.snrDb, s.sampleRate, ringBytes(s.sampleRate), s.decoderEnabled);
    set({
      fileIq: bytes,
      fileName: name,
      sourceKind: "file",
      source: `FILE ${name}`,
      engine: eng,
      history: [],
      peak: null,
      snapshot: null,
    });
    get().log("event", `Loaded ${name} (${(bytes.length / 2).toFixed(0)} complex samples). Center is operator-labeled.`);
  },
  clearFile: () => {
    const s = get();
    const eng = makeEngine(s.fftSize, s.snrDb, s.sampleRate, ringBytes(s.sampleRate), s.decoderEnabled);
    set({
      fileIq: null,
      fileName: null,
      sourceKind: "synthetic",
      source: "SYNTHETIC IQ",
      engine: eng,
      history: [],
      peak: null,
    });
    get().log("info", "Returned to synthetic IQ source.");
  },
  toggleDecoder: (id) => {
    const s = get();
    const decoderEnabled = { ...s.decoderEnabled, [id]: !s.decoderEnabled[id] };
    if (s.engine) {
      s.engine.decoder.enabled = {
        ...s.engine.decoder.enabled,
        [id]: decoderEnabled[id],
        ADS_B: decoderEnabled.MODE_S,
      };
      if (id === "MODE_S") s.engine.decoder.reset();
    }
    set({ decoderEnabled });
    get().log("info", `${id} decoder ${decoderEnabled[id] ? "enabled" : "disabled"}.`);
  },
  setFftSize: (n) => {
    const s = get();
    const eng = makeEngine(n, s.snrDb, s.sampleRate, ringBytes(s.sampleRate), s.decoderEnabled);
    set({ fftSize: n, engine: eng, history: [], peak: null });
    get().log("info", `FFT size ${n}`);
  },
  setSnr: (n) => {
    const s = get();
    if (s.engine) s.engine.detector.threshold = n;
    set({ snrDb: n });
  },
  setPeakHold: (v) => set({ peakHold: v }),
  setOverlay: (v) => set({ overlay: v }),
  select: (delta) => get().jumpVisible(delta),
  selectIndex: (i) => {
    const count = get().snapshot?.islands.length ?? 0;
    if (!count) return;
    const selected = Math.max(0, Math.min(count - 1, i));
    set({ selected });
    panToIsland(set, get, selected);
  },
  jumpVisible: (delta) => {
    const s = get();
    const vis = visibleIndices(s);
    if (!vis.length) return;
    const cur = vis.indexOf(s.selected);
    let next = cur < 0 ? 0 : cur;
    if (delta === "home") next = 0;
    else if (delta === "end") next = vis.length - 1;
    else next = Math.max(0, Math.min(vis.length - 1, next + delta));
    const selected = vis[next]!;
    set({ selected });
    panToIsland(set, get, selected);
  },
  setZoom: (z) => set({ zoom: Math.max(1, Math.min(16, z)) }),
  setPan: (p) => set({ pan: Math.max(0, Math.min(1, p)) }),
  setHover: (hz, dbfs, frac) =>
    set({
      hoverHz: hz,
      hoverDbfs: dbfs,
      hoverFrac: frac === undefined ? get().hoverFrac : Math.max(0, Math.min(1, frac)),
    }),
  setDragStart: (frac) => set({ dragStart: frac }),
  applyDrag: (frac) => {
    const s = get();
    if (s.dragStart === null) return;
    const a = Math.min(s.dragStart, frac);
    const b = Math.max(s.dragStart, frac);
    const n = s.snapshot?.spectrum.powerDbfs.length ?? 0;
    if (!n) {
      const width = Math.max(0.04, b - a);
      set({ zoom: Math.min(16, 1 / width), pan: (a + b) / 2, dragStart: null });
      return;
    }
    const view = Math.max(1, n / s.zoom);
    const start = s.pan * n - view / 2;
    const startBin = start + a * view;
    const endBin = start + b * view;
    const newWidth = Math.max(1, endBin - startBin);
    set({
      zoom: Math.max(1, Math.min(16, n / newWidth)),
      pan: Math.max(0, Math.min(1, (startBin + endBin) / 2 / n)),
      dragStart: null,
    });
  },
  selectNearest: (hz) => {
    const s = get();
    const vis = visibleIndices(s);
    const islands = s.snapshot?.islands ?? [];
    let best = -1;
    let dist = Infinity;
    for (const i of vis) {
      const d = Math.abs(islands[i]!.centerHz - hz);
      if (d < dist) {
        dist = d;
        best = i;
      }
    }
    if (best >= 0) set({ selected: best });
  },
  zoomAt: (factor, frac) => {
    const s = get();
    const n = s.snapshot?.spectrum.powerDbfs.length ?? 0;
    const nextZoom = Math.max(1, Math.min(16, s.zoom * factor));
    if (!n) {
      set({ zoom: nextZoom });
      return;
    }
    const width = Math.max(1, n / s.zoom);
    const start = s.pan * n - width / 2;
    const bin = start + frac * width;
    const newWidth = Math.max(1, n / nextZoom);
    const newStart = bin - frac * newWidth;
    set({
      zoom: nextZoom,
      pan: Math.max(0, Math.min(1, (newStart + newWidth / 2) / n)),
    });
  },
  zoomToSelected: () => {
    const s = get();
    const snap = s.snapshot;
    const island = snap?.islands[s.selected];
    if (!snap || !island) return;
    const span = snap.spectrum.binHz * snap.spectrum.powerDbfs.length;
    if (span <= 0) return;
    const windowHz = Math.min(span, Math.max(island.bandwidthHz * 8, span / 16));
    set({
      zoom: Math.max(1, Math.min(16, span / windowHz)),
      pan: Math.max(0, Math.min(1, (island.centerHz - snap.spectrum.startHz) / span)),
    });
  },
  toggleAudio: () => {
    const s = get();
    if (s.audioActive) {
      set({ audioActive: false });
      get().log("info", "Audio muted.");
      return;
    }
    const island = s.snapshot?.islands[s.selected];
    const hz = island?.centerHz ?? s.centerHz;
    const fading = island ? activityOf(island.state) === "FADING" : false;
    set({ audioActive: true, lockedHz: hz });
    get().log(
      fading ? "warn" : "info",
      fading
        ? "Listen locked to a FADING island — the carrier may already be gone."
        : `Listen locked ${(hz / 1e6).toFixed(6)} MHz.`,
    );
  },
  cycleAudioMode: () => {
    const next = (get().audioMode + 1) % 3;
    set({ audioMode: next });
    get().log("info", `Mode ${["AM", "FM", "NFM"][next]} (manual).`);
  },
  setAudioVolume: (v) => set({ audioVolume: Math.max(0, Math.min(100, v)) }),
  toggleSort: () => {
    set({ sortSnr: !get().sortSnr });
    clampToVisible(set, get);
  },
  toggleHideFading: () => {
    set({ hideFading: !get().hideFading });
    clampToVisible(set, get);
  },
  toggleLock: () => {
    const s = get();
    const island = s.snapshot?.islands[s.selected];
    if (s.lockedHz !== null) {
      set({ lockedHz: null });
      get().log("info", "Frequency lock released.");
      return;
    }
    if (island) {
      set({ lockedHz: island.centerHz });
      get().log("info", `Locked ${(island.centerHz / 1e6).toFixed(6)} MHz.`);
    }
  },
  toggleRecord: () => {
    const s = get();
    if (s.replay) return;
    if (s.recording) {
      set({ recording: false, recordStartedAt: null, captureActive: false, quitArmedAt: null });
      get().log("info", "Metadata recording stopped.");
      set({ status: "Recording finalized. Observation continues." });
    } else {
      set({ recording: true, recordStartedAt: Date.now() });
      get().log("event", "Metadata recording started. IQ capture requires an explicit event.");
    }
  },
  captureIq: () => {
    const s = get();
    if (s.replay) return;
    if (!s.recording) {
      get().log("warn", "Capture IQ requires an active recording.");
      set({ status: "Recording must be active to preserve IQ." });
      return;
    }
    const snap = s.snapshot;
    if (!snap || !s.engine) return;
    const island = snap.islands[s.selected];
    const event: CapturedEvent = {
      id: `AW-EVENT-${String(s.events.length + 1).padStart(6, "0")}`,
      atNs: snap.timestampNs,
      label: island
        ? `Island ${String(island.id).padStart(4, "0")} @ ${(island.centerHz / 1e6).toFixed(6)} MHz`
        : "Manual window capture",
      centerHz: island?.centerHz ?? snap.receiver.centerHz,
      islandId: island?.id ?? null,
      peakDbfs: island?.peakDbfs ?? snap.spectrum.noiseDbfs,
      snrDb: island?.snrDb ?? 0,
      samples: s.engine.ring.bytes / 2,
      spectrum: Array.from(snap.spectrum.powerDbfs),
      note: island?.verified
        ? `${island.protocol} CRC-verified at capture.`
        : "Protocol UNKNOWN at capture. IQ preserved.",
      protocol: (island?.protocol ?? "UNKNOWN") as ProtocolId,
      hex: s.frames.find((f) => f.islandId === island?.id)?.rawHex,
    };
    const events = [...s.events, event];
    persistLibrary(events);
    const post = snap.receiver.sampleRate * 10;
    set({
      events,
      captureActive: true,
      captureUntilSample: s.engine.sample + post,
    });
    get().log("event", `${event.id} preserved ring IQ and started 10 s post-roll.`);
  },
  togglePause: () => {
    const paused = !get().paused;
    set({ paused });
    get().log("info", paused ? "Presentation paused. Capture continues." : "Presentation resumed.");
  },
  toggleDemo: () => set({ demo: !get().demo }),
  toggleReplay: () => {
    const s = get();
    if (s.replay) {
      set({ replay: false, status: "Returned to live observation." });
      get().log("info", "Live observation resumed.");
      return;
    }
    if (s.history.length < 8) {
      get().log("warn", "Not enough measured frames to replay.");
      return;
    }
    const buffer: Snapshot[] = s.history
      .slice()
      .reverse()
      .map((row, i) => {
        const base = s.snapshot!;
        return {
          ...base,
          timestampNs: base.timestampNs + i,
          spectrum: { ...base.spectrum, powerDbfs: row },
        };
      });
    set({
      replay: true,
      replayBuffer: buffer,
      replayIndex: 0,
      recording: false,
      captureActive: false,
      scanning: false,
      status: "Replay of measured spectra (not re-decoded).",
    });
    get().log("info", `Replaying ${buffer.length} measured frames.`);
  },
  setSpeed: (speed) => set({ speed }),
  stepReplay: () => {
    const s = get();
    if (!s.replay || !s.replayBuffer.length) return;
    const idx = s.replayIndex % s.replayBuffer.length;
    applySnapshot(set, get, s.replayBuffer[idx]!, false);
    set({ replayIndex: idx + 1, paused: true });
  },
  jumpEvent: (dir) => {
    const s = get();
    if (!s.events.length) return;
    const i = s.events.length - 1;
    const ev = s.events[dir === 1 ? i : Math.max(0, i - 1)];
    if (!ev) return;
    get().log("info", `Jumped to ${ev.id}`);
    set({ overlay: "events" });
  },
  setFocused: (n) => set({ focused: ((n % 3) + 3) % 3 }),
  cycleFocus: (dir = 1) => set({ focused: (get().focused + dir + 3) % 3 }),
  requestQuit: () => {
    const s = get();
    if (s.recording) {
      if (s.quitArmedAt && Date.now() - s.quitArmedAt < 3000) {
        get().resetSession();
        return;
      }
      set({
        quitArmedAt: Date.now(),
        status: "Recording is active · Q again within 3s to reset the session.",
      });
      get().log("warn", "Quit armed while recording");
      return;
    }
    get().resetSession();
  },
  log: (level, message) =>
    set((st) => ({ logs: [...st.logs.slice(-199), { t: Date.now(), level, message }] })),
  resetSession: () => {
    const s = get();
    const eng = makeEngine(s.fftSize, s.snrDb, s.sampleRate, ringBytes(s.sampleRate), s.decoderEnabled);
    set({
      engine: eng,
      history: [],
      peak: null,
      snapshot: null,
      selected: 0,
      recording: false,
      recordStartedAt: null,
      captureActive: false,
      paused: false,
      replay: false,
      scanning: false,
      zoom: 1,
      pan: 0.5,
      lockedHz: null,
      overlay: null,
      quitArmedAt: null,
      audioActive: false,
      frames: [],
      status: "Session reset. Tuner epoch restarted.",
    });
    get().log("info", "Session reset. Terminal restored.");
  },
}));

type SetFn = (partial: Partial<AirwavState> | ((s: AirwavState) => Partial<AirwavState>)) => void;
type GetFn = () => AirwavState;

function visibleIndices(s: AirwavState): number[] {
  const islands = s.snapshot?.islands ?? [];
  const idx = islands
    .map((_, i) => i)
    .filter((i) => !s.hideFading || activityOf(islands[i]!.state) !== "FADING");
  if (s.sortSnr) {
    idx.sort((a, b) => islands[b]!.snrDb - islands[a]!.snrDb || islands[a]!.centerHz - islands[b]!.centerHz);
  }
  return idx;
}

function clampToVisible(set: SetFn, get: GetFn): void {
  const s = get();
  const vis = visibleIndices(s);
  if (vis.length && !vis.includes(s.selected)) set({ selected: vis[0]! });
}

function panToIsland(set: SetFn, get: GetFn, index: number): void {
  const s = get();
  const snap = s.snapshot;
  const island = snap?.islands[index];
  if (!snap || !island) return;
  const n = snap.spectrum.powerDbfs.length;
  const width = Math.max(1, Math.min(n, Math.floor(n / s.zoom)));
  const start = Math.max(0, Math.min(n - width, Math.floor(s.pan * n - width / 2)));
  const bin = (island.centerHz - snap.spectrum.startHz) / snap.spectrum.binHz;
  if (bin < start || bin >= start + width) {
    set({ pan: Math.max(0, Math.min(1, bin / n)) });
  }
}

function applySnapshot(
  set: SetFn,
  get: GetFn,
  snap: Snapshot,
  recordHistory: boolean,
): void {
  const s = get();
  let selected = s.selected;
  const old = s.snapshot?.islands[s.selected];
  if (old) {
    const next = snap.islands.findIndex((i) => i.id === old.id);
    if (next >= 0) selected = next;
  }
  selected = Math.min(selected, Math.max(0, snap.islands.length - 1));
  let history = s.history;
  if (recordHistory) {
    history = [new Float32Array(snap.spectrum.powerDbfs), ...s.history];
    if (history.length > WATERFALL_ROWS) history = history.slice(0, WATERFALL_ROWS);
  }
  let peak = s.peak;
  if (s.peakHold) {
    const n = snap.spectrum.powerDbfs.length;
    if (!peak || peak.length !== n) peak = new Float32Array(snap.spectrum.powerDbfs);
    else {
      const next = new Float32Array(n);
      for (let i = 0; i < n; i++) {
        next[i] = Math.max(peak[i]! * 0.992, snap.spectrum.powerDbfs[i]!);
      }
      peak = next;
    }
  }
  set({ snapshot: snap, history, selected, peak });
}

function formatElapsed(samples: number, rate: number): string {
  const sec = samples / rate;
  const m = Math.floor(sec / 60);
  const s = sec - m * 60;
  return `${String(m).padStart(2, "0")}:${s.toFixed(1).padStart(4, "0")}`;
}
