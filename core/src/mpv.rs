//! Controls an mpv process over its JSON IPC (https://mpv.io/manual/master/#json-ipc).
//! The app ships its own mpv and starts it with a private socket (Unix) or named pipe (Windows).

use crate::error::{Error, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, oneshot, Mutex as AsyncMutex};

type Reply = std::result::Result<Value, String>;
type Reader = Box<dyn AsyncRead + Send + Unpin>;
type Writer = Box<dyn AsyncWrite + Send + Unpin>;

#[derive(Clone, Debug)]
pub enum MpvEvent {
    Property { name: String, value: Value },
    /// A `script-message` sent by a Lua script, for example the quality button.
    Client(Vec<String>),
    /// Any other mpv event such as "file-loaded" or "end-file".
    Named(String),
    Exited,
}

pub struct Mpv {
    writer: AsyncMutex<Writer>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Reply>>>>,
    props: Arc<Mutex<HashMap<String, Value>>>,
    next_id: AtomicU64,
    events: broadcast::Sender<MpvEvent>,
    closed: Arc<AtomicBool>,
    child: AsyncMutex<Option<tokio::process::Child>>,
}

fn socket_path() -> String {
    let unique = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0));
    if cfg!(windows) {
        format!(r"\\.\pipe\reelhouse-mpv-{unique}")
    } else {
        std::env::temp_dir().join(format!("reelhouse-mpv-{unique}.sock")).to_string_lossy().into_owned()
    }
}

#[cfg(unix)]
async fn open_ipc(path: &str) -> std::io::Result<(Reader, Writer)> {
    let s = tokio::net::UnixStream::connect(path).await?;
    let (r, w) = s.into_split();
    Ok((Box::new(r), Box::new(w)))
}

#[cfg(windows)]
async fn open_ipc(path: &str) -> std::io::Result<(Reader, Writer)> {
    let c = tokio::net::windows::named_pipe::ClientOptions::new().open(path)?;
    let (r, w) = tokio::io::split(c);
    Ok((Box::new(r), Box::new(w)))
}

impl Mpv {
    /// Starts mpv with `args` and connects to it. `binary` is a path or a name on PATH.
    pub async fn start(binary: &str, args: &[String]) -> Result<Arc<Mpv>> {
        let ipc = socket_path();
        #[cfg(unix)]
        let _ = std::fs::remove_file(&ipc);

        let mut cmd = tokio::process::Command::new(binary);
        cmd.args(args)
            .arg(format!("--input-ipc-server={ipc}"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // no console window

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::msg(format!("mpv was not found (\"{binary}\"). Reinstall Reelhouse, or set the mpv path in Settings > General."))
            } else {
                Error::from(e)
            }
        })?;

        let mut connected = None;
        for _ in 0..80 {
            if let Ok(Some(_)) = child.try_wait() {
                return Err(Error::msg("mpv closed before it was ready. Check the mpv path and your GPU drivers."));
            }
            match open_ipc(&ipc).await {
                Ok(rw) => {
                    connected = Some(rw);
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(150)).await,
            }
        }
        let (reader, writer) = connected.ok_or_else(|| Error::msg("Could not connect to mpv."))?;

        let (events, _) = broadcast::channel(256);
        let mpv = Arc::new(Mpv {
            writer: AsyncMutex::new(writer),
            pending: Arc::new(Mutex::new(HashMap::new())),
            props: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            events,
            closed: Arc::new(AtomicBool::new(false)),
            child: AsyncMutex::new(Some(child)),
        });
        mpv.spawn_reader(reader);
        Ok(mpv)
    }

    fn spawn_reader(&self, reader: Reader) {
        let pending = self.pending.clone();
        let props = self.props.clone();
        let events = self.events.clone();
        let closed = self.closed.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(id) = msg.get("request_id").and_then(|v| v.as_u64()) {
                    if let Some(tx) = pending.lock().unwrap().remove(&id) {
                        let reply = if msg["error"] == "success" {
                            Ok(msg.get("data").cloned().unwrap_or(Value::Null))
                        } else {
                            Err(msg["error"].as_str().unwrap_or("error").to_string())
                        };
                        let _ = tx.send(reply);
                    }
                    continue;
                }
                match msg.get("event").and_then(|e| e.as_str()) {
                    Some("property-change") => {
                        let name = msg["name"].as_str().unwrap_or("").to_string();
                        let value = msg.get("data").cloned().unwrap_or(Value::Null);
                        props.lock().unwrap().insert(name.clone(), value.clone());
                        let _ = events.send(MpvEvent::Property { name, value });
                    }
                    Some("client-message") => {
                        let args = msg["args"].as_array().map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
                        let _ = events.send(MpvEvent::Client(args));
                    }
                    Some(other) => {
                        let _ = events.send(MpvEvent::Named(other.to_string()));
                    }
                    None => {}
                }
            }
            closed.store(true, Ordering::SeqCst);
            pending.lock().unwrap().clear(); // dropping the senders fails every waiting command
            let _ = events.send(MpvEvent::Exited);
        });
    }

    pub fn running(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MpvEvent> {
        self.events.subscribe()
    }

    pub async fn command(&self, args: Vec<Value>) -> Result<Value> {
        if !self.running() {
            return Err(Error::msg("mpv is not running."));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let line = format!("{}\n", json!({"command": args, "request_id": id}));
        {
            let mut w = self.writer.lock().await;
            w.write_all(line.as_bytes()).await?;
            w.flush().await?;
        }
        match tokio::time::timeout(Duration::from_secs(20), rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(Error::msg(format!("mpv: {e}"))),
            _ => Err(Error::msg("mpv did not answer.")),
        }
    }

    /// Convenience for commands made of strings only.
    pub async fn cmd(&self, args: &[&str]) -> Result<Value> {
        self.command(args.iter().map(|s| Value::String((*s).to_string())).collect()).await
    }

    pub async fn set_property(&self, name: &str, value: Value) -> Result<()> {
        self.command(vec![json!("set_property"), json!(name), value]).await.map(|_| ())
    }

    pub async fn get_property(&self, name: &str) -> Result<Value> {
        self.command(vec![json!("get_property"), json!(name)]).await
    }

    pub async fn show_text(&self, text: &str, ms: u32) {
        let _ = self.command(vec![json!("show-text"), json!(text), json!(ms)]).await;
    }

    pub async fn observe(&self, names: &[&str]) -> Result<()> {
        for (i, n) in names.iter().enumerate() {
            self.command(vec![json!("observe_property"), json!(100 + i), json!(n)]).await?;
        }
        Ok(())
    }

    pub fn prop(&self, name: &str) -> Option<Value> {
        self.props.lock().unwrap().get(name).cloned()
    }

    /// Waits until `pred` is true for the observed properties. Fails on timeout or when mpv closes.
    pub async fn wait_for(&self, what: &str, timeout: Duration, pred: impl Fn(&HashMap<String, Value>) -> bool) -> Result<()> {
        let start = Instant::now();
        loop {
            if pred(&self.props.lock().unwrap()) {
                return Ok(());
            }
            if !self.running() {
                return Err(Error::msg("mpv was closed."));
            }
            if start.elapsed() > timeout {
                return Err(Error::msg(format!("Timed out waiting for {what}.")));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn quit(&self) {
        if self.running() {
            let _ = self.cmd(&["quit"]).await;
        }
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            if tokio::time::timeout(Duration::from_millis(1500), child.wait()).await.is_err() {
                let _ = child.kill().await;
            }
        }
    }
}
