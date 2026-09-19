.POSIX:
.PHONY: fmt clippy test bench fixture smoke check

fmt:
	cargo fmt --check

clippy:
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

test:
	cargo test --workspace --locked

bench:
	cargo bench -p airwav-dsp --bench pipeline --locked
	cargo bench -p airwav-dsp --bench audio --locked

fixture:
	cargo run --release --example make_fixture -- /tmp/airwav-demo.awr

smoke: fixture
	cargo build --release --locked
	python3 tools/smoke-tui.py target/release/airwav /tmp/airwav-demo.awr

# Mirrors .github/workflows/ci.yml (minus rust-cache / toolchain install / audio TUI smoke).
check: fmt clippy test bench
	cargo run --release --example make_fixture -- /tmp/airwav-demo.awr
	cargo run --release --bin airwav -- inspect /tmp/airwav-demo.awr
	cargo run --release --bin airwav -- replay /tmp/airwav-demo.awr --headless
	cargo run --release --bin airwav -- export /tmp/airwav-demo.awr --output /tmp/airwav-demo.svg
	python3 tools/smoke-tui.py target/release/airwav /tmp/airwav-demo.awr
