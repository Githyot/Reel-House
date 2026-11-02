//! The stream that last played each title, so the next episode keeps the same release and resuming a video picks
//! the very stream that worked (idea from NuvioMobile's stream link cache and binge groups, GPL-3.0; written anew).
//! Kept in `stream_memory.json`: one small entry per title, the newest 300 titles.
use crate::json::Json;
use crate::streams::{key_of, Stream};
use std::fs;
use std::path::Path;

const FILE: &str = "stream_memory.json";
const KEEP: usize = 300;

/// What is kept of the stream that played a title last.
#[derive(Clone, Debug)]
pub struct Remembered {
    pub meta_id: String,
    /// The video it played ("tt123:1:5" or the movie id).
    pub video_id: String,
    /// `streams::key_of` of that stream.
    pub key: String,
    /// Enough of the stream to find its release again (name, text, file name, quality, binge group).
    pub stream: Stream,
}

fn to_json(r: &Remembered) -> Json {
    let s = &r.stream;
    Json::obj(vec![
        ("meta", r.meta_id.clone().into()),
        ("video", r.video_id.clone().into()),
        ("key", r.key.clone().into()),
        ("addon", s.addon.clone().into()),
        ("name", s.name.clone().into()),
        ("desc", s.desc.clone().into()),
        ("filename", s.filename.clone().unwrap_or_default().into()),
        ("binge", s.binge_group.clone().unwrap_or_default().into()),
        ("res", s.res.into()),
    ])
}

fn from_json(j: &Json) -> Remembered {
    let nz = |s: String| if s.is_empty() { None } else { Some(s) };
    Remembered {
        meta_id: j.get("meta").str(),
        video_id: j.get("video").str(),
        key: j.get("key").str(),
        stream: Stream {
            addon: j.get("addon").str(),
            name: j.get("name").str(),
            desc: j.get("desc").str(),
            filename: nz(j.get("filename").str()),
            binge_group: nz(j.get("binge").str()),
            res: j.get("res").as_u64().unwrap_or(0) as u32,
            ..Default::default()
        },
    }
}

fn load_all(dir: &Path) -> Vec<Remembered> {
    fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|t| Json::parse(&t).ok())
        .map(|j| j.items().iter().map(from_json).filter(|r| !r.meta_id.is_empty()).collect())
        .unwrap_or_default()
}

/// The stream that last played `meta_id`, if any.
pub fn recall(dir: &Path, meta_id: &str) -> Option<Remembered> {
    if meta_id.is_empty() {
        return None;
    }
    load_all(dir).into_iter().find(|r| r.meta_id == meta_id)
}

/// Keeps `stream` as the last one that played `video_id` of `meta_id`. Copies on disk are not kept (they are found
/// by themselves).
pub fn remember(dir: &Path, meta_id: &str, video_id: &str, stream: &Stream) {
    if meta_id.is_empty() || stream.local.is_some() {
        return;
    }
    let key = key_of(stream);
    if key.is_empty() {
        return;
    }
    let mut all = load_all(dir);
    all.retain(|r| r.meta_id != meta_id);
    let kept = Stream {
        addon: stream.addon.clone(),
        name: stream.name.clone(),
        desc: stream.desc.clone(),
        filename: stream.filename.clone(),
        binge_group: stream.binge_group.clone(),
        res: stream.res,
        ..Default::default()
    };
    all.insert(0, Remembered { meta_id: meta_id.to_string(), video_id: video_id.to_string(), key, stream: kept });
    all.truncate(KEEP);
    let _ = fs::create_dir_all(dir);
    let tmp = dir.join(format!("{}.tmp", FILE));
    if fs::write(&tmp, Json::Arr(all.iter().map(to_json).collect()).dump()).is_ok() {
        let _ = fs::rename(&tmp, dir.join(FILE));
    }
}

/// For choosing a stream of `video_id`: the key to play again when it is the same video, and the stream whose
/// release the next episode should keep.
pub fn hints(dir: &Path, meta_id: &str, video_id: &str) -> (Option<String>, Option<Stream>) {
    match recall(dir, meta_id) {
        Some(r) => (if r.video_id == video_id { Some(r.key) } else { None }, Some(r.stream)),
        None => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_one_stream_per_title() {
        let dir = std::env::temp_dir().join(format!("rh-streammem-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = Stream { name: "1080p".into(), info_hash: Some("aa".into()), binge_group: Some("g".into()), res: 1080, ..Default::default() };
        remember(&dir, "tt1", "tt1:1:4", &s);
        let t = Stream { info_hash: Some("bb".into()), binge_group: Some("h".into()), ..Default::default() };
        remember(&dir, "tt1", "tt1:1:5", &t);
        remember(&dir, "tt2", "tt2", &s);
        let (same, prev) = hints(&dir, "tt1", "tt1:1:5");
        assert_eq!(same.as_deref(), Some("bb"));
        assert_eq!(prev.unwrap().binge_group.as_deref(), Some("h"));
        let (same, prev) = hints(&dir, "tt1", "tt1:1:6");
        assert!(same.is_none());
        assert!(prev.is_some());
        assert_eq!(recall(&dir, "tt2").unwrap().stream.res, 1080);
        // a copy on disk is not kept
        let local = Stream { local: Some("/x.mkv".into()), ..Default::default() };
        remember(&dir, "tt3", "tt3", &local);
        assert!(recall(&dir, "tt3").is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
