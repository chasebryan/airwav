//! Cell-buffer SVG export.
use ratatui::{buffer::Buffer, style::Color};
use std::fmt::Write as _;

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
        .replace('\"', "&quot;")
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
