# Development log

## 2026-09-18 — recorded AM/FM audio

Priority: support both aviation-oriented AM voice and FM radio audio while retaining explicit measurement provenance and the physical receiver acceptance gate.

Implemented `airwav audio` for AM, mono broadcast FM and narrow FM, frequency selection, 48 kHz PCM WAV export, fixed gain, optional squelch, broadcast de-emphasis and optional system-player playback. WAV metadata points back to the verified source event and IQ hash. Added a bounded event index to `inspect`, deterministic audio vectors, an audible fixture generator and a dedicated throughput benchmark. See [audio.md](audio.md) for the signal-processing contract and limitations.

Validation so far: all 48 workspace tests pass; formatting and Clippy pass with warnings denied. Tests cover all three modes, positive/negative offsets, adjacent-channel rejection, anti-alias filtering, de-emphasis, non-integer rate conversion, irregular blocks, corrupted IQ, WAV structure/provenance, event selection, overwrite protection and system-player invocation/failure. The player test uses a separate test executable, not a physical sound device. Optimized build, audible-fixture inspection and throughput measurements are being checked before this increment is marked ready.

Next priorities:

1. Independent recorded RF vectors and measured filter/selectivity comparisons, especially close-spaced AM channels and weak-signal FM.
2. Event audio controls in offline replay, with an explicit distinction between measurement replay timing and audio playback.
3. Physical V4 acceptance, then bounded live audio monitoring with drop accounting and terminal controls.
4. Recording/recovery reliability and remaining roadmap gates, preserving immutable source evidence.

No physical receiver or sound-device acceptance is claimed. AM/FM demodulation does not establish an aviation protocol or identity; FM is mono and does not decode stereo/RDS.
