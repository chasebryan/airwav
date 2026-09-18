use airwav_core::Config;
use airwav_record::Source;
use airwav_v4::V4Driver;
use anyhow::{Result, ensure};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use crate::runtime::Runtime;
use std::{
    io::{IsTerminal, stdout},
    path::{Path, PathBuf},
};

pub fn start_runtime(mut config: Config, data: &Path, recording: Option<PathBuf>) -> Result<Runtime> {
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
pub struct Screen {
    pub terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}
impl Screen {
    pub fn enter() -> Result<Self> {
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
pub fn truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit")
}
