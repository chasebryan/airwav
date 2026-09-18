# Fixtures

No independent prerecorded aviation IQ fixture is included, so no aviation decoder support is claimed.

- `crates/airwav-v4/tests/fixtures/test_driver.c` is a test-only C ABI implementation. Tests compile it to a temporary shared library; it is never installed or linked into production.
- DSP unit tests generate deterministic numerical vectors and assert measurable output.
- `cargo run --release --example make_fixture -- /tmp/airwav-demo.awr` generates a labeled synthetic IQ recording and processes it through the real DSP/recording path.

The generated source label is DEMO FIXTURE and persists through replay, inspection, recovery and screenshots. Its signals have no protocol or identity assignment. Large generated bundles are excluded from Git.
