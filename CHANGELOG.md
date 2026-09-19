# Changelog

## Unreleased

- Terminal recording header shows wall-clock elapsed time while metadata recording is active.
- `airwav doctor` storage check fails closed when free space is below `minimum_free_bytes`, with clearer guidance.
- CI concurrency cancel-in-progress and 45-minute job timeout; `make check` mirrors the core CI gate set.
- CONTRIBUTING: short calm-PR hygiene (fmt before push, small PRs, re-land instead of WIP recovery).

- Native terminal Listen/Mute, AM/FM/NFM mode and volume controls for live IQ and recorded events, with independent bounded audio processing and visible player failures.
- Terminal PCM levels, sent/clipped sample counts and explicit silence, missing-input and stalled-output diagnostics.
- Fix rapid terminal Listen/Mute and mode changes using stale display state; apply live audio commands in queue order and preserve current channel volume.

- Offline AM, mono broadcast FM and narrow FM audio from verified AWR events, 48 kHz WAV export with embedded provenance, and optional system-player playback.
- Bounded event ID index in `inspect` for selecting audio captures; deterministic audio fixtures and benchmark.

- Optional `web/` DEMO FIXTURE observer. Synthetic IQ only; not a live receiver.

## 0.1.0 — capture foundation

Working native executable for passive RTL-SDR Blog V4 observation:

- Fail-closed V4 identity, R828D and clock checks, bounded async IQ
- Hann FFT, local-noise Signal Islands, UNKNOWN by default
- AWR v1 journals, BLAKE3 IQ artifacts, SQLite index, crash recovery
- Native terminal with spectrum, measured waterfall, five themes, SVG export
- Deterministic DEMO FIXTURE; not selectable as a live receiver

Hardware acceptance on a physical V4 remains outstanding. No aviation decoders.
