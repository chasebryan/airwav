use airwav_core::{IqBlock, MAX_SAMPLE_RATE};
use airwav_dsp::audio::{AudioConfig, AudioDemodulator, AudioMode};
use std::{hint::black_box, time::Instant};

fn main() {
    for mode in [AudioMode::Am, AudioMode::Fm, AudioMode::Nfm] {
        let mut dsp = AudioDemodulator::new(AudioConfig {
            input_rate: MAX_SAMPLE_RATE,
            offset_hz: 160_000.,
            mode,
            gain: 0.8,
            squelch_dbfs: None,
            deemphasis_us: if mode == AudioMode::Fm { 75 } else { 0 },
        })
        .unwrap();
        let mut block = IqBlock {
            first_sample: 0,
            received_ns: 0,
            bytes: vec![128; 65536],
        };
        let mut pcm = Vec::with_capacity(2048);
        let start = Instant::now();
        for _ in 0..128 {
            pcm.clear();
            dsp.push(black_box(&block), black_box(&mut pcm)).unwrap();
            black_box(&pcm);
            block.first_sample += block.samples();
        }
        let rate = block.first_sample as f64 / start.elapsed().as_secs_f64();
        println!(
            "{}: {:.2} MS/s, {:.2}x 2.56 MS/s input",
            mode.name(),
            rate / 1e6,
            rate / MAX_SAMPLE_RATE as f64
        );
    }
}
