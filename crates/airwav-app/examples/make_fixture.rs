//! Explicitly synthetic DSP fixture. Not accessible from production capture mode.
use airwav_core::{Config, IqBlock, Metrics, Snapshot};
use airwav_dsp::{Detector, IqRing, SpectrumEngine};
use airwav_record::{Source, Writer};
use std::{path::PathBuf, sync::Arc};
fn main() -> anyhow::Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "demo-fixture.awr".into());
    let config = Config {
        pre_trigger_seconds: 1,
        post_trigger_seconds: 1,
        minimum_free_bytes: 0,
        ..Config::default()
    };
    let mut writer = Writer::create(
        &output,
        &config,
        Source::DemoFixture {
            description: "DEMO FIXTURE: deterministic synthetic IQ; three drifting/burst carriers plus seeded noise. No aircraft, protocols, or identities.".into(),
        },
    )?;
    let mut fft = SpectrumEngine::new(config.fft_size)?;
    let mut detector = Detector::new(config.detection_snr_db, config.receiver.sample_rate);
    let mut ring = IqRing::new(config.ring_bytes());
    let mut seed = 7u32;
    let count = 256u64;
    for block_number in 0..count {
        let first_sample = block_number * 32768;
        let mut bytes = Vec::with_capacity(65536);
        for i in 0..32768 {
            let position = first_sample + i;
            let time = position as f64 / config.receiver.sample_rate as f64;
            let mut re = 0.;
            let mut im = 0.;
            for (hz, amp, on) in [
                (-410_000. + time * 130., 0.20, true),
                (240_000., 0.08, !((time * 2.) as u64).is_multiple_of(3)),
                (610_000., 0.30, time > 1.3 && time < 2.4),
            ] {
                if on {
                    let phase = std::f64::consts::TAU * hz * time;
                    re += amp * phase.cos();
                    im += amp * phase.sin();
                }
            }
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            re += ((seed >> 16) as f64 / 65536. - 0.5) * 0.04;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            im += ((seed >> 16) as f64 / 65536. - 0.5) * 0.04;
            bytes.push((127.5 + re * 128.).round().clamp(0., 255.) as u8);
            bytes.push((127.5 + im * 128.).round().clamp(0., 255.) as u8);
        }
        let block = Arc::new(IqBlock {
            first_sample,
            received_ns: 1_789_718_400_000_000_000
                + first_sample * 1_000_000_000 / config.receiver.sample_rate as u64,
            bytes,
        });
        writer.append_iq(&block)?;
        ring.push(block.clone());
        let spectrum = fft.push(&block, &config.receiver)?.expect("full frames");
        let islands = detector.update(&spectrum);
        let snapshot = Snapshot {
            timestamp_ns: block.received_ns,
            receiver: config.receiver.clone(),
            spectrum,
            islands,
            metrics: Metrics {
                received_samples: first_sample + 32768,
                processed_samples: first_sample + 32768,
                ring_bytes: ring.bytes() as u64,
                ring_capacity_bytes: config.ring_bytes() as u64,
                ..Metrics::default()
            },
        };
        if block_number % 8 == 0 {
            writer.append_snapshot(&snapshot)?;
        }
        if block_number == 88 {
            writer.begin_event(snapshot, &ring.snapshot(), first_sample + 32768)?;
        }
    }
    writer.finish()?;
    println!("DEMO FIXTURE written to {}", output.display());
    Ok(())
}
