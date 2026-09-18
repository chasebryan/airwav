# AM and FM audio

`airwav audio` converts one verified AWR IQ event into a **48 kHz, mono, signed 16-bit PCM WAV**. It runs offline without a receiver, librtlsdr, an audio server, or FFmpeg. It can optionally play the completed WAV through a system player.

```bash
airwav inspect session.awr
airwav audio session.awr --event AW-EVENT-000001 --mode am \
  --frequency-hz 125000000 --output voice.wav --play
airwav audio session.awr --mode fm --frequency-hz 99500000 \
  --deemphasis-us 75 --output radio.wav --play
airwav audio session.awr --mode nfm --output narrow-voice.wav
```

The frequencies above are examples, not tuning instructions for an arbitrary recording: the entire selected channel must fit within its captured IQ bandwidth. Omit `--frequency-hz` to use the event's receiver center. `--event` is optional only when the recording has exactly one event. `inspect` includes a bounded `event_index` with IDs, lengths, completion status and timestamps (first 4,096 entries); larger recordings retain every event in `events.jsonl`. Audio covers the selected event's IQ, not the full metadata session. An incomplete event exports its available contiguous IQ with `event_complete: false` in the result.

| Mode | Channel filter cutoff | Audio cutoff | Demodulation |
| --- | --- | --- | --- |
| `am` | 5 kHz | 3.5 kHz | Envelope, carrier/DC removal; voice-oriented |
| `fm` | 100 kHz | 15 kHz | Quadrature phase difference, 75 kHz nominal deviation; mono broadcast FM |
| `nfm` | 6 kHz | 3.5 kHz | Quadrature phase difference, 2.5 kHz nominal deviation |

These are filter cutoffs, not guaranteed flat passbands or validated channel masks. Mode selection is manual and does not establish that a signal is an aviation transmission, a supported protocol, or a verified identity. Broadcast stereo and RDS are not decoded.

`--gain` sets a fixed linear audio gain from 0 to 20 (default 0.8); this does not change receiver gain. There is no automatic gain control. Output saturates at the PCM range and reports `clipped_samples`. Optional `--squelch-dbfs -50` gates on smoothed channel power, not calibrated RF power or SNR. Broadcast FM de-emphasis accepts 0, 50 or 75 microseconds (default 75); AM/NFM use 0. Select the value appropriate to the recording.

## Evidence and file behavior

Before export, AIRWAV validates the selected event's evidence, chunk continuity, artifact length, path containment and BLAKE3 digest. It then streams from the same verified file descriptor in 64 KiB blocks. Output is written to a temporary file and published without overwriting only after success. Cancellation removes the temporary output. The destination must be outside the source recording. Existing recordings are never changed.

The WAV embeds a RIFF `LIST/INFO/ICMT` JSON comment with the session ID, event ID, source, IQ hash, first sample, tuning/mode/filter-version parameters, sample count, completion and clipping status. The same metadata is printed to stdout. Synthetic input retains **DEMO FIXTURE** in the embedded metadata; the derived WAV is not evidence of received RF. Players may ignore metadata when displaying files.

`--play` tries `pw-play`, `paplay`, `aplay`, then `ffplay`, choosing the first executable available. A player failure is reported, and the completed WAV remains available. Ctrl+C stops playback. No sound plays unless `--play` is supplied. There is no live receiver audio or synchronized TUI replay audio in this increment.

## DSP and validation scope

The streaming pipeline mixes the requested channel to zero frequency, applies a normalized Blackman-windowed sinc FIR before integer decimation, demodulates, removes DC with a 30 Hz one-pole filter, applies optional FM de-emphasis/squelch, then resamples through a 256-phase, 129-tap audio low-pass bank to 48 kHz. Oscillator and filter state cross input block boundaries; discontinuities are rejected. Initial RF filter settling is muted. Filter delay remains in the WAV, and the tail is not padded. For very short events, the filter transient can dominate the audio. No sample-rate calibration beyond recorded receiver settings is inferred.

Tests use independently generated AM/FM/NFM analytic IQ vectors to check recovered tone frequency/purity, both tuning directions, neighboring-channel suppression, audio anti-alias filtering, de-emphasis, irregular block boundaries, non-integer rate conversion, invalid input, corrupted recordings, WAV structure, metadata and overwrite refusal. They do **not** replace independent real RF fixtures, listening comparisons, or physical V4 acceptance. Live audio integration remains gated on the hardware procedure.

References for the signal/format definitions: [GNU Radio AM demodulation](https://wiki.gnuradio.org/index.php/AM_Demod), [GNU Radio quadrature demodulation](https://www.gnuradio.org/doc/doxygen/classgr_1_1analog_1_1quadrature__demod__cf.html), [RIFF/WAVE format](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/WAVE.html). The AIRWAV implementation is native Rust and does not depend on GNU Radio.

## Reproducible audible fixture

```bash
cargo run --release --example make_audio_fixture -- /tmp/audio-demo.awr
cargo run --release --bin airwav -- audio /tmp/audio-demo.awr \
  --mode am --frequency-hz 135800000 --output /tmp/am-demo.wav
cargo run --release --bin airwav -- audio /tmp/audio-demo.awr \
  --mode fm --frequency-hz 136250000 --output /tmp/fm-demo.wav
cargo bench -p airwav-dsp --bench audio --locked
```

This produces two seconds of synthetic 440 Hz AM and 660 Hz FM tones. The generator is an explicit development example and cannot be selected as a live receiver.
