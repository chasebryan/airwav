//! Synthetic AM/FM test tones; never used as a receiver fallback.
use airwav_core::{Config, IqBlock, Metrics, Snapshot};
use airwav_dsp::SpectrumEngine;
use airwav_record::{Source, Writer};
use std::{f64::consts::TAU, path::PathBuf, sync::Arc};

fn main() -> anyhow::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "audio-fixture.awr".into());
    let config = Config {
        post_trigger_seconds: 0,
        minimum_free_bytes: 0,
        ..Config::default()
    };
    let rate = config.receiver.sample_rate;
    let bytes = (0..rate * 2)
        .flat_map(|i| {
            let t = i as f64 / rate as f64;
            let am = 0.25 * (1. + 0.6 * (TAU * 440. * t).cos());
            let am_phase = TAU * -200_000. * t;
            let fm_phase = TAU * 250_000. * t + 40_000. / 660. * (TAU * 660. * t).sin();
            let re = am * am_phase.cos() + 0.35 * fm_phase.cos();
            let im = am * am_phase.sin() + 0.35 * fm_phase.sin();
            [
                (127.5 + 128. * re).round().clamp(0., 255.) as u8,
                (127.5 + 128. * im).round().clamp(0., 255.) as u8,
            ]
        })
        .collect();
    let block = Arc::new(IqBlock {
        first_sample: 0,
        received_ns: 1_789_718_400_000_000_000,
        bytes,
    });
    let spectrum = SpectrumEngine::new(2048)?
        .push(&block, &config.receiver)?
        .expect("fixture has full frames");
    let snapshot = Snapshot {
        timestamp_ns: block.received_ns,
        receiver: config.receiver.clone(),
        spectrum,
        islands: vec![],
        metrics: Metrics::default(),
        frames: vec![],
    };
    let mut writer = Writer::create(&path, &config, Source::DemoFixture {
        description: "DEMO FIXTURE: synthetic AM 440 Hz at 135800000 Hz and mono FM 660 Hz at 136250000 Hz. No received RF or speech.".into()
    })?;
    writer.append_snapshot(&snapshot)?;
    writer.begin_event(snapshot, &[block], rate as u64 * 2)?;
    writer.finish()?;
    println!("DEMO FIXTURE written to {}", path.display());
    Ok(())
}
