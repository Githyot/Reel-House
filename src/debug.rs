//! The debug report: when it is turned on (Preferences > General > Diagnostics) the app writes a text file with
//! what happens and how long it takes: the processor use of the app every few seconds, every tick of the window
//! that was slow and which part of it was, every network request, the player, Suwayomi and the karaoke.
//! The file can be sent as it is. Nothing is written when the option is off.
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

static ON: AtomicBool = AtomicBool::new(false);
/// Pictures the window drew since the last summary line.
static FRAMES: AtomicU64 = AtomicU64::new(0);
static STATE: Mutex<Option<State>> = Mutex::new(None);
/// Where the report goes (set once at start).
static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

const MAX_BYTES: u64 = 24 * 1024 * 1024;
/// A part of the window clock slower than this (milliseconds) is written down.
pub const SLOW_MS: u128 = 12;

struct State {
    file: File,
    start: Instant,
    bytes: u64,
    /// Heartbeat bookkeeping.
    beat_at: Instant,
    beat_cpu: Option<f64>,
    ticks: u64,
    tick_ms: f64,
    tick_max: f64,
    tick_max_what: String,
    /// The slowest part of the tick being measured now.
    cur_what: String,
    cur_ms: f64,
}

/// A folder chosen by the interface (the Qt version keeps every log in `debug/` next to the program).
static FORCED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Sets the folder of every log before the core starts; `init` uses it instead of the data folder.
pub fn set_dir(dir: PathBuf) {
    if let Ok(mut f) = FORCED.lock() {
        *f = Some(dir);
    }
}

pub fn init(data_dir: &Path) {
    let forced = FORCED.lock().ok().and_then(|f| f.clone());
    if let Ok(mut d) = DIR.lock() {
        *d = Some(forced.unwrap_or_else(|| data_dir.join("debug")));
    }
}

pub fn path() -> Option<PathBuf> {
    DIR.lock().ok()?.as_ref().map(|d| d.join("reelhouse-debug.log"))
}

pub fn dir() -> Option<PathBuf> {
    DIR.lock().ok()?.clone()
}

/// Called by the window after each picture it draws.
pub fn frame() {
    FRAMES.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Turns the report on or off (cheap to call every tick). `header` is written when the file is opened.
pub fn set_enabled(on: bool, header: impl FnOnce() -> String) {
    if on == ON.load(Ordering::Relaxed) {
        return;
    }
    if !on {
        log("debug", "report turned off");
        ON.store(false, Ordering::Relaxed);
        if let Ok(mut s) = STATE.lock() {
            *s = None;
        }
        return;
    }
    let Some(path) = path() else { return };
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // The report of the run before is kept next to the new one.
    let _ = std::fs::rename(&path, path.with_extension("old.log"));
    let Ok(file) = File::create(&path) else { return };
    let now = Instant::now();
    if let Ok(mut s) = STATE.lock() {
        *s = Some(State { file, start: now, bytes: 0, beat_at: now, beat_cpu: cpu_seconds(), ticks: 0, tick_ms: 0.0, tick_max: 0.0, tick_max_what: String::new(), cur_what: String::new(), cur_ms: 0.0 });
    }
    ON.store(true, Ordering::Relaxed);
    log("debug", &header());
}

/// One line of the report: `[seconds since start] [category] text`.
pub fn log(cat: &str, msg: &str) {
    if !enabled() {
        return;
    }
    if let Ok(mut g) = STATE.lock() {
        if let Some(s) = g.as_mut() {
            if s.bytes > MAX_BYTES {
                return;
            }
            let line = format!("[{:9.3}] [{}] {}\n", s.start.elapsed().as_secs_f64(), cat, msg);
            s.bytes += line.len() as u64;
            let _ = s.file.write_all(line.as_bytes());
        }
    }
}

/// Runs `f`; when the report is on and it took long, writes it down and remembers it as the slowest part of this tick.
pub fn section<R>(name: &str, f: impl FnOnce() -> R) -> R {
    if !enabled() {
        return f();
    }
    let t = Instant::now();
    let r = f();
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    if ms as u128 >= SLOW_MS {
        log("slow", &format!("{} took {:.1} ms", name, ms));
    }
    if let Ok(mut g) = STATE.lock() {
        if let Some(s) = g.as_mut() {
            if ms > s.cur_ms {
                s.cur_ms = ms;
                s.cur_what = name.to_string();
            }
        }
    }
    r
}

/// Called at the end of every tick of the window with how long the whole tick took. Every 5 seconds a summary line
/// is written: processor use of the app, memory, how many ticks, the average and worst tick and what the worst was.
pub fn tick_done(ms: f64, extra: impl FnOnce() -> String) {
    if !enabled() {
        return;
    }
    let mut beat: Option<String> = None;
    if let Ok(mut g) = STATE.lock() {
        if let Some(s) = g.as_mut() {
            s.ticks += 1;
            s.tick_ms += ms;
            if ms > s.tick_max {
                s.tick_max = ms;
                s.tick_max_what = if s.cur_what.is_empty() { "(nothing measured)".into() } else { s.cur_what.clone() };
            }
            s.cur_what.clear();
            s.cur_ms = 0.0;
            let wall = s.beat_at.elapsed().as_secs_f64();
            if wall >= 5.0 {
                let cpu = cpu_seconds();
                let pct = match (cpu, s.beat_cpu) {
                    (Some(a), Some(b)) if wall > 0.0 => format!("{:.0}%", (a - b) / wall * 100.0),
                    _ => "?".to_string(),
                };
                let mem = memory_mb().map(|m| format!("{} MB", m)).unwrap_or_else(|| "?".into());
                beat = Some(format!(
                    "cpu {} of one core, memory {}, {} frames drawn, {} ticks (avg {:.1} ms, worst {:.1} ms in {})",
                    pct,
                    mem,
                    FRAMES.swap(0, Ordering::Relaxed),
                    s.ticks,
                    s.tick_ms / s.ticks.max(1) as f64,
                    s.tick_max,
                    s.tick_max_what
                ));
                s.beat_at = Instant::now();
                s.beat_cpu = cpu;
                s.ticks = 0;
                s.tick_ms = 0.0;
                s.tick_max = 0.0;
            }
        }
    }
    if let Some(mut b) = beat {
        let e = extra();
        if !e.is_empty() {
            b.push_str(", ");
            b.push_str(&e);
        }
        log("beat", &b);
    }
}

/// Hides keys and tokens that may sit in a link.
pub fn redact(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else { return url.to_string() };
    let q: Vec<String> = query
        .split('&')
        .map(|p| match p.split_once('=') {
            Some((k, _)) if ["key", "api_key", "apikey", "token", "access_token", "client_id", "password", "auth", "secret"].contains(&k.to_lowercase().as_str()) => format!("{}=***", k),
            _ => p.to_string(),
        })
        .collect();
    format!("{}?{}", base, q.join("&"))
}

/// Seconds of processor time the app has used so far (all its threads).
pub fn cpu_seconds() -> Option<f64> {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default, Clone, Copy)]
        struct Ft {
            lo: u32,
            hi: u32,
        }
        extern "system" {
            fn GetCurrentProcess() -> isize;
            fn GetProcessTimes(h: isize, c: *mut Ft, e: *mut Ft, k: *mut Ft, u: *mut Ft) -> i32;
        }
        let (mut c, mut e, mut k, mut u) = (Ft::default(), Ft::default(), Ft::default(), Ft::default());
        // SAFETY: plain system call with pointers to four local structures.
        let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) };
        if ok == 0 {
            return None;
        }
        let v = |f: Ft| ((f.hi as u64) << 32 | f.lo as u64) as f64 / 10_000_000.0;
        Some(v(k) + v(u))
    }
    #[cfg(all(not(windows), target_os = "linux"))]
    {
        let t = std::fs::read_to_string("/proc/self/stat").ok()?;
        let rest = t.rsplit_once(')')?.1;
        let f: Vec<&str> = rest.split_whitespace().collect();
        let (ut, st): (f64, f64) = (f.get(11)?.parse().ok()?, f.get(12)?.parse().ok()?);
        Some((ut + st) / 100.0)
    }
    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        None
    }
}

/// Memory the app holds, in MB.
pub fn memory_mb() -> Option<u64> {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct Pmc {
            cb: u32,
            page_faults: u32,
            peak_ws: usize,
            ws: usize,
            a: usize,
            b: usize,
            c: usize,
            d: usize,
            pagefile: usize,
            peak_pagefile: usize,
        }
        extern "system" {
            fn GetCurrentProcess() -> isize;
            fn K32GetProcessMemoryInfo(h: isize, p: *mut Pmc, cb: u32) -> i32;
        }
        let mut p = Pmc { cb: std::mem::size_of::<Pmc>() as u32, page_faults: 0, peak_ws: 0, ws: 0, a: 0, b: 0, c: 0, d: 0, pagefile: 0, peak_pagefile: 0 };
        // SAFETY: plain system call with a pointer to a local structure of the size it is told.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut p, p.cb) };
        if ok == 0 {
            return None;
        }
        Some((p.ws / (1024 * 1024)) as u64)
    }
    #[cfg(all(not(windows), target_os = "linux"))]
    {
        let t = std::fs::read_to_string("/proc/self/statm").ok()?;
        let pages: u64 = t.split_whitespace().nth(1)?.parse().ok()?;
        Some(pages * 4096 / (1024 * 1024))
    }
    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_keys_in_links() {
        assert_eq!(redact("https://x.org/a?q=1&api_key=SECRET&token=abc"), "https://x.org/a?q=1&api_key=***&token=***");
        assert_eq!(redact("https://x.org/a"), "https://x.org/a");
    }

    #[test]
    fn writes_only_when_on() {
        let d = std::env::temp_dir().join(format!("rh-dbg-{}", std::process::id()));
        init(&d);
        log("t", "nothing");
        assert!(!path().unwrap().exists());
        set_enabled(true, || "header".to_string());
        log("t", "hello");
        section("fast", || 1);
        set_enabled(false, String::new);
        let text = std::fs::read_to_string(path().unwrap()).unwrap();
        assert!(text.contains("header") && text.contains("[t] hello"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn reads_the_processor_clock() {
        if cfg!(target_os = "linux") {
            assert!(cpu_seconds().is_some());
            assert!(memory_mb().is_some());
        }
    }
}
