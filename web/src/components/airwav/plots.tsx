import { useCallback, useEffect, useRef, type MouseEvent } from "react";
import { getTheme, hexToRgb, waterfallLut } from "@/lib/airwav/themes";
import { formatMhz } from "@/lib/airwav/types";
import { useAirwav } from "@/lib/airwav/store";
import { cn } from "@/lib/utils";

function viewRange(len: number, zoom: number, pan: number): [number, number] {
  const width = Math.max(1, Math.min(len, Math.floor(len / zoom)));
  const start = Math.max(0, Math.min(len - width, Math.floor(pan * len - width / 2)));
  return [start, start + width];
}

function maxIn(data: ArrayLike<number>, from: number, to: number): number {
  let v = -160;
  const end = Math.min(to, data.length);
  for (let i = from; i < end; i++) if (data[i]! > v) v = data[i]!;
  return v;
}

export function SpectrumPlot() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const theme = useAirwav((s) => s.theme);
  const snapshot = useAirwav((s) => s.snapshot);
  const peak = useAirwav((s) => s.peak);
  const peakHold = useAirwav((s) => s.peakHold);
  const zoom = useAirwav((s) => s.zoom);
  const pan = useAirwav((s) => s.pan);
  const selected = useAirwav((s) => s.selected);
  const lockedHz = useAirwav((s) => s.lockedHz);
  const dragStart = useAirwav((s) => s.dragStart);
  const focused = useAirwav((s) => s.focused);
  const setHover = useAirwav((s) => s.setHover);
  const setDragStart = useAirwav((s) => s.setDragStart);
  const applyDrag = useAirwav((s) => s.applyDrag);
  const hoverFrac = useRef(0);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== Math.floor(w * dpr) || canvas.height !== Math.floor(h * dpr)) {
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const t = getTheme(theme);
    ctx.fillStyle = t.panel;
    ctx.fillRect(0, 0, w, h);
    if (!snapshot) {
      ctx.fillStyle = t.muted;
      ctx.font = "12px IBM Plex Mono, ui-monospace, monospace";
      ctx.fillText("No received or replayed IQ.", 16, h / 2);
      return;
    }
    const p = snapshot.spectrum.powerDbfs;
    const [a, b] = viewRange(p.length, zoom, pan);
    const floor = Math.max(snapshot.spectrum.noiseDbfs - 10, -140);
    const ceiling = Math.max(-40, maxIn(p, a, b)) + 5;
    const span = Math.max(20, ceiling - floor);
    const plotH = h - 28;

    ctx.strokeStyle = t.border;
    ctx.lineWidth = 1;
    ctx.globalAlpha = 0.45;
    for (let i = 1; i < 4; i++) {
      const y = (plotH / 4) * i;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;

    const toY = (db: number) =>
      plotH - ((db - floor) / span) * plotH;

    if (peakHold && peak && peak.length === p.length) {
      ctx.beginPath();
      for (let x = 0; x < w; x++) {
        const from = a + Math.floor((x / w) * (b - a));
        const to = Math.max(from + 1, a + Math.floor(((x + 1) / w) * (b - a)));
        const y = toY(maxIn(peak, from, to));
        if (x === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.strokeStyle = t.muted;
      ctx.globalAlpha = 0.45;
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    const accent = hexToRgb(t.accent);
    ctx.beginPath();
    for (let x = 0; x < w; x++) {
      const from = a + Math.floor((x / w) * (b - a));
      const to = Math.max(from + 1, a + Math.floor(((x + 1) / w) * (b - a)));
      const y = toY(maxIn(p, from, to));
      if (x === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.lineTo(w, plotH);
    ctx.lineTo(0, plotH);
    ctx.closePath();
    ctx.fillStyle = `rgba(${accent[0]},${accent[1]},${accent[2]},0.16)`;
    ctx.fill();

    ctx.beginPath();
    for (let x = 0; x < w; x++) {
      const from = a + Math.floor((x / w) * (b - a));
      const to = Math.max(from + 1, a + Math.floor(((x + 1) / w) * (b - a)));
      const y = toY(maxIn(p, from, to));
      if (x === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.strokeStyle = t.accent;
    ctx.lineWidth = 1.4;
    ctx.stroke();

    const spec = snapshot.spectrum;
    const hzAt = (bin: number) => spec.startHz + bin * spec.binHz;
    for (let i = 0; i < snapshot.islands.length; i++) {
      const island = snapshot.islands[i]!;
      const bin = (island.centerHz - spec.startHz) / spec.binHz;
      if (bin < a || bin >= b) continue;
      const x = ((bin - a) / (b - a)) * w;
      ctx.fillStyle = i === selected ? t.selected : t.unknown;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x - 5, 8);
      ctx.lineTo(x + 5, 8);
      ctx.closePath();
      ctx.fill();
    }

    if (lockedHz !== null) {
      const bin = (lockedHz - spec.startHz) / spec.binHz;
      if (bin >= a && bin < b) {
        const x = ((bin - a) / (b - a)) * w;
        ctx.strokeStyle = t.prism;
        ctx.setLineDash([3, 3]);
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, plotH);
        ctx.stroke();
        ctx.setLineDash([]);
      }
    }

    if (dragStart !== null) {
      const x0 = dragStart * w;
      const x1 = hoverFrac.current * w;
      ctx.fillStyle = `rgba(${accent[0]},${accent[1]},${accent[2]},0.12)`;
      ctx.fillRect(Math.min(x0, x1), 0, Math.abs(x1 - x0), plotH);
    }

    ctx.fillStyle = t.muted;
    ctx.font = "11px IBM Plex Mono, ui-monospace, monospace";
    const fromHz = hzAt(a);
    const toHz = hzAt(b);
    ctx.fillText(`${formatMhz(fromHz, 3)} MHz`, 8, h - 8);
    ctx.textAlign = "center";
    ctx.fillText(`${formatMhz((fromHz + toHz) / 2, 3)} MHz`, w / 2, h - 8);
    ctx.textAlign = "right";
    ctx.fillText(`${formatMhz(toHz, 3)} MHz`, w - 8, h - 8);
    ctx.textAlign = "left";
  }, [theme, snapshot, peak, peakHold, zoom, pan, selected, lockedHz, dragStart]);

  useEffect(() => {
    draw();
    const onResize = () => draw();
    window.addEventListener("resize", onResize);
    const canvas = canvasRef.current;
    const ro = canvas ? new ResizeObserver(onResize) : null;
    if (canvas && ro) ro.observe(canvas);
    const onWheelNative = (ev: Event) => {
      const e = ev as globalThis.WheelEvent;
      e.preventDefault();
      const state = useAirwav.getState();
      if (e.shiftKey) {
        const dir = e.deltaY < 0 ? 1 : -1;
        state.setPan(state.pan + (dir * 0.1) / state.zoom);
      } else {
        const dir = e.deltaY < 0 ? 1 : -1;
        state.setZoom(state.zoom * 2 ** dir);
      }
    };
    canvas?.addEventListener("wheel", onWheelNative, { passive: false });
    return () => {
      window.removeEventListener("resize", onResize);
      ro?.disconnect();
      canvas?.removeEventListener("wheel", onWheelNative);
    };
  }, [draw]);

  const onMove = (e: MouseEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const frac = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    hoverFrac.current = frac;
    const snap = useAirwav.getState().snapshot;
    if (!snap) return;
    const [a, b] = viewRange(snap.spectrum.powerDbfs.length, zoom, pan);
    const bin = a + frac * (b - a);
    const hz = snap.spectrum.startHz + bin * snap.spectrum.binHz;
    const from = Math.floor(bin);
    const dbfs = maxIn(snap.spectrum.powerDbfs, from, from + 1);
    setHover(hz, dbfs);
    if (dragStart !== null) draw();
  };

  const onLeave = () => setHover(null, null);

  const onDown = (e: MouseEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const frac = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    setDragStart(frac);
  };

  const onUp = (e: MouseEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const frac = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const start = useAirwav.getState().dragStart;
    if (start !== null && Math.abs(frac - start) > 0.02) applyDrag(frac);
    else setDragStart(null);
  };

  return (
    <section
      className={cn(
        "relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border bg-panel",
        focused === 0 ? "border-accent" : "border-border",
      )}
    >
      <header className="flex items-center justify-between border-b border-border px-3 py-1.5">
        <h2 className="font-mono text-[10px] tracking-[0.18em] text-muted uppercase">
          01 / Spectrum · dBFS
        </h2>
        <p className="font-mono text-[10px] text-muted tabular-nums">
          {snapshot
            ? `Floor ${snapshot.spectrum.noiseDbfs.toFixed(1)} dBFS · ${(snapshot.spectrum.binHz / 1000).toFixed(1)} kHz/bin · zoom ${zoom.toFixed(0)}×`
            : "—"}
        </p>
      </header>
      <canvas
        ref={canvasRef}
        className="block h-full min-h-[140px] w-full flex-1 touch-none"
        onMouseMove={onMove}
        onMouseLeave={onLeave}
        onMouseDown={onDown}
        onMouseUp={onUp}
      />
    </section>
  );
}

export function WaterfallPlot() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const lutRef = useRef<Uint8ClampedArray | null>(null);
  const theme = useAirwav((s) => s.theme);
  const history = useAirwav((s) => s.history);
  const snapshot = useAirwav((s) => s.snapshot);
  const zoom = useAirwav((s) => s.zoom);
  const pan = useAirwav((s) => s.pan);
  const selected = useAirwav((s) => s.selected);
  const focused = useAirwav((s) => s.focused);
  const setZoom = useAirwav((s) => s.setZoom);
  const setPan = useAirwav((s) => s.setPan);

  useEffect(() => {
    lutRef.current = waterfallLut(getTheme(theme));
  }, [theme]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = 1;
    const cssW = Math.max(1, Math.floor(canvas.clientWidth));
    const cssH = Math.max(1, Math.floor(canvas.clientHeight));
    const w = Math.floor(cssW * dpr);
    const h = Math.floor(cssH * dpr);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    const t = getTheme(theme);
    const lut = lutRef.current;
    const bg = hexToRgb(t.panel);
    const img = ctx.createImageData(w, h);
    const data = img.data;
    for (let i = 0; i < data.length; i += 4) {
      data[i] = bg[0];
      data[i + 1] = bg[1];
      data[i + 2] = bg[2];
      data[i + 3] = 255;
    }
    if (lut && history.length) {
      const n = history[0]!.length;
      const [a, b] = viewRange(n, zoom, pan);
      const floor = snapshot ? snapshot.spectrum.noiseDbfs - 2 : -80;
      const rows = Math.min(history.length, h);
      for (let row = 0; row < rows; row++) {
        const spec = history[row]!;
        for (let x = 0; x < w; x++) {
          const from = a + Math.floor((x / w) * (b - a));
          const to = Math.max(from + 1, a + Math.floor(((x + 1) / w) * (b - a)));
          const power = maxIn(spec, from, to);
          const index = Math.max(0, Math.min(255, Math.round(((power - floor) / 36) * 255)));
          const off = (row * w + x) * 4;
          data[off] = lut[index * 4]!;
          data[off + 1] = lut[index * 4 + 1]!;
          data[off + 2] = lut[index * 4 + 2]!;
          data[off + 3] = 255;
        }
      }
    }
    ctx.putImageData(img, 0, 0);
    if (snapshot && history.length) {
      const spec = snapshot.spectrum;
      const n = spec.powerDbfs.length;
      const [a, b] = viewRange(n, zoom, pan);
      for (let i = 0; i < snapshot.islands.length; i++) {
        const island = snapshot.islands[i]!;
        const bin = (island.centerHz - spec.startHz) / spec.binHz;
        if (bin < a || bin >= b) continue;
        const x = ((bin - a) / (b - a)) * w;
        ctx.fillStyle = i === selected ? t.selected : t.unknown;
        ctx.fillRect(Math.floor(x), 0, 2, 10);
      }
    }
  }, [theme, history, snapshot, zoom, pan, selected]);

  return (
    <section
      className={cn(
        "relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border bg-panel",
        focused === 1 ? "border-accent" : "border-border",
      )}
    >
      <header className="flex items-center justify-between border-b border-border px-3 py-1.5">
        <h2 className="font-mono text-[10px] tracking-[0.18em] text-muted uppercase">
          02 / Waterfall · measured history
        </h2>
        <p className="font-mono text-[10px] text-muted">Newest at top · not generated texture</p>
      </header>
      <canvas
        ref={canvasRef}
        className="block h-full min-h-[140px] w-full flex-1 touch-none"
        onWheel={(e) => {
          e.preventDefault();
          if (e.shiftKey) {
            const dir = e.deltaY < 0 ? 1 : -1;
            setPan(pan + (dir * 0.1) / zoom);
          } else {
            const dir = e.deltaY < 0 ? 1 : -1;
            setZoom(zoom * 2 ** dir);
          }
        }}
      />
    </section>
  );
}
