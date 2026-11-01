//! Subtitle appearance: turns the settings into mpv options and into a CSS preview.
use crate::settings::SubStyle;

/// "#RRGGBB" (or "RRGGBB") to (r, g, b). Bad input gives white.
pub fn parse_color(c: &str) -> (u8, u8, u8) {
    let h = c.trim().trim_start_matches('#');
    // ASCII only: a typed accent would make the byte slices below cut a character and panic
    if h.len() >= 6 && h.is_ascii() {
        if let (Ok(r), Ok(g), Ok(b)) = (u8::from_str_radix(&h[0..2], 16), u8::from_str_radix(&h[2..4], 16), u8::from_str_radix(&h[4..6], 16)) {
            return (r, g, b);
        }
    }
    (255, 255, 255)
}

pub fn to_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

fn alpha_byte(opacity_percent: u32) -> u8 {
    ((opacity_percent.min(100) as f64) * 255.0 / 100.0).round() as u8
}

/// mpv colour string: #AARRGGBB, alpha 255 = fully visible.
pub fn mpv_color(color: &str, opacity_percent: u32) -> String {
    let (r, g, b) = parse_color(color);
    format!("#{:02X}{:02X}{:02X}{:02X}", alpha_byte(opacity_percent), r, g, b)
}

fn yes(b: bool) -> String {
    if b { "yes" } else { "no" }.to_string()
}

/// Everything mpv needs to draw subtitles like the user asked. Safe to apply while playing.
pub fn mpv_options(s: &SubStyle) -> Vec<(String, String)> {
    let mut o: Vec<(String, String)> = vec![
        ("sub-font".into(), if s.font.trim().is_empty() { "Sans".into() } else { s.font.trim().to_string() }),
        ("sub-font-size".into(), s.size.clamp(10, 200).to_string()),
        ("sub-bold".into(), yes(s.bold)),
        ("sub-italic".into(), yes(s.italic)),
        ("sub-color".into(), mpv_color(&s.text_color, s.text_opacity)),
        ("sub-border-color".into(), mpv_color(&s.border_color, 100)),
        ("sub-border-size".into(), format!("{:.1}", s.border_size.clamp(0.0, 12.0))),
        ("sub-shadow-color".into(), mpv_color(&s.shadow_color, 80)),
        ("sub-shadow-offset".into(), format!("{:.1}", s.shadow_offset.clamp(0.0, 10.0))),
        ("sub-pos".into(), s.position.min(100).to_string()),
        ("sub-align-x".into(), if matches!(s.align_x.as_str(), "left" | "right") { s.align_x.clone() } else { "center".to_string() }),
        ("sub-margin-x".into(), s.margin_x.min(600).to_string()),
        ("sub-ass-override".into(), if s.force_style { "force" } else { "scale" }.into()),
        ("sub-ass-style-overrides".into(), if s.underline { "Underline=1".into() } else { String::new() }),
    ];
    if s.bg_enabled {
        o.push(("sub-border-style".into(), "background-box".into()));
        o.push(("sub-back-color".into(), mpv_color(&s.bg_color, s.bg_opacity)));
    } else {
        o.push(("sub-border-style".into(), "outline-and-shadow".into()));
        o.push(("sub-back-color".into(), "#00000000".into()));
    }
    o
}

/// CSS for a preview label inside the settings window.
pub fn preview_css(s: &SubStyle) -> String {
    let (r, g, b) = parse_color(&s.text_color);
    let (br, bg, bb) = parse_color(&s.border_color);
    let (sr, sg, sb) = parse_color(&s.shadow_color);
    let (kr, kg, kb) = parse_color(&s.bg_color);
    let a = s.text_opacity.min(100) as f64 / 100.0;
    let bw = s.border_size;
    let mut shadows: Vec<String> = Vec::new();
    if bw > 0.0 {
        for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
            shadows.push(format!("{:.1}px {:.1}px 0 rgb({},{},{})", dx * bw * 0.6, dy * bw * 0.6, br, bg, bb));
        }
    }
    if s.shadow_offset > 0.0 {
        shadows.push(format!("{:.1}px {:.1}px 2px rgba({},{},{},0.8)", s.shadow_offset, s.shadow_offset, sr, sg, sb));
    }
    let mut css = format!(
        "label.sub-preview {{ color: rgba({},{},{},{:.2}); font-family: \"{}\"; font-size: {}px; font-weight: {}; font-style: {}; text-decoration: {}; padding: 6px 12px; border-radius: 4px;",
        r,
        g,
        b,
        a,
        s.font.trim().replace('"', ""),
        (s.size as f64 * 0.42).round(),
        if s.bold { "bold" } else { "normal" },
        if s.italic { "italic" } else { "normal" },
        if s.underline { "underline" } else { "none" },
    );
    if !shadows.is_empty() {
        css.push_str(&format!(" text-shadow: {};", shadows.join(", ")));
    }
    if s.bg_enabled {
        css.push_str(&format!(" background-color: rgba({},{},{},{:.2});", kr, kg, kb, s.bg_opacity.min(100) as f64 / 100.0));
    }
    css.push_str(" }");
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt<'a>(o: &'a [(String, String)], k: &str) -> &'a str {
        o.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()).unwrap_or("")
    }

    #[test]
    fn colours() {
        assert_eq!(parse_color("#FF8000"), (255, 128, 0));
        assert_eq!(parse_color("zzz"), (255, 255, 255));
        assert_eq!(parse_color("#ÿÿÿÿÿÿ"), (255, 255, 255));
        assert_eq!(crate::controller::wordmix::ass_color("#éééé0000").0, "&HFFFFFF&");
        assert_eq!(mpv_color("#FF0000", 100), "#FFFF0000");
        assert_eq!(mpv_color("#00FF00", 0), "#0000FF00");
        assert_eq!(mpv_color("#0000FF", 50), "#800000FF");
        assert_eq!(to_hex(1, 2, 255), "#0102FF");
    }

    #[test]
    fn plain_style() {
        let o = mpv_options(&SubStyle::default());
        assert_eq!(opt(&o, "sub-border-style"), "outline-and-shadow");
        assert_eq!(opt(&o, "sub-color"), "#FFFFFFFF");
        assert_eq!(opt(&o, "sub-ass-style-overrides"), "");
        assert_eq!(opt(&o, "sub-bold"), "no");
    }

    #[test]
    fn box_underline_and_opacity() {
        let s = SubStyle { bg_enabled: true, bg_color: "#101010".into(), bg_opacity: 40, underline: true, bold: true, text_opacity: 90, ..SubStyle::default() };
        let o = mpv_options(&s);
        assert_eq!(opt(&o, "sub-border-style"), "background-box");
        assert_eq!(opt(&o, "sub-back-color"), "#66101010");
        assert_eq!(opt(&o, "sub-ass-style-overrides"), "Underline=1");
        assert_eq!(opt(&o, "sub-color"), "#E6FFFFFF");
        assert_eq!(opt(&o, "sub-bold"), "yes");
    }

    #[test]
    fn preview_mentions_the_choices() {
        let s = SubStyle { underline: true, bg_enabled: true, ..SubStyle::default() };
        let c = preview_css(&s);
        assert!(c.contains("underline") && c.contains("background-color") && c.contains("text-shadow"));
    }
}
