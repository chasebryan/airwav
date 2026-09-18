import { BANDS } from "@/lib/airwav/types";
import { formatKhz, formatMhz, formatSamples } from "@/lib/airwav/types";
import { useAirwav } from "@/lib/airwav/store";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Slider } from "@/components/ui/slider";

export function IslandList() {
  const snapshot = useAirwav((s) => s.snapshot);
  const selected = useAirwav((s) => s.selected);
  const selectIndex = useAirwav((s) => s.selectIndex);
  const setOverlay = useAirwav((s) => s.setOverlay);
  const focused = useAirwav((s) => s.focused);
  const islands = snapshot?.islands ?? [];

  return (
    <section
      className={cn(
        "flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border bg-panel",
        focused === 2 ? "border-accent" : "border-border",
      )}
    >
      <header className="flex items-center justify-between border-b border-border px-3 py-1.5">
        <h2 className="font-mono text-[10px] tracking-[0.18em] text-muted uppercase">
          03 / Signal Islands
        </h2>
        <span className="font-mono text-[10px] text-muted tabular-nums">
          {islands.length} / 256
        </span>
      </header>
      <div className="grid grid-cols-[18px_1fr_64px_88px] gap-x-2 px-3 py-1.5 font-mono text-[10px] tracking-wider text-muted uppercase">
        <span />
        <span>MHz</span>
        <span className="text-right">SNR</span>
        <span>State</span>
      </div>
      <ul className="min-h-0 flex-1 overflow-auto px-1 pb-2">
        {islands.length === 0 ? (
          <li className="px-3 py-4 font-mono text-xs text-muted">
            No activity above threshold
          </li>
        ) : (
          islands.map((island, i) => (
            <li key={island.id}>
              <button
                type="button"
                onClick={() => selectIndex(i)}
                onDoubleClick={() => {
                  selectIndex(i);
                  setOverlay("evidence");
                }}
                className={cn(
                  "grid w-full grid-cols-[18px_1fr_64px_88px] items-center gap-x-2 rounded-sm px-2 py-2 text-left font-mono text-xs tabular-nums",
                  i === selected
                    ? "bg-accent/10 text-selected"
                    : "text-unknown hover:bg-fg/5",
                )}
              >
                <span className="text-accent">{i === selected ? "▌" : ""}</span>
                <span>{formatMhz(island.centerHz)}</span>
                <span className="text-right">{island.snrDb.toFixed(1)} dB</span>
                <span>
                  {island.state.startsWith("FADING") ? "FADING" : "UNKNOWN"}
                </span>
              </button>
            </li>
          ))
        )}
      </ul>
    </section>
  );
}

export function EvidencePanel() {
  const snapshot = useAirwav((s) => s.snapshot);
  const selected = useAirwav((s) => s.selected);
  const island = snapshot?.islands[selected];

  return (
    <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-border bg-panel">
      <header className="border-b border-border px-3 py-1.5">
        <h2 className="font-mono text-[10px] tracking-[0.18em] text-muted uppercase">
          04 / Evidence
        </h2>
      </header>
      <div className="min-h-0 flex-1 overflow-auto px-4 py-3 font-mono text-xs leading-relaxed">
        {island ? (
          <dl className="grid grid-cols-[110px_1fr] gap-y-1.5">
            <dt className="text-muted">Island</dt>
            <dd className="text-fg">{String(island.id).padStart(4, "0")}</dd>
            <dt className="text-muted">Frequency</dt>
            <dd className="tabular-nums">{formatMhz(island.centerHz)} MHz</dd>
            <dt className="text-muted">Bandwidth</dt>
            <dd className="tabular-nums">{formatKhz(island.bandwidthHz)} kHz</dd>
            <dt className="text-muted">Peak</dt>
            <dd className="tabular-nums">{island.peakDbfs.toFixed(1)} dBFS</dd>
            <dt className="text-muted">SNR</dt>
            <dd className="tabular-nums">{island.snrDb.toFixed(1)} dB</dd>
            <dt className="text-muted">Observations</dt>
            <dd className="tabular-nums">{island.observations}</dd>
            <dt className="text-muted">Protocol</dt>
            <dd className="text-unknown">UNKNOWN</dd>
            <dt className="text-muted">Confidence</dt>
            <dd>not established</dd>
            <dt className="text-muted">Evidence</dt>
            <dd>FFT power / local noise</dd>
          </dl>
        ) : (
          <div className="space-y-2 text-muted">
            <p className="text-fg">Observe first. Conclude second.</p>
            <p>Select a measured signal to inspect it.</p>
            <p>No protocol has been established.</p>
          </div>
        )}
        <p className="mt-4 text-[11px] text-muted">
          No frame or identity decoded. SNR is a measurement, not protocol
          confidence.
        </p>
      </div>
    </section>
  );
}

export function Overlay() {
  const overlay = useAirwav((s) => s.overlay);
  const setOverlay = useAirwav((s) => s.setOverlay);
  if (!overlay) return null;
  return (
    <div
      className="absolute inset-0 z-30 flex items-center justify-center bg-bg/70 p-4"
      onClick={() => setOverlay(null)}
    >
      <div
        className="max-h-[min(88vh,640px)] w-full max-w-2xl overflow-auto rounded-lg border border-accent/40 bg-panel p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-start justify-between gap-4">
          <h2 className="font-mono text-sm tracking-[0.18em] text-accent uppercase">
            AIRWAV / {overlay}
          </h2>
          <button
            type="button"
            className="h-10 px-3 font-mono text-[11px] text-accent uppercase"
            onClick={() => setOverlay(null)}
          >
            Close
          </button>
        </div>
        {overlay === "help" && <HelpBody />}
        {overlay === "diagnostics" && <DiagnosticsBody />}
        {overlay === "events" && <EventsBody />}
        {overlay === "evidence" && <EvidencePanel />}
        {overlay === "settings" && <SettingsBody />}
        {overlay === "log" && <LogBody />}
      </div>
    </div>
  );
}

function HelpBody() {
  return (
    <div className="space-y-3 font-mono text-xs leading-relaxed text-fg">
      <p className="text-accent">Observe first. Conclude second.</p>
      <ul className="space-y-1 text-muted">
        <li>↑/↓ select · Enter / I evidence · D diagnostics · E events</li>
        <li>R recording · C pre/post-trigger IQ · Space pause presentation</li>
        <li>Replay: 1–5 = 0.25× / 0.5× / 1× / 2× / 4× · . step · P replay</li>
        <li>[ / ] jump events · +/− zoom · ←/→ pan · drag spectrum to zoom</li>
        <li>L lock frequency · S settings · G log · T theme · F10 demo</li>
        <li>F12 export · Esc closes overlays · Q reset session</li>
      </ul>
      <p>
        A Signal Island is measured RF activity above a local noise estimate.
        UNKNOWN means observed, but a protocol has not been established.
      </p>
      <p className="text-muted">
        This browser build runs the DEMO FIXTURE generator and the same
        measurement semantics as the native capture foundation. It is not a live
        RTL-SDR Blog V4 receiver, and it does not transmit.
      </p>
    </div>
  );
}

function DiagnosticsBody() {
  const snap = useAirwav((s) => s.snapshot);
  const source = useAirwav((s) => s.source);
  const m = snap?.metrics;
  const rows = m
    ? [
        ["Source", source],
        ["Received IQ samples", formatSamples(m.receivedSamples)],
        ["Application drops", `${m.queueDroppedSamples} samples`],
        ["USB sample loss", "unavailable in RF mode"],
        ["Processed samples", formatSamples(m.processedSamples)],
        ["DSP gaps", String(m.discontinuities)],
        ["DSP time / last block", `${m.dspUs} μs`],
        [
          "Ring memory",
          `${(m.ringBytes / 1048576).toFixed(2)} / ${(m.ringCapacityBytes / 1048576).toFixed(2)} MiB`,
        ],
        ["Storage snapshots lost", String(m.storageDroppedSnapshots)],
        ["Event IQ samples lost", String(m.storageDroppedIqSamples)],
        ["Signal islands", `${snap?.islands.length ?? 0} / 256`],
        ["Island candidates omitted", String(m.islandCandidatesOmitted)],
        ["Decoder workers", "0 (not enabled in this milestone)"],
        ["MAX-I", "not enabled in this milestone"],
      ]
    : [["Source", source]];
  return (
    <dl className="grid grid-cols-[minmax(140px,200px)_1fr] gap-y-1.5 font-mono text-xs">
      {rows.map(([k, v]) => (
        <div key={k} className="contents">
          <dt className="text-muted">{k}</dt>
          <dd className="tabular-nums">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function EventsBody() {
  const events = useAirwav((s) => s.events);
  if (!events.length) {
    return <p className="font-mono text-xs text-muted">No event captures in this session.</p>;
  }
  return (
    <ul className="space-y-2 font-mono text-xs">
      {[...events].reverse().map((ev) => (
        <li key={ev.id} className="rounded-sm border border-border px-3 py-2">
          <div className="flex items-center justify-between gap-3">
            <span className="text-accent">{ev.id}</span>
            <Badge className="text-unknown">{formatMhz(ev.centerHz)} MHz</Badge>
          </div>
          <p className="mt-1 text-fg">{ev.label}</p>
          <p className="mt-1 text-muted">
            SNR {ev.snrDb.toFixed(1)} dB · {formatSamples(ev.samples)} samples · UNKNOWN
          </p>
          <p className="mt-1 text-[11px] text-muted">{ev.note}</p>
        </li>
      ))}
    </ul>
  );
}

function SettingsBody() {
  const band = useAirwav((s) => s.band);
  const setBand = useAirwav((s) => s.setBand);
  const fftSize = useAirwav((s) => s.fftSize);
  const setFftSize = useAirwav((s) => s.setFftSize);
  const snrDb = useAirwav((s) => s.snrDb);
  const setSnr = useAirwav((s) => s.setSnr);
  const peakHold = useAirwav((s) => s.peakHold);
  const setPeakHold = useAirwav((s) => s.setPeakHold);
  const theme = useAirwav((s) => s.theme);
  const setTheme = useAirwav((s) => s.setTheme);

  return (
    <div className="space-y-5 font-mono text-xs">
      <fieldset>
        <legend className="mb-2 text-muted uppercase tracking-wider">Observation window</legend>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
          {BANDS.map((b) => (
            <button
              key={b.id}
              type="button"
              onClick={() => setBand(b.id)}
              className={cn(
                "rounded-md border px-3 py-3 text-left min-h-12",
                band === b.id ? "border-accent bg-accent/10" : "border-border hover:bg-fg/5",
              )}
            >
              <div className="text-fg">{b.label}</div>
              <div className="mt-1 text-muted tabular-nums">{formatMhz(b.centerHz, 3)} MHz</div>
            </button>
          ))}
        </div>
        <p className="mt-2 text-muted">
          Center frequency is an observation window, not a decoder claim.
        </p>
      </fieldset>
      <label className="block">
        <span className="text-muted">Detection SNR · {snrDb.toFixed(0)} dB</span>
        <Slider
          min={3}
          max={40}
          step={1}
          value={[snrDb]}
          onValueChange={([v]) => setSnr(v ?? 12)}
        />
      </label>
      <fieldset>
        <legend className="mb-2 text-muted uppercase tracking-wider">FFT size</legend>
        <div className="flex flex-wrap gap-2">
          {[1024, 2048, 4096].map((n) => (
            <button
              key={n}
              type="button"
              onClick={() => setFftSize(n)}
              className={cn(
                "h-10 min-w-16 rounded-sm border px-3",
                fftSize === n ? "border-accent text-accent" : "border-border text-muted",
              )}
            >
              {n}
            </button>
          ))}
        </div>
      </fieldset>
      <label className="flex h-10 items-center gap-3">
        <input
          type="checkbox"
          checked={peakHold}
          onChange={(e) => setPeakHold(e.target.checked)}
        />
        Peak hold on spectrum
      </label>
      <fieldset>
        <legend className="mb-2 text-muted uppercase tracking-wider">Theme</legend>
        <div className="flex flex-wrap gap-2">
          {(["Midnight", "Radar", "Arctic", "Ember", "Studio"] as const).map((name) => (
            <button
              key={name}
              type="button"
              onClick={() => setTheme(name)}
              className={cn(
                "h-10 rounded-sm border px-3",
                theme === name ? "border-accent text-accent" : "border-border text-muted",
              )}
            >
              {name}
            </button>
          ))}
        </div>
      </fieldset>
    </div>
  );
}

function LogBody() {
  const logs = useAirwav((s) => s.logs);
  return (
    <ul className="space-y-1.5 font-mono text-[11px]">
      {[...logs].reverse().map((l, i) => (
        <li key={`${l.t}-${i}`} className="flex gap-3">
          <span className="text-muted tabular-nums">
            {new Date(l.t).toISOString().slice(11, 23)}
          </span>
          <span
            className={
              l.level === "warn"
                ? "text-unknown"
                : l.level === "event"
                  ? "text-prism"
                  : "text-fg"
            }
          >
            {l.message}
          </span>
        </li>
      ))}
    </ul>
  );
}
