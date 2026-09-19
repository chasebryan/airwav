/** Synthetic DEMO FIXTURE listen tone. Not V4 IQ demodulation. */

export interface ListenState {
  active: boolean;
  mode: number;
  volume: number;
  snrDb: number;
  fading: boolean;
}

let ctx: AudioContext | null = null;
let carrier: OscillatorNode | null = null;
let master: GainNode | null = null;
let lfo: OscillatorNode | null = null;
let amDepth: GainNode | null = null;
let fmDepth: GainNode | null = null;
let started = false;

function graph(): AudioContext | null {
  if (typeof window === "undefined") return null;
  const AC = window.AudioContext || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!AC) return null;
  if (!ctx || ctx.state === "closed") {
    ctx = new AC();
    started = false;
  }
  if (!started && ctx) {
    carrier = ctx.createOscillator();
    master = ctx.createGain();
    lfo = ctx.createOscillator();
    amDepth = ctx.createGain();
    fmDepth = ctx.createGain();
    carrier.type = "sine";
    lfo.type = "sine";
    carrier.frequency.value = 680;
    lfo.frequency.value = 5;
    master.gain.value = 0;
    amDepth.gain.value = 0;
    fmDepth.gain.value = 0;
    lfo.connect(amDepth);
    amDepth.connect(master.gain);
    lfo.connect(fmDepth);
    fmDepth.connect(carrier.frequency);
    carrier.connect(master);
    master.connect(ctx.destination);
    carrier.start();
    lfo.start();
    started = true;
  }
  return ctx;
}

/** Call from a click or key handler. AudioContext stays suspended until a user gesture. */
export function armListen(): void {
  const ac = graph();
  if (ac && ac.state !== "running") void ac.resume();
}

export function setListen(state: ListenState | null): void {
  if (!state || !state.active) {
    if (!ctx || !master || !amDepth || !fmDepth) return;
    const now = ctx.currentTime;
    master.gain.setTargetAtTime(0, now, 0.04);
    amDepth.gain.setTargetAtTime(0, now, 0.04);
    fmDepth.gain.setTargetAtTime(0, now, 0.04);
    return;
  }
  const ac = graph();
  if (!ac || !carrier || !master || !lfo || !amDepth || !fmDepth) return;
  if (ac.state === "suspended") void ac.resume();
  const snr = Math.max(0, Math.min(1, (state.snrDb - 3) / 28));
  const fade = state.fading ? 0.35 : 1;
  const level = (state.volume / 100) * (0.28 + snr * 0.32) * fade;
  const now = ac.currentTime;
  const mode = state.mode % 3;
  carrier.frequency.setTargetAtTime(mode === 1 ? 420 : mode === 2 ? 880 : 680, now, 0.05);
  lfo.frequency.setTargetAtTime(mode === 1 ? 3.5 : mode === 2 ? 8 : 5, now, 0.05);
  master.gain.setTargetAtTime(level, now, 0.05);
  amDepth.gain.setTargetAtTime(mode === 0 ? level * 0.45 : 0, now, 0.05);
  fmDepth.gain.setTargetAtTime(mode === 1 ? 36 : mode === 2 ? 12 : 0, now, 0.05);
}

export function stopListen(): void {
  setListen(null);
}
