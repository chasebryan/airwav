//! Theme palette for AIRWAV TUI.
use ratatui::style::Color;

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
