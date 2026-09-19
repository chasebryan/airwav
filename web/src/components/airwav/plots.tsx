import { useCallback, useEffect, useRef, type MouseEvent } from "react";
import { getTheme, hexToRgb, waterfallLut } from "@/lib/airwav/themes";
import { activityOf, formatMhz } from "@/lib/airwav/types";
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

function binX(bin: number, a: number, b: number, left: number, plotW: number): number {
  return left + ((bin - a) / Math.max(1, b - a)) * plotW;
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
  const hoverHz = useAirwav((s) => s.hoverHz);
  const hoverDbfs = useAirwav((s) => s.hoverDbfs);
  const hoverFrac = useAirwav((s) => s.hoverFrac);
  const focused = useAirwav((s) => s.focused);
  const setHover = useAirwav((s) => s.setHover);
  const setDragStart = useAirwav((s) => s.setDragStart);
  const applyDrag = useAirwav((s) => s.applyDrag);
  const setFocused = useAirwav((s) => s.setFocused);
  const hoverFracRef = useRef(0.5);

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
    const gutter = 36;
    const axis = 22;
    const left = gutter;
    const plotW = Math.max(1, w - gutter);
    const plotH = Math.max(1, h - axis);
    const p = snapshot.spectrum.powerDbfs;
    const [a, b] = viewRange(p.length, zoom, pan);
    const floor = Math.max(snapshot.spectrum.noiseDbfs - 10, -140);
    const ceiling = Math.max(-40, maxIn(p, a, b)) + 5;
    const span = Math.max(20, ceiling - floor);
    const toY = (db: number) => plotH - ((db - floor) / span) * plotH;
    const hzAt = (bin: number) => snapshot.spectrum.startHz + bin * snapshot.spectrum.binHz;

    ctx.strokeStyle = t.border;
    ctx.lineWidth = 1;
    ctx.font = "10px IBM Plex Mono, ui-monospace, monospace";
    ctx.fillStyle = t.muted;
    ctx.textAlign = "right";
    for (let i = 0; i <= 4; i++) {
      const db = ceiling - (span * i) / 4;
      const y = (plotH / 4) * i;
      ctx.globalAlpha = 0.35;
      ctx.beginPath();
      ctx.moveTo(left, y);
      ctx.lineTo(w, y);
      ctx.stroke();
      ctx.globalAlpha = 1;
      ctx.fillText(db.toFixed(0), left - 6, y + 3);
    }
    ctx.textAlign = "left";

    const spec = snapshot.spectrum;
    for (let i = 0; i < snapshot.islands.length; i++) {
      const island = snapshot.islands[i]!;
      const lo = (island.centerHz - island.bandwidthHz / 2 - spec.startHz) / spec.binHz;
      const hi = (island.centerHz + island.bandwidthHz / 2 - spec.startHz) / spec.binHz;
      if (hi < a || lo >= b) continue;
      const x0 = binX(Math.max(lo, a), a, b, left, plotW);
      const x1 = binX(Math.min(hi, b), a, b, left, plotW);
      const act = activityOf(island.state);
      const rgb = hexToRgb(i === selected ? t.selected : act === "LIVE" ? t.live : t.muted);
      ctx.fillStyle = `rgba(${rgb[0]},${rgb[1]},${rgb[2]},${i === selected ? 0.16 : 0.07})`;
      ctx.fillRect(x0, 0, Math.max(1, x1 - x0), plotH);
    }

    const noiseY = toY(snapshot.spectrum.noiseDbfs);
    ctx.save();
    ctx.strokeStyle = t.border;
    ctx.setLineDash([3, 4]);
    ctx.globalAlpha = 0.8;
    ctx.beginPath();
    ctx.moveTo(left, noiseY);
    ctx.lineTo(w, noiseY);
    ctx.stroke();
    ctx.restore();
    ctx.fillStyle = t.muted;
    ctx.font = "10px IBM Plex Mono, ui-monospace, monospace";
    ctx.fillText("noise", left + 6, Math.min(plotH - 4, noiseY - 4));

    if (peakHold && peak && peak.length === p.length) {
      ctx.beginPath();
      for (let x = 0; x < plotW; x++) {
        const from = a + Math.floor((x / plotW) * (b - a));
        const to = Math.max(from + 1, a + Math.floor(((x + 1) / plotW) * (b - a)));
        const y = toY(maxIn(peak, from, to));
        if (x === 0) ctx.moveTo(left + x, y);
        else ctx.lineTo(left + x, y);
      }
      ctx.strokeStyle = t.muted;
      ctx.globalAlpha = 0.45;
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    const accent = hexToRgb(t.accent);
    ctx.beginPath();
    for (let x = 0; x < plotW; x++) {
      const from = a + Math.floor((x / plotW) * (b - a));
      const to = Math.max(from + 1, a + Math.floor(((x + 1) / plotW) * (b - a)));
      const y = toY(maxIn(p, from, to));
      if (x === 0) ctx.moveTo(left + x, y);
      else ctx.lineTo(left + x, y);
    }
    ctx.lineTo(left + plotW, plotH);
    ctx.lineTo(left, plotH);
    ctx.closePath();
    const fill = ctx.createLinearGradient(0, 0, 0, plotH);
    fill.addColorStop(0, `rgba(${accent[0]},${accent[1]},${accent[2]},0.28)`);
    fill.addColorStop(1, `rgba(${accent[0]},${accent[1]},${accent[2]},0.02)`);
    ctx.fillStyle = fill;
    ctx.fill();

    ctx.beginPath();
    for (let x = 0; x < plotW; x++) {
      const from = a + Math.floor((x / plotW) * (b - a));
      const to = Math.max(from + 1, a + Math.floor(((x + 1) / plotW) * (b - a)));
      const y = toY(maxIn(p, from, to));
      if (x === 0) ctx.moveTo(left + x, y);
      else ctx.lineTo(left + x, y);
    }
    ctx.strokeStyle = t.accent;
    ctx.lineWidth = 1.5;
    ctx.stroke();

    for (let i = 0; i < snapshot.islands.length; i++) {
      const island = snapshot.islands[i]!;
      const bin = (island.centerHz - spec.startHz) / spec.binHz;
      if (bin < a || bin >= b) continue;
      const x = binX(bin, a, b, left, plotW);
      const act = activityOf(island.state);
      ctx.fillStyle = i === selected ? t.selected : act === "LIVE" ? t.live : t.muted;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x - 5, 9);
      ctx.lineTo(x + 5, 9);
      ctx.closePath();
      ctx.fill();
      if (i === selected) {
        ctx.strokeStyle = t.selected;
        ctx.globalAlpha = 0.35;
        ctx.beginPath();
        ctx.moveTo(x, 9);
        ctx.lineTo(x, plotH);
        ctx.stroke();
        ctx.globalAlpha = 1;
      }
    }

    if (lockedHz !== null) {
      const bin = (lockedHz - spec.startHz) / spec.binHz;
      if (bin >= a && bin < b) {
        const x = binX(bin, a, b, left, plotW);
        ctx.strokeStyle = t.prism;
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, plotH);
        ctx.stroke();
        ctx.setLineDash([]);
      }
    }

    if (dragStart !== null) {
      const x0 = left + dragStart * plotW;
      const x1 = left + hoverFracRef.current * plotW;
      ctx.fillStyle = `rgba(${accent[0]},${accent[1]},${accent[2]},0.14)`;
      ctx.fillRect(Math.min(x0, x1), 0, Math.abs(x1 - x0), plotH);
      ctx.strokeStyle = t.accent;
      ctx.globalAlpha = 0.7;
      ctx.strokeRect(Math.min(x0, x1) + 0.5, 0.5, Math.abs(x1 - x0), plotH - 1);
      ctx.globalAlpha = 1;
    }

    if (hoverHz !== null && hoverDbfs !== null && dragStart === null) {
      const x = left + hoverFrac * plotW;
      const y = toY(hoverDbfs);
      ctx.strokeStyle = t.muted;
      ctx.globalAlpha = 0.45;
      ctx.setLineDash([2, 3]);
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, plotH);
      ctx.moveTo(left, y);
      ctx.lineTo(w, y);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.globalAlpha = 1;
    }

    ctx.fillStyle = t.muted;
    ctx.font = "11px IBM Plex Mono, ui-monospace, monospace";
    const fromHz = hzAt(a);
    const toHz = hzAt(b);
    ctx.fillText(`${formatMhz(fromHz, 3)} MHz`, left + 8, h - 7);
    ctx.textAlign = "center";
    ctx.fillText(`${formatMhz((fromHz + toHz) / 2, 3)} MHz`, left + plotW / 2, h - 7);
    ctx.textAlign = "right";
    ctx.fillText(`${formatMhz(toHz, 3)} MHz`, w - 8, h - 7);
    ctx.textAlign = "left";
  }, [theme, snapshot, peak, peakHold, zoom, pan, selected, lockedHz, dragStart, hoverHz, hoverDbfs, hoverFrac]);

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
      const node = canvasRef.current;
      const rect = node?.getBoundingClientRect();
      const gutter = 36;
      const plotW = rect ? Math.max(1, rect.width - gutter) : 1;
      const frac =
        rect && plotW > 0
          ? Math.max(0, Math.min(1, (e.clientX - rect.left - gutter) / plotW))
          : 0.5;
      if (e.shiftKey) {
        const dir = e.deltaY < 0 ? 1 : -1;
        state.setPan(state.pan + (dir * 0.1) / state.zoom);
      } else {
        const dir = e.deltaY < 0 ? 1 : -1;
        state.zoomAt(2 ** dir, frac);
      }
    };
    canvas?.addEventListener("wheel", onWheelNative, { passive: false });
    return () => {
      window.removeEventListener("resize", onResize);
      ro?.disconnect();
      canvas?.removeEventListener("wheel", onWheelNative);
    };
  }, [draw]);

  const fracOf = (e: MouseEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const gutter = 36;
    const plotW = Math.max(1, rect.width - gutter);
    return Math.max(0, Math.min(1, (e.clientX - rect.left - gutter) / plotW));
  };

  const onMove = (e: MouseEvent<HTMLCanvasElement>) => {
    const frac = fracOf(e);
    hoverFracRef.current = frac;
    const snap = useAirwav.getState().snapshot;
    if (!snap) return;
    const [a, b] = viewRange(snap.spectrum.powerDbfs.length, zoom, pan);
    const bin = a + frac * (b - a);
    const hz = snap.spectrum.startHz + bin * snap.spectrum.binHz;
    const from = Math.floor(bin);
    const dbfs = maxIn(snap.spectrum.powerDbfs, from, from + 1);
    setHover(hz, dbfs, frac);
    if (dragStart !== null) draw();
  };

  const onLeave = () => setHover(null, null);

  const onDown = (e: MouseEvent<HTMLCanvasElement>) => {
    setFocused(0);
    setDragStart(fracOf(e));
  };

  const onUp = (e: MouseEvent<HTMLCanvasElement>) => {
    const frac = fracOf(e);
    const start = useAirwav.getState().dragStart;
    if (start !== null && Math.abs(frac - start) > 0.02) applyDrag(frac);
    else {
      const hz = useAirwav.getState().hoverHz;
      if (hz !== null) {
        if (e.shiftKey) useAirwav.getState().setCenter(hz, "click");
        else useAirwav.getState().selectNearest(hz);
      }
      setDragStart(null);
    }
  };

  return (
    <section
      className={cn(
        "relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border bg-panel",
        focused === 0 ? "border-accent" : "border-border",
      )}
    >
      <header className="flex items-center justify-between border-b border-border px-3 py-1.5">
        <h2 className="font-mono text-xs tracking-[0.18em] text-muted uppercase">
          01 / Spectrum · dBFS
        </h2>
        <p className="font-mono text-xs text-muted tabular-nums">
          {snapshot
            ? `Floor ${snapshot.spectrum.noiseDbfs.toFixed(1)} dBFS · ${(snapshot.spectrum.binHz / 1000).toFixed(1)} kHz/bin · ${zoom.toFixed(0)}×`
            : "—"}
        </p>
      </header>
      <canvas
        ref={canvasRef}
        className="block h-full min-h-36 w-full flex-1 touch-none"
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
  const setFocused = useAirwav((s) => s.setFocused);
  const zoomAt = useAirwav((s) => s.zoomAt);
  const setPan = useAirwav((s) => s.setPan);

  useEffect(() => {
    lutRef.current = waterfallLut(getTheme(theme));
  }, [theme]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const cssW = Math.max(1, Math.floor(canvas.clientWidth));
    const cssH = Math.max(1, Math.floor(canvas.clientHeight));
    if (canvas.width !== cssW || canvas.height !== cssH) {
      canvas.width = cssW;
      canvas.height = cssH;
    }
    const t = getTheme(theme);
    const lut = lutRef.current;
    const bg = hexToRgb(t.panel);
    const img = ctx.createImageData(cssW, cssH);
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
      const rows = Math.min(history.length, cssH);
      for (let row = 0; row < rows; row++) {
        const spec = history[row]!;
        for (let x = 0; x < cssW; x++) {
          const from = a + Math.floor((x / cssW) * (b - a));
          const to = Math.max(from + 1, a + Math.floor(((x + 1) / cssW) * (b - a)));
          const power = maxIn(spec, from, to);
          const index = Math.max(0, Math.min(255, Math.round(((power - floor) / 36) * 255)));
          const off = (row * cssW + x) * 4;
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
        const x = ((bin - a) / (b - a)) * cssW;
        const act = activityOf(island.state);
        ctx.fillStyle = i === selected ? t.selected : act === "LIVE" ? t.live : t.muted;
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
        <h2 className="font-mono text-xs tracking-[0.18em] text-muted uppercase">
          02 / Waterfall · measured history
        </h2>
        <p className="hidden font-mono text-xs text-muted sm:block">Newest at top · not generated texture</p>
      </header>
      <canvas
        ref={canvasRef}
        className="block h-full min-h-36 w-full flex-1 touch-none"
        onMouseDown={() => setFocused(1)}
        onWheel={(e) => {
          e.preventDefault();
          const rect = e.currentTarget.getBoundingClientRect();
          const frac = rect.width > 0 ? Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width)) : 0.5;
          if (e.shiftKey) {
            const dir = e.deltaY < 0 ? 1 : -1;
            setPan(pan + (dir * 0.1) / zoom);
          } else {
            const dir = e.deltaY < 0 ? 1 : -1;
            zoomAt(2 ** dir, frac);
          }
        }}
      />
    </section>
  );
}
