use airwav_core::IqBlock;
use airwav_dsp::audio::{AUDIO_RATE, AudioConfig, AudioDemodulator, AudioMode};
use airwav_record::{Event, Reader};
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, ValueEnum};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Mode {
    /// AM envelope detection, 3.5 kHz voice audio.
    Am,
    /// Mono broadcast FM, 75 kHz deviation and 15 kHz audio.
    Fm,
    /// Narrow FM, 2.5 kHz deviation and 3.5 kHz voice audio.
    Nfm,
}
impl From<Mode> for AudioMode {
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::Am => Self::Am,
            Mode::Fm => Self::Fm,
            Mode::Nfm => Self::Nfm,
        }
    }
}
#[derive(Args)]
pub struct AudioArgs {
    pub recording: PathBuf,
    /// Required when a recording contains more than one event.
    #[arg(long)]
    pub event: Option<String>,
    #[arg(long, value_enum)]
    pub mode: Mode,
    /// Absolute channel frequency in Hz; defaults to the event's receiver center.
    #[arg(long)]
    pub frequency_hz: Option<u32>,
    /// New WAV outside the recording directory. Existing files are never overwritten.
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, default_value_t = 0.8)]
    pub gain: f32,
    /// Channel power threshold in dBFS (-120..0); omitted means no squelch.
    #[arg(long, allow_hyphen_values = true)]
    pub squelch_dbfs: Option<f32>,
    /// FM only: 0, 50 or 75 microseconds. Defaults to 75 for FM, 0 otherwise.
    #[arg(long)]
    pub deemphasis_us: Option<u32>,
    /// Play the completed WAV with pw-play, paplay, aplay or ffplay.
    #[arg(long)]
    pub play: bool,
}

fn select_event(reader: &Reader, id: Option<&str>) -> Result<Event> {
    let mut selected = None;
    for row in reader.events()? {
        let event = row?;
        if id.is_none_or(|id| event.id == id) {
            ensure!(
                selected.is_none(),
                "Multiple matching events; select a unique --event ID from airwav inspect"
            );
            selected = Some(event);
        }
    }
    selected.context("No matching IQ event; metadata-only recordings cannot produce audio")
}

pub fn export(args: &AudioArgs, quit: &AtomicBool) -> Result<()> {
    let reader = Reader::open(&args.recording)?;
    let event = select_event(&reader, args.event.as_deref())?;
    ensure!(event.iq.bytes > 0, "The selected event has no IQ samples");
    let mode: AudioMode = args.mode.into();
    let receiver = &event.evidence.receiver;
    let frequency_hz = args.frequency_hz.unwrap_or(receiver.center_hz);
    let deemphasis_us = args
        .deemphasis_us
        .unwrap_or(if mode == AudioMode::Fm { 75 } else { 0 });
    let mut dsp = AudioDemodulator::new(AudioConfig {
        input_rate: receiver.sample_rate,
        offset_hz: frequency_hz as f64 - receiver.center_hz as f64,
        mode,
        gain: args.gain,
        squelch_dbfs: args.squelch_dbfs,
        deemphasis_us,
    })?;
    ensure!(
        !args.output.try_exists()?,
        "Refusing to overwrite {}",
        args.output.display()
    );
    let parent = args
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(
        !parent
            .canonicalize()?
            .starts_with(reader.root.canonicalize()?),
        "Audio output must be outside the immutable recording directory"
    );
    // Verify before creating output; stream from the exact descriptor that was hashed.
    let mut input = reader.open_event_iq(&event)?;
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    output.write_all(&[0; 44])?;
    let mut remaining = event.iq.bytes;
    let mut block = IqBlock {
        first_sample: event
            .chunks
            .first()
            .context("Missing IQ chunk index")?
            .first_sample,
        received_ns: event.timestamp_ns,
        bytes: vec![0; 65_536],
    };
    let mut pcm = Vec::with_capacity(4096);
    let mut pcm_bytes = Vec::with_capacity(8192);
    let mut samples = 0u64;
    while remaining > 0 {
        ensure!(
            !quit.load(Ordering::Acquire),
            "Audio export cancelled; no WAV published"
        );
        block.bytes.resize(remaining.min(65_536) as usize, 0);
        input.read_exact(&mut block.bytes)?;
        pcm.clear();
        dsp.push(&block, &mut pcm)?;
        samples += pcm.len() as u64;
        ensure!(
            samples <= (u32::MAX as u64 - 65_536) / 2,
            "Audio exceeds the RIFF/WAV 4 GiB limit"
        );
        pcm_bytes.clear();
        for sample in &pcm {
            pcm_bytes.extend_from_slice(&sample.to_le_bytes());
        }
        output.write_all(&pcm_bytes)?;
        remaining -= block.bytes.len() as u64;
        block.first_sample = block
            .first_sample
            .checked_add(block.samples())
            .context("IQ position overflow")?;
    }
    ensure!(
        samples > 0,
        "IQ event is too short to produce an audio sample"
    );
    let metadata = serde_json::json!({
        "application": "AIRWAV", "audio_version": 1,
        "source": reader.manifest.source, "source_label": reader.manifest.source.label(),
        "session_id": reader.manifest.session_id, "event_id": event.id,
        "event_complete": event.complete, "iq_blake3": event.iq.blake3,
        "input_sample_rate": receiver.sample_rate, "first_sample": event.chunks[0].first_sample,
        "frequency_hz": frequency_hz, "mode": mode.name(), "sample_rate": AUDIO_RATE,
        "channels": 1, "sample_format": "s16le", "samples": samples,
        "gain": args.gain, "squelch_dbfs": args.squelch_dbfs, "deemphasis_us": deemphasis_us,
        "clipped_samples": dsp.clipped_samples,
        "limitations": "Derived audio; no protocol/identity inference. Filter startup delay retained; tail not padded."
    });
    finish_wav(output.as_file_mut(), samples as u32, &metadata)?;
    output.as_file().sync_all()?;
    ensure!(
        !quit.load(Ordering::Acquire),
        "Audio export cancelled; no WAV published"
    );
    output
        .persist_noclobber(&args.output)
        .with_context(|| format!("Publish {} without overwriting", args.output.display()))?;
    File::open(parent)?.sync_all()?;
    println!("{}", serde_json::to_string_pretty(&metadata)?);
    if args.play {
        play(&args.output, quit)?;
    }
    Ok(())
}

fn finish_wav(file: &mut File, samples: u32, metadata: &serde_json::Value) -> Result<()> {
    // Keep evidence provenance in the WAV itself, including the DEMO FIXTURE label.
    let mut comment = serde_json::to_vec(metadata)?;
    comment.push(0);
    ensure!(
        comment.len() < 60_000,
        "Audio provenance metadata is too large"
    );
    let padded = comment.len() + comment.len() % 2;
    file.write_all(b"LIST")?;
    file.write_all(&(12 + padded as u32).to_le_bytes())?;
    file.write_all(b"INFOICMT")?;
    file.write_all(&(comment.len() as u32).to_le_bytes())?;
    file.write_all(&comment)?;
    if !comment.len().is_multiple_of(2) {
        file.write_all(&[0])?;
    }
    let size = u32::try_from(
        file.stream_position()?
            .checked_sub(8)
            .context("Invalid WAV size")?,
    )?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(b"RIFF")?;
    file.write_all(&size.to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?; // PCM
    file.write_all(&1u16.to_le_bytes())?; // mono
    file.write_all(&AUDIO_RATE.to_le_bytes())?;
    file.write_all(&(AUDIO_RATE * 2).to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?; // block alignment
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&(samples * 2).to_le_bytes())?;
    Ok(())
}

fn play(path: &Path, quit: &AtomicBool) -> Result<()> {
    // Absolute paths cannot be interpreted as options by the player. No shell expansion.
    let path = path.canonicalize()?;
    for (program, flags) in [
        ("pw-play", &[][..]),
        ("paplay", &[][..]),
        ("aplay", &["-q"][..]),
        (
            "ffplay",
            &["-nodisp", "-autoexit", "-loglevel", "error"][..],
        ),
    ] {
        let mut child = match Command::new(program)
            .args(flags)
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("Start {program}; WAV was saved")),
        };
        loop {
            if quit.load(Ordering::Acquire) {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(());
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    ensure!(
                        status.success(),
                        "{program} playback failed; WAV remains at {}",
                        path.display()
                    );
                    return Ok(());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(30)),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e).context("Wait for audio player; WAV was saved");
                }
            }
        }
    }
    bail!(
        "WAV saved to {}; install pw-play, paplay, aplay or ffplay for --play",
        path.display()
    )
}
