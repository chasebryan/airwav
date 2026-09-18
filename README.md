# AIRWAV

**Observe first. Conclude second.**

A native Rust terminal for passive RF observation, built exclusively for the **RTL-SDR Blog V4**. No generic SDR backend, browser, cloud service, or transmitter support.

## Current milestone

This is a **capture foundation**, not the first production release described in the product specification. It includes a working native executable, a fail-closed V4 hardware boundary, measured spectrum and waterfall, signal activity inspection, bounded pre/post-trigger IQ captures, indexed AWR recordings, integrity checks, offline replay, and SVG terminal screenshots.

**Hardware acceptance remains outstanding.** Automated tests exercise the driver ABI and streaming lifecycle through a separate test-only C library. They cannot establish reliable reception on a physical V4. Run the [hardware acceptance procedure](docs/hardware-acceptance.md) before proceeding to production receiver features.

There are **no supported aviation decoders yet**. Detected activity is `UNKNOWN`; AIRWAV does not fabricate aircraft, identities, protocol confidence, decoder output, PRISM channels, or MAX-I scores. The default is a manual observation window until autonomous scheduling is implemented and verified. See the [milestone ledger](docs/roadmap.md) for the remaining work.

## Build and run

Linux, Rust 1.98 or newer, and a C compiler are required. The executable builds and replays recordings **without librtlsdr installed**. Live capture loads the V4-capable library at runtime. SQLite is bundled.

```bash
cargo build --release --locked
cargo install --path crates/airwav-app --locked
airwav doctor
airwav
```

For Fedora/i3, follow [Linux setup](docs/linux.md). Use a truecolor terminal with mouse reporting. The minimum terminal size is 70 × 22 cells.

```bash
airwav --center-hz 136000000
airwav devices
airwav doctor --stream-seconds 30 --counter-test
airwav config --init
airwav capture session.awr --seconds 20
airwav inspect session.awr
airwav replay session.awr
airwav replay session.awr --headless
airwav demo session.awr
airwav export session.awr --output screenshot.svg
airwav recover interrupted.awr --output recovered.awr
```

`doctor --counter-test` captures the receiver's hardware counter, not RF. It checks modulo-256 counter continuity and does not record counter bytes as observations. In normal RF mode librtlsdr does not expose USB loss; AIRWAV reports that limitation separately from measured application queue drops.

## Terminal controls

| Input | Operation |
| --- | --- |
| ↑ / ↓, click signal | Select a Signal Island |
| Enter / I, double/right click signal | Inspect measured evidence |
| R, Record button | Start/stop metadata recording |
| C, Capture IQ button | Preserve available pre-trigger IQ and configured post-roll; recording must be active |
| Space, Pause button | Pause presentation; live capture continues |
| + / −, wheel over spectrum | Zoom |
| ← / →, Shift+wheel | Pan |
| D / E / ? | Diagnostics / event list / contextual help |
| T | Cycle Midnight, Radar, Arctic, Ember, Studio |
| F10 | Presentation-only Demo Mode |
| F12, Save button | SVG cell-buffer screenshot with JSON metadata |
| Q, Quit button | Stop capture, finalize recording, restore terminal |
| Esc, Close button | Close overlay |
| Replay 1 / 2 / 3 / 4 / 5 | 0.25× / 0.5× / 1× / 2× / 4× |
| Replay . | Step one measurement |
| Replay [ / ], event buttons | Jump to previous/next captured event |

This milestone implements the core mouse actions above; full context menus, range dragging, frequency locking, and audio monitoring remain on the roadmap.

## Deterministic development fixture

The synthetic preview is **explicitly labeled DEMO FIXTURE**, is generated from IQ, and cannot be selected as a live receiver. It contains tones, drift, bursts, and seeded noise. It makes no decoder claims.

```bash
cargo run --release --example make_fixture -- /tmp/airwav-demo.awr
airwav replay /tmp/airwav-demo.awr
airwav export /tmp/airwav-demo.awr --output /tmp/airwav-demo.svg
```

## Configuration and data

The defaults use `~/.config/airwav/config.toml` and `~/.local/share/airwav/`; standard XDG variables are respected. `AIRWAV_CONFIG` and `AIRWAV_DATA_DIR` override these paths. `--config` and `--library` select explicit configuration/library paths. Missing configuration uses validated defaults. Unknown TOML fields are errors.

The default 2.56 MS/s IQ ring holds at most **25.6 MB** for five seconds; post-trigger capture is ten seconds. It stores raw unsigned 8-bit IQ, not floating-point copies. Recording stops at its configured size/free-space thresholds. It never automatically deletes old captures. At 2.56 MS/s, raw IQ costs 5.12 MB/s; continuous recording is deliberately not enabled in this milestone.

## Verification

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
cargo bench -p airwav-dsp --bench pipeline
python3 tools/smoke-tui.py target/release/airwav /tmp/airwav-demo.awr
```

Tests cover strict device identity, driver clock checks, ABI contracts, counter gaps, bounded queue saturation, disconnect/reopen and shutdown races, numerical DSP, property-tested rings, AWR corruption and recovery, database migrations, terminal sizing, and production/fixture separation.

See [architecture](docs/architecture.md), [DSP semantics](docs/dsp.md), [AWR format](docs/recording-format.md), [safety](docs/safety.md), and [validation results](docs/validation.md).

Licensed under the repository's existing [GNU AGPL v3](LICENSE). The user-installed RTL-SDR Blog driver is a separate dependency; AIRWAV does not vendor its implementation.
