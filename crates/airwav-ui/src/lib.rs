//! AIRWAV's native terminal presentation. Rendering never touches receiver I/O.
mod theme;
mod ui;
mod draw;
mod panels;
mod svg;

pub use theme::Theme;
pub use ui::Ui;
pub use draw::draw;
pub use svg::to_svg;

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
    pub spectrum: ratatui::layout::Rect,
    pub signals: ratatui::layout::Rect,
    pub buttons: Vec<(ratatui::layout::Rect, Action)>,
    pub close: ratatui::layout::Rect,
}

#[cfg(test)]
mod tests;
