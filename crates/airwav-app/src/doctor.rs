use airwav_core::Config;
use airwav_v4::V4Driver;
use anyhow::{Result, bail, ensure};
use std::{
    io::{IsTerminal, stdout},
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

pub fn doctor(
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
    let available = fs2::available_space(data)?;
    let storage_ok = available >= config.minimum_free_bytes;
    add(
        "storage",
        if storage_ok { "PASS" } else { "FAIL" },
        format!(
            "{} • {} MiB available (configured minimum {} MiB); recordings refuse to start below this threshold",
            data.display(),
            available / 1048576,
            config.minimum_free_bytes / 1048576
        ),
    );
    add(
        "audio",
        "INFO",
        "Offline AM/FM/NFM WAV export: airwav audio --help; live monitoring awaits hardware acceptance".into(),
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
        "Doctor found a blocking issue. Review FAIL rows above. Missing V4 library: see docs/linux.md. Low free space: free disk or lower minimum_free_bytes only if you accept the risk. Hardware FAIL with no library path: install the RTL-SDR Blog V4 driver or pass --library."
    );
    Ok(())
}
