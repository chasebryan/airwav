import type { Snapshot } from "./types";
import { formatMhz } from "./types";
import type { AirwavTheme } from "./themes";

export function snapshotJson(snapshot: Snapshot, source: string): string {
  return JSON.stringify(
    {
      format: "airwav-observation",
      version: 1,
      source,
      timestampNs: snapshot.timestampNs,
      receiver: snapshot.receiver,
      noiseDbfs: snapshot.spectrum.noiseDbfs,
      binHz: snapshot.spectrum.binHz,
      startHz: snapshot.spectrum.startHz,
      islands: snapshot.islands.map((i) => ({
        id: i.id,
        centerHz: i.centerHz,
        bandwidthHz: i.bandwidthHz,
        peakDbfs: i.peakDbfs,
        snrDb: i.snrDb,
        observations: i.observations,
        state: i.state,
      })),
      metrics: snapshot.metrics,
      note: "Measurements only. No protocol, identity, or decoder output.",
    },
    null,
    2,
  );
}

export function spectrumSvg(
  snapshot: Snapshot,
  theme: AirwavTheme,
  width = 1200,
  height = 360,
): string {
  const p = snapshot.spectrum.powerDbfs;
  const n = p.length;
  const floor = Math.max(snapshot.spectrum.noiseDbfs - 10, -140);
  let ceiling = -40;
  for (let i = 0; i < n; i++) if (p[i]! > ceiling) ceiling = p[i]!;
  ceiling = Math.ceil(ceiling) + 5;
  const span = Math.max(20, ceiling - floor);
  const pts: string[] = [];
  for (let x = 0; x < width; x++) {
    const from = Math.floor((x / width) * n);
    const to = Math.max(from + 1, Math.floor(((x + 1) / width) * n));
    let v = -160;
    for (let i = from; i < to && i < n; i++) if (p[i]! > v) v = p[i]!;
    const y = height - 28 - ((v - floor) / span) * (height - 40);
    pts.push(`${x},${y.toFixed(1)}`);
  }
  const start = snapshot.spectrum.startHz;
  const end = start + n * snapshot.spectrum.binHz;
  return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
  <rect width="100%" height="100%" fill="${theme.bg}"/>
  <text x="24" y="28" fill="${theme.text}" font-family="IBM Plex Mono, ui-monospace, monospace" font-size="16" font-weight="600">AIRWAV / SPECTRUM</text>
  <text x="24" y="48" fill="${theme.muted}" font-family="IBM Plex Mono, ui-monospace, monospace" font-size="11">DEMO FIXTURE · ${formatMhz(snapshot.receiver.centerHz)} MHz · UNKNOWN only</text>
  <polyline fill="none" stroke="${theme.accent}" stroke-width="1.4" points="${pts.join(" ")}"/>
  <text x="24" y="${height - 10}" fill="${theme.muted}" font-family="IBM Plex Mono, ui-monospace, monospace" font-size="11">${formatMhz(start, 3)} MHz</text>
  <text x="${width / 2}" y="${height - 10}" fill="${theme.muted}" font-family="IBM Plex Mono, ui-monospace, monospace" font-size="11" text-anchor="middle">${formatMhz((start + end) / 2, 3)} MHz</text>
  <text x="${width - 24}" y="${height - 10}" fill="${theme.muted}" font-family="IBM Plex Mono, ui-monospace, monospace" font-size="11" text-anchor="end">${formatMhz(end, 3)} MHz</text>
</svg>`;
}

export function download(filename: string, contents: string, mime: string): void {
  const blob = new Blob([contents], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
