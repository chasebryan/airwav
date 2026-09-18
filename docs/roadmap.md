# Milestone ledger

The full product brief defines the intended release, not capabilities already shipped. Live receiver validation is a release gate. The additional offline inspection tools here allow the capture pipeline to be exercised without implying that later phases are complete.

| Product phase | Implemented | Remaining gate/work |
| --- | --- | --- |
| 1. V4 foundation | Strict USB identity, R828D and clock validation, gain/PPM/bias configuration, async IQ, bounded queue, counters, doctor, CLI, ABI/lifecycle tests | Physical V4 sustained-ingest acceptance, USB permissions and disconnect testing on Fedora/i3 |
| 2. DSP foundation | Hann FFT, spectral averaging, noise estimate, activity islands, bounded raw ring, vectors and benchmark | Real-capture validation; independent recorded burst vectors, IQ imbalance assessment |
| 3. Native terminal | Spectrum, real measurement waterfall, evidence, diagnostics, mouse selection/buttons, zoom/pan, resize, five themes, keyboard help | Full settings/control parity, context menus, drag selection, lock/retune/settings controls, recording elapsed time, in-app log panel |
| 4. PRISM | First-class measured Signal Islands and drift/hysteresis tracking | Adaptive DDC/filtering/resampling, allocation budget, merge/split histories, channel DSP validation |
| 5. Recording/replay | Bounded pre/post IQ, AWR v1, hashes, SQLite, crash-safe journals, recovery to new bundle, semantic replay controls, SVG export, event-linked AM/FM/NFM WAV export and playback | Automatic event policy, event-linked screenshots, PNG, rotation/cleanup UI, robust event-browser indexing |
| 6. First decoder | No support claimed | Mode S/1090ES detection, demodulation, extraction, CRC/invariants, independent known-good RF fixtures, parsed fields and evidence integration |
| 7. MAX-I | No scores fabricated | Separate tested scoring components, explore/exploit/follow/verify, starvation budget, tune decisions and logged explanations |
| 8. More aviation | Offline AM and FM demodulation from verified IQ; synthetic tone validation only | Physical/independent RF audio acceptance and live monitoring; UAT, ACARS and VDL2 added independently after validated fixtures and legal module controls |
| 9. AIRWAV Memory | Immutable observation/event persistence | Measured fingerprints, similarity calibration, observation linking, immutable superseding interpretations |
| 10. Demo system | Presentation-only Demo Mode, Studio theme, SVG cell-buffer export | Director Mode from recorded events, PNG, cast, optional FFmpeg video workflow |

Do not advance the live capture milestone without completing hardware-acceptance.md. Do not display a supported protocol merely because its center frequency matches an aviation band. UNKNOWN is the default until a verified decoder has justified a stronger statement.
