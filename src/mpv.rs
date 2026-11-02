//! mpv, loaded as a library (libmpv) and drawn straight into the app window through its
//! OpenGL render API. There is no separate player window and no mpv install.
use libloading::Library;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::Arc;

/// The value of mpv's `http-header-fields` (a list option): "Name: value" items joined by commas, with `\` and `,`
/// escaped by a backslash as mpv's list parser expects, so a comma inside a value (cookies, Accept) does not split
/// one header in two.
pub fn header_list(h: &[(String, String)]) -> String {
    h.iter()
        .map(|(k, v)| format!("{}: {}", k, v).replace('\\', "\\\\").replace(',', "\\,"))
        .collect::<Vec<_>>()
        .join(",")
}

type Handle = *mut c_void;
type GetProc = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;
type UpdateCb = unsafe extern "C" fn(*mut c_void);

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct OpenglInit {
    get_proc_address: GetProc,
    get_proc_address_ctx: *mut c_void,
    /// Only in libmpv 1.x (removed in 2.0, where it is ignored): must be null there, so it is always sent as null.
    extra_exts: *const c_char,
}

#[repr(C)]
struct OpenglFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}

// mpv_format
const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;
// mpv_render_param_type
const PARAM_API_TYPE: c_int = 1;
const PARAM_OPENGL_INIT: c_int = 2;
const PARAM_OPENGL_FBO: c_int = 3;
const PARAM_FLIP_Y: c_int = 4;
const PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;
const PARAM_SW_SIZE: c_int = 17;
const PARAM_SW_FORMAT: c_int = 18;
const PARAM_SW_STRIDE: c_int = 19;
const PARAM_SW_POINTER: c_int = 20;

struct Api {
    _lib: Library,
    create: unsafe extern "C" fn() -> Handle,
    initialize: unsafe extern "C" fn(Handle) -> c_int,
    set_option_string: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    set_property_string: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(Handle, *const *const c_char) -> c_int,
    get_property_string: unsafe extern "C" fn(Handle, *const c_char) -> *mut c_char,
    get_property: unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int,
    free: unsafe extern "C" fn(*mut c_void),
    wait_event: unsafe extern "C" fn(Handle, f64) -> *mut Event,
    observe_property: unsafe extern "C" fn(Handle, u64, *const c_char, c_int) -> c_int,
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    terminate_destroy: unsafe extern "C" fn(Handle),
    render_create: unsafe extern "C" fn(*mut *mut c_void, Handle, *mut RenderParam) -> c_int,
    render_render: unsafe extern "C" fn(*mut c_void, *mut RenderParam) -> c_int,
    render_set_update_callback: unsafe extern "C" fn(*mut c_void, Option<UpdateCb>, *mut c_void),
}

unsafe fn sym<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, String> {
    lib.get::<T>(name)
        .map(|s| *s)
        .map_err(|e| format!("this libmpv is missing {} ({})", String::from_utf8_lossy(&name[..name.len() - 1]), e))
}

fn system_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["libmpv-2.dll", "mpv-2.dll", "libmpv.dll"]
    } else if cfg!(target_os = "macos") {
        &["libmpv.2.dylib", "libmpv.dylib"]
    } else if cfg!(target_os = "android") {
        // packed into the APK by Qt (QT_ANDROID_EXTRA_LIBS, see qt/CMakeLists.txt): the loader finds it by name
        &["libmpv.so", "libmpv.so.2"]
    } else if cfg!(target_os = "ios") {
        // MPVKit's framework in the app bundle (Frameworks/), or a plain dylib beside it
        &["@rpath/Libmpv.framework/Libmpv", "Libmpv.framework/Libmpv", "@rpath/MPVKit.framework/MPVKit", "libmpv.dylib"]
    } else {
        &["libmpv.so.2", "libmpv.so.1", "libmpv.so"]
    }
}

fn load_library(candidates: &[PathBuf]) -> Result<Library, String> {
    let mut tried: Vec<String> = Vec::new();
    for p in candidates.iter().filter(|p| p.is_file()) {
        match unsafe { Library::new(p) } {
            Ok(l) => return Ok(l),
            Err(e) => tried.push(format!("{} ({})", p.display(), e)),
        }
    }
    for n in system_names() {
        match unsafe { Library::new(n) } {
            Ok(l) => return Ok(l),
            Err(e) => tried.push(format!("{} ({})", n, e)),
        }
    }
    Err(format!("Could not load the mpv library. Tried: {}", tried.join("; ")))
}

impl Api {
    fn load(candidates: &[PathBuf]) -> Result<Api, String> {
        let lib = load_library(candidates)?;
        unsafe {
            Ok(Api {
                create: sym(&lib, b"mpv_create\0")?,
                initialize: sym(&lib, b"mpv_initialize\0")?,
                set_option_string: sym(&lib, b"mpv_set_option_string\0")?,
                set_property_string: sym(&lib, b"mpv_set_property_string\0")?,
                command: sym(&lib, b"mpv_command\0")?,
                get_property_string: sym(&lib, b"mpv_get_property_string\0")?,
                get_property: sym(&lib, b"mpv_get_property\0")?,
                free: sym(&lib, b"mpv_free\0")?,
                wait_event: sym(&lib, b"mpv_wait_event\0")?,
                observe_property: sym(&lib, b"mpv_observe_property\0")?,
                error_string: sym(&lib, b"mpv_error_string\0")?,
                terminate_destroy: sym(&lib, b"mpv_terminate_destroy\0")?,
                render_create: sym(&lib, b"mpv_render_context_create\0")?,
                render_render: sym(&lib, b"mpv_render_context_render\0")?,
                render_set_update_callback: sym(&lib, b"mpv_render_context_set_update_callback\0")?,
                _lib: lib,
            })
        }
    }
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: i64,
    pub lang: String,
    pub title: String,
    pub selected: bool,
    pub external: bool,
    /// Path of the file of an external subtitle.
    pub file: String,
}

impl Track {
    pub fn label(&self) -> String {
        let mut s = if self.lang.is_empty() { format!("Track {}", self.id) } else { self.lang.clone() };
        if !self.title.is_empty() {
            s.push_str(&format!(" - {}", self.title));
        }
        if self.external {
            s.push_str(" (added)");
        }
        s
    }
}

#[derive(Clone, Debug)]
pub enum Evt {
    FileLoaded,
    PlaybackRestart,
    /// reason 0 = end of file, 2 = stopped, 3 = quit, 4 = error
    EndFile { reason: i32, error: String },
    Shutdown,
    /// An observed property changed (`Mpv::observe`): its name and value (None when it has no value now).
    Prop(String, Option<PropValue>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Flag(bool),
    Double(f64),
}

/// mpv_event_property
#[repr(C)]
struct EventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

pub struct Mpv {
    api: Arc<Api>,
    h: Handle,
    render: Handle,
    update_data: *mut c_void,
}

// mpv's client API is thread-safe, and the render context is only used from the GL thread.
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

unsafe extern "C" fn on_update(data: *mut c_void) {
    let f = &*(data as *const Box<dyn Fn() + Send + Sync>);
    f();
}

impl Mpv {
    /// The main player, drawn into the window through the render API.
    pub fn new(candidates: &[PathBuf]) -> Result<Mpv, String> {
        Self::create(
            candidates,
            &[
                ("vo", "libmpv"),
                ("idle", "yes"),
                ("keep-open", "yes"),
                ("input-default-bindings", "no"),
                ("input-vo-keyboard", "no"),
                ("osc", "no"),
                ("osd-level", "0"),
                ("config", "no"),
                ("sub-auto", "no"),
                ("audio-display", "no"),
                ("cache", "yes"),
                ("ytdl", "no"),
                ("volume-max", "150"),
                // Torrent streams can be slow to answer at first; do not give up early.
                ("network-timeout", "120"),
                ("demuxer-max-bytes", "200MiB"),
                ("demuxer-readahead-secs", "90"),
            ],
        )
    }

    /// Sound of the Study cards ([sound:...] of Anki): no picture, no window.
    pub fn new_audio(candidates: &[PathBuf]) -> Result<Mpv, String> {
        Self::create(
            candidates,
            &[("vo", "null"), ("video", "no"), ("idle", "yes"), ("config", "no"), ("load-scripts", "no"), ("ytdl", "no"), ("terminal", "no"), ("input-default-bindings", "no")],
        )
    }

    /// The clips of the Study cards: mpv's own small window, on top, with its on-screen controller.
    pub fn new_window(candidates: &[PathBuf], title: &str) -> Result<Mpv, String> {
        Self::create(
            candidates,
            &[
                ("force-window", "yes"),
                ("idle", "yes"),
                ("keep-open", "yes"),
                ("ontop", "yes"),
                ("autofit", "50%"),
                ("osc", "yes"),
                ("input-default-bindings", "yes"),
                ("input-vo-keyboard", "yes"),
                ("config", "no"),
                ("ytdl", "no"),
                ("terminal", "no"),
                ("hwdec", "auto-safe"),
                ("title", title),
            ],
        )
    }

    /// The opening / ending videos inside the X-Ray: a second player with sound whose pictures are drawn into memory
    /// (`init_render_sw` + `render_sw`), small, and shown by the interface. Decoding stays on the processor (`hwdec=no`):
    /// the pictures are read in memory anyway, and the graphics card stays with the episode.
    pub fn new_embedded(candidates: &[PathBuf]) -> Result<Mpv, String> {
        Self::create(
            candidates,
            &[
                ("vo", "libmpv"),
                ("idle", "yes"),
                ("keep-open", "yes"),
                ("config", "no"),
                ("osc", "no"),
                ("osd-level", "0"),
                ("input-default-bindings", "no"),
                ("input-vo-keyboard", "no"),
                ("load-scripts", "no"),
                ("ytdl", "no"),
                ("terminal", "no"),
                ("hwdec", "no"),
                ("sid", "no"),
                ("audio-display", "no"),
                ("cache", "yes"),
                ("network-timeout", "30"),
                ("vd-lavc-threads", "2"),
            ],
        )
    }

    /// A silent second player that only produces preview pictures for the seek bar.
    pub fn new_thumbnailer(candidates: &[PathBuf]) -> Result<Mpv, String> {
        Self::create(
            candidates,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("keep-open", "yes"),
                ("pause", "yes"),
                ("config", "no"),
                ("osc", "no"),
                ("input-default-bindings", "no"),
                ("sub", "no"),
                ("aid", "no"),
                ("hr-seek", "no"),
                ("hwdec", "no"), // thumbnails only
                ("vd-lavc-skiploopfilter", "all"),
                ("vd-lavc-threads", "2"),
                ("cache", "yes"),
                ("demuxer-max-bytes", "32MiB"),
                ("ytdl", "no"),
                ("load-scripts", "no"),
            ],
        )
    }

    /// Cuts `start..end` seconds of `url` into `out` with mpv's own encoder (no other program needed): `opts` are the
    /// encoding options (`ovc`, `oac`, ...). `picture` makes a single JPEG picture at `start` instead of a video.
    /// Blocks until done (up to `limit_secs`).
    /// Only the sound of `start..end`, as MP3 (Anki plays it everywhere); Opus in Ogg when this mpv has no MP3 encoder.
    pub fn encode_audio(candidates: &[PathBuf], url: &str, headers: &[(String, String)], start: f64, end: f64, out: &str) -> Result<String, String> {
        let mp3 = format!("{}.mp3", out);
        match Self::encode_opts(candidates, url, headers, start, end, &mp3, &[("vid", "no"), ("oac", "libmp3lame"), ("oacopts", "b=128k"), ("of", "mp3")], false, 90) {
            Ok(()) => Ok(mp3),
            Err(e1) => {
                let _ = std::fs::remove_file(&mp3);
                let ogg = format!("{}.ogg", out);
                Self::encode_opts(candidates, url, headers, start, end, &ogg, &[("vid", "no"), ("oac", "libopus"), ("oacopts", "b=64k"), ("of", "ogg")], false, 90)
                    .map(|_| ogg)
                    .map_err(|e2| format!("{} / {}", e1, e2))
            }
        }
    }

    /// The sound of audio track `aid` ("auto" for the file's own choice) from `start` to `end`, decoded to 8 kHz mono
    /// 16-bit samples (raw, little endian) in `out`, for comparing two audio tracks (`audiosync`). Tries a raw file
    /// first, then WAV when this mpv's FFmpeg has no raw muxer; returns which one was written ("s16le" or "wav").
    pub fn encode_pcm(candidates: &[PathBuf], url: &str, headers: &[(String, String)], aid: &str, start: f64, end: f64, out: &str, limit_secs: u64) -> Result<&'static str, String> {
        let af = "lavfi=[aresample=8000,aformat=sample_fmts=s16:sample_rates=8000:channel_layouts=mono]";
        let mut errs = Vec::new();
        // Android's libmpv (from mpv-android) is built without FFmpeg encoders or muxers, so the encoder below always
        // fails there: mpv's own PCM writer (ao=pcm) needs neither and goes first on Android, last elsewhere.
        let android = cfg!(target_os = "android");
        if android {
            match Self::dump_pcm(candidates, url, headers, aid, start, end, out, limit_secs) {
                Ok(()) => return Ok("s16le"),
                Err(e) => errs.push(format!("ao-pcm: {}", e)),
            }
        }
        for of in ["s16le", "wav"] {
            let _ = std::fs::remove_file(out);
            match Self::encode_opts(candidates, url, headers, start, end, out, &[("vid", "no"), ("aid", aid), ("af", af), ("oac", "pcm_s16le"), ("of", of)], false, limit_secs) {
                Ok(()) => return Ok(of),
                Err(e) => errs.push(format!("{}: {}", of, e)),
            }
        }
        if !android {
            match Self::dump_pcm(candidates, url, headers, aid, start, end, out, limit_secs) {
                Ok(()) => return Ok("s16le"),
                Err(e) => errs.push(format!("ao-pcm: {}", e)),
            }
        }
        Err(errs.join(" / "))
    }

    /// `encode_pcm` through mpv's PCM audio output: the decoded sound, resampled by mpv itself to 8 kHz mono s16, is
    /// written raw to `out` as fast as it decodes (no FFmpeg encoder, muxer or filter involved).
    fn dump_pcm(candidates: &[PathBuf], url: &str, headers: &[(String, String)], aid: &str, start: f64, end: f64, out: &str, limit_secs: u64) -> Result<(), String> {
        let _ = std::fs::remove_file(out);
        let hdr = header_list(headers);
        let st = format!("{:.3}", start.max(0.0));
        let en = format!("{:.3}", end.max(start + 0.1));
        let mut opts: Vec<(&str, &str)> = vec![
            ("config", "no"),
            ("idle", "no"),
            ("osc", "no"),
            ("ytdl", "no"),
            ("load-scripts", "no"),
            ("input-default-bindings", "no"),
            ("hwdec", "no"),
            ("vid", "no"),
            ("sid", "no"),
            ("aid", aid),
            ("network-timeout", "60"),
            ("ao", "pcm"),
            ("ao-pcm-file", out),
            ("ao-pcm-waveheader", "no"),
            ("ao-pcm-append", "no"),
            ("audio-samplerate", "8000"),
            ("audio-channels", "mono"),
            ("audio-format", "s16"),
            ("untimed", "yes"),
            ("start", st.as_str()),
            ("end", en.as_str()),
        ];
        if !hdr.is_empty() {
            opts.push(("http-header-fields", hdr.as_str()));
        }
        {
            let m = Self::create(candidates, &opts)?;
            m.cmd(&["loadfile", url])?;
            let t0 = std::time::Instant::now();
            'run: loop {
                for e in m.events() {
                    match e {
                        Evt::EndFile { reason, error } => {
                            if reason == 0 {
                                break 'run;
                            }
                            return Err(if !error.is_empty() { error } else { format!("mpv stopped (reason {})", reason) });
                        }
                        Evt::Shutdown => break 'run,
                        _ => {}
                    }
                }
                if t0.elapsed().as_secs() > limit_secs {
                    return Err("reading the audio took too long".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            // dropped here: the player is destroyed and the PCM file closed before it is read
        }
        match std::fs::metadata(out) {
            Ok(md) if md.len() >= 1600 => Ok(()),
            Ok(md) => Err(format!("only {} bytes of sound", md.len())),
            Err(e) => Err(format!("no sound file ({})", e)),
        }
    }

    pub fn encode(candidates: &[PathBuf], url: &str, headers: &[(String, String)], start: f64, end: f64, out: &str, picture: bool, limit_secs: u64) -> Result<(), String> {
        let extra: Vec<(&str, &str)> = if picture {
            vec![("aid", "no"), ("ovc", "mjpeg"), ("ovcopts", "strict=-1"), ("of", "image2"), ("ofopts", "update=1"), ("frames", "1"), ("vf", "scale=-2:360")]
        } else {
            vec![("ovc", "libx264"), ("ovcopts", "crf=27,preset=veryfast"), ("oac", "aac"), ("oacopts", "b=96k"), ("vf", "scale=-2:480"), ("of", "mp4")]
        };
        Self::encode_opts(candidates, url, headers, start, end, out, &extra, picture, limit_secs)
    }

    /// mpv's encoder on `url` from `start` (to `end` unless it is a single picture) with the output options `extra`.
    fn encode_opts(candidates: &[PathBuf], url: &str, headers: &[(String, String)], start: f64, end: f64, out: &str, extra: &[(&str, &str)], picture: bool, limit_secs: u64) -> Result<(), String> {
        let hdr = header_list(headers);
        let st = format!("{:.3}", start.max(0.0));
        let en = format!("{:.3}", end.max(start + 0.1));
        let mut opts: Vec<(&str, &str)> = vec![
            ("config", "no"),
            ("idle", "no"),
            ("osc", "no"),
            ("ytdl", "no"),
            ("load-scripts", "no"),
            ("input-default-bindings", "no"),
            ("hwdec", "no"),
            ("sid", "no"),
            ("network-timeout", "60"),
            ("o", out),
            ("start", st.as_str()),
        ];
        if !picture {
            opts.push(("end", en.as_str()));
        }
        opts.extend(extra.iter().copied());
        if !hdr.is_empty() {
            opts.push(("http-header-fields", hdr.as_str()));
        }
        let m = Self::create(candidates, &opts)?;
        m.cmd(&["loadfile", url])?;
        let t0 = std::time::Instant::now();
        loop {
            for e in m.events() {
                match e {
                    Evt::EndFile { reason, error } => {
                        return if reason == 0 || reason == 2 && picture { Ok(()) } else if !error.is_empty() { Err(error) } else { Err(format!("the encoder stopped (reason {})", reason)) };
                    }
                    Evt::Shutdown => return Err("mpv closed".into()),
                    _ => {}
                }
            }
            if t0.elapsed().as_secs() > limit_secs {
                return Err("cutting the clip took too long".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
    }

    fn create(candidates: &[PathBuf], opts: &[(&str, &str)]) -> Result<Mpv, String> {
        let api = Arc::new(Api::load(candidates)?);
        let h = unsafe { (api.create)() };
        if h.is_null() {
            return Err("mpv could not create a player instance.".to_string());
        }
        let m = Mpv { api, h, render: null_mut(), update_data: null_mut() };
        for (k, v) in opts.iter().copied() {
            m.set_option(k, v)?;
        }
        let rc = unsafe { (m.api.initialize)(m.h) };
        if rc < 0 {
            return Err(format!("mpv failed to start: {}", m.err(rc)));
        }
        Ok(m)
    }

    fn err(&self, code: c_int) -> String {
        unsafe {
            let p = (self.api.error_string)(code);
            if p.is_null() { format!("error {}", code) } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
        }
    }

    fn cstr(s: &str) -> Result<CString, String> {
        CString::new(s).map_err(|_| "text contains a NUL character".to_string())
    }

    pub fn set_option(&self, name: &str, value: &str) -> Result<(), String> {
        let (n, v) = (Self::cstr(name)?, Self::cstr(value)?);
        let rc = unsafe { (self.api.set_option_string)(self.h, n.as_ptr(), v.as_ptr()) };
        if rc < 0 { Err(format!("mpv option {}={}: {}", name, value, self.err(rc))) } else { Ok(()) }
    }

    pub fn set(&self, name: &str, value: &str) -> Result<(), String> {
        let (n, v) = (Self::cstr(name)?, Self::cstr(value)?);
        let rc = unsafe { (self.api.set_property_string)(self.h, n.as_ptr(), v.as_ptr()) };
        if rc < 0 { Err(format!("mpv property {}={}: {}", name, value, self.err(rc))) } else { Ok(()) }
    }

    pub fn cmd(&self, args: &[&str]) -> Result<(), String> {
        let owned: Vec<CString> = args.iter().map(|a| Self::cstr(a)).collect::<Result<_, _>>()?;
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        let rc = unsafe { (self.api.command)(self.h, ptrs.as_ptr()) };
        if rc < 0 { Err(format!("mpv {}: {}", args.first().unwrap_or(&""), self.err(rc))) } else { Ok(()) }
    }

    pub fn cmd_owned(&self, args: &[String]) -> Result<(), String> {
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.cmd(&refs)
    }

    pub fn get_str(&self, name: &str) -> Option<String> {
        let n = CString::new(name).ok()?;
        let p = unsafe { (self.api.get_property_string)(self.h, n.as_ptr()) };
        if p.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { (self.api.free)(p as *mut c_void) };
        Some(s)
    }

    pub fn get_f64(&self, name: &str) -> Option<f64> {
        let n = CString::new(name).ok()?;
        let mut v: f64 = 0.0;
        let rc = unsafe { (self.api.get_property)(self.h, n.as_ptr(), FORMAT_DOUBLE, &mut v as *mut f64 as *mut c_void) };
        if rc < 0 { None } else { Some(v) }
    }

    pub fn get_i64(&self, name: &str) -> Option<i64> {
        let n = CString::new(name).ok()?;
        let mut v: i64 = 0;
        let rc = unsafe { (self.api.get_property)(self.h, n.as_ptr(), FORMAT_INT64, &mut v as *mut i64 as *mut c_void) };
        if rc < 0 { None } else { Some(v) }
    }

    pub fn get_flag(&self, name: &str) -> Option<bool> {
        let n = CString::new(name).ok()?;
        let mut v: c_int = 0;
        let rc = unsafe { (self.api.get_property)(self.h, n.as_ptr(), FORMAT_FLAG, &mut v as *mut c_int as *mut c_void) };
        if rc < 0 { None } else { Some(v != 0) }
    }

    /// Chapters stored inside the video file: (start second, title).
    pub fn chapters(&self) -> Vec<(f64, String)> {
        let count = self.get_i64("chapter-list/count").unwrap_or(0);
        (0..count)
            .filter_map(|i| {
                let t = self.get_f64(&format!("chapter-list/{}/time", i))?;
                Some((t, self.get_str(&format!("chapter-list/{}/title", i)).unwrap_or_default()))
            })
            .collect()
    }

    pub fn tracks(&self, kind: &str) -> Vec<Track> {
        let count = self.get_i64("track-list/count").unwrap_or(0);
        let mut out = Vec::new();
        for i in 0..count {
            if self.get_str(&format!("track-list/{}/type", i)).as_deref() != Some(kind) {
                continue;
            }
            out.push(Track {
                id: self.get_i64(&format!("track-list/{}/id", i)).unwrap_or(0),
                lang: self.get_str(&format!("track-list/{}/lang", i)).unwrap_or_default(),
                title: self.get_str(&format!("track-list/{}/title", i)).unwrap_or_default(),
                selected: self.get_flag(&format!("track-list/{}/selected", i)).unwrap_or(false),
                external: self.get_flag(&format!("track-list/{}/external", i)).unwrap_or(false),
                file: self.get_str(&format!("track-list/{}/external-filename", i)).unwrap_or_default(),
            });
        }
        out
    }

    /// Asks mpv to report every change of a property as `Evt::Prop` (mpv_observe_property): `double` for numbers
    /// like time-pos, otherwise a yes/no flag like pause.
    pub fn observe(&self, name: &str, double: bool) -> Result<(), String> {
        let n = CString::new(name).map_err(|e| e.to_string())?;
        let rc = unsafe { (self.api.observe_property)(self.h, 0, n.as_ptr(), if double { FORMAT_DOUBLE } else { FORMAT_FLAG }) };
        if rc < 0 {
            Err(self.err(rc))
        } else {
            Ok(())
        }
    }

    pub fn events(&self) -> Vec<Evt> {
        let mut out = Vec::new();
        loop {
            let ev = unsafe { (self.api.wait_event)(self.h, 0.0) };
            if ev.is_null() {
                break;
            }
            let e = unsafe { &*ev };
            match e.id {
                0 => break,
                1 => out.push(Evt::Shutdown),
                7 => {
                    let (reason, error) = if e.data.is_null() {
                        (0, 0)
                    } else {
                        let d = unsafe { &*(e.data as *const EndFile) };
                        (d.reason, d.error)
                    };
                    out.push(Evt::EndFile { reason, error: if error < 0 { self.err(error) } else { String::new() } });
                }
                8 => out.push(Evt::FileLoaded),
                21 => out.push(Evt::PlaybackRestart),
                22 if !e.data.is_null() => {
                    let p = unsafe { &*(e.data as *const EventProperty) };
                    let name = if p.name.is_null() { String::new() } else { unsafe { CStr::from_ptr(p.name) }.to_string_lossy().into_owned() };
                    let value = if p.data.is_null() {
                        None
                    } else {
                        match p.format {
                            FORMAT_DOUBLE => Some(PropValue::Double(unsafe { *(p.data as *const f64) })),
                            FORMAT_FLAG => Some(PropValue::Flag(unsafe { *(p.data as *const c_int) } != 0)),
                            _ => None,
                        }
                    };
                    out.push(Evt::Prop(name, value));
                }
                _ => {}
            }
        }
        out
    }

    /// Must be called with the OpenGL context current (inside the window's creation callback).
    pub fn init_render(&mut self, on_update: Box<dyn Fn() + Send + Sync>) -> Result<(), String> {
        let api_type = CString::new("opengl").unwrap();
        let mut init = OpenglInit { get_proc_address: gl_loader::get_proc, get_proc_address_ctx: null_mut(), extra_exts: std::ptr::null() };
        let mut params = [
            RenderParam { kind: PARAM_API_TYPE, data: api_type.as_ptr() as *mut c_void },
            RenderParam { kind: PARAM_OPENGL_INIT, data: &mut init as *mut OpenglInit as *mut c_void },
            RenderParam { kind: 0, data: null_mut() },
        ];
        let mut ctx: *mut c_void = null_mut();
        let rc = unsafe { (self.api.render_create)(&mut ctx, self.h, params.as_mut_ptr()) };
        if rc < 0 || ctx.is_null() {
            return Err(format!("mpv could not set up OpenGL drawing: {}", self.err(rc)));
        }
        self.render = ctx;
        let boxed: Box<Box<dyn Fn() + Send + Sync>> = Box::new(on_update);
        self.update_data = Box::into_raw(boxed) as *mut c_void;
        unsafe { (self.api.render_set_update_callback)(self.render, Some(on_update_trampoline()), self.update_data) };
        Ok(())
    }

    /// The video is drawn into memory by mpv (no OpenGL needed): used when the window is drawn by Vulkan.
    pub fn init_render_sw(&mut self, on_update: Box<dyn Fn() + Send + Sync>) -> Result<(), String> {
        let api_type = CString::new("sw").unwrap();
        let mut params = [
            RenderParam { kind: PARAM_API_TYPE, data: api_type.as_ptr() as *mut c_void },
            RenderParam { kind: 0, data: null_mut() },
        ];
        let mut ctx: *mut c_void = null_mut();
        let rc = unsafe { (self.api.render_create)(&mut ctx, self.h, params.as_mut_ptr()) };
        if rc < 0 || ctx.is_null() {
            return Err(format!("mpv could not set up software drawing: {}", self.err(rc)));
        }
        self.render = ctx;
        let boxed: Box<Box<dyn Fn() + Send + Sync>> = Box::new(on_update);
        self.update_data = Box::into_raw(boxed) as *mut c_void;
        unsafe { (self.api.render_set_update_callback)(self.render, Some(on_update_trampoline()), self.update_data) };
        Ok(())
    }

    /// mpv draws with the graphics card (shaders such as Anime4K included) into an OpenGL window nobody sees, and each
    /// frame is read back into memory (see `render_hidden`). Used when the visible window is drawn by Vulkan.
    pub fn init_render_hidden(&mut self, on_update: Box<dyn Fn() + Send + Sync>) -> Result<(), String> {
        #[cfg(windows)]
        {
            let gl = hidden_gl::HiddenGl::new()?;
            gl.make_current();
            hidden_gl::HIDDEN.with(|h| *h.borrow_mut() = Some(gl));
            crate::util::breadcrumb("hidden GL: mpv render context");
            self.init_render(on_update)
        }
        #[cfg(not(windows))]
        {
            let _ = on_update;
            Err("hidden OpenGL drawing is only for Windows".into())
        }
    }

    /// One frame of the hidden OpenGL drawing, `w` x `h`, as B, G, R, unused bytes. False if it could not be made.
    pub fn render_hidden(&self, w: i32, h: i32, buf: &mut [u8]) -> bool {
        #[cfg(windows)]
        {
            if self.render.is_null() || buf.len() < (w as usize) * (h as usize) * 4 {
                return false;
            }
            hidden_gl::HIDDEN.with(|cell| {
                let mut b = cell.borrow_mut();
                let Some(gl) = b.as_mut() else { return false };
                gl.make_current();
                crate::util::breadcrumb("hidden GL: framebuffer");
                let Some(fbo) = gl.fbo_for(w, h) else { return false };
                crate::util::breadcrumb("hidden GL: mpv draws a frame");
                self.render(fbo as i32, w, h);
                crate::util::breadcrumb("hidden GL: reading the frame back");
                let ok = gl.read_back(fbo, w, h, buf);
                crate::util::breadcrumb("hidden GL: frame done");
                ok
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (w, h, buf);
            false
        }
    }

    /// Draws the current frame, scaled to `w` x `h`, as 4 bytes per pixel in B, G, R, unused order.
    pub fn render_sw(&self, w: i32, h: i32, buf: &mut [u8]) {
        self.render_sw_opt(w, h, buf, true);
    }

    /// Like `render_sw`, but returns at once instead of waiting for the moment the frame is due (mpv waits by
    /// default, up to a frame's time): for a caller on the interface thread that must not stall.
    pub fn render_sw_now(&self, w: i32, h: i32, buf: &mut [u8]) {
        self.render_sw_opt(w, h, buf, false);
    }

    fn render_sw_opt(&self, w: i32, h: i32, buf: &mut [u8], wait: bool) {
        if self.render.is_null() || w <= 0 || h <= 0 || buf.len() < (w as usize) * (h as usize) * 4 {
            return;
        }
        let mut size: [c_int; 2] = [w, h];
        let format = CString::new("bgr0").unwrap();
        let mut stride: usize = (w as usize) * 4;
        let mut block: c_int = if wait { 1 } else { 0 };
        let mut params = [
            RenderParam { kind: PARAM_SW_SIZE, data: size.as_mut_ptr() as *mut c_void },
            RenderParam { kind: PARAM_SW_FORMAT, data: format.as_ptr() as *mut c_void },
            RenderParam { kind: PARAM_SW_STRIDE, data: &mut stride as *mut usize as *mut c_void },
            RenderParam { kind: PARAM_SW_POINTER, data: buf.as_mut_ptr() as *mut c_void },
            RenderParam { kind: PARAM_BLOCK_FOR_TARGET_TIME, data: &mut block as *mut c_int as *mut c_void },
            RenderParam { kind: 0, data: null_mut() },
        ];
        unsafe { (self.api.render_render)(self.render, params.as_mut_ptr()) };
    }

    /// Draws the current video frame into the framebuffer. Call from the GL thread only.
    pub fn render(&self, fbo: i32, w: i32, h: i32) {
        // Set REELHOUSE_NOFLIP=1 if the picture ever shows up upside down on some graphics driver.
        self.render_flip(fbo, w, h, std::env::var_os("REELHOUSE_NOFLIP").is_none());
    }

    /// Draws into the OpenGL framebuffer `fbo`. `flip` is false when the target is presented the right way up already (Qt Quick).
    pub fn render_flip(&self, fbo: i32, w: i32, h: i32, flip: bool) {
        if self.render.is_null() {
            return;
        }
        let mut f = OpenglFbo { fbo, w, h, internal_format: 0 };
        let mut flip: c_int = if flip { 1 } else { 0 };
        let mut params = [
            RenderParam { kind: PARAM_OPENGL_FBO, data: &mut f as *mut OpenglFbo as *mut c_void },
            RenderParam { kind: PARAM_FLIP_Y, data: &mut flip as *mut c_int as *mut c_void },
            RenderParam { kind: 0, data: null_mut() },
        ];
        unsafe { (self.api.render_render)(self.render, params.as_mut_ptr()) };
    }

    pub fn has_render(&self) -> bool {
        !self.render.is_null()
    }
}

fn on_update_trampoline() -> UpdateCb {
    on_update
}

impl Drop for Mpv {
    fn drop(&mut self) {
        // Stop playback so the sound ends. The render context and the player are deliberately left for the
        // operating system to reclaim: freeing GL objects after the window is gone can crash on exit.
        let _ = self.cmd(&["stop"]);
        // A player without a render context (the preview-picture one) can be freed safely.
        if self.render.is_null() {
            unsafe { (self.api.terminate_destroy)(self.h) };
        }
    }
}

/// The OpenGL loader of the window that owns the context (Qt: QOpenGLContext::getProcAddress). It is asked first:
/// it knows the context really current (desktop WGL, or Qt's software OpenGL), which a lookup in opengl32 may not.
static HOST_GL_PROC: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn set_host_gl_loader(f: Option<unsafe extern "C" fn(*const c_char) -> *mut c_void>) {
    HOST_GL_PROC.store(f.map(|f| f as usize).unwrap_or(0), std::sync::atomic::Ordering::SeqCst);
}

fn host_gl_proc(name: *const c_char) -> *mut c_void {
    let f = HOST_GL_PROC.load(std::sync::atomic::Ordering::SeqCst);
    if f == 0 {
        return null_mut();
    }
    let f: unsafe extern "C" fn(*const c_char) -> *mut c_void = unsafe { std::mem::transmute(f) };
    unsafe { f(name) }
}

#[cfg(windows)]
mod gl_loader {
    use libloading::Library;
    use std::ffi::{c_char, c_void, CStr};
    use std::ptr::null_mut;
    use std::sync::OnceLock;

    struct Gl {
        lib: Library,
        wgl_get_proc_address: unsafe extern "system" fn(*const c_char) -> *mut c_void,
    }

    static GL: OnceLock<Option<Gl>> = OnceLock::new();

    fn gl() -> Option<&'static Gl> {
        GL.get_or_init(|| unsafe {
            let lib = Library::new("opengl32.dll").ok()?;
            let f: unsafe extern "system" fn(*const c_char) -> *mut c_void = *lib.get(b"wglGetProcAddress\0").ok()?;
            Some(Gl { lib, wgl_get_proc_address: f })
        })
        .as_ref()
    }

    pub unsafe extern "C" fn get_proc(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
        let h = super::host_gl_proc(name);
        if !h.is_null() {
            return h;
        }
        let Some(gl) = gl() else { return null_mut() };
        let p = (gl.wgl_get_proc_address)(name);
        let v = p as usize;
        // wglGetProcAddress returns 0..3 or -1 for functions that live in opengl32.dll itself (GL 1.1).
        if v <= 3 || v == usize::MAX {
            match gl.lib.get::<unsafe extern "system" fn()>(CStr::from_ptr(name).to_bytes_with_nul()) {
                Ok(s) => *s as usize as *mut c_void,
                Err(_) => null_mut(),
            }
        } else {
            p
        }
    }
}

#[cfg(not(windows))]
mod gl_loader {
    use libloading::Library;
    use std::ffi::{c_char, c_void};
    use std::ptr::null_mut;
    use std::sync::OnceLock;

    struct Gl {
        _lib: Library,
        get_proc_address: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    }

    static GL: OnceLock<Option<Gl>> = OnceLock::new();

    fn gl() -> Option<&'static Gl> {
        GL.get_or_init(|| unsafe {
            // Android has no versioned sonames (libEGL.so); iOS has no EGL at all, where the host (Qt) answers every
            // name through `host_gl_proc` above.
            for (lib_name, sym_name) in [
                ("libEGL.so.1", &b"eglGetProcAddress\0"[..]),
                ("libEGL.so", &b"eglGetProcAddress\0"[..]),
                ("libGL.so.1", &b"glXGetProcAddressARB\0"[..]),
                ("libGL.so", &b"glXGetProcAddress\0"[..]),
            ] {
                if let Ok(lib) = Library::new(lib_name) {
                    let f: Result<unsafe extern "C" fn(*const c_char) -> *mut c_void, _> = lib.get(sym_name).map(|s| *s);
                    if let Ok(f) = f {
                        return Some(Gl { _lib: lib, get_proc_address: f });
                    }
                }
            }
            None
        })
        .as_ref()
    }

    pub unsafe extern "C" fn get_proc(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
        let h = super::host_gl_proc(name);
        if !h.is_null() {
            return h;
        }
        match gl() {
            Some(g) => (g.get_proc_address)(name),
            None => null_mut(),
        }
    }
}

/// The framebuffer bound for the current draw.
pub fn current_fbo() -> i32 {
    unsafe {
        let p = gl_loader::get_proc(null_mut(), b"glGetIntegerv\0".as_ptr() as *const c_char);
        if p.is_null() {
            return 0;
        }
        let f: unsafe extern "C" fn(u32, *mut i32) = std::mem::transmute(p);
        let mut v: i32 = 0;
        f(0x8CA6, &mut v); // GL_FRAMEBUFFER_BINDING
        v
    }
}

/// A small invisible OpenGL window (Windows) that mpv draws into when the visible window is drawn by Vulkan.
#[cfg(windows)]
mod hidden_gl {
    use super::gl_loader;
    use libloading::Library;
    use std::cell::RefCell;
    use std::ffi::{c_char, c_int, c_void, CString};
    use std::ptr::null_mut;

    #[repr(C)]
    struct Pfd {
        size: u16,
        version: u16,
        flags: u32,
        pixel_type: u8,
        color_bits: u8,
        rest: [u8; 16],
        layer_type: u8,
        reserved: u8,
        masks: [u32; 3],
    }

    type Fp = *mut c_void;
    thread_local! {
        pub static HIDDEN: RefCell<Option<HiddenGl>> = const { RefCell::new(None) };
    }

    pub struct HiddenGl {
        hdc: Fp,
        rc: Fp,
        wgl_make_current: unsafe extern "system" fn(Fp, Fp) -> c_int,
        _libs: Vec<Library>,
        fbo: u32,
        tex: u32,
        size: (i32, i32),
        gen_fb: unsafe extern "system" fn(i32, *mut u32),
        bind_fb: unsafe extern "system" fn(u32, u32),
        fb_tex: unsafe extern "system" fn(u32, u32, u32, u32, i32),
        gen_tex: unsafe extern "system" fn(i32, *mut u32),
        bind_tex: unsafe extern "system" fn(u32, u32),
        tex_image: unsafe extern "system" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void),
        tex_param: unsafe extern "system" fn(u32, u32, i32),
        pixel_store: unsafe extern "system" fn(u32, i32),
        read_px: unsafe extern "system" fn(i32, i32, i32, i32, u32, u32, *mut c_void),
        del_tex: unsafe extern "system" fn(i32, *const u32),
        del_fb: unsafe extern "system" fn(i32, *const u32),
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn proc<T: Copy>(name: &str) -> Result<T, String> {
        let c = CString::new(name).unwrap();
        let p = unsafe { gl_loader::get_proc(null_mut(), c.as_ptr() as *const c_char) };
        if p.is_null() {
            return Err(format!("OpenGL function {} is missing", name));
        }
        Ok(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
    }

    impl HiddenGl {
        pub fn new() -> Result<HiddenGl, String> {
            unsafe {
                crate::util::breadcrumb("hidden GL: loading libraries");
                let user32 = Library::new("user32.dll").map_err(|e| e.to_string())?;
                let gdi32 = Library::new("gdi32.dll").map_err(|e| e.to_string())?;
                let ogl = Library::new("opengl32.dll").map_err(|e| e.to_string())?;
                let create_window: unsafe extern "system" fn(u32, *const u16, *const u16, u32, i32, i32, i32, i32, Fp, Fp, Fp, Fp) -> Fp =
                    *user32.get(b"CreateWindowExW\0").map_err(|e| e.to_string())?;
                let get_dc: unsafe extern "system" fn(Fp) -> Fp = *user32.get(b"GetDC\0").map_err(|e| e.to_string())?;
                let choose: unsafe extern "system" fn(Fp, *const Pfd) -> c_int = *gdi32.get(b"ChoosePixelFormat\0").map_err(|e| e.to_string())?;
                let set_fmt: unsafe extern "system" fn(Fp, c_int, *const Pfd) -> c_int = *gdi32.get(b"SetPixelFormat\0").map_err(|e| e.to_string())?;
                let create_ctx: unsafe extern "system" fn(Fp) -> Fp = *ogl.get(b"wglCreateContext\0").map_err(|e| e.to_string())?;
                let make_cur: unsafe extern "system" fn(Fp, Fp) -> c_int = *ogl.get(b"wglMakeCurrent\0").map_err(|e| e.to_string())?;

                crate::util::breadcrumb("hidden GL: creating the window");
                // WS_POPUP, never shown: it only gives OpenGL a surface.
                let hwnd = create_window(0, wide("STATIC").as_ptr(), wide("").as_ptr(), 0x8000_0000, 0, 0, 16, 16, null_mut(), null_mut(), null_mut(), null_mut());
                if hwnd.is_null() {
                    return Err("could not create the hidden OpenGL window".into());
                }
                let hdc = get_dc(hwnd);
                if hdc.is_null() {
                    return Err("could not get a device context for the hidden OpenGL window".into());
                }
                let mut pfd: Pfd = std::mem::zeroed();
                pfd.size = std::mem::size_of::<Pfd>() as u16;
                pfd.version = 1;
                pfd.flags = 0x04 | 0x20 | 0x01; // draw to window, support OpenGL, double buffer
                pfd.pixel_type = 0; // RGBA
                pfd.color_bits = 32;
                let fmt = choose(hdc, &pfd);
                if fmt == 0 || set_fmt(hdc, fmt, &pfd) == 0 {
                    return Err("no pixel format for the hidden OpenGL window".into());
                }
                crate::util::breadcrumb("hidden GL: creating the context");
                let rc = create_ctx(hdc);
                if rc.is_null() {
                    return Err("could not create the hidden OpenGL context".into());
                }
                if make_cur(hdc, rc) == 0 {
                    return Err("could not use the hidden OpenGL context".into());
                }
                crate::util::breadcrumb("hidden GL: loading the OpenGL functions");
                Ok(HiddenGl {
                    hdc,
                    rc,
                    wgl_make_current: make_cur,
                    _libs: vec![user32, gdi32, ogl],
                    fbo: 0,
                    tex: 0,
                    size: (0, 0),
                    gen_fb: proc("glGenFramebuffers")?,
                    bind_fb: proc("glBindFramebuffer")?,
                    fb_tex: proc("glFramebufferTexture2D")?,
                    gen_tex: proc("glGenTextures")?,
                    bind_tex: proc("glBindTexture")?,
                    tex_image: proc("glTexImage2D")?,
                    tex_param: proc("glTexParameteri")?,
                    pixel_store: proc("glPixelStorei")?,
                    read_px: proc("glReadPixels")?,
                    del_tex: proc("glDeleteTextures")?,
                    del_fb: proc("glDeleteFramebuffers")?,
                })
            }
        }

        pub fn make_current(&self) {
            unsafe { (self.wgl_make_current)(self.hdc, self.rc) };
        }

        /// The framebuffer to draw into, made again when the size changes.
        pub fn fbo_for(&mut self, w: i32, h: i32) -> Option<u32> {
            if w <= 0 || h <= 0 {
                return None;
            }
            unsafe {
                if self.size != (w, h) || self.fbo == 0 {
                    if self.fbo != 0 {
                        (self.del_fb)(1, &self.fbo);
                        (self.del_tex)(1, &self.tex);
                    }
                    (self.gen_tex)(1, &mut self.tex);
                    (self.bind_tex)(0x0DE1, self.tex);
                    (self.tex_param)(0x0DE1, 0x2801, 0x2601); // min filter: linear (no mipmaps)
                    (self.tex_param)(0x0DE1, 0x2800, 0x2601);
                    (self.tex_image)(0x0DE1, 0, 0x8058, w, h, 0, 0x1908, 0x1401, std::ptr::null());
                    (self.gen_fb)(1, &mut self.fbo);
                    (self.bind_fb)(0x8D40, self.fbo);
                    (self.fb_tex)(0x8D40, 0x8CE0, 0x0DE1, self.tex, 0);
                    self.size = (w, h);
                }
            }
            Some(self.fbo)
        }

        /// Copies the picture drawn into the framebuffer to memory, B, G, R, unused bytes per pixel.
        pub fn read_back(&self, fbo: u32, w: i32, h: i32, buf: &mut [u8]) -> bool {
            unsafe {
                (self.bind_fb)(0x8D40, fbo);
                (self.pixel_store)(0x0D05, 1); // pack alignment
                (self.read_px)(0, 0, w, h, 0x80E1, 0x1401, buf.as_mut_ptr() as *mut c_void); // BGRA, bytes
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_header_list() {
        let h = vec![("Accept".to_string(), "a/b, c/d".to_string()), ("X".to_string(), "p\\q".to_string())];
        assert_eq!(header_list(&h), "Accept: a/b\\, c/d,X: p\\\\q");
    }
}
