mod runtime;
use airwav_core::{Config, now_ns};
use airwav_record::{Reader, Source};
use airwav_ui::{Action, Ui};
use airwav_v4::V4Driver;
use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{CrosstermBackend, TestBackend},
};
use runtime::{Control, Runtime};
use std::{
    fs::{self, OpenOptions},
    io::{IsTerminal, Write, stdout},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    name = "airwav",
    version,
    about = "AIRWAV • V4-exclusive, passive RF observation terminal",
    long_about = "AIRWAV captures and measures RTL-SDR Blog V4 IQ, preserves evidence, and replays AWR recordings offline. This is the capture foundation milestone; protocol decoding and MAX-I scheduling are not yet enabled."
)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    library: Option<PathBuf>,
    #[arg(long, global = true, default_value = "info")]
    log_level: String,
    #[arg(long, global = true)]
    center_hz: Option<u32>,
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Subcommand)]
enum Commands {
    /// Check receiver, V4 driver behavior, streaming, terminal, and storage.
    Doctor {
        #[arg(long)]
        json: bool,
        #[arg(long,default_value_t=1,value_parser=clap::value_parser!(u64).range(0..=60))]
        stream_seconds: u64,
        #[arg(long)]
        counter_test: bool,
    },
    /// List USB identities; incompatible devices are explicitly rejected.
    Devices,
    /// Print effective TOML; --init writes it if the file does not exist.
    Config {
        #[arg(long)]
        init: bool,
    },
    /// Record metadata and a pre/post-trigger IQ event without a TUI.
    Capture {
        output: PathBuf,
        #[arg(long,default_value_t=20,value_parser=clap::value_parser!(u64).range(1..=86400))]
        seconds: u64,
    },
    /// Replay measurements without a receiver or installed driver.
    Replay {
        recording: PathBuf,
        #[arg(long)]
        headless: bool,
    },
    /// Verify AWR structure and IQ hashes, and report counts.
    Inspect { recording: PathBuf },
    /// Export the first measured frame as an SVG terminal screenshot + JSON.
    Export {
        recording: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 132)]
        width: u16,
        #[arg(long, default_value_t = 42)]
        height: u16,
    },
    /// Recover an incomplete recording into a new bundle; source is preserved.
    Recover {
        recording: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Use a recorded session with presentation-only Demo Mode enabled.
    Demo { recording: PathBuf },
}
fn data_dir() -> PathBuf {
    std::env::var_os("AIRWAV_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home().join(".local/share"))
                .join("airwav")
        })
}
fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
fn config_path(cli: &Cli) -> PathBuf {
    cli.config
        .clone()
        .or_else(|| std::env::var_os("AIRWAV_CONFIG").map(PathBuf::from))
        .unwrap_or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home().join(".config"))
                .join("airwav/config.toml")
        })
}
fn load_config(cli: &Cli) -> Result<Config> {
    let path = config_path(cli);
    let mut config = match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("Parse {}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
        Err(e) => return Err(e).context("read config"),
    };
    if let Some(path) = &cli.library {
        config.library = Some(path.clone());
    }
    if let Some(center) = cli.center_hz {
        config.receiver.center_hz = center;
    }
    config.validate()?;
    Ok(config)
}
fn main() {
    if let Err(error) = run() {
        eprintln!("AIRWAV: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config(&cli)?;
    if let Some(Commands::Config { init }) = &cli.command {
        let text = toml::to_string_pretty(&config)?;
        if *init {
            let path = config_path(&cli);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .with_context(|| format!("Refusing to overwrite {}", path.display()))?;
            file.write_all(text.as_bytes())?;
            println!("Created {}", path.display());
        } else {
            print!("{text}");
        }
        return Ok(());
    }
    let data = data_dir();
    fs::create_dir_all(&data)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(data.join("airwav.log"))?;
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_new(&cli.log_level)?)
        .with_writer(log)
        .with_ansi(false)
        .init();
    let quit = Arc::new(AtomicBool::new(false));
    let signal = quit.clone();
    ctrlc::set_handler(move || {
        signal.store(true, Ordering::Release);
    })?;
    match cli.command {
        Some(Commands::Doctor {
            json,
            stream_seconds,
            counter_test,
        }) => doctor(&config, &data, json, stream_seconds, counter_test),
        Some(Commands::Devices) => {
            let driver = V4Driver::load(config.library.as_deref())?;
            let devices = driver.devices()?;
            if devices.is_empty() {
                println!("No RTL USB devices accessible. AIRWAV requires an RTL-SDR Blog V4.");
            }
            for d in devices {
                println!(
                    "{}  {} / {} / {}  {}",
                    d.index,
                    d.manufacturer,
                    d.product,
                    d.serial,
                    if d.is_v4() {
                        "V4 candidate (run doctor)"
                    } else {
                        "REJECTED — AIRWAV requires an RTL-SDR Blog V4."
                    }
                );
            }
            Ok(())
        }
        Some(Commands::Inspect { recording }) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&Reader::open(&recording)?.summary()?)?
            );
            Ok(())
        }
        Some(Commands::Export {
            recording,
            output,
            width,
            height,
        }) => export(&config, &recording, &output, width, height),
        Some(Commands::Recover { recording, output }) => {
            let m = airwav_record::recover(&recording, &output)?;
            println!("Recovered {} to {}", m.session_id, output.display());
            Ok(())
        }
        Some(Commands::Replay {
            recording,
            headless,
        }) => replay(&config, &recording, headless, false, &quit, &data),
        Some(Commands::Demo { recording }) => {
            replay(&config, &recording, false, true, &quit, &data)
        }
        Some(Commands::Capture { output, seconds }) => {
            capture(config, &data, output, seconds, &quit)
        }
        None => live(config, &data, &quit),
        Some(Commands::Config { .. }) => unreachable!("handled config before logging"),
    }
}
fn doctor(
    config: &Config,
    data: &Path,
    json: bool,
    seconds: u64,
    counter_test: bool,
) -> Result<()> {
    let mut checks = vec![];
    let mut success = true;
    let mut add = |name: &str, status: &str, detail: String| {
        if status == "FAIL" {
            success = false;
        }
        checks.push(serde_json::json!({"check":name,"status":status,"detail":detail}));
    };
    add(
        "terminal",
        if stdout().is_terminal() {
            "PASS"
        } else {
            "INFO"
        },
        format!(
            "TTY {}; TERM {}; COLORTERM {}; mouse events require a supporting terminal",
            stdout().is_terminal(),
            std::env::var("TERM").unwrap_or_default(),
            std::env::var("COLORTERM").unwrap_or_default()
        ),
    );
    add(
        "storage",
        "PASS",
        format!(
            "{} • {} MiB available",
            data.display(),
            fs2::available_space(data)? / 1048576
        ),
    );
    add(
        "audio",
        "INFO",
        "Audio monitoring is a later milestone".into(),
    );
    let ffmpeg = std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    add(
        "FFmpeg",
        "INFO",
        if ffmpeg {
            "installed; video export is a later milestone"
        } else {
            "not installed; optional: sudo dnf install ffmpeg-free"
        }
        .into(),
    );
    let hardware: Result<()> = (|| {
        let driver = V4Driver::load(config.library.as_deref())?;
        add(
            "library",
            "PASS",
            format!(
                "Loaded {} (behavioral V4 validation follows)",
                driver.library
            ),
        );
        let devices = driver.devices()?;
        add(
            "USB identity",
            "INFO",
            format!(
                "{} accessible RTL USB device(s), {} exact V4 identities",
                devices.len(),
                devices.iter().filter(|d| d.is_v4()).count()
            ),
        );
        let receiver = driver.open(&config.receiver)?;
        add(
            "V4 validation",
            "PASS",
            format!(
                "{} • R828D • initialized RTL/tuner clocks 28.8 MHz • configuration read back",
                receiver.identity.serial
            ),
        );
        if seconds > 0 {
            let mut stream = receiver.start(config.queue_blocks, counter_test)?;
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(seconds) {
                match stream.receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(_) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(_) => bail!("Receiver stream ended during doctor"),
                }
            }
            stream.stop()?;
            let stats = stream.stats.snapshot();
            let gaps = stream.stats.counter_discontinuities.load(Ordering::Relaxed);
            let expected = config.receiver.sample_rate as f64 * start.elapsed().as_secs_f64();
            let ratio = stats.received_samples as f64 / expected;
            let okay = stats.received_samples > 0
                && stats.queue_dropped_samples == 0
                && stats.malformed_bytes == 0
                && gaps == 0
                && ratio > 0.75;
            add(
                "stream",
                if okay { "PASS" } else { "FAIL" },
                format!(
                    "{} samples; {} application drops; {:.2} MS/s; USB loss {}",
                    stats.received_samples,
                    stats.queue_dropped_samples,
                    stats.received_samples as f64 / start.elapsed().as_secs_f64() / 1e6,
                    if counter_test {
                        format!(
                            "counter discontinuities {gaps}; missing bytes lower bound {} (modulo-256 limitation)",
                            stream
                                .stats
                                .counter_missing_bytes_lower_bound
                                .load(Ordering::Relaxed)
                        )
                    } else {
                        "unavailable in normal RF mode; use --counter-test".into()
                    }
                ),
            );
        } else {
            add(
                "stream",
                "INFO",
                "Not tested; run airwav doctor --stream-seconds 30 --counter-test".into(),
            );
        }
        Ok(())
    })();
    if let Err(error) = hardware {
        add("hardware", "FAIL", format!("{error:#}"));
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"application":"AIRWAV","hardware_target":"RTL-SDR Blog V4 only","ok":success,"checks":checks})
            )?
        );
    } else {
        println!("\n  A I R W A V  /  DOCTOR\n");
        for c in checks {
            println!(
                "  {:<18} {:<5}  {}",
                c["check"].as_str().unwrap_or(""),
                c["status"].as_str().unwrap_or(""),
                c["detail"].as_str().unwrap_or("")
            );
        }
    }
    ensure!(
        success,
        "Doctor found a blocking issue. See the checks above and docs/linux.md."
    );
    Ok(())
}
fn start_runtime(mut config: Config, data: &Path, recording: Option<PathBuf>) -> Result<Runtime> {
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
struct Screen {
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}
impl Screen {
    fn enter() -> Result<Self> {
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
fn truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit")
}
fn live(config: Config, data: &Path, quit: &AtomicBool) -> Result<()> {
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
        ui.recording = runtime.recording.load(Ordering::Acquire);
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
fn capture(
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
fn save_view(ui: &mut Ui, path: &Path, width: u16, height: u16) -> Result<()> {
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
fn export(config: &Config, recording: &Path, output: &Path, width: u16, height: u16) -> Result<()> {
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
fn replay(
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
fn delay(next: Option<u64>, previous: u64, speed: f64) -> Duration {
    Duration::from_secs_f64(
        next.map_or(0., |v| v.saturating_sub(previous) as f64 / 1e9 / speed)
            .min(86400.),
    )
}
