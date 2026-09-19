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
    time::{Duration, Instant},
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
    pub level: Option<PcmLevel>,
    pub clipped_samples: u64,
    last_input: Option<Instant>,
    pending_output: Option<Instant>,
    draining: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct PcmLevel {
    pub rms_dbfs: f32,
    pub peak_dbfs: f32,
}
impl PcmLevel {
    fn measure(pcm: &[i16]) -> Option<Self> {
        if pcm.is_empty() {
            return None;
        }
        let mut energy = 0.;
        let mut peak = 0f64;
        for sample in pcm {
            let amplitude = *sample as f64 / 32768.;
            energy += amplitude * amplitude;
            peak = peak.max(amplitude.abs());
        }
        // A finite floor makes digital silence safe to save as JSON.
        Some(Self {
            rms_dbfs: (10. * (energy / pcm.len() as f64).max(1e-12).log10()) as f32,
            peak_dbfs: (20. * peak.max(1e-6).log10()) as f32,
        })
    }
}
impl Status {
    pub fn flow(&self, now: Instant) -> String {
        if !self.active {
            return String::new();
        }
        if self
            .pending_output
            .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_secs(1))
        {
            return "Output stalled • check system audio".into();
        }
        if self.draining {
            return "Output draining".into();
        }
        if self
            .last_input
            .is_none_or(|at| now.saturating_duration_since(at) >= Duration::from_secs(1))
        {
            return "Waiting for IQ".into();
        }
        match self.level {
            None => "Preparing audio".into(),
            Some(level) if level.peak_dbfs <= -120. => "Silent PCM • check channel/volume".into(),
            Some(level) => format!("PCM {:.0} dBFS", level.rms_dbfs),
        }
    }
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
    let mut last_fail = String::new();
    for &(program, args) in PLAYERS {
        match spawn_player(program, args) {
            Ok(Some(child)) => return Ok((child, program)),
            Ok(None) => continue,
            Err(error) => last_fail = error.to_string(),
        }
    }
    if last_fail.is_empty() {
        bail!(
            "No audio player found; install pipewire-utils (pw-cat), pulseaudio-utils (paplay), alsa-utils (aplay), or ffplay"
        );
    }
    bail!("Audio player failed to start: {last_fail}")
}

const PLAYERS: &[(&str, &[&str])] = &[
    (
        "pw-cat",
        &[
            "--playback",
            "--raw",
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

fn spawn_player(program: &str, args: &[&str]) -> Result<Option<Child>> {
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("Start audio player {program}")),
    };
    // pw-cat without --raw dies immediately: libsndfile tries to open "-" as a WAV.
    thread::sleep(Duration::from_millis(40));
    match child.try_wait() {
        Ok(None) => Ok(Some(child)),
        Ok(Some(status)) => {
            let mut tail = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut tail);
            }
            let _ = child.wait();
            let detail = tail
                .chars()
                .filter(|c| !c.is_control())
                .take(160)
                .collect::<String>();
            bail!("{program} exited {status} {detail}");
        }
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(e).with_context(|| format!("Wait for audio player {program}"))
        }
    }
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
    settings: Settings,
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
                        update(&worker_status, |s| s.last_input = Some(Instant::now()));
                        if expected.is_some_and(|next| next != block.first_sample) {
                            dsp = AudioDemodulator::new(config.clone())?;
                            update(&worker_status, |s| s.discontinuities += 1);
                        }
                        expected = block.first_sample.checked_add(block.samples());
                        dsp.set_gain(worker_volume.load(Ordering::Acquire) as f32 / 100.)?;
                        pcm.clear();
                        let previously_clipped = dsp.clipped_samples;
                        dsp.push(&block, &mut pcm)?;
                        let level = PcmLevel::measure(&pcm);
                        bytes.clear();
                        for sample in &pcm {
                            bytes.extend_from_slice(&sample.to_le_bytes());
                        }
                        if !bytes.is_empty() {
                            update(&worker_status, |s| s.pending_output = Some(Instant::now()));
                            stdin.write_all(&bytes).context("Audio output failed")?;
                            update(&worker_status, |s| {
                                s.pending_output = None;
                                s.pcm_samples += pcm.len() as u64;
                                s.level = level;
                                s.clipped_samples += dsp.clipped_samples - previously_clipped;
                            });
                        }
                    }
                    drop(stdin);
                    update(&worker_status, |s| s.draining = true);
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
            settings,
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
    pub fn settings(&self) -> Settings {
        Settings {
            volume: self.volume.load(Ordering::Acquire),
            ..self.settings
        }
    }
    pub fn is_active(&self) -> bool {
        self.status.lock().is_ok_and(|status| status.active)
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

    #[test]
    fn pipewire_player_requests_raw_pcm_not_a_sound_file() {
        let args = PLAYERS
            .iter()
            .find(|(name, _)| *name == "pw-cat")
            .expect("pw-cat is the preferred PipeWire player")
            .1;
        assert!(
            args.contains(&"--raw"),
            "pw-cat must not hand stdin to libsndfile: {args:?}"
        );
        assert!(args.contains(&"--playback"), "{args:?}");
        assert!(args.contains(&"--format=s16"), "{args:?}");
        assert!(args.contains(&"-"), "{args:?}");
        assert!(
            PLAYERS
                .iter()
                .any(|(name, args)| *name == "paplay" && args.contains(&"--raw")),
            "paplay must also request raw PCM"
        );
    }

    #[test]
    fn pcm_meter_measures_level_and_handles_silence_without_nonfinite_values() {
        assert!(PcmLevel::measure(&[]).is_none());
        let half = PcmLevel::measure(&[16384, -16384, 16384, -16384]).unwrap();
        assert!((half.rms_dbfs + 6.0206).abs() < 0.001);
        assert!((half.peak_dbfs + 6.0206).abs() < 0.001);
        let silence = PcmLevel::measure(&[0; 100]).unwrap();
        assert_eq!(silence.rms_dbfs, -120.);
        assert_eq!(silence.peak_dbfs, -120.);
        let full = PcmLevel::measure(&[i16::MIN, 0]).unwrap();
        assert_eq!(full.peak_dbfs, 0.);
        assert!((full.rms_dbfs + 3.0103).abs() < 0.001);
    }
    #[test]
    fn flow_distinguishes_input_silence_output_stalls_and_drain() {
        let now = Instant::now();
        let mut status = Status {
            active: true,
            ..Status::default()
        };
        assert_eq!(status.flow(now), "Waiting for IQ");
        status.last_input = Some(now);
        assert_eq!(status.flow(now), "Preparing audio");
        status.level = PcmLevel::measure(&[0; 10]);
        assert!(status.flow(now).starts_with("Silent PCM"));
        status.level = PcmLevel::measure(&[16384; 10]);
        assert_eq!(status.flow(now), "PCM -6 dBFS");
        status.pending_output = Some(now);
        assert_eq!(status.flow(now + Duration::from_millis(999)), "PCM -6 dBFS");
        assert!(
            status
                .flow(now + Duration::from_secs(1))
                .starts_with("Output stalled")
        );
        status.pending_output = None;
        assert_eq!(status.flow(now + Duration::from_secs(1)), "Waiting for IQ");
        status.draining = true;
        assert_eq!(status.flow(now + Duration::from_secs(1)), "Output draining");
        status.active = false;
        assert!(status.flow(now).is_empty());
    }

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
        assert!(status.lock().unwrap().level.unwrap().rms_dbfs > -40.);
        monitor.set_volume(0);
        assert_eq!(monitor.settings().volume, 0);
        assert_eq!(monitor.settings().frequency_hz, settings().frequency_hz);
        assert_eq!(monitor.settings().mode, AudioMode::Am);
        monitor.push(make(100_000));
        await_condition(|| status.lock().unwrap().discontinuities == 1);
        await_condition(|| status.lock().unwrap().pcm_samples > 1000);
        assert_eq!(status.lock().unwrap().level.unwrap().peak_dbfs, -120.);
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
        let directory = tempfile::tempdir().unwrap();
        let ready = directory.path().join("player-ready");
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
                    "import fcntl,pathlib,sys,time\nfcntl.fcntl(0,fcntl.F_SETPIPE_SZ,4096)\npathlib.Path(sys.argv[1]).touch()\ntime.sleep(60)",
                    &[ready.as_os_str()],
                )
            },
        )
        .unwrap();
        await_condition(|| ready.exists());
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
        await_condition(|| {
            status
                .lock()
                .unwrap()
                .flow(Instant::now())
                .starts_with("Output stalled")
        });
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
