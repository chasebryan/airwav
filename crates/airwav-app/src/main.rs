mod audio;
mod runtime;
mod doctor;
mod support;
mod session;
use airwav_core::Config;
use airwav_record::Reader;
use airwav_v4::V4Driver;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
    /// Demodulate a recorded IQ event to a 48 kHz mono WAV (AM, FM or NFM).
    Audio(audio::AudioArgs),
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
        }) => doctor::doctor(&config, &data, json, stream_seconds, counter_test),
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
        Some(Commands::Audio(args)) => audio::export(&args, &quit),
        Some(Commands::Export {
            recording,
            output,
            width,
            height,
        }) => session::export(&config, &recording, &output, width, height),
        Some(Commands::Recover { recording, output }) => {
            let m = airwav_record::recover(&recording, &output)?;
            println!("Recovered {} to {}", m.session_id, output.display());
            Ok(())
        }
        Some(Commands::Replay {
            recording,
            headless,
        }) => session::replay(&config, &recording, headless, false, &quit, &data),
        Some(Commands::Demo { recording }) => {
            session::replay(&config, &recording, false, true, &quit, &data)
        }
        Some(Commands::Capture { output, seconds }) => {
            session::capture(config, &data, output, seconds, &quit)
        }
        None => session::live(config, &data, &quit),
        Some(Commands::Config { .. }) => unreachable!("handled config before logging"),
    }
}
