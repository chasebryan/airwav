import { useEffect } from "react";
import {
  Circle,
  Pause,
  Play,
  HelpCircle,
  Activity,
  Camera,
  Settings,
  Lock,
  Unlock,
  Gauge,
  ScrollText,
  RotateCcw,
  Radio,
} from "lucide-react";
import { SpectrumPlot, WaterfallPlot } from "./plots";
import { EvidencePanel, IslandList, Overlay } from "./panels";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { download, snapshotJson, spectrumSvg } from "@/lib/airwav/export";
import { getTheme } from "@/lib/airwav/themes";
import { activityOf, BANDS, formatMhz } from "@/lib/airwav/types";
import { useAirwav } from "@/lib/airwav/store";
import { setListen } from "@/lib/airwav/audio";
import { cn } from "@/lib/utils";

export function Terminal() {
  const theme = useAirwav((s) => s.theme);
  const start = useAirwav((s) => s.start);
  const stop = useAirwav((s) => s.stop);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.background = getTheme(theme).bg;
  }, [theme]);

  useEffect(() => {
    start();
    return () => {
      stop();
      setListen(null);
    };
  }, [start, stop]);

  const audioActive = useAirwav((s) => s.audioActive);
  const audioMode = useAirwav((s) => s.audioMode);
  const audioVolume = useAirwav((s) => s.audioVolume);
  const snapshot = useAirwav((s) => s.snapshot);
  const selected = useAirwav((s) => s.selected);
  useEffect(() => {
    const island = snapshot?.islands[selected];
    setListen(
      audioActive
        ? {
            active: true,
            mode: audioMode,
            volume: audioVolume,
            snrDb: island?.snrDb ?? 0,
            fading: island ? activityOf(island.state) === "FADING" : false,
          }
        : null,
    );
  }, [audioActive, audioMode, audioVolume, snapshot, selected]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const s = useAirwav.getState();
      const key = e.key;
      if (key === "Escape") {
        s.setOverlay(null);
        return;
      }
      if (s.overlay && key !== "?" && key !== "F12") return;
      switch (key) {
        case "q":
        case "Q":
          s.requestQuit();
          break;
        case "r":
        case "R":
          s.toggleRecord();
          break;
        case "c":
        case "C":
          if (e.ctrlKey) s.requestQuit();
          else s.captureIq();
          break;
        case " ":
          e.preventDefault();
          s.togglePause();
          break;
        case "ArrowUp":
          e.preventDefault();
          s.select(-1);
          break;
        case "ArrowDown":
          e.preventDefault();
          s.select(1);
          break;
        case "ArrowLeft":
          s.setPan(s.pan - 0.1 / s.zoom);
          break;
        case "ArrowRight":
          s.setPan(s.pan + 0.1 / s.zoom);
          break;
        case "Home":
          e.preventDefault();
          s.jumpVisible("home");
          break;
        case "End":
          e.preventDefault();
          s.jumpVisible("end");
          break;
        case "PageUp":
          e.preventDefault();
          s.jumpVisible(-8);
          break;
        case "PageDown":
          e.preventDefault();
          s.jumpVisible(8);
          break;
        case "Tab":
          e.preventDefault();
          s.cycleFocus(e.shiftKey ? -1 : 1);
          break;
        case "+":
        case "=":
          s.zoomAt(2, s.hoverHz === null ? 0.5 : s.hoverFrac);
          break;
        case "-":
        case "_":
          s.zoomAt(0.5, s.hoverHz === null ? 0.5 : s.hoverFrac);
          break;
        case "z":
        case "Z":
          s.zoomToSelected();
          break;
        case "Enter":
        case "i":
        case "I":
          s.setOverlay("evidence");
          break;
        case "d":
        case "D":
          s.setOverlay("diagnostics");
          break;
        case "e":
        case "E":
          s.setOverlay("events");
          break;
        case "?":
          s.setOverlay(s.overlay === "help" ? null : "help");
          break;
        case "s":
        case "S":
          s.setOverlay("settings");
          break;
        case "g":
        case "G":
          s.setOverlay("log");
          break;
        case "t":
        case "T":
          s.cycleTheme();
          break;
        case "a":
        case "A":
          s.toggleAudio();
          break;
        case "m":
        case "M":
          s.cycleAudioMode();
          break;
        case "9":
          s.setAudioVolume(s.audioVolume - 10);
          break;
        case "0":
          s.setAudioVolume(s.audioVolume + 10);
          break;
        case "o":
        case "O":
          s.toggleSort();
          break;
        case "f":
        case "F":
          s.toggleHideFading();
          break;
        case "k":
        case "K":
          s.setPeakHold(!s.peakHold);
          break;
        case "l":
        case "L":
          s.toggleLock();
          break;
        case "p":
        case "P":
          s.toggleReplay();
          break;
        case ".":
          s.stepReplay();
          break;
        case "[":
          s.jumpEvent(-1);
          break;
        case "]":
          s.jumpEvent(1);
          break;
        case "1":
        case "2":
        case "3":
        case "4":
        case "5":
          if (s.replay) s.setSpeed([0.25, 0.5, 1, 2, 4][Number(key) - 1]!);
          break;
        case "F10":
          e.preventDefault();
          s.toggleDemo();
          break;
        case "F12":
          e.preventDefault();
          exportNow();
          break;
        default:
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="relative flex h-dvh min-h-0 flex-col overflow-hidden bg-bg text-fg">
      <Header />
      <MainStage />
      <Toolbar />
      <StatusBar />
      <Overlay />
    </div>
  );
}

function Header() {
  const source = useAirwav((s) => s.source);
  const snapshot = useAirwav((s) => s.snapshot);
  const recording = useAirwav((s) => s.recording);
  const recordStartedAt = useAirwav((s) => s.recordStartedAt);
  const captureActive = useAirwav((s) => s.captureActive);
  const paused = useAirwav((s) => s.paused);
  const replay = useAirwav((s) => s.replay);
  const speed = useAirwav((s) => s.speed);
  const band = useAirwav((s) => s.band);
  const demo = useAirwav((s) => s.demo);
  const hoverHz = useAirwav((s) => s.hoverHz);
  const hoverDbfs = useAirwav((s) => s.hoverDbfs);
  const lockedHz = useAirwav((s) => s.lockedHz);
  const audioActive = useAirwav((s) => s.audioActive);
  const audioMode = useAirwav((s) => s.audioMode);
  const audioVolume = useAirwav((s) => s.audioVolume);
  const selected = useAirwav((s) => s.selected);
  const now = useNow(recording);
  const elapsed =
    recording && recordStartedAt ? ((now - recordStartedAt) / 1000).toFixed(1) : null;
  const bandMeta = BANDS.find((b) => b.id === band);
  const ring = snapshot
    ? snapshot.metrics.ringCapacityBytes
      ? snapshot.metrics.ringBytes / snapshot.metrics.ringCapacityBytes
      : 0
    : 0;
  const island = snapshot?.islands[selected];
  const vu = island ? Math.max(0, Math.min(1, (island.peakDbfs + 80) / 50)) : 0;
  const modes = ["AM", "FM", "NFM"] as const;
  const liveCount = snapshot?.islands.filter((i) => activityOf(i.state) === "LIVE").length ?? 0;
  const fadingCount = (snapshot?.islands.length ?? 0) - liveCount;

  return (
    <header className="shrink-0 border-b border-border px-3 py-2 sm:px-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-baseline gap-3">
          <h1 className="font-mono text-sm font-semibold tracking-[0.28em] text-fg">
            AIRWAV
          </h1>
          <span className="hidden font-mono text-xs tracking-[0.14em] text-muted uppercase sm:inline">
            / RF observation
          </span>
        </div>
        <Badge className="border-unknown/50 text-unknown">{source}</Badge>
      </div>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-4 gap-y-1 font-mono text-xs text-muted tabular-nums">
        <span>SYNTHETIC IQ · not a receiver</span>
        <span>
          {snapshot
            ? `${formatMhz(snapshot.receiver.centerHz)} MHz`
            : bandMeta
              ? `${formatMhz(bandMeta.centerHz, 3)} MHz`
              : "—"}
        </span>
        <span>{snapshot ? `${(snapshot.receiver.sampleRate / 1e6).toFixed(2)} MS/s` : "2.56 MS/s"}</span>
        <span>{replay ? `REPLAY ${speed.toFixed(2)}×` : "MANUAL WINDOW"}</span>
        <span className="inline-flex items-center gap-2 text-prism">
          RING
          <span className="inline-block h-1.5 w-16 overflow-hidden rounded-full bg-fg/10">
            <span className="block h-full bg-prism" style={{ width: `${Math.round(ring * 100)}%` }} />
          </span>
          {Math.round(ring * 100)}%
        </span>
        <span>USB n/a</span>
        {hoverHz !== null && hoverDbfs !== null && (
          <span className="text-accent">
            {formatMhz(hoverHz)} MHz · {hoverDbfs.toFixed(1)} dBFS
          </span>
        )}
      </div>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-xs uppercase tracking-wider">
        <span
          className={cn(
            "inline-flex items-center gap-1.5",
            recording ? "text-danger aw-rec" : "text-muted",
          )}
        >
          <Circle className="size-2.5 fill-current" />
          {recording ? `Recording ${elapsed}s` : replay ? "Replay" : "Observing"}
        </span>
        <span className={captureActive ? "text-prism" : "text-muted"}>
          {captureActive ? "Event IQ · collecting" : "Receive only"}
        </span>
        {paused && <span className="text-unknown">View paused</span>}
        <span className="text-live">{liveCount} live</span>
        <span className="text-muted">{fadingCount} fading</span>
        <span className="text-unknown">Unk</span>
        <span className={audioActive ? "text-prism" : "text-muted"}>
          {modes[audioMode % 3]} {audioVolume}%
          {audioActive && lockedHz !== null ? ` · lock ${formatMhz(lockedHz)}` : ""}
        </span>
        {audioActive && (
          <span className="inline-flex items-center gap-1.5 text-accent normal-case tracking-normal">
            VU
            <span className="inline-block h-1.5 w-16 overflow-hidden rounded-full bg-fg/10">
              <span
                className="block h-full bg-accent"
                style={{ width: `${Math.round(vu * 100)}%` }}
              />
            </span>
          </span>
        )}
        {demo && <span className="text-accent">Demo view</span>}
      </div>
    </header>
  );
}

function MainStage() {
  const demo = useAirwav((s) => s.demo);
  return (
    <div
      className={cn(
        "grid min-h-0 flex-1 gap-2 p-2 aw-enter aw-enter-delay-1",
        demo
          ? "grid-cols-1 lg:grid-cols-[1fr_240px]"
          : "grid-cols-1 lg:grid-cols-[minmax(0,1.6fr)_minmax(260px,0.9fr)]",
      )}
    >
      <div className="grid min-h-0 grid-rows-2 gap-2">
        <SpectrumPlot />
        <WaterfallPlot />
      </div>
      <div className={cn("grid min-h-0 gap-2", demo ? "hidden lg:grid lg:grid-rows-2" : "grid-rows-2")}>
        <IslandList />
        <EvidencePanel />
      </div>
    </div>
  );
}

function Toolbar() {
  const recording = useAirwav((s) => s.recording);
  const paused = useAirwav((s) => s.paused);
  const replay = useAirwav((s) => s.replay);
  const lockedHz = useAirwav((s) => s.lockedHz);
  const setOverlay = useAirwav((s) => s.setOverlay);
  const toggleRecord = useAirwav((s) => s.toggleRecord);
  const captureIq = useAirwav((s) => s.captureIq);
  const togglePause = useAirwav((s) => s.togglePause);
  const toggleLock = useAirwav((s) => s.toggleLock);
  const toggleAudio = useAirwav((s) => s.toggleAudio);
  const cycleAudioMode = useAirwav((s) => s.cycleAudioMode);
  const setAudioVolume = useAirwav((s) => s.setAudioVolume);
  const audioActive = useAirwav((s) => s.audioActive);
  const audioMode = useAirwav((s) => s.audioMode);
  const audioVolume = useAirwav((s) => s.audioVolume);
  const zoomToSelected = useAirwav((s) => s.zoomToSelected);
  const toggleReplay = useAirwav((s) => s.toggleReplay);
  const cycleTheme = useAirwav((s) => s.cycleTheme);
  const resetSession = useAirwav((s) => s.resetSession);
  const stepReplay = useAirwav((s) => s.stepReplay);

  return (
    <div className="shrink-0 border-t border-border px-2 py-2 aw-enter aw-enter-delay-2">
      <div className="flex gap-1.5 overflow-x-auto pb-1">
        {!replay ? (
          <>
            <Button variant={recording ? "rec" : "default"} onClick={toggleRecord}>
              <Circle className="size-3 fill-current" />
              {recording ? "Stop rec" : "Record"}
            </Button>
            <Button onClick={captureIq}>
              <Radio className="size-3.5" />
              Capture IQ
            </Button>
          </>
        ) : (
          <Button onClick={stepReplay}>Step</Button>
        )}
        <Button onClick={togglePause}>
          {paused ? <Play className="size-3.5" /> : <Pause className="size-3.5" />}
          {paused ? "Resume" : "Pause"}
        </Button>
        <Button variant={audioActive ? "solid" : "default"} onClick={toggleAudio}>
          {audioActive ? "Mute" : "Listen"}
        </Button>
        <Button onClick={cycleAudioMode}>{["AM", "FM", "NFM"][audioMode % 3]}</Button>
        <Button onClick={() => setAudioVolume(audioVolume - 10)}>Vol −</Button>
        <Button onClick={() => setAudioVolume(audioVolume + 10)}>Vol +</Button>
        <Button onClick={zoomToSelected}>Zoom island</Button>
        <Button onClick={toggleLock}>
          {lockedHz !== null ? <Unlock className="size-3.5" /> : <Lock className="size-3.5" />}
          {lockedHz !== null ? "Unlock" : "Lock"}
        </Button>
        <Button onClick={toggleReplay}>
          <Activity className="size-3.5" />
          {replay ? "Live" : "Replay"}
        </Button>
        <Button onClick={exportNow}>
          <Camera className="size-3.5" />
          Save
        </Button>
        <Button variant="ghost" onClick={() => setOverlay("evidence")}>
          Evidence
        </Button>
        <Button variant="ghost" onClick={() => setOverlay("events")}>
          Events
        </Button>
        <Button variant="ghost" onClick={() => setOverlay("diagnostics")}>
          <Gauge className="size-3.5" />
          Diag
        </Button>
        <Button variant="ghost" onClick={() => setOverlay("settings")}>
          <Settings className="size-3.5" />
          Settings
        </Button>
        <Button variant="ghost" onClick={() => setOverlay("log")}>
          <ScrollText className="size-3.5" />
          Log
        </Button>
        <Button variant="ghost" onClick={cycleTheme}>
          Theme
        </Button>
        <Button variant="ghost" onClick={() => setOverlay("help")}>
          <HelpCircle className="size-3.5" />
          Help
        </Button>
        <Button variant="ghost" onClick={resetSession}>
          <RotateCcw className="size-3.5" />
          Reset
        </Button>
      </div>
    </div>
  );
}

function StatusBar() {
  const status = useAirwav((s) => s.status);
  const snapshot = useAirwav((s) => s.snapshot);
  return (
    <footer className="flex shrink-0 flex-col gap-0.5 border-t border-border px-4 py-2 font-mono text-[11px] text-muted aw-enter aw-enter-delay-3 sm:flex-row sm:items-center sm:justify-between">
      <p className="truncate text-fg/80">{status}</p>
      <p className="hidden sm:block">
        ? Help · ↑↓ Select · click/drag spectrum · Z island · A Listen · T Theme · Q Reset
        {snapshot ? ` · ${snapshot.islands.length} islands` : ""}
      </p>
    </footer>
  );
}

function exportNow() {
  const s = useAirwav.getState();
  if (!s.snapshot) return;
  const theme = getTheme(s.theme);
  download("airwav-spectrum.svg", spectrumSvg(s.snapshot, theme), "image/svg+xml");
  download(
    "airwav-observation.json",
    snapshotJson(s.snapshot, s.source),
    "application/json",
  );
  s.log("event", "Exported SVG spectrum and JSON observation metadata.");
}

function useNow(active: boolean): number {
  const start = useAirwav((s) => s.recordStartedAt) ?? Date.now();
  // re-render ~2 Hz while recording via snapshot frames
  const frames = useAirwav((s) => s.snapshot?.metrics.frames ?? 0);
  if (!active) return start;
  return Date.now() + frames * 0;
}
