//! Interactive UI state and recording elapsed helpers.
use crate::theme::Theme;
use crate::{HitAreas, View};
use airwav_core::Snapshot;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Borders},
};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub struct Ui {
    pub snapshot: Option<Snapshot>,
    pub history: VecDeque<Vec<f32>>,
    pub theme: Theme,
    pub source: String,
    pub status: String,
    pub recording: bool,
    /// Wall-clock start of the active recording session; cleared when recording stops.
    pub(crate) recording_started: Option<Instant>,
    pub capture_active: bool,
    pub replay: bool,
    pub paused: bool,
    pub speed: f64,
    pub selected: usize,
    pub view: Option<View>,
    pub demo: bool,
    pub events: Vec<String>,
    pub areas: HitAreas,
    pub zoom: f64,
    pub pan: f64,
    pub(crate) last_click: Option<(u16, u16, Instant)>,
    pub(crate) hover: Option<(u16, u16)>,
    pub focused: usize,
    pub(crate) signal_scroll: usize,
}
impl Ui {
    pub fn new(theme: &str, truecolor: bool, source: &str, replay: bool) -> Self {
        Self {
            snapshot: None,
            history: VecDeque::new(),
            theme: Theme::named(theme, truecolor),
            source: source.into(),
            status: "Waiting for received IQ...".into(),
            recording: false,
            recording_started: None,
            capture_active: false,
            replay,
            paused: false,
            speed: 1.,
            selected: 0,
            view: None,
            demo: false,
            events: vec![],
            areas: HitAreas::default(),
            zoom: 1.,
            pan: 0.5,
            last_click: None,
            hover: None,
            focused: 0,
            signal_scroll: 0,
        }
    }
    pub fn update(&mut self, snapshot: Snapshot) {
        if self
            .snapshot
            .as_ref()
            .is_none_or(|s| s.timestamp_ns != snapshot.timestamp_ns)
        {
            self.history
                .push_front(snapshot.spectrum.power_dbfs.clone());
            self.history.truncate(160);
        }
        if let Some(old) = self
            .snapshot
            .as_ref()
            .and_then(|s| s.islands.get(self.selected))
            && let Some(next) = snapshot.islands.iter().position(|s| s.id == old.id)
        {
            self.selected = next;
        }
        self.selected = self.selected.min(snapshot.islands.len().saturating_sub(1));
        self.snapshot = Some(snapshot);
    }
    /// Track recording state and the wall-clock start used for elapsed display.
    pub fn set_recording(&mut self, active: bool) {
        match (self.recording, active) {
            (false, true) => {
                self.recording = true;
                self.recording_started = Some(Instant::now());
            }
            (true, false) => {
                self.recording = false;
                self.recording_started = None;
            }
            (true, true) => {
                // Keep the existing start time across redraws.
                if self.recording_started.is_none() {
                    self.recording_started = Some(Instant::now());
                }
            }
            (false, false) => {}
        }
    }
    pub(crate) fn cycle_theme(&mut self) {
        let names = ["Midnight", "Radar", "Arctic", "Ember", "Studio"];
        let i = names
            .iter()
            .position(|n| *n == self.theme.name)
            .unwrap_or(0);
        self.theme = Theme::named(
            names[(i + 1) % 5],
            matches!(self.theme.background, Color::Rgb(..)),
        );
    }
    pub(crate) fn select(&mut self, delta: i32) {
        let count = self.snapshot.as_ref().map_or(0, |s| s.islands.len());
        self.selected = (self.selected as i32 + delta)
            .max(0)
            .min(count.saturating_sub(1) as i32) as usize;
    }
}


pub(crate) fn contains(r: Rect, p: (u16, u16)) -> bool {
    r.contains((p.0, p.1).into())
}
/// Format a recording duration as H:MM:SS or M:SS for the status header.
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
pub(crate) fn recording_label(ui: &Ui) -> String {
    if !ui.recording {
        return "  ○ OBSERVING".into();
    }
    let elapsed = ui
        .recording_started
        .map(|t| format_elapsed(t.elapsed()))
        .unwrap_or_else(|| "0:00".into());
    format!("  ● RECORDING {elapsed}")
}
pub(crate) fn panel<'a>(title: impl Into<Line<'a>>, theme: &Theme, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .title_style(Style::default().fg(if focused { theme.accent } else { theme.muted }))
        .style(Style::default().bg(theme.panel).fg(theme.text))
        .border_style(Style::default().fg(if focused { theme.accent } else { theme.border }))
}
