//! Shared observations and validated configuration. No hardware or UI dependencies.
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub const BLOCK_BYTES: usize = 65_536;
pub const MAX_SAMPLE_RATE: u32 = 2_560_000;

#[derive(Debug, Error)]
#[error("Invalid AIRWAV configuration: {0}")]
pub struct ConfigError(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReceiverConfig {
    pub center_hz: u32,
    pub sample_rate: u32,
    /// None selects tuner automatic gain. Otherwise units are tenths of a dB.
    pub gain_tenth_db: Option<i32>,
    pub ppm: i32,
    pub bias_tee: bool,
    pub serial: Option<String>,
}
impl Default for ReceiverConfig {
    fn default() -> Self {
        Self {
            center_hz: 136_000_000,
            sample_rate: MAX_SAMPLE_RATE,
            gain_tenth_db: None,
            ppm: 0,
            bias_tee: false,
            serial: None,
        }
    }
}
impl ReceiverConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(500_000..=1_766_000_000).contains(&self.center_hz) {
            return Err(ConfigError("center_hz must be 500000..=1766000000".into()));
        }
        if !(900_001..=MAX_SAMPLE_RATE).contains(&self.sample_rate) {
            return Err(ConfigError(
                "sample_rate must be 900001..=2560000 Hz".into(),
            ));
        }
        if !(-200..=200).contains(&self.ppm) {
            return Err(ConfigError("ppm must be -200..=200".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub receiver: ReceiverConfig,
    pub queue_blocks: usize,
    pub fft_size: usize,
    pub detection_snr_db: f32,
    pub pre_trigger_seconds: u32,
    pub post_trigger_seconds: u32,
    pub max_ring_mib: u32,
    pub max_recording_bytes: u64,
    pub minimum_free_bytes: u64,
    pub theme: String,
    pub library: Option<PathBuf>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            receiver: ReceiverConfig::default(),
            queue_blocks: 16,
            fft_size: 2048,
            detection_snr_db: 12.0,
            pre_trigger_seconds: 5,
            post_trigger_seconds: 10,
            max_ring_mib: 128,
            max_recording_bytes: 1024 * 1024 * 1024,
            minimum_free_bytes: 512 * 1024 * 1024,
            theme: "Midnight".into(),
            library: None,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.receiver.validate()?;
        let fail = |s: &str| Err(ConfigError(s.into()));
        if !(2..=256).contains(&self.queue_blocks) {
            return fail("queue_blocks must be 2..=256");
        }
        if !(256..=16_384).contains(&self.fft_size) || !self.fft_size.is_power_of_two() {
            return fail("fft_size must be a power of two from 256 through 16384");
        }
        if !self.detection_snr_db.is_finite() || !(3.0..=60.0).contains(&self.detection_snr_db) {
            return fail("detection_snr_db must be finite, 3..=60 dB");
        }
        if self.pre_trigger_seconds > 60 || self.post_trigger_seconds > 60 {
            return fail("pre/post trigger durations must be at most 60 seconds");
        }
        if !(1..=512).contains(&self.max_ring_mib) {
            return fail("max_ring_mib must be 1..=512");
        }
        if self.ring_bytes() > self.max_ring_mib as usize * 1024 * 1024 {
            return fail(
                "pre-trigger duration exceeds max_ring_mib; shorten it or raise the memory limit",
            );
        }
        if self.max_recording_bytes < 1024 * 1024 {
            return fail("max_recording_bytes must be at least 1 MiB");
        }
        if !["Midnight", "Radar", "Arctic", "Ember", "Studio"].contains(&self.theme.as_str()) {
            return fail("theme must be Midnight, Radar, Arctic, Ember, or Studio");
        }
        Ok(())
    }
    pub fn ring_bytes(&self) -> usize {
        self.receiver.sample_rate as usize * 2 * self.pre_trigger_seconds as usize
    }
}

pub fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u64::MAX as u128) as u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub index: u32,
    pub manufacturer: String,
    pub product: String,
    pub serial: String,
}
impl DeviceIdentity {
    pub fn is_v4(&self) -> bool {
        self.manufacturer == "RTLSDRBlog" && self.product == "Blog V4"
    }
}

#[derive(Debug, Clone)]
pub struct IqBlock {
    /// Cumulative sample position at the callback boundary, including application drops.
    pub first_sample: u64,
    /// Host receipt time, not a hardware timestamp.
    pub received_ns: u64,
    pub bytes: Vec<u8>,
}
impl IqBlock {
    pub fn samples(&self) -> u64 {
        (self.bytes.len() / 2) as u64
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub received_samples: u64,
    pub queue_dropped_samples: u64,
    pub malformed_bytes: u64,
    /// RTL-SDR does not expose USB sample loss in normal RF mode.
    pub hardware_lost_samples: Option<u64>,
    pub processed_samples: u64,
    pub discontinuities: u64,
    pub dsp_us: u64,
    pub ring_bytes: u64,
    pub ring_capacity_bytes: u64,
    pub storage_dropped_snapshots: u64,
    #[serde(default)]
    pub storage_dropped_iq_samples: u64,
    #[serde(default)]
    pub island_candidates_omitted: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalIsland {
    pub id: u64,
    pub first_sample: u64,
    pub last_sample: u64,
    pub center_hz: f64,
    pub bandwidth_hz: f64,
    pub peak_dbfs: f32,
    pub snr_db: f32,
    pub observations: u64,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spectrum {
    pub first_sample: u64,
    pub bin_hz: f64,
    pub start_hz: f64,
    pub power_dbfs: Vec<f32>,
    pub noise_dbfs: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestamp_ns: u64,
    pub receiver: ReceiverConfig,
    pub spectrum: Spectrum,
    pub islands: Vec<SignalIsland>,
    pub metrics: Metrics,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_fit_memory_budget() {
        Config::default().validate().unwrap();
    }
    #[test]
    fn rejects_invalid_config() {
        let mut c = Config {
            fft_size: 1000,
            ..Config::default()
        };
        assert!(c.validate().is_err());
        c.fft_size = 2048;
        c.detection_snr_db = f32::NAN;
        assert!(c.validate().is_err());
        c.detection_snr_db = 12.;
        c.max_ring_mib = 1;
        assert!(c.validate().is_err());
        c.max_ring_mib = 128;
        c.theme = "Neon".into();
        assert!(c.validate().is_err());
        c.theme = "Midnight".into();
        c.receiver.ppm = 201;
        assert!(c.validate().is_err());
        c.receiver.ppm = 0;
        c.receiver.center_hz = 100;
        assert!(c.validate().is_err());
        c.receiver.center_hz = 136_000_000;
        c.queue_blocks = 1;
        assert!(c.validate().is_err());
        c.queue_blocks = 16;
        c.pre_trigger_seconds = 61;
        assert!(c.validate().is_err());
        c.pre_trigger_seconds = 5;
        assert!(c.validate().is_ok());
    }
    #[test]
    fn exact_v4_only() {
        for (m, p, want) in [
            ("RTLSDRBlog", "Blog V4", true),
            ("RTLSDRBlog", "Blog V3", false),
            ("RTLSDRBlog", "Blog V4L", false),
            ("Realtek", "Blog V4", false),
        ] {
            assert_eq!(
                DeviceIdentity {
                    index: 0,
                    manufacturer: m.into(),
                    product: p.into(),
                    serial: "1".into()
                }
                .is_v4(),
                want
            );
        }
    }
    #[test]
    fn unknown_config_keys_are_errors() {
        assert!(toml::from_str::<Config>("quue_blocks = 4").is_err());
    }
}
