//! AIRWAV's native terminal presentation. Rendering never touches receiver I/O.
use airwav_core::Snapshot;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
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

#[derive(Clone)]
pub struct Theme {
    pub name: &'static str,
    pub background: Color,
    pub panel: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
    pub accent: Color,
    pub prism: Color,
    pub unknown: Color,
    pub selected: Color,
    pub danger: Color,
    pub waterfall: [Color; 8],
}
impl Theme {
    pub fn named(name: &str, truecolor: bool) -> Self {
        let (name, bg, panel, text, accent) = match name {
            "Radar" => (
                "Radar",
                (6, 16, 21),
                (10, 24, 29),
                (208, 229, 219),
                (74, 208, 166),
            ),
            "Arctic" => (
                "Arctic",
                (222, 230, 239),
                (234, 240, 246),
                (23, 40, 56),
                (8, 100, 167),
            ),
            "Ember" => (
                "Ember",
                (24, 16, 21),
                (34, 22, 28),
                (240, 222, 218),
                (244, 151, 93),
            ),
            "Studio" => (
                "Studio",
                (8, 12, 21),
                (14, 21, 32),
                (243, 247, 252),
                (89, 213, 245),
            ),
            _ => (
                "Midnight",
                (10, 14, 22),
                (15, 22, 32),
                (213, 224, 238),
                (84, 190, 240),
            ),
        };
        let c = |(r, g, b)| color(r, g, b, truecolor);
        Self {
            name,
            background: c(bg),
            panel: c(panel),
            text: c(text),
            muted: c((117, 137, 159)),
            border: c((43, 62, 83)),
            accent: c(accent),
            prism: c((80, 210, 191)),
            unknown: c((230, 181, 98)),
            selected: c((168, 142, 247)),
            danger: c((238, 101, 130)),
            waterfall: [
                c(bg),
                c((16, 29, 49)),
                c((26, 47, 82)),
                c((39, 65, 129)),
                c((54, 105, 176)),
                c((70, 161, 204)),
                c((123, 198, 219)),
                c((203, 168, 245)),
            ],
        }
    }
}
fn color(r: u8, g: u8, b: u8, truecolor: bool) -> Color {
    if truecolor {
        Color::Rgb(r, g, b)
    } else {
        let q = |v: u8| ((v as u16 * 5 + 127) / 255) as u8;
        Color::Indexed(16 + 36 * q(r) + 6 * q(g) + q(b))
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    None,
    Quit,
    Record,
    Capture,
    Screenshot,
    Pause,
    Step,
    PreviousEvent,
    NextEvent,
    Speed(f64),
    Open(View),
    Theme,
    Demo,
    CycleSpeed,
    AudioToggle,
    AudioMode,
    AudioVolume(i8),
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    Evidence,
    Diagnostics,
    Events,
    Help,
}
#[derive(Default, Clone)]
pub struct HitAreas {
    pub spectrum: Rect,
    pub signals: Rect,
    pub buttons: Vec<(Rect, Action)>,
    pub close: Rect,
}
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
    pub audio_active: bool,
    pub audio_mode: usize,
    pub audio_volume: u8,
    pub audio_frequency: Option<u32>,
    pub audio_status: String,
    pub audio_flow: String,
    pub audio_pcm_samples: u64,
    pub audio_rms_dbfs: Option<f32>,
    pub audio_peak_dbfs: Option<f32>,
    pub audio_clipped_samples: u64,
    pub audio_dropped_samples: u64,
    pub audio_discontinuities: u64,
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
    last_click: Option<(u16, u16, Instant)>,
    hover: Option<(u16, u16)>,
    pub focused: usize,
    signal_scroll: usize,
}
impl Ui {
    pub fn new(theme: &str, truecolor: bool, source: &str, replay: bool) -> Self {
        Self {
            snapshot: None,
            history: VecDeque::new(),
            theme: Theme::named(theme, truecolor),
            source: source.into(),
            status: "Waiting for received IQ…".into(),
            recording: false,
            recording_started: None,
            capture_active: false,
            audio_active: false,
            audio_mode: 0,
            audio_volume: 50,
            audio_frequency: None,
            audio_status: "Audio off • A Listen · M Mode · 9/0 Volume".into(),
            audio_flow: String::new(),
            audio_pcm_samples: 0,
            audio_rms_dbfs: None,
            audio_peak_dbfs: None,
            audio_clipped_samples: 0,
            audio_dropped_samples: 0,
            audio_discontinuities: 0,
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
                if self.recording_started.is_none() {
                    self.recording_started = Some(Instant::now());
                }
            }
            (false, false) => {}
        }
    }
    include!("handle.inc");
    fn cycle_theme(&mut self) {
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
    fn select(&mut self, delta: i32) {
        let count = self.snapshot.as_ref().map_or(0, |s| s.islands.len());
        self.selected = (self.selected as i32 + delta)
            .max(0)
            .min(count.saturating_sub(1) as i32) as usize;
    }
}
pub(crate) fn contains(r: Rect, p: (u16, u16)) -> bool {
    r.contains((p.0, p.1).into())
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

mod chrome;
mod draw;
mod recording_label;
mod svg;
pub use draw::draw;
pub use svg::to_svg;

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;
