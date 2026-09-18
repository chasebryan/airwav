# PRISM — Parallel RF Interpretation & Spectrum Management

PRISM is the future adaptive channelization subsystem. This milestone provides its measured input: contiguous spectral Signal Islands with center, bandwidth, power, SNR, duration by sample position, observation count, drift-tolerant IDs and fading lifecycle.

A Signal Island is not yet a processing lane. No fixed 25 kHz partition is used and no inactive decoder is presented as running. The UI draws selected island centroids over measured waterfall history.

The next implementation must allocate bounded channel DSP from the observed widths, apply numerically tested DDC and anti-alias filtering before decimation/resampling, and retain merge/split relationships as immutable observations. Benchmarks and adjacent/unequal-power signal vectors must precede live decoder dispatch.
