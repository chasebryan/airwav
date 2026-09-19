//! Terminal screen setup and audio UI helpers.
use crate::monitor;
use crate::runtime::Runtime;
use airwav_core::Config;
use airwav_record::Source;
use airwav_ui::Ui;
use airwav_v4::V4Driver;
use anyhow::{Context, Result, ensure};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io::{IsTerminal, stdout},
    path::{Path, PathBuf},
    time::Instant,
};

pub(crate) fn start_runtime(
    mut config: Config,
    data: &Path,
    recording: Option<PathBuf>,
) -> Result<Runtime> {
    let driver = V4Driver::load(config.library.as_deref())?;
    let receiver = driver.open(&config.receiver)?;
    config.receiver = receiver.config.clone();
    config.validate()?;
    let source = Source::LiveV4 {
        device: receiver.identity.clone(),
        library: driver.library.clone(),
    };
    let stream = receiver.start(config.queue_blocks, false)?;
    Runtime::start(stream, config, source, data.into(), recording)
}
pub(crate) struct Screen {
    pub(crate) terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}
impl Screen {
    pub(crate) fn enter() -> Result<Self> {
        ensure!(
            stdout().is_terminal() && std::io::stdin().is_terminal(),
            "AIRWAV needs an interactive terminal. Use airwav doctor, capture, inspect, or replay --headless in scripts."
        );
        enable_raw_mode()?;
        if let Err(error) = execute!(stdout(), EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        match Terminal::new(CrosstermBackend::new(stdout())) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
                let _ = disable_raw_mode();
                Err(error.into())
            }
        }
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
pub(crate) fn truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit")
}
pub(crate) fn audio_settings(ui: &mut Ui, keep_frequency: bool) -> Result<monitor::Settings> {
    let snapshot = ui
        .snapshot
        .as_ref()
        .context("Waiting for IQ before starting audio")?;
    let selected = snapshot
        .islands
        .get(ui.selected)
        .map(|s| s.center_hz.round() as u32)
        .unwrap_or(snapshot.receiver.center_hz);
    let frequency_hz = if keep_frequency {
        ui.audio_frequency.unwrap_or(selected)
    } else {
        selected
    };
    ui.audio_frequency = Some(frequency_hz);
    Ok(monitor::Settings {
        frequency_hz,
        mode: [
            airwav_dsp::audio::AudioMode::Am,
            airwav_dsp::audio::AudioMode::Fm,
            airwav_dsp::audio::AudioMode::Nfm,
        ][ui.audio_mode % 3],
        volume: ui.audio_volume,
    })
}
pub(crate) fn sync_audio(ui: &mut Ui, audio: &std::sync::Mutex<monitor::Status>) {
    if let Ok(audio) = audio.lock() {
        ui.audio_active = audio.active;
        if !audio.message.is_empty() {
            ui.audio_status = audio.message.clone();
        }
        ui.audio_dropped_samples = audio.dropped_samples;
        ui.audio_discontinuities = audio.discontinuities;
        ui.audio_flow = audio.flow(Instant::now());
        ui.audio_pcm_samples = audio.pcm_samples;
        ui.audio_rms_dbfs = audio.level.map(|level| level.rms_dbfs);
        ui.audio_peak_dbfs = audio.level.map(|level| level.peak_dbfs);
        ui.audio_clipped_samples = audio.clipped_samples;
    }
}
pub(crate) fn audio_message(audio: &std::sync::Mutex<monitor::Status>, message: impl Into<String>) {
    if let Ok(mut audio) = audio.lock() {
        audio.message = message.into();
    }
}
