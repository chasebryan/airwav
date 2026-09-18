# DSP measurements and reproducibility

Unsigned interleaved I/Q bytes are converted with `(x - 127.5) / 128`. Raw bytes remain unchanged in captures. Each FFT removes the complex mean and applies a periodic Hann window. FFTW-style forward output is shifted to ascending RF frequency. Bin zero is `center - sample_rate / 2`; bins are spaced by `sample_rate / N`.

Power is `|FFT|² / sum(window)²`, averaged **in linear power** over all complete transforms in a block, then converted with `10 log10`. A complex tone of amplitude 0.5 at a bin center measures approximately −6.02 dBFS. This is coherent bin power, not calibrated antenna power or dBm, and not spectral density per Hz. Hann window equivalent noise bandwidth must be accounted for before making noise-density claims. Values below −160 dBFS are clipped for display.

FFT tails carry over arbitrary contiguous input blocks. A sample-index gap discards the tail; it is never silently bridged. There is no unvalidated I/Q imbalance correction. The waterfall contains recorded/measured spectra, not generated texture.

Noise is the 30th percentile of bin powers; the global estimate is shown in the UI. Detection uses a local estimate for each 128-bin region, requires the configured SNR threshold, ignores the central three bins and outer 2% edges, joins a one-bin gap, and requires at least two bins. Guard bins and local quantiles are explicit heuristics. Broad signals occupying most of a noise region may be underestimated; dense-band and real-capture validation remain necessary. SNR is peak bin power relative to that noise estimate, not a protocol probability.

A Signal Island is a contiguous above-threshold region, with a power-weighted centroid and measured occupied bin width. Greedy nearest compatible matches retain an ID through modest drift. Temporal state is capped at 256 islands, prioritizing current measurements and then stronger power. Candidate omissions are counted in diagnostics; this is not a claim to preserve every island under overload. One-second sample-clock hysteresis displays fading islands before retirement. Merges/splits are not yet a channel-allocation policy; ambiguous matches must not be treated as entity identity.

Tests assert tone frequency/amplitude, DC removal, two adjacent signals, a weaker neighbor, noise rejection, drift identity, state reset at gaps, arbitrary block boundaries and bounded ring suffixes. The fixture example generates a deterministic IQ recording with bursts, drift and seeded noise, visibly labeled DEMO FIXTURE. It is not a real aviation decoder fixture.

`cargo bench -p airwav-dsp --bench pipeline` measures FFT + detector + ring throughput over 13,107,200 samples. It prints MS/s, real-time factor at 2.56 MS/s and time per block. It does not measure USB or validate radio performance.
