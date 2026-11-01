// Embeds the files in `assets/` into the executable, so the finished program is one file and
// needs nothing installed next to it:
//   assets/libmpv-2.dll        the mpv player library (Windows)
//   assets/rqbit.exe           the torrent engine used when Real-Debrid is off
//   assets/shaders/*.glsl      the Anime4K upscaling shaders
// Anything missing is simply left out. The program then falls back to a system libmpv, to
// Real-Debrid only, and to downloading the shaders from the Settings page.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn first_existing(dir: &Path, names: &[&str]) -> Option<(String, PathBuf)> {
    names.iter().find_map(|n| {
        let p = dir.join(n);
        if p.is_file() { Some((n.to_string(), p)) } else { None }
    })
}

fn option_line(var: &str, found: Option<(String, PathBuf)>) -> String {
    match found {
        Some((name, path)) => {
            let p = path.to_string_lossy().to_string();
            format!("pub static {}: Option<(&str, &[u8])> = Some(({:?}, include_bytes!({:?})));\n", var, name, p)
        }
        None => format!("pub static {}: Option<(&str, &[u8])> = None;\n", var),
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let assets = manifest.join("assets");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=REELHOUSE_RELEASE");
    println!("cargo:rerun-if-changed=assets");

    // `reelhouse_mobile`: the code is for a phone or tablet (Android, iOS), where no helper program can be started.
    // Set by the target itself or by the `mobile` feature (so a desktop build can be tried in the mobile shape).
    println!("cargo:rustc-check-cfg=cfg(reelhouse_mobile)");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let mobile = env::var("CARGO_FEATURE_MOBILE").is_ok() || target_os == "android" || target_os == "ios";
    if mobile {
        println!("cargo:rustc-cfg=reelhouse_mobile");
        if env::var("CARGO_FEATURE_DESKTOP").is_ok() && (target_os == "android" || target_os == "ios") {
            println!("cargo:warning=the `desktop` feature is on for a {} build: use --no-default-features --features \"app js webp mobile\"", target_os);
        }
    }

    let mut code = String::new();
    // The Windows libmpv and the rqbit program are only built in on desktop: a phone cannot run them.
    let (libmpv, rqbit) = if mobile {
        (None, None)
    } else {
        (first_existing(&assets, &["libmpv-2.dll", "mpv-2.dll", "libmpv.dll"]), first_existing(&assets, &["rqbit.exe", "rqbit"]))
    };
    code.push_str(&option_line("LIBMPV", libmpv));
    code.push_str(&option_line("RQBIT", rqbit));

    code.push_str("pub static SHADERS: &[(&str, &[u8])] = &[\n");
    let mut shaders: Vec<PathBuf> = fs::read_dir(assets.join("shaders"))
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|x| x == "glsl").unwrap_or(false)).collect())
        .unwrap_or_default();
    shaders.sort();
    for p in shaders {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        code.push_str(&format!("    ({:?}, include_bytes!({:?})),\n", name, p.to_string_lossy().to_string()));
    }
    code.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("embedded.rs");
    fs::write(out, code).expect("write embedded.rs");
}
