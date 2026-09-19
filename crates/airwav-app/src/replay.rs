//! Offline replay session loop.
use super::capture_export::save_view;
use crate::monitor;
use crate::support::{Screen, audio_message, audio_settings, sync_audio, truecolor};
use airwav_core::{Config, now_ns};
use airwav_record::{Reader, Source};
use airwav_ui::{Action, Ui};
use anyhow::{Context, Result};
use crossterm::event;
use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub(crate) fn replay(
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
    let audio_state = Arc::new(std::sync::Mutex::new(monitor::Status::default()));
    let mut audio_monitor: Option<monitor::Monitor> = None;
    let mut audio_event: Option<String> = None;
    while !quit.load(Ordering::Acquire) {
        sync_audio(&mut ui, &audio_state);
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
                action @ (Action::AudioToggle | Action::AudioMode) => {
                    let was_active = ui.audio_active;
                    if action == Action::AudioMode {
                        ui.audio_mode = (ui.audio_mode + 1) % 3;
                    }
                    if action == Action::AudioToggle && was_active {
                        audio_monitor.take();
                        audio_message(&audio_state, "Audio off • A Listen");
                        continue;
                    }
                    if action == Action::AudioMode && !was_active {
                        continue;
                    }
                    audio_monitor.take();
                    let started: Result<monitor::Monitor> = (|| {
                        if action == Action::AudioToggle || audio_event.is_none() {
                            let sample =
                                ui.snapshot.as_ref().map_or(0, |s| s.spectrum.first_sample);
                            let chosen = events
                                .iter()
                                .rfind(|e| e.1 <= sample)
                                .or_else(|| events.first())
                                .context(
                                    "No captured IQ events; metadata-only replay has no audio",
                                )?;
                            audio_event = Some(chosen.0.clone());
                        }
                        let mut selected = None;
                        for row in reader.events()? {
                            let event = row?;
                            if Some(&event.id) == audio_event.as_ref() {
                                selected = Some(event);
                                break;
                            }
                        }
                        let event = selected.context("Recorded audio event was not found")?;
                        let settings = audio_settings(&mut ui, action == Action::AudioMode)?;
                        monitor::Monitor::recorded(settings, &reader, &event, audio_state.clone())
                    })();
                    match started {
                        Ok(monitor) => audio_monitor = Some(monitor),
                        Err(e) => audio_message(&audio_state, format!("Audio unavailable: {e:#}")),
                    }
                }
                Action::AudioVolume(delta) => {
                    ui.audio_volume = (ui.audio_volume as i16 + delta as i16).clamp(0, 100) as u8;
                    if let Some(monitor) = &audio_monitor {
                        monitor.set_volume(ui.audio_volume);
                    }
                }
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
fn delay(next: Option<u64>, previous: u64, speed: f64) -> Duration {
    Duration::from_secs_f64(
        next.map_or(0., |v| v.saturating_sub(previous) as f64 / 1e9 / speed)
            .min(86400.),
    )
}
