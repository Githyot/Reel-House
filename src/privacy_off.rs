//! Builds without the `privacy` feature: no IP protection, nothing is ever proxied.
use crate::json::Json;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cfg {
    pub mode: String,
    pub proxy_url: String,
    pub wg_conf: String,
    pub wg_name: String,
    pub torrent_over_tor: bool,
}

impl Cfg {
    pub fn from_json(_: &Json) -> Cfg {
        Cfg { mode: "off".into(), ..Cfg::default() }
    }
}

pub fn init() {}

pub fn started() {}
pub fn apply(_: Cfg) -> Result<(), String> {
    Err("This build has no IP protection.".into())
}
pub fn cfg() -> Cfg {
    Cfg { mode: "off".into(), ..Cfg::default() }
}
pub fn generation() -> u64 {
    0
}
pub fn active() -> bool {
    false
}
pub fn ready() -> bool {
    false
}
pub fn take_dropped() -> bool {
    false
}
pub fn http_proxy() -> Option<String> {
    None
}
pub fn socks_proxy() -> Option<String> {
    None
}
pub fn proxy_port() -> u16 {
    0
}
pub fn gate() -> Result<(), String> {
    Ok(())
}
pub fn torrent_allowed() -> Result<(), String> {
    Ok(())
}
pub fn rqbit_args() -> Vec<String> {
    Vec::new()
}
pub fn magnet_for_tunnel(m: &str) -> String {
    m.to_string()
}
pub fn set_exit_ip(_: Result<String, String>) {}
pub fn system_tunnel_up() -> bool {
    false
}
pub fn state_json() -> Json {
    Json::obj(vec![("mode", "off".into()), ("phase", "off".into()), ("tor", false.into()), ("unavailable", true.into())])
}
pub fn connect_tcp(host: &str, port: u16, timeout: Duration) -> std::io::Result<std::net::TcpStream> {
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
pub fn mode() -> String {
    "off".into()
}
pub fn qt_port() -> u16 {
    0
}
pub fn warp_register(_: &dyn crate::http::Http) -> Result<String, String> {
    Err("This build has no IP protection.".into())
}
