# Contributing to AIRWAV

Observe first. Conclude second.

AIRWAV is a capture foundation for one receiver: the RTL-SDR Blog V4. Changes that invent decoder output, aircraft identities, protocol confidence, MAX-I scores, or a generic SDR backend will be rejected.

## Before you write code

1. Read [docs/roadmap.md](docs/roadmap.md). Do not skip a release gate.
2. Read [docs/safety.md](docs/safety.md). Passive observation only.
3. Hardware features require [docs/hardware-acceptance.md](docs/hardware-acceptance.md). Automated ABI tests are not physical V4 acceptance.
4. Do not display a supported protocol merely because a center frequency matches an aviation band. `UNKNOWN` is the default.

## Development

Linux, Rust 1.98+, a C compiler. Replay and tests do not need librtlsdr.

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

A fixture recording (not a live receiver):

```bash
cargo run --release --example make_fixture -- /tmp/airwav-demo.awr
cargo run --release --bin airwav -- inspect /tmp/airwav-demo.awr
cargo run --release --bin airwav -- replay /tmp/airwav-demo.awr --headless
```

`make check` runs the same gates CI uses.

## Crate boundaries

- `airwav-v4` is the only crate that may contain `unsafe` / FFI.
- DSP stays deterministic and allocation-bounded. Do not add unvalidated I/Q correction.
- The UI must not touch receiver I/O. Presentation pause must not stop capture.
- Recordings are immutable observations. Do not revise RF facts in place.
- Fixture sources must remain labeled `DEMO FIXTURE` and must not be selectable as live hardware.

## Pull requests

- Keep the change in one crate when possible.
- Add a test for any measurement, journal, or driver contract you touch.
- Do not weaken `deny_unknown_fields` on configuration.
- Describe what was measured, not what you hope the signal is.
