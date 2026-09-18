# Development log

## 2026-09-18 — ordered terminal audio controls

Reproduced a live control race in the previous installed build: two rapid Listen presses left AM playing instead of returning to off. The UI was deciding start/stop and mode behavior from a stale status snapshot. It now sends toggle/mode intent to the runtime, which applies commands in queue order using the actual monitor state. Mode changes preserve the locked frequency and latest volume, and a rejected mode command no longer changes the visible selector.

The expanded terminal regression fails on the prior executable and passes on the fixed debug build. It covers rapid toggles and a combined Listen/volume/mode sequence. The smoke harness now waits for measured app state, uses a known mid-band synthetic AM carrier instead of counter-pattern harmonics, paces its PCM sink, bounds screenshot/transcript storage and cleans up its own process group on failure. The stalled-player unit test waits for its sink to be ready before filling the pipe. All 56 workspace tests, formatting and warning-free Clippy pass. The terminal regression passes with the fixed debug executable. No physical receiver or speaker acceptance is claimed.

## 2026-09-18 — diagnosing terminal silence

Added measured RMS/peak PCM levels and sent/clipped sample counts, plus explicit waiting-for-IQ, silent-PCM, stalled-output and draining messages. A blocked player is detected after one second without conflating the PCM measurement with speaker output. Readouts appear in the terminal header, Diagnostics and F12 metadata. The finite silence floor keeps screenshot JSON valid.

All 56 workspace tests, formatting and warning-free Clippy pass. New vectors check exact RMS/peak values, silence and full-scale handling; deterministic status tests cover wait/stall/drain transitions. The terminal test now stalls a separate player process, verifies the visible warning and continued IQ processing across two screenshots, and checks clean shutdown. Physical RF and audible speaker acceptance remain outstanding.

## 2026-09-18 — terminal listening

The earlier audio command did not connect sound to the main terminal. Added A Listen/Mute, M AM/FM/NFM and 9/0 volume controls for live IQ and recorded-event playback. The header displays the locked audio channel and player failures; Diagnostics reports queue drops and gaps. Audio uses a bounded worker and a system PCM player, with cancellation that kills/reaps the player before joining a blocked writer. Volume changes preserve DSP state. Replay audio plays a whole captured event at 1× independently of measurement timing.

Validation: all 53 workspace tests, formatting and warning-free Clippy pass. New coverage includes PCM delivery, volume, gap reset, stalled-player shutdown, error reporting, recorded EOF and keyboard/mouse controls. The optimized release passes both pseudo-terminal suites, including replay and the separate test-only V4 ABI driver, mode/volume changes, visible player failure, screenshot export and terminal restoration. The local PulseAudio-on-PipeWire server accepted and drained a synthetic AM tone using the terminal's raw PCM player arguments. The installed executable was updated after verification, with its previous build backed up. This does not establish audible speaker output or physical V4 reception; those acceptance checks remain unperformed.

## 2026-09-18 — recorded AM/FM audio

Priority: support both aviation-oriented AM voice and FM radio audio while retaining explicit measurement provenance and the physical receiver acceptance gate.

Implemented `airwav audio` for AM, mono broadcast FM and narrow FM, frequency selection, 48 kHz PCM WAV export, fixed gain, optional squelch, broadcast de-emphasis and optional system-player playback. WAV metadata points back to the verified source event and IQ hash. Added a bounded event index to `inspect`, deterministic audio vectors, an audible fixture generator and a dedicated throughput benchmark. See [audio.md](audio.md) for the signal-processing contract and limitations.

Validation so far: all 48 workspace tests pass; formatting and Clippy pass with warnings denied. Tests cover all three modes, positive/negative offsets, adjacent-channel rejection, anti-alias filtering, de-emphasis, non-integer rate conversion, irregular blocks, corrupted IQ, WAV structure/provenance, event selection, overwrite protection and system-player invocation/failure. The player test uses a separate test executable, not a physical sound device. The optimized audio benchmark measured AM at 8.15 MS/s, FM at 8.26 MS/s and NFM at 8.41 MS/s (3.19–3.28 times the default 2.56 MS/s input). This measures the DSP path on the development host, excludes file/receiver I/O and playback, and is not a live receiver guarantee. The optimized application and examples build successfully. Python’s independent WAV reader verified mono/16-bit/48 kHz output, two-second duration within one output sample, embedded DEMO FIXTURE provenance, 440 Hz AM and 660 Hz FM recovery, and zero clipped samples. The player lifecycle is tested with a test-only process; no physical sound-device playback is claimed. Published as [PR #3](https://github.com/chasebryan/airwav/pull/3).

Next priorities:

1. Independent recorded RF vectors and measured filter/selectivity comparisons, especially close-spaced AM channels and weak-signal FM.
2. Event audio controls in offline replay, with an explicit distinction between measurement replay timing and audio playback.
3. Physical V4 acceptance, then bounded live audio monitoring with drop accounting and terminal controls.
4. Recording/recovery reliability and remaining roadmap gates, preserving immutable source evidence.

No physical receiver or sound-device acceptance is claimed. AM/FM demodulation does not establish an aviation protocol or identity; FM is mono and does not decode stereo/RDS.
