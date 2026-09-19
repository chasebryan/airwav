//! AIRWAV's native terminal presentation. Rendering never touches receiver I/O.
mod theme;
use airwav_core::{SignalIsland, Snapshot};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use std::{
    collections::VecDeque,
    fmt::Write as _,
    time::{Duration, Instant},
};
pub use theme::Theme;

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
    Log,
    Settings,
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
    pub focused: usize,
    pub peak_hold: bool,
    pub sort_snr: bool,
    pub hide_fading: bool,
    pub recording_started: Option<Instant>,
    pub quit_armed: Option<Instant>,
    pub logs: VecDeque<String>,
    last_click: Option<(u16, u16, Instant)>,
    hover: Option<(u16, u16)>,
    cursor_hz: Option<f64>,
    cursor_dbfs: Option<f32>,
    signal_scroll: usize,
    overlay_scroll: usize,
    peak: Option<Vec<f32>>,
    drag_start: Option<f64>,
    drag_now: Option<f64>,
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
            focused: 0,
            peak_hold: true,
            sort_snr: false,
            hide_fading: false,
            recording_started: None,
            quit_armed: None,
            logs: VecDeque::new(),
            last_click: None,
            hover: None,
            cursor_hz: None,
            cursor_dbfs: None,
            signal_scroll: 0,
            overlay_scroll: 0,
            peak: None,
            drag_start: None,
            drag_now: None,
        }
    }

    pub fn note(&mut self, line: impl Into<String>) {
        self.logs.push_front(line.into());
        self.logs.truncate(200);
    }

    pub fn update(&mut self, snapshot: Snapshot) {
        if self
            .snapshot
            .as_ref()
            .is_none_or(|s| s.timestamp_ns != snapshot.timestamp_ns)
        {
            self.history
                .push_front(snapshot.spectrum.power_dbfs.clone());
            self.history.truncate(220);
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
        if self.peak_hold {
            if self
                .peak
                .as_ref()
                .is_none_or(|p| p.len() != snapshot.spectrum.power_dbfs.len())
            {
                self.peak = Some(snapshot.spectrum.power_dbfs.clone());
            } else if let Some(peak) = &mut self.peak {
                for (held, value) in peak.iter_mut().zip(&snapshot.spectrum.power_dbfs) {
                    *held = (*held * 0.992).max(*value);
                }
            }
        }
        self.snapshot = Some(snapshot);
        self.clamp_selection();
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

    pub fn handle(&mut self, event: Event) -> Action {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') => Action::Quit,
                KeyCode::Char('a') => Action::AudioToggle,
                KeyCode::Char('m') => Action::AudioMode,
                KeyCode::Char('9') => Action::AudioVolume(-10),
                KeyCode::Char('0') => Action::AudioVolume(10),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
                KeyCode::Esc => {
                    self.view = None;
                    self.overlay_scroll = 0;
                    Action::None
                }
                KeyCode::Char('?') | KeyCode::Char('h') if self.view.is_none() => {
                    self.view = Some(View::Help);
                    self.overlay_scroll = 0;
                    Action::None
                }
                KeyCode::Char('d') => self.open(View::Diagnostics),
                KeyCode::Char('e') => self.open(View::Events),
                KeyCode::Char('g') => self.open(View::Log),
                KeyCode::Char('s') => self.open(View::Settings),
                KeyCode::Char('i') | KeyCode::Enter => self.open(View::Evidence),
                KeyCode::Tab => {
                    self.focused = (self.focused + 1) % 3;
                    Action::None
                }
                KeyCode::BackTab => {
                    self.focused = (self.focused + 2) % 3;
                    Action::None
                }
                KeyCode::Home => {
                    self.jump_visible(isize::MIN);
                    Action::None
                }
                KeyCode::End => {
                    self.jump_visible(isize::MAX);
                    Action::None
                }
                KeyCode::PageDown => {
                    self.jump_visible(8);
                    Action::None
                }
                KeyCode::PageUp => {
                    self.jump_visible(-8);
                    Action::None
                }
                KeyCode::Down => {
                    if self.view.is_some() {
                        self.overlay_scroll = self.overlay_scroll.saturating_add(1);
                    } else {
                        self.jump_visible(1);
                    }
                    Action::None
                }
                KeyCode::Up => {
                    if self.view.is_some() {
                        self.overlay_scroll = self.overlay_scroll.saturating_sub(1);
                    } else {
                        self.jump_visible(-1);
                    }
                    Action::None
                }
                KeyCode::Char('r') if !self.replay => Action::Record,
                KeyCode::Char('c') if !self.replay => Action::Capture,
                KeyCode::Char(' ') => Action::Pause,
                KeyCode::Char('.') if self.replay => Action::Step,
                KeyCode::Char('[') if self.replay => Action::PreviousEvent,
                KeyCode::Char(']') if self.replay => Action::NextEvent,
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    let frac = self.cursor_frac();
                    self.zoom_by(1., frac);
                    Action::None
                }
                KeyCode::Char('-') => {
                    let frac = self.cursor_frac();
                    self.zoom_by(-1., frac);
                    Action::None
                }
                KeyCode::Left if self.focused != 2 => {
                    self.pan = (self.pan - 0.1 / self.zoom).max(0.);
                    Action::None
                }
                KeyCode::Right if self.focused != 2 => {
                    self.pan = (self.pan + 0.1 / self.zoom).min(1.);
                    Action::None
                }
                KeyCode::Left => {
                    self.jump_visible(-1);
                    Action::None
                }
                KeyCode::Right => {
                    self.jump_visible(1);
                    Action::None
                }
                KeyCode::Char(n @ '1'..='5') if self.replay => {
                    Action::Speed([0.25, 0.5, 1., 2., 4.][n as usize - '1' as usize])
                }
                KeyCode::Char('t') => {
                    self.cycle_theme();
                    Action::None
                }
                KeyCode::Char('k') => {
                    self.peak_hold = !self.peak_hold;
                    if !self.peak_hold {
                        self.peak = None;
                    }
                    Action::None
                }
                KeyCode::Char('o') => {
                    self.sort_snr = !self.sort_snr;
                    self.clamp_selection();
                    Action::None
                }
                KeyCode::Char('f') => {
                    self.hide_fading = !self.hide_fading;
                    self.clamp_selection();
                    Action::None
                }
                KeyCode::Char('z') => {
                    self.zoom_to_selected();
                    Action::None
                }
                KeyCode::F(10) => {
                    self.demo = !self.demo;
                    Action::None
                }
                KeyCode::F(12) => Action::Screenshot,
                _ => Action::None,
            },
            Event::Mouse(mouse) => {
                let point = (mouse.column, mouse.row);
                self.hover = Some(point);
                if contains(self.areas.spectrum, point)
                    && let Some(s) = &self.snapshot
                {
                    let (hz, dbfs) = probe(s, self.areas.spectrum, point, self.zoom, self.pan);
                    self.cursor_hz = Some(hz);
                    self.cursor_dbfs = Some(dbfs);
                } else if mouse.kind == MouseEventKind::Moved {
                    self.cursor_hz = None;
                    self.cursor_dbfs = None;
                }
                if self.view.is_some() {
                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left)
                            if contains(self.areas.close, point) =>
                        {
                            self.view = None;
                            self.overlay_scroll = 0;
                        }
                        MouseEventKind::ScrollUp => {
                            self.overlay_scroll = self.overlay_scroll.saturating_sub(1);
                        }
                        MouseEventKind::ScrollDown => {
                            self.overlay_scroll = self.overlay_scroll.saturating_add(1);
                        }
                        _ => {}
                    }
                    return Action::None;
                }
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        for (r, action) in &self.areas.buttons {
                            if contains(*r, point) {
                                self.drag_start = None;
                                self.drag_now = None;
                                return match action.clone() {
                                    Action::Open(view) => self.open(view),
                                    Action::Theme => {
                                        self.cycle_theme();
                                        Action::None
                                    }
                                    Action::Demo => {
                                        self.demo = !self.demo;
                                        Action::None
                                    }
                                    Action::CycleSpeed => {
                                        let speeds = [0.25, 0.5, 1., 2., 4.];
                                        let i = speeds
                                            .iter()
                                            .position(|s| *s == self.speed)
                                            .unwrap_or(2);
                                        Action::Speed(speeds[(i + 1) % 5])
                                    }
                                    other => other,
                                };
                            }
                        }
                        if contains(self.areas.spectrum, point) {
                            let frac = self.spectrum_frac(point.0);
                            self.drag_start = Some(frac);
                            self.drag_now = Some(frac);
                            self.focused = 0;
                        }
                        if contains(self.areas.signals, point)
                            && mouse.row > self.areas.signals.y + 1
                        {
                            let row = (mouse.row - self.areas.signals.y - 2) as usize;
                            let visible = self.visible_indices();
                            if let Some(&idx) = visible.get(self.signal_scroll + row) {
                                self.selected = idx;
                                self.focused = 2;
                                self.pan_to_selected();
                                if self.last_click.is_some_and(|(x, y, t)| {
                                    x == point.0
                                        && y == point.1
                                        && t.elapsed() < Duration::from_millis(400)
                                }) {
                                    return self.open(View::Evidence);
                                }
                                self.last_click = Some((point.0, point.1, Instant::now()));
                            }
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if let Some(start) = self.drag_start {
                            let frac = self.spectrum_frac(point.0);
                            self.drag_start = None;
                            self.drag_now = None;
                            if (frac - start).abs() > 0.04 {
                                self.apply_drag(start, frac);
                            } else {
                                self.select_nearest_to_cursor();
                            }
                            self.focused = 0;
                        }
                    }
                    MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
                        if contains(self.areas.spectrum, point) && self.drag_start.is_some() {
                            self.drag_now = Some(self.spectrum_frac(point.0));
                        }
                    }
                    MouseEventKind::Down(MouseButton::Right) => {
                        if contains(self.areas.signals, point)
                            || contains(self.areas.spectrum, point)
                        {
                            if contains(self.areas.spectrum, point) {
                                self.select_nearest_to_cursor();
                            }
                            return self.open(View::Evidence);
                        }
                    }
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                        let direction = if mouse.kind == MouseEventKind::ScrollUp {
                            1.
                        } else {
                            -1.
                        };
                        if contains(self.areas.spectrum, point) {
                            if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                                self.pan = (self.pan + direction * 0.1 / self.zoom).clamp(0., 1.);
                            } else {
                                let frac = if self.areas.spectrum.width > 0 {
                                    Some(
                                        (point.0.saturating_sub(self.areas.spectrum.x) as f64)
                                            / self.areas.spectrum.width as f64,
                                    )
                                } else {
                                    None
                                };
                                self.zoom_by(direction, frac);
                            }
                            self.focused = 0;
                        } else {
                            self.jump_visible(-direction as isize);
                            self.focused = 2;
                        }
                    }
                    _ => {}
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    fn open(&mut self, view: View) -> Action {
        self.view = Some(view);
        self.overlay_scroll = 0;
        Action::None
    }

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

    fn visible_indices(&self) -> Vec<usize> {
        let Some(s) = &self.snapshot else {
            return Vec::new();
        };
        let mut idx: Vec<usize> = s
            .islands
            .iter()
            .enumerate()
            .filter(|(_, island)| !self.hide_fading || activity(&island.state) != "FADING")
            .map(|(i, _)| i)
            .collect();
        if self.sort_snr {
            idx.sort_by(|&a, &b| {
                s.islands[b]
                    .snr_db
                    .total_cmp(&s.islands[a].snr_db)
                    .then_with(|| s.islands[a].center_hz.total_cmp(&s.islands[b].center_hz))
            });
        }
        idx
    }

    fn clamp_selection(&mut self) {
        let visible = self.visible_indices();
        if visible.is_empty() {
            self.selected = 0;
            self.signal_scroll = 0;
            return;
        }
        if !visible.contains(&self.selected) {
            self.selected = visible[0];
        }
    }

    fn jump_visible(&mut self, delta: isize) {
        let visible = self.visible_indices();
        if visible.is_empty() {
            return;
        }
        let current = visible
            .iter()
            .position(|i| *i == self.selected)
            .unwrap_or(0);
        let next = if delta == isize::MIN {
            0
        } else if delta == isize::MAX {
            visible.len() - 1
        } else {
            current.saturating_add_signed(delta).min(visible.len() - 1)
        };
        self.selected = visible[next];
        self.pan_to_selected();
    }

    fn zoom_by(&mut self, direction: f64, frac: Option<f64>) {
        let Some(s) = &self.snapshot else {
            self.zoom = (self.zoom * 2f64.powf(direction)).clamp(1., 16.);
            return;
        };
        let len = s.spectrum.power_dbfs.len() as f64;
        let (a, b) = range(self.zoom, self.pan, s.spectrum.power_dbfs.len());
        let frac = frac.unwrap_or(0.5).clamp(0., 1.);
        let bin = a as f64 + frac * (b - a) as f64;
        self.zoom = (self.zoom * 2f64.powf(direction)).clamp(1., 16.);
        let width = (len / self.zoom).max(1.);
        let start = bin - frac * width;
        self.pan = ((start + width / 2.) / len).clamp(0., 1.);
    }

    fn zoom_to_selected(&mut self) {
        let Some(s) = &self.snapshot else {
            return;
        };
        let Some(island) = s.islands.get(self.selected) else {
            return;
        };
        let span = s.spectrum.bin_hz * s.spectrum.power_dbfs.len() as f64;
        if span <= 0. {
            return;
        };
        let window = (island.bandwidth_hz * 8.).max(span / 16.).min(span);
        self.zoom = (span / window).clamp(1., 16.);
        self.pan = ((island.center_hz - s.spectrum.start_hz) / span).clamp(0., 1.);
    }

    fn spectrum_frac(&self, col: u16) -> f64 {
        if self.areas.spectrum.width == 0 {
            0.5
        } else {
            (col.saturating_sub(self.areas.spectrum.x) as f64 / self.areas.spectrum.width as f64)
                .clamp(0., 1.)
        }
    }

    fn cursor_frac(&self) -> Option<f64> {
        let s = self.snapshot.as_ref()?;
        let hz = self.cursor_hz?;
        let (a, b) = range(self.zoom, self.pan, s.spectrum.power_dbfs.len());
        let start = s.spectrum.start_hz + a as f64 * s.spectrum.bin_hz;
        let end = s.spectrum.start_hz + b as f64 * s.spectrum.bin_hz;
        let span = end - start;
        if span <= 0. {
            return None;
        }
        Some(((hz - start) / span).clamp(0., 1.))
    }

    fn apply_drag(&mut self, start: f64, end: f64) {
        let a = start.min(end);
        let b = start.max(end);
        let width = (b - a).max(0.04);
        let Some(s) = &self.snapshot else {
            self.zoom = (1. / width).min(16.);
            self.pan = (a + b) / 2.;
            return;
        };
        let n = s.spectrum.power_dbfs.len() as f64;
        let (from, to) = range(self.zoom, self.pan, s.spectrum.power_dbfs.len());
        let view = (to - from) as f64;
        let start_bin = from as f64 + a * view;
        let end_bin = from as f64 + b * view;
        let new_width = (end_bin - start_bin).max(1.);
        self.zoom = (n / new_width).clamp(1., 16.);
        self.pan = ((start_bin + end_bin) / 2. / n).clamp(0., 1.);
    }

    fn select_nearest_to_cursor(&mut self) {
        let Some(hz) = self.cursor_hz else {
            return;
        };
        let Some(s) = &self.snapshot else {
            return;
        };
        let visible = self.visible_indices();
        let best = visible.into_iter().min_by(|&a, &b| {
            (s.islands[a].center_hz - hz)
                .abs()
                .total_cmp(&(s.islands[b].center_hz - hz).abs())
        });
        if let Some(idx) = best {
            self.selected = idx;
            self.pan_to_selected();
        }
    }

    fn pan_to_selected(&mut self) {
        let Some(s) = &self.snapshot else {
            return;
        };
        let Some(island) = s.islands.get(self.selected) else {
            return;
        };
        let (a, b) = range(self.zoom, self.pan, s.spectrum.power_dbfs.len());
        let bin = (island.center_hz - s.spectrum.start_hz) / s.spectrum.bin_hz;
        if bin < a as f64 || bin >= b as f64 {
            let n = s.spectrum.power_dbfs.len() as f64;
            if n > 0. {
                self.pan = (bin / n).clamp(0., 1.);
            }
        }
    }
}

fn activity(state: &str) -> &'static str {
    if state.starts_with("FADING") {
        "FADING"
    } else {
        "LIVE"
    }
}

fn contains(r: Rect, p: (u16, u16)) -> bool {
    r.contains((p.0, p.1).into())
}

fn range(zoom: f64, pan: f64, len: usize) -> (usize, usize) {
    let width = (len as f64 / zoom) as usize;
    let width = width.max(1).min(len);
    let start = ((pan * len as f64) as usize)
        .saturating_sub(width / 2)
        .min(len.saturating_sub(width));
    (start, start + width)
}

fn probe(s: &Snapshot, area: Rect, point: (u16, u16), zoom: f64, pan: f64) -> (f64, f32) {
    let (a, b) = range(zoom, pan, s.spectrum.power_dbfs.len());
    let frac = if area.width == 0 {
        0.
    } else {
        (point.0.saturating_sub(area.x) as f64 / area.width as f64).clamp(0., 0.999)
    };
    let bin = a as f64 + frac * (b - a) as f64;
    let from = bin as usize;
    let to = (from + 1)
        .max(a + 1)
        .min(b)
        .min(s.spectrum.power_dbfs.len());
    let from = from.min(s.spectrum.power_dbfs.len().saturating_sub(1));
    let dbfs = s.spectrum.power_dbfs[from..to]
        .iter()
        .copied()
        .fold(-160., f32::max);
    (s.spectrum.start_hz + bin * s.spectrum.bin_hz, dbfs)
}

fn panel<'a>(title: impl Into<Line<'a>>, theme: &Theme, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .title_style(Style::default().fg(if focused { theme.accent } else { theme.muted }))
        .style(Style::default().bg(theme.panel).fg(theme.text))
        .border_style(Style::default().fg(if focused { theme.accent } else { theme.border }))
}

fn bar(filled: f32, width: usize) -> String {
    let n = ((filled.clamp(0., 1.) * width as f32).round() as usize).min(width);
    format!("{}{}", "█".repeat(n), "░".repeat(width.saturating_sub(n)))
}

fn dwell(signal: &SignalIsland, rate: u32) -> String {
    if rate == 0 {
        return "—".into();
    }
    let samples = signal.last_sample.saturating_sub(signal.first_sample);
    let seconds = samples as f64 / rate as f64;
    if seconds < 1. {
        format!("{} obs", signal.observations)
    } else if seconds < 60. {
        format!("{seconds:.1}s")
    } else {
        format!("{:.0}m", seconds / 60.)
    }
}

pub fn draw(frame: &mut Frame, ui: &mut Ui) {
    let area = frame.area();
    let t = ui.theme.clone();
    frame.render_widget(
        Block::default().style(Style::default().bg(t.background).fg(t.text)),
        area,
    );
    if area.width < 60 || area.height < 16 {
        frame.render_widget(
            Paragraph::new(
                "AIRWAV\n\nResize to at least 60 × 16 cells.\nCapture continues independently.\n\nQ quit · ? help",
            )
            .style(Style::default().fg(t.accent)),
            area,
        );
        return;
    }
    let compact = area.width < 92 || area.height < 24;
    let header_h = 4;
    let command_h = 2;
    let footer_h = 1;
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Min(8),
            Constraint::Length(command_h),
            Constraint::Length(footer_h),
        ])
        .split(area);
    header(frame, main[0], ui, &t, compact);
    stage(frame, main[1], ui, &t, compact);
    commands(frame, main[2], ui, &t);
    footer(frame, main[3], ui, &t);
    if let Some(view) = ui.view {
        overlay(frame, ui, view, &t);
    }
}

fn header(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme, compact: bool) {
    let source_color = if ui.source.contains("FIXTURE") {
        t.unknown
    } else {
        t.prism
    };
    let (mhz, rate, usb, ring) = if let Some(s) = &ui.snapshot {
        let fill = if s.metrics.ring_capacity_bytes == 0 {
            0.
        } else {
            s.metrics.ring_bytes as f32 / s.metrics.ring_capacity_bytes as f32
        };
        (
            format!("{:>10.6} MHz", s.receiver.center_hz as f64 / 1e6),
            format!("{:.2} MS/s", s.receiver.sample_rate as f64 / 1e6),
            match s.metrics.hardware_lost_samples {
                Some(n) => format!("USB lost {n}"),
                None => "USB n/a".into(),
            },
            format!("RING {} {:>3.0}%", bar(fill, 8), fill * 100.),
        )
    } else {
        (
            "—".into(),
            "—".into(),
            "USB n/a".into(),
            "RING ░░░░░░░░".into(),
        )
    };
    let mode = ["AM", "FM", "NFM"][ui.audio_mode % 3];
    let rec = if ui.recording {
        let elapsed = ui
            .recording_started
            .map(|t0| t0.elapsed().as_secs_f32())
            .unwrap_or(0.);
        format!("REC {elapsed:05.1}s")
    } else if ui.replay {
        format!("REPLAY {:.2}×", ui.speed)
    } else {
        "OBSERVING".into()
    };
    let vu = match ui.audio_rms_dbfs {
        Some(rms) if ui.audio_active => {
            let fill = ((rms + 60.) / 60.).clamp(0., 1.);
            format!("VU {} {rms:.0}", bar(fill, 10))
        }
        _ if ui.audio_active => "VU ░░░░░░░░░░".into(),
        _ => String::new(),
    };
    let lock = match (ui.audio_active, ui.audio_frequency) {
        (true, Some(hz)) => {
            let selected = ui
                .snapshot
                .as_ref()
                .and_then(|s| s.islands.get(ui.selected))
                .map(|i| i.center_hz.round() as u32);
            if selected.is_some_and(|sel| sel != hz) {
                format!("LOCK {:.6} MHz ≠ sel", hz as f64 / 1e6)
            } else {
                format!("LOCK {:.6} MHz", hz as f64 / 1e6)
            }
        }
        _ => String::new(),
    };
    let cursor = match (ui.cursor_hz, ui.cursor_dbfs) {
        (Some(hz), Some(db)) => format!("{:.6} MHz  {db:.1} dBFS", hz / 1e6),
        _ => String::new(),
    };
    let hardware = if ui.source.contains("FIXTURE") {
        "SYNTHETIC IQ · not a receiver"
    } else {
        "RTL-SDR BLOG V4"
    };
    let line1 = Line::from(vec![
        Span::styled(
            "  AIRWAV",
            Style::default().fg(t.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  /  RF OBSERVATION", Style::default().fg(t.muted)),
        Span::styled(
            format!("    {}", ui.source),
            Style::default().fg(source_color),
        ),
        Span::styled(format!("    {}", t.name), Style::default().fg(t.muted)),
    ]);
    let line2 = if compact {
        Line::styled(
            format!("  {hardware}   {mhz}   {rate}   {rec}"),
            Style::default().fg(t.muted),
        )
    } else {
        Line::from(vec![
            Span::styled(
                format!("  {hardware}   {mhz}   {rate}   "),
                Style::default().fg(t.muted),
            ),
            Span::styled(ring, Style::default().fg(t.prism)),
            Span::styled(format!("   {usb}"), Style::default().fg(t.muted)),
            Span::styled(
                if cursor.is_empty() {
                    String::new()
                } else {
                    format!("   {cursor}")
                },
                Style::default().fg(t.accent),
            ),
        ])
    };
    let rec_style = if ui.recording {
        Style::default().fg(t.danger).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(t.muted)
    };
    let mut flags = vec![Span::styled(format!("  {rec}"), rec_style)];
    flags.push(Span::styled(
        if ui.capture_active {
            "   IQ COLLECT"
        } else {
            "   RX ONLY"
        },
        Style::default().fg(if ui.capture_active { t.prism } else { t.muted }),
    ));
    if ui.paused {
        flags.push(Span::styled(
            "   VIEW PAUSED",
            Style::default().fg(t.unknown),
        ));
    }
    if ui.demo {
        flags.push(Span::styled("   DEMO VIEW", Style::default().fg(t.accent)));
    }
    if let Some(s) = &ui.snapshot {
        let live = s
            .islands
            .iter()
            .filter(|i| activity(&i.state) == "LIVE")
            .count();
        let fading = s.islands.len().saturating_sub(live);
        flags.push(Span::styled(
            format!("   {live} LIVE"),
            Style::default().fg(t.live),
        ));
        flags.push(Span::styled(
            format!("   {fading} FADING"),
            Style::default().fg(t.muted),
        ));
        flags.push(Span::styled("   UNK", Style::default().fg(t.unknown)));
    }
    let audio_line = Line::from(vec![
        Span::styled(
            format!("  {}  VOL {}%", mode, ui.audio_volume),
            Style::default().fg(if ui.audio_active { t.prism } else { t.unknown }),
        ),
        Span::styled(
            if ui.audio_flow.is_empty() {
                String::new()
            } else {
                format!("   {}", ui.audio_flow)
            },
            Style::default().fg(t.unknown),
        ),
        Span::styled(
            if lock.is_empty() {
                String::new()
            } else {
                format!("   {lock}")
            },
            Style::default().fg(t.prism),
        ),
        Span::styled(
            if vu.is_empty() {
                String::new()
            } else {
                format!("   {vu}")
            },
            Style::default().fg(t.accent),
        ),
        Span::styled(
            format!("   {}", ui.audio_status),
            Style::default().fg(if ui.audio_active { t.prism } else { t.muted }),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(vec![line1, line2, Line::from(flags), audio_line]),
        area,
    );
}

fn stage(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme, compact: bool) {
    let show_side = area.width >= 88 && !ui.demo;
    let show_waterfall = area.height >= 16;
    if show_side {
        let cols = Layout::horizontal([
            Constraint::Percentage(if ui.demo { 70 } else { 64 }),
            Constraint::Min(30),
        ])
        .split(area);
        let left = if show_waterfall {
            Layout::vertical([Constraint::Percentage(46), Constraint::Percentage(54)])
                .split(cols[0])
        } else {
            Layout::vertical([Constraint::Min(6)]).split(cols[0])
        };
        let right = Layout::vertical([Constraint::Percentage(56), Constraint::Percentage(44)])
            .split(cols[1]);
        spectrum(frame, left[0], ui, t);
        if show_waterfall {
            waterfall(frame, left[1], ui, t);
        }
        signals(frame, right[0], ui, t);
        evidence(frame, right[1], ui, t);
    } else {
        let rows = if show_waterfall && !compact {
            Layout::vertical([
                Constraint::Percentage(40),
                Constraint::Percentage(32),
                Constraint::Min(6),
            ])
            .split(area)
        } else {
            Layout::vertical([Constraint::Percentage(58), Constraint::Min(6)]).split(area)
        };
        spectrum(frame, rows[0], ui, t);
        if show_waterfall && !compact && rows.len() > 2 {
            waterfall(frame, rows[1], ui, t);
            signals(frame, rows[2], ui, t);
        } else {
            signals(frame, rows[1], ui, t);
        }
    }
}

fn spectrum(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme) {
    let hold = if ui.peak_hold { " · HOLD" } else { "" };
    let block = panel(format!(" 01 / SPECTRUM  dBFS{hold} "), t, ui.focused == 0);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(s) = &ui.snapshot else {
        frame.render_widget(Paragraph::new("\n  No received or replayed IQ."), inner);
        ui.areas.spectrum = inner;
        return;
    };
    if inner.height < 3 || inner.width < 8 {
        ui.areas.spectrum = inner;
        return;
    }
    let scale = 4u16;
    let plot = Rect::new(
        inner.x + scale,
        inner.y,
        inner.width.saturating_sub(scale),
        inner.height.saturating_sub(2),
    );
    ui.areas.spectrum = plot;
    let (a, b) = range(ui.zoom, ui.pan, s.spectrum.power_dbfs.len());
    let ceiling = s.spectrum.power_dbfs[a..b]
        .iter()
        .copied()
        .fold(-40., f32::max)
        .ceil()
        + 5.;
    let floor = (s.spectrum.noise_dbfs - 10.).max(-140.);
    let span = (ceiling - floor).max(20.);
    let height = plot.height;
    for y in 0..height {
        let db = ceiling - span * (y as f32 + 0.5) / height as f32;
        if y % 2 == 0 {
            frame.buffer_mut()[(inner.x, inner.y + y)]
                .set_symbol(&format!("{db:>4.0}"))
                .set_fg(t.muted);
        }
    }
    let blocks = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    for x in 0..plot.width {
        let from = a + x as usize * (b - a) / plot.width.max(1) as usize;
        let to = (a + (x as usize + 1) * (b - a) / plot.width.max(1) as usize)
            .max(from + 1)
            .min(b);
        let value = s.spectrum.power_dbfs[from..to]
            .iter()
            .copied()
            .fold(-160., f32::max);
        let held = ui.peak.as_ref().and_then(|p| {
            p.get(from..to.min(p.len()))
                .map(|slice| slice.iter().copied().fold(-160., f32::max))
        });
        let bars =
            ((value - floor) / span * height as f32 * 8.).clamp(0., height as f32 * 8.) as usize;
        let color_i = (((value - floor) / span * 7.).clamp(0., 7.)) as usize;
        for y in 0..height {
            let level = bars.saturating_sub(y as usize * 8).min(8);
            let cell = &mut frame.buffer_mut()[(plot.x + x, plot.y + height - 1 - y)];
            cell.set_symbol(blocks[level]).set_fg(t.waterfall[color_i]);
        }
        if ui.peak_hold
            && let Some(held) = held
        {
            let hy = ((held - floor) / span * height as f32).clamp(0., height as f32 - 1.) as u16;
            let cell = &mut frame.buffer_mut()[(plot.x + x, plot.y + height - 1 - hy)];
            if cell.symbol() == " " {
                cell.set_symbol("·").set_fg(t.muted);
            }
        }
    }
    if let (Some(start), Some(now)) = (ui.drag_start, ui.drag_now) {
        let x0 = plot.x + (start.min(now) * plot.width as f64) as u16;
        let x1 = plot.x + (start.max(now) * plot.width as f64) as u16;
        for x in x0..=x1.min(plot.right().saturating_sub(1)) {
            let cell = &mut frame.buffer_mut()[(x, plot.y)];
            cell.set_fg(t.accent);
        }
    }
    let noise_y = ((ui.snapshot.as_ref().unwrap().spectrum.noise_dbfs - floor) / span
        * height as f32)
        .clamp(0., height as f32 - 1.) as u16;
    for x in (0..plot.width).step_by(2) {
        let cell = &mut frame.buffer_mut()[(plot.x + x, plot.y + height - 1 - noise_y)];
        if cell.symbol() == " " {
            cell.set_symbol("·").set_fg(t.border);
        }
    }
    if let Some(s) = &ui.snapshot {
        let (a, b) = range(ui.zoom, ui.pan, s.spectrum.power_dbfs.len());
        for (i, signal) in s.islands.iter().enumerate() {
            let bin = (signal.center_hz - s.spectrum.start_hz) / s.spectrum.bin_hz;
            if bin >= a as f64 && bin < b as f64 && plot.height > 0 {
                let x =
                    plot.x + ((bin - a as f64) / (b - a).max(1) as f64 * plot.width as f64) as u16;
                let x = x.min(plot.right().saturating_sub(1));
                let fading = activity(&signal.state) == "FADING";
                let fg = if i == ui.selected {
                    t.selected
                } else if fading {
                    t.muted
                } else {
                    t.live
                };
                frame.buffer_mut()[(x, plot.y)]
                    .set_symbol(if i == ui.selected { "▼" } else { "▴" })
                    .set_fg(fg);
                if i == ui.selected {
                    for y in 1..plot.height {
                        let cell = &mut frame.buffer_mut()[(x, plot.y + y)];
                        if cell.symbol() == " " {
                            cell.set_symbol("│").set_fg(t.selected);
                        }
                    }
                }
            }
        }
    }
    let from = (s.spectrum.start_hz + a as f64 * s.spectrum.bin_hz) / 1e6;
    let to = (s.spectrum.start_hz + b as f64 * s.spectrum.bin_hz) / 1e6;
    let axis = Rect::new(inner.x, inner.y + plot.height, inner.width, 2);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                format!(
                    " {from:.3} MHz   ──   {:.3} MHz   ──   {to:.3}",
                    (from + to) / 2.
                ),
                Style::default().fg(t.muted),
            ),
            Line::styled(
                format!(
                    " floor {:.1} dBFS · {:.1} kHz/bin · zoom {:.0}× · noise · hold {}",
                    s.spectrum.noise_dbfs,
                    s.spectrum.bin_hz / 1000.,
                    ui.zoom,
                    if ui.peak_hold { "on" } else { "off" }
                ),
                Style::default().fg(t.muted),
            ),
        ]),
        axis,
    );
}

fn waterfall(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
    let seconds = (ui.history.len() as f64 / 10.).min(22.);
    let block = panel(
        format!(" 02 / WATERFALL  measured {seconds:.0}s "),
        t,
        ui.focused == 1,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let floor = ui
        .snapshot
        .as_ref()
        .map_or(-100., |s| s.spectrum.noise_dbfs - 4.);
    let rows = inner.height as usize;
    let hist = ui.history.len().max(1);
    for y in 0..rows {
        let src = if hist <= rows { y } else { y * hist / rows };
        let Some(data) = ui.history.get(src) else {
            continue;
        };
        let (a, b) = range(ui.zoom, ui.pan, data.len());
        for x in 0..inner.width {
            let from = a + x as usize * (b - a) / inner.width.max(1) as usize;
            let to = (a + (x as usize + 1) * (b - a) / inner.width.max(1) as usize)
                .max(from + 1)
                .min(b)
                .min(data.len());
            if from >= data.len() {
                continue;
            }
            let power = data[from..to].iter().copied().fold(-160., f32::max);
            let index = (((power - floor) / 50. * 7.).clamp(0., 7.)) as usize;
            frame.buffer_mut()[(inner.x + x, inner.y + y as u16)]
                .set_symbol(" ")
                .set_bg(t.waterfall[index]);
        }
    }
    if let Some(s) = &ui.snapshot {
        let (a, b) = range(ui.zoom, ui.pan, s.spectrum.power_dbfs.len());
        for (i, signal) in s.islands.iter().enumerate() {
            let bin = (signal.center_hz - s.spectrum.start_hz) / s.spectrum.bin_hz;
            if bin >= a as f64 && bin < b as f64 && inner.height > 0 {
                let x = inner.x
                    + ((bin - a as f64) / (b - a).max(1) as f64 * inner.width as f64) as u16;
                frame.buffer_mut()[(x.min(inner.right().saturating_sub(1)), inner.y)]
                    .set_symbol("▼")
                    .set_fg(if i == ui.selected {
                        t.selected
                    } else if activity(&signal.state) == "FADING" {
                        t.muted
                    } else {
                        t.live
                    });
            }
        }
    }
}

fn signals(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme) {
    let count = ui.snapshot.as_ref().map_or(0, |s| s.islands.len());
    let order = if ui.sort_snr { "SNR" } else { "FREQ" };
    let block = panel(
        format!(" 03 / ISLANDS  {count}/256  {order} "),
        t,
        ui.focused == 2,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    ui.areas.signals = inner;
    let mut lines = vec![Line::styled(
        "  MHz          kHz    SNR    STATE   PROT",
        Style::default().fg(t.muted),
    )];
    let visible = ui.visible_indices();
    let rows = inner.height.saturating_sub(1) as usize;
    if !visible.is_empty() {
        let pos = visible.iter().position(|i| *i == ui.selected).unwrap_or(0);
        ui.signal_scroll = ui
            .signal_scroll
            .min(pos)
            .max(pos.saturating_sub(rows.saturating_sub(1)));
    } else {
        ui.signal_scroll = 0;
    }
    if let Some(s) = &ui.snapshot {
        for &i in visible.iter().skip(ui.signal_scroll).take(rows) {
            let signal = &s.islands[i];
            let act = activity(&signal.state);
            let marker = if i == ui.selected { "▌" } else { " " };
            let line = format!(
                "{marker}{:>10.6}  {:>5.1}  {:>4.1}  {:<6}  UNK",
                signal.center_hz / 1e6,
                signal.bandwidth_hz / 1000.,
                signal.snr_db,
                act
            );
            let fg = if i == ui.selected {
                t.selected
            } else if act == "FADING" {
                t.muted
            } else {
                t.live
            };
            lines.push(Line::styled(line, Style::default().fg(fg)));
        }
        if visible.is_empty() {
            lines.push(Line::styled(
                "  No activity above threshold",
                Style::default().fg(t.muted),
            ));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn evidence_lines(ui: &Ui) -> Vec<String> {
    let mut lines = vec![];
    if let Some(s) = &ui.snapshot {
        if let Some(signal) = s.islands.get(ui.selected) {
            let act = activity(&signal.state);
            let listen_warn = if ui.audio_active
                && ui
                    .audio_frequency
                    .is_some_and(|hz| hz != signal.center_hz.round() as u32)
            {
                "Audio is locked to another frequency. Mute, then Listen."
            } else if ui.audio_active && act == "FADING" {
                "Listening to a FADING island — the carrier may already be gone."
            } else {
                ""
            };
            lines.extend([
                format!("SIGNAL ISLAND {:04}", signal.id),
                String::new(),
                format!("Frequency     {:.6} MHz", signal.center_hz / 1e6),
                format!("Bandwidth     {:.2} kHz", signal.bandwidth_hz / 1000.),
                format!("Peak          {:.1} dBFS", signal.peak_dbfs),
                format!(
                    "SNR           {:.1} dB  (measurement, not confidence)",
                    signal.snr_db
                ),
                format!("Observations  {}", signal.observations),
                format!("Dwell         {}", dwell(signal, s.receiver.sample_rate)),
                format!("Activity      {act}"),
                String::new(),
                "Protocol      UNKNOWN".into(),
                "Confidence    not established".into(),
                "Evidence      FFT power / local noise".into(),
                "No frame or identity decoded.".into(),
            ]);
            if !listen_warn.is_empty() {
                lines.push(String::new());
                lines.push(listen_warn.into());
            }
        } else {
            lines.extend([
                "OBSERVE FIRST. CONCLUDE SECOND.".into(),
                String::new(),
                "Select a measured signal to inspect it.".into(),
                "No protocol has been established.".into(),
            ]);
        }
    }
    lines
}

fn evidence(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
    let block = panel(" 04 / EVIDENCE ", t, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(evidence_lines(ui).join("\n"))
            .style(Style::default().fg(t.text))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn commands(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme) {
    let specs = if ui.replay {
        vec![
            ("Pause", Action::Pause),
            ("Step", Action::Step),
            ("Prev", Action::PreviousEvent),
            ("Next", Action::NextEvent),
            (
                if ui.audio_active { "Mute" } else { "Listen" },
                Action::AudioToggle,
            ),
            ("Mode", Action::AudioMode),
            ("Inspect", Action::Open(View::Evidence)),
            ("Diag", Action::Open(View::Diagnostics)),
            ("Save", Action::Screenshot),
            ("Quit", Action::Quit),
        ]
    } else {
        vec![
            (if ui.recording { "Stop" } else { "Record" }, Action::Record),
            ("Capture", Action::Capture),
            ("Pause", Action::Pause),
            (
                if ui.audio_active { "Mute" } else { "Listen" },
                Action::AudioToggle,
            ),
            ("Mode", Action::AudioMode),
            ("Inspect", Action::Open(View::Evidence)),
            ("Diag", Action::Open(View::Diagnostics)),
            ("Save", Action::Screenshot),
            ("Quit", Action::Quit),
        ]
    };
    let constraints: Vec<_> = specs
        .iter()
        .map(|_| Constraint::Ratio(1, specs.len() as u32))
        .collect();
    let layout = Layout::horizontal(constraints).split(Rect::new(area.x, area.y, area.width, 1));
    ui.areas.buttons.clear();
    for ((label, action), r) in specs.into_iter().zip(layout.iter()) {
        let hovered = ui.hover.is_some_and(|p| contains(*r, p));
        let rec = matches!(action, Action::Record) && ui.recording;
        let fg = if rec {
            t.danger
        } else if hovered {
            t.text
        } else {
            t.accent
        };
        frame.render_widget(
            Paragraph::new(format!(" {label} ")).centered().style(
                Style::default()
                    .fg(fg)
                    .bg(if hovered { t.border } else { t.panel }),
            ),
            *r,
        );
        ui.areas.buttons.push((*r, action));
    }
    let mut secondary = vec![
        ("Events", Action::Open(View::Events)),
        ("Log", Action::Open(View::Log)),
        ("Settings", Action::Open(View::Settings)),
        ("Vol-", Action::AudioVolume(-10)),
        ("Vol+", Action::AudioVolume(10)),
        ("Theme", Action::Theme),
        ("Demo", Action::Demo),
        ("Help", Action::Open(View::Help)),
    ];
    if ui.replay {
        secondary.insert(0, ("Speed", Action::CycleSpeed));
    }
    let constraints: Vec<_> = secondary
        .iter()
        .map(|_| Constraint::Ratio(1, secondary.len() as u32))
        .collect();
    let layout =
        Layout::horizontal(constraints).split(Rect::new(area.x, area.y + 1, area.width, 1));
    for ((label, action), r) in secondary.into_iter().zip(layout.iter()) {
        let hovered = ui.hover.is_some_and(|p| contains(*r, p));
        frame.render_widget(
            Paragraph::new(format!("[{label}]"))
                .centered()
                .style(Style::default().fg(if hovered { t.text } else { t.muted })),
            *r,
        );
        ui.areas.buttons.push((*r, action));
    }
}

fn footer(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("  {}   ", ui.status), Style::default().fg(t.text)),
            Span::styled(
                "? help  ↑↓ select  click spectrum  Z zoom-to  K hold  O sort  F hide-fading  Q quit",
                Style::default().fg(t.muted),
            ),
        ])),
        area,
    );
}

fn overlay(frame: &mut Frame, ui: &mut Ui, view: View, t: &Theme) {
    let area = frame.area();
    let width = area.width.min(92).saturating_sub(4);
    let height = area.height.min(34).saturating_sub(2);
    let rect = Rect::new(
        (area.width - width) / 2,
        (area.height - height) / 2,
        width,
        height,
    );
    let (title, mut lines) = match view {
        View::Help => (
            " AIRWAV / HELP ",
            vec![
                "OBSERVE FIRST. CONCLUDE SECOND.".into(),
                "".into(),
                "↑/↓ select · Home/End/PgUp/PgDn · Enter/I evidence · D diagnostics".into(),
                "E events · G log · S settings · click a spectrum bin to select nearest island"
                    .into(),
                "R recording · C pre/post-trigger IQ · Space pause presentation".into(),
                "A Listen/Mute · M AM/FM/NFM · 9/0 volume · audio locks the selected island".into(),
                "Mute then Listen to change frequency. Mode is manual.".into(),
                "View pause/speed do not pause audio. PCM level is player input, not the speaker."
                    .into(),
                "Replay: 1–5 = 0.25× / 0.5× / 1× / 2× / 4× · . step · [ / ] events".into(),
                "+/− zoom · wheel zooms toward cursor · Shift+wheel pan · Z zoom to island".into(),
                "K peak hold · O sort SNR/frequency · F hide fading islands".into(),
                "T theme · Tab focus · F10 demo · F12 SVG · Esc close · Q quit".into(),
                "Recording: Q asks once more before finalizing.".into(),
                "".into(),
                "LIVE is current activity. FADING is hysteresis — the carrier is no longer".into(),
                "above threshold, but the island is held for one second of sample clock.".into(),
                "UNKNOWN is the protocol. SNR is a measurement, not identity confidence.".into(),
                "A capture preserves available ring IQ and the configured post-roll.".into(),
                "USB sample loss cannot be measured in normal librtlsdr RF mode.".into(),
                "PRISM, MAX-I and decoders are future milestones; nothing is fabricated.".into(),
            ],
        ),
        View::Evidence => (" AIRWAV / EVIDENCE ", evidence_lines(ui)),
        View::Events => (
            " AIRWAV / EVENTS ",
            if ui.events.is_empty() {
                vec!["No event captures in this session.".into()]
            } else {
                ui.events.iter().rev().cloned().collect()
            },
        ),
        View::Log => (
            " AIRWAV / LOG ",
            if ui.logs.is_empty() {
                vec!["No session notes yet.".into()]
            } else {
                ui.logs.iter().cloned().collect()
            },
        ),
        View::Settings => (
            " AIRWAV / SETTINGS ",
            vec![
                format!("Theme          {}   (T cycles)", ui.theme.name),
                format!(
                    "Peak hold      {}   (K)",
                    if ui.peak_hold { "on" } else { "off" }
                ),
                format!(
                    "Island order   {}   (O)",
                    if ui.sort_snr { "SNR" } else { "frequency" }
                ),
                format!(
                    "Hide fading    {}   (F)",
                    if ui.hide_fading { "yes" } else { "no" }
                ),
                format!("Zoom           {:.0}×   (Z zoom-to-selected)", ui.zoom),
                "".into(),
                "Center, gain, FFT size and detection SNR are configuration,".into(),
                "not live retune. This milestone does not retune while streaming.".into(),
            ],
        ),
        View::Diagnostics => {
            let mut l = vec![
                format!("Source: {}", ui.source),
                format!("Audio: {}", ui.audio_status),
                format!(
                    "Audio flow: {}",
                    if ui.audio_flow.is_empty() {
                        "Stopped"
                    } else {
                        &ui.audio_flow
                    }
                ),
                match (ui.audio_rms_dbfs, ui.audio_peak_dbfs) {
                    (Some(rms), Some(peak)) => {
                        format!("Last PCM block: RMS {rms:.1} / peak {peak:.1} dBFS")
                    }
                    _ => "PCM level: no samples delivered yet".into(),
                },
                format!(
                    "PCM sent: {} samples; clipped: {}",
                    ui.audio_pcm_samples, ui.audio_clipped_samples
                ),
                "Measured after AIRWAV volume; speaker output is not measured.".into(),
                format!(
                    "Audio queue drops: {} IQ samples; discontinuities: {}",
                    ui.audio_dropped_samples, ui.audio_discontinuities
                ),
                "".into(),
            ];
            if let Some(s) = &ui.snapshot {
                let m = &s.metrics;
                l.extend([
                    format!("Received IQ samples     {}", m.received_samples),
                    format!(
                        "Application drops       {} samples",
                        m.queue_dropped_samples
                    ),
                    match m.hardware_lost_samples {
                        Some(n) => format!("USB sample loss         {n}"),
                        None => "USB sample loss         unavailable in RF mode".into(),
                    },
                    format!("Processed samples       {}", m.processed_samples),
                    format!("DSP gaps                {}", m.discontinuities),
                    format!("DSP time / last block   {} μs", m.dsp_us),
                    format!(
                        "Ring memory             {:.2} / {:.2} MiB",
                        m.ring_bytes as f64 / 1048576.,
                        m.ring_capacity_bytes as f64 / 1048576.
                    ),
                    format!("Storage snapshots lost  {}", m.storage_dropped_snapshots),
                    format!("Event IQ samples lost   {}", m.storage_dropped_iq_samples),
                    format!("Signal islands          {} / 256", s.islands.len()),
                    format!("Island candidates omitted {}", m.island_candidates_omitted),
                    "Decoder workers         0 (not enabled in this milestone)".into(),
                    "MAX-I                   not enabled in this milestone".into(),
                    "CPU/RSS                 not instrumented".into(),
                ]);
            }
            (" AIRWAV / DIAGNOSTICS ", l)
        }
    };
    if ui.overlay_scroll > lines.len().saturating_sub(2) {
        ui.overlay_scroll = lines.len().saturating_sub(2);
    }
    if ui.overlay_scroll > 0 {
        lines = lines.split_off(ui.overlay_scroll);
    }
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .block(panel(title, t, true))
            .style(Style::default().bg(t.panel).fg(t.text))
            .wrap(Wrap { trim: false }),
        rect,
    );
    ui.areas.close = Rect::new(rect.right().saturating_sub(10), rect.y, 9, 1);
    frame.render_widget(
        Paragraph::new("[ Close ]").style(Style::default().fg(t.accent)),
        ui.areas.close,
    );
}

/// Export the exact Ratatui cell buffer; Unicode and color remain native SVG text.
pub fn to_svg(buffer: &Buffer) -> String {
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><style>text{{font-family:'DejaVu Sans Mono',monospace;font-size:14px;white-space:pre}}</style>",
        buffer.area.width as u32 * 9,
        buffer.area.height as u32 * 18,
        buffer.area.width as u32 * 9,
        buffer.area.height as u32 * 18
    );
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let c = &buffer[(x + buffer.area.x, y + buffer.area.y)];
            let _ = write!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"9\" height=\"18\" fill=\"{}\"/>",
                x as u32 * 9,
                y as u32 * 18,
                theme::css(c.bg)
            );
            if c.symbol() != " " {
                let _ = write!(
                    svg,
                    "<text x=\"{}\" y=\"{}\" fill=\"{}\">{}</text>",
                    x as u32 * 9,
                    y as u32 * 18 + 14,
                    theme::css(c.fg),
                    escape(c.symbol())
                );
            }
        }
    }
    svg.push_str("<desc>");
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            svg.push_str(&escape(
                buffer[(x + buffer.area.x, y + buffer.area.y)].symbol(),
            ));
        }
        svg.push('\n');
    }
    svg.push_str("</desc></svg>");
    svg
}

fn escape(s: &str) -> String {
    s.replace('&', "\u{26}amp;")
        .replace('<', "\u{26}lt;")
        .replace('>', "\u{26}gt;")
        .replace('"', "\u{26}quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use airwav_core::{Metrics, ReceiverConfig, SignalIsland, Spectrum};
    use ratatui::{Terminal, backend::TestBackend};

    fn island(id: u64, mhz: f64, snr: f32, state: &str) -> SignalIsland {
        SignalIsland {
            id,
            first_sample: 0,
            last_sample: 256_000,
            center_hz: mhz * 1e6,
            bandwidth_hz: 12_500.,
            peak_dbfs: -40.,
            snr_db: snr,
            observations: 4,
            state: state.into(),
        }
    }

    fn snapshot(islands: Vec<SignalIsland>) -> Snapshot {
        Snapshot {
            timestamp_ns: 1,
            receiver: ReceiverConfig::default(),
            spectrum: Spectrum {
                first_sample: 0,
                bin_hz: 1250.,
                start_hz: 134_720_000.,
                power_dbfs: vec![-80.; 2048],
                noise_dbfs: -90.,
            },
            islands,
            metrics: Metrics::default(),
        }
    }

    #[test]
    fn audio_health_is_visible_at_minimum_width_and_in_diagnostics() {
        let mut ui = Ui::new("Midnight", true, "DEMO FIXTURE", true);
        ui.audio_active = true;
        ui.audio_flow = "Output stalled • check system audio".into();
        ui.audio_status = "AM 136.000000 MHz via test-only-player".into();
        ui.audio_pcm_samples = 4800;
        ui.audio_rms_dbfs = Some(-12.);
        ui.audio_peak_dbfs = Some(-6.);
        for width in [70, 132] {
            let mut terminal = Terminal::new(TestBackend::new(width, 42)).unwrap();
            terminal.draw(|f| draw(f, &mut ui)).unwrap();
            let svg = to_svg(terminal.backend().buffer());
            assert!(svg.contains("Output stalled • check system audio"));
        }
        ui.view = Some(View::Diagnostics);
        let mut terminal = Terminal::new(TestBackend::new(132, 42)).unwrap();
        terminal.draw(|f| draw(f, &mut ui)).unwrap();
        let svg = to_svg(terminal.backend().buffer());
        assert!(svg.contains("RMS -12.0 / peak -6.0 dBFS"));
        assert!(svg.contains("PCM sent: 4800 samples; clipped: 0"));
        assert!(svg.contains("speaker output is not measured"));
    }

    #[test]
    fn audio_controls_have_keyboard_and_mouse_actions() {
        let mut ui = Ui::new("Midnight", true, "DEMO FIXTURE", true);
        for (key, expected) in [
            ('a', Action::AudioToggle),
            ('m', Action::AudioMode),
            ('9', Action::AudioVolume(-10)),
            ('0', Action::AudioVolume(10)),
        ] {
            assert_eq!(
                ui.handle(Event::Key(crossterm::event::KeyEvent::new(
                    KeyCode::Char(key),
                    KeyModifiers::NONE
                ))),
                expected
            );
        }
        let mut terminal = Terminal::new(TestBackend::new(132, 42)).unwrap();
        terminal.draw(|f| draw(f, &mut ui)).unwrap();
        for action in [
            Action::AudioToggle,
            Action::AudioMode,
            Action::AudioVolume(-10),
            Action::AudioVolume(10),
        ] {
            let (rect, _) = ui.areas.buttons.iter().find(|(_, a)| *a == action).unwrap();
            let event = Event::Mouse(crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x + 1,
                row: rect.y,
                modifiers: KeyModifiers::NONE,
            });
            assert_eq!(ui.handle(event), action);
        }
        assert!(!ui.audio_active);
    }

    #[test]
    fn renders_without_hardware_and_resizes() {
        for (w, h) in [(120, 38), (80, 24), (30, 8)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            let mut ui = Ui::new("Midnight", true, "NO HARDWARE", false);
            terminal.draw(|f| draw(f, &mut ui)).unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("AIRWAV") || text.contains("A I R W A V"));
            assert!(!text.contains("CONNECTED"));
        }
    }

    #[test]
    fn replay_cannot_trigger_live_capture() {
        let mut ui = Ui::new("Midnight", true, "REPLAY", true);
        assert_eq!(
            ui.handle(Event::Key(crossterm::event::KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::NONE
            ))),
            Action::None
        );
    }

    #[test]
    fn demo_changes_presentation_only() {
        let mut ui = Ui::new("Studio", true, "DEMO FIXTURE", true);
        ui.handle(Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::F(10),
            KeyModifiers::NONE,
        )));
        assert!(ui.demo);
        assert!(ui.snapshot.is_none());
    }

    #[test]
    fn svg_escapes_xml_and_preserves_unicode() {
        let mut b = Buffer::empty(Rect::new(0, 0, 2, 1));
        b[(0, 0)].set_symbol("<");
        b[(1, 0)].set_symbol("╭");
        let svg = to_svg(&b);
        assert!(svg.contains("\u{26}lt;"));
        assert!(svg.contains('╭'));
    }

    #[test]
    fn diagnostics_and_help_are_mouse_accessible() {
        let mut terminal = Terminal::new(TestBackend::new(120, 38)).unwrap();
        let mut ui = Ui::new("Midnight", true, "REPLAY", true);
        terminal.draw(|f| draw(f, &mut ui)).unwrap();
        let area = ui
            .areas
            .buttons
            .iter()
            .find(|(_, a)| *a == Action::Open(View::Diagnostics))
            .unwrap()
            .0;
        let event = Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 1,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(ui.handle(event), Action::None);
        assert_eq!(ui.view, Some(View::Diagnostics));
    }

    #[test]
    fn themes_support_256_colors() {
        for name in ["Midnight", "Radar", "Arctic", "Ember", "Studio"] {
            assert!(matches!(
                Theme::named(name, false).accent,
                Color::Indexed(_)
            ));
        }
    }

    #[test]
    fn live_and_fading_are_not_collapsed_to_unknown() {
        let mut ui = Ui::new("Midnight", true, "DEMO FIXTURE", true);
        ui.update(snapshot(vec![
            island(1, 135.2, 18., "LIVE"),
            island(2, 136.1, 9., "FADING"),
        ]));
        let mut terminal = Terminal::new(TestBackend::new(132, 42)).unwrap();
        terminal.draw(|f| draw(f, &mut ui)).unwrap();
        let svg = to_svg(terminal.backend().buffer());
        assert!(svg.contains("LIVE"), "{svg}");
        assert!(svg.contains("FADING"), "{svg}");
        ui.view = Some(View::Evidence);
        terminal.draw(|f| draw(f, &mut ui)).unwrap();
        let svg = to_svg(terminal.backend().buffer());
        assert!(svg.contains("Protocol      UNKNOWN"));
        assert!(svg.contains("Activity      LIVE"));
    }

    #[test]
    fn hide_fading_and_snr_sort_change_the_visible_list() {
        let mut ui = Ui::new("Midnight", true, "DEMO FIXTURE", true);
        ui.update(snapshot(vec![
            island(1, 135.0, 8., "LIVE"),
            island(2, 136.0, 30., "FADING"),
            island(3, 137.0, 22., "LIVE"),
        ]));
        ui.hide_fading = true;
        ui.clamp_selection();
        let visible = ui.visible_indices();
        assert_eq!(visible.len(), 2);
        ui.sort_snr = true;
        let visible = ui.visible_indices();
        assert_eq!(ui.snapshot.as_ref().unwrap().islands[visible[0]].id, 3);
    }

    #[test]
    fn fixture_header_does_not_claim_hardware() {
        let mut ui = Ui::new("Midnight", true, "DEMO FIXTURE", true);
        ui.update(snapshot(vec![]));
        let mut terminal = Terminal::new(TestBackend::new(132, 42)).unwrap();
        terminal.draw(|f| draw(f, &mut ui)).unwrap();
        let svg = to_svg(terminal.backend().buffer());
        assert!(svg.contains("SYNTHETIC IQ"));
        assert!(!svg.contains("RTL-SDR BLOG V4"));
    }

    #[test]
    fn zoom_to_selected_island_narrows_the_window() {
        let mut ui = Ui::new("Midnight", true, "DEMO FIXTURE", true);
        ui.update(snapshot(vec![island(1, 136.0, 20., "LIVE")]));
        ui.selected = 0;
        ui.zoom_to_selected();
        assert!(ui.zoom > 1., "zoom={}", ui.zoom);
        assert!((ui.pan - 0.5).abs() < 0.2, "pan={}", ui.pan);
    }

    #[test]
    fn keyboard_zoom_keeps_the_cursor_frequency() {
        let mut ui = Ui::new("Midnight", true, "DEMO FIXTURE", true);
        ui.update(snapshot(vec![]));
        let start = 134_720_000.;
        let bin_hz = 1250.;
        ui.cursor_hz = Some(start + 512. * bin_hz);
        ui.cursor_dbfs = Some(-40.);
        ui.zoom_by(1., ui.cursor_frac());
        assert!((ui.zoom - 2.).abs() < 1e-9);
        let (a, b) = range(ui.zoom, ui.pan, 2048);
        let hz = ui.cursor_hz.unwrap();
        let bin = (hz - start) / bin_hz;
        assert!(
            bin >= a as f64 && bin < b as f64,
            "cursor left the window ({a}..{b}), bin={bin}"
        );
    }

    #[test]
    fn drag_spectrum_zooms_the_window() {
        let mut ui = Ui::new("Midnight", true, "DEMO FIXTURE", true);
        ui.update(snapshot(vec![]));
        ui.apply_drag(0.4, 0.6);
        assert!(ui.zoom > 1., "zoom={}", ui.zoom);
        assert!((ui.pan - 0.5).abs() < 0.05, "pan={}", ui.pan);
    }

    #[test]
    fn selecting_an_offscreen_island_pans_the_window() {
        let mut ui = Ui::new("Midnight", true, "DEMO FIXTURE", true);
        ui.update(snapshot(vec![
            island(1, 135.0, 20., "LIVE"),
            island(2, 137.0, 20., "LIVE"),
        ]));
        ui.zoom = 8.;
        ui.pan = 0.1;
        ui.selected = 0;
        ui.pan_to_selected();
        ui.selected = 1;
        ui.pan_to_selected();
        let (a, b) = range(ui.zoom, ui.pan, 2048);
        let start = 134_720_000.;
        let bin = (137e6 - start) / 1250.;
        assert!(
            bin >= a as f64 && bin < b as f64,
            "bin={bin} window={a}..{b}"
        );
    }

    #[test]
    fn unknown_activity_from_old_recordings_renders_as_live() {
        assert_eq!(activity("UNKNOWN"), "LIVE");
        assert_eq!(activity("FADING / UNKNOWN"), "FADING");
        assert_eq!(activity("LIVE"), "LIVE");
        assert_eq!(activity("FADING"), "FADING");
    }
}
