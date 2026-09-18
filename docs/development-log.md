# Development log

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
