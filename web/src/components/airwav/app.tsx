import { useEffect } from "react";
import { Terminal } from "@/components/airwav/terminal";
import { useAirwav } from "@/lib/airwav/store";

export default function App() {
  const booted = useAirwav((s) => s.booted);
  const boot = useAirwav((s) => s.boot);

  useEffect(() => {
    if (booted) return;
    const t = window.setTimeout(boot, 900);
    return () => window.clearTimeout(t);
  }, [booted, boot]);

  if (!booted) {
    return (
      <button
        type="button"
        onClick={boot}
        className="flex h-dvh w-full flex-col items-center justify-center gap-5 bg-bg px-6 text-center"
      >
        <p className="font-mono text-[11px] tracking-[0.4em] text-muted uppercase aw-enter">
          Observation terminal
        </p>
        <h1 className="font-mono text-3xl font-semibold tracking-[0.35em] text-fg aw-enter aw-enter-delay-1 sm:text-5xl">
          AIRWAV
        </h1>
        <p className="max-w-md font-mono text-sm text-muted aw-enter aw-enter-delay-2">
          Observe first. Conclude second.
        </p>
        <p className="font-mono text-[11px] text-unknown aw-enter aw-enter-delay-3">
          Tuner · CRC-gated Mode S / ACARS / APRS / POCSAG / SAME · not a USB receiver
        </p>
        <p className="font-mono text-[11px] text-muted aw-enter aw-enter-delay-3">
          Click to open the observation terminal
        </p>
      </button>
    );
  }

  return <Terminal />;
}
