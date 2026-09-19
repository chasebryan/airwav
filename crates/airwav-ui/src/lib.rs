//! AIRWAV's native terminal presentation. Rendering never touches receiver I/O.
use airwav_core::Snapshot;
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
                    Action::None
                }
                KeyCode::Char('?') => {
                    self.view = Some(View::Help);
                    Action::None
                }
                KeyCode::Char('d') => {
                    self.view = Some(View::Diagnostics);
                    Action::None
                }
                KeyCode::Char('e') => {
                    self.view = Some(View::Events);
                    Action::None
                }
                KeyCode::Char('i') | KeyCode::Enter => {
                    self.view = Some(View::Evidence);
                    Action::None
                }
                KeyCode::Tab => {
                    self.focused = (self.focused + 1) % 3;
                    Action::None
                }
                KeyCode::BackTab => {
                    self.focused = (self.focused + 2) % 3;
                    Action::None
                }
                KeyCode::Down => {
                    self.select(1);
                    Action::None
                }
                KeyCode::Up => {
                    self.select(-1);
                    Action::None
                }
                KeyCode::Char('r') if !self.replay => Action::Record,
                KeyCode::Char('c') if !self.replay => Action::Capture,
                KeyCode::Char(' ') => Action::Pause,
                KeyCode::Char('.') if self.replay => Action::Step,
                KeyCode::Char('[') if self.replay => Action::PreviousEvent,
                KeyCode::Char(']') if self.replay => Action::NextEvent,
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.zoom = (self.zoom * 2.).min(16.);
                    Action::None
                }
                KeyCode::Char('-') => {
                    self.zoom = (self.zoom / 2.).max(1.);
                    Action::None
                }
                KeyCode::Left => {
                    self.pan = (self.pan - 0.1 / self.zoom).max(0.);
                    Action::None
                }
                KeyCode::Right => {
                    self.pan = (self.pan + 0.1 / self.zoom).min(1.);
                    Action::None
                }
                KeyCode::Char(n @ '1'..='5') if self.replay => {
                    Action::Speed([0.25, 0.5, 1., 2., 4.][n as usize - '1' as usize])
                }
                KeyCode::Char('t') => {
                    self.cycle_theme();
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
                if self.view.is_some() {
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                        && contains(self.areas.close, point)
                    {
                        self.view = None;
                    }
                    return Action::None;
                }
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        for (r, action) in &self.areas.buttons {
                            if contains(*r, point) {
                                return match action.clone() {
                                    Action::Open(view) => {
                                        self.view = Some(view);
                                        Action::None
                                    }
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
                        if contains(self.areas.signals, point)
                            && mouse.row > self.areas.signals.y + 2
                        {
                            self.selected = self.signal_scroll
                                + (mouse.row - self.areas.signals.y - 3) as usize;
                            self.select(0);
                            if self.last_click.is_some_and(|(x, y, t)| {
                                x == point.0
                                    && y == point.1
                                    && t.elapsed() < Duration::from_millis(400)
                            }) {
                                self.view = Some(View::Evidence);
                            }
                            self.last_click = Some((point.0, point.1, Instant::now()));
                        }
                    }
                    MouseEventKind::Down(MouseButton::Right) => {
                        if contains(self.areas.signals, point) {
                            self.selected = self.signal_scroll
                                + mouse.row.saturating_sub(self.areas.signals.y + 3) as usize;
                            self.select(0);
                            self.view = Some(View::Evidence);
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
                                self.zoom = (self.zoom * 2f64.powf(direction)).clamp(1., 16.);
                            }
                        } else {
                            self.select(-direction as i32);
                        }
                    }
                    _ => {}
                }
                Action::None
            }
            _ => Action::None,
        }
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
    fn select(&mut self, delta: i32) {
        let count = self.snapshot.as_ref().map_or(0, |s| s.islands.len());
        self.selected = (self.selected as i32 + delta)
            .max(0)
            .min(count.saturating_sub(1) as i32) as usize;
    }
}
fn contains(r: Rect, p: (u16, u16)) -> bool {
    r.contains((p.0, p.1).into())
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

pub fn draw(frame: &mut Frame, ui: &mut Ui) {
    let area = frame.area();
    let t = ui.theme.clone();
    frame.render_widget(
        Block::default().style(Style::default().bg(t.background).fg(t.text)),
        area,
    );
    if area.width < 70 || area.height < 22 {
        frame.render_widget(Paragraph::new("AIRWAV\n\nResize to at least 70 × 22 cells.\nCapture continues independently.\n\nQ quit · ? help").style(Style::default().fg(t.accent)),area);
        return;
    }
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(12),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(area);
    let source_color = if ui.source.contains("FIXTURE") {
        t.unknown
    } else {
        t.prism
    };
    let mut header = vec![Line::from(vec![
        Span::styled(
            "  A I R W A V ",
            Style::default().fg(t.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" / RF OBSERVATION TERMINAL", Style::default().fg(t.muted)),
        Span::styled(
            format!("    {}", ui.source),
            Style::default().fg(source_color),
        ),
    ])];
    let info = if let Some(s) = &ui.snapshot {
        format!(
            "  RTL-SDR BLOG V4   {:>10.6} MHz   {:.2} MS/s   {}   USB loss: unavailable",
            s.receiver.center_hz as f64 / 1e6,
            s.receiver.sample_rate as f64 / 1e6,
            if ui.replay {
                format!("REPLAY {:.2}×", ui.speed)
            } else {
                "MANUAL WINDOW".into()
            }
        )
    } else {
        "  RTL-SDR BLOG V4   •   Waiting for first measured spectrum".into()
    };
    header.push(Line::styled(info, Style::default().fg(t.muted)));
    header.push(Line::from(vec![
        Span::styled(
            if ui.recording {
                "  ● RECORDING"
            } else if ui.replay {
                "  ○ REPLAY"
            } else {
                "  ○ OBSERVING"
            },
            Style::default().fg(if ui.recording { t.danger } else { t.muted }),
        ),
        Span::styled(
            if ui.capture_active {
                "    EVENT IQ • COLLECTING"
            } else {
                "    RECEIVE ONLY"
            },
            Style::default().fg(t.prism),
        ),
        Span::styled(
            if ui.paused { "    VIEW PAUSED" } else { "" },
            Style::default().fg(t.unknown),
        ),
    ]));
    header.push(Line::styled(
        format!(
            "  {} · VOL {}% · {}{}",
            ["AM", "FM", "NFM"][ui.audio_mode % 3],
            ui.audio_volume,
            if ui.audio_flow.is_empty() {
                String::new()
            } else {
                format!("{} · ", ui.audio_flow)
            },
            ui.audio_status
        ),
        Style::default().fg(if ui.audio_active { t.prism } else { t.unknown }),
    ));
    frame.render_widget(Paragraph::new(header), main[0]);
    let cols = Layout::horizontal([
        Constraint::Percentage(if ui.demo { 67 } else { 62 }),
        Constraint::Min(27),
    ])
    .split(main[1]);
    let left =
        Layout::vertical([Constraint::Percentage(44), Constraint::Percentage(56)]).split(cols[0]);
    let right =
        Layout::vertical([Constraint::Percentage(47), Constraint::Percentage(53)]).split(cols[1]);
    ui.areas.spectrum = left[0];
    ui.areas.signals = right[0];
    spectrum(frame, left[0], ui, &t);
    waterfall(frame, left[1], ui, &t);
    signals(frame, right[0], ui, &t);
    evidence(frame, right[1], ui, &t);
    buttons(frame, main[2], ui, &t);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(format!("  {}", ui.status), Style::default().fg(t.muted)),
            Line::styled(
                "  ? Help    ↑↓ Select    Enter Evidence    T Theme    F10 Demo view    Q Quit",
                Style::default().fg(t.muted),
            ),
        ]),
        main[3],
    );
    if let Some(view) = ui.view {
        overlay(frame, ui, view, &t);
    }
}
fn range(ui: &Ui, len: usize) -> (usize, usize) {
    let width = (len as f64 / ui.zoom) as usize;
    let width = width.max(1).min(len);
    let start = ((ui.pan * len as f64) as usize)
        .saturating_sub(width / 2)
        .min(len - width);
    (start, start + width)
}
fn spectrum(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
    let block = panel(" 01 / SPECTRUM · dBFS ", t, ui.focused == 0);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(s) = &ui.snapshot else {
        frame.render_widget(Paragraph::new("\n  No received or replayed IQ."), inner);
        return;
    };
    if inner.height < 3 || inner.width == 0 {
        return;
    }
    let (a, b) = range(ui, s.spectrum.power_dbfs.len());
    let ceiling = s.spectrum.power_dbfs[a..b]
        .iter()
        .copied()
        .fold(-40., f32::max)
        .ceil()
        + 5.;
    let floor = (s.spectrum.noise_dbfs - 10.).max(-140.);
    let span = (ceiling - floor).max(20.);
    let height = inner.height - 2;
    for x in 0..inner.width {
        let from = a + x as usize * (b - a) / inner.width as usize;
        let to = (a + (x as usize + 1) * (b - a) / inner.width as usize)
            .max(from + 1)
            .min(b);
        let value = s.spectrum.power_dbfs[from..to]
            .iter()
            .copied()
            .fold(-160., f32::max);
        let bars =
            ((value - floor) / span * height as f32 * 8.).clamp(0., height as f32 * 8.) as usize;
        for y in 0..height {
            let level = bars.saturating_sub(y as usize * 8).min(8);
            let ch = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"][level];
            let cell = &mut frame.buffer_mut()[(inner.x + x, inner.y + height - 1 - y)];
            cell.set_symbol(ch)
                .set_fg(if y > height / 2 { t.selected } else { t.accent });
        }
    }
    let from = (s.spectrum.start_hz + a as f64 * s.spectrum.bin_hz) / 1e6;
    let to = (s.spectrum.start_hz + b as f64 * s.spectrum.bin_hz) / 1e6;
    let axis = Rect::new(inner.x, inner.y + height, inner.width, 2);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                format!(
                    "{from:.3} MHz  ──  {:.3} MHz  ──  {to:.3}",
                    (from + to) / 2.
                ),
                Style::default().fg(t.muted),
            ),
            Line::styled(
                format!(
                    "Floor {:.1} dBFS · {:.1} kHz/bin · zoom {:.0}×",
                    s.spectrum.noise_dbfs,
                    s.spectrum.bin_hz / 1000.,
                    ui.zoom
                ),
                Style::default().fg(t.muted),
            ),
        ]),
        axis,
    );
}
fn waterfall(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
    let block = panel(" 02 / WATERFALL · measured history ", t, ui.focused == 1);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let floor = ui
        .snapshot
        .as_ref()
        .map_or(-100., |s| s.spectrum.noise_dbfs - 4.);
    for (row, data) in ui.history.iter().take(inner.height as usize).enumerate() {
        let (a, b) = range(ui, data.len());
        for x in 0..inner.width {
            let from = a + x as usize * (b - a) / inner.width as usize;
            let to = (a + (x as usize + 1) * (b - a) / inner.width as usize)
                .max(from + 1)
                .min(b);
            let power = data[from..to].iter().copied().fold(-160., f32::max);
            let index = (((power - floor) / 50. * 7.).clamp(0., 7.)) as usize;
            frame.buffer_mut()[(inner.x + x, inner.y + row as u16)]
                .set_symbol(" ")
                .set_bg(t.waterfall[index]);
        }
    }
    if let Some(s) = &ui.snapshot {
        let (a, b) = range(ui, s.spectrum.power_dbfs.len());
        for (i, signal) in s.islands.iter().enumerate() {
            let bin = (signal.center_hz - s.spectrum.start_hz) / s.spectrum.bin_hz;
            if bin >= a as f64 && bin < b as f64 && inner.height > 0 {
                let x = inner.x + ((bin - a as f64) / (b - a) as f64 * inner.width as f64) as u16;
                frame.buffer_mut()[(x.min(inner.right() - 1), inner.y)]
                    .set_symbol("▼")
                    .set_fg(if i == ui.selected {
                        t.selected
                    } else {
                        t.unknown
                    });
            }
        }
    }
}
fn signals(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme) {
    let block = panel(" 03 / SIGNAL ISLANDS ", t, ui.focused == 2);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = vec![
        Line::styled(
            " MHz          SNR       STATE",
            Style::default().fg(t.muted),
        ),
        Line::from(""),
    ];
    if let Some(s) = &ui.snapshot {
        let visible = inner.height.saturating_sub(2) as usize;
        ui.signal_scroll = ui.selected.saturating_sub(visible.saturating_sub(1));
        for (i, signal) in s
            .islands
            .iter()
            .enumerate()
            .skip(ui.signal_scroll)
            .take(visible)
        {
            lines.push(Line::styled(
                format!(
                    "{}{:>10.6}  {:>4.1} dB  {}",
                    if i == ui.selected { "▌" } else { " " },
                    signal.center_hz / 1e6,
                    signal.snr_db,
                    if signal.state.starts_with("FADING") {
                        "FADING"
                    } else {
                        "UNKNOWN"
                    }
                ),
                Style::default().fg(if i == ui.selected {
                    t.selected
                } else {
                    t.unknown
                }),
            ));
        }
        if s.islands.is_empty() {
            lines.push(Line::styled(
                " No activity above threshold",
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
            lines.extend([
                format!("SIGNAL ISLAND {:04}", signal.id),
                String::new(),
                format!("Frequency   {:.6} MHz", signal.center_hz / 1e6),
                format!("Bandwidth   {:.2} kHz", signal.bandwidth_hz / 1000.),
                format!("Peak        {:.1} dBFS", signal.peak_dbfs),
                format!("SNR         {:.1} dB", signal.snr_db),
                format!("Observations {}", signal.observations),
                String::new(),
                "Protocol    UNKNOWN".into(),
                "Confidence  not established".into(),
                "Evidence    FFT power / local noise".into(),
                "No frame or identity decoded.".into(),
            ]);
        } else {
            lines.extend([
                "OBSERVATION BEFORE CONCLUSION".into(),
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
fn buttons(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme) {
    let specs = if ui.replay {
        vec![
            ("⏯ Pause", Action::Pause),
            ("Step", Action::Step),
            ("◀ Event", Action::PreviousEvent),
            ("Event ▶", Action::NextEvent),
            (
                if ui.audio_active {
                    "A Mute"
                } else {
                    "A Listen"
                },
                Action::AudioToggle,
            ),
            ("F12 Save", Action::Screenshot),
            ("Quit", Action::Quit),
        ]
    } else {
        vec![
            (
                if ui.recording {
                    "■ Stop REC"
                } else {
                    "● Record"
                },
                Action::Record,
            ),
            ("Capture IQ", Action::Capture),
            ("⏯ View", Action::Pause),
            (
                if ui.audio_active {
                    "A Mute"
                } else {
                    "A Listen"
                },
                Action::AudioToggle,
            ),
            ("F12 Save", Action::Screenshot),
            ("Quit", Action::Quit),
        ]
    };
    let constraints: Vec<_> = specs
        .iter()
        .map(|_| Constraint::Ratio(1, specs.len() as u32))
        .collect();
    let layout = Layout::horizontal(constraints).split(Rect::new(area.x, area.y, area.width, 3));
    ui.areas.buttons.clear();
    for ((label, action), r) in specs.into_iter().zip(layout.iter()) {
        let hovered = ui.hover.is_some_and(|p| contains(*r, p));
        frame.render_widget(
            Paragraph::new(label)
                .centered()
                .block(panel("", t, hovered))
                .style(Style::default().fg(if hovered { t.text } else { t.accent })),
            *r,
        );
        ui.areas.buttons.push((*r, action));
    }
    let mut secondary = vec![
        ("Evidence", Action::Open(View::Evidence)),
        ("Events", Action::Open(View::Events)),
        ("Diagnostics", Action::Open(View::Diagnostics)),
        ("M Mode", Action::AudioMode),
        ("9 Vol−", Action::AudioVolume(-10)),
        ("0 Vol+", Action::AudioVolume(10)),
        ("Theme", Action::Theme),
        ("Demo", Action::Demo),
        ("Help", Action::Open(View::Help)),
    ];
    if ui.replay {
        secondary.push(("Speed", Action::CycleSpeed));
    }
    let constraints: Vec<_> = secondary
        .iter()
        .map(|_| Constraint::Ratio(1, secondary.len() as u32))
        .collect();
    let layout =
        Layout::horizontal(constraints).split(Rect::new(area.x, area.y + 3, area.width, 1));
    for ((label, action), r) in secondary.into_iter().zip(layout.iter()) {
        frame.render_widget(
            Paragraph::new(format!("[ {label} ]"))
                .centered()
                .style(Style::default().fg(t.muted)),
            *r,
        );
        ui.areas.buttons.push((*r, action));
    }
}
fn overlay(frame: &mut Frame, ui: &mut Ui, view: View, t: &Theme) {
    let area = frame.area();
    let width = area.width.min(88).saturating_sub(4);
    let height = area.height.min(32).saturating_sub(2);
    let rect = Rect::new(
        (area.width - width) / 2,
        (area.height - height) / 2,
        width,
        height,
    );
    let (title, lines) = match view {
        View::Help => (
            " AIRWAV / HELP ",
            vec![
                "OBSERVE FIRST. CONCLUDE SECOND.".into(),
                "".into(),
                "↑/↓ select · Enter/I evidence · D diagnostics · E events".into(),
                "R recording · C pre/post-trigger IQ · Space pause presentation".into(),
                "A Listen/Mute · M cycles AM/FM/NFM · 9/0 volume down/up".into(),
                "Listen locks the selected island (or receiver center if none).".into(),
                "Mute then Listen to select another frequency. Mode is manual.".into(),
                "Replay audio plays the nearest captured IQ event at 1×.".into(),
                "View pause/speed do not pause or change audio; A mutes.".into(),
                "PCM level measures player input, not speaker output. D shows details.".into(),
                "Replay: 1–5 = 0.25× / 0.5× / 1× / 2× / 4× · . step".into(),
                "[ / ] jump to recorded events · +/- zoom · ←/→ pan".into(),
                "Mouse: click select · double/right click inspect · wheel zoom".into(),
                "Shift+wheel pan · buttons at the bottom control the session".into(),
                "T theme · Tab focus · F10 presentation-only demo · F12 SVG".into(),
                "Esc closes overlays · Q quits and finalizes recording".into(),
                "".into(),
                "Signal Island: measured RF activity above a local noise estimate.".into(),
                "UNKNOWN: observed, but a protocol has not been established.".into(),
                "SNR is a measurement, not protocol confidence.".into(),
                "".into(),
                "This foundation release uses a manual observation window.".into(),
                "PRISM allocation, MAX-I scheduling and decoders are".into(),
                "future milestones; no decoder or scheduler score is fabricated.".into(),
                "A capture preserves available ring IQ and the configured post-roll.".into(),
                "USB sample loss cannot be measured in normal librtlsdr RF mode.".into(),
            ],
        ),
        View::Evidence => (" AIRWAV / EVIDENCE ", evidence_lines(ui)),
        View::Events => (
            " AIRWAV / EVENTS ",
            if ui.events.is_empty() {
                vec!["No event captures in this session.".into()]
            } else {
                ui.events.iter().rev().take(22).cloned().collect()
            },
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
                    "USB sample loss         unavailable in RF mode".into(),
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
                css(c.bg)
            );
            if c.symbol() != " " {
                let _ = write!(
                    svg,
                    "<text x=\"{}\" y=\"{}\" fill=\"{}\">{}</text>",
                    x as u32 * 9,
                    y as u32 * 18 + 14,
                    css(c.fg),
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
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn css(c: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(n) if n >= 232 => {
            let v = 8 + (n - 232) * 10;
            format!("#{v:02x}{v:02x}{v:02x}")
        }
        Color::Indexed(n) if n >= 16 => {
            let n = n - 16;
            let channel = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            format!(
                "#{:02x}{:02x}{:02x}",
                channel(n / 36),
                channel(n / 6 % 6),
                channel(n % 6)
            )
        }
        Color::Black | Color::Reset => "#0a0e16".into(),
        _ => "#d5e0ee".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
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
        assert!(svg.contains("&lt;"));
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
}
