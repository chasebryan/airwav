use airwav_core::{Config, IqBlock, Snapshot, now_ns};
use airwav_dsp::{Detector, IqRing, SpectrumEngine};
use airwav_record::{Source, Writer};
use airwav_v4::Stream;
use anyhow::{Context, Result};
use crossbeam_channel::{Sender, bounded};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct State {
    pub latest: Option<Snapshot>,
    pub message: String,
    pub events: Vec<String>,
    pub finished: bool,
}
pub enum Control {
    Record,
    Capture,
    Quit,
}
enum StoreCommand {
    Start(PathBuf),
    Snapshot(Box<Snapshot>),
    Capture(Box<Snapshot>, Vec<Arc<IqBlock>>, u64),
    Iq(Arc<IqBlock>),
    Stop,
}
pub struct Runtime {
    pub control: Sender<Control>,
    pub state: Arc<Mutex<State>>,
    pub recording: Arc<AtomicBool>,
    pub capturing: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
    stop: Arc<AtomicBool>,
}
fn message(state: &Mutex<State>, value: impl Into<String>) {
    if let Ok(mut state) = state.lock() {
        state.message = value.into();
    }
}
impl Runtime {
    pub fn start(
        mut stream: Stream,
        config: Config,
        source: Source,
        data: PathBuf,
        initial_recording: Option<PathBuf>,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = stop.clone();
        let (control, commands) = bounded(16);
        let state = Arc::new(Mutex::new(State::default()));
        let out = state.clone();
        let recording = Arc::new(AtomicBool::new(false));
        let active = recording.clone();
        let capturing = Arc::new(AtomicBool::new(false));
        let cap = capturing.clone();
        let (store_tx, store_rx) = bounded(64);
        let store_state = state.clone();
        let store_active = active.clone();
        let store_cap = cap.clone();
        let store_config = config.clone();
        let storage =
            thread::Builder::new()
                .name("airwav-storage".into())
                .spawn(move || -> Result<()> {
                    let mut writer: Option<Writer> = None;
                    let mut storage_error = None;
                    for command in store_rx {
                        let result: Result<()> = (|| {
                            match command {
                                StoreCommand::Start(path) => {
                                    if writer.is_none() {
                                        writer = Some(Writer::create(
                                            &path,
                                            &store_config,
                                            source.clone(),
                                        )?);
                                        store_active.store(true, Ordering::Release);
                                        message(
                                            &store_state,
                                            format!("Recording {}", path.display()),
                                        );
                                    }
                                }
                                StoreCommand::Snapshot(s) => {
                                    if let Some(w) = writer.as_mut() {
                                        w.append_snapshot(&s)?;
                                    }
                                }
                                StoreCommand::Capture(s, pre, trigger) => {
                                    if let Some(w) = writer.as_mut() {
                                        let id = w.begin_event(*s, &pre, trigger)?;
                                        let mut state = store_state
                                            .lock()
                                            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                                        state.events.push(id.clone());
                                        if state.events.len() > 256 {
                                            state.events.remove(0);
                                        }
                                        state.message = format!("{id} • preserving measured IQ");
                                    }
                                }
                                StoreCommand::Iq(block) => {
                                    if let Some(w) = writer.as_mut() {
                                        w.append_iq(&block)?;
                                    }
                                }
                                StoreCommand::Stop => {
                                    if let Some(w) = writer.take() {
                                        w.finish()?;
                                    }
                                    store_active.store(false, Ordering::Release);
                                    message(
                                        &store_state,
                                        "Recording finalized; IQ hashes and metadata saved",
                                    );
                                }
                            }
                            Ok(())
                        })();
                        if let Err(error) = result {
                            tracing::error!(%error,"recording stopped");
                            message(&store_state, format!("Recording stopped: {error}"));
                            storage_error = Some(error);
                            writer.take();
                            store_active.store(false, Ordering::Release);
                        }
                        store_cap.store(
                            writer.as_ref().is_some_and(Writer::capture_active),
                            Ordering::Release,
                        );
                    }
                    if let Some(w) = writer {
                        w.finish()?;
                    }
                    store_active.store(false, Ordering::Release);
                    store_cap.store(false, Ordering::Release);
                    if let Some(error) = storage_error {
                        return Err(error);
                    }
                    Ok(())
                })?;
        let worker=thread::Builder::new().name("airwav-dsp".into()).spawn(move ||->Result<()> {
            let result:Result<()>= (|| {
                let mut fft=SpectrumEngine::new(config.fft_size)?;let mut detector=Detector::new(config.detection_snr_db,config.receiver.sample_rate);
                let mut ring=IqRing::new(config.ring_bytes());let mut latest:Option<Snapshot>=None;let mut next_snapshot=0;let mut processed=0;
                let mut next_sample=0;let mut capture_until=0;let mut dropped_snapshots=0;let mut dropped_iq=0;let mut record_requested=false;
                if let Some(path)=initial_recording {
                    active.store(true,Ordering::Release);store_tx.try_send(StoreCommand::Start(path))?;record_requested=true;
                }
                loop {
                    if stop_worker.load(Ordering::Acquire){break;}
                    let mut quit=false;
                    while let Ok(command)=commands.try_recv() {
                        match command {
                            Control::Quit=>{quit=true;break;},
                            Control::Record=>{
                                if record_requested && active.load(Ordering::Acquire) {
                                    if store_tx.try_send(StoreCommand::Stop).is_ok(){record_requested=false;capture_until=0;}else{message(&out,"Storage busy; retry stopping the recording");}
                                }else {
                                    let path = data.join("sessions").join(format!("{}.awr", now_ns()));
                                    let Some(parent) = path.parent() else {
                                        message(&out, "Internal error: recording path has no parent");
                                        continue;
                                    };
                                    if let Err(error) = std::fs::create_dir_all(parent) {
                                        message(&out, format!("Cannot create sessions directory: {error}"));
                                        continue;
                                    }
                                    active.store(true, Ordering::Release);
                                    if store_tx.try_send(StoreCommand::Start(path)).is_ok() {
                                        record_requested = true;
                                    } else {
                                        active.store(false, Ordering::Release);
                                        message(&out, "Storage busy; recording did not start");
                                    }
                                }
                            },
                            Control::Capture=>{
                                if capture_until>next_sample{message(&out,"An event is already collecting post-trigger IQ");continue;}
                                if !record_requested || !active.load(Ordering::Acquire){message(&out,"Start a recording with R before capturing an event");continue;}
                                if let Some(s)=latest.as_ref() {
                                    let command=StoreCommand::Capture(Box::new(s.clone()),ring.snapshot(),next_sample);
                                    if store_tx.try_send(command).is_ok(){capture_until=next_sample+config.post_trigger_seconds as u64*config.receiver.sample_rate as u64;cap.store(true,Ordering::Release);}else{message(&out,"Storage queue full; capture was not started");}
                                }else{message(&out,"Waiting for the first received spectrum before capture");}
                            },
                        }
                    }
                    if quit{break;}
                    let block=match stream.receiver.recv_timeout(Duration::from_millis(20)) {
                        Ok(b)=>Arc::new(b),Err(crossbeam_channel::RecvTimeoutError::Timeout)=>{if stream.is_finished(){anyhow::bail!("Receiver stream ended unexpectedly; check USB connection and run airwav doctor");}continue;},
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected)=>{anyhow::bail!("Receiver disconnected; current recording will be finalized with available data");},
                    };
                    if stream.stats.snapshot().malformed_bytes>0{anyhow::bail!("Malformed IQ buffer received; stream stopped to preserve sample alignment");}
                    next_sample=block.first_sample+block.samples();processed+=block.samples();
                    if capture_until>block.first_sample && active.load(Ordering::Acquire) && store_tx.try_send(StoreCommand::Iq(block.clone())).is_err() {
                        dropped_iq+=block.samples();message(&out,"Storage overload: event IQ block dropped; capture will be marked incomplete");
                        tracing::warn!(first_sample=block.first_sample,"event IQ queue full");
                    }
                    let start=Instant::now();let spectrum=fft.push(&block,&config.receiver)?;ring.push(block.clone());
                    if let Some(spectrum)=spectrum {
                        let islands=detector.update(&spectrum);let mut metrics=stream.stats.snapshot();metrics.processed_samples=processed;metrics.discontinuities=fft.discontinuities;
                        metrics.dsp_us=start.elapsed().as_micros() as u64;metrics.ring_bytes=ring.bytes() as u64;metrics.ring_capacity_bytes=config.ring_bytes() as u64;metrics.storage_dropped_snapshots=dropped_snapshots;metrics.storage_dropped_iq_samples=dropped_iq;metrics.island_candidates_omitted=detector.candidates_omitted;
                        let snapshot=Snapshot{timestamp_ns:block.received_ns,receiver:config.receiver.clone(),spectrum,islands,metrics};
                        latest=Some(snapshot.clone());
                        if next_sample>=next_snapshot {
                            next_snapshot=next_sample+config.receiver.sample_rate as u64/10;
                            if active.load(Ordering::Acquire) && store_tx.try_send(StoreCommand::Snapshot(Box::new(snapshot.clone()))).is_err(){dropped_snapshots+=1;}
                            if let Ok(mut state)=out.lock(){state.latest=Some(snapshot);}
                        }
                    }
                }
                Ok(())
            })();
            let stopped=stream.stop();drop(store_tx);
            let saved=storage.join().map_err(|_|anyhow::anyhow!("storage worker panicked"))?;
            if let Err(error)=&result{message(&out,error.to_string());}
            if let Ok(mut state)=out.lock(){state.finished=true;}
            result?;stopped?;saved?;Ok(())
        }).context("start DSP worker")?;
        Ok(Self {
            control,
            state,
            recording,
            capturing,
            worker: Some(worker),
            stop,
        })
    }
    pub fn stop(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        let _ = self.control.try_send(Control::Quit);
        if let Some(w) = self.worker.take() {
            w.join()
                .map_err(|_| anyhow::anyhow!("DSP worker panicked"))??;
        }
        Ok(())
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
