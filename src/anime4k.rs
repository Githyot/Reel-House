//! Anime4K (https://github.com/bloc97/Anime4K) real-time upscaling shaders for mpv.
//! "HQ" mirrors the upstream high-end template (GTX 1080 / RTX 2070 / RX 6600 XT and up).
//! "Fast" mirrors the low-end one (GTX 980 / GTX 1060 / RX 570).
use crate::http::{Http, Request};
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

/// Fallback when the shaders were not built in: downloads the Anime4K repository and copies the shaders. The archive
/// is unpacked here (gzip + tar read in the app), so it works on Android too, which has no `tar` to run.
pub fn install_from_github(http: &dyn Http, dir: &Path, status: &dyn Fn(String)) -> Result<usize, String> {
    status("Downloading Anime4K shaders from GitHub".to_string());
    let r = http.send(&Request::get("https://codeload.github.com/bloc97/Anime4K/tar.gz/refs/heads/master").timeout(180))?;
    if !r.ok() {
        return Err(format!("GitHub answered {}.", r.status));
    }
    status("Extracting".to_string());
    let mut tar = Vec::new();
    {
        use std::io::Read;
        flate2::read::GzDecoder::new(&r.body[..]).read_to_end(&mut tar).map_err(|e| format!("Could not extract the archive: {}", e))?;
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let wanted = required_files();
    let mut n = 0;
    for (name, data) in tar_files(&tar) {
        let base = name.rsplit('/').next().unwrap_or(&name).to_string();
        if wanted.contains(&base) {
            fs::write(dir.join(&base), data).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}

/// The plain files of a tar archive: (path, contents). Reads ustar names (prefix + name), GNU long names (`L`) and
/// pax `path=` records (`x`), which GitHub's archives use for long paths.
fn tar_files(t: &[u8]) -> Vec<(String, &[u8])> {
    fn field(b: &[u8]) -> String {
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf8_lossy(&b[..end]).to_string()
    }
    fn octal(b: &[u8]) -> usize {
        let s = field(b);
        usize::from_str_radix(s.trim(), 8).unwrap_or(0)
    }
    let mut out = Vec::new();
    let mut pos = 0;
    let mut long: Option<String> = None;
    while pos + 512 <= t.len() {
        let h = &t[pos..pos + 512];
        if h.iter().all(|&c| c == 0) {
            break;
        }
        let size = octal(&h[124..136]);
        let kind = h[156];
        let start = pos + 512;
        let end = (start + size).min(t.len());
        let body = &t[start..end];
        pos = start + size.div_ceil(512) * 512;
        match kind {
            b'L' => long = Some(field(body)),
            b'x' => {
                for rec in String::from_utf8_lossy(body).lines() {
                    if let Some(i) = rec.find(" path=") {
                        long = Some(rec[i + 6..].to_string());
                    }
                }
            }
            b'0' | 0 => {
                let name = long.take().unwrap_or_else(|| {
                    let (prefix, name) = (field(&h[345..500]), field(&h[0..100]));
                    if prefix.is_empty() { name } else { format!("{}/{}", prefix, name) }
                });
                out.push((name, body));
            }
            _ => long = None,
        }
    }
    out
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
        let f = tar_files(&t);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].0, "a/glsl/Long_Name.glsl");
        assert_eq!(f[0].1, b"abc");
        assert_eq!(f[1].0, "x/plain.glsl");
    }
}
