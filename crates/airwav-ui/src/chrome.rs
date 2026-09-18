//! Bottom controls and overlay dialogs.
use crate::theme::Theme;
use crate::ui::{contains, panel};
use crate::{Action, Ui, View};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    widgets::{Clear, Paragraph, Wrap},
};

pub(crate) fn buttons(frame: &mut Frame, area: Rect, ui: &mut Ui, t: &Theme) {
    let specs = if ui.replay {
        vec![
            ("⏯ Pause", Action::Pause),
            ("Step", Action::Step),
            ("◀ Event", Action::PreviousEvent),
            ("Event ▶", Action::NextEvent),
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
pub(crate) fn overlay(frame: &mut Frame, ui: &mut Ui, view: View, t: &Theme) {
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
                "Recording header shows elapsed wall time while metadata recording".into(),
                "is active (R / Record). Offline AM/FM/NFM: airwav audio --help.".into(),
                "Live audio, PRISM channel DSP, MAX-I, and aviation decoders remain".into(),
                "future milestones; no decoder or scheduler score is fabricated.".into(),
                "A capture preserves available ring IQ and the configured post-roll.".into(),
                "USB sample loss cannot be measured in normal librtlsdr RF mode.".into(),
            ],
        ),
        View::Evidence => (" AIRWAV / EVIDENCE ", crate::panels::evidence_lines(ui)),
        View::Events => (
            " AIRWAV / EVENTS ",
            if ui.events.is_empty() {
                vec!["No event captures in this session.".into()]
            } else {
                ui.events.iter().rev().take(22).cloned().collect()
            },
        ),
        View::Diagnostics => {
            let mut l = vec![format!("Source: {}", ui.source), "".into()];
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
