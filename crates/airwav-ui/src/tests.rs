use crate::theme::Theme;
use crate::ui::{format_elapsed, recording_label};
use crate::{Action, Ui, View, draw, to_svg};
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::Color,
};
use std::time::Duration;

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
#[test]
fn format_elapsed_uses_compact_clock() {
    assert_eq!(format_elapsed(Duration::from_secs(0)), "0:00");
    assert_eq!(format_elapsed(Duration::from_secs(65)), "1:05");
    assert_eq!(format_elapsed(Duration::from_secs(3723)), "1:02:03");
}
#[test]
fn set_recording_tracks_elapsed_start() {
    let mut ui = Ui::new("Midnight", true, "LIVE V4", false);
    assert!(!ui.recording);
    assert!(ui.recording_started.is_none());
    ui.set_recording(true);
    assert!(ui.recording);
    let started = ui.recording_started.expect("start time");
    ui.set_recording(true);
    assert_eq!(ui.recording_started, Some(started));
    let label = recording_label(&ui);
    assert!(label.starts_with("  ● RECORDING "), "{label}");
    ui.set_recording(false);
    assert!(!ui.recording);
    assert!(ui.recording_started.is_none());
    assert_eq!(recording_label(&ui), "  ○ OBSERVING");
}
#[test]
fn recording_elapsed_appears_in_drawn_header() {
    let mut terminal = Terminal::new(TestBackend::new(120, 38)).unwrap();
    let mut ui = Ui::new("Midnight", true, "LIVE V4", false);
    ui.set_recording(true);
    terminal.draw(|f| draw(f, &mut ui)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(text.contains("RECORDING"), "{text}");
    assert!(text.contains(':'), "expected elapsed clock in header");
}
