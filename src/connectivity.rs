//! Is there internet? A small watcher that the whole app reads, so that, with no connection, nothing waits for a
//! timeout before falling back to what is already on disk (downloaded chapters, novels, videos, cached lists).
//!
//! - A thread knocks on a few well-known addresses (TCP connect, no data sent) every 20 s while online and every
//!   5 s while offline. Any answer = online.
//! - `net::UreqHttp` refuses requests to other computers at once while offline ("no internet connection"), and a
//!   request that fails to connect asks for a check right away.
//! - The user can also choose "Offline mode" by hand (`set_forced`): then the app behaves as offline even with a
//!   connection (saves data, tests what was downloaded).
//!
//! Requests to this computer and the home network (Suwayomi, Ollama, Whisper on 127.0.0.1) are never blocked.
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// What the last check found. Starts true: until the first check, nothing is refused.
static NET: AtomicBool = AtomicBool::new(true);
/// The user's "Offline mode".
static FORCED: AtomicBool = AtomicBool::new(false);
/// Raised by a failed request: the watcher checks at once instead of waiting for its next turn.
static RECHECK: AtomicBool = AtomicBool::new(false);
/// Grows each time the state seen by the app changes (the bridge sends an event when it moves).
static GEN: AtomicU64 = AtomicU64::new(0);
static STARTED: AtomicBool = AtomicBool::new(false);

/// True when the app may use the internet.
pub fn online() -> bool {
    !FORCED.load(Ordering::Relaxed) && NET.load(Ordering::Relaxed)
}

/// True when the user turned on "Offline mode" by hand.
pub fn forced() -> bool {
    FORCED.load(Ordering::Relaxed)
}

/// True when the last check found a connection (whatever the user's choice).
pub fn network() -> bool {
    NET.load(Ordering::Relaxed)
}

/// Counter of changes of `online()`: compare with a saved value to know whether it moved.
pub fn generation() -> u64 {
    GEN.load(Ordering::Relaxed)
}

pub fn set_forced(on: bool) {
    if FORCED.swap(on, Ordering::Relaxed) != on {
        GEN.fetch_add(1, Ordering::Relaxed);
        crate::debug::log("net", if on { "offline mode turned on by the user" } else { "offline mode turned off by the user" });
        if !on {
            RECHECK.store(true, Ordering::Relaxed);
        }
    }
}

fn set_network(on: bool) {
    if NET.swap(on, Ordering::Relaxed) != on {
        GEN.fetch_add(1, Ordering::Relaxed);
        crate::debug::log("net", if on { "connection is back" } else { "no internet connection: using what is on disk" });
    }
}

/// A request to another computer could not connect: check again now (it may be the network that went away).
pub fn note_failure() {
    RECHECK.store(true, Ordering::Relaxed);
}

/// True for addresses on this computer or the home network (never blocked: the local servers of the app live there).
pub fn is_local(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let netloc = rest.split('/').next().unwrap_or("");
    let netloc = netloc.rsplit('@').next().unwrap_or(netloc);
    if netloc.starts_with("[::1]") {
        return true;
    }
    let host = netloc.split(':').next().unwrap_or("");
    // the home network too (a Suwayomi server on another PC of the house still answers without internet)
    host == "localhost" || host == "0.0.0.0" || host.ends_with(".local") || host.parse::<std::net::Ipv4Addr>().map(|ip| ip.is_loopback() || ip.is_private() || ip.is_link_local()).unwrap_or(false)
}

/// The error given instead of a request while offline. Callers that keep a copy on disk show the copy.
pub fn offline_error(url: &str) -> String {
    let host = url.split('/').nth(2).unwrap_or(url);
    format!("no internet connection (offline mode), {} was not asked", host)
}

/// True when `e` is the error of a request refused because the app is offline.
pub fn is_offline_error(e: &str) -> bool {
    e.starts_with("no internet connection")
}

/// Knocks on a few addresses: fixed IPs first (no DNS needed), then names (a network that only lets names through).
fn probe() -> bool {
    let fixed = ["1.1.1.1:443", "8.8.8.8:443", "9.9.9.9:443", "[2606:4700:4700::1111]:443"];
    for a in fixed {
        if let Ok(addr) = a.parse::<SocketAddr>() {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(2500)).is_ok() {
                return true;
            }
        }
    }
    for name in ["www.google.com:443", "github.com:443"] {
        if let Ok(addrs) = name.to_socket_addrs() {
            for addr in addrs.take(2) {
                if TcpStream::connect_timeout(&addr, Duration::from_millis(2500)).is_ok() {
                    return true;
                }
            }
        }
    }
    false
}

/// Starts the watcher once (later calls do nothing). The first check runs at once.
pub fn start() {
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let _ = std::thread::Builder::new().name("connectivity".into()).spawn(|| loop {
        RECHECK.store(false, Ordering::Relaxed);
        set_network(probe());
        let wait = if NET.load(Ordering::Relaxed) { 20 } else { 5 };
        for _ in 0..wait * 4 {
            std::thread::sleep(Duration::from_millis(250));
            if RECHECK.load(Ordering::Relaxed) {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_addresses_are_never_blocked() {
        assert!(is_local("http://127.0.0.1:4567/api/graphql"));
        assert!(is_local("http://localhost:11434/api/chat"));
        assert!(is_local("http://[::1]:8080/"));
        assert!(!is_local("https://api.mangadex.org/manga"));
        assert!(is_local("http://192.168.0.10:4567/"));
        assert!(!is_local("https://127.example.com/"));
        assert!(!is_local("https://8.8.8.8/"));
    }

    #[test]
    fn offline_errors_are_recognised() {
        let e = offline_error("https://api.themoviedb.org/3/tv/1");
        assert!(is_offline_error(&e), "{}", e);
        assert!(e.contains("api.themoviedb.org"));
        assert!(!is_offline_error("could not reach x (timed out)"));
    }
}
