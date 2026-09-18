use airwav_core::{IqBlock, ReceiverConfig};
use airwav_dsp::{Detector, IqRing, SpectrumEngine};
use std::{hint::black_box, sync::Arc, time::Instant};
fn main() {
    let c = ReceiverConfig::default();
    let mut engine = SpectrumEngine::new(2048).unwrap();
    let mut detector = Detector::new(12., c.sample_rate);
    let mut ring = IqRing::new(25_600_000);
    let bytes: Vec<_> = (0..65_536).map(|i| ((i * 73 + 19) % 256) as u8).collect();
    let start = Instant::now();
    let n = 400;
    for i in 0..n {
        let block = Arc::new(IqBlock {
            first_sample: i * 32768,
            received_ns: 0,
            bytes: bytes.clone(),
        });
        let spectrum = engine.push(&block, &c).unwrap().unwrap();
        black_box(detector.update(&spectrum));
        ring.push(block);
    }
    let secs = start.elapsed().as_secs_f64();
    println!(
        "FFT + detection + bounded ring: {:.2} MS/s ({:.2}x 2.56 MS/s), {:.1} us/block",
        n as f64 * 32768. / secs / 1e6,
        n as f64 * 32768. / secs / c.sample_rate as f64,
        secs * 1e6 / n as f64
    );
}
