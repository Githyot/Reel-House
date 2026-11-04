//! IP protection: the "VPN inside the app" (Preferences > General > IP protection).
//!
//! Torrent peers, trackers, addon servers and every site the app talks to see the IP address that the traffic comes
//! from. With protection on, the app's traffic leaves through a tunnel instead, and never straight from this device:
//!
//! - **proxy**: any VPN or proxy service the user has (SOCKS5 or HTTP proxy, `socks5://user:pass@host:port`).
//! - **wireguard**: a WireGuard file (`.conf`) of any VPN that offers one, run INSIDE the app (userspace WireGuard,
//!   `third_party/tokio-wireguard`: no admin rights, no system network device, works on Android without VpnService).
//! - **tor**: the Tor network through Arti, the Tor Project's own Rust client, built into the app. Free and run by
//!   volunteers; slow, and the Tor Project asks people not to use it for torrents, so torrents are refused on Tor
//!   unless the user turns `torrent_over_tor` on.
//! - **system**: a VPN app of the system (WireGuard, OpenVPN, Proton VPN…) does the work; the app only checks that a
//!   tunnel device is up and stops when it is not.
//!
//! How: a small gateway listens on 127.0.0.1 (one port for the whole run) and speaks both SOCKS5 and HTTP proxy
//! (CONNECT and plain requests). Everything in the app points at it: the HTTP client (`net.rs`), the torrent engine
//! (rqbit, peers over SOCKS5 with DHT, UPnP, LSD, uTP and incoming connections off), mpv/ffmpeg, the Qt pictures, the
//! research browser and the helper programs (environment `HTTP(S)_PROXY`). The gateway forwards each connection
//! through the chosen tunnel and resolves names there too (no DNS leak).
//!
//! Kill switch: while protection is on nothing ever falls back to a direct connection. When the tunnel is still
//! connecting or went down, the gateway refuses the connection, so requests fail instead of leaking; the bridge also
//! stops the torrent engine when the tunnel drops (`tunnel_down`).
use crate::json::Json;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

// ---------------------------------------------------------------------------------------------------------------
// The user's choice (<data>/privacy.json, shared by every profile; kept out of backups: it holds private keys)
// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Cfg {
    /// "off", "proxy", "wireguard", "tor" or "system".
    pub mode: String,
    /// For "proxy": `socks5://[user:pass@]host:port`, `socks5h://…` or `http://[user:pass@]host:port`.
    pub proxy_url: String,
    /// For "wireguard": the text of a wg-quick file.
    pub wg_conf: String,
    /// What the WireGuard file is called (its file name, or "Cloudflare WARP").
    pub wg_name: String,
    /// Torrents on Tor (off: torrents are refused while on Tor).
    pub torrent_over_tor: bool,
}

impl Default for Cfg {
    fn default() -> Self {
        Cfg { mode: "off".into(), proxy_url: String::new(), wg_conf: String::new(), wg_name: String::new(), torrent_over_tor: false }
    }
}

impl Cfg {
    pub fn from_json(j: &Json) -> Cfg {
        let mode = j.get("mode").str();
        Cfg {
            mode: if ["proxy", "wireguard", "tor", "system"].contains(&mode.as_str()) { mode } else { "off".into() },
            proxy_url: j.get("proxy_url").str().trim().to_string(),
            wg_conf: j.get("wg_conf").str(),
            wg_name: j.get("wg_name").str(),
            torrent_over_tor: j.get("torrent_over_tor").as_bool().unwrap_or(false),
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("mode", self.mode.clone().into()),
            ("proxy_url", self.proxy_url.clone().into()),
            ("wg_conf", self.wg_conf.clone().into()),
            ("wg_name", self.wg_name.clone().into()),
            ("torrent_over_tor", self.torrent_over_tor.into()),
        ])
    }

    fn path() -> std::path::PathBuf {
        crate::util::data_dir().join("privacy.json")
    }

    pub fn load() -> Cfg {
        std::fs::read_to_string(Self::path()).ok().and_then(|t| Json::parse(&t).ok()).map(|j| Cfg::from_json(&j)).unwrap_or_default()
    }

    fn save(&self) {
        let p = Self::path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(&p, self.to_json().pretty());
        // the file holds a private key or a password: only this user may read it
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// State seen by the rest of the app
// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Off,
    Connecting,
    Ready,
    /// Was ready, the tunnel stopped answering (kill switch holds; tried again every few seconds).
    Down,
    Error,
}

impl Phase {
    fn id(self) -> &'static str {
        match self {
            Phase::Off => "off",
            Phase::Connecting => "connecting",
            Phase::Ready => "ready",
            Phase::Down => "down",
            Phase::Error => "error",
        }
    }
}

struct State {
    cfg: Cfg,
    phase: Phase,
    message: String,
    /// The address the internet sees, found by `check` ("" until checked).
    exit_ip: String,
    /// Why the last check of the address failed.
    check_error: String,
    /// Tor's start-up progress (0-100), only on Tor.
    progress: u32,
    upstream: Option<Arc<Upstream>>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
/// The gateway's port (0 = not started yet).
static PORT: AtomicU16 = AtomicU16::new(0);
/// Grows when anything shown changes: the bridge sends a "privacy" event when it moves.
static GEN: AtomicU64 = AtomicU64::new(0);
/// Grows on each new tunnel: an old connect task that finishes late is ignored.
static EPOCH: AtomicU64 = AtomicU64::new(0);
/// The mode is not "off" (read on every request, so kept outside the lock).
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// The tunnel works now.
static READY: AtomicBool = AtomicBool::new(false);
/// Set when a ready tunnel went down: the bridge stops the torrent engine and clears it.
static DROPPED: AtomicBool = AtomicBool::new(false);

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State { cfg: Cfg::default(), phase: Phase::Off, message: String::new(), exit_ip: String::new(), check_error: String::new(), progress: 0, upstream: None }))
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|p| p.into_inner())
}

fn changed() {
    GEN.fetch_add(1, Ordering::Relaxed);
}

fn set_phase(epoch: u64, phase: Phase, message: &str) {
    if EPOCH.load(Ordering::Relaxed) != epoch {
        return;
    }
    let mut s = lock();
    if s.phase == Phase::Ready && phase != Phase::Ready {
        DROPPED.store(true, Ordering::Relaxed);
    }
    if s.phase != phase || s.message != message {
        crate::debug::log("privacy", &format!("{} {}", phase.id(), message));
    }
    s.phase = phase;
    s.message = message.to_string();
    READY.store(phase == Phase::Ready, Ordering::Relaxed);
    if phase != Phase::Ready {
        s.exit_ip.clear();
    }
    drop(s);
    changed();
}

/// Counter of changes (compare with a saved value).
pub fn generation() -> u64 {
    GEN.load(Ordering::Relaxed)
}

/// Protection is chosen (whatever the tunnel's state): nothing may go out directly.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// The tunnel works now.
pub fn ready() -> bool {
    READY.load(Ordering::Relaxed)
}

/// True once after the tunnel went down (the caller stops the torrent engine).
pub fn take_dropped() -> bool {
    DROPPED.swap(false, Ordering::Relaxed)
}

pub fn mode() -> String {
    lock().cfg.mode.clone()
}

fn gateway_mode() -> bool {
    active() && mode() != "system"
}

/// The gateway as an HTTP proxy ("http://127.0.0.1:port"), when the traffic must go through it.
pub fn http_proxy() -> Option<String> {
    let p = proxy_port();
    (p != 0).then(|| format!("http://127.0.0.1:{}", p))
}

/// The gateway as a SOCKS5 proxy ("socks5://127.0.0.1:port"), for the torrent engine and the browser.
pub fn socks_proxy() -> Option<String> {
    let p = proxy_port();
    (p != 0).then(|| format!("socks5://127.0.0.1:{}", p))
}

/// The gateway's port when the traffic must go through it, else 0 (for the Qt side). If the gateway could not
/// open, port 1 (nothing listens there): everything is refused instead of going out straight.
pub fn proxy_port() -> u16 {
    if gateway_mode() {
        match PORT.load(Ordering::Relaxed) {
            0 => 1,
            p => p,
        }
    } else {
        0
    }
}

/// The port Qt and the browser use: the gateway's, or 1 (nothing listens: all refused) while the system VPN of the
/// "system" mode is off, so pictures and pages do not go out without it.
pub fn qt_port() -> u16 {
    let p = proxy_port();
    if p == 0 && active() && !ready() && !system_tunnel_up() {
        1
    } else {
        p
    }
}

/// Before a request to another computer: with protection on and the tunnel not working, the request is refused
/// (kill switch) with a message for the user.
pub fn gate() -> Result<(), String> {
    if !active() || ready() {
        return Ok(());
    }
    // the system VPN mode is checked on the spot (the watcher may not have run yet)
    if mode() == "system" && system_tunnel_up() {
        return Ok(());
    }
    let s = lock();
    Err(match s.phase {
        Phase::Connecting => "IP protection is still connecting; nothing goes out without it. Try again in a moment.".to_string(),
        _ if s.cfg.mode == "system" => "IP protection: the system VPN is off, so nothing goes out (turn the VPN on, or change it in Preferences > General).".to_string(),
        _ => format!("IP protection is not working ({}); nothing goes out without it.", if s.message.is_empty() { "the tunnel is down" } else { &s.message }),
    })
}

/// Torrents may start now (Tor refuses them unless the user allowed it).
pub fn torrent_allowed() -> Result<(), String> {
    if !active() {
        return Ok(());
    }
    let s = lock();
    if s.cfg.mode == "tor" && !s.cfg.torrent_over_tor {
        return Err("Torrents do not go over Tor (it is slow and the Tor Project asks not to). Use a WireGuard file or a proxy in Preferences > General > IP protection, or Real-Debrid.".to_string());
    }
    drop(s);
    gate()
}

/// The rqbit program's options with protection on: peers only through the gateway (SOCKS5), and nothing that
/// would show this device's address (DHT and local discovery are UDP from here, incoming connections and UPnP
/// open a port on this address).
pub fn rqbit_args() -> Vec<String> {
    let mut v = Vec::new();
    if !active() {
        return v;
    }
    for a in ["--disable-dht", "--disable-tcp-listen", "--disable-upnp-port-forward", "--disable-lsd"] {
        v.push(a.to_string());
    }
    if let Some(p) = socks_proxy() {
        v.push("--socks-url".into());
        v.push(p);
    }
    v
}

/// The trackers of a magnet that work through the tunnel: UDP trackers cannot go over SOCKS5 or Tor, so each one
/// is also tried over HTTP at the same address (most public trackers answer both), and the UDP ones are removed.
pub fn magnet_for_tunnel(magnet: &str) -> String {
    if !active() {
        return magnet.to_string();
    }
    rewrite_magnet(magnet)
}

fn rewrite_magnet(magnet: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let (head, query) = magnet.split_once('?').unwrap_or((magnet, ""));
    let mut seen = std::collections::HashSet::new();
    for part in query.split('&') {
        if let Some(tr) = part.strip_prefix("tr=") {
            let url = url_decode(tr);
            if let Some(rest) = url.strip_prefix("udp://") {
                // "udp://host:port/announce" -> "http://host:port/announce"
                let rest = rest.split('/').next().unwrap_or(rest);
                let http = format!("http://{}/announce", rest);
                if seen.insert(http.clone()) {
                    out.push(format!("tr={}", crate::util::url_encode(&http)));
                }
                continue;
            }
            if seen.insert(url) {
                out.push(part.to_string());
            }
            continue;
        }
        if !part.is_empty() {
            out.push(part.to_string());
        }
    }
    format!("{}?{}", head, out.join("&"))
}

// ---------------------------------------------------------------------------------------------------------------
// Start / change
// ---------------------------------------------------------------------------------------------------------------

/// At start-up: reads the saved choice and connects.
pub fn init() {
    let cfg = Cfg::load();
    start(cfg, false);
}

/// The user's new choice: saved and applied (the old tunnel is closed).
pub fn apply(cfg: Cfg) -> Result<(), String> {
    match cfg.mode.as_str() {
        "proxy" => {
            Proxy::parse(&cfg.proxy_url)?;
        }
        "wireguard" => {
            WgFile::parse(&cfg.wg_conf)?;
        }
        _ => {}
    }
    start(cfg, true);
    Ok(())
}

fn start(cfg: Cfg, save: bool) {
    if save {
        cfg.save();
    }
    let epoch = EPOCH.fetch_add(1, Ordering::Relaxed) + 1;
    let on = cfg.mode != "off";
    {
        let mut s = lock();
        s.upstream = None;
        s.exit_ip.clear();
        s.check_error.clear();
        s.progress = 0;
        s.phase = if on { Phase::Connecting } else { Phase::Off };
        s.message.clear();
        s.cfg = cfg.clone();
    }
    READY.store(false, Ordering::Relaxed);
    ACTIVE.store(on, Ordering::Relaxed);
    set_child_env();
    changed();
    crate::debug::log("privacy", &format!("mode {}", cfg.mode));
    if !on {
        return;
    }
    if cfg.mode == "system" {
        std::thread::Builder::new().name("privacy-system".into()).spawn(move || watch_system(epoch)).ok();
        return;
    }
    if let Err(e) = ensure_gateway() {
        set_phase(epoch, Phase::Error, &e);
        return;
    }
    set_child_env();
    changed();
    runtime().spawn(connect(cfg, epoch));
}

/// Helper programs (yt-dlp, Ollama, ffmpeg inside mpv, the upscaler) read these: they go through the gateway too.
fn set_child_env() {
    match http_proxy() {
        Some(p) => {
            for k in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY", "all_proxy"] {
                std::env::set_var(k, &p);
            }
            for k in ["NO_PROXY", "no_proxy"] {
                std::env::set_var(k, "localhost,127.0.0.1,::1");
            }
        }
        None => {
            for k in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY", "all_proxy", "NO_PROXY", "no_proxy"] {
                std::env::remove_var(k);
            }
        }
    }
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).thread_name("privacy").enable_all().build().expect("privacy runtime"))
}

fn ensure_gateway() -> Result<(), String> {
    if PORT.load(Ordering::Relaxed) != 0 {
        return Ok(());
    }
    let std_listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("could not open the local gateway ({})", e))?;
    std_listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = std_listener.local_addr().map_err(|e| e.to_string())?.port();
    let rt = runtime();
    let listener = {
        let _g = rt.enter();
        tokio::net::TcpListener::from_std(std_listener).map_err(|e| e.to_string())?
    };
    rt.spawn(serve(listener));
    PORT.store(port, Ordering::Relaxed);
    crate::debug::log("privacy", &format!("gateway on 127.0.0.1:{}", port));
    Ok(())
}

async fn connect(cfg: Cfg, epoch: u64) {
    // a tunnel that never answers must not leave the switch "connecting" forever (Tor has its own 3 minute limit)
    let limit = Duration::from_secs(if cfg.mode == "tor" { 200 } else { 45 });
    let made = tokio::time::timeout(limit, make_upstream(&cfg, epoch)).await.unwrap_or_else(|_| Err(format!("the tunnel did not answer in {} seconds", limit.as_secs())));
    let up = match made {
        Ok(u) => Arc::new(u),
        Err(e) => {
            set_phase(epoch, Phase::Error, &e);
            return;
        }
    };
    if EPOCH.load(Ordering::Relaxed) != epoch {
        return;
    }
    lock().upstream = Some(up.clone());
    // the first connection through the tunnel proves it works
    match probe(&up).await {
        Ok(()) => set_phase(epoch, Phase::Ready, ""),
        Err(e) => set_phase(epoch, Phase::Error, &format!("the tunnel does not reach the internet ({})", e)),
    }
    watch(up, epoch).await;
}

/// Every 30 s (every 8 s while down) a connection through the tunnel: two failures in a row = down.
async fn watch(up: Arc<Upstream>, epoch: u64) {
    let mut fails = 0;
    loop {
        let wait = if ready() { 30 } else { 8 };
        tokio::time::sleep(Duration::from_secs(wait)).await;
        if EPOCH.load(Ordering::Relaxed) != epoch {
            return;
        }
        match probe(&up).await {
            Ok(()) => {
                fails = 0;
                if !ready() {
                    set_phase(epoch, Phase::Ready, "");
                }
            }
            Err(e) => {
                fails += 1;
                if fails >= 2 && (ready() || lock().phase != Phase::Down) {
                    set_phase(epoch, Phase::Down, &format!("the tunnel stopped answering ({})", e));
                }
            }
        }
    }
}

async fn probe(up: &Upstream) -> Result<(), String> {
    // two well-known HTTPS servers; one answering is enough
    let mut last = String::new();
    for (host, port) in [("one.one.one.one", 443u16), ("dns.google", 443)] {
        match tokio::time::timeout(Duration::from_secs(25), up.dial(host, port)).await {
            Ok(Ok(_)) => return Ok(()),
            Ok(Err(e)) => last = e.to_string(),
            Err(_) => last = "timed out".into(),
        }
    }
    Err(last)
}

/// Saves the address the internet sees through the tunnel (the controller fetches it through the gateway), or why
/// it could not be found.
pub fn set_exit_ip(ip: Result<String, String>) {
    let mut s = lock();
    match ip {
        Ok(ip) => {
            s.exit_ip = ip;
            s.check_error.clear();
        }
        Err(e) => {
            s.exit_ip.clear();
            s.check_error = e;
        }
    }
    drop(s);
    changed();
}

/// For the window: {mode, phase, message, exit_ip, progress, proxy_port, proxy_url, wg_name, wg_has, torrent_over_tor,
/// tor, system_up}. The WireGuard file and the proxy password are not sent.
pub fn state_json() -> Json {
    // before the lock: qt_port() reads the mode, which takes the same lock (a std Mutex is not reentrant; holding
    // it here froze the window as soon as protection was on, e.g. right after "Use WARP")
    let port = qt_port();
    let system_up = mode() == "system" && system_tunnel_up();
    let s = lock();
    let shown_proxy = Proxy::parse(&s.cfg.proxy_url).map(|p| p.shown()).unwrap_or_else(|_| s.cfg.proxy_url.clone());
    let wg_summary = WgFile::parse(&s.cfg.wg_conf).map(|w| w.summary()).unwrap_or_default();
    Json::obj(vec![
        ("mode", s.cfg.mode.clone().into()),
        ("phase", s.phase.id().into()),
        ("message", s.message.clone().into()),
        ("exit_ip", s.exit_ip.clone().into()),
        ("check_error", s.check_error.clone().into()),
        ("progress", (s.progress as i64).into()),
        ("proxy_port", (port as i64).into()),
        ("proxy_url", shown_proxy.into()),
        ("wg_name", s.cfg.wg_name.clone().into()),
        ("wg_has", (!s.cfg.wg_conf.trim().is_empty()).into()),
        ("wg_summary", wg_summary.into()),
        ("torrent_over_tor", s.cfg.torrent_over_tor.into()),
        ("tor", true.into()),
        ("system_up", (s.cfg.mode == "system" && system_up).into()),
    ])
}

/// The current choice (for the command that changes one field).
pub fn cfg() -> Cfg {
    lock().cfg.clone()
}

// ---------------------------------------------------------------------------------------------------------------
// The tunnels
// ---------------------------------------------------------------------------------------------------------------

trait Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> Io for T {}
type Stream = Box<dyn Io>;

enum Upstream {
    Socks(Proxy),
    Http(Proxy),
    Wg { iface: tokio_wireguard::Interface, dns: Vec<std::net::IpAddr> },
    Tor(Arc<arti_client::TorClient<tor_rtcompat::PreferredRuntime>>),
}

async fn make_upstream(cfg: &Cfg, epoch: u64) -> Result<Upstream, String> {
    match cfg.mode.as_str() {
        "proxy" => {
            let p = Proxy::parse(&cfg.proxy_url)?;
            Ok(if p.http { Upstream::Http(p) } else { Upstream::Socks(p) })
        }
        "wireguard" => {
            let w = WgFile::parse(&cfg.wg_conf)?;
            let dns = w.dns.clone();
            let config = w.into_config().await?;
            let iface = tokio_wireguard::Interface::new(config).map_err(|e| format!("WireGuard could not start ({})", e))?;
            Ok(Upstream::Wg { iface, dns })
        }
        "tor" => tor_client(epoch).await.map(Upstream::Tor),
        other => Err(format!("unknown protection mode {}", other)),
    }
}

async fn tor_client(epoch: u64) -> Result<Arc<arti_client::TorClient<tor_rtcompat::PreferredRuntime>>, String> {
    let base = crate::util::data_dir().join("tor");
    let mut b = arti_client::config::TorClientConfigBuilder::from_directories(base.join("state"), base.join("cache"));
    // Windows and Android folders do not have the Unix permissions Arti checks for
    b.storage().permissions().dangerously_trust_everyone();
    let config = b.build().map_err(|e| format!("Tor settings: {}", e))?;
    let client = arti_client::TorClient::builder().config(config).create_unbootstrapped().map_err(|e| format!("Tor could not start ({})", e))?;
    // the start-up progress for the window
    let events = client.bootstrap_events();
    tokio::spawn(async move {
        use futures_lite_stream::next;
        let mut events = events;
        while let Some(st) = next(&mut events).await {
            if EPOCH.load(Ordering::Relaxed) != epoch {
                return;
            }
            let pct = (st.as_frac() * 100.0).round() as u32;
            let mut s = lock();
            if s.progress != pct {
                s.progress = pct;
                drop(s);
                changed();
            }
            if st.ready_for_traffic() {
                return;
            }
        }
    });
    tokio::time::timeout(Duration::from_secs(180), client.bootstrap())
        .await
        .map_err(|_| "Tor did not finish connecting in 3 minutes (is Tor blocked on this network?)".to_string())?
        .map_err(|e| format!("Tor could not connect ({})", e))?;
    Ok(client)
}

/// `futures::StreamExt::next` without the futures crate (Arti's event stream is a `futures::Stream`).
mod futures_lite_stream {
    use std::future::poll_fn;
    use std::pin::Pin;
    pub async fn next<S: futures_core::Stream + Unpin>(s: &mut S) -> Option<S::Item> {
        poll_fn(|cx| Pin::new(&mut *s).poll_next(cx)).await
    }
}

impl Upstream {
    async fn dial(&self, host: &str, port: u16) -> std::io::Result<Stream> {
        use std::io::{Error, ErrorKind};
        match self {
            Upstream::Socks(p) => {
                let mut s = tcp(&p.addr()).await?;
                socks5_handshake(&mut s, p, host, port).await?;
                Ok(Box::new(s))
            }
            Upstream::Http(p) => {
                let mut s = tcp(&p.addr()).await?;
                http_connect(&mut s, p, host, port).await?;
                Ok(Box::new(s))
            }
            Upstream::Wg { iface, dns } => {
                let ip = match host.trim_matches(|c| c == '[' || c == ']').parse::<std::net::IpAddr>() {
                    Ok(ip) => ip,
                    Err(_) => wg_resolve(iface, dns, host).await?,
                };
                let addr = std::net::SocketAddr::new(ip, port);
                let s = tokio::time::timeout(Duration::from_secs(20), tokio_wireguard::TcpStream::connect(addr, iface))
                    .await
                    .map_err(|_| Error::new(ErrorKind::TimedOut, "connect through WireGuard timed out"))??;
                Ok(Box::new(s))
            }
            Upstream::Tor(c) => {
                let s = c.connect((host.trim_matches(|ch| ch == '[' || ch == ']'), port)).await.map_err(|e| Error::other(format!("Tor: {}", e)))?;
                Ok(Box::new(s))
            }
        }
    }
}

async fn tcp(addr: &str) -> std::io::Result<tokio::net::TcpStream> {
    tokio::time::timeout(Duration::from_secs(15), tokio::net::TcpStream::connect(addr))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, format!("{} did not answer", addr)))?
}

/// A proxy address the user typed.
#[derive(Clone, Debug, PartialEq)]
struct Proxy {
    http: bool,
    host: String,
    port: u16,
    user: String,
    pass: String,
}

impl Proxy {
    fn parse(url: &str) -> Result<Proxy, String> {
        let url = url.trim();
        if url.is_empty() {
            return Err("Type the proxy address (socks5://host:port or http://host:port).".into());
        }
        let (scheme, rest) = url.split_once("://").unwrap_or(("socks5", url));
        let http = match scheme.to_lowercase().as_str() {
            "socks5" | "socks5h" | "socks" => false,
            "http" => true,
            "https" => return Err("Proxies over https:// are not supported: use socks5:// or http://.".into()),
            other => return Err(format!("Unknown proxy type \"{}\": use socks5:// or http://.", other)),
        };
        let rest = rest.trim_end_matches('/');
        let (auth, hostport) = match rest.rsplit_once('@') {
            Some((a, h)) => (Some(a), h),
            None => (None, rest),
        };
        let (user, pass) = match auth {
            Some(a) => {
                let (u, p) = a.split_once(':').unwrap_or((a, ""));
                (url_decode(u), url_decode(p))
            }
            None => (String::new(), String::new()),
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(':') || h.starts_with('[') => (h.trim_matches(|c| c == '[' || c == ']').to_string(), p.parse::<u16>().map_err(|_| format!("\"{}\" is not a port number.", p))?),
            _ => (hostport.to_string(), if http { 8080 } else { 1080 }),
        };
        if host.is_empty() {
            return Err("The proxy address has no host.".into());
        }
        Ok(Proxy { http, host, port, user, pass })
    }

    fn addr(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// For the window: the address without the password.
    fn shown(&self) -> String {
        let auth = if self.user.is_empty() { String::new() } else { format!("{}:•••@", self.user) };
        format!("{}://{}{}", if self.http { "http" } else { "socks5" }, auth, self.addr())
    }
}

async fn socks5_handshake<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(s: &mut S, p: &Proxy, host: &str, port: u16) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let auth = !p.user.is_empty();
    s.write_all(if auth { &[5, 2, 0, 2] } else { &[5, 1, 0] }).await?;
    let mut r = [0u8; 2];
    s.read_exact(&mut r).await?;
    if r[0] != 5 {
        return Err(Error::new(ErrorKind::InvalidData, "the proxy is not a SOCKS5 proxy"));
    }
    match r[1] {
        0 => {}
        2 => {
            let (u, pw) = (p.user.as_bytes(), p.pass.as_bytes());
            let mut m = vec![1, u.len().min(255) as u8];
            m.extend_from_slice(&u[..u.len().min(255)]);
            m.push(pw.len().min(255) as u8);
            m.extend_from_slice(&pw[..pw.len().min(255)]);
            s.write_all(&m).await?;
            s.read_exact(&mut r).await?;
            if r[1] != 0 {
                return Err(Error::new(ErrorKind::PermissionDenied, "the proxy refused the user name or password"));
            }
        }
        _ => return Err(Error::new(ErrorKind::PermissionDenied, "the proxy wants a user name and password")),
    }
    let mut m = vec![5, 1, 0];
    let bare = host.trim_matches(|c| c == '[' || c == ']');
    match bare.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            m.push(1);
            m.extend_from_slice(&ip.octets());
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            m.push(4);
            m.extend_from_slice(&ip.octets());
        }
        // the name is resolved by the proxy (no DNS request from here)
        Err(_) => {
            m.push(3);
            m.push(bare.len().min(255) as u8);
            m.extend_from_slice(&bare.as_bytes()[..bare.len().min(255)]);
        }
    }
    m.extend_from_slice(&port.to_be_bytes());
    s.write_all(&m).await?;
    let mut head = [0u8; 4];
    s.read_exact(&mut head).await?;
    if head[1] != 0 {
        return Err(Error::new(ErrorKind::ConnectionRefused, format!("the proxy could not reach {}:{} (code {})", bare, port, head[1])));
    }
    let skip = match head[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await?;
            l[0] as usize
        }
        _ => 0,
    } + 2;
    let mut rest = vec![0u8; skip];
    s.read_exact(&mut rest).await?;
    Ok(())
}

async fn http_connect<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(s: &mut S, p: &Proxy, host: &str, port: u16) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    use tokio::io::AsyncWriteExt;
    let target = if host.contains(':') && !host.starts_with('[') { format!("[{}]:{}", host, port) } else { format!("{}:{}", host, port) };
    let mut req = format!("CONNECT {t} HTTP/1.1\r\nHost: {t}\r\n", t = target);
    if !p.user.is_empty() {
        req.push_str(&format!("Proxy-Authorization: Basic {}\r\n", b64(format!("{}:{}", p.user, p.pass).as_bytes())));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).await?;
    let (head, _) = read_head(s).await?;
    let first = head.lines().next().unwrap_or("");
    if first.split_whitespace().nth(1) != Some("200") {
        return Err(Error::new(ErrorKind::ConnectionRefused, format!("the proxy answered \"{}\"", first)));
    }
    Ok(())
}

/// Reads an HTTP head (up to the empty line); returns it and the bytes read after it.
async fn read_head<S: tokio::io::AsyncRead + Unpin>(s: &mut S) -> std::io::Result<(String, Vec<u8>)> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    loop {
        let n = s.read(&mut chunk).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "the connection closed"));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf[i + 4..].to_vec();
            buf.truncate(i + 4);
            return Ok((String::from_utf8_lossy(&buf).into_owned(), rest));
        }
        if buf.len() > 64 * 1024 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the request head is too long"));
        }
    }
}

// ---- WireGuard ----

/// A wg-quick file ([Interface] + [Peer]).
#[derive(Clone, Debug, Default)]
struct WgFile {
    private_key: [u8; 32],
    address: String,
    dns: Vec<std::net::IpAddr>,
    mtu: Option<usize>,
    peer_key: [u8; 32],
    preshared: Option<[u8; 32]>,
    endpoint: String,
    allowed: Vec<String>,
    keepalive: Option<u16>,
}

impl WgFile {
    fn parse(text: &str) -> Result<WgFile, String> {
        if text.trim().is_empty() {
            return Err("Choose a WireGuard file (.conf) first.".into());
        }
        let mut w = WgFile::default();
        let (mut section, mut peers, mut have_key, mut have_peer_key) = (String::new(), 0, false, false);
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                section = line.trim_matches(|c| c == '[' || c == ']').trim().to_lowercase();
                if section == "peer" {
                    peers += 1;
                }
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim().to_lowercase(), v.trim());
            // only the first peer is used
            if section == "peer" && peers > 1 {
                continue;
            }
            let key = |v: &str| -> Result<[u8; 32], String> { b64_decode(v).and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()).ok_or_else(|| format!("\"{}\" is not a WireGuard key.", v)) };
            match (section.as_str(), k.as_str()) {
                ("interface", "privatekey") => {
                    w.private_key = key(v)?;
                    have_key = true;
                }
                ("interface", "address") => w.address = v.to_string(),
                ("interface", "dns") => w.dns = v.split(',').filter_map(|d| d.trim().parse().ok()).collect(),
                ("interface", "mtu") => w.mtu = v.parse().ok(),
                ("peer", "publickey") => {
                    w.peer_key = key(v)?;
                    have_peer_key = true;
                }
                ("peer", "presharedkey") => w.preshared = Some(key(v)?),
                ("peer", "endpoint") => w.endpoint = v.to_string(),
                ("peer", "allowedips") => w.allowed = v.split(',').map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect(),
                ("peer", "persistentkeepalive") => w.keepalive = v.parse().ok(),
                _ => {}
            }
        }
        if !have_key {
            return Err("The WireGuard file has no PrivateKey in [Interface].".into());
        }
        if !have_peer_key || w.endpoint.is_empty() {
            return Err("The WireGuard file has no [Peer] with PublicKey and Endpoint.".into());
        }
        if w.address.is_empty() {
            return Err("The WireGuard file has no Address in [Interface].".into());
        }
        if w.allowed.is_empty() {
            w.allowed = vec!["0.0.0.0/0".into(), "::/0".into()];
        }
        Ok(w)
    }

    fn summary(&self) -> String {
        format!("{} · {}", self.endpoint, self.address)
    }

    fn address_with_prefix(&self) -> String {
        self.address
            .split(',')
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .map(|a| if a.contains('/') { a.to_string() } else if a.contains(':') { format!("{}/128", a) } else { format!("{}/32", a) })
            .collect::<Vec<_>>()
            .join(",")
    }

    async fn into_config(self) -> Result<tokio_wireguard::Config, String> {
        use tokio_wireguard::x25519::{PublicKey, StaticSecret};
        let address = self.address_with_prefix().parse::<tokio_wireguard::config::Address>().map_err(|e| format!("The WireGuard Address \"{}\" is not valid ({:?}).", self.address, e))?;
        // the VPN server's own address is looked up here (like every WireGuard app does); the rest goes through it
        let ep = self.endpoint.clone();
        let mut addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(ep.clone()).await.map_err(|e| format!("Could not find the VPN server {} ({}).", ep, e))?.collect();
        addrs.sort_by_key(|a| a.is_ipv6());
        let endpoint = *addrs.first().ok_or_else(|| format!("Could not find the VPN server {}.", ep))?;
        let allowed_ips = self.allowed.iter().filter_map(|a| a.parse().ok()).collect::<Vec<ipnet::IpNet>>();
        Ok(tokio_wireguard::Config {
            interface: tokio_wireguard::config::Interface { private_key: StaticSecret::from(self.private_key), address, listen_port: None, mtu: self.mtu },
            peers: vec![tokio_wireguard::config::Peer {
                endpoint: Some(endpoint),
                allowed_ips,
                public_key: PublicKey::from(self.peer_key),
                persistent_keepalive: Some(self.keepalive.unwrap_or(25)),
                preshared_key: self.preshared,
            }],
        })
    }
}

/// A name looked up through the tunnel: a DNS question over UDP to the file's DNS server (else 1.1.1.1).
async fn wg_resolve(iface: &tokio_wireguard::Interface, dns: &[std::net::IpAddr], host: &str) -> std::io::Result<std::net::IpAddr> {
    use std::io::{Error, ErrorKind};
    let v6_only = !iface.address().is_v4();
    let qtype: u16 = if v6_only { 28 } else { 1 };
    let mut servers: Vec<std::net::IpAddr> = dns.iter().copied().filter(|d| d.is_ipv6() == v6_only).collect();
    if servers.is_empty() {
        servers.push(if v6_only { "2606:4700:4700::1111".parse().unwrap() } else { "1.1.1.1".parse().unwrap() });
    }
    let bind = if v6_only { "[::]:0" } else { "0.0.0.0:0" };
    let sock = tokio_wireguard::UdpSocket::bind(bind, iface).await?;
    let id: u16 = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7) & 0xffff) as u16;
    let q = dns_query(id, host, qtype);
    for server in servers {
        for _ in 0..2 {
            sock.send_to(&q, std::net::SocketAddr::new(server, 53)).await?;
            let mut buf = vec![0u8; 1500];
            match tokio::time::timeout(Duration::from_secs(4), sock.recv_from(&mut buf[..])).await {
                Ok(Ok((n, _))) => {
                    if let Some(ip) = dns_answer(&buf[..n], id, qtype) {
                        return Ok(ip);
                    }
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => continue,
            }
        }
    }
    Err(Error::new(ErrorKind::NotFound, format!("could not look up {} through the tunnel", host)))
}

fn dns_query(id: u16, host: &str, qtype: u16) -> Vec<u8> {
    let mut q = Vec::with_capacity(32 + host.len());
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in host.trim_end_matches('.').split('.') {
        let l = &label.as_bytes()[..label.len().min(63)];
        q.push(l.len() as u8);
        q.extend_from_slice(l);
    }
    q.push(0);
    q.extend_from_slice(&qtype.to_be_bytes());
    q.extend_from_slice(&[0, 1]);
    q
}

/// The first address of the asked type in a DNS answer.
fn dns_answer(r: &[u8], id: u16, qtype: u16) -> Option<std::net::IpAddr> {
    if r.len() < 12 || u16::from_be_bytes([r[0], r[1]]) != id || r[3] & 0x0f != 0 {
        return None;
    }
    let qd = u16::from_be_bytes([r[4], r[5]]) as usize;
    let an = u16::from_be_bytes([r[6], r[7]]) as usize;
    let mut i = 12;
    let skip_name = |r: &[u8], mut i: usize| -> Option<usize> {
        loop {
            let l = *r.get(i)? as usize;
            if l == 0 {
                return Some(i + 1);
            }
            if l & 0xc0 == 0xc0 {
                return Some(i + 2);
            }
            i += 1 + l;
        }
    };
    for _ in 0..qd {
        i = skip_name(r, i)? + 4;
    }
    for _ in 0..an {
        i = skip_name(r, i)?;
        let t = u16::from_be_bytes([*r.get(i)?, *r.get(i + 1)?]);
        let len = u16::from_be_bytes([*r.get(i + 8)?, *r.get(i + 9)?]) as usize;
        let data = r.get(i + 10..i + 10 + len)?;
        i += 10 + len;
        if t == qtype && qtype == 1 && len == 4 {
            return Some(std::net::IpAddr::from(<[u8; 4]>::try_from(data).ok()?));
        }
        if t == qtype && qtype == 28 && len == 16 {
            return Some(std::net::IpAddr::from(<[u8; 16]>::try_from(data).ok()?));
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------------------------
// Cloudflare WARP (free, no account): only when the user presses its button
// ---------------------------------------------------------------------------------------------------------------

/// Registers a new WARP device with Cloudflare (the same request the 1.1.1.1 app makes) and returns a wg-quick file
/// for it. A new key pair is made here; only the public key is sent. Nothing about the user goes with it.
pub fn warp_register(http: &dyn crate::http::Http) -> Result<String, String> {
    let (secret, public) = tokio_wireguard::x25519::keypair();
    let tos = {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let (y, mo, d, h, mi, se) = civil(secs);
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z", y, mo, d, h, mi, se)
    };
    let body = Json::obj(vec![
        ("key", b64(public.as_bytes()).into()),
        ("install_id", "".into()),
        ("fcm_token", "".into()),
        ("tos", tos.into()),
        ("type", "Android".into()),
        ("model", "PC".into()),
        ("locale", "en_US".into()),
    ]);
    let req = crate::http::Request::post("https://api.cloudflareclient.com/v0a2158/reg")
        .header("CF-Client-Version", "a-6.11-2223")
        .header("User-Agent", "okhttp/3.12.1")
        .json_body(body.dump())
        .timeout(30);
    let r = http.send(&req)?;
    if r.status >= 400 {
        return Err(format!("Cloudflare answered {} to the WARP registration.", r.status));
    }
    let j = Json::parse(&String::from_utf8_lossy(&r.body)).map_err(|_| "Cloudflare's answer could not be read.".to_string())?;
    warp_conf(&j, &b64(&secret.to_bytes())).ok_or_else(|| "Cloudflare's answer has no WARP tunnel in it.".to_string())
}

/// The wg-quick file from Cloudflare's registration answer (with or without the "result" wrapper).
fn warp_conf(j: &Json, private_b64: &str) -> Option<String> {
    let root = if j.get("config").is_null() { j.get("result") } else { j };
    let cfg = root.get("config");
    let peer = cfg.get("peers").at(0);
    let pubkey = peer.get("public_key").str();
    let mut endpoint = peer.get("endpoint").get("host").str();
    if endpoint.is_empty() {
        endpoint = "engage.cloudflareclient.com:2408".into();
    }
    let v4 = cfg.get("interface").get("addresses").get("v4").str();
    let v6 = cfg.get("interface").get("addresses").get("v6").str();
    if pubkey.is_empty() || v4.is_empty() {
        return None;
    }
    let addr = if v6.is_empty() { format!("{}/32", v4) } else { format!("{}/32, {}/128", v4, v6) };
    Some(format!(
        "# Cloudflare WARP (free), made by Reelhouse\n[Interface]\nPrivateKey = {}\nAddress = {}\nDNS = 1.1.1.1, 2606:4700:4700::1111\nMTU = 1280\n\n[Peer]\nPublicKey = {}\nAllowedIPs = 0.0.0.0/0, ::/0\nEndpoint = {}\nPersistentKeepalive = 25\n",
        private_b64, addr, pubkey, endpoint
    ))
}

/// Unix seconds to (year, month, day, hour, minute, second), UTC.
fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d, (rem / 3600) as u32, (rem % 3600 / 60) as u32, (rem % 60) as u32)
}

// ---------------------------------------------------------------------------------------------------------------
// The gateway (127.0.0.1): SOCKS5 and HTTP proxy in one port
// ---------------------------------------------------------------------------------------------------------------

async fn serve(listener: tokio::net::TcpListener) {
    loop {
        match listener.accept().await {
            Ok((s, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                tokio::spawn(async move {
                    if let Err(e) = handle(s).await {
                        if crate::debug::enabled() {
                            crate::debug::log("privacy", &format!("gateway: {}", e));
                        }
                    }
                });
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
}

/// The tunnel to use now; waits while it is still connecting (Tor can take a minute), none = refuse (kill switch).
async fn current_upstream() -> Option<Arc<Upstream>> {
    for _ in 0..240 {
        if !active() {
            return None;
        }
        let (phase, up) = {
            let s = lock();
            (s.phase, s.upstream.clone())
        };
        match (phase, up) {
            (Phase::Ready, Some(u)) | (Phase::Down, Some(u)) => return Some(u),
            (Phase::Connecting, _) | (Phase::Ready, None) => tokio::time::sleep(Duration::from_millis(250)).await,
            _ => return None,
        }
    }
    None
}

async fn handle(mut client: tokio::net::TcpStream) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let _ = client.set_nodelay(true);
    let mut first = [0u8; 1];
    let n = client.peek(&mut first).await?;
    if n == 0 {
        return Ok(());
    }
    if first[0] == 5 {
        // ---- SOCKS5 (no authentication: only programs on this device reach it) ----
        let mut h = [0u8; 2];
        client.read_exact(&mut h).await?;
        let mut methods = vec![0u8; h[1] as usize];
        client.read_exact(&mut methods).await?;
        client.write_all(&[5, 0]).await?;
        let mut req = [0u8; 4];
        client.read_exact(&mut req).await?;
        let host = match req[3] {
            1 => {
                let mut a = [0u8; 4];
                client.read_exact(&mut a).await?;
                std::net::Ipv4Addr::from(a).to_string()
            }
            4 => {
                let mut a = [0u8; 16];
                client.read_exact(&mut a).await?;
                std::net::Ipv6Addr::from(a).to_string()
            }
            3 => {
                let mut l = [0u8; 1];
                client.read_exact(&mut l).await?;
                let mut name = vec![0u8; l[0] as usize];
                client.read_exact(&mut name).await?;
                String::from_utf8_lossy(&name).into_owned()
            }
            _ => return Err(Error::new(ErrorKind::InvalidData, "bad SOCKS address")),
        };
        let mut p = [0u8; 2];
        client.read_exact(&mut p).await?;
        let port = u16::from_be_bytes(p);
        if req[1] != 1 {
            // only CONNECT (no UDP, no BIND)
            client.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            return Ok(());
        }
        let Some(up) = current_upstream().await else {
            client.write_all(&[5, 2, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            return Ok(());
        };
        let mut remote = match up.dial(&host, port).await {
            Ok(r) => r,
            Err(e) => {
                client.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
                return Err(e);
            }
        };
        client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        let _ = tokio::io::copy_bidirectional(&mut client, &mut remote).await;
        return Ok(());
    }
    // ---- HTTP proxy ----
    let (head, rest) = read_head(&mut client).await?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("").to_string();
    let mut parts = request_line.split_whitespace();
    let (method, target, version) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or("HTTP/1.1"));
    let Some(up) = current_upstream().await else {
        client.write_all(b"HTTP/1.1 503 IP protection is not connected\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").await?;
        return Ok(());
    };
    if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = split_host_port(target, 443);
        let mut remote = match up.dial(&host, port).await {
            Ok(r) => r,
            Err(e) => {
                client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").await?;
                return Err(e);
            }
        };
        client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await?;
        if !rest.is_empty() {
            remote.write_all(&rest).await?;
        }
        let _ = tokio::io::copy_bidirectional(&mut client, &mut remote).await;
        return Ok(());
    }
    // a plain request "GET http://host[:port]/path HTTP/1.1": sent on as "GET /path" (one request per connection)
    let Some(after) = target.strip_prefix("http://") else {
        client.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").await?;
        return Ok(());
    };
    let (hostport, path) = match after.find('/') {
        Some(i) => (&after[..i], &after[i..]),
        None => (after, "/"),
    };
    let (host, port) = split_host_port(hostport, 80);
    let mut out = format!("{} {} {}\r\n", method, path, version);
    for l in lines {
        let lower = l.to_lowercase();
        if l.is_empty() || lower.starts_with("proxy-") || lower.starts_with("connection:") || lower.starts_with("keep-alive:") {
            continue;
        }
        out.push_str(l);
        out.push_str("\r\n");
    }
    out.push_str("Connection: close\r\n\r\n");
    let mut remote = match up.dial(&host, port).await {
        Ok(r) => r,
        Err(e) => {
            client.write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").await?;
            return Err(e);
        }
    };
    remote.write_all(out.as_bytes()).await?;
    if !rest.is_empty() {
        remote.write_all(&rest).await?;
    }
    let _ = tokio::io::copy_bidirectional(&mut client, &mut remote).await;
    Ok(())
}

fn split_host_port(s: &str, default: u16) -> (String, u16) {
    if let Some(rest) = s.strip_prefix('[') {
        if let Some((h, p)) = rest.split_once(']') {
            return (h.to_string(), p.trim_start_matches(':').parse().unwrap_or(default));
        }
    }
    match s.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => (h.to_string(), p.parse().unwrap_or(default)),
        _ => (s.to_string(), default),
    }
}

/// A blocking TCP connection to `host:port` that goes through the gateway when protection is on (HTTP CONNECT),
/// else straight: for code with its own sockets (Gemini Live's WebSocket).
pub fn connect_tcp(host: &str, port: u16, timeout: Duration) -> std::io::Result<std::net::TcpStream> {
    use std::io::{Error, ErrorKind, Read, Write};
    if active() {
        gate().map_err(|e| Error::new(ErrorKind::PermissionDenied, e))?;
    }
    // off, or "system" (the system VPN carries it): straight
    let Some(p) = http_proxy() else {
        return connect_direct(host, port, timeout);
    };
    let port_gw: u16 = p.rsplit(':').next().and_then(|x| x.parse().ok()).unwrap_or(0);
    let mut s = std::net::TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], port_gw)), timeout)?;
    s.set_read_timeout(Some(Duration::from_secs(90)))?;
    let target = if host.contains(':') { format!("[{}]:{}", host, port) } else { format!("{}:{}", host, port) };
    write!(s, "CONNECT {t} HTTP/1.1\r\nHost: {t}\r\n\r\n", t = target)?;
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if s.read(&mut b)? == 0 || head.len() > 8192 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "the IP protection gateway closed the connection"));
        }
        head.push(b[0]);
    }
    let text = String::from_utf8_lossy(&head);
    if text.split_whitespace().nth(1) != Some("200") {
        return Err(Error::new(ErrorKind::ConnectionRefused, format!("IP protection: {}", text.lines().next().unwrap_or(""))));
    }
    s.set_read_timeout(None)?;
    Ok(s)
}

fn connect_direct(host: &str, port: u16, timeout: Duration) -> std::io::Result<std::net::TcpStream> {
    use std::net::ToSocketAddrs;
    let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, format!("could not find {}", host));
    for a in (host, port).to_socket_addrs()? {
        match std::net::TcpStream::connect_timeout(&a, timeout) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}

// ---------------------------------------------------------------------------------------------------------------
// "system": a VPN app of the system
// ---------------------------------------------------------------------------------------------------------------

/// A tunnel device is up (tun*, wg*, ppp*, utun*, ipsec*, and the names VPN apps give theirs).
pub fn system_tunnel_up() -> bool {
    interface_names().iter().any(|n| is_tunnel_name(n))
}

fn is_tunnel_name(n: &str) -> bool {
    let n = n.to_lowercase();
    ["tun", "wg", "ppp", "utun", "ipsec", "tap", "proton", "nordlynx", "mullvad", "cloudflarewarp", "warp", "wireguard", "openvpn", "vpn", "tailscale", "zt"].iter().any(|p| n.starts_with(p))
}

/// The names of the network devices that are up and running.
#[cfg(unix)]
fn interface_names() -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut ifa: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifa) != 0 {
            return out;
        }
        let mut p = ifa;
        while !p.is_null() {
            let i = &*p;
            let flags = i.ifa_flags as libc::c_int;
            if flags & libc::IFF_UP != 0 && flags & libc::IFF_RUNNING != 0 && !i.ifa_name.is_null() {
                let name = std::ffi::CStr::from_ptr(i.ifa_name).to_string_lossy().into_owned();
                if !out.contains(&name) {
                    out.push(name);
                }
            }
            p = i.ifa_next;
        }
        libc::freeifaddrs(ifa);
    }
    out
}

/// Windows: the adapters that are up, from the system's own list (`netsh`-free: PowerShell is slow, so ipconfig).
#[cfg(not(unix))]
fn interface_names() -> Vec<String> {
    let mut cmd = std::process::Command::new("ipconfig");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let Ok(out) = cmd.output() else { return vec![] };
    let text = String::from_utf8_lossy(&out.stdout);
    // "Unknown adapter ProtonVPN:" / "PPP adapter X:" followed by an IPv4 line = up
    let mut names = Vec::new();
    let mut current = String::new();
    for l in text.lines() {
        if !l.starts_with(' ') && l.trim_end().ends_with(':') {
            current = l.trim_end().trim_end_matches(':').rsplit(" adapter ").next().unwrap_or("").to_string();
            if l.starts_with("PPP") {
                current = format!("ppp {}", current);
            }
        } else if l.contains("IPv4") && !current.is_empty() {
            names.push(current.replace(' ', ""));
        }
    }
    names
}

fn watch_system(epoch: u64) {
    loop {
        if EPOCH.load(Ordering::Relaxed) != epoch {
            return;
        }
        let up = system_tunnel_up();
        let was = ready();
        if up && !was {
            set_phase(epoch, Phase::Ready, "");
        } else if !up && (was || lock().phase == Phase::Connecting) {
            set_phase(epoch, if was { Phase::Down } else { Phase::Error }, "no VPN tunnel is on in the system");
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------------------------------------------

/// "%40" -> "@", "+" stays (user names and passwords in a proxy address, trackers in a magnet).
pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
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

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn b64_decode(t: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for ch in t.trim().bytes() {
        if ch == b'=' {
            break;
        }
        let v = B64.iter().position(|&c| c == ch).or(match ch {
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        })? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONF: &str = "[Interface]\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = 10.2.0.2/32, fd00::2/128\nDNS = 10.2.0.1\n\n[Peer]\n# server\nPublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\nAllowedIPs = 0.0.0.0/0, ::/0\nEndpoint = 127.0.0.1:51820\nPersistentKeepalive = 15\n";

    #[test]
    fn reads_a_wireguard_file() {
        let w = WgFile::parse(CONF).unwrap();
        assert_eq!(w.address_with_prefix(), "10.2.0.2/32,fd00::2/128");
        assert_eq!(w.dns, vec!["10.2.0.1".parse::<std::net::IpAddr>().unwrap()]);
        assert_eq!(w.allowed.len(), 2);
        assert_eq!(w.keepalive, Some(15));
        assert_eq!(b64(&w.private_key), "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=");
        assert!(WgFile::parse("[Interface]\nAddress = 1.2.3.4").is_err());
        let bare = WgFile::parse(&CONF.replace("10.2.0.2/32, fd00::2/128", "10.2.0.2")).unwrap();
        assert_eq!(bare.address_with_prefix(), "10.2.0.2/32");
    }

    #[test]
    fn reads_proxy_addresses() {
        let p = Proxy::parse("socks5://me:p%40ss@example.org:1080").unwrap();
        assert_eq!((p.http, p.host.as_str(), p.port, p.user.as_str(), p.pass.as_str()), (false, "example.org", 1080, "me", "p@ss"));
        assert_eq!(p.shown(), "socks5://me:•••@example.org:1080");
        let h = Proxy::parse("http://10.0.0.1:3128").unwrap();
        assert!(h.http && h.port == 3128);
        assert_eq!(Proxy::parse("example.org").unwrap().port, 1080);
        assert_eq!(Proxy::parse("socks5://[::1]:9050").unwrap().host, "::1");
        assert!(Proxy::parse("https://x:1").is_err());
        assert!(Proxy::parse("").is_err());
    }

    #[test]
    fn base64_round_trip() {
        for s in ["", "a", "ab", "abc", "abcd", "hello world!"] {
            assert_eq!(b64_decode(&b64(s.as_bytes())).unwrap(), s.as_bytes());
        }
    }

    #[test]
    fn dns_answer_is_read() {
        let q = dns_query(0x1234, "example.org", 1);
        // an answer: the question, then one A record with a name pointer
        let mut r = q.clone();
        r[2] = 0x81;
        r[3] = 0x80;
        r[7] = 1;
        r.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 93, 184, 216, 34]);
        assert_eq!(dns_answer(&r, 0x1234, 1), Some("93.184.216.34".parse().unwrap()));
        assert_eq!(dns_answer(&r, 0x9999, 1), None);
    }

    #[test]
    fn udp_trackers_become_http_on_a_tunnel() {
        let m = rewrite_magnet("magnet:?xt=urn:btih:abc&dn=x&tr=udp%3A%2F%2Ftracker.example.org%3A1337%2Fannounce&tr=https%3A%2F%2Ft.example%2Fannounce");
        assert!(m.starts_with("magnet:?xt=urn:btih:abc&dn=x&"), "{}", m);
        assert!(!m.contains("udp"), "{}", m);
        assert_eq!(url_decode(m.split("&tr=").nth(1).unwrap()), "http://tracker.example.org:1337/announce");
        assert!(m.contains("tr=https%3A%2F%2Ft.example%2Fannounce"), "{}", m);
    }

    #[test]
    fn warp_answer_becomes_a_wireguard_file() {
        let j = Json::parse(r#"{"result":{"config":{"peers":[{"public_key":"bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo=","endpoint":{"host":"engage.cloudflareclient.com:2408"}}],"interface":{"addresses":{"v4":"172.16.0.2","v6":"2606:4700:110:8a36::2"}}}}}"#).unwrap();
        let conf = warp_conf(&j, "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=").unwrap();
        let w = WgFile::parse(&conf).unwrap();
        assert_eq!(w.endpoint, "engage.cloudflareclient.com:2408");
        assert_eq!(w.address_with_prefix(), "172.16.0.2/32,2606:4700:110:8a36::2/128");
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_793_664_000), (2026, 11, 3, 0, 0, 0));
    }

    #[test]
    fn tunnel_names() {
        for n in ["tun0", "wg0", "ProtonVPN", "ppp0", "utun3", "CloudflareWARP", "nordlynx"] {
            assert!(is_tunnel_name(n), "{}", n);
        }
        for n in ["eth0", "wlan0", "lo", "enp3s0", "rmnet_data0"] {
            assert!(!is_tunnel_name(n), "{}", n);
        }
    }

    /// The gateway end to end: HTTP CONNECT and SOCKS5 into it, out through a fake SOCKS5 "VPN proxy", to a local
    /// echo server; then the kill switch.
    #[test]
    fn gateway_forwards_through_a_socks_upstream() {
        use std::io::{Read, Write};
        // echo server
        let echo = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in echo.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut s = s;
                    let mut b = [0u8; 64];
                    while let Ok(n) = s.read(&mut b) {
                        if n == 0 || s.write_all(&b[..n]).is_err() {
                            break;
                        }
                    }
                });
            }
        });
        // a tiny SOCKS5 server (the "VPN provider's proxy") that connects to whatever is asked
        let up = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let up_port = up.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in up.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut c = s;
                    let mut b = [0u8; 262];
                    c.read_exact(&mut b[..2]).unwrap();
                    let n = b[1] as usize;
                    c.read_exact(&mut b[..n]).unwrap();
                    c.write_all(&[5, 0]).unwrap();
                    c.read_exact(&mut b[..4]).unwrap();
                    let host = match b[3] {
                        1 => {
                            let mut a = [0u8; 4];
                            c.read_exact(&mut a).unwrap();
                            std::net::Ipv4Addr::from(a).to_string()
                        }
                        3 => {
                            let mut l = [0u8; 1];
                            c.read_exact(&mut l).unwrap();
                            let mut nm = vec![0u8; l[0] as usize];
                            c.read_exact(&mut nm).unwrap();
                            String::from_utf8(nm).unwrap()
                        }
                        _ => panic!(),
                    };
                    let mut p = [0u8; 2];
                    c.read_exact(&mut p).unwrap();
                    let mut r = std::net::TcpStream::connect((host.as_str(), u16::from_be_bytes(p))).unwrap();
                    c.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();
                    let mut c2 = c.try_clone().unwrap();
                    let mut r2 = r.try_clone().unwrap();
                    std::thread::spawn(move || {
                        let _ = std::io::copy(&mut r2, &mut c2);
                    });
                    let _ = std::io::copy(&mut c, &mut r);
                });
            }
        });
        // the app's choice: that proxy (the health probe to the internet fails here, so set the state by hand)
        std::env::set_var("REELHOUSE_DATA", std::env::temp_dir().join(format!("rh-privacy-{}", std::process::id())));
        apply(Cfg { mode: "proxy".into(), proxy_url: format!("socks5://127.0.0.1:{}", up_port), ..Cfg::default() }).unwrap();
        for _ in 0..100 {
            if lock().upstream.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // stop the probe's verdict from mattering
        EPOCH.fetch_add(1, Ordering::Relaxed);
        {
            let mut s = lock();
            s.phase = Phase::Ready;
        }
        READY.store(true, Ordering::Relaxed);
        // through the gateway with HTTP CONNECT
        let mut s = connect_tcp("127.0.0.1", echo_port, Duration::from_secs(5)).unwrap();
        s.write_all(b"ping").unwrap();
        let mut b = [0u8; 4];
        s.read_exact(&mut b).unwrap();
        assert_eq!(&b, b"ping");
        // and with SOCKS5 to the gateway, by name
        let gw = PORT.load(Ordering::Relaxed);
        let mut s = std::net::TcpStream::connect(("127.0.0.1", gw)).unwrap();
        s.write_all(&[5, 1, 0]).unwrap();
        s.read_exact(&mut b[..2]).unwrap();
        let name = b"localhost";
        let mut m = vec![5, 1, 0, 3, name.len() as u8];
        m.extend_from_slice(name);
        m.extend_from_slice(&echo_port.to_be_bytes());
        s.write_all(&m).unwrap();
        let mut rep = [0u8; 10];
        s.read_exact(&mut rep).unwrap();
        assert_eq!(rep[1], 0);
        s.write_all(b"pong").unwrap();
        s.read_exact(&mut b).unwrap();
        assert_eq!(&b, b"pong");
        // the window's state while protection is on (this used to take the state lock twice and hang the window)
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(state_json().get("proxy_port").as_i64().unwrap_or(-1));
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).expect("state_json must not hang"), gw as i64);
        // kill switch: protection on, tunnel not ready -> refused
        READY.store(false, Ordering::Relaxed);
        lock().phase = Phase::Error;
        assert!(gate().is_err());
        assert!(connect_tcp("127.0.0.1", echo_port, Duration::from_secs(5)).is_err());
        apply(Cfg::default()).unwrap();
        assert!(gate().is_ok());
    }
}
