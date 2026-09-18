//! Frame drawing for spectrum, waterfall, and chrome.
use crate::theme::Theme;
use crate::ui::{panel, recording_label};
use crate::Ui;
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
    let mode_label = if ui.recording {
        recording_label(ui)
    } else if ui.replay {
        "  ○ REPLAY".into()
    } else {
        "  ○ OBSERVING".into()
    };
    header.push(Line::from(vec![
        Span::styled(
            mode_label,
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
    crate::panels::signals(frame, right[0], ui, &t);
    crate::panels::evidence(frame, right[1], ui, &t);
    crate::chrome::buttons(frame, main[2], ui, &t);
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
        crate::chrome::overlay(frame, ui, view, &t);
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
