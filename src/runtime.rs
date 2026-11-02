//! Files that are built into the executable: libmpv, the torrent engine and the Anime4K shaders.
//! They are written to the app's data folder on first run, then reused.
use crate::anime4k;
use std::fs;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

pub struct Runtime {
    pub dir: PathBuf,
    pub libmpv: Option<PathBuf>,
    pub rqbit: Option<PathBuf>,
    pub shaders_dir: PathBuf,
    pub shaders_embedded: bool,
}

fn extract(item: Option<(&str, &[u8])>, dir: &Path) -> Option<PathBuf> {
    let (name, bytes) = item?;
    fs::create_dir_all(dir).ok()?;
    let path = dir.join(name);
    if fs::metadata(&path).map(|m| m.len() == bytes.len() as u64).unwrap_or(false) {
        return Some(path);
    }
    let part = dir.join(format!("{}.part", name));
    fs::write(&part, bytes).ok()?;
    let path = match fs::rename(&part, &path) {
        Ok(()) => path,
        Err(_) => {
            // an older copy that cannot be replaced (still loaded by another window, held by an antivirus): this
            // version gets its own name next to it
            let (stem, ext) = name.rsplit_once('.').map(|(a, b)| (a.to_string(), format!(".{}", b))).unwrap_or((name.to_string(), String::new()));
            let own = dir.join(format!("{}-{}{}", stem, bytes.len(), ext));
            if !fs::metadata(&own).map(|m| m.len() == bytes.len() as u64).unwrap_or(false) {
                fs::rename(&part, &own).ok()?;
            } else {
                let _ = fs::remove_file(&part);
            }
            own
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o755));
    }
    Some(path)
}

pub fn prepare(data_dir: &Path) -> Runtime {
    let dir = data_dir.join("runtime");
    let shaders_dir = data_dir.join("shaders");
    if !SHADERS.is_empty() {
        let _ = anime4k::extract_embedded(SHADERS, &shaders_dir);
    }
    Runtime {
        libmpv: extract(LIBMPV, &dir),
        rqbit: extract(RQBIT, &dir).or_else(find_rqbit_beside_exe),
        dir,
        shaders_dir,
        shaders_embedded: !SHADERS.is_empty(),
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn find_rqbit_beside_exe() -> Option<PathBuf> {
    let d = exe_dir()?;
    ["rqbit.exe", "rqbit"].iter().map(|n| d.join(n)).find(|p| p.is_file())
}

impl Runtime {
    /// Where to look for libmpv, best first: the built-in copy, then next to the program.
    /// The system library names are tried after these by the loader.
    pub fn libmpv_candidates(&self) -> Vec<PathBuf> {
        let mut v = Vec::new();
        if let Some(p) = &self.libmpv {
            v.push(p.clone());
        }
        if let Some(d) = exe_dir() {
            for n in ["libmpv-2.dll", "mpv-2.dll", "libmpv.dll"] {
                v.push(d.join(n));
            }
            // the macOS app bundle (.github/workflows/macos.yml) carries libmpv in Contents/Frameworks
            if cfg!(target_os = "macos") {
                v.push(d.join("../Frameworks/libmpv.2.dylib"));
                v.push(d.join("libmpv.2.dylib"));
            }
        }
        v
    }

    pub fn has_builtin_mpv(&self) -> bool {
        LIBMPV.is_some()
    }
}
