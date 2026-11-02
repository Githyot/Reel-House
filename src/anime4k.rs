//! Anime4K (https://github.com/bloc97/Anime4K) real-time upscaling shaders for mpv.
//! "HQ" mirrors the upstream high-end template (GTX 1080 / RTX 2070 / RX 6600 XT and up).
//! "Fast" mirrors the low-end one (GTX 980 / GTX 1060 / RX 570).
use crate::http::Http;
use crate::settings::Upscaling;
use std::fs;
use std::path::Path;

pub const MODES: [&str; 6] = ["A", "B", "C", "A+A", "B+B", "C+A"];

pub fn mode_info(mode: &str) -> &'static str {
    match mode {
        "A" => "Restore, then upscale. Best for 1080p anime with compression artifacts.",
        "B" => "Soft restore, then upscale. For sources with slight blur or ringing.",
        "C" => "Denoise and upscale. Suits clean sources and grainy ones alike.",
        "A+A" => "Two restore passes. Use only when upscaling by 2x or more.",
        "B+B" => "Two soft restore passes. Use only when upscaling by 2x or more.",
        "C+A" => "Denoise, then restore. Use only when upscaling by 2x or more.",
        _ => "",
    }
}

const CLAMP: &str = "Clamp_Highlights";
const DS2: &str = "AutoDownscalePre_x2";
const DS4: &str = "AutoDownscalePre_x4";

pub fn chain_names(mode: &str, quality: &str, clamp: bool) -> Result<Vec<&'static str>, String> {
    let hq = quality != "Fast";
    let mut c: Vec<&'static str> = match (hq, mode) {
        (true, "A") => vec![CLAMP, "Restore_CNN_VL", "Upscale_CNN_x2_VL", DS2, DS4, "Upscale_CNN_x2_M"],
        (true, "B") => vec![CLAMP, "Restore_CNN_Soft_VL", "Upscale_CNN_x2_VL", DS2, DS4, "Upscale_CNN_x2_M"],
        (true, "C") => vec![CLAMP, "Upscale_Denoise_CNN_x2_VL", DS2, DS4, "Upscale_CNN_x2_M"],
        (true, "A+A") => vec![CLAMP, "Restore_CNN_VL", "Upscale_CNN_x2_VL", "Restore_CNN_M", DS2, DS4, "Upscale_CNN_x2_M"],
        (true, "B+B") => vec![CLAMP, "Restore_CNN_Soft_VL", "Upscale_CNN_x2_VL", "Restore_CNN_Soft_M", DS2, DS4, "Upscale_CNN_x2_M"],
        (true, "C+A") => vec![CLAMP, "Upscale_Denoise_CNN_x2_VL", DS2, DS4, "Restore_CNN_M", "Upscale_CNN_x2_M"],
        (false, "A") => vec![CLAMP, "Restore_CNN_M", "Upscale_CNN_x2_M", DS2, DS4, "Upscale_CNN_x2_S"],
        (false, "B") => vec![CLAMP, "Restore_CNN_Soft_M", "Upscale_CNN_x2_M", DS2, DS4, "Upscale_CNN_x2_S"],
        (false, "C") => vec![CLAMP, "Upscale_Denoise_CNN_x2_M", DS2, DS4, "Upscale_CNN_x2_S"],
        (false, "A+A") => vec![CLAMP, "Restore_CNN_M", "Upscale_CNN_x2_M", "Restore_CNN_S", DS2, DS4, "Upscale_CNN_x2_S"],
        (false, "B+B") => vec![CLAMP, "Restore_CNN_Soft_M", "Upscale_CNN_x2_M", "Restore_CNN_Soft_S", DS2, DS4, "Upscale_CNN_x2_S"],
        (false, "C+A") => vec![CLAMP, "Upscale_Denoise_CNN_x2_M", DS2, DS4, "Restore_CNN_S", "Upscale_CNN_x2_S"],
        _ => return Err(format!("Unknown Anime4K mode \"{}\".", mode)),
    };
    if !clamp {
        c.retain(|n| *n != CLAMP);
    }
    Ok(c)
}

pub fn file_for(name: &str) -> String {
    format!("Anime4K_{}.glsl", name)
}

pub fn required_files() -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    for q in ["HQ", "Fast"] {
        for m in MODES {
            for n in chain_names(m, q, true).unwrap_or_default() {
                let f = file_for(n);
                if !all.contains(&f) {
                    all.push(f);
                }
            }
        }
    }
    all.sort();
    all
}

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// mpv commands that replace the active shader list with the chosen chain.
/// The list is cleared and appended to, which avoids the platform-specific path separator.
pub fn mpv_commands(dir: &Path, u: &Upscaling) -> Result<Vec<Vec<String>>, String> {
    let mut out = vec![vec!["change-list".to_string(), "glsl-shaders".to_string(), "clr".to_string(), String::new()]];
    for n in chain_names(&u.mode, &u.quality, u.clamp_highlights)? {
        out.push(vec!["change-list".into(), "glsl-shaders".into(), "append".into(), slash(&dir.join(file_for(n)))]);
    }
    Ok(out)
}

pub fn clear_command() -> Vec<String> {
    vec!["change-list".into(), "glsl-shaders".into(), "clr".into(), String::new()]
}

/// Options set once when mpv starts.
pub fn mpv_options(u: &Upscaling) -> Vec<(String, String)> {
    let mut o = Vec::new();
    if u.gpu_hq {
        o.push(("profile".to_string(), "gpu-hq".to_string()));
    }
    if !u.scaler.is_empty() {
        o.push(("scale".to_string(), u.scaler.clone()));
        o.push(("cscale".to_string(), u.scaler.clone()));
    }
    o
}

pub fn should_apply(u: &Upscaling, source_height: Option<i64>) -> bool {
    if !u.enabled {
        return false;
    }
    match source_height {
        Some(h) if h > 0 => h <= u.max_source_height as i64,
        _ => true,
    }
}

pub fn missing_for(dir: &Path, u: &Upscaling) -> Vec<String> {
    chain_names(&u.mode, &u.quality, u.clamp_highlights)
        .unwrap_or_default()
        .iter()
        .map(|n| file_for(n))
        .filter(|f| !dir.join(f).exists())
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct ShaderStatus {
    pub total: usize,
    pub present: usize,
}

pub fn status(dir: &Path) -> ShaderStatus {
    let need = required_files();
    ShaderStatus { total: need.len(), present: need.iter().filter(|f| dir.join(f).exists()).count() }
}

/// Writes the shader files that were built into the program. Existing files of the same size are kept.
pub fn extract_embedded(files: &[(&str, &[u8])], dir: &Path) -> Result<usize, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut n = 0;
    for (name, bytes) in files {
        let p = dir.join(name);
        if fs::metadata(&p).map(|m| m.len() == bytes.len() as u64).unwrap_or(false) {
            continue;
        }
        fs::write(&p, bytes).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

/// Fallback when the shaders were not built in (Android builds before 2026-11-02 had none). First the small zip of
/// Anime4K's latest Release (about 100 KB, only the shaders); if that fails, the whole repository as tar.gz. Both are
/// saved to a file first (`Http::download`, long timeout, size checked) and checked to be complete before anything is
/// unpacked: reading the repository archive into memory in one request was cut short on Android (Waydroid), and the
/// gzip reader then failed with "incomplete deflate stream".
pub fn install_from_github(http: &dyn Http, dir: &Path, status: &dyn Fn(String)) -> Result<usize, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let wanted = required_files();
    let tmp = dir.join(".anime4k-download.tmp");
    let mut errors = Vec::new();
    for attempt in 0..2 {
        status(if attempt == 0 { "Downloading the Anime4K shaders (GitHub Release)".to_string() } else { "Trying the Release again".to_string() });
        match from_release(http, dir, &tmp, &wanted) {
            Ok(n) if n > 0 => {
                let _ = fs::remove_file(&tmp);
                return finish(dir, n);
            }
            Ok(_) => errors.push("the Release zip has no Anime4K shaders".to_string()),
            Err(e) => errors.push(format!("Release: {}", e)),
        }
    }
    status("Downloading the whole Anime4K repository (larger, may take a while)".to_string());
    let r = from_repository(http, dir, &tmp, &wanted, status);
    let _ = fs::remove_file(&tmp);
    match r {
        Ok(n) if n > 0 => finish(dir, n),
        Ok(_) => Err(format!("No Anime4K shaders found in the download ({}).", errors.join("; "))),
        Err(e) => {
            errors.push(format!("repository: {}", e));
            Err(format!("Could not download the Anime4K shaders: {}", errors.join("; ")))
        }
    }
}

fn finish(dir: &Path, n: usize) -> Result<usize, String> {
    let st = status(dir);
    if st.present < st.total {
        return Err(format!("Only {} of {} Anime4K shaders were found ({} written).", st.present, st.total, n));
    }
    Ok(n)
}

fn from_release(http: &dyn Http, dir: &Path, tmp: &Path, wanted: &[String]) -> Result<usize, String> {
    let j = crate::http::get_json(http, "https://api.github.com/repos/bloc97/Anime4K/releases/latest", 30)?;
    let url = j
        .get("assets")
        .items()
        .iter()
        .map(|a| a.get("browser_download_url").str())
        .find(|u| u.to_lowercase().ends_with(".zip"))
        .ok_or("no zip in the latest Release")?;
    http.download(&url, tmp, &|_, _| {})?;
    let zip = fs::read(tmp).map_err(|e| e.to_string())?;
    let mut n = 0;
    for (name, data) in zip_files(&zip, wanted)? {
        fs::write(dir.join(&name), data).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

fn from_repository(http: &dyn Http, dir: &Path, tmp: &Path, wanted: &[String], status: &dyn Fn(String)) -> Result<usize, String> {
    let mb = |b: u64| b / (1024 * 1024);
    http.download("https://codeload.github.com/bloc97/Anime4K/tar.gz/refs/heads/master", tmp, &|done, _| status(format!("Downloading the repository: {} MB", mb(done))))?;
    status("Extracting".to_string());
    let f = fs::File::open(tmp).map_err(|e| e.to_string())?;
    let mut n = 0;
    let mut write_err = None;
    tar_each(flate2::read::GzDecoder::new(std::io::BufReader::new(f)), &mut |name, data| {
        let base = name.rsplit('/').next().unwrap_or(name).to_string();
        if wanted.contains(&base) {
            match fs::write(dir.join(&base), data) {
                Ok(()) => n += 1,
                Err(e) => write_err = Some(e.to_string()),
            }
        }
    }, &|name| wanted.iter().any(|w| name.ends_with(w.as_str())))
    .map_err(|e| format!("the archive is damaged or incomplete ({}); try again", e))?;
    if let Some(e) = write_err {
        return Err(e);
    }
    Ok(n)
}

/// The wanted files of a zip archive (matched by file name, folders ignored): stored or deflated entries, read through
/// the central directory.
fn zip_files(z: &[u8], wanted: &[String]) -> Result<Vec<(String, Vec<u8>)>, String> {
    use std::io::Read;
    let u16_at = |i: usize| -> Option<usize> { z.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) };
    let u32_at = |i: usize| -> Option<usize> { z.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) };
    let bad = || "the zip is damaged or incomplete".to_string();
    let eocd = (0..z.len().saturating_sub(21)).rev().take(65_557).find(|&i| z[i..].starts_with(b"PK\x05\x06")).ok_or_else(bad)?;
    let count = u16_at(eocd + 10).ok_or_else(bad)?;
    let mut p = u32_at(eocd + 16).ok_or_else(bad)?;
    let mut out = Vec::new();
    for _ in 0..count {
        if !z.get(p..).map(|r| r.starts_with(b"PK\x01\x02")).unwrap_or(false) {
            return Err(bad());
        }
        let method = u16_at(p + 10).ok_or_else(bad)?;
        let csize = u32_at(p + 20).ok_or_else(bad)?;
        let (nlen, xlen, clen) = (u16_at(p + 28).ok_or_else(bad)?, u16_at(p + 30).ok_or_else(bad)?, u16_at(p + 32).ok_or_else(bad)?);
        let local = u32_at(p + 42).ok_or_else(bad)?;
        let name = String::from_utf8_lossy(z.get(p + 46..p + 46 + nlen).ok_or_else(bad)?).to_string();
        p += 46 + nlen + xlen + clen;
        let base = name.rsplit('/').next().unwrap_or(&name).to_string();
        if !wanted.contains(&base) {
            continue;
        }
        let start = local + 30 + u16_at(local + 26).ok_or_else(bad)? + u16_at(local + 28).ok_or_else(bad)?;
        let raw = z.get(start..start + csize).ok_or_else(bad)?;
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut v = Vec::new();
                flate2::read::DeflateDecoder::new(raw).read_to_end(&mut v).map_err(|e| format!("{} ({})", bad(), e))?;
                v
            }
            m => return Err(format!("unsupported zip compression {}", m)),
        };
        out.push((base, data));
    }
    Ok(out)
}

/// Walks a tar archive as it is read: `keep(path)` says which plain files to read, `on_file(path, contents)` gets
/// them; the rest is skipped without being kept in memory. Reads ustar names (prefix + name), GNU long names (`L`) and
/// pax `path=` records (`x`), which GitHub's archives use for long paths. An archive that ends early is an error.
fn tar_each(mut r: impl std::io::Read, on_file: &mut dyn FnMut(&str, &[u8]), keep: &dyn Fn(&str) -> bool) -> std::io::Result<()> {
    use std::io::Read;
    fn field(b: &[u8]) -> String {
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf8_lossy(&b[..end]).to_string()
    }
    fn octal(b: &[u8]) -> u64 {
        u64::from_str_radix(field(b).trim(), 8).unwrap_or(0)
    }
    let mut long: Option<String> = None;
    let mut h = [0u8; 512];
    loop {
        r.read_exact(&mut h)?;
        if h.iter().all(|&c| c == 0) {
            return Ok(());
        }
        let size = octal(&h[124..136]);
        let padded = size.div_ceil(512) * 512;
        let kind = h[156];
        let name = match kind {
            b'0' | 0 => Some(long.take().unwrap_or_else(|| {
                let (prefix, name) = (field(&h[345..500]), field(&h[0..100]));
                if prefix.is_empty() { name } else { format!("{}/{}", prefix, name) }
            })),
            _ => None,
        };
        let read_body = matches!(kind, b'L' | b'x') || name.as_deref().map(keep).unwrap_or(false);
        if read_body {
            let mut body = vec![0u8; size as usize];
            r.read_exact(&mut body)?;
            let skip = padded - size;
            std::io::copy(&mut (&mut r).take(skip), &mut std::io::sink())?;
            match (kind, name) {
                (b'L', _) => long = Some(field(&body)),
                (b'x', _) => {
                    for rec in String::from_utf8_lossy(&body).lines() {
                        if let Some(i) = rec.find(" path=") {
                            long = Some(rec[i + 6..].to_string());
                        }
                    }
                }
                (_, Some(n)) => on_file(&n, &body),
                _ => {}
            }
        } else {
            if kind != b'0' && kind != 0 {
                long = None;
            }
            let got = std::io::copy(&mut (&mut r).take(padded), &mut std::io::sink())?;
            if got < padded {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "archive ends early"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn u() -> Upscaling {
        Settings::default().upscaling
    }

    #[test]
    fn mode_a_hq_matches_the_upstream_template() {
        assert_eq!(
            chain_names("A", "HQ", true).unwrap(),
            vec!["Clamp_Highlights", "Restore_CNN_VL", "Upscale_CNN_x2_VL", "AutoDownscalePre_x2", "AutoDownscalePre_x4", "Upscale_CNN_x2_M"]
        );
        assert!(!chain_names("A", "HQ", false).unwrap().contains(&"Clamp_Highlights"));
        assert!(chain_names("Z", "HQ", true).is_err());
    }

    #[test]
    fn every_chain_ends_with_an_upscaler() {
        for q in ["HQ", "Fast"] {
            for m in MODES {
                let c = chain_names(m, q, true).unwrap();
                assert!(c.last().unwrap().starts_with("Upscale_CNN_x2_"), "{} {}", q, m);
                assert!(!mode_info(m).is_empty());
            }
        }
        assert!(required_files().iter().all(|f| f.starts_with("Anime4K_") && f.ends_with(".glsl")));
    }

    #[test]
    fn builds_mpv_commands_with_forward_slashes() {
        let mut up = u();
        up.mode = "C".into();
        up.quality = "Fast".into();
        let c = mpv_commands(Path::new("C:\\Users\\me\\shaders"), &up).unwrap();
        assert_eq!(c[0], vec!["change-list", "glsl-shaders", "clr", ""]);
        assert_eq!(c[1][3], "C:/Users/me/shaders/Anime4K_Clamp_Highlights.glsl");
        assert_eq!(c.len(), 1 + chain_names("C", "Fast", true).unwrap().len());
    }

    #[test]
    fn options_and_height_gate() {
        let mut up = u();
        assert!(!should_apply(&up, Some(720)));
        up.enabled = true;
        assert!(should_apply(&up, Some(720)));
        assert!(!should_apply(&up, Some(2160)));
        assert!(should_apply(&up, None));
        let o = mpv_options(&up);
        assert!(o.contains(&("profile".to_string(), "gpu-hq".to_string())));
        assert!(o.contains(&("scale".to_string(), "ewa_lanczossharp".to_string())));
    }

    #[test]
    fn extracts_and_reports_status() {
        let dir = std::env::temp_dir().join(format!("rh-a4k-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(status(&dir).present, 0);
        let names = required_files();
        let owned: Vec<(String, Vec<u8>)> = names.iter().map(|n| (n.clone(), b"//".to_vec())).collect();
        let refs: Vec<(&str, &[u8])> = owned.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
        assert_eq!(extract_embedded(&refs, &dir).unwrap(), names.len());
        assert_eq!(extract_embedded(&refs, &dir).unwrap(), 0);
        let st = status(&dir);
        assert_eq!(st.present, st.total);
        assert!(missing_for(&dir, &u()).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_tar_names() {
        fn header(name: &str, kind: u8, size: usize) -> Vec<u8> {
            let mut h = vec![0u8; 512];
            h[..name.len()].copy_from_slice(name.as_bytes());
            let sz = format!("{:011o}", size);
            h[124..135].copy_from_slice(sz.as_bytes());
            h[156] = kind;
            h
        }
        let mut t = Vec::new();
        let pax = "30 path=a/glsl/Long_Name.glsl\n";
        t.extend(header("pax", b'x', pax.len()));
        let mut b = pax.as_bytes().to_vec();
        b.resize(512, 0);
        t.extend(b);
        t.extend(header("short", b'0', 3));
        let mut b = b"abc".to_vec();
        b.resize(512, 0);
        t.extend(b);
        t.extend(header("x/plain.glsl", b'0', 0));
        t.extend(vec![0u8; 1024]);
        let mut f: Vec<(String, Vec<u8>)> = Vec::new();
        tar_each(&t[..], &mut |n, d| f.push((n.to_string(), d.to_vec())), &|_| true).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].0, "a/glsl/Long_Name.glsl");
        assert_eq!(f[0].1, b"abc");
        assert_eq!(f[1].0, "x/plain.glsl");
        // only what `keep` wants is read
        let mut g = Vec::new();
        tar_each(&t[..], &mut |n, _| g.push(n.to_string()), &|n| n.ends_with("plain.glsl")).unwrap();
        assert_eq!(g, vec!["x/plain.glsl"]);
        // an archive cut short is an error, not a silent partial list
        assert!(tar_each(&t[..1100], &mut |_, _| {}, &|_| true).is_err());
    }

    fn zip_of(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        use std::io::Write;
        let (mut z, mut cd) = (Vec::new(), Vec::new());
        for (name, data, deflate) in files {
            let body = if *deflate {
                let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                e.write_all(data).unwrap();
                e.finish().unwrap()
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let off = z.len() as u32;
            z.extend(b"PK\x03\x04");
            z.extend([20, 0, 0, 0]);
            z.extend(method.to_le_bytes());
            z.extend([0u8; 8]);
            z.extend((body.len() as u32).to_le_bytes());
            z.extend((data.len() as u32).to_le_bytes());
            z.extend((name.len() as u16).to_le_bytes());
            z.extend([0, 0]);
            z.extend(name.as_bytes());
            z.extend(&body);
            cd.extend(b"PK\x01\x02");
            cd.extend([20, 0, 20, 0, 0, 0]);
            cd.extend(method.to_le_bytes());
            cd.extend([0u8; 8]);
            cd.extend((body.len() as u32).to_le_bytes());
            cd.extend((data.len() as u32).to_le_bytes());
            cd.extend((name.len() as u16).to_le_bytes());
            cd.extend([0u8; 12]);
            cd.extend(off.to_le_bytes());
            cd.extend(name.as_bytes());
        }
        let cd_off = z.len() as u32;
        z.extend(&cd);
        z.extend(b"PK\x05\x06");
        z.extend([0u8; 4]);
        z.extend((files.len() as u16).to_le_bytes());
        z.extend((files.len() as u16).to_le_bytes());
        z.extend((cd.len() as u32).to_le_bytes());
        z.extend(cd_off.to_le_bytes());
        z.extend([0, 0]);
        z
    }

    #[test]
    fn reads_wanted_zip_entries() {
        let z = zip_of(&[("glsl/Anime4K_Upscale_CNN_x2_M.glsl", b"//!HOOK MAIN\nshader", true), ("readme.txt", b"x", false), ("Anime4K_Clamp_Highlights.glsl", b"clamp", false)]);
        let want = vec!["Anime4K_Upscale_CNN_x2_M.glsl".to_string(), "Anime4K_Clamp_Highlights.glsl".to_string()];
        let f = zip_files(&z, &want).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0], ("Anime4K_Upscale_CNN_x2_M.glsl".to_string(), b"//!HOOK MAIN\nshader".to_vec()));
        assert_eq!(f[1].1, b"clamp");
        // a zip cut short is reported, not unpacked half
        assert!(zip_files(&z[..z.len() - 30], &want).is_err());
    }
}
