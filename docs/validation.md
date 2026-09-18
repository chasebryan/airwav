# Validation — 2026-09-18

The results below describe the original capture foundation. For subsequent AM/FM audio behavior, see [the development log](development-log.md) and [audio validation scope](audio.md).

Environment: Linux x86_64, Rust/Cargo 1.98.1. The workspace has no exposed physical V4 and no installed production librtlsdr. Hardware acceptance is **not performed**. Checks used a separate temporary C ABI test library or explicitly labeled synthetic IQ where hardware would otherwise be needed.

## Automated checks

- `cargo fmt --check`: passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed with no warnings.
- `cargo test --workspace`: **39 tests passed**, no failures or ignored tests. Includes four CLI integration tests and one C-ABI lifecycle test containing saturation, counter discontinuity, disconnect, and thirty immediate stop/reopen cycles.
- Optimized workspace/example build and `cargo build --release --bin airwav`: passed. The release executable reports `airwav 0.1.0` and passed the same PTY replay and offline artifact checks.
- `bash -n tools/build-v4-driver.sh`: passed. The actual vendor driver build and device initialization need the documented libusb development dependency and physical receiver acceptance.

## DSP benchmark

`cargo bench -p airwav-dsp --bench pipeline` processed 13,107,200 samples through FFT, detection and the raw ring:

```text
49.99 MS/s
19.53 × the 2.56 MS/s input rate
655.6 microseconds per 32,768-sample block
```

This is one measurement on the development host, not a receiver throughput guarantee. It excludes USB transport, storage and terminal rendering. The benchmark code and its input are preserved for reproduction.

## Recording and terminal checks

The deterministic fixture generator produced an AWR v1 bundle with 32 measured spectrum snapshots, one complete event, and **10,240,000 bytes of BLAKE3-verified IQ**. The source is DEMO FIXTURE. `inspect`, `replay --headless`, and SVG/JSON export succeeded without loading librtlsdr.

A real Linux pseudo-terminal at 132 × 42 cells exercised replay, pause, F12 screenshot, diagnostics, theme switching, and quit. The process exited successfully and emitted both alternate-screen entry and restoration sequences. The screenshot metadata retained DEMO FIXTURE and measured spectrum/island data. SVG was rendered to PNG externally for visual inspection; native PNG export is not claimed.

`airwav doctor --json --stream-seconds 0` reports `ok: false` and a hardware failure when the production driver is absent. CLI tests verify that failure is nonzero and structured, configuration initialization never overwrites a file, invalid settings fail, and replay/export work with an intentionally nonexistent library path.

## Terminal audio checks

PR #4 adds terminal AM/FM/NFM listening, ordered mode and volume controls, and visible PCM/output failures. All 56 workspace tests, formatting, and warning-free Clippy passed locally. GitHub run 35384617151 passed the optimized build, benchmarks, and both pseudo-terminal suites on the current PR head. The earlier successful CI run 35382717055 produced a Linux executable; its downloaded ZIP matched GitHub's SHA-256 artifact digest. That executable passed both terminal suites locally, then the installed copy passed the audio suite again. The suites cover synthetic live IQ, recorded replay, rapid controls, AM/FM PCM delivery to a fake sink, player failure/stall, continued IQ processing, and clean terminal restoration.

These checks establish software behavior under synthetic IQ and a test PCM sink. A local PulseAudio-on-PipeWire server previously accepted and drained synthetic PCM, but physical V4 reception and sound from an actual speaker remain unverified. The tested CI executable is not a hardware-accepted release.

## Unverified / incomplete

- Physical V4 sustained capture, RF calibration, USB behavior, actual driver setup, and Fedora/i3 device acceptance.
- PRISM DDC/resampling/allocation, autonomous MAX-I retuning, aviation decoding, verified identities, physical receiver/speaker audio acceptance, and fingerprint memory.
- PNG/video/cast exports, Director Mode, continuous IQ recording, recording rotation, and complete settings/locking UI.

These are documented milestones, not hidden substitutes or claimed supported features. Follow hardware-acceptance.md before promoting the capture milestone or proceeding to live receiver features.
