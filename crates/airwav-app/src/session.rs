use airwav_core::{Config, now_ns};
use airwav_record::{Reader, Source};
use airwav_ui::{Action, Ui};
use anyhow::{Context, Result, ensure};
use crossterm::event;
use ratatui::{Terminal, backend::TestBackend};
use crate::runtime::{Control, Runtime};
use crate::support::{Screen, start_runtime, truecolor};
use std::{
    fs::{self, OpenOptions},
    io::{IsTerminal, Write, stdout},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub fn live(config: Config, data: &Path, quit: &AtomicBool) -> Result<()> {
    ensure!(
        stdout().is_terminal(),
        "An interactive terminal is required. Run airwav doctor for diagnostics."
    );
    let mut runtime = start_runtime(config.clone(), data, None)?;
    let mut screen = Screen::enter()?;
    let mut ui = Ui::new(&config.theme, truecolor(), "LIVE V4", false);
    while !quit.load(Ordering::Acquire) {
        {
            let state = runtime
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("UI state lock poisoned"))?;
            if !ui.paused
                && let Some(s) = &state.latest
            {
                ui.update(s.clone());
            }
            if !state.message.is_empty() {
                ui.status = state.message.clone();
            }
            ui.events = state.events.clone();
            if state.finished {
                ui.source = "V4 STREAM STOPPED".into();
            }
        }
        ui.set_recording(runtime.recording.load(Ordering::Acquire));
        ui.capture_active = runtime.capturing.load(Ordering::Acquire);
        screen
            .terminal
            .draw(|frame| airwav_ui::draw(frame, &mut ui))?;
        if event::poll(Duration::from_millis(65))? {
            match ui.handle(event::read()?) {
                Action::Quit => break,
                Action::Pause => ui.paused = !ui.paused,
                Action::Record => {
                    if runtime.control.try_send(Control::Record).is_err() {
                        ui.status = "Control queue busy".into();
                    }
                }
                Action::Capture => {
                    if runtime.control.try_send(Control::Capture).is_err() {
                        ui.status = "Control queue busy".into();
                    }
                }
                Action::Screenshot => {
                    let directory = data.join("screenshots");
                    fs::create_dir_all(&directory)?;
                    let path = directory.join(format!("airwav-{}.svg", now_ns()));
                    save_view(
                        &mut ui,
                        &path,
                        screen.terminal.size()?.width,
                        screen.terminal.size()?.height,
                    )?;
                    ui.status = format!("Saved {}", path.display());
                }
                _ => {}
            }
        }
    }
    drop(screen);
    runtime.stop()
}
pub fn capture(
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
pub fn save_view(ui: &mut Ui, path: &Path, width: u16, height: u16) -> Result<()> {
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
    file.write_all(&serde_json::to_vec_pretty(&serde_json::json!({"timestamp_ns":now_ns(),"source":ui.source,"selected_signal":ui.selected,"theme":ui.theme.name,"demo":ui.demo,"width":width,"height":height,"observation":ui.snapshot}))?)?;
    file.sync_all()?;
    Ok(())
}
pub fn export(config: &Config, recording: &Path, output: &Path, width: u16, height: u16) -> Result<()> {
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
pub fn replay(
    config: &Config,
    path: &Path,
    headless: bool,
    demo: bool,
    quit: &AtomicBool,
    data: &Path,
) -> Result<()> {
    let reader = Reader::open(path)?;
    if headless {
        println!("{}", serde_json::to_string_pretty(&reader.summary()?)?);
        return Ok(());
    }
    let mut events = Vec::new();
    for event in reader.events()?.take(4096) {
        let event = event?;
        reader.verify_event(&event)?;
        events.push((event.id, event.trigger_sample, event.complete));
    }
    let mut stream = reader.snapshots()?;
    let mut pending = stream.next().transpose()?;
    let mut ui = Ui::new(
        &config.theme,
        truecolor(),
        reader.manifest.source.label(),
        true,
    );
    ui.demo = demo;
    ui.events = events
        .iter()
        .map(|e| {
            format!(
                "{} • sample {}{}",
                e.0,
                e.1,
                if e.2 { "" } else { " • PARTIAL" }
            )
        })
        .collect();
    ui.status = if !reader.manifest.complete {
        "INCOMPLETE SESSION • intact journal rows only; use airwav recover"
    } else if matches!(reader.manifest.source, Source::DemoFixture { .. }) {
        "Replay • synthetic IQ fixture measurements"
    } else {
        "Replay • recorded V4 measurements"
    }
    .into();
    let mut screen = Screen::enter()?;
    let mut due = Instant::now();
    let mut current_ns = 0;
    while !quit.load(Ordering::Acquire) {
        if !ui.paused && Instant::now() >= due {
            if let Some(snapshot) = pending.take() {
                current_ns = snapshot.timestamp_ns;
                ui.update(snapshot);
                pending = stream.next().transpose()?;
                due += delay(
                    pending.as_ref().map(|s| s.timestamp_ns),
                    current_ns,
                    ui.speed,
                );
            } else {
                ui.paused = true;
                ui.status = "End of recording • [ / ] jump to an event • Q quit".into();
            }
        }
        screen.terminal.draw(|f| airwav_ui::draw(f, &mut ui))?;
        let poll_for = if ui.paused {
            Duration::from_millis(30)
        } else {
            due.saturating_duration_since(Instant::now())
                .min(Duration::from_millis(30))
        };
        if event::poll(poll_for)? {
            match ui.handle(event::read()?) {
                Action::Quit => break,
                Action::Pause => {
                    ui.paused = !ui.paused;
                    due = Instant::now();
                }
                Action::Step => {
                    ui.paused = true;
                    if let Some(s) = pending.take() {
                        current_ns = s.timestamp_ns;
                        ui.update(s);
                        pending = stream.next().transpose()?;
                    }
                }
                Action::Speed(speed) => {
                    ui.speed = speed;
                    due = Instant::now()
                        + delay(pending.as_ref().map(|s| s.timestamp_ns), current_ns, speed);
                }
                action @ (Action::PreviousEvent | Action::NextEvent) => {
                    // Determine direction from current sample, so both buttons work after playback.
                    let sample = ui.snapshot.as_ref().map_or(0, |s| s.spectrum.first_sample);
                    if !events.is_empty() {
                        let event_cursor = if action == Action::PreviousEvent {
                            events
                                .iter()
                                .rposition(|e| e.1 < sample)
                                .unwrap_or(events.len() - 1)
                        } else {
                            events.iter().position(|e| e.1 > sample).unwrap_or(0)
                        };
                        let target = events[event_cursor].1;
                        stream = reader.snapshots()?;
                        ui.history.clear();
                        pending = None;
                        for row in stream.by_ref() {
                            let s = row?;
                            if s.spectrum.first_sample >= target {
                                pending = Some(s);
                                break;
                            }
                        }
                        due = Instant::now();
                        ui.paused = false;
                    }
                }
                Action::Screenshot => {
                    let directory = data.join("screenshots");
                    fs::create_dir_all(&directory)?;
                    let output = directory.join(format!("airwav-{}.svg", now_ns()));
                    let size = screen.terminal.size()?;
                    save_view(&mut ui, &output, size.width, size.height)?;
                    ui.status = format!("Saved {}", output.display());
                }
                _ => {}
            }
        }
    }
    Ok(())
}
pub fn delay(next: Option<u64>, previous: u64, speed: f64) -> Duration {
    Duration::from_secs_f64(
        next.map_or(0., |v| v.saturating_sub(previous) as f64 / 1e9 / speed)
            .min(86400.),
    )
}
