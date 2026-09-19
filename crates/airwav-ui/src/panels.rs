//! Spectrum, waterfall, signals, and evidence panels.
use crate::{Theme, Ui, panel};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Paragraph, Wrap},
};

fn range(ui: &Ui, len: usize) -> (usize, usize) {
    let width = (len as f64 / ui.zoom) as usize;
    let width = width.max(1).min(len);
    let start = ((ui.pan * len as f64) as usize)
        .saturating_sub(width / 2)
        .min(len - width);
    (start, start + width)
}

pub(crate) fn spectrum(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
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
pub(crate) fn waterfall(frame: &mut Frame, area: Rect, ui: &Ui, t: &Theme) {
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
