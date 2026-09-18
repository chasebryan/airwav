# Development log

## 2026-09-18 — foundation polish (terminal elapsed, doctor storage, CI)

Capture-foundation polish without decoder or hardware-boundary changes:

- Native terminal shows wall-clock elapsed time while metadata recording is active.
- Doctor storage check aligns with `minimum_free_bytes` fail-closed policy.
- Help overlay and validation notes distinguish shipped offline AM/FM/NFM from future live audio / decoders.
- CI concurrency cancel-in-progress; Makefile `check` matches the CI gate set.
- Safer sessions-directory creation in the DSP worker; broader config validation tests.

No physical V4 acceptance is claimed. UNKNOWN remains the default observation label.

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
