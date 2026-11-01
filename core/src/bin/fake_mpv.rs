//! Stand-in for mpv used by the tests. It speaks just enough of the JSON IPC protocol.
//! Every argument and command it receives is appended to the file named by FAKE_MPV_LOG.

#[cfg(unix)]
fn main() {
    use serde_json::{json, Value};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::{Arc, Mutex};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let ipc = args
        .iter()
        .find_map(|a| a.strip_prefix("--input-ipc-server=").map(String::from))
        .expect("--input-ipc-server");
    let log_path = std::env::var("FAKE_MPV_LOG").expect("FAKE_MPV_LOG");
    let log = move |v: Value| {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).unwrap();
        writeln!(f, "{v}").unwrap();
    };
    log(json!({"argv": args}));

    let height: u64 = std::env::var("FAKE_HEIGHT").ok().and_then(|h| h.parse().ok()).unwrap_or(720);
    let values = json!({
        "track-list": [{"id": 1, "type": "video"}, {"id": 1, "type": "audio", "lang": "jpn"}],
        "height": height,
        "cache-buffering-state": 100,
        "demuxer-cache-duration": 30,
        "time-pos": 42.5
    });

    let _ = std::fs::remove_file(&ipc);
    let listener = UnixListener::bind(&ipc).expect("bind");
    let (stream, _) = listener.accept().expect("accept");
    let writer = Arc::new(Mutex::new(stream.try_clone().unwrap()));
    let send = {
        let writer = writer.clone();
        move |v: Value| {
            let mut w = writer.lock().unwrap();
            let _ = writeln!(w, "{v}");
        }
    };

    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        let msg: Value = serde_json::from_str(&line).unwrap();
        let command = msg["command"].as_array().cloned().unwrap_or_default();
        let id = msg["request_id"].clone();
        log(json!({"command": command}));
        let name = command.first().and_then(|c| c.as_str()).unwrap_or("");

        let data = if name == "get_property" {
            values[command[1].as_str().unwrap_or("")].clone()
        } else {
            Value::Null
        };
        send(json!({"request_id": id, "error": "success", "data": data}));

        match name {
            "observe_property" => {
                let prop = command[2].as_str().unwrap_or("");
                send(json!({"event": "property-change", "name": prop, "data": values[prop]}));
            }
            "script-message" if command.get(1).and_then(|c| c.as_str()) == Some("rh-set-qualities") => {
                // Pretend the user clicked the second entry of the quality menu.
                let send = send.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    send(json!({"event": "client-message", "args": ["rh-quality-pick", "1"]}));
                });
            }
            "loadfile" => {
                let send = send.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    send(json!({"event": "file-loaded"}));
                });
            }
            "quit" => std::process::exit(0),
            _ => {}
        }
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("fake-mpv is only used by the Unix tests");
}
