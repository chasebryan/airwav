# Physical V4 acceptance gate

**Status: not performed in the development environment (no exposed USB receiver).** A passing test-only driver is not a hardware acceptance result.

Record the host, kernel, USB controller/path, V4 serial, vendor driver commit/library hash, build commit, sample rate, terminal and date in a new results file. Preserve diagnostic output and AWR artifacts. Use a lawful receive setup.

1. Confirm `airwav devices` identifies the V4 and explicitly rejects an incompatible RTL receiver. Confirm Blog V4 Lite is not accepted.
2. Run `airwav doctor --stream-seconds 60 --counter-test` repeatedly, including a 30-minute soak under normal desktop load. There must be no observed counter discontinuities, application drops, malformed bytes, or callback panics. Compare measured rate to the configured rate. The modulo-256 counter cannot prove absence of all USB loss; record this limitation.
3. Repeat normal RF streaming at 2.56 MS/s. Inspect diagnostics while resizing, zooming, changing themes, pausing the view, and saving screenshots. Check that view operations do not cause application drops.
4. Check a known frequency reference and gain/PPM settings. Confirm bias-tee off policy and V4 band behavior using appropriate equipment; no software-only test establishes calibration.
5. Start recording, collect at least five seconds of pre-roll, then capture an event. Confirm ten seconds of post-roll and the absence of marked gaps. Stop cleanly and verify with `airwav inspect`.
6. Disconnect the receiver during recording. Confirm capture stops without panic or deadlock, available metadata is retained, and a partial event never claims complete IQ. Reconnect and launch again.
7. Stop the process abruptly during recording. Preserve the original, recover to a new `.awr`, and inspect that bundle. Check that a torn final row is omitted and earlier committed artifacts verify.
8. Disconnect hardware and replay, change speed, pause/step, navigate events and export SVG. Inspect source and receiver metadata; Demo Mode must not add signals.
9. Exercise recording size and minimum-free-space limits in a disposable test directory. Confirm the recorder reports the limit and does not delete existing data.
10. Check permission failures, an occupied USB interface, an outdated driver, a missing library, multiple devices and unsupported gain values. Each error should name the failing condition and remedy.

Only after evidence from these checks should the live capture milestone be marked accepted. Then proceed to PRISM channel DSP and the first independently fixture-validated aviation decoder.
