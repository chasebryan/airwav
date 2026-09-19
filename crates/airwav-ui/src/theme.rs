//! Palette only. Rendering never invents RF facts from color.
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
    pub live: Color,
    pub waterfall: [Color; 8],
}

impl Theme {
    pub fn named(name: &str, truecolor: bool) -> Self {
        let c = |rgb: (u8, u8, u8)| color(rgb.0, rgb.1, rgb.2, truecolor);
        match name {
            "Radar" => Self {
                name: "Radar",
                background: c((6, 16, 21)),
                panel: c((10, 24, 29)),
                text: c((208, 229, 219)),
                muted: c((110, 138, 128)),
                border: c((29, 61, 56)),
                accent: c((74, 208, 166)),
                prism: c((126, 224, 192)),
                unknown: c((230, 181, 98)),
                selected: c((214, 255, 240)),
                danger: c((238, 101, 130)),
                live: c((74, 208, 166)),
                waterfall: [
                    c((6, 16, 21)),
                    c((7, 34, 28)),
                    c((11, 58, 48)),
                    c((15, 90, 70)),
                    c((26, 138, 104)),
                    c((74, 208, 166)),
                    c((154, 232, 200)),
                    c((228, 250, 240)),
                ],
            },
            "Arctic" => Self {
                name: "Arctic",
                background: c((222, 230, 239)),
                panel: c((234, 240, 246)),
                text: c((23, 40, 56)),
                muted: c((91, 115, 136)),
                border: c((183, 198, 212)),
                accent: c((8, 100, 167)),
                prism: c((11, 122, 134)),
                unknown: c((161, 92, 18)),
                selected: c((8, 61, 104)),
                danger: c((180, 35, 60)),
                live: c((11, 122, 90)),
                waterfall: [
                    c((222, 230, 239)),
                    c((197, 213, 230)),
                    c((155, 184, 214)),
                    c((110, 150, 196)),
                    c((61, 116, 174)),
                    c((27, 91, 150)),
                    c((13, 63, 114)),
                    c((8, 40, 68)),
                ],
            },
            "Ember" => Self {
                name: "Ember",
                background: c((24, 16, 21)),
                panel: c((34, 22, 28)),
                text: c((240, 222, 218)),
                muted: c((154, 125, 120)),
                border: c((74, 48, 54)),
                accent: c((244, 151, 93)),
                prism: c((232, 192, 122)),
                unknown: c((230, 181, 98)),
                selected: c((255, 216, 194)),
                danger: c((238, 101, 130)),
                live: c((120, 196, 132)),
                waterfall: [
                    c((24, 16, 21)),
                    c((42, 20, 22)),
                    c((74, 28, 28)),
                    c((122, 42, 34)),
                    c((180, 69, 44)),
                    c((224, 106, 58)),
                    c((244, 151, 93)),
                    c((253, 224, 200)),
                ],
            },
            "Studio" => Self {
                name: "Studio",
                background: c((8, 12, 21)),
                panel: c((14, 21, 32)),
                text: c((243, 247, 252)),
                muted: c((123, 141, 163)),
                border: c((36, 48, 68)),
                accent: c((89, 213, 245)),
                prism: c((126, 224, 210)),
                unknown: c((230, 181, 98)),
                selected: c((215, 246, 255)),
                danger: c((238, 101, 130)),
                live: c((80, 210, 191)),
                waterfall: [
                    c((8, 12, 21)),
                    c((12, 28, 51)),
                    c((18, 48, 86)),
                    c((26, 74, 130)),
                    c((42, 116, 184)),
                    c((89, 213, 245)),
                    c((166, 234, 248)),
                    c((232, 248, 255)),
                ],
            },
            _ => Self {
                name: "Midnight",
                background: c((10, 14, 22)),
                panel: c((15, 22, 32)),
                text: c((213, 224, 238)),
                muted: c((117, 137, 159)),
                border: c((43, 62, 83)),
                accent: c((84, 190, 240)),
                prism: c((80, 210, 191)),
                unknown: c((230, 181, 98)),
                selected: c((197, 232, 247)),
                danger: c((238, 101, 130)),
                live: c((80, 210, 191)),
                waterfall: [
                    c((10, 14, 22)),
                    c((16, 29, 49)),
                    c((26, 47, 82)),
                    c((39, 65, 129)),
                    c((54, 105, 176)),
                    c((70, 161, 204)),
                    c((123, 198, 219)),
                    c((215, 238, 246)),
                ],
            },
        }
    }
}

pub fn color(r: u8, g: u8, b: u8, truecolor: bool) -> Color {
    if truecolor {
        Color::Rgb(r, g, b)
    } else {
        let q = |v: u8| ((v as u16 * 5 + 127) / 255) as u8;
        Color::Indexed(16 + 36 * q(r) + 6 * q(g) + q(b))
    }
}

pub fn css(c: Color) -> String {
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
