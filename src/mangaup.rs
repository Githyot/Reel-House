//! Upscaling of manga pages in the reader. The pictures go through an external Vulkan engine
//! (realesrgan-ncnn-vulkan: it runs on NVIDIA, AMD and Intel graphics cards, with no Python or CUDA) and the result is
//! kept on disk, so a page is only upscaled once. The engine takes any NCNN model: put the `.param` and `.bin`
//! files of a model (for example a MangaJaNai model converted with chaiNNer) in its `models` folder and it shows up in
//! the list. A different program with the same command line can be chosen in the settings.
use crate::http::Http;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// The model that is used until another one is chosen: made for drawn pictures, fast, and it keeps lines clean.
pub const DEFAULT_MODEL: &str = "realesr-animevideov3";
const RELEASE: &str = "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/";

/// Pages that are being upscaled or were asked for, so none is done twice.
static ASKED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// The engine uses the graphics card: one page at a time.
static BUSY: Mutex<()> = Mutex::new(());
/// Changes when the chapter changes: pages of the old chapter that wait their turn are dropped.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn new_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// Marks a page as asked for. False when it already was.
pub fn ask(key: &str) -> bool {
    let mut g = ASKED.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(HashSet::new).insert(key.to_string())
}

pub fn forget(key: &str) {
    if let Some(s) = ASKED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        s.remove(key);
    }
}

pub fn archive_name() -> &'static str {
    if cfg!(windows) {
        "realesrgan-ncnn-vulkan-20220424-windows.zip"
    } else if cfg!(target_os = "macos") {
        "realesrgan-ncnn-vulkan-20220424-macos.zip"
    } else {
        "realesrgan-ncnn-vulkan-20220424-ubuntu.zip"
    }
}

pub fn download_url() -> String {
    format!("{}{}", RELEASE, archive_name())
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "realesrgan-ncnn-vulkan.exe"
    } else {
        "realesrgan-ncnn-vulkan"
    }
}

pub fn engine_dir(data: &Path) -> PathBuf {
    data.join("upscaler")
}

fn find_in(dir: &Path, depth: u32) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subs = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_file() && p.file_name().map(|n| n == exe_name()).unwrap_or(false) {
            return Some(p);
        }
        if p.is_dir() {
            subs.push(p);
        }
    }
    if depth == 0 {
        return None;
    }
    subs.into_iter().find_map(|s| find_in(&s, depth - 1))
}

/// The engine to run: the program chosen in the settings, or the one this app downloaded.
pub fn find_exe(data: &Path, custom: &str) -> Option<PathBuf> {
    let c = custom.trim();
    if !c.is_empty() {
        let p = PathBuf::from(c);
        return p.is_file().then_some(p);
    }
    find_in(&engine_dir(data), 2)
}

/// The models the engine has (names without extension). The three sizes of a video model count as one.
pub fn models(exe: &Path) -> Vec<String> {
    let dir = exe.parent().map(|p| p.join("models")).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "param").unwrap_or(false) && p.with_extension("bin").is_file() {
                if let Some(n) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) {
                    let base = family(&n);
                    if !out.contains(&base) {
                        out.push(base);
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// "realesr-animevideov3-x2" is one model in three sizes: the family name is what is asked for.
fn family(name: &str) -> String {
    for k in ["-x2", "-x3", "-x4"] {
        if let Some(b) = name.strip_suffix(k) {
            if b.starts_with("realesr-animevideov3") {
                return b.to_string();
            }
        }
    }
    name.to_string()
}

/// The size a model makes by itself ("2x_Name", "Name-x4", "x4plus"); None for a model that comes in several sizes.
pub fn native_scale(model: &str) -> Option<u32> {
    if model.starts_with("realesr-animevideov3") {
        return None;
    }
    let m = model.to_lowercase();
    for (k, v) in [("2x", 2), ("3x", 3), ("4x", 4), ("8x", 8)] {
        if m.starts_with(&format!("{}_", k)) || m.starts_with(&format!("{}-", k)) || m.contains(&format!("-{}", k.chars().rev().collect::<String>())) || m.contains(&format!("_{}", k.chars().rev().collect::<String>())) {
            return Some(v);
        }
    }
    Some(4)
}

/// Most the page grows: 2x the original. More costs memory and graphics-card time for no visible gain on a screen
/// (a 4x page of 1400 px is 5600 px wide, about 180 MB once decoded). Models that only make 4x are brought back to 2x
/// where the result is read (`qt/src/imageprovider.cpp`).
pub const MAX_SCALE: u32 = 2;

/// The scale the engine is asked for and gets.
pub fn scale_for(model: &str, wanted: u32) -> u32 {
    native_scale(model).unwrap_or_else(|| wanted.clamp(2, MAX_SCALE))
}

pub fn args(model: &str, scale: u32, input: &Path, output: &Path, tile: u32) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-o".into(),
        output.to_string_lossy().into_owned(),
        "-n".into(),
        model.to_string(),
        "-s".into(),
        scale.to_string(),
        "-f".into(),
        "webp".into(),
    ];
    if tile > 0 {
        a.push("-t".into());
        a.push(tile.to_string());
    }
    a
}

/// Where the upscaled version of a page is kept.
pub fn cached(data: &Path, url: &str, model: &str, scale: u32) -> PathBuf {
    data.join("upscaled").join(format!("{}.webp", crate::util::fnv_hex(&format!("{}|{}|{}", url, model, scale))))
}

/// Deletes the oldest upscaled pages until the folder is under `limit_mb`.
pub fn trim_cache(data: &Path, limit_mb: u64) {
    let Ok(rd) = std::fs::read_dir(data.join("upscaled")) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, p) in files {
        if total <= limit_mb * 1_048_576 {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

/// Downloads and unpacks the engine (about 40 MB, once).
pub fn install(http: &dyn Http, data: &Path, status: &dyn Fn(String)) -> Result<PathBuf, String> {
    crate::platform::needs_program("Upscaling pages needs the Real-ESRGAN program, which cannot run on phones and tablets yet.")?;
    let dir = engine_dir(data);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(archive_name());
    let part = dir.join(format!("{}.part", archive_name()));
    status("Downloading the upscaling engine…".to_string());
    http.download(&download_url(), &part, &|done, total| {
        status(match total {
            Some(t) => format!("Downloading the upscaling engine: {} of {} MB", done / 1_048_576, t / 1_048_576),
            None => format!("Downloading the upscaling engine: {} MB", done / 1_048_576),
        })
    })
    .map_err(|e| format!("Downloading the engine failed: {}", e))?;
    std::fs::rename(&part, &file).map_err(|e| e.to_string())?;
    status("Unpacking the upscaling engine…".to_string());
    let r = unzip(&file, &dir);
    let _ = std::fs::remove_file(&file);
    r?;
    let exe = find_in(&dir, 2).ok_or_else(|| "The engine was unpacked, but its program was not found.".to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
    }
    Ok(exe)
}

fn unzip(file: &Path, dest: &Path) -> Result<(), String> {
    let (f, d) = (file.to_string_lossy().into_owned(), dest.to_string_lossy().into_owned());
    let mut tries: Vec<(String, Vec<String>)> = Vec::new();
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        tries.push((format!("{}\\System32\\tar.exe", root), vec!["-xf".into(), f.clone(), "-C".into(), d.clone()]));
        tries.push((
            "powershell".to_string(),
            vec!["-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), format!("Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force", f.replace('\'', "''"), d.replace('\'', "''"))],
        ));
    }
    tries.push(("unzip".to_string(), vec!["-o".into(), f.clone(), "-d".into(), d.clone()]));
    tries.push(("tar".to_string(), vec!["-xf".into(), f.clone(), "-C".into(), d.clone()]));
    let mut last: Vec<String> = Vec::new();
    for (prog, a) in tries {
        let mut cmd = Command::new(&prog);
        cmd.args(&a).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        crate::suwayomi_host::hide(&mut cmd);
        match cmd.output() {
            Ok(o) if o.status.success() => return Ok(()),
            Ok(o) => last.push(format!("{}: {}", prog, String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or("no message"))),
            Err(e) => last.push(format!("{}: {}", prog, e)),
        }
    }
    Err(format!("Could not unpack the engine ({}).", last.join(" | ")))
}

/// Upscales one page. `original` is the picture as it was downloaded. Returns the file with the result.
pub fn run(data: &Path, exe: &Path, model: &str, wanted: u32, tile: u32, url: &str, original: &[u8], generation_at_ask: u64) -> Result<PathBuf, String> {
    crate::platform::needs_program("Upscaling pages needs the Real-ESRGAN program, which cannot run on phones and tablets yet.")?;
    let scale = scale_for(model, wanted);
    let out = cached(data, url, model, scale);
    if out.is_file() {
        return Ok(out);
    }
    let _turn = BUSY.lock().unwrap_or_else(|e| e.into_inner());
    // The reader moved on to another chapter while this page waited.
    if generation_at_ask != generation() {
        return Err("skipped".to_string());
    }
    if out.is_file() {
        return Ok(out);
    }
    let dir = data.join("upscaled");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let src = dir.join(format!("{}.src", crate::util::fnv_hex(url)));
    std::fs::write(&src, original).map_err(|e| e.to_string())?;
    // The result is written aside and moved when it is whole.
    let tmp = out.with_extension("part.webp");
    let mut cmd = Command::new(exe);
    cmd.args(args(model, scale, &src, &tmp, tile)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    if let Some(p) = exe.parent() {
        cmd.current_dir(p);
    }
    crate::suwayomi_host::hide(&mut cmd);
    let r = cmd.output();
    let _ = std::fs::remove_file(&src);
    match r {
        Ok(o) if o.status.success() && tmp.is_file() => {
            std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        Ok(o) => {
            let _ = std::fs::remove_file(&tmp);
            let msg = String::from_utf8_lossy(&o.stderr);
            let line = msg.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message");
            Err(format!("The engine failed ({}): {}", o.status, line.trim()))
        }
        Err(e) => Err(format!("The engine could not start: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_of_a_model() {
        assert_eq!(native_scale("realesr-animevideov3"), None);
        assert_eq!(scale_for("realesr-animevideov3", 3), 2);
        assert_eq!(scale_for("realesr-animevideov3", 9), 2);
        assert_eq!(scale_for("realesrgan-x4plus-anime", 2), 4);
        assert_eq!(scale_for("2x_MangaJaNai_1200p_V1_ESRGAN_90k", 4), 2);
        assert_eq!(scale_for("MyModel-x2", 4), 2);
        assert_eq!(scale_for("Unknown", 2), 4);
    }

    #[test]
    fn command_line() {
        let a = args("realesr-animevideov3", 2, Path::new("in.jpg"), Path::new("out.webp"), 0);
        assert_eq!(a.join(" "), "-i in.jpg -o out.webp -n realesr-animevideov3 -s 2 -f webp");
        assert!(args("m", 2, Path::new("a"), Path::new("b"), 200).join(" ").ends_with("-t 200"));
    }

    #[test]
    fn model_list_and_cache() {
        let d = std::env::temp_dir().join(format!("rh-mangaup-{}", std::process::id()));
        let m = d.join("models");
        std::fs::create_dir_all(&m).unwrap();
        for f in ["realesr-animevideov3-x2", "realesr-animevideov3-x3", "my-model", "lonely"] {
            std::fs::write(m.join(format!("{}.param", f)), "x").unwrap();
            if f != "lonely" {
                std::fs::write(m.join(format!("{}.bin", f)), "x").unwrap();
            }
        }
        let exe = d.join(exe_name());
        std::fs::write(&exe, "x").unwrap();
        assert_eq!(models(&exe), vec!["my-model".to_string(), "realesr-animevideov3".to_string()]);
        assert_eq!(find_exe(&d.parent().unwrap().join("nothing-here"), ""), None);
        assert_eq!(find_exe(&d, exe.to_str().unwrap()), Some(exe.clone()));
        assert_ne!(cached(&d, "a", "m", 2), cached(&d, "a", "m", 3));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn pages_are_asked_once() {
        assert!(ask("test-page-1"));
        assert!(!ask("test-page-1"));
        forget("test-page-1");
        assert!(ask("test-page-1"));
        let g = generation();
        assert!(new_generation() > g);
    }
}
