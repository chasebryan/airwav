# Changelog

## Unreleased

- Terminal Listen: `pw-cat` now requests raw s16 PCM (`--raw`). Without it, libsndfile tried to open stdin (`-`) as a sound file, exited, and AIRWAV reported Broken pipe.

- Live VFO: n/N step, `/` enter MHz, cursor/island/Shift-click retune, gain and PPM while streaming. A retune starts a new DSP/decoder epoch.
- CRC-gated decoders in `airwav-decode` and the browser observer: Mode S/1090ES (CRC-24), ACARS (odd parity + block checksum), APRS (AX.25 FCS), POCSAG (BCH(31,21)+parity), SAME (`ZCZC` header). Frequency coincidence is not identity.
- Native Frames overlay (V) and observer frame rail with decoder arming chips.
- Browser observer of synthetic or dropped IQ is no longer a presentation-only DEMO FIXTURE; it is still not a USB V4.
- Doctor storage check fails closed when free space is below configured `minimum_free_bytes`, with clear guidance naming the minimum.
- TUI header shows recording elapsed wall time (`● RECORDING M:SS` / `H:MM:SS`) via sticky `Ui::set_recording` start.
- CI concurrency cancel-in-progress and 45-minute job timeout; `make check` mirrors the core CI gate set.
- Safer sessions directory creation (no `expect` on the parent path).
- CONTRIBUTING: short calm-PR hygiene (fmt before push, small PRs, re-land instead of WIP recovery).

- Terminal presentation overhaul: compact instrument chrome, power-mapped spectrum with dB scale, noise floor, peak hold, island cursors, cursor frequency/dBFS readout, theme-true waterfalls, and a denser island table (frequency, bandwidth, SNR, LIVE/FADING, protocol UNKNOWN).
- Separate activity (LIVE / FADING hysteresis) from protocol (UNKNOWN). Old recordings that stored `UNKNOWN` on current islands still render as LIVE.
- Click a spectrum bin to select the nearest island; wheel zoom keeps the cursor frequency; Z zooms to the selected island; Home/End/PgUp/PgDn move the list; O sorts by SNR; F hides fading islands; K toggles peak hold.
- Recording elapsed time, IQ ring fill, audio lock vs selection mismatch, FADING-listen warning, fixture header no longer claims V4 hardware, USB loss shown when the metric exists, and Q requires a second press while a recording is active.
- Click-drag a spectrum span to zoom that window. Keyboard +/- zoom toward the cursor frequency when the mouse is over the plot, not the view center.
- Selecting an island outside the zoomed window pans the view so the measurement stays on screen.

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
