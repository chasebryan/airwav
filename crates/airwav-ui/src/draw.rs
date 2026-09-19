//! Frame drawing for spectrum, waterfall, and chrome.
use crate::recording_label;
use crate::{Theme, Ui, View};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

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
            recording_label::mode_status_label(ui),
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

fn spectrum(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
    crate::panels::spectrum(frame, area, ui, t);
}
fn waterfall(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
    crate::panels::waterfall(frame, area, ui, t);
}
fn signals(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme) {
    crate::panels::signals(frame, area, ui, t);
}
fn evidence(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
    crate::panels::evidence(frame, area, ui, t);
}
fn buttons(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme) {
    crate::chrome::buttons(frame, area, ui, t);
}
fn overlay(frame: &mut Frame, ui: &mut Ui, view: View, t: &Theme) {
    crate::chrome::overlay(frame, ui, view, t);
}
