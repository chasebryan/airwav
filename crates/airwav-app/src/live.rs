//! Live V4 terminal session.
use crate::runtime::{Control, Runtime};
use crate::support::{Screen, audio_message, audio_settings, start_runtime, sync_audio, truecolor};
use super::capture_export::save_view;
use airwav_core::{Config, now_ns};
use airwav_ui::{Action, Ui};
use anyhow::{Result, ensure};
use crossterm::event;
use std::{
    fs,
    io::{IsTerminal, stdout},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub(crate) fn live(config: Config, data: &Path, quit: &AtomicBool) -> Result<()> {
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
        sync_audio(&mut ui, &runtime.audio);
        ui.set_recording(runtime.recording.load(Ordering::Acquire));
        ui.capture_active = runtime.capturing.load(Ordering::Acquire);
        screen
            .terminal
            .draw(|frame| airwav_ui::draw(frame, &mut ui))?;
        if event::poll(Duration::from_millis(65))? {
            match ui.handle(event::read()?) {
                Action::Quit => break,
                Action::Pause => ui.paused = !ui.paused,
                Action::AudioToggle => match audio_settings(&mut ui, false) {
                    Ok(settings) => {
                        if runtime
                            .control
                            .try_send(Control::AudioToggle(settings))
                            .is_err()
                        {
                            audio_message(&runtime.audio, "Audio control queue busy; retry");
                        }
                    }
                    Err(e) => audio_message(&runtime.audio, format!("Audio: {e:#}")),
                },
                Action::AudioMode => {
                    let next = (ui.audio_mode + 1) % 3;
                    let mode = [
                        airwav_dsp::audio::AudioMode::Am,
                        airwav_dsp::audio::AudioMode::Fm,
                        airwav_dsp::audio::AudioMode::Nfm,
                    ][next];
                    if runtime.control.try_send(Control::AudioMode(mode)).is_ok() {
                        ui.audio_mode = next;
                    } else {
                        audio_message(&runtime.audio, "Audio control queue busy; retry mode");
                    }
                }
                Action::AudioVolume(delta) => {
                    let next = (ui.audio_volume as i16 + delta as i16).clamp(0, 100) as u8;
                    if runtime.control.try_send(Control::AudioVolume(next)).is_ok() {
                        ui.audio_volume = next;
                    } else {
                        audio_message(&runtime.audio, "Audio control queue busy; retry volume");
                    }
                }
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
