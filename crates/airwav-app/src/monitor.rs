//! Terminal audio output. A stalled player never blocks the receiver/DSP worker.
use airwav_core::{IqBlock, ReceiverConfig};
use airwav_dsp::audio::{AudioConfig, AudioDemodulator, AudioMode};
use airwav_record::{Event, Reader};
use anyhow::{Context, Result, bail, ensure};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Copy)]
pub struct Settings {
    pub frequency_hz: u32,
    pub mode: AudioMode,
    pub volume: u8,
}
impl Settings {
    fn config(self, receiver: &ReceiverConfig) -> AudioConfig {
        AudioConfig {
            input_rate: receiver.sample_rate,
            offset_hz: self.frequency_hz as f64 - receiver.center_hz as f64,
            mode: self.mode,
            gain: self.volume as f32 / 100.,
            squelch_dbfs: None,
            deemphasis_us: if self.mode == AudioMode::Fm { 75 } else { 0 },
        }
    }
}
#[derive(Default, Clone)]
pub struct Status {
    pub active: bool,
    pub message: String,
    pub dropped_samples: u64,
    pub discontinuities: u64,
    pub pcm_samples: u64,
}
fn update(status: &Mutex<Status>, f: impl FnOnce(&mut Status)) {
    if let Ok(mut status) = status.lock() {
        f(&mut status);
    }
}
fn stop_player(child: &Mutex<Child>) {
    if let Ok(mut child) = child.lock() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
fn player() -> Result<(Child, &'static str)> {
    let choices: &[(&str, &[&str])] = &[
        (
            "pw-cat",
            &[
                "--playback",
                "--format=s16",
                "--rate=48000",
                "--channels=1",
                "--latency=100ms",
                "-",
            ],
        ),
        (
            "paplay",
            &[
                "--raw",
                "--format=s16le",
                "--rate=48000",
                "--channels=1",
                "--latency-msec=100",
                "--stream-name=AIRWAV",
            ],
        ),
        (
            "aplay",
            &[
                "-q",
                "-t",
                "raw",
                "-f",
                "S16_LE",
                "-r",
                "48000",
                "-c",
                "1",
                "--buffer-time=100000",
            ],
        ),
        (
            "ffplay",
            &[
                "-nodisp",
                "-autoexit",
                "-loglevel",
                "error",
                "-f",
                "s16le",
                "-ar",
                "48000",
                "-ac",
                "1",
                "-i",
                "pipe:0",
            ],
        ),
    ];
    for &(program, args) in choices {
        match Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => return Ok((child, program)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("Start audio player {program}")),
        }
    }
    bail!(
        "No audio player found; install pipewire-utils (pw-cat), pulseaudio-utils (paplay), alsa-utils (aplay), or ffplay"
    )
}

enum Input {
    Live(Receiver<Arc<IqBlock>>),
    Recorded {
        file: File,
        first_sample: u64,
        remaining: u64,
    },
}
pub struct Monitor {
    sender: Option<Sender<Arc<IqBlock>>>,
    child: Arc<Mutex<Child>>,
    stop: Arc<AtomicBool>,
    volume: Arc<AtomicU8>,
    worker: Option<JoinHandle<()>>,
    status: Arc<Mutex<Status>>,
}
impl Monitor {
    pub fn live(
        settings: Settings,
        receiver: &ReceiverConfig,
        status: Arc<Mutex<Status>>,
    ) -> Result<Self> {
        let (sender, input) = bounded(8);
        Self::start(
            settings,
            receiver,
            status,
            Input::Live(input),
            Some(sender),
            player,
        )
    }
    pub fn recorded(
        settings: Settings,
        reader: &Reader,
        event: &Event,
        status: Arc<Mutex<Status>>,
    ) -> Result<Self> {
        ensure!(
            event.iq.bytes > 0,
            "This event has no captured IQ to listen to"
        );
        let file = reader.open_event_iq(event)?;
        let first_sample = event
            .chunks
            .first()
            .context("Missing event IQ index")?
            .first_sample;
        Self::start(
            settings,
            &event.evidence.receiver,
            status,
            Input::Recorded {
                file,
                first_sample,
                remaining: event.iq.bytes,
            },
            None,
            player,
        )
    }
    fn start(
        settings: Settings,
        receiver: &ReceiverConfig,
        status: Arc<Mutex<Status>>,
        mut input: Input,
        sender: Option<Sender<Arc<IqBlock>>>,
        spawn_player: impl FnOnce() -> Result<(Child, &'static str)>,
    ) -> Result<Self> {
        let config = settings.config(receiver);
        let mut dsp = AudioDemodulator::new(config.clone())?;
        let (mut child, program) = spawn_player()?;
        let mut stdin = child.stdin.take().expect("piped audio stdin");
        let mut stderr = child.stderr.take().expect("piped audio stderr");
        let child = Arc::new(Mutex::new(child));
        let error_tail = Arc::new(Mutex::new(VecDeque::with_capacity(4096)));
        let error_out = error_tail.clone();
        let logger = match thread::Builder::new()
            .name("airwav-audio-errors".into())
            .spawn(move || {
                let mut bytes = [0; 1024];
                while let Ok(n) = stderr.read(&mut bytes) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut tail) = error_out.lock() {
                        tail.extend(&bytes[..n]);
                        let excess = tail.len().saturating_sub(4096);
                        tail.drain(..excess);
                    }
                }
            }) {
            Ok(logger) => logger,
            Err(e) => {
                stop_player(&child);
                return Err(e).context("Start audio error reader");
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let volume = Arc::new(AtomicU8::new(settings.volume.min(100)));
        let worker_volume = volume.clone();
        let worker_status = status.clone();
        let worker_child = child.clone();
        update(&status, |s| {
            *s = Status {
                active: true,
                message: format!(
                    "{} {:.6} MHz via {program}",
                    settings.mode.name().to_uppercase(),
                    settings.frequency_hz as f64 / 1e6
                ),
                ..Status::default()
            }
        });
        let worker = thread::Builder::new()
            .name("airwav-audio".into())
            .spawn(move || {
                let result: Result<()> = (|| {
                    let mut expected = None;
                    let mut pcm = Vec::with_capacity(4096);
                    let mut bytes = Vec::with_capacity(8192);
                    while !worker_stop.load(Ordering::Acquire) {
                        if let Some(exit) = worker_child
                            .lock()
                            .map_err(|_| anyhow::anyhow!("audio player lock poisoned"))?
                            .try_wait()?
                        {
                            bail!("{program} exited {exit}");
                        }
                        let block = match &mut input {
                            Input::Live(input) => {
                                match input.recv_timeout(Duration::from_millis(30)) {
                                    Ok(block) => block,
                                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                                }
                            }
                            Input::Recorded {
                                file,
                                first_sample,
                                remaining,
                            } => {
                                if *remaining == 0 {
                                    break;
                                }
                                let mut data = vec![0; (*remaining).min(65_536) as usize];
                                file.read_exact(&mut data)?;
                                let block = Arc::new(IqBlock {
                                    first_sample: *first_sample,
                                    received_ns: 0,
                                    bytes: data,
                                });
                                *first_sample = first_sample
                                    .checked_add(block.samples())
                                    .context("Audio sample position overflow")?;
                                *remaining -= block.bytes.len() as u64;
                                block
                            }
                        };
                        if expected.is_some_and(|next| next != block.first_sample) {
                            dsp = AudioDemodulator::new(config.clone())?;
                            update(&worker_status, |s| s.discontinuities += 1);
                        }
                        expected = block.first_sample.checked_add(block.samples());
                        dsp.set_gain(worker_volume.load(Ordering::Acquire) as f32 / 100.)?;
                        pcm.clear();
                        dsp.push(&block, &mut pcm)?;
                        bytes.clear();
                        for sample in &pcm {
                            bytes.extend_from_slice(&sample.to_le_bytes());
                        }
                        stdin.write_all(&bytes).context("Audio output failed")?;
                        update(&worker_status, |s| s.pcm_samples += pcm.len() as u64);
                    }
                    drop(stdin);
                    // Let the player drain recorded audio. Stop/mute can always kill it.
                    while !worker_stop.load(Ordering::Acquire) {
                        if let Some(exit) = worker_child
                            .lock()
                            .map_err(|_| anyhow::anyhow!("audio player lock poisoned"))?
                            .try_wait()?
                        {
                            ensure!(exit.success(), "{program} exited {exit}");
                            break;
                        }
                        thread::sleep(Duration::from_millis(20));
                    }
                    Ok(())
                })();
                stop_player(&worker_child);
                let _ = logger.join();
                if !worker_stop.load(Ordering::Acquire) {
                    let tail = error_tail
                        .lock()
                        .map(|s| {
                            String::from_utf8_lossy(&s.iter().copied().collect::<Vec<u8>>())
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(240)
                                .collect::<String>()
                        })
                        .unwrap_or_default();
                    update(&worker_status, |s| {
                        s.active = false;
                        s.message = match result {
                            Ok(()) => "Audio finished • A to listen again".into(),
                            Err(e) => {
                                format!("Audio failed: {e:#} {tail} • check system output; A retry")
                            }
                        };
                    });
                }
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(e) => {
                stop_player(&child);
                update(&status, |s| s.active = false);
                return Err(e).context("Start audio worker");
            }
        };
        Ok(Self {
            sender,
            child,
            stop,
            volume,
            worker: Some(worker),
            status,
        })
    }
    pub fn push(&self, block: Arc<IqBlock>) {
        if let Some(sender) = &self.sender {
            let samples = block.samples();
            if matches!(
                sender.try_send(block),
                Err(crossbeam_channel::TrySendError::Full(_))
            ) {
                update(&self.status, |s| s.dropped_samples += samples);
            }
        }
    }
    pub fn set_volume(&self, volume: u8) {
        self.volume.store(volume.min(100), Ordering::Release);
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Kill before joining: write_all may be blocked by a hung audio server.
        stop_player(&self.child);
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        update(&self.status, |s| {
            s.active = false;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn settings() -> Settings {
        Settings {
            frequency_hz: 136_000_000,
            mode: AudioMode::Am,
            volume: 50,
        }
    }
    fn child(script: &str, args: &[&std::ffi::OsStr]) -> Result<(Child, &'static str)> {
        let child = Command::new("python3")
            .args(["-u", "-c", script])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        Ok((child, "test-only-player"))
    }
    fn await_condition(mut condition: impl FnMut() -> bool) {
        let until = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(
                Instant::now() < until,
                "audio worker did not reach expected state"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn live_audio_outputs_pcm_changes_volume_and_resets_on_gaps() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("pcm.raw");
        let (sender, receiver) = bounded(8);
        let status = Arc::new(Mutex::new(Status::default()));
        let monitor = Monitor::start(settings(), &ReceiverConfig::default(), status.clone(), Input::Live(receiver), Some(sender), || {
            child("import os,sys\nf=open(sys.argv[1],'wb',buffering=0)\nwhile True:\n b=os.read(0,4096)\n if not b: break\n f.write(b)", &[output.as_os_str()])
        }).unwrap();
        let make = |first| {
            Arc::new(IqBlock {
                first_sample: first,
                received_ns: 0,
                bytes: (0..32768)
                    .flat_map(|i| {
                        let phase = std::f64::consts::TAU * 1000. * (first + i) as f64 / 2_560_000.;
                        [
                            (127.5 + 128. * 0.4 * (1. + 0.6 * phase.cos())).round() as u8,
                            128,
                        ]
                    })
                    .collect(),
            })
        };
        monitor.push(make(0));
        await_condition(|| status.lock().unwrap().pcm_samples > 0);
        monitor.set_volume(0);
        monitor.push(make(100_000));
        await_condition(|| status.lock().unwrap().discontinuities == 1);
        await_condition(|| status.lock().unwrap().pcm_samples > 1000);
        // The test player writes unbuffered PCM, independently of the audio state counter.
        await_condition(|| std::fs::metadata(&output).is_ok_and(|m| m.len() >= 2400));
        drop(monitor);
        let bytes = std::fs::read(output).unwrap();
        assert!(bytes[..1000].iter().any(|b| *b != 0));
        assert!(bytes[bytes.len() - 1000..].iter().all(|b| *b == 0));
        assert!(!status.lock().unwrap().active);
    }
    #[test]
    fn stalled_player_cannot_block_ingest_or_shutdown() {
        let (sender, receiver) = bounded(8);
        let status = Arc::new(Mutex::new(Status::default()));
        let monitor = Monitor::start(
            settings(),
            &ReceiverConfig::default(),
            status.clone(),
            Input::Live(receiver),
            Some(sender),
            || child("import time\ntime.sleep(60)", &[]),
        )
        .unwrap();
        let start = Instant::now();
        for n in 0..512 {
            monitor.push(Arc::new(IqBlock {
                first_sample: n * 32768,
                received_ns: 0,
                bytes: vec![180; 65536],
            }));
        }
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(status.lock().unwrap().dropped_samples > 0);
        // Allow the writer to fill the pipe, then verify mute/quit kills it before joining.
        thread::sleep(Duration::from_millis(150));
        let start = Instant::now();
        drop(monitor);
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(!status.lock().unwrap().active);
    }
    #[test]
    fn player_failure_is_visible_without_stopping_the_receiver() {
        let (sender, receiver) = bounded(8);
        let status = Arc::new(Mutex::new(Status::default()));
        let monitor = Monitor::start(
            settings(),
            &ReceiverConfig::default(),
            status.clone(),
            Input::Live(receiver),
            Some(sender),
            || {
                child(
                    "import sys\nsys.stderr.write('No default audio device\\n')\nsys.exit(3)",
                    &[],
                )
            },
        )
        .unwrap();
        await_condition(|| !status.lock().unwrap().active);
        assert!(
            status
                .lock()
                .unwrap()
                .message
                .contains("No default audio device")
        );
        // Further IQ delivery is still nonblocking after the audio worker exits.
        monitor.push(Arc::new(IqBlock {
            first_sample: 0,
            received_ns: 0,
            bytes: vec![128; 1024],
        }));
        drop(monitor);
    }
    #[test]
    fn recorded_audio_reaches_eof_and_player_is_reaped() {
        let mut input = tempfile::tempfile().unwrap();
        use std::io::{Seek, SeekFrom};
        input.write_all(&vec![180; 65536]).unwrap();
        input.seek(SeekFrom::Start(0)).unwrap();
        let status = Arc::new(Mutex::new(Status::default()));
        let monitor = Monitor::start(
            settings(),
            &ReceiverConfig::default(),
            status.clone(),
            Input::Recorded {
                file: input,
                first_sample: 123,
                remaining: 65536,
            },
            None,
            || child("import sys\nsys.stdin.buffer.read()", &[]),
        )
        .unwrap();
        await_condition(|| !status.lock().unwrap().active);
        assert!(status.lock().unwrap().pcm_samples > 0);
        assert!(status.lock().unwrap().message.contains("Audio finished"));
        drop(monitor);
    }
}
