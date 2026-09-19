//! Fail-closed free-space evaluation for `airwav doctor`.
use std::path::Path;

/// Result of comparing available bytes against the configured minimum.
pub struct StorageCheck {
    pub status: &'static str,
    pub detail: String,
}

/// PASS only when `available >= minimum_free_bytes`; otherwise FAIL with guidance.
pub fn evaluate(data: &Path, available: u64, minimum_free_bytes: u64) -> StorageCheck {
    let ok = available >= minimum_free_bytes;
    StorageCheck {
        status: if ok { "PASS" } else { "FAIL" },
        detail: format!(
            "{} • {} MiB available (configured minimum {} MiB); recordings refuse to start below this threshold",
            data.display(),
            available / 1_048_576,
            minimum_free_bytes / 1_048_576
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn passes_when_available_meets_minimum() {
        let check = evaluate(Path::new("/tmp/airwav"), 600 * 1_048_576, 512 * 1_048_576);
        assert_eq!(check.status, "PASS");
        assert!(check.detail.contains("configured minimum 512 MiB"));
        assert!(check.detail.contains("600 MiB available"));
    }

    #[test]
    fn fails_when_available_is_below_minimum() {
        let check = evaluate(Path::new("/tmp/airwav"), 100 * 1_048_576, 512 * 1_048_576);
        assert_eq!(check.status, "FAIL");
        assert!(check.detail.contains("configured minimum 512 MiB"));
        assert!(check.detail.contains("100 MiB available"));
        assert!(check.detail.contains("refuse to start"));
    }

    #[test]
    fn boundary_equality_passes() {
        let check = evaluate(Path::new("/data"), 512 * 1_048_576, 512 * 1_048_576);
        assert_eq!(check.status, "PASS");
    }
}
