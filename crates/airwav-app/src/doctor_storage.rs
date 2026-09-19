//! Doctor storage gate helpers.
use airwav_core::Config;
use std::path::Path;

pub fn storage_status(config: &Config, data: &Path) -> Result<(bool, String), fs2::Error> {
    let available = fs2::available_space(data)?;
    let ok = available >= config.minimum_free_bytes;
    let detail = format!(
        "{} • {} MiB available (configured minimum {} MiB); recordings refuse to start below this threshold",
        data.display(),
        available / 1048576,
        config.minimum_free_bytes / 1048576
    );
    Ok((ok, detail))
}
