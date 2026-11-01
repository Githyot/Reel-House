//! End-to-end playback tests against a fake mpv. They run as one test because the fake reads its
//! settings from environment variables, which are shared by the whole process.
#![cfg(unix)]

use httpmock::prelude::*;
use reelhouse_core::anime4k::{required_files, ShaderManager};
use reelhouse_core::engine::{Engine, EngineConfig, PlayRequest};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Events = Arc<Mutex<Vec<(String, Value)>>>;

struct Rig {
    engine: Arc<Engine>,
    events: Events,
    log: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

fn commands(rig: &Rig) -> Vec<Value> {
    std::fs::read_to_string(&rig.log)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn names(rig: &Rig) -> Vec<String> {
    commands(rig)
        .iter()
        .filter_map(|c| c.get("command").and_then(|c| c.as_array()).map(|a| a.iter().map(|x| x.to_string().trim_matches('"').to_string()).collect::<Vec<_>>().join(" ")))
        .collect()
}

async fn wait_stage(rig: &Rig, stage: &str, secs: u64) -> Option<Value> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if let Some((_, p)) = rig.events.lock().unwrap().iter().find(|(c, p)| c == "play-status" && p["stage"] == stage) {
            return Some(p.clone());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

async fn wait_for_log(rig: &Rig, needle: &str, secs: u64) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        if names(rig).iter().any(|n| n.contains(needle)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

async fn rig(addon: &MockServer, libre_url: &str, height: u32, shaders: bool, max_wait: u64, mpv: Option<&str>) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("mpv.log");
    std::env::set_var("FAKE_MPV_LOG", &log);
    std::env::set_var("FAKE_HEIGHT", height.to_string());
    let events: Events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let engine = Engine::new(
        EngineConfig { data_dir: dir.path().to_path_buf(), bundled_mpv: None, locale: "pt-BR".into() },
        Arc::new(move |c, p| sink.lock().unwrap().push((c.to_string(), p))),
    );
    engine
        .settings
        .patch(json!({
            "general": {"mpv_path": mpv.unwrap_or(env!("CARGO_BIN_EXE_fake-mpv")), "buffer_seconds": 5},
            "upscaling": {"enabled": true, "mode": "B", "quality": "Fast"},
            "subtitles": {"provider": "libretranslate", "libretranslate_url": libre_url, "max_wait_seconds": max_wait}
        }))
        .unwrap();
    if shaders {
        let sm = ShaderManager::new(dir.path());
        std::fs::create_dir_all(&sm.dir).unwrap();
        for f in required_files() {
            std::fs::write(sm.dir.join(f), "//").unwrap();
        }
    }
    engine.addons.install(&format!("{}/manifest.json", addon.base_url())).await.unwrap();
    Rig { engine, events, log, _dir: dir }
}

fn request(engine_views: &[reelhouse_core::streams::StreamView], i: usize) -> PlayRequest {
    PlayRequest { stream: engine_views[i].stream.clone(), kind: "movie".into(), id: "tt1".into(), title: "Test Movie".into() }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn playback_scenarios() {
    let addon = MockServer::start();
    addon.mock(|w, t| {
        w.method(GET).path("/manifest.json");
        t.status(200).json_body(json!({"id":"t.addon","name":"Test","version":"1","types":["movie"],"resources":["stream","subtitles"]}));
    });
    addon.mock(|w, t| {
        w.method(GET).path("/stream/movie/tt1.json");
        t.status(200).json_body(json!({"streams":[
            {"url":"http://video.example/720.mp4","name":"720p"},
            {"url":"http://video.example/1080.mp4","name":"1080p"}]}));
    });
    addon.mock(|w, t| {
        w.method(GET).path("/subtitles/movie/tt1.json");
        t.status(200).json_body(json!({"subtitles":[{"id":"1","lang":"eng","url": addon.url("/en.srt")}]}));
    });
    addon.mock(|w, t| {
        w.method(GET).path("/en.srt");
        t.status(200).body("1\n00:00:01,000 --> 00:00:02,000\nHello\n\n");
    });
    let libre = MockServer::start();
    libre.mock(|w, t| {
        w.method(POST).path("/translate");
        t.status(200).json_body(json!({"translatedText": ["PT:Hello"]}));
    });

    // 1. Normal play: paused start, shaders, translated subtitle added before unpausing, then a quality switch.
    {
        let rig = rig(&addon, &libre.base_url(), 720, true, 5, None).await;
        let views = rig.engine.streams("movie", "tt1").await;
        assert_eq!(views[0].quality, "1080p", "the best stream is listed first");
        rig.engine.play(request(&views, 0)).await;
        let ready = wait_stage(&rig, "ready", 20).await.expect("ready status");
        assert_eq!(ready["message"], "Playing in mpv");

        let all = commands(&rig);
        let argv: Vec<String> = all[0]["argv"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect();
        assert!(argv.contains(&"--pause=yes".to_string()), "mpv starts paused");
        assert!(argv.contains(&"--profile=gpu-hq".to_string()));
        assert!(argv.iter().any(|a| a.starts_with("--script=") && a.ends_with("reelhouse.lua")));
        assert!(argv.iter().any(|a| a.starts_with("--input-conf=")));

        let n = names(&rig);
        let shader = n.iter().position(|c| c.starts_with("change-list")).expect("shaders applied");
        let sub = n.iter().position(|c| c.starts_with("sub-add")).expect("subtitle added");
        let unpause = n.iter().position(|c| c == "set_property pause false").expect("unpaused");
        assert!(shader < sub && sub < unpause, "{n:?}");
        let sub_file = all.iter().find_map(|c| c["command"][0].eq("sub-add").then(|| c["command"][1].as_str().unwrap().to_string())).unwrap();
        assert!(std::fs::read_to_string(sub_file).unwrap().contains("PT:Hello"));

        // The fake mpv now "clicks" the second quality entry, which is the 720p stream.
        assert!(wait_for_log(&rig, "script-message rh-set-qualities", 5).await);
        assert!(wait_for_log(&rig, "loadfile http://video.example/720.mp4 replace -1 start=42.5", 10).await, "{:?}", names(&rig));
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(names(&rig).iter().filter(|c| c.starts_with("sub-add")).count() >= 2, "the subtitle is added again after switching");
        rig.engine.stop().await;
    }

    // 2. A 4K source is above the 1080p limit, so no shaders are applied.
    {
        let rig = rig(&addon, &libre.base_url(), 2160, true, 5, None).await;
        let views = rig.engine.streams("movie", "tt1").await;
        rig.engine.play(request(&views, 0)).await;
        wait_stage(&rig, "ready", 20).await.expect("ready");
        assert!(!names(&rig).iter().any(|c| c.starts_with("change-list")));
        rig.engine.stop().await;
    }

    // 3. Shaders not downloaded yet: playback still works, without upscaling.
    {
        let rig = rig(&addon, &libre.base_url(), 720, false, 5, None).await;
        let views = rig.engine.streams("movie", "tt1").await;
        rig.engine.play(request(&views, 0)).await;
        wait_stage(&rig, "ready", 20).await.expect("ready");
        assert!(!names(&rig).iter().any(|c| c.starts_with("change-list")));
        rig.engine.stop().await;
    }

    // 4. Translation slower than the wait limit: playback starts without subtitles and says so.
    {
        let slow = MockServer::start();
        slow.mock(|w, t| {
            w.method(POST).path("/translate");
            t.status(200).delay(Duration::from_secs(4)).json_body(json!({"translatedText": ["PT:Hello"]}));
        });
        let rig = rig(&addon, &slow.base_url(), 720, true, 1, None).await;
        let views = rig.engine.streams("movie", "tt1").await;
        rig.engine.play(request(&views, 0)).await;
        let ready = wait_stage(&rig, "ready", 20).await.expect("ready");
        assert!(ready["message"].as_str().unwrap().contains("took too long"), "{ready}");
        assert!(!names(&rig).iter().any(|c| c.starts_with("sub-add")));
        assert!(names(&rig).iter().any(|c| c == "set_property pause false"));
        rig.engine.stop().await;
    }

    // 5. A torrent stream without a Real-Debrid token explains what to do.
    {
        let rig = rig(&addon, &libre.base_url(), 720, true, 5, None).await;
        let req = PlayRequest {
            stream: serde_json::from_value(json!({"infoHash":"abc","name":"4K"})).unwrap(),
            kind: "movie".into(),
            id: "tt1".into(),
            title: "T".into(),
        };
        rig.engine.play(req).await;
        let err = wait_stage(&rig, "error", 10).await.expect("error status");
        assert!(err["message"].as_str().unwrap().contains("Real-Debrid"), "{err}");
    }

    // 6. A missing mpv gives a clear message.
    {
        let rig = rig(&addon, &libre.base_url(), 720, true, 5, Some("/nonexistent/mpv")).await;
        let views = rig.engine.streams("movie", "tt1").await;
        rig.engine.play(request(&views, 0)).await;
        let err = wait_stage(&rig, "error", 10).await.expect("error status");
        assert!(err["message"].as_str().unwrap().contains("mpv was not found"), "{err}");
    }
}
