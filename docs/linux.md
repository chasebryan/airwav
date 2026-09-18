# Linux / Fedora / i3 setup

The runtime is a terminal executable. Use a recent stable Rust (1.98+), a C compiler, and a Unicode terminal. Fedora package names below match the development environment; Rust installed with rustup is also suitable. Do not run AIRWAV as root.

```bash
sudo dnf install rust cargo rustfmt clippy gcc gcc-c++ make cmake git pkgconf-pkg-config libusb1-devel
cargo build --release --locked
cargo install --path crates/airwav-app --locked
```

## V4-capable driver

Use the [RTL-SDR Blog driver's source](https://github.com/rtlsdrblog/rtl-sdr-blog) and [official V4 guide](https://www.rtl-sdr.com/V4/). AIRWAV checks the exact `RTLSDRBlog` / `Blog V4` identity, R828D tuner, and the initialized 28.8 MHz RTL and tuner clocks. In the [vendor implementation](https://github.com/rtlsdrblog/rtl-sdr-blog/blob/aed0ea19f3a273370a13c9009b96313c75d54c7b/src/librtlsdr.c), V4-specific initialization distinguishes this from the older 16 MHz R828D path. This is a behavioral compatibility check for that driver family, not a universal capability attestation for arbitrary forks.

The helper below downloads a fixed upstream commit and builds into a user-selected prefix. It does not install a global library, change udev, or edit EEPROM:

```bash
tools/build-v4-driver.sh "$HOME/.local/opt/airwav-v4"
airwav --library "$HOME/.local/opt/airwav-v4/lib/librtlsdr.so" doctor
```

Set `library` at the top level of `airwav config --init`'s TOML file to retain that explicit path. The helper uses `lib` consistently. Without an explicit path, AIRWAV uses the system loader's `librtlsdr.so.0`, then `librtlsdr.so`. It never falls back to another hardware backend.

USB access may require installing the vendor's udev rule and reconnecting the device. Follow the vendor guide for your system's group/session policy. If a DVB kernel driver or another SDR program owns the receiver, close that application and apply the vendor-recommended driver configuration. `doctor` reports the failing operation and librtlsdr status code. AIRWAV does not silently change kernel modules or device permissions.

Do not rewrite EEPROM product/manufacturer strings: the driver uses them for V4 initialization. Blog V3, generic RTL devices and Blog V4 Lite are rejected. Multiple matching V4 units require an explicit unique `receiver.serial`.

## First session

```bash
airwav doctor --stream-seconds 30 --counter-test
airwav doctor --stream-seconds 30
airwav --center-hz 136000000
```

A 2.56 MS/s setting is the intended V4 window, but stability depends on the USB host and CPU. Hardware counter tests have a modulo-256 limitation; normal RF samples cannot reveal exact USB loss. If the host fails sustained tests, lower `receiver.sample_rate` (for example to 2400000), then repeat acceptance. Do not label untested operation stable.

In i3, run AIRWAV in a terminal with truecolor and mouse reporting. `COLORTERM=truecolor` selects RGB; otherwise AIRWAV maps themes to 256 colors. Resize is handled without restarting capture. F10 changes the layout, not the terminal's font size. Terminal font size is controlled by your terminal emulator.

## Paths and overrides

- Config: `$AIRWAV_CONFIG`, `--config`, or `$XDG_CONFIG_HOME/airwav/config.toml`.
- Data: `$AIRWAV_DATA_DIR` or `$XDG_DATA_HOME/airwav`.
- Default XDG roots: `~/.config` and `~/.local/share`.
- Logs: `airwav.log` under the data root, with tracing fields; `--log-level debug` increases detail.
- Screenshots and TUI-started sessions: `screenshots/` and `sessions/` under the data root.

Core capture, processing and replay do not require internet access. Dependency and driver downloads are build/setup operations only. Offline AM/FM/NFM WAV export needs no audio system or FFmpeg. Optional `airwav audio --play` uses an installed `pw-play`, `paplay`, `aplay`, or `ffplay`; live monitoring remains unverified. See [audio](audio.md). `doctor` reports their status without claiming unsupported export functionality.
