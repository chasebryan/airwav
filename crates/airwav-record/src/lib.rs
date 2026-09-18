//! Versioned, bounded AWR bundles. JSONL is the recovery source; SQLite is an index.
use airwav_core::{Config, DeviceIdentity, IqBlock, ReceiverConfig, Snapshot, now_ns};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
const MAX_LINE: u64 = 2 * 1024 * 1024;
const RESERVE: u64 = 128 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Source {
    LiveV4 {
        device: DeviceIdentity,
        library: String,
    },
    DemoFixture {
        description: String,
    },
}
impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LiveV4 { .. } => "RECORDED V4",
            Self::DemoFixture { .. } => "DEMO FIXTURE",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub session_id: String,
    pub created_ns: u64,
    pub finalized_ns: Option<u64>,
    pub source: Source,
    pub receiver: ReceiverConfig,
    pub sample_format: String,
    pub complete: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub first_sample: u64,
    pub received_ns: u64,
    pub file_offset: u64,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub bytes: u64,
    pub blake3: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub timestamp_ns: u64,
    pub trigger_sample: u64,
    pub requested_post_samples: u64,
    pub complete: bool,
    pub iq: Artifact,
    pub chunks: Vec<Chunk>,
    pub evidence: Snapshot,
}
struct Capture {
    id: String,
    file: File,
    path: String,
    hasher: blake3::Hasher,
    bytes: u64,
    chunks: Vec<Chunk>,
    trigger_sample: u64,
    end_sample: u64,
    evidence: Snapshot,
}
impl Capture {
    fn append(&mut self, block: &IqBlock) -> Result<()> {
        let count = block
            .samples()
            .min(self.end_sample.saturating_sub(block.first_sample));
        if count == 0 {
            return Ok(());
        }
        let bytes = &block.bytes[..count as usize * 2];
        self.file.write_all(bytes)?;
        self.hasher.update(bytes);
        self.chunks.push(Chunk {
            first_sample: block.first_sample,
            received_ns: block.received_ns,
            file_offset: self.bytes,
            bytes: bytes.len() as u64,
        });
        self.bytes += bytes.len() as u64;
        Ok(())
    }
}
pub struct Writer {
    pub root: PathBuf,
    pub manifest: Manifest,
    observations: File,
    events: File,
    db: Connection,
    config: Config,
    event_id: u64,
    capture: Option<Capture>,
    finished: bool,
}
impl Writer {
    pub fn create(root: &Path, config: &Config, source: Source) -> Result<Self> {
        config.validate()?;
        ensure!(
            root.extension().is_some_and(|e| e == "awr"),
            "Recording directory must end in .awr"
        );
        fs::create_dir(root).with_context(|| {
            format!(
                "Cannot create {}; AIRWAV never overwrites a recording",
                root.display()
            )
        })?;
        fs::create_dir(root.join("iq"))?;
        let manifest = Manifest {
            format: "airwav-recording".into(),
            version: 1,
            session_id: format!("AW-SESSION-{}", now_ns()),
            created_ns: now_ns(),
            finalized_ns: None,
            source,
            receiver: config.receiver.clone(),
            sample_format: "cu8-interleaved".into(),
            complete: false,
        };
        atomic_json(root, "manifest.json", &manifest)?;
        atomic_json(root, "session.json", config)?;
        let observations = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("observations.jsonl"))?;
        let events = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("events.jsonl"))?;
        let db = Connection::open(root.join("index.sqlite"))?;
        migrate(&db)?;
        db.execute(
            "INSERT INTO sessions(id, created_ns, source) VALUES (?1, ?2, ?3)",
            params![
                manifest.session_id,
                manifest.created_ns,
                serde_json::to_string(&manifest.source)?
            ],
        )?;
        let writer = Self {
            root: root.into(),
            manifest,
            observations,
            events,
            db,
            config: config.clone(),
            event_id: 0,
            capture: None,
            finished: false,
        };
        writer.check_budget(0)?;
        Ok(writer)
    }
    fn check_budget(&self, incoming: u64) -> Result<()> {
        let used = directory_bytes(&self.root)?;
        ensure!(
            used.saturating_add(incoming).saturating_add(RESERVE)
                <= self.config.max_recording_bytes,
            "Recording size limit reached; metadata and existing captures were preserved"
        );
        let available = fs2::available_space(&self.root)?;
        ensure!(
            available
                >= self
                    .config
                    .minimum_free_bytes
                    .saturating_add(incoming)
                    .saturating_add(RESERVE),
            "Minimum free disk threshold reached; recording stopped before the disk filled"
        );
        Ok(())
    }
    pub fn append_snapshot(&mut self, snapshot: &Snapshot) -> Result<()> {
        validate_snapshot(snapshot)?;
        let bytes = json_line(snapshot)?;
        self.check_budget(bytes.len() as u64)?;
        self.observations.write_all(&bytes)?;
        self.observations.sync_data()?;
        self.db.execute(
            "INSERT INTO observations(timestamp_ns, first_sample, json) VALUES (?1, ?2, ?3)",
            params![
                snapshot.timestamp_ns,
                snapshot.spectrum.first_sample,
                String::from_utf8(bytes)?
            ],
        )?;
        Ok(())
    }
    pub fn capture_active(&self) -> bool {
        self.capture.is_some()
    }
    pub fn begin_event(
        &mut self,
        snapshot: Snapshot,
        pre: &[Arc<IqBlock>],
        trigger_sample: u64,
    ) -> Result<String> {
        ensure!(
            self.capture.is_none(),
            "An event capture is already collecting post-trigger IQ"
        );
        validate_snapshot(&snapshot)?;
        let bytes = pre.iter().map(|b| b.bytes.len() as u64).sum::<u64>();
        let post =
            self.config.post_trigger_seconds as u64 * self.manifest.receiver.sample_rate as u64;
        let end_sample = trigger_sample
            .checked_add(post)
            .context("Event sample position overflow")?;
        let mut previous_end = None;
        for block in pre {
            ensure!(
                block.bytes.len().is_multiple_of(2),
                "Pre-trigger IQ requires complete I/Q pairs"
            );
            let end = block
                .first_sample
                .checked_add(block.samples())
                .context("IQ sample position overflow")?;
            ensure!(
                end <= trigger_sample,
                "Pre-trigger IQ extends beyond trigger"
            );
            if let Some(previous) = previous_end {
                ensure!(
                    block.first_sample == previous,
                    "Pre-trigger IQ is discontinuous"
                );
            }
            previous_end = Some(end);
        }
        if let Some(end) = previous_end {
            ensure!(
                end == trigger_sample,
                "Pre-trigger IQ must end at the trigger"
            );
        }
        self.check_budget(bytes + post * 2 + MAX_LINE)?;
        self.event_id += 1;
        let id = format!("AW-EVENT-{:06}", self.event_id);
        let path = format!("iq/{id}.cu8");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(format!("{path}.partial")))?;
        let mut capture = Capture {
            id: id.clone(),
            file,
            path,
            hasher: blake3::Hasher::new(),
            bytes: 0,
            chunks: Vec::new(),
            trigger_sample,
            end_sample,
            evidence: snapshot,
        };
        for block in pre {
            ensure!(
                block.first_sample + block.samples() <= trigger_sample,
                "Pre-trigger IQ cannot extend beyond the trigger"
            );
            capture.append(block)?;
        }
        self.capture = Some(capture);
        if post == 0 {
            self.finish_event(true)?;
        }
        Ok(id)
    }
    pub fn append_iq(&mut self, block: &IqBlock) -> Result<()> {
        if self.capture.is_none() {
            return Ok(());
        }
        self.check_budget(block.bytes.len() as u64)?;
        let capture = self.capture.as_mut().expect("checked capture");
        let expected = capture
            .chunks
            .last()
            .map(|c| c.first_sample + c.bytes / 2)
            .unwrap_or(capture.trigger_sample);
        if block.first_sample != expected {
            self.finish_event(false)?;
            bail!("IQ discontinuity interrupted event capture; the partial event was preserved");
        }
        capture.append(block)?;
        if block.first_sample + block.samples() >= capture.end_sample {
            self.finish_event(true)?;
        }
        Ok(())
    }
    fn finish_event(&mut self, complete: bool) -> Result<()> {
        if let Some(capture) = self.capture.take() {
            capture.file.sync_all()?;
            fs::rename(
                self.root.join(format!("{}.partial", capture.path)),
                self.root.join(&capture.path),
            )?;
            sync_dir(&self.root.join("iq"))?;
            let event = Event {
                id: capture.id,
                timestamp_ns: capture.evidence.timestamp_ns,
                trigger_sample: capture.trigger_sample,
                requested_post_samples: capture.end_sample - capture.trigger_sample,
                complete,
                iq: Artifact {
                    path: capture.path,
                    bytes: capture.bytes,
                    blake3: capture.hasher.finalize().to_hex().to_string(),
                },
                chunks: capture.chunks,
                evidence: capture.evidence,
            };
            let bytes = json_line(&event)?;
            self.events.write_all(&bytes)?;
            self.events.sync_all()?;
            self.db.execute(
                "INSERT INTO events(id, timestamp_ns, json) VALUES (?1, ?2, ?3)",
                params![event.id, event.timestamp_ns, String::from_utf8(bytes)?],
            )?;
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<Manifest> {
        self.finish_event(false)?;
        self.observations.sync_all()?;
        self.events.sync_all()?;
        self.db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        self.manifest.complete = true;
        self.manifest.finalized_ns = Some(now_ns());
        atomic_json(&self.root, "manifest.json", &self.manifest)?;
        self.finished = true;
        Ok(self.manifest.clone())
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.observations.sync_data();
            let _ = self.events.sync_data();
        }
        // Deliberately leave the incomplete manifest and .partial artifact on failure.
    }
}
fn migrate(db: &Connection) -> Result<()> {
    let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(version <= 1, "Unsupported future SQLite schema {version}");
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
    if version == 0 {
        db.execute_batch("BEGIN IMMEDIATE;
        CREATE TABLE sessions(id TEXT PRIMARY KEY, created_ns INTEGER NOT NULL, source TEXT NOT NULL);
        CREATE TABLE observations(id INTEGER PRIMARY KEY, timestamp_ns INTEGER NOT NULL, first_sample INTEGER NOT NULL, json TEXT NOT NULL);
        CREATE INDEX observations_time ON observations(timestamp_ns);
        CREATE TABLE events(id TEXT PRIMARY KEY, timestamp_ns INTEGER NOT NULL, json TEXT NOT NULL);
        CREATE TRIGGER immutable_observation_update BEFORE UPDATE ON observations BEGIN SELECT RAISE(ABORT,'observations are immutable'); END;
        CREATE TRIGGER immutable_observation_delete BEFORE DELETE ON observations BEGIN SELECT RAISE(ABORT,'observations are immutable'); END;
        CREATE TRIGGER immutable_event_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT,'events are immutable'); END;
        CREATE TRIGGER immutable_event_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT,'events are immutable'); END;
        PRAGMA user_version=1; COMMIT;")?;
    }
    Ok(())
}
fn json_line<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut b = serde_json::to_vec(value)?;
    ensure!(
        (b.len() as u64) < MAX_LINE,
        "Recording row exceeds size limit"
    );
    b.push(b'\n');
    Ok(b)
}
fn atomic_json<T: Serialize>(root: &Path, name: &str, value: &T) -> Result<()> {
    let path = root.join(format!(".{name}.new"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    fs::rename(&path, root.join(name))?;
    sync_dir(root)
}
fn sync_dir(root: &Path) -> Result<()> {
    File::open(root)?.sync_all()?;
    Ok(())
}
fn directory_bytes(root: &Path) -> Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total += directory_bytes(&entry.path())?;
        } else {
            total += metadata.len();
        }
    }
    Ok(total)
}
fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    ensure!(
        path.components().all(|p| matches!(p, Component::Normal(_))),
        "Unsafe recording artifact path: {relative}"
    );
    let canonical = root.canonicalize()?;
    let result = canonical.join(path).canonicalize()?;
    ensure!(
        result.starts_with(&canonical),
        "Artifact escapes its recording directory"
    );
    Ok(result)
}
fn bounded_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    ensure!(
        fs::metadata(path)?.len() <= MAX_LINE,
        "Recording metadata exceeds size limit"
    );
    Ok(serde_json::from_reader(File::open(path)?.take(MAX_LINE))?)
}
fn validate_snapshot(s: &Snapshot) -> Result<()> {
    s.receiver.validate()?;
    let p = &s.spectrum.power_dbfs;
    ensure!(
        (256..=16384).contains(&p.len()) && p.len().is_power_of_two(),
        "Invalid spectrum length"
    );
    ensure!(
        p.iter().all(|v| v.is_finite()) && s.spectrum.noise_dbfs.is_finite(),
        "Non-finite spectrum power"
    );
    ensure!(
        s.spectrum.bin_hz.is_finite() && s.spectrum.bin_hz > 0. && s.spectrum.start_hz.is_finite(),
        "Invalid spectrum axis"
    );
    ensure!(
        (s.spectrum.bin_hz * p.len() as f64 - s.receiver.sample_rate as f64).abs() < 1.,
        "Spectrum sample rate mismatch"
    );
    let expected_start = s.receiver.center_hz as f64 - s.receiver.sample_rate as f64 / 2.;
    ensure!(
        (s.spectrum.start_hz - expected_start).abs() < 1.,
        "Spectrum receiver-frequency mismatch"
    );

    ensure!(s.islands.len() <= 8192, "Too many signal islands");
    ensure!(
        s.islands.iter().all(|i| i.center_hz.is_finite()
            && i.bandwidth_hz.is_finite()
            && i.bandwidth_hz > 0.
            && i.snr_db.is_finite()
            && i.peak_dbfs.is_finite()),
        "Invalid signal measurement"
    );
    Ok(())
}

/// One bounded JSON row at a time, including when the final row was torn by a crash.
pub struct Journal<T> {
    reader: BufReader<File>,
    line: u64,
    allow_torn_tail: bool,
    done: bool,
    _item: std::marker::PhantomData<T>,
}
impl<T: serde::de::DeserializeOwned> Journal<T> {
    fn open(path: &Path, allow_torn_tail: bool) -> Result<Self> {
        Ok(Self {
            reader: BufReader::new(File::open(path)?),
            line: 0,
            allow_torn_tail,
            done: false,
            _item: std::marker::PhantomData,
        })
    }
}
impl<T: serde::de::DeserializeOwned> Iterator for Journal<T> {
    type Item = Result<T>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let mut line = Vec::new();
        let result = self
            .reader
            .by_ref()
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut line);
        match result {
            Err(e) => {
                self.done = true;
                Some(Err(e.into()))
            }
            Ok(0) => {
                self.done = true;
                None
            }
            Ok(n) => {
                self.line += 1;
                if n as u64 > MAX_LINE {
                    self.done = true;
                    return Some(Err(anyhow::anyhow!(
                        "Journal line {} exceeds size limit",
                        self.line
                    )));
                }
                if !line.ends_with(b"\n") {
                    self.done = true;
                    if self.allow_torn_tail {
                        return None;
                    }
                    return Some(Err(anyhow::anyhow!(
                        "Truncated journal at line {}",
                        self.line
                    )));
                }
                Some(
                    serde_json::from_slice(&line)
                        .with_context(|| format!("Invalid journal row {}", self.line)),
                )
            }
        }
    }
}
pub struct Reader {
    pub root: PathBuf,
    pub manifest: Manifest,
}
impl Reader {
    pub fn open(root: &Path) -> Result<Self> {
        let manifest: Manifest = bounded_json(&safe_path(root, "manifest.json")?)?;
        ensure!(
            manifest.format == "airwav-recording" && manifest.version == 1,
            "Unsupported AWR format/version; expected airwav-recording v1"
        );
        ensure!(
            manifest.sample_format == "cu8-interleaved",
            "Unsupported IQ sample format"
        );
        manifest.receiver.validate()?;
        if let Source::LiveV4 { device, .. } = &manifest.source {
            ensure!(
                device.is_v4(),
                "A LiveV4 recording must identify an RTL-SDR Blog V4"
            );
        }
        Ok(Self {
            root: root.into(),
            manifest,
        })
    }
    pub fn snapshots(&self) -> Result<impl Iterator<Item = Result<Snapshot>>> {
        Ok(Journal::<Snapshot>::open(
            &safe_path(&self.root, "observations.jsonl")?,
            !self.manifest.complete,
        )?
        .map(|row| {
            let s = row?;
            validate_snapshot(&s)?;
            Ok(s)
        }))
    }
    pub fn events(&self) -> Result<Journal<Event>> {
        Journal::open(
            &safe_path(&self.root, "events.jsonl")?,
            !self.manifest.complete,
        )
    }
    pub fn verify_event(&self, event: &Event) -> Result<()> {
        validate_snapshot(&event.evidence)?;
        let path = safe_path(&self.root, &event.iq.path)?;
        ensure!(
            fs::metadata(&path)?.len() == event.iq.bytes,
            "IQ length mismatch for {}",
            event.id
        );
        let mut file = File::open(path)?;
        let mut hasher = blake3::Hasher::new();
        let mut bytes = [0u8; 65536];
        loop {
            let n = file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            hasher.update(&bytes[..n]);
        }
        ensure!(
            hasher.finalize().to_hex().as_str() == event.iq.blake3,
            "IQ BLAKE3 mismatch for {}",
            event.id
        );
        let mut offset = 0;
        let mut previous = None;
        for c in &event.chunks {
            ensure!(
                c.bytes > 0 && c.bytes.is_multiple_of(2) && c.file_offset == offset,
                "Invalid IQ chunk index"
            );
            if let Some(last) = previous {
                ensure!(last == c.first_sample, "Unmarked gap in event IQ");
            }
            offset = offset
                .checked_add(c.bytes)
                .context("Chunk offset overflow")?;
            previous = c.first_sample.checked_add(c.bytes / 2);
            ensure!(previous.is_some(), "Sample index overflow");
        }
        ensure!(
            offset == event.iq.bytes,
            "IQ chunk index does not cover artifact"
        );
        if event.complete {
            let expected = event
                .trigger_sample
                .checked_add(event.requested_post_samples)
                .context("Event sample position overflow")?;
            ensure!(
                previous.unwrap_or(event.trigger_sample) == expected,
                "Complete event does not contain its requested post-trigger IQ"
            );
        }

        Ok(())
    }
    pub fn summary(&self) -> Result<serde_json::Value> {
        let mut observations = 0;
        for s in self.snapshots()? {
            s?;
            observations += 1;
        }
        let mut events = 0;
        let mut bytes = 0;
        for e in self.events()? {
            let e = e?;
            self.verify_event(&e)?;
            events += 1;
            bytes += e.iq.bytes;
        }
        Ok(
            serde_json::json!({"manifest":self.manifest,"observations":observations,"events":events,"verified_iq_bytes":bytes}),
        )
    }
}
/// Recover into a NEW bundle, preserving the original and only committing complete journal rows.
pub fn recover(source: &Path, destination: &Path) -> Result<Manifest> {
    let reader = Reader::open(source)?;
    ensure!(
        !reader.manifest.complete,
        "This session is already complete"
    );
    let config: Config = bounded_json(&safe_path(source, "session.json")?)?;
    let mut writer = Writer::create(destination, &config, reader.manifest.source.clone())?;
    for row in reader.snapshots()? {
        writer.append_snapshot(&row?)?;
    }
    for event in reader.events()? {
        let event = event?;
        reader.verify_event(&event)?;
        writer.check_budget(event.iq.bytes + MAX_LINE)?;
        let src = safe_path(source, &event.iq.path)?;
        ensure!(
            Path::new(&event.iq.path).parent() == Some(Path::new("iq")),
            "Unexpected artifact directory"
        );
        let dst = writer.root.join(&event.iq.path);
        let mut input = File::open(src)?;
        let mut output = OpenOptions::new().write(true).create_new(true).open(dst)?;
        std::io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        let bytes = json_line(&event)?;
        writer.events.write_all(&bytes)?;
        writer.db.execute(
            "INSERT INTO events(id,timestamp_ns,json) VALUES (?1,?2,?3)",
            params![event.id, event.timestamp_ns, String::from_utf8(bytes)?],
        )?;
    }
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            timestamp_ns: 42,
            receiver: ReceiverConfig::default(),
            spectrum: airwav_core::Spectrum {
                first_sample: 0,
                bin_hz: 10000.,
                start_hz: 134_720_000.,
                power_dbfs: vec![-80.; 256],
                noise_dbfs: -80.,
            },
            islands: vec![],
            metrics: Default::default(),
        }
    }
    fn source() -> Source {
        Source::DemoFixture {
            description: "unit test".into(),
        }
    }
    #[test]
    fn recording_roundtrip_hash_and_corruption_detection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.awr");
        let config = Config {
            post_trigger_seconds: 0,
            ..Config::default()
        };
        let mut w = Writer::create(&path, &config, source()).unwrap();
        let s = snapshot();
        w.append_snapshot(&s).unwrap();
        w.begin_event(
            s,
            &[Arc::new(IqBlock {
                first_sample: 0,
                received_ns: 0,
                bytes: vec![128; 2048],
            })],
            1024,
        )
        .unwrap();
        w.finish().unwrap();
        let r = Reader::open(&path).unwrap();
        assert!(r.manifest.complete);
        assert_eq!(r.snapshots().unwrap().count(), 1);
        let e = r.events().unwrap().next().unwrap().unwrap();
        r.verify_event(&e).unwrap();
        fs::write(path.join(&e.iq.path), vec![129; 2048]).unwrap();
        assert!(
            r.verify_event(&e)
                .unwrap_err()
                .to_string()
                .contains("BLAKE3")
        );
    }
    #[test]
    fn post_trigger_stops_at_exact_sample() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.awr");
        let config = Config {
            post_trigger_seconds: 1,
            ..Config::default()
        };
        let mut w = Writer::create(&path, &config, source()).unwrap();
        w.begin_event(snapshot(), &[], 10).unwrap();
        w.append_iq(&IqBlock {
            first_sample: 10,
            received_ns: 0,
            bytes: vec![128; 5_120_200],
        })
        .unwrap();
        w.finish().unwrap();
        let r = Reader::open(&path).unwrap();
        let e = r.events().unwrap().next().unwrap().unwrap();
        assert_eq!(e.iq.bytes, 5_120_000);
        assert!(e.complete);
        r.verify_event(&e).unwrap();
    }
    #[test]
    fn recovery_ignores_torn_tail_and_preserves_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.awr");
        let mut w = Writer::create(&path, &Config::default(), source()).unwrap();
        w.append_snapshot(&snapshot()).unwrap();
        drop(w);
        OpenOptions::new()
            .append(true)
            .open(path.join("observations.jsonl"))
            .unwrap()
            .write_all(b"{\"broken")
            .unwrap();
        let recovered = dir.path().join("recovered.awr");
        recover(&path, &recovered).unwrap();
        assert!(!Reader::open(&path).unwrap().manifest.complete);
        assert_eq!(
            Reader::open(&recovered)
                .unwrap()
                .snapshots()
                .unwrap()
                .count(),
            1
        );
    }
    #[test]
    fn future_version_and_path_escape_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.awr");
        let w = Writer::create(&path, &Config::default(), source()).unwrap();
        w.finish().unwrap();
        assert!(safe_path(&path, "../other").is_err());
        let mut r = Reader::open(&path).unwrap();
        r.manifest.version = 999;
        fs::write(
            path.join("manifest.json"),
            serde_json::to_vec(&r.manifest).unwrap(),
        )
        .unwrap();
        assert!(Reader::open(&path).is_err());
    }
    #[test]
    fn size_limit_and_overwrite_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.awr");
        let c = Config {
            max_recording_bytes: 1024 * 1024,
            ..Config::default()
        };
        let mut w = Writer::create(&path, &c, source()).unwrap();
        assert!(w.begin_event(snapshot(), &[], 0).is_err());
        assert!(Writer::create(&path, &c, source()).is_err());
    }
    #[test]
    fn migration_idempotent_and_history_immutable() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        db.execute(
            "INSERT INTO observations(timestamp_ns,first_sample,json) VALUES(1,1,'{}')",
            [],
        )
        .unwrap();
        assert!(
            db.execute("UPDATE observations SET json='bad'", [])
                .is_err()
        );
        assert!(db.execute("DELETE FROM observations", []).is_err());
    }
    #[test]
    fn invalid_pretrigger_is_rejected_before_artifact_creation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.awr");
        let mut writer = Writer::create(&path, &Config::default(), source()).unwrap();
        let pre = [
            Arc::new(IqBlock {
                first_sample: 0,
                received_ns: 0,
                bytes: vec![128; 10],
            }),
            Arc::new(IqBlock {
                first_sample: 10,
                received_ns: 0,
                bytes: vec![128; 10],
            }),
        ];
        assert!(writer.begin_event(snapshot(), &pre, 15).is_err());
        assert_eq!(std::fs::read_dir(path.join("iq")).unwrap().count(), 0);
    }
    #[test]
    fn gaps_end_capture_as_incomplete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.awr");
        let mut w = Writer::create(&path, &Config::default(), source()).unwrap();
        w.begin_event(snapshot(), &[], 0).unwrap();
        assert!(
            w.append_iq(&IqBlock {
                first_sample: 100,
                received_ns: 0,
                bytes: vec![128; 1024]
            })
            .is_err()
        );
        w.finish().unwrap();
        assert!(
            !Reader::open(&path)
                .unwrap()
                .events()
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .complete
        );
    }
}
