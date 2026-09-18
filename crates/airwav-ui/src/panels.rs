//! Signal list and evidence panels.
use crate::theme::Theme;
use crate::ui::panel;
use crate::Ui;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Paragraph, Wrap},
};

pub(crate) fn signals(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme) {
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
pub(crate) fn evidence_lines(ui: &Ui) -> Vec<String> {
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
pub(crate) fn evidence(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
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
