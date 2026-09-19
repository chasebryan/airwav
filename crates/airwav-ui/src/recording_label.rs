//! Recording elapsed wall-time helpers for the TUI header.
use crate::Ui;
use std::time::Duration;

/// Format a recording duration as `H:MM:SS` or `M:SS` for the status header.
pub(crate) fn format_elapsed(elapsed: Duration) -> String {
    let total = elapsed.as_secs().min(99 * 3600 + 59 * 60 + 59);
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Header label for observing / replay / recording-with-elapsed.
pub(crate) fn mode_status_label(ui: &Ui) -> String {
    if ui.recording {
        let elapsed = ui
            .recording_started
            .map(|t| format_elapsed(t.elapsed()))
            .unwrap_or_else(|| "0:00".into());
        format!("  ● RECORDING {elapsed}")
    } else if ui.replay {
        "  ○ REPLAY".into()
    } else {
        "  ○ OBSERVING".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn format_elapsed_uses_compact_clock() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "0:00");
        assert_eq!(format_elapsed(Duration::from_secs(65)), "1:05");
        assert_eq!(format_elapsed(Duration::from_secs(3723)), "1:02:03");
    }
}
