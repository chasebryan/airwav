# Changelog

## Unreleased

- Terminal recording header shows wall-clock elapsed time while metadata recording is active (roadmap phase 3 polish).
- `airwav doctor` storage check fails closed when free space is below `minimum_free_bytes`, with clearer FAIL guidance.
- CI concurrency cancel-in-progress and job timeout; `make check` aligned with the full CI gate set.
- Help overlay distinguishes shipped offline AM/FM/NFM from future live-audio / decoder milestones.
- Broader config validation unit coverage; recording path creation no longer uses `expect` on the sessions directory.

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
