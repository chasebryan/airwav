//! Offline mono audio from a selected IQ channel. No protocol or identity inference.
use crate::DspError;
use airwav_core::{IqBlock, MAX_SAMPLE_RATE};
use num_complex::{Complex32, Complex64};
use std::f64::consts::{PI, TAU};

pub const AUDIO_RATE: u32 = 48_000;
const AUDIO_TAPS: usize = 129;
const PHASES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMode {
    Am,
    /// Broadcast FM, mono, nominal peak deviation 75 kHz.
    Fm,
    /// Narrow FM voice, nominal peak deviation 2.5 kHz.
    Nfm,
}
impl AudioMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Am => "am",
            Self::Fm => "fm",
            Self::Nfm => "nfm",
        }
    }
}
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub input_rate: u32,
    pub offset_hz: f64,
    pub mode: AudioMode,
    pub gain: f32,
    /// Channel power threshold; None disables squelch. This is dBFS, not calibrated RF power.
    pub squelch_dbfs: Option<f32>,
    /// 0, 50 or 75 microseconds. Only applicable to broadcast FM.
    pub deemphasis_us: u32,
}

/// Blackman-windowed sinc, normalized for unity DC gain.
fn lowpass(len: usize, cutoff: f64, delay: f64) -> Vec<f32> {
    let mut taps: Vec<_> = (0..len)
        .map(|i| {
            let x = i as f64 - (len - 1) as f64 / 2. - delay;
            let sinc = if x.abs() < 1e-12 {
                2. * cutoff
            } else {
                (TAU * cutoff * x).sin() / (PI * x)
            };
            let angle = TAU * i as f64 / (len - 1) as f64;
            (sinc * (0.42 - 0.5 * angle.cos() + 0.08 * (2. * angle).cos())) as f32
        })
        .collect();
    let sum: f32 = taps.iter().sum();
    for tap in &mut taps {
        *tap /= sum;
    }
    taps
}

/// State is bounded by filter lengths; output is appended to a caller-owned buffer.
/// IQ must be contiguous; start a new demodulator after any acquisition gap.
pub struct AudioDemodulator {
    config: AudioConfig,
    decimation: u32,
    decimation_phase: u32,
    rf_taps: Vec<f32>,
    rf_history: Vec<Complex32>,
    rf_cursor: usize,
    rf_settling: usize,
    oscillator: Complex64,
    rotation: Complex64,
    oscillator_ticks: u32,
    previous_iq: Complex32,
    dc: f32,
    dc_alpha: f32,
    power: f32,
    power_alpha: f32,
    squelch_power: f32,
    deemphasis: f32,
    deemphasis_alpha: f32,
    fm_gain: f32,
    audio_taps: Vec<Vec<f32>>,
    audio_history: [f32; AUDIO_TAPS],
    audio_cursor: usize,
    output_clock: u64,
    expected_sample: Option<u64>,
    pub clipped_samples: u64,
}
impl AudioDemodulator {
    pub fn new(config: AudioConfig) -> Result<Self, DspError> {
        if !(900_001..=MAX_SAMPLE_RATE).contains(&config.input_rate) {
            return Err(DspError("audio input rate must be 900001..=2560000 Hz"));
        }
        if !config.gain.is_finite() || !(0.0..=20.0).contains(&config.gain) {
            return Err(DspError("audio gain must be finite and between 0 and 20"));
        }
        if config
            .squelch_dbfs
            .is_some_and(|v| !v.is_finite() || !(-120.0..=0.0).contains(&v))
        {
            return Err(DspError(
                "squelch must be finite and between -120 and 0 dBFS",
            ));
        }
        if ![0, 50, 75].contains(&config.deemphasis_us)
            || (config.mode != AudioMode::Fm && config.deemphasis_us != 0)
        {
            return Err(DspError(
                "de-emphasis is 0, 50 or 75 us, and only available for FM",
            ));
        }
        let (target_rate, rf_cutoff, audio_cutoff, deviation) = match config.mode {
            AudioMode::Am => (48_000, 5_000., 3_500., 1.),
            AudioMode::Fm => (256_000, 100_000., 15_000., 75_000.),
            AudioMode::Nfm => (48_000, 6_000., 3_500., 2_500.),
        };
        // Leave room for the channel filter's transition band at the recording edge.
        if !config.offset_hz.is_finite()
            || config.offset_hz.abs() + rf_cutoff + target_rate as f64 / 8.
                > config.input_rate as f64 / 2.
        {
            return Err(DspError(
                "selected audio channel extends outside the recorded IQ bandwidth",
            ));
        }
        let decimation = config.input_rate / target_rate;
        let channel_rate = config.input_rate as f64 / decimation as f64;
        let rf_taps = lowpass(
            64 * decimation as usize + 1,
            rf_cutoff / config.input_rate as f64,
            0.,
        );
        let rf_history = vec![Complex32::default(); rf_taps.len()];
        let audio_taps = (0..PHASES)
            .map(|phase| {
                lowpass(
                    AUDIO_TAPS,
                    audio_cutoff / channel_rate,
                    phase as f64 / PHASES as f64,
                )
            })
            .collect();
        Ok(Self {
            rotation: Complex64::from_polar(1., -TAU * config.offset_hz / config.input_rate as f64),
            oscillator: Complex64::new(1., 0.),
            oscillator_ticks: 0,
            decimation,
            decimation_phase: 0,
            rf_settling: rf_taps.len().div_ceil(decimation as usize),
            rf_taps,
            rf_history,
            rf_cursor: 0,
            previous_iq: Complex32::default(),
            dc: 0.,
            dc_alpha: (1. - (-TAU * 30. / channel_rate).exp()) as f32,
            power: 0.,
            power_alpha: (1. - (-1. / (0.01 * channel_rate)).exp()) as f32,
            squelch_power: config
                .squelch_dbfs
                .map(|db| 10f32.powf(db / 10.))
                .unwrap_or(0.),
            deemphasis: 0.,
            deemphasis_alpha: if config.deemphasis_us == 0 {
                1.
            } else {
                (1. - (-1. / (channel_rate * config.deemphasis_us as f64 * 1e-6)).exp()) as f32
            },
            fm_gain: (channel_rate / (TAU * deviation)) as f32,
            audio_taps,
            audio_history: [0.; AUDIO_TAPS],
            audio_cursor: 0,
            output_clock: 0,
            expected_sample: None,
            clipped_samples: 0,
            config,
        })
    }
    pub fn push(&mut self, block: &IqBlock, output: &mut Vec<i16>) -> Result<(), DspError> {
        if !block.bytes.len().is_multiple_of(2) {
            return Err(DspError("audio IQ requires complete I/Q pairs"));
        }
        let end = block
            .first_sample
            .checked_add(block.samples())
            .ok_or(DspError("audio sample position overflow"))?;
        if self
            .expected_sample
            .is_some_and(|s| s != block.first_sample)
        {
            return Err(DspError(
                "audio IQ is discontinuous; start a new audio segment",
            ));
        }
        self.expected_sample = Some(end);
        for iq in block.bytes.as_chunks::<2>().0 {
            let sample =
                Complex32::new((iq[0] as f32 - 127.5) / 128., (iq[1] as f32 - 127.5) / 128.);
            self.rf_cursor = (self.rf_cursor + self.rf_history.len() - 1) % self.rf_history.len();
            self.rf_history[self.rf_cursor] =
                sample * Complex32::new(self.oscillator.re as f32, self.oscillator.im as f32);
            self.oscillator *= self.rotation;
            self.oscillator_ticks += 1;
            if self.oscillator_ticks == 4096 {
                self.oscillator /= self.oscillator.norm();
                self.oscillator_ticks = 0;
            }
            self.decimation_phase += 1;
            if self.decimation_phase != self.decimation {
                continue;
            }
            self.decimation_phase = 0;
            // Evaluate the decimating FIR only when an output sample is due.
            let history = self.rf_history[self.rf_cursor..]
                .iter()
                .chain(&self.rf_history[..self.rf_cursor]);
            let channel: Complex32 = history.zip(&self.rf_taps).map(|(v, tap)| *v * *tap).sum();
            let power = channel.norm_sqr();
            self.power += self.power_alpha * (power - self.power);
            let demodulated = if self.rf_settling > 0 {
                self.rf_settling -= 1;
                self.previous_iq = channel;
                0.
            } else {
                match self.config.mode {
                    AudioMode::Am => channel.norm() * 2.,
                    AudioMode::Fm | AudioMode::Nfm => {
                        let product = channel * self.previous_iq.conj();
                        self.previous_iq = channel;
                        product.arg() * self.fm_gain
                    }
                }
            };
            self.dc += self.dc_alpha * (demodulated - self.dc);
            self.deemphasis += self.deemphasis_alpha * (demodulated - self.dc - self.deemphasis);
            let audio = if self.power >= self.squelch_power {
                self.deemphasis
            } else {
                0.
            };
            self.audio_cursor = (self.audio_cursor + AUDIO_TAPS - 1) % AUDIO_TAPS;
            self.audio_history[self.audio_cursor] = audio;
            self.output_clock += AUDIO_RATE as u64 * self.decimation as u64;
            if self.output_clock >= self.config.input_rate as u64 {
                self.output_clock -= self.config.input_rate as u64;
                let phase = (self.output_clock * PHASES as u64
                    / (AUDIO_RATE as u64 * self.decimation as u64))
                    as usize;
                let history = self.audio_history[self.audio_cursor..]
                    .iter()
                    .chain(&self.audio_history[..self.audio_cursor]);
                let value: f32 = history
                    .zip(&self.audio_taps[phase])
                    .map(|(v, tap)| v * tap)
                    .sum::<f32>()
                    * self.config.gain;
                if value.abs() > 1. {
                    self.clipped_samples += 1;
                }
                output.push((value.clamp(-1., 1.) * i16::MAX as f32).round() as i16);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(mode: AudioMode, rate: u32, offset: f64) -> AudioConfig {
        AudioConfig {
            input_rate: rate,
            offset_hz: offset,
            mode,
            gain: 0.8,
            squelch_dbfs: None,
            deemphasis_us: 0,
        }
    }
    fn signal(mode: AudioMode, rate: u32, offset: f64, hz: f64, seconds: f64) -> IqBlock {
        let bytes = (0..(rate as f64 * seconds) as usize)
            .flat_map(|i| {
                let time = i as f64 / rate as f64;
                let audio_phase = TAU * hz * time;
                let (amp, phase) = match mode {
                    AudioMode::Am => (0.35 * (1. + 0.6 * audio_phase.cos()), TAU * offset * time),
                    AudioMode::Fm => (0.6, TAU * offset * time + 50_000. / hz * audio_phase.sin()),
                    AudioMode::Nfm => (0.6, TAU * offset * time + 1_500. / hz * audio_phase.sin()),
                };
                [
                    (127.5 + 128. * amp * phase.cos()).round().clamp(0., 255.) as u8,
                    (127.5 + 128. * amp * phase.sin()).round().clamp(0., 255.) as u8,
                ]
            })
            .collect();
        IqBlock {
            first_sample: 0,
            received_ns: 0,
            bytes,
        }
    }
    fn amplitude(pcm: &[i16], hz: f64) -> f64 {
        let sum: Complex64 = pcm
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Complex64::from_polar(*s as f64 / 32768., TAU * hz * i as f64 / AUDIO_RATE as f64)
            })
            .sum();
        2. * sum.norm() / pcm.len() as f64
    }
    #[test]
    fn am_and_fm_recover_known_tones_at_positive_and_negative_offsets() {
        for mode in [AudioMode::Am, AudioMode::Fm, AudioMode::Nfm] {
            for offset in [-180_000., 180_000.] {
                let rate = 1_024_000;
                let block = signal(mode, rate, offset, 1_000., 0.12);
                let mut dsp = AudioDemodulator::new(config(mode, rate, offset)).unwrap();
                let mut pcm = vec![];
                dsp.push(&block, &mut pcm).unwrap();
                assert!((pcm.len() as i64 - 5760).abs() <= 1);
                let samples = &pcm[960..5760.min(pcm.len())];
                let tone = amplitude(samples, 1_000.);
                assert!(tone > 0.25, "{mode:?}: amplitude {tone}");
                let rms = (samples
                    .iter()
                    .map(|s| (*s as f64 / 32768.).powi(2))
                    .sum::<f64>()
                    / samples.len() as f64)
                    .sqrt();
                assert!(tone / (2f64.sqrt() * rms) > 0.99, "{mode:?}: tone purity");
                assert_eq!(dsp.clipped_samples, 0);
            }
        }
    }
    #[test]
    fn irregular_blocks_match_one_block_and_noninteger_rate_keeps_duration() {
        let rate = 900_001;
        let block = signal(AudioMode::Fm, rate, 140_000., 1_000., 0.08);
        let c = config(AudioMode::Fm, rate, 140_000.);
        let mut expected = vec![];
        AudioDemodulator::new(c.clone())
            .unwrap()
            .push(&block, &mut expected)
            .unwrap();
        let mut actual = vec![];
        let mut dsp = AudioDemodulator::new(c).unwrap();
        let mut first = 0;
        for bytes in block.bytes.chunks(1994) {
            let b = IqBlock {
                first_sample: first,
                received_ns: 0,
                bytes: bytes.to_vec(),
            };
            first += b.samples();
            dsp.push(&b, &mut actual).unwrap();
        }
        assert_eq!(actual, expected);
        assert!((actual.len() as i64 - 3840).abs() <= 1);
    }
    #[test]
    fn channel_filter_rejects_neighbor_and_audio_filter_rejects_out_of_band_tone() {
        for (mode, offset, hz) in [
            (AudioMode::Am, 50_000., 1_000.),
            (AudioMode::Fm, 400_000., 1_000.),
            (AudioMode::Fm, 0., 30_000.),
        ] {
            let block = signal(mode, 1_024_000, offset, hz, 0.08);
            let mut c = config(mode, 1_024_000, 0.);
            c.squelch_dbfs = Some(-50.);
            let mut pcm = vec![];
            AudioDemodulator::new(c)
                .unwrap()
                .push(&block, &mut pcm)
                .unwrap();
            let rms = (pcm[960..]
                .iter()
                .map(|s| (*s as f64 / 32768.).powi(2))
                .sum::<f64>()
                / (pcm.len() - 960) as f64)
                .sqrt();
            assert!(
                rms < 0.01,
                "{mode:?}, offset {offset}, tone {hz}: RMS {rms}"
            );
        }
    }
    #[test]
    fn desired_audio_survives_a_simultaneous_adjacent_channel() {
        let rate = 1_024_000;
        for (mode, neighbor_hz) in [(AudioMode::Am, 50_000.), (AudioMode::Fm, 300_000.)] {
            let make_block = |with_neighbor: bool| IqBlock {
                first_sample: 0,
                received_ns: 0,
                bytes: (0..rate / 10)
                    .flat_map(|i| {
                        let t = i as f64 / rate as f64;
                        let desired_tone = TAU * 1_000. * t;
                        let adjacent_tone = TAU * 2_000. * t;
                        let desired = match mode {
                            AudioMode::Am => {
                                Complex64::from_polar(0.3 * (1. + 0.5 * desired_tone.cos()), 0.)
                            }
                            AudioMode::Fm => {
                                Complex64::from_polar(0.3, 50_000. / 1_000. * desired_tone.sin())
                            }
                            AudioMode::Nfm => unreachable!(),
                        };
                        let adjacent = if with_neighbor {
                            match mode {
                                AudioMode::Am => Complex64::from_polar(
                                    0.3 * (1. + 0.5 * adjacent_tone.cos()),
                                    TAU * neighbor_hz * t,
                                ),
                                AudioMode::Fm => Complex64::from_polar(
                                    0.3,
                                    TAU * neighbor_hz * t + 50_000. / 2_000. * adjacent_tone.sin(),
                                ),
                                AudioMode::Nfm => unreachable!(),
                            }
                        } else {
                            Complex64::new(0., 0.)
                        };
                        let iq = desired + adjacent;
                        [
                            (127.5 + 128. * iq.re).round().clamp(0., 255.) as u8,
                            (127.5 + 128. * iq.im).round().clamp(0., 255.) as u8,
                        ]
                    })
                    .collect(),
            };
            let decode = |block: IqBlock| {
                let mut pcm = vec![];
                AudioDemodulator::new(config(mode, rate, 0.))
                    .unwrap()
                    .push(&block, &mut pcm)
                    .unwrap();
                pcm
            };
            let baseline = decode(make_block(false));
            let mixed = decode(make_block(true));
            let desired = amplitude(&mixed[960..], 1_000.);
            let leakage = amplitude(&mixed[960..], 2_000.);
            let baseline_desired = amplitude(&baseline[960..], 1_000.);
            let baseline_leakage = amplitude(&baseline[960..], 2_000.);
            assert!(
                desired > baseline_desired * 0.75,
                "{mode:?}: desired {desired} versus {baseline_desired}"
            );
            assert!(
                leakage < desired * 0.1 && leakage < baseline_leakage + 0.03,
                "{mode:?}: adjacent leakage {leakage} versus desired {desired}, baseline {baseline_leakage}"
            );
        }
    }
    #[test]
    fn invalid_input_and_gaps_leave_output_unchanged() {
        let c = config(AudioMode::Am, 1_024_000, 0.);
        for bad in [
            AudioConfig {
                offset_hz: f64::NAN,
                ..c.clone()
            },
            AudioConfig {
                offset_hz: 510_000.,
                ..c.clone()
            },
            AudioConfig {
                gain: f32::INFINITY,
                ..c.clone()
            },
            AudioConfig {
                deemphasis_us: 75,
                ..c.clone()
            },
        ] {
            assert!(AudioDemodulator::new(bad).is_err());
        }
        let mut dsp = AudioDemodulator::new(c).unwrap();
        let mut pcm = vec![];
        let block = IqBlock {
            first_sample: 0,
            received_ns: 0,
            bytes: vec![128; 1024],
        };
        dsp.push(&block, &mut pcm).unwrap();
        let len = pcm.len();
        assert!(dsp.push(&block, &mut pcm).is_err());
        assert!(
            dsp.push(
                &IqBlock {
                    first_sample: 512,
                    received_ns: 0,
                    bytes: vec![1]
                },
                &mut pcm
            )
            .is_err()
        );
        assert_eq!(pcm.len(), len);
    }
    #[test]
    fn fm_deemphasis_attenuates_high_audio_frequencies() {
        let block = signal(AudioMode::Fm, 1_024_000, 0., 10_000., 0.08);
        let mut levels = vec![];
        for us in [0, 50, 75] {
            let mut c = config(AudioMode::Fm, 1_024_000, 0.);
            c.deemphasis_us = us;
            let mut pcm = vec![];
            AudioDemodulator::new(c)
                .unwrap()
                .push(&block, &mut pcm)
                .unwrap();
            levels.push(amplitude(&pcm[960..], 10_000.));
        }
        assert!(levels[1] < levels[0] * 0.4);
        assert!(levels[2] < levels[1] * 0.8);
    }
}
