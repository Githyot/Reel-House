//! Accent colour maths: turns "#rrggbb" into the CSS that recolours libadwaita.

pub const PRESETS: [(&str, &str); 8] = [
    ("#3584e4", "Blue"),
    ("#2190a4", "Teal"),
    ("#3a944a", "Green"),
    ("#c88800", "Yellow"),
    ("#ed5b00", "Orange"),
    ("#e62d42", "Red"),
    ("#d56199", "Pink"),
    ("#9141ac", "Purple"),
];

pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.trim().strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((v(0)?, v(2)?, v(4)?))
}

fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    (0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64) / 255.0
}

fn mix((r, g, b): (u8, u8, u8), with: u8, t: f64) -> (u8, u8, u8) {
    let m = |c: u8| (c as f64 + (with as f64 - c as f64) * t).round().clamp(0.0, 255.0) as u8;
    (m(r), m(g), m(b))
}

fn hex(c: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

/// CSS for the accent, or an empty string for the system default.
pub fn accent_css(accent: &str, dark: bool) -> String {
    let Some(bg) = parse_hex(accent) else { return String::new() };
    let fg = if luminance(bg) > 0.62 { "#000000" } else { "#ffffff" };
    // The accent used for text and icons has to stay readable on the window colour.
    let text = if dark { mix(bg, 255, 0.35) } else { mix(bg, 0, 0.15) };
    let (bg, text) = (hex(bg), hex(text));
    format!(
        "@define-color accent_bg_color {bg};\n@define-color accent_fg_color {fg};\n@define-color accent_color {text};\n:root {{ --accent-bg-color: {bg}; --accent-fg-color: {fg}; --accent-color: {text}; }}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#3584e4"), Some((0x35, 0x84, 0xe4)));
        assert_eq!(parse_hex("3584e4"), None);
        assert_eq!(parse_hex("#35"), None);
        assert_eq!(parse_hex(""), None);
    }

    #[test]
    fn css_follows_the_scheme() {
        assert_eq!(accent_css("", true), "");
        let light = accent_css("#e62d42", false);
        assert!(light.contains("--accent-bg-color: #e62d42"));
        assert!(light.contains("accent_fg_color #ffffff"));
        let dark = accent_css("#e62d42", true);
        assert_ne!(light, dark);
        assert!(accent_css("#f5e050", false).contains("accent_fg_color #000000"));
    }
}
