//! Capture, screenshot export, and offline SVG export.
use crate::runtime::Control;
use crate::support::start_runtime;
use airwav_core::{Config, now_ns};
use airwav_record::Reader;
use airwav_ui::Ui;
use anyhow::{Context, Result, ensure};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(crate) fn capture(
    config: Config,
    data: &Path,
    path: PathBuf,
    seconds: u64,
    quit: &AtomicBool,
) -> Result<()> {
    ensure!(
        seconds > config.pre_trigger_seconds as u64 + config.post_trigger_seconds as u64,
        "Capture duration must include configured pre/post trigger durations plus one second"
    );
    let trigger = config.pre_trigger_seconds as u64;
    let mut runtime = start_runtime(config, data, Some(path.clone()))?;
    println!(
        "Capturing real V4 observations to {} for {seconds} seconds",
        path.display()
    );
    let start = Instant::now();
    let mut sent = false;
    while start.elapsed() < Duration::from_secs(seconds) && !quit.load(Ordering::Acquire) {
        if !sent
            && start.elapsed() >= Duration::from_secs(trigger)
            && runtime
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
                .latest
                .is_some()
        {
            runtime.control.send(Control::Capture)?;
            sent = true;
        }
        if runtime
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
            .finished
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    runtime.stop()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&Reader::open(&path)?.summary()?)?
    );
    Ok(())
}
pub(crate) fn save_view(ui: &mut Ui, path: &Path, width: u16, height: u16) -> Result<()> {
    ensure!(
        (70..=300).contains(&width) && (22..=150).contains(&height),
        "Screenshot dimensions must be 70..300 columns and 22..150 rows"
    );
    ensure!(
        path.extension().is_some_and(|e| e == "svg"),
        "This milestone exports SVG; output must end in .svg"
    );
    let metadata = path.with_extension("json");
    ensure!(
        !path.exists() && !metadata.exists(),
        "Screenshot or metadata already exists; choose a new output path"
    );
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|f| airwav_ui::draw(f, ui))?;
    let svg = airwav_ui::to_svg(terminal.backend().buffer());
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(svg.as_bytes())?;
    file.sync_all()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(metadata)?;
    file.write_all(&serde_json::to_vec_pretty(&serde_json::json!({"timestamp_ns":now_ns(),"source":ui.source,"selected_signal":ui.selected,"theme":ui.theme.name,"demo":ui.demo,"width":width,"height":height,"observation":ui.snapshot,"audio":{"active":ui.audio_active,"mode":(["AM","FM","NFM"][ui.audio_mode%3]),"volume":ui.audio_volume,"frequency_hz":ui.audio_frequency,"status":ui.audio_status,"flow":ui.audio_flow,"pcm_samples":ui.audio_pcm_samples,"rms_dbfs":ui.audio_rms_dbfs,"peak_dbfs":ui.audio_peak_dbfs,"clipped_samples":ui.audio_clipped_samples,"dropped_samples":ui.audio_dropped_samples,"discontinuities":ui.audio_discontinuities}}))?)?;
    file.sync_all()?;
    Ok(())
}
pub(crate) fn export(
    config: &Config,
    recording: &Path,
    output: &Path,
    width: u16,
    height: u16,
) -> Result<()> {
    let reader = Reader::open(recording)?;
    let mut ui = Ui::new(&config.theme, true, reader.manifest.source.label(), true);
    let snapshot = reader
        .snapshots()?
        .next()
        .context("Recording contains no measured spectrum")??;
    ui.update(snapshot);
    ui.status = "Recorded measurements • UNKNOWN remains a valid observation".into();
    save_view(&mut ui, output, width, height)?;
    println!("Exported {} and companion JSON", output.display());
    Ok(())
}
