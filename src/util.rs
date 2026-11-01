//! Small helpers with no dependencies.
use std::path::PathBuf;

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Like `url_encode` but also escapes ':' (for query strings and form bodies).
pub fn form_encode(s: &str) -> String {
    url_encode(s).replace(':', "%3A")
}

pub fn form_body(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{}={}", form_encode(k), form_encode(v))).collect::<Vec<_>>().join("&")
}

pub fn fnv_hex(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

/// "pt-BR", "por", "pt_BR" and "PT" all become "pt".
pub fn lang_base(code: &str) -> String {
    let head = code.trim().to_lowercase().replace('_', "-");
    let head = head.split('-').next().unwrap_or("").to_string();
    let mapped = match head.as_str() {
        "eng" => "en", "por" | "pob" => "pt", "spa" => "es", "fre" | "fra" => "fr", "ger" | "deu" => "de",
        "ita" => "it", "rus" => "ru", "jpn" => "ja", "kor" => "ko", "chi" | "zho" => "zh", "ara" => "ar",
        "hin" => "hi", "tur" => "tr", "pol" => "pl", "dut" | "nld" => "nl", "swe" => "sv", "nor" => "no",
        "dan" => "da", "fin" => "fi", "cze" | "ces" => "cs", "hun" => "hu", "rum" | "ron" => "ro",
        "gre" | "ell" => "el", "heb" => "he", "tha" => "th", "vie" => "vi", "ind" => "id", "ukr" => "uk",
        "bul" => "bg", "hrv" => "hr", "srp" => "sr", "slv" => "sl", "cat" => "ca",
        other => other,
    };
    mapped.to_string()
}

/// The name of a language as its speakers write it ("pt" and "por" both give "Português").
pub fn lang_name(code: &str) -> String {
    let base = lang_base(code);
    let name = match base.as_str() {
        "en" => "English", "pt" => "Português", "es" => "Español", "fr" => "Français", "de" => "Deutsch",
        "it" => "Italiano", "ru" => "Русский", "ja" => "日本語", "ko" => "한국어", "zh" => "中文",
        "ar" => "العربية", "hi" => "हिन्दी", "tr" => "Türkçe", "pl" => "Polski", "nl" => "Nederlands",
        "sv" => "Svenska", "no" => "Norsk", "da" => "Dansk", "fi" => "Suomi", "cs" => "Čeština",
        "hu" => "Magyar", "ro" => "Română", "el" => "Ελληνικά", "he" => "עברית", "th" => "ไทย",
        "vi" => "Tiếng Việt", "id" => "Bahasa Indonesia", "uk" => "Українська", "bg" => "Български",
        "hr" => "Hrvatski", "sr" => "Српски", "sl" => "Slovenščina", "ca" => "Català", "fa" | "per" => "فارسی",
        "ms" => "Bahasa Melayu", "" => "Unknown",
        other => return other.to_uppercase(),
    };
    name.to_string()
}

pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("REELHOUSE_DATA") {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        if let Ok(p) = std::env::var("APPDATA") {
            return PathBuf::from(p).join("Reelhouse");
        }
    }
    if let Ok(p) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(p).join("reelhouse");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".config").join("reelhouse");
    }
    PathBuf::from(".reelhouse")
}

/// Locale such as "pt-BR" from the environment, or "en" when it cannot be read.
pub fn system_lang() -> String {
    // The window sets REELHOUSE_SYSTEM_LANG from QLocale: Windows, Android and iOS have no LANG variable.
    if let Ok(v) = std::env::var("REELHOUSE_SYSTEM_LANG") {
        if !v.trim().is_empty() {
            return lang_base(&v);
        }
    }
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(var) {
            let base = v.split('.').next().unwrap_or("").to_string();
            if !base.is_empty() && base != "C" && base != "POSIX" {
                return lang_base(&base);
            }
        }
    }
    "en".to_string()
}

/// Calendar date and time (UTC) of a Unix time: (year, month, day, hour, minute, second).
pub fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d, (rem / 3600) as u32, ((rem / 60) % 60) as u32, (rem % 60) as u32)
}

/// "2h 05m", "45m" or "3d 4h" for a number of minutes.
pub fn fmt_minutes(mins: u64) -> String {
    if mins >= 24 * 60 * 2 {
        format!("{}d {}h", mins / 1440, (mins / 60) % 24)
    } else if mins >= 60 {
        format!("{}h {:02}m", mins / 60, mins % 60)
    } else {
        format!("{}m", mins)
    }
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn fmt_clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// True when `name` is a plain file name for the Study media folder: one ordinary path part, nothing that climbs out
/// of the folder or points somewhere else. Checked by hand as well as with `Path`, because on Linux `Path` does not see
/// the Windows forms (C:x relative to a drive, \\host\share, a\b) that would escape the folder there.
pub fn safe_media_name(name: &str) -> bool {
    if name.is_empty() || name.contains(['/', '\\', ':', '\0']) || name == "." || name == ".." {
        return false;
    }
    let mut parts = std::path::Path::new(name).components();
    matches!((parts.next(), parts.next()), (Some(std::path::Component::Normal(_)), None))
}

/// The local path of a "file://" URL ("file:///C:/a%20b.srt" is "C:/a b.srt", "file:///home/x.srt" is "/home/x.srt");
/// anything else is returned as it is (mpv usually gives plain paths already).
pub fn file_url_path(s: &str) -> String {
    let Some(rest) = s.strip_prefix("file://") else { return s.to_string() };
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    // "/C:/x" -> "C:/x": the slash before a drive letter belongs to the URL, not to the path
    let b = rest.as_bytes();
    let rest = if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' { &rest[1..] } else { rest };
    // percent escapes (a space is %20 in a URL)
    let mut out: Vec<u8> = Vec::with_capacity(rest.len());
    let b = rest.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && rest.is_char_boundary(i + 3) {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_names_and_file_urls() {
        assert!(safe_media_name("a b.jpg"));
        assert!(!safe_media_name("C:x.jpg"));
        assert!(!safe_media_name("\\\\host\\share\\x.jpg"));
        assert!(!safe_media_name("../x.jpg"));
        assert!(!safe_media_name("/etc/x"));
        assert!(!safe_media_name(".."));
        assert!(!safe_media_name(""));
        assert_eq!(file_url_path("file:///C:/a%20b.srt"), "C:/a b.srt");
        assert_eq!(file_url_path("file:///home/x.srt"), "/home/x.srt");
        assert_eq!(file_url_path("C:\\x.srt"), "C:\\x.srt");
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_700_000_000), (2023, 11, 14, 22, 13, 20));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn minutes_text() {
        assert_eq!(fmt_minutes(45), "45m");
        assert_eq!(fmt_minutes(125), "2h 05m");
        assert_eq!(fmt_minutes(60 * 50), "2d 2h");
    }

    #[test]
    fn encodes_urls_and_forms() {
        assert_eq!(url_encode("tt1:1:2"), "tt1:1:2");
        assert_eq!(url_encode("a b&c"), "a%20b%26c");
        assert_eq!(form_body(&[("link", "https://x/y?z=1"), ("a", "b c")]), "link=https%3A%2F%2Fx%2Fy%3Fz%3D1&a=b%20c");
    }

    #[test]
    fn maps_languages() {
        assert_eq!(lang_base("pt-BR"), "pt");
        assert_eq!(lang_base("eng"), "en");
        assert_eq!(lang_base("pt_BR"), "pt");
        assert_eq!(lang_base(""), "");
    }

    #[test]
    fn names_languages() {
        assert_eq!(lang_name("por"), "Português");
        assert_eq!(lang_name("pt-BR"), "Português");
        assert_eq!(lang_name("xx"), "XX");
        assert_eq!(lang_name(""), "Unknown");
    }

    #[test]
    fn formats_clock() {
        assert_eq!(fmt_clock(65.0), "1:05");
        assert_eq!(fmt_clock(3725.0), "1:02:05");
    }
}

/// MD5 as lowercase hex. Anime Skip's login asks for the MD5 of the password; nothing else uses it.
pub fn md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11,
        16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32).collect();
    let (mut a0, mut b0, mut c0, mut d0) = (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_le_bytes());
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk.chunks(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (mut f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    [a0, b0, c0, d0].iter().flat_map(|v| v.to_le_bytes()).map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod md5_tests {
    use super::md5_hex;
    #[test]
    fn known_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(md5_hex(b"The quick brown fox jumps over the lazy dog"), "9e107d9d372bb6826bd81d3542a419d6");
        assert_eq!(md5_hex(&[b'a'; 100]), "36a92cc94a9e0fa21f625f8bfb007adf");
    }
}

/// Page links of imgur (`imgur.com/abc123`) point to a web page; the picture is at i.imgur.com.
pub fn direct_image_url(url: &str) -> String {
    let u = url.trim();
    let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).unwrap_or(u);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    if let Some(path) = rest.strip_prefix("imgur.com/") {
        let id = path.split(['?', '#', '/']).next().unwrap_or("");
        let page = !path.starts_with("a/") && !path.starts_with("gallery/") && !path.starts_with("t/");
        if page && !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric()) {
            let id = id.split('.').next().unwrap_or(id);
            return format!("https://i.imgur.com/{}.png", id);
        }
    }
    u.to_string()
}

#[cfg(test)]
mod image_url_tests {
    use super::direct_image_url;

    #[test]
    fn imgur_pages_become_pictures() {
        assert_eq!(direct_image_url("https://imgur.com/AbC123"), "https://i.imgur.com/AbC123.png");
        assert_eq!(direct_image_url("http://www.imgur.com/AbC123?x=1"), "https://i.imgur.com/AbC123.png");
        assert_eq!(direct_image_url("https://imgur.com/a/AbC123"), "https://imgur.com/a/AbC123");
        assert_eq!(direct_image_url("https://i.imgur.com/AbC123.jpg"), "https://i.imgur.com/AbC123.jpg");
        assert_eq!(direct_image_url("https://cdn.example.com/p.jpg"), "https://cdn.example.com/p.jpg");
    }
}

/// The last step a risky native part (drawing, graphics set-up) started: written to crash.txt if the program dies there.
static STEP: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn breadcrumb(step: &str) {
    if let Ok(mut s) = STEP.lock() {
        *s = step.to_string();
    }
}

pub fn last_breadcrumb() -> String {
    STEP.try_lock().map(|s| s.clone()).unwrap_or_default()
}
