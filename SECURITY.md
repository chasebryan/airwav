# Security

AIRWAV is a **passive** observer for a single RTL-SDR Blog V4. It has no transmitter abstraction, no generic SoapySDR source, no network radio, and no exploitation, injection, jamming, spoofing, or protected-content circumvention.

## Scope

- Report issues that affect local integrity of captures, driver FFI soundness, path traversal in AWR bundles, or unexpected privilege use.
- Bias tee defaults off. AIRWAV never writes EEPROM.
- USB identity checks are compatibility checks, not cryptographic anti-counterfeit certification.

## Out of scope

- Requests for transmit, replay-over-the-air, protocol cracking, or decoder modules that interpret protected payloads.
- Claims that a recording is forensically certified. AWR hashes integrity of stored bytes; they do not authenticate the author or prove the IQ arrived over RF.

## Reporting

Open a GitHub issue if the finding can be discussed in public. For FFI or integrity bugs that might put operators at risk, email the repository owner through GitHub and wait for acknowledgement before disclosure.

Do not run AIRWAV as root. See [docs/linux.md](docs/linux.md) and [docs/safety.md](docs/safety.md).
