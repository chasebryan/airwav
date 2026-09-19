//! Deterministic baseband measurements. Input is unsigned 8-bit interleaved IQ.
pub mod audio;
use airwav_core::{IqBlock, ReceiverConfig, SignalIsland, Spectrum};
use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::{collections::VecDeque, f32::consts::TAU, sync::Arc};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("DSP input error: {0}")]
pub struct DspError(pub &'static str);

pub struct SpectrumEngine {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    normalization: f32,
    pending: Vec<Complex32>,
    work: Vec<Complex32>,
    scratch: Vec<Complex32>,
    expected_sample: Option<u64>,
    pending_first: u64,
    pub discontinuities: u64,
}
impl SpectrumEngine {
    pub fn new(size: usize) -> Result<Self, DspError> {
        if !size.is_power_of_two() || !(256..=16_384).contains(&size) {
            return Err(DspError("FFT size must be a power of two in 256..=16384"));
        }
        let fft = FftPlanner::new().plan_fft_forward(size);
        let window: Vec<_> = (0..size)
            .map(|i| 0.5 - 0.5 * (TAU * i as f32 / size as f32).cos())
            .collect();
        let normalization = window.iter().sum::<f32>().powi(2);
        let scratch = vec![Complex32::default(); fft.get_inplace_scratch_len()];
        Ok(Self {
            fft,
            window,
            normalization,
            pending: Vec::with_capacity(size),
            work: vec![Complex32::default(); size],
            scratch,
            expected_sample: None,
            pending_first: 0,
            discontinuities: 0,
        })
    }
    pub fn push(
        &mut self,
        block: &IqBlock,
        receiver: &ReceiverConfig,
    ) -> Result<Option<Spectrum>, DspError> {
        if !block.bytes.len().is_multiple_of(2) {
            return Err(DspError("IQ requires complete I/Q pairs"));
        }
        if receiver.sample_rate == 0 {
            return Err(DspError("sample rate cannot be zero"));
        }
        if self
            .expected_sample
            .is_some_and(|v| v != block.first_sample)
        {
            self.pending.clear();
            self.discontinuities += 1;
        }
        self.expected_sample = Some(block.first_sample + block.samples());
        let size = self.window.len();
        let mut power = vec![0f32; size];
        let mut frames = 0u32;
        let mut first = block.first_sample;
        for (i, iq) in block.bytes.as_chunks::<2>().0.iter().enumerate() {
            if self.pending.is_empty() {
                self.pending_first = block.first_sample + i as u64;
            }
            self.pending.push(Complex32::new(
                (iq[0] as f32 - 127.5) / 128.,
                (iq[1] as f32 - 127.5) / 128.,
            ));
            if self.pending.len() == size {
                if frames == 0 {
                    first = self.pending_first;
                }
                let mean = self.pending.iter().copied().sum::<Complex32>() / size as f32;
                for ((out, value), window) in
                    self.work.iter_mut().zip(&self.pending).zip(&self.window)
                {
                    *out = (*value - mean) * window;
                }
                self.fft
                    .process_with_scratch(&mut self.work, &mut self.scratch);
                for (i, out) in power.iter_mut().enumerate() {
                    *out += self.work[(i + size / 2) % size].norm_sqr() / self.normalization;
                }
                self.pending.clear();
                frames += 1;
            }
        }
        if frames == 0 {
            return Ok(None);
        }
        for p in &mut power {
            *p = 10. * (*p / frames as f32).max(1e-16).log10();
        }
        let noise_dbfs = quantile(&power, 0.3);
        Ok(Some(Spectrum {
            first_sample: first,
            bin_hz: receiver.sample_rate as f64 / size as f64,
            start_hz: receiver.center_hz as f64 - receiver.sample_rate as f64 / 2.,
            power_dbfs: power,
            noise_dbfs,
        }))
    }
}
fn quantile(values: &[f32], q: f32) -> f32 {
    if values.is_empty() {
        return -160.;
    }
    let mut sorted = values.to_vec();
    let n = ((sorted.len() - 1) as f32 * q) as usize;
    *sorted.select_nth_unstable_by(n, f32::total_cmp).1
}

/// Adaptive spectral islands, with sample-clock hysteresis and stable observation IDs.
/// These are measured activity regions, not verified protocols or decoder channels.
pub struct Detector {
    threshold: f32,
    next_id: u64,
    tracked: Vec<SignalIsland>,
    expiry_samples: u64,
    pub candidates_omitted: u64,
}
impl Detector {
    pub fn new(threshold: f32, sample_rate: u32) -> Self {
        Self {
            threshold,
            next_id: 1,
            tracked: Vec::new(),
            expiry_samples: sample_rate as u64,
            candidates_omitted: 0,
        }
    }
    pub fn update(&mut self, spectrum: &Spectrum) -> Vec<SignalIsland> {
        let p = &spectrum.power_dbfs;
        if p.len() < 16 {
            return Vec::new();
        }
        let local: Vec<f32> = p.chunks(128).map(|region| quantile(region, 0.3)).collect();
        let mut active = vec![false; p.len()];
        let guard = (p.len() / 50).max(2);
        for i in guard..p.len() - guard {
            let noise = local[i / 128];
            active[i] =
                p[i] > noise + self.threshold && p[i] > -100. && i.abs_diff(p.len() / 2) > 1;
        }
        // A single missing bin within an island is tolerated. Wider valleys split islands.
        let original = active.clone();
        for i in 1..p.len() - 1 {
            if original[i - 1] && original[i + 1] {
                active[i] = true;
            }
        }
        let mut measured = Vec::new();
        let mut i = 0;
        while i < p.len() {
            if !active[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < p.len() && active[i] {
                i += 1;
            }
            if i - start < 2 {
                continue;
            }
            let end = i;
            let peak = p[start..end].iter().copied().fold(-160., f32::max);
            let (mut total, mut centroid) = (0f64, 0f64);
            for (j, &db) in p.iter().enumerate().take(end).skip(start) {
                let power = 10f64.powf(db as f64 / 10.);
                total += power;
                centroid += power * j as f64;
            }
            let center = spectrum.start_hz + (centroid / total) * spectrum.bin_hz;
            measured.push(SignalIsland {
                id: 0,
                first_sample: spectrum.first_sample,
                last_sample: spectrum.first_sample,
                center_hz: center,
                bandwidth_hz: (end - start) as f64 * spectrum.bin_hz,
                peak_dbfs: peak,
                snr_db: peak - local[(start + end - 1) / 2 / 128],
                observations: 1,
                state: "LIVE".into(),
            });
        }
        self.tracked.retain(|old| {
            spectrum.first_sample.saturating_sub(old.last_sample) <= self.expiry_samples
        });
        let mut used = vec![false; self.tracked.len()];
        for new in &mut measured {
            let best = self
                .tracked
                .iter()
                .enumerate()
                .filter(|(j, old)| {
                    !used[*j]
                        && (old.center_hz - new.center_hz).abs()
                            < (old.bandwidth_hz.max(new.bandwidth_hz) / 2. + 3. * spectrum.bin_hz)
                })
                .min_by(|(_, a), (_, b)| {
                    (a.center_hz - new.center_hz)
                        .abs()
                        .total_cmp(&(b.center_hz - new.center_hz).abs())
                });
            if let Some((j, old)) = best {
                new.id = old.id;
                new.first_sample = old.first_sample;
                new.observations = old.observations + 1;
                used[j] = true;
            } else {
                new.id = self.next_id;
                self.next_id += 1;
            }
        }
        for (j, old) in self.tracked.iter().enumerate() {
            if !used[j] {
                let mut fading = old.clone();
                fading.state = "FADING".into();
                measured.push(fading);
            }
        }
        // Bound temporal state even in adversarially dense or rapidly changing input.
        // Prefer current measurements, then stronger evidence; report every omission.
        measured.sort_by(|a, b| {
            b.last_sample
                .cmp(&a.last_sample)
                .then_with(|| b.peak_dbfs.total_cmp(&a.peak_dbfs))
        });
        self.candidates_omitted += measured.len().saturating_sub(256) as u64;
        measured.truncate(256);
        measured.sort_by(|a, b| a.center_hz.total_cmp(&b.center_hz));
        self.tracked = measured.clone();
        measured
    }
}

/// Bounded block ring. Blocks remain shared during event persistence.
pub struct IqRing {
    blocks: VecDeque<Arc<IqBlock>>,
    bytes: usize,
    capacity: usize,
}
impl IqRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            blocks: VecDeque::new(),
            bytes: 0,
            capacity: capacity - capacity % 2,
        }
    }
    pub fn push(&mut self, block: Arc<IqBlock>) {
        if self.capacity == 0 {
            return;
        }
        // Never cross an acquisition gap with an apparently continuous pre-trigger.
        if self
            .blocks
            .back()
            .is_some_and(|b| b.first_sample + b.samples() != block.first_sample)
        {
            self.blocks.clear();
            self.bytes = 0;
        }
        self.bytes += block.bytes.len();
        self.blocks.push_back(block);
        while self.bytes > self.capacity {
            let front = self.blocks.pop_front().expect("nonempty ring");
            let excess = self.bytes - self.capacity;
            if front.bytes.len() <= excess {
                self.bytes -= front.bytes.len();
            } else {
                let kept = IqBlock {
                    first_sample: front.first_sample + (excess / 2) as u64,
                    received_ns: front.received_ns,
                    bytes: front.bytes[excess..].to_vec(),
                };
                self.bytes -= excess;
                self.blocks.push_front(Arc::new(kept));
            }
        }
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn snapshot(&self) -> Vec<Arc<IqBlock>> {
        self.blocks.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    fn tones(n: usize, tones: &[(f32, f32)]) -> IqBlock {
        let mut bytes = Vec::new();
        for i in 0..n {
            let v = tones
                .iter()
                .map(|&(cycles, amplitude)| {
                    Complex32::from_polar(amplitude, TAU * cycles * i as f32)
                })
                .sum::<Complex32>();
            bytes.push((127.5 + v.re * 128.).round().clamp(0., 255.) as u8);
            bytes.push((127.5 + v.im * 128.).round().clamp(0., 255.) as u8);
        }
        IqBlock {
            first_sample: 0,
            received_ns: 0,
            bytes,
        }
    }
    #[test]
    fn tone_frequency_and_calibrated_amplitude() {
        let mut fft = SpectrumEngine::new(2048).unwrap();
        let s = fft
            .push(
                &tones(8192, &[(128. / 2048., 0.5)]),
                &ReceiverConfig::default(),
            )
            .unwrap()
            .unwrap();
        let peak = s
            .power_dbfs
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .unwrap();
        assert_eq!(peak.0, 1024 + 128);
        assert!((*peak.1 + 6.02).abs() < 0.1);
    }
    #[test]
    fn dc_is_removed() {
        let b = IqBlock {
            first_sample: 0,
            received_ns: 0,
            bytes: vec![180; 8192],
        };
        let s = SpectrumEngine::new(2048)
            .unwrap()
            .push(&b, &ReceiverConfig::default())
            .unwrap()
            .unwrap();
        assert!(s.power_dbfs.iter().all(|p| *p < -140.));
    }
    #[test]
    fn two_adjacent_signals_and_weak_neighbor() {
        let config = ReceiverConfig::default();
        let s = SpectrumEngine::new(2048)
            .unwrap()
            .push(
                &tones(8192, &[(128. / 2048., 0.6), (140. / 2048., 0.06)]),
                &config,
            )
            .unwrap()
            .unwrap();
        let islands = Detector::new(12., config.sample_rate).update(&s);
        let useful: Vec<_> = islands.iter().filter(|i| i.snr_db > 25.).collect();
        for bin in [128., 140.] {
            assert!(useful.iter().any(|i| {
                (i.center_hz - (config.center_hz as f64 + bin * s.bin_hz)).abs() < s.bin_hz
            }));
        }
    }
    #[test]
    fn arbitrary_block_boundaries_preserve_fft() {
        let config = ReceiverConfig::default();
        let b = tones(2048, &[(0.0625, 0.5)]);
        let expected = SpectrumEngine::new(2048)
            .unwrap()
            .push(&b, &config)
            .unwrap()
            .unwrap();
        let mut engine = SpectrumEngine::new(2048).unwrap();
        let mut actual = None;
        for (n, data) in b.bytes.chunks(200).enumerate() {
            let block = IqBlock {
                first_sample: n as u64 * 100,
                received_ns: 0,
                bytes: data.to_vec(),
            };
            actual = engine.push(&block, &config).unwrap().or(actual);
        }
        assert_eq!(actual.unwrap().power_dbfs, expected.power_dbfs);
    }
    #[test]
    fn am_sidebands_have_expected_frequency_and_ratio() {
        let n = 2048usize;
        let mut bytes = Vec::new();
        for i in 0..n {
            let envelope = 0.4 * (1. + 0.5 * (TAU * 8. * i as f32 / n as f32).cos());
            let v = Complex32::from_polar(envelope, TAU * 128. * i as f32 / n as f32);
            bytes.extend([
                (127.5 + 128. * v.re).round() as u8,
                (127.5 + 128. * v.im).round() as u8,
            ]);
        }
        let block = IqBlock {
            first_sample: 0,
            received_ns: 0,
            bytes,
        };
        let s = SpectrumEngine::new(n)
            .unwrap()
            .push(&block, &ReceiverConfig::default())
            .unwrap()
            .unwrap();
        let carrier = s.power_dbfs[1024 + 128];
        for index in [1024 + 120, 1024 + 136] {
            assert!((carrier - s.power_dbfs[index] - 12.04).abs() < 0.2);
        }
    }
    #[test]
    fn fm_has_expected_sidebands_and_total_power() {
        let n = 2048usize;
        let mut bytes = Vec::new();
        for i in 0..n {
            let phase =
                TAU * 128. * i as f32 / n as f32 + 2. * (TAU * 8. * i as f32 / n as f32).sin();
            let v = Complex32::from_polar(0.5, phase);
            bytes.extend([
                (127.5 + 128. * v.re).round() as u8,
                (127.5 + 128. * v.im).round() as u8,
            ]);
        }
        let s = SpectrumEngine::new(n)
            .unwrap()
            .push(
                &IqBlock {
                    first_sample: 0,
                    received_ns: 0,
                    bytes,
                },
                &ReceiverConfig::default(),
            )
            .unwrap()
            .unwrap();
        let total: f32 = s.power_dbfs.iter().map(|p| 10f32.powf(p / 10.)).sum();
        assert!((total - 0.25 * 1.5).abs() < 0.01); // Hann equivalent noise bandwidth is 1.5 bins.
        for index in [1024 + 120, 1024 + 136] {
            assert!(s.power_dbfs[index] > s.power_dbfs[1024 + 128] + 6.);
        }
    }
    #[test]
    fn gap_discards_partial_fft() {
        let mut engine = SpectrumEngine::new(256).unwrap();
        let mut b = tones(128, &[(0.0625, 0.5)]);
        let c = ReceiverConfig::default();
        assert!(engine.push(&b, &c).unwrap().is_none());
        b.first_sample = 1000;
        assert!(engine.push(&b, &c).unwrap().is_none());
        assert_eq!(engine.discontinuities, 1);
    }
    #[test]
    fn drift_retains_id_and_hysteresis_expires() {
        let c = ReceiverConfig::default();
        let mut fft = SpectrumEngine::new(2048).unwrap();
        let mut d = Detector::new(12., c.sample_rate);
        let s = fft
            .push(&tones(2048, &[(0.0625, 0.5)]), &c)
            .unwrap()
            .unwrap();
        let a = d.update(&s);
        let id = a
            .iter()
            .max_by(|a, b| a.peak_dbfs.total_cmp(&b.peak_dbfs))
            .unwrap()
            .id;
        let mut moved = s.clone();
        moved.start_hz += s.bin_hz;
        moved.first_sample = 2048;
        assert!(
            d.update(&moved)
                .iter()
                .any(|i| i.id == id && i.observations == 2)
        );
        moved.power_dbfs.fill(-100.);
        moved.first_sample += c.sample_rate as u64 + 1;
        assert!(d.update(&moved).is_empty());
    }
    #[test]
    fn live_and_fading_are_distinct_activity_states() {
        let c = ReceiverConfig::default();
        let mut fft = SpectrumEngine::new(2048).unwrap();
        let mut d = Detector::new(12., c.sample_rate);
        let s = fft
            .push(&tones(2048, &[(0.0625, 0.5)]), &c)
            .unwrap()
            .unwrap();
        let live = d.update(&s);
        assert!(
            live.iter().any(|i| i.state == "LIVE" && i.snr_db > 20.),
            "{live:?}"
        );
        let mut gone = s.clone();
        gone.power_dbfs.fill(-100.);
        gone.first_sample = 4096;
        let fading = d.update(&gone);
        assert!(fading.iter().any(|i| i.state == "FADING"), "{fading:?}");
        assert!(fading.iter().all(|i| i.state != "UNKNOWN"));
    }
    #[test]
    fn seeded_noise_has_no_persistent_islands() {
        let mut seed = 123u32;
        let mut bytes = Vec::new();
        for _ in 0..65_536 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            bytes.push(127 + ((seed >> 28) as u8) - 8);
        }
        let b = IqBlock {
            first_sample: 0,
            received_ns: 0,
            bytes,
        };
        let c = ReceiverConfig::default();
        let s = SpectrumEngine::new(2048)
            .unwrap()
            .push(&b, &c)
            .unwrap()
            .unwrap();
        assert!(Detector::new(12., c.sample_rate).update(&s).is_empty());
    }
    proptest! {
        #[test] fn ring_is_bounded_and_keeps_suffix(capacity in 0usize..10000, sizes in prop::collection::vec(1usize..4096,1..30)) {
            let mut ring=IqRing::new(capacity);let mut first=0;
            for size in sizes { ring.push(Arc::new(IqBlock{first_sample:first,received_ns:0,bytes:vec![42;size*2]}));first+=size as u64; }
            prop_assert!(ring.bytes()<=capacity);
            let blocks=ring.snapshot();
            if let Some(last)=blocks.last(){prop_assert_eq!(last.first_sample+last.samples(),first);}
            for pair in blocks.windows(2) {prop_assert_eq!(pair[0].first_sample+pair[0].samples(),pair[1].first_sample);}
        }
    }
    #[test]
    fn dense_spectrum_has_a_bounded_track_budget() {
        let mut p = vec![-100.; 16384];
        for i in (400..16000).step_by(8) {
            p[i] = -10.;
            p[i + 1] = -10.;
        }
        let s = Spectrum {
            first_sample: 0,
            bin_hz: 156.25,
            start_hz: 134_720_000.,
            power_dbfs: p,
            noise_dbfs: -100.,
        };
        let mut detector = Detector::new(12., 2_560_000);
        assert_eq!(detector.update(&s).len(), 256);
        assert!(detector.candidates_omitted > 0);
    }
    #[test]
    fn ring_resets_on_drop() {
        let mut ring = IqRing::new(100);
        for i in [0, 100] {
            ring.push(Arc::new(IqBlock {
                first_sample: i,
                received_ns: 0,
                bytes: vec![0; 10],
            }));
        }
        assert_eq!(ring.bytes(), 10);
    }
}
