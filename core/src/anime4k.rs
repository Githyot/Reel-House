//! Anime4K (https://github.com/bloc97/Anime4K) real-time upscaling shaders for mpv.
//! The chains mirror the upstream mpv templates: "HQ" is the high-end GPU template
//! (GTX 1080 / RTX 2070 / RX 6600 XT and up) and "Fast" is the low-end one (GTX 980 / GTX 1060 / RX 570).

use crate::error::{Error, Result};
use crate::settings::Upscaling;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

const CLAMP: &str = "Clamp_Highlights";
const DS2: &str = "AutoDownscalePre_x2";
const DS4: &str = "AutoDownscalePre_x4";

pub const MODES: [&str; 6] = ["A", "B", "C", "A+A", "B+B", "C+A"];

pub fn mode_info(mode: &str) -> &'static str {
    match mode {
        "A" => "Restore, then upscale. Best for 1080p anime with compression artifacts.",
        "B" => "Soft restore, then upscale. For sources with slight blur or ringing.",
        "C" => "Denoise and upscale. Suits clean 1080p sources and noisy, grainy ones alike.",
        "A+A" => "Two restore passes. Use only when upscaling by 2x or more.",
        "B+B" => "Two soft restore passes. Use only when upscaling by 2x or more.",
        "C+A" => "Denoise, then restore. Use only when upscaling by 2x or more.",
        _ => "",
    }
}

fn chain(quality: &str, mode: &str) -> Option<Vec<String>> {
    let hq = quality != "Fast";
    let (big, mid_or_small) = if hq { ("VL", "M") } else { ("M", "S") };
    // The names below are built per tier so that HQ and Fast cannot drift apart.
    let restore = |soft: bool, size: &str| -> String { format!("Restore_CNN_{}{}", if soft { "Soft_" } else { "" }, size) };
    let up = |size: &str| -> String { format!("Upscale_CNN_x2_{size}") };
    let list: Vec<String> = match mode {
        "A" => vec![CLAMP.into(), restore(false, big), up(big), DS2.into(), DS4.into(), up(mid_or_small)],
        "B" => vec![CLAMP.into(), restore(true, big), up(big), DS2.into(), DS4.into(), up(mid_or_small)],
        "C" => vec![CLAMP.into(), format!("Upscale_Denoise_CNN_x2_{big}"), DS2.into(), DS4.into(), up(mid_or_small)],
        "A+A" => vec![CLAMP.into(), restore(false, big), up(big), restore(false, mid_or_small), DS2.into(), DS4.into(), up(mid_or_small)],
        "B+B" => vec![CLAMP.into(), restore(true, big), up(big), restore(true, mid_or_small), DS2.into(), DS4.into(), up(mid_or_small)],
        "C+A" => vec![CLAMP.into(), format!("Upscale_Denoise_CNN_x2_{big}"), DS2.into(), DS4.into(), restore(false, mid_or_small), up(mid_or_small)],
        _ => return None,
    };
    Some(list)
}

pub fn chain_names(u: &Upscaling) -> Result<Vec<String>> {
    let c = chain(&u.quality, &u.mode).ok_or_else(|| Error::msg(format!("Unknown Anime4K mode \"{}\".", u.mode)))?;
    Ok(if u.clamp_highlights { c } else { c.into_iter().filter(|n| n != CLAMP).collect() })
}

fn file_for(name: &str) -> String {
    format!("Anime4K_{name}.glsl")
}

pub fn required_files() -> Vec<String> {
    let mut set = BTreeSet::new();
    for q in ["HQ", "Fast"] {
        for m in MODES {
            for n in chain(q, m).unwrap() {
                set.insert(file_for(&n));
            }
        }
    }
    set.into_iter().collect()
}

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub fn chain_paths(dir: &Path, u: &Upscaling) -> Result<Vec<String>> {
    Ok(chain_names(u)?.iter().map(|n| slash(&dir.join(file_for(n)))).collect())
}

/// mpv's glsl-shaders option is a list. Clearing and appending avoids the platform-specific ":" / ";" separator.
pub fn mpv_commands(dir: &Path, u: &Upscaling) -> Result<Vec<Vec<String>>> {
    let mut cmds = vec![vec!["change-list".to_string(), "glsl-shaders".into(), "clr".into(), String::new()]];
    for p in chain_paths(dir, u)? {
        cmds.push(vec!["change-list".into(), "glsl-shaders".into(), "append".into(), p]);
    }
    Ok(cmds)
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Key bindings that match the upstream templates: Ctrl+1..6 pick a mode, Ctrl+0 clears the shaders.
pub fn input_conf(dir: &Path, quality: &str, clamp: bool) -> String {
    let mut out = String::new();
    for (i, mode) in MODES.iter().enumerate() {
        let u = Upscaling { mode: (*mode).into(), quality: quality.into(), clamp_highlights: clamp, ..Upscaling::default() };
        let appends: Vec<String> = chain_paths(dir, &u).unwrap().iter().map(|p| format!("no-osd change-list glsl-shaders append {}", quote(p))).collect();
        out.push_str(&format!(
            "CTRL+{} no-osd change-list glsl-shaders clr \"\"; {}; show-text {}\n",
            i + 1,
            appends.join("; "),
            quote(&format!("Anime4K: Mode {mode} ({quality})"))
        ));
    }
    out.push_str("CTRL+0 no-osd change-list glsl-shaders clr \"\"; show-text \"GLSL shaders cleared\"\n");
    out
}

/// Options for mpv's command line, taken from the user's upscaling settings.
pub fn mpv_args(u: &Upscaling) -> Vec<String> {
    let mut a = Vec::new();
    if u.gpu_hq_profile {
        a.push("--profile=gpu-hq".to_string());
    }
    if !u.scaler.is_empty() {
        a.push(format!("--scale={}", u.scaler));
        a.push(format!("--cscale={}", u.scaler));
    }
    a
}

pub fn should_apply(u: &Upscaling, source_height: Option<u32>) -> bool {
    u.enabled && source_height.map_or(true, |h| h <= u.max_source_height)
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct ShaderStatus {
    pub dir: String,
    pub total: usize,
    pub present: usize,
    pub ready: bool,
    pub modes: Vec<(String, String)>,
}

pub struct ShaderManager {
    pub dir: PathBuf,
}

impl ShaderManager {
    pub fn new(data_dir: &Path) -> Self {
        Self { dir: data_dir.join("shaders") }
    }

    pub fn status(&self) -> ShaderStatus {
        let need = required_files();
        let present = need.iter().filter(|f| self.dir.join(f).exists()).count();
        ShaderStatus {
            dir: self.dir.to_string_lossy().into_owned(),
            total: need.len(),
            present,
            ready: present == need.len(),
            modes: MODES.iter().map(|m| (m.to_string(), mode_info(m).to_string())).collect(),
        }
    }

    pub fn missing_for(&self, u: &Upscaling) -> Vec<String> {
        chain_names(u).unwrap_or_default().iter().map(|n| file_for(n)).filter(|f| !self.dir.join(f).exists()).collect()
    }

    pub fn write_input_conf(&self, u: &Upscaling) -> Result<PathBuf> {
        let file = self.dir.parent().unwrap_or(&self.dir).join("anime4k-input.conf");
        std::fs::write(&file, input_conf(&self.dir, &u.quality, u.clamp_highlights))?;
        Ok(file)
    }

    /// Unpacks the .glsl files we need from a GitHub repository tarball (.tar.gz) into the shader folder.
    pub fn install_from_targz(&self, bytes: &[u8]) -> Result<usize> {
        std::fs::create_dir_all(&self.dir)?;
        let wanted: BTreeSet<String> = required_files().into_iter().collect();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
        let mut copied = 0;
        for entry in archive.entries()? {
            let mut entry = entry?;
            let name = entry.path()?.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if wanted.contains(&name) {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                std::fs::write(self.dir.join(&name), data)?;
                copied += 1;
            }
        }
        if copied == 0 {
            return Err(Error::msg("The download did not contain any Anime4K shader files."));
        }
        Ok(copied)
    }

    pub async fn install(&self, http: &reqwest::Client, on_status: &(dyn Fn(String) + Send + Sync)) -> Result<ShaderStatus> {
        on_status("Downloading Anime4K shaders from GitHub".into());
        let res = http.get("https://codeload.github.com/bloc97/Anime4K/tar.gz/refs/heads/master").send().await?;
        if !res.status().is_success() {
            return Err(Error::msg(format!("GitHub answered {}", res.status().as_u16())));
        }
        let bytes = res.bytes().await?;
        on_status("Extracting".into());
        let n = self.install_from_targz(&bytes)?;
        on_status(format!("Installed {n} shader files"));
        Ok(self.status())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up(mode: &str, quality: &str, clamp: bool) -> Upscaling {
        Upscaling { mode: mode.into(), quality: quality.into(), clamp_highlights: clamp, ..Upscaling::default() }
    }

    #[test]
    fn mode_a_hq_matches_the_upstream_template() {
        assert_eq!(
            chain_names(&up("A", "HQ", true)).unwrap(),
            ["Clamp_Highlights", "Restore_CNN_VL", "Upscale_CNN_x2_VL", "AutoDownscalePre_x2", "AutoDownscalePre_x4", "Upscale_CNN_x2_M"]
        );
        assert!(!chain_names(&up("A", "HQ", false)).unwrap().iter().any(|n| n == "Clamp_Highlights"));
        assert!(chain_names(&up("Z", "HQ", true)).is_err());
    }

    #[test]
    fn fast_and_combined_modes_follow_the_upstream_pattern() {
        assert_eq!(
            chain_names(&up("A", "Fast", true)).unwrap(),
            ["Clamp_Highlights", "Restore_CNN_M", "Upscale_CNN_x2_M", "AutoDownscalePre_x2", "AutoDownscalePre_x4", "Upscale_CNN_x2_S"]
        );
        assert_eq!(
            chain_names(&up("C+A", "HQ", true)).unwrap(),
            ["Clamp_Highlights", "Upscale_Denoise_CNN_x2_VL", "AutoDownscalePre_x2", "AutoDownscalePre_x4", "Restore_CNN_M", "Upscale_CNN_x2_M"]
        );
        assert_eq!(chain_names(&up("B+B", "Fast", true)).unwrap()[3], "Restore_CNN_Soft_S");
    }

    #[test]
    fn every_mode_ends_in_an_upscaler() {
        for q in ["HQ", "Fast"] {
            for m in MODES {
                let c = chain_names(&up(m, q, true)).unwrap();
                assert!(c.last().unwrap().starts_with("Upscale_CNN_x2_"), "{q} {m}");
            }
        }
        assert!(required_files().iter().all(|f| f.starts_with("Anime4K_") && f.ends_with(".glsl")));
    }

    #[test]
    fn commands_and_input_conf_use_forward_slashes() {
        let dir = Path::new("C:\\Users\\me\\shaders");
        let cmds = mpv_commands(dir, &up("C", "Fast", true)).unwrap();
        assert_eq!(cmds[0], ["change-list", "glsl-shaders", "clr", ""]);
        assert_eq!(cmds[1][3], "C:/Users/me/shaders/Anime4K_Clamp_Highlights.glsl");
        let conf = input_conf(dir, "HQ", true);
        for k in ["CTRL+1 ", "CTRL+2 ", "CTRL+3 ", "CTRL+4 ", "CTRL+5 ", "CTRL+6 ", "CTRL+0 "] {
            assert!(conf.contains(k), "{k}");
        }
        assert!(!conf.contains("\\U"), "no raw backslash paths");
    }

    #[test]
    fn should_apply_respects_the_switch_and_source_height() {
        let u = Upscaling { enabled: true, max_source_height: 1080, ..Upscaling::default() };
        assert!(should_apply(&u, Some(720)));
        assert!(!should_apply(&u, Some(2160)));
        assert!(should_apply(&u, None));
        assert!(!should_apply(&Upscaling { enabled: false, ..u }, Some(480)));
    }

    #[test]
    fn status_and_install_from_a_tarball() {
        let dir = tempfile::tempdir().unwrap();
        let sm = ShaderManager::new(dir.path());
        assert_eq!(sm.status().present, 0);

        // Build a tarball shaped like the GitHub archive: a top folder with nested directories.
        let mut gz = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::fast());
            let mut b = tar::Builder::new(enc);
            for f in required_files() {
                let data = b"//glsl";
                let mut h = tar::Header::new_gnu();
                h.set_size(data.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                b.append_data(&mut h, format!("Anime4K-master/glsl/Restore/{f}"), &data[..]).unwrap();
            }
            let mut h = tar::Header::new_gnu();
            h.set_size(3);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, "Anime4K-master/README.md", &b"hi!"[..]).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        let n = sm.install_from_targz(&gz).unwrap();
        assert_eq!(n, required_files().len());
        assert!(sm.status().ready);
        assert!(sm.missing_for(&up("A", "HQ", true)).is_empty());
        assert!(sm.install_from_targz(&[0x1f, 0x8b]).is_err());
    }
}
