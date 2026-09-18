# Validation — 2026-09-18

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

## Unverified / incomplete

- Physical V4 sustained capture, RF calibration, USB behavior, actual driver setup, and Fedora/i3 device acceptance.
- PRISM DDC/resampling/allocation, autonomous MAX-I retuning, aviation decoding, verified identities, audio, and fingerprint memory.
- PNG/video/cast exports, Director Mode, continuous IQ recording, recording rotation, and complete settings/locking UI.

These are documented milestones, not hidden substitutes or claimed supported features. Follow hardware-acceptance.md before promoting the capture milestone or proceeding to live receiver features.
