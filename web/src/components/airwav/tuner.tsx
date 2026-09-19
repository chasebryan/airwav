import { useRef, useState } from "react";
import { BOOKMARKS, TUNE_STEPS, armedDecoders, formatTune } from "@/lib/airwav/tune";
import { DECODER_KEYS, DECODER_LABEL, type DecoderKey } from "@/lib/airwav/types";
import { useAirwav } from "@/lib/airwav/store";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function TunerBar() {
  const centerHz = useAirwav((s) => s.centerHz);
  const tuneStepHz = useAirwav((s) => s.tuneStepHz);
  const setTuneStep = useAirwav((s) => s.setTuneStep);
  const stepTune = useAirwav((s) => s.stepTune);
  const parseAndTune = useAirwav((s) => s.parseAndTune);
  const setBand = useAirwav((s) => s.setBand);
  const band = useAirwav((s) => s.band);
  const scanning = useAirwav((s) => s.scanning);
  const toggleScan = useAirwav((s) => s.toggleScan);
  const gainTenthDb = useAirwav((s) => s.gainTenthDb);
  const setGain = useAirwav((s) => s.setGain);
  const ppm = useAirwav((s) => s.ppm);
  const setPpm = useAirwav((s) => s.setPpm);
  const loadIqFile = useAirwav((s) => s.loadIqFile);
  const clearFile = useAirwav((s) => s.clearFile);
  const sourceKind = useAirwav((s) => s.sourceKind);
  const fileName = useAirwav((s) => s.fileName);
  const decoderEnabled = useAirwav((s) => s.decoderEnabled);
  const toggleDecoder = useAirwav((s) => s.toggleDecoder);
  const sampleRate = useAirwav((s) => s.sampleRate);
  const [draft, setDraft] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);
  const armed = armedDecoders(centerHz, sampleRate);

  const commit = () => {
    if (!draft.trim()) return;
    if (parseAndTune(draft)) setDraft("");
  };

  return (
    <div className="shrink-0 border-b border-border px-2 py-2 sm:px-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" onClick={() => stepTune(-1)} aria-label="Step down">
          −
        </Button>
        <form
          className="flex min-w-0 flex-1 items-center gap-2 sm:max-w-xs"
          onSubmit={(e) => {
            e.preventDefault();
            commit();
          }}
        >
          <label className="sr-only" htmlFor="aw-vfo">
            Center frequency
          </label>
          <input
            id="aw-vfo"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={formatTune(centerHz)}
            inputMode="decimal"
            autoComplete="off"
            className="h-10 min-h-10 w-full min-w-0 rounded-sm border border-border bg-panel px-3 font-mono text-sm tabular-nums text-fg outline-none ring-accent focus:border-accent focus:ring-1"
          />
          <Button type="submit" size="sm">
            Tune
          </Button>
        </form>
        <Button size="sm" onClick={() => stepTune(1)} aria-label="Step up">
          +
        </Button>
        <span className="font-mono text-lg font-medium tabular-nums tracking-tight text-accent sm:text-xl">
          {(centerHz / 1e6).toFixed(6)}
        </span>
        <span className="font-mono text-[11px] tracking-wider text-muted uppercase">MHz</span>
        <div className="flex flex-wrap gap-1">
          {TUNE_STEPS.map((st) => (
            <button
              key={st.hz}
              type="button"
              onClick={() => setTuneStep(st.hz)}
              className={cn(
                "h-8 rounded-sm border px-2 font-mono text-[10px] uppercase",
                tuneStepHz === st.hz ? "border-accent text-accent" : "border-border text-muted",
              )}
            >
              {st.label}
            </button>
          ))}
        </div>
        <Button variant={scanning ? "solid" : "default"} size="sm" onClick={toggleScan}>
          {scanning ? "Stop scan" : "Scan"}
        </Button>
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-1.5">
        {BOOKMARKS.map((b) => (
          <button
            key={b.hz}
            type="button"
            title={b.note}
            onClick={() => {
              const id = BOOKMARKS.find((x) => x.hz === b.hz);
              const match = (
                [
                  ["VHF air", "vhf-air"],
                  ["ACARS", "acars"],
                  ["APRS", "aprs"],
                  ["NOAA", "noaa"],
                  ["FM", "fm"],
                  ["433 ISM", "ism433"],
                  ["1090 ES", "es1090"],
                ] as const
              ).find(([label]) => label === id?.label);
              if (match) setBand(match[1]);
            }}
            className={cn(
              "h-8 rounded-sm border px-2.5 font-mono text-[10px] uppercase tracking-wide",
              Math.abs(centerHz - b.hz) < 1 || band === bookmarkBand(b.label)
                ? "border-accent bg-accent/10 text-accent"
                : "border-border text-muted hover:text-fg",
            )}
          >
            {b.label}
          </button>
        ))}
        <label className="ml-auto flex items-center gap-2 font-mono text-[11px] text-muted">
          Gain
          <select
            className="h-8 rounded-sm border border-border bg-panel px-2 text-fg"
            value={gainTenthDb === null ? "auto" : String(gainTenthDb)}
            onChange={(e) => setGain(e.target.value === "auto" ? null : Number(e.target.value))}
          >
            <option value="auto">AUTO</option>
            {[0, 90, 148, 259, 366, 402, 496].map((g) => (
              <option key={g} value={g}>
                {(g / 10).toFixed(1)} dB
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-2 font-mono text-[11px] text-muted">
          PPM
          <input
            type="number"
            min={-200}
            max={200}
            value={ppm}
            onChange={(e) => setPpm(Number(e.target.value))}
            className="h-8 w-16 rounded-sm border border-border bg-panel px-2 text-fg tabular-nums"
          />
        </label>
        <input
          ref={fileRef}
          type="file"
          accept=".cu8,.u8,.iq,.bin,.raw"
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            file.arrayBuffer().then((buf) => loadIqFile(file.name, buf));
            e.target.value = "";
          }}
        />
        <Button size="sm" variant="ghost" onClick={() => fileRef.current?.click()}>
          Load IQ
        </Button>
        {sourceKind === "file" && (
          <Button size="sm" variant="ghost" onClick={clearFile}>
            Unload {fileName}
          </Button>
        )}
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-1.5">
        <span className="font-mono text-[10px] tracking-wider text-muted uppercase">Decoders</span>
        {DECODER_KEYS.map((id) => {
          const on = decoderEnabled[id];
          const live = on && armed.includes(id);
          return (
            <button
              key={id}
              type="button"
              title={
                live
                  ? `${DECODER_LABEL[id]} armed at this VFO`
                  : on
                    ? `${DECODER_LABEL[id]} enabled — retune into its window to arm`
                    : `${DECODER_LABEL[id]} disabled`
              }
              onClick={() => toggleDecoder(id as DecoderKey)}
              className={cn(
                "h-8 rounded-sm border px-2.5 font-mono text-[10px] uppercase tracking-wide",
                live
                  ? "border-prism bg-prism/10 text-prism"
                  : on
                    ? "border-border text-fg"
                    : "border-border text-muted line-through decoration-muted",
              )}
            >
              {DECODER_LABEL[id]}
              {live ? " · armed" : on ? "" : " · off"}
            </button>
          );
        })}
        {armed.length === 0 && (
          <span className="font-mono text-[10px] text-muted">
            No decoder window at this frequency
          </span>
        )}
      </div>
    </div>
  );
}

function bookmarkBand(label: string): string | null {
  switch (label) {
    case "VHF air":
      return "vhf-air";
    case "ACARS":
      return "acars";
    case "APRS":
      return "aprs";
    case "NOAA":
      return "noaa";
    case "FM":
      return "fm";
    case "433 ISM":
      return "ism433";
    case "1090 ES":
      return "es1090";
    default:
      return null;
  }
}
