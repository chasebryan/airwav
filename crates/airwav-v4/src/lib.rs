//! V4-only librtlsdr boundary. All unsafe code lives in this crate.
//! Configuration is exclusive; only cancel_async may overlap read_async.
use airwav_core::{BLOCK_BYTES, DeviceIdentity, IqBlock, Metrics, ReceiverConfig, now_ns};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_void},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use thiserror::Error;

type Dev = *mut c_void;
type Callback = unsafe extern "C" fn(*mut u8, u32, *mut c_void);
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(
        "V4-capable librtlsdr could not be loaded: {0}. Install the RTL-SDR Blog driver; see docs/linux.md. Run airwav doctor."
    )]
    Library(String),
    #[error("AIRWAV requires an RTL-SDR Blog V4. {0}")]
    Device(String),
    #[error(
        "RTL-SDR Blog V4 detected, but the driver failed V4 validation: {0}. A generic or outdated driver may tune incorrectly. Install the RTL-SDR Blog V4 driver and run airwav doctor."
    )]
    Driver(String),
    #[error(
        "{operation} failed (librtlsdr code {code}). Check USB permissions, connection, and whether another application or DVB kernel driver owns the receiver."
    )]
    Usb { operation: &'static str, code: i32 },
    #[error("{0}")]
    Config(String),
    #[error(
        "Receiver did not stop within 2 seconds; its handle is retained by the worker to avoid use-after-free."
    )]
    Shutdown,
}
fn check(code: i32, operation: &'static str) -> Result<()> {
    if code < 0 {
        Err(Error::Usb { operation, code })
    } else {
        Ok(())
    }
}

macro_rules! api {
    ($($field:ident : $ty:ty = $symbol:literal),+ $(,)?) => {
        struct Api { _library: Library, $($field: $ty),+ }
        impl Api {
            fn load(path: &Path) -> Result<Self> {
                // SAFETY: librtlsdr is a trusted, user-installed native library. All
                // symbol signatures below match its public C ABI; Library outlives them.
                unsafe {
                    let library = Library::new(path).map_err(|e| Error::Library(e.to_string()))?;
                    $(let $field = *library.get::<$ty>(concat!($symbol, "\0").as_bytes())
                        .map_err(|e| Error::Library(e.to_string()))?;)+
                    Ok(Self { _library: library, $($field),+ })
                }
            }
        }
    }
}
api! {
    count: unsafe extern "C" fn() -> u32 = "rtlsdr_get_device_count",
    strings: unsafe extern "C" fn(u32,*mut c_char,*mut c_char,*mut c_char)->c_int = "rtlsdr_get_device_usb_strings",
    opened_strings: unsafe extern "C" fn(Dev,*mut c_char,*mut c_char,*mut c_char)->c_int = "rtlsdr_get_usb_strings",
    open: unsafe extern "C" fn(*mut Dev,u32)->c_int = "rtlsdr_open",
    close: unsafe extern "C" fn(Dev)->c_int = "rtlsdr_close",
    tuner: unsafe extern "C" fn(Dev)->c_int = "rtlsdr_get_tuner_type",
    xtal: unsafe extern "C" fn(Dev,*mut u32,*mut u32)->c_int = "rtlsdr_get_xtal_freq",
    rate: unsafe extern "C" fn(Dev,u32)->c_int = "rtlsdr_set_sample_rate",
    get_rate: unsafe extern "C" fn(Dev)->u32 = "rtlsdr_get_sample_rate",
    center: unsafe extern "C" fn(Dev,u32)->c_int = "rtlsdr_set_center_freq",
    get_center: unsafe extern "C" fn(Dev)->u32 = "rtlsdr_get_center_freq",
    gain_mode: unsafe extern "C" fn(Dev,c_int)->c_int = "rtlsdr_set_tuner_gain_mode",
    gain: unsafe extern "C" fn(Dev,c_int)->c_int = "rtlsdr_set_tuner_gain",
    gains: unsafe extern "C" fn(Dev,*mut c_int)->c_int = "rtlsdr_get_tuner_gains",
    ppm: unsafe extern "C" fn(Dev,c_int)->c_int = "rtlsdr_set_freq_correction",
    get_ppm: unsafe extern "C" fn(Dev)->c_int = "rtlsdr_get_freq_correction",
    bias: unsafe extern "C" fn(Dev,c_int)->c_int = "rtlsdr_set_bias_tee",
    eeprom: unsafe extern "C" fn(Dev,*mut u8,u8,u16)->c_int = "rtlsdr_read_eeprom",
    agc: unsafe extern "C" fn(Dev,c_int)->c_int = "rtlsdr_set_agc_mode",
    reset: unsafe extern "C" fn(Dev)->c_int = "rtlsdr_reset_buffer",
    test: unsafe extern "C" fn(Dev,c_int)->c_int = "rtlsdr_set_testmode",
    read: unsafe extern "C" fn(Dev,Option<Callback>,*mut c_void,u32,u32)->c_int = "rtlsdr_read_async",
    cancel: unsafe extern "C" fn(Dev)->c_int = "rtlsdr_cancel_async",
}

pub struct V4Driver {
    api: Arc<Api>,
    pub library: String,
}
impl V4Driver {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        if let Some(path) = path {
            return Ok(Self {
                api: Arc::new(Api::load(path)?),
                library: path.display().to_string(),
            });
        }
        let mut errors = Vec::new();
        for name in ["librtlsdr.so.0", "librtlsdr.so"] {
            match Api::load(Path::new(name)) {
                Ok(api) => {
                    return Ok(Self {
                        api: Arc::new(api),
                        library: name.into(),
                    });
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        Err(Error::Library(errors.join("; ")))
    }
    pub fn devices(&self) -> Result<Vec<DeviceIdentity>> {
        // SAFETY: enumeration has no device handles and output arrays are 256 bytes.
        unsafe {
            let count = (self.api.count)();
            if count > 256 {
                return Err(Error::Driver("implausible USB device count".into()));
            }
            let mut devices = Vec::new();
            for index in 0..count {
                let (mut m, mut p, mut s) = ([0i8; 256], [0i8; 256], [0i8; 256]);
                check(
                    (self.api.strings)(index, m.as_mut_ptr(), p.as_mut_ptr(), s.as_mut_ptr()),
                    "read USB identity",
                )?;
                devices.push(identity(index, &m, &p, &s));
            }
            Ok(devices)
        }
    }
    pub fn open(&self, config: &ReceiverConfig) -> Result<V4Receiver> {
        config
            .validate()
            .map_err(|e| Error::Config(e.to_string()))?;
        let found: Vec<_> = self
            .devices()?
            .into_iter()
            .filter(|d| d.is_v4() && config.serial.as_ref().is_none_or(|s| *s == d.serial))
            .collect();
        if found.is_empty() {
            return Err(Error::Device(
                "No matching V4 is accessible; check the serial selection and USB permissions."
                    .into(),
            ));
        }
        if found.len() > 1 {
            return Err(Error::Device(
                "Multiple V4 units match; select a unique serial in config.".into(),
            ));
        }
        let identity = found.into_iter().next().expect("one match");
        let mut ptr = std::ptr::null_mut();
        // SAFETY: ptr is a valid out pointer; successful open transfers one handle.
        unsafe {
            check((self.api.open)(&mut ptr, identity.index), "open V4")?;
        }
        if ptr.is_null() {
            return Err(Error::Driver("open returned a null handle".into()));
        }
        let device = Arc::new(Device {
            ptr,
            api: self.api.clone(),
        });
        // SAFETY: exclusive access before streaming; output pointers have correct sizes.
        unsafe {
            let (mut m, mut p, mut s) = ([0i8; 256], [0i8; 256], [0i8; 256]);
            check(
                (self.api.opened_strings)(ptr, m.as_mut_ptr(), p.as_mut_ptr(), s.as_mut_ptr()),
                "recheck V4 identity",
            )?;
            let opened = identity_from_open(identity.index, &m, &p, &s);
            if !opened.is_v4() || opened.serial != identity.serial {
                return Err(Error::Device(
                    "USB identity changed during open; reconnect and retry.".into(),
                ));
            }
            let tuner = (self.api.tuner)(ptr);
            let (mut rtl, mut tuner_clock) = (0, 0);
            check(
                (self.api.xtal)(ptr, &mut rtl, &mut tuner_clock),
                "read initialized tuner clock",
            )?;
            validate_driver(tuner, rtl, tuner_clock)?;
            // The Blog driver can force bias power from EEPROM. Never claim it is off.
            let mut eeprom_flag = 0u8;
            check(
                (self.api.eeprom)(ptr, &mut eeprom_flag, 7, 1),
                "read bias-tee EEPROM policy",
            )?;
            if eeprom_flag & 2 == 0 && !config.bias_tee {
                return Err(Error::Config("The V4 EEPROM forces bias-tee power on, but config requests off. AIRWAV does not rewrite EEPROM. Restore the EEPROM setting using the vendor tools before continuing.".into()));
            }
            check((self.api.rate)(ptr, config.sample_rate), "set sample rate")?;
            if (self.api.get_ppm)(ptr) != config.ppm {
                check((self.api.ppm)(ptr, config.ppm), "set frequency correction")?;
            }
            check((self.api.center)(ptr, config.center_hz), "tune V4")?;
            check((self.api.agc)(ptr, 0), "disable digital AGC")?;
            check(
                (self.api.gain_mode)(ptr, i32::from(config.gain_tenth_db.is_some())),
                "set tuner gain mode",
            )?;
            if let Some(gain) = config.gain_tenth_db {
                let n = (self.api.gains)(ptr, std::ptr::null_mut());
                if !(1..=256).contains(&n) {
                    return Err(Error::Driver("invalid gain table".into()));
                }
                let mut gains = vec![0; n as usize];
                let actual = (self.api.gains)(ptr, gains.as_mut_ptr());
                if actual != n || !gains.contains(&gain) {
                    return Err(Error::Config(format!(
                        "Unsupported gain {gain}; available gains in tenths of dB: {gains:?}"
                    )));
                }
                check((self.api.gain)(ptr, gain), "set tuner gain")?;
            }
            check(
                (self.api.bias)(ptr, i32::from(config.bias_tee)),
                "set bias tee",
            )?;
            check((self.api.test)(ptr, 0), "disable counter test")?;
            check((self.api.reset)(ptr), "reset USB buffer")?;
            let mut actual = config.clone();
            actual.sample_rate = (self.api.get_rate)(ptr);
            actual.center_hz = (self.api.get_center)(ptr);
            actual
                .validate()
                .map_err(|e| Error::Driver(e.to_string()))?;
            if actual.sample_rate.abs_diff(config.sample_rate) > 10
                || actual.center_hz != config.center_hz
            {
                return Err(Error::Driver(
                    "receiver readback does not match requested configuration".into(),
                ));
            }
            Ok(V4Receiver {
                device,
                identity,
                config: actual,
            })
        }
    }
}
fn text_string(a: &[i8; 256]) -> String {
    let b: Vec<u8> = a
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&b).into_owned()
}
fn identity(index: u32, m: &[i8; 256], p: &[i8; 256], s: &[i8; 256]) -> DeviceIdentity {
    DeviceIdentity {
        index,
        manufacturer: text_string(m),
        product: text_string(p),
        serial: text_string(s),
    }
}
fn identity_from_open(index: u32, m: &[i8; 256], p: &[i8; 256], s: &[i8; 256]) -> DeviceIdentity {
    identity(index, m, p, s)
}
pub fn validate_driver(tuner: i32, rtl: u32, tuner_clock: u32) -> Result<()> {
    if tuner != 6 {
        return Err(Error::Driver(format!(
            "expected R828D tuner (6), got {tuner}"
        )));
    }
    if rtl != 28_800_000 || tuner_clock != 28_800_000 {
        return Err(Error::Driver(format!(
            "expected initialized clocks 28.8 MHz; got RTL={rtl}, tuner={tuner_clock}"
        )));
    }
    Ok(())
}
struct Device {
    ptr: Dev,
    api: Arc<Api>,
}
// SAFETY: Device is private. Only the streaming worker uses read; configuration
// finishes before sharing. The only concurrent call is the documented cancel API.
unsafe impl Send for Device {}
// SAFETY: the same private read/cancel invariant prevents other concurrent FFI.
unsafe impl Sync for Device {}
impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: final Arc drops only after read_async returned or the retained worker exits.
        unsafe {
            (self.api.bias)(self.ptr, 0);
            (self.api.close)(self.ptr);
        }
    }
}

pub struct V4Receiver {
    device: Arc<Device>,
    pub identity: DeviceIdentity,
    pub config: ReceiverConfig,
}
impl V4Receiver {
    pub fn start(self, queue_blocks: usize, counter_test: bool) -> Result<Stream> {
        if !(2..=256).contains(&queue_blocks) {
            return Err(Error::Config("queue capacity must be 2..=256".into()));
        }
        if counter_test {
            // SAFETY: streaming has not started, configuration is exclusive.
            unsafe {
                check(
                    (self.device.api.test)(self.device.ptr, 1),
                    "enable hardware counter test",
                )?;
            }
        }
        let (tx, rx) = bounded(queue_blocks);
        let stats = Arc::new(StreamStats::default());
        let stop = Arc::new(AtomicBool::new(false));
        let device = self.device.clone();
        let mut context = Context {
            tx,
            stats: stats.clone(),
            stop: stop.clone(),
            device: device.clone(),
            next_sample: 0,
            counter_test,
            expected_byte: None,
        };
        let worker = thread::Builder::new()
            .name("airwav-v4".into())
            .spawn(move || {
                if context.stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                // SAFETY: context is pinned on this stack for the duration of the
                // blocking C read. librtlsdr calls the callback serially on this thread.
                let code = unsafe {
                    (device.api.read)(
                        device.ptr,
                        Some(callback),
                        (&mut context as *mut Context).cast(),
                        12,
                        BLOCK_BYTES as u32,
                    )
                };
                check(code, "asynchronous IQ capture")
            })
            .map_err(|e| Error::Config(format!("cannot spawn receiver: {e}")))?;
        Ok(Stream {
            receiver: rx,
            stats,
            device: self.device,
            stop,
            worker: Some(worker),
        })
    }
}
#[derive(Default)]
pub struct StreamStats {
    received: AtomicU64,
    dropped: AtomicU64,
    malformed: AtomicU64,
    pub counter_discontinuities: AtomicU64,
    pub counter_missing_bytes_lower_bound: AtomicU64,
    pub callback_panics: AtomicU64,
}
impl StreamStats {
    pub fn snapshot(&self) -> Metrics {
        Metrics {
            received_samples: self.received.load(Ordering::Relaxed),
            queue_dropped_samples: self.dropped.load(Ordering::Relaxed),
            malformed_bytes: self.malformed.load(Ordering::Relaxed),
            ..Metrics::default()
        }
    }
}
struct Context {
    tx: Sender<IqBlock>,
    stats: Arc<StreamStats>,
    stop: Arc<AtomicBool>,
    device: Arc<Device>,
    next_sample: u64,
    counter_test: bool,
    expected_byte: Option<u8>,
}
impl Context {
    fn consume(&mut self, bytes: &[u8]) {
        if bytes.is_empty() || !bytes.len().is_multiple_of(2) || bytes.len() > BLOCK_BYTES {
            self.stats
                .malformed
                .fetch_add(bytes.len() as u64, Ordering::Relaxed);
            return;
        }
        if self.counter_test {
            for &b in bytes {
                if let Some(expected) = self.expected_byte
                    && b != expected
                {
                    self.stats
                        .counter_discontinuities
                        .fetch_add(1, Ordering::Relaxed);
                    self.stats
                        .counter_missing_bytes_lower_bound
                        .fetch_add(b.wrapping_sub(expected) as u64, Ordering::Relaxed);
                }
                self.expected_byte = Some(b.wrapping_add(1));
            }
        }
        let n = (bytes.len() / 2) as u64;
        self.stats.received.fetch_add(n, Ordering::Relaxed);
        let position = self.next_sample;
        self.next_sample += n;
        // Avoid allocating when the bounded queue is already saturated.
        if self.tx.is_full() {
            self.stats.dropped.fetch_add(n, Ordering::Relaxed);
            return;
        }
        let block = IqBlock {
            first_sample: position,
            received_ns: now_ns(),
            bytes: bytes.to_vec(),
        };
        match self.tx.try_send(block) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.stats.dropped.fetch_add(n, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {
                self.stop.store(true, Ordering::Release);
            }
        }
    }
}
unsafe extern "C" fn callback(buf: *mut u8, len: u32, ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: read_async receives a live Context; callbacks are serial.
    let context = unsafe { &mut *ctx.cast::<Context>() };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !buf.is_null() && len as usize <= BLOCK_BYTES {
            // SAFETY: librtlsdr owns this buffer for the callback duration only.
            context.consume(unsafe { std::slice::from_raw_parts(buf, len as usize) });
        } else {
            context
                .stats
                .malformed
                .fetch_add(len as u64, Ordering::Relaxed);
        }
    }));
    if outcome.is_err() {
        context
            .stats
            .callback_panics
            .fetch_add(1, Ordering::Relaxed);
        context.stop.store(true, Ordering::Release);
    }
    if context.stop.load(Ordering::Acquire) {
        // SAFETY: cancel is explicitly permitted during read_async, including callbacks.
        unsafe {
            (context.device.api.cancel)(context.device.ptr);
        }
    }
}
pub struct Stream {
    pub receiver: Receiver<IqBlock>,
    pub stats: Arc<StreamStats>,
    device: Arc<Device>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
}
impl Stream {
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(|w| w.is_finished())
    }
    pub fn stop(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        let start = Instant::now();
        while self.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            // SAFETY: Arc retains the handle and library, and only cancellation overlaps read.
            unsafe {
                (self.device.api.cancel)(self.device.ptr);
            }
            if start.elapsed() > Duration::from_secs(2) {
                return Err(Error::Shutdown);
            }
            thread::sleep(Duration::from_millis(5));
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| Error::Config("receiver worker panicked".into()))??;
        }
        if self.stats.callback_panics.load(Ordering::Relaxed) != 0 {
            return Err(Error::Config(
                "IQ callback panicked; capture stopped".into(),
            ));
        }
        Ok(())
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            tracing::error!(%error,"capture shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_r828d_driver_fails_closed() {
        assert!(validate_driver(6, 28_800_000, 16_000_000).is_err());
    }
    #[test]
    fn r820t_is_not_v4() {
        assert!(validate_driver(5, 28_800_000, 28_800_000).is_err());
    }
    #[test]
    fn v4_behavior_passes() {
        validate_driver(6, 28_800_000, 28_800_000).unwrap();
    }
    #[test]
    fn fixed_buffers_need_no_nul() {
        assert_eq!(text_string(&[65; 256]).len(), 256);
    }
}
