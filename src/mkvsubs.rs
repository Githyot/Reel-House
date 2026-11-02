//! Reads the text subtitles built into a Matroska / WebM file (MKV), so Forced translation can translate the
//! track the video carries. Streams the file once from the start (a local file or the HTTP address the player
//! plays, such as the torrent engine's), keeping only the blocks of the chosen track. SRT, ASS/SSA and WebVTT
//! tracks are read; picture subtitles (PGS, VobSub) are not text and cannot be translated.

use crate::subtitles::Cue;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};

const EBML: u64 = 0x1A45DFA3;
const SEGMENT: u64 = 0x18538067;
const INFO: u64 = 0x1549A966;
const TIMECODE_SCALE: u64 = 0x2AD7B1;
const TRACKS: u64 = 0x1654AE6B;
const TRACK_ENTRY: u64 = 0xAE;
const TRACK_NUMBER: u64 = 0xD7;
const TRACK_TYPE: u64 = 0x83;
const CODEC_ID: u64 = 0x86;
const LANGUAGE: u64 = 0x22B59C;
const LANGUAGE_BCP47: u64 = 0x22B59D;
const NAME: u64 = 0x536E;
const CONTENT_ENCODINGS: u64 = 0x6D80;
const CONTENT_ENCODING: u64 = 0x6240;
const CONTENT_COMPRESSION: u64 = 0x5034;
const COMP_ALGO: u64 = 0x4254;
const COMP_SETTINGS: u64 = 0x4255;
const CLUSTER: u64 = 0x1F43B675;
const TIMECODE: u64 = 0xE7;
const SIMPLE_BLOCK: u64 = 0xA3;
const BLOCK_GROUP: u64 = 0xA0;
const BLOCK: u64 = 0xA1;
const BLOCK_DURATION: u64 = 0x9B;

/// One subtitle track of the file.
#[derive(Clone, Debug, Default)]
pub struct SubTrack {
    /// The Matroska track number (what mpv shows as the track's `src-id`).
    pub number: u64,
    pub codec: String,
    pub lang: String,
    pub name: String,
    /// Content compression: 0 zlib, 3 header stripping (with the stripped bytes).
    comp: Option<u64>,
    comp_settings: Vec<u8>,
}

impl SubTrack {
    pub fn is_text(&self) -> bool {
        let c = self.codec.to_ascii_uppercase();
        c.starts_with("S_TEXT/") || c == "S_ASS" || c == "S_SSA"
    }
}

/// Whether mpv's codec name of a track is a picture subtitle (cannot be translated).
pub fn is_picture_codec(codec: &str) -> bool {
    let c = codec.to_ascii_lowercase();
    c.contains("pgs") || c.contains("dvd_sub") || c.contains("dvb_sub") || c.contains("vobsub") || c == "xsub"
}

/// Which subtitle track to read: the Matroska number mpv reported, else the nth subtitle track of the file
/// (mpv numbers the built-in subtitle tracks in file order).
#[derive(Clone, Debug)]
pub struct Want {
    pub src_id: Option<u64>,
    pub nth: usize,
}

impl Want {
    fn pick<'a>(&self, subs: &'a [SubTrack]) -> Option<&'a SubTrack> {
        self.src_id.and_then(|n| subs.iter().find(|t| t.number == n)).or_else(|| subs.get(self.nth))
    }
}

struct Src<'a> {
    r: &'a mut dyn Read,
    pos: u64,
    last_report: u64,
    progress: &'a mut dyn FnMut(u64),
}

impl Src<'_> {
    fn byte(&mut self) -> std::io::Result<Option<u8>> {
        let mut b = [0u8; 1];
        loop {
            match self.r.read(&mut b) {
                Ok(0) => return Ok(None),
                Ok(_) => {
                    self.advance(1);
                    return Ok(Some(b[0]));
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }
    fn advance(&mut self, n: u64) {
        self.pos += n;
        if self.pos - self.last_report >= 4 * 1024 * 1024 {
            self.last_report = self.pos;
            (self.progress)(self.pos);
        }
    }
    fn exact(&mut self, n: usize) -> std::io::Result<Vec<u8>> {
        let mut v = vec![0u8; n];
        self.r.read_exact(&mut v)?;
        self.advance(n as u64);
        Ok(v)
    }
    fn skip(&mut self, n: u64) -> std::io::Result<()> {
        let got = std::io::copy(&mut (&mut *self.r).take(n), &mut std::io::sink())?;
        self.advance(got);
        if got < n {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        Ok(())
    }
    /// An element id (marker bits kept). None at the end of the file.
    fn id(&mut self) -> std::io::Result<Option<u64>> {
        let Some(first) = self.byte()? else { return Ok(None) };
        let len = first.leading_zeros() as usize + 1;
        if len > 4 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad element id"));
        }
        let mut v = first as u64;
        for _ in 1..len {
            let b = self.byte()?.ok_or(std::io::ErrorKind::UnexpectedEof)?;
            v = (v << 8) | b as u64;
        }
        Ok(Some(v))
    }
    /// An element size; None for "unknown size".
    fn size(&mut self) -> std::io::Result<Option<u64>> {
        let first = self.byte()?.ok_or(std::io::ErrorKind::UnexpectedEof)?;
        let len = first.leading_zeros() as usize + 1;
        if len > 8 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad element size"));
        }
        let mut v = (first as u64) & (0xFF >> len);
        let mut all_ones = v == (0xFF >> len) as u64;
        for _ in 1..len {
            let b = self.byte()?.ok_or(std::io::ErrorKind::UnexpectedEof)?;
            all_ones &= b == 0xFF;
            v = (v << 8) | b as u64;
        }
        Ok(if all_ones { None } else { Some(v) })
    }
}

fn uint(b: &[u8]) -> u64 {
    b.iter().fold(0u64, |a, x| (a << 8) | *x as u64)
}

/// A variable-length number at the start of a block (the track number); (value, bytes used).
fn vint(b: &[u8]) -> Option<(u64, usize)> {
    let first = *b.first()?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 || b.len() < len {
        return None;
    }
    let mut v = (first as u64) & (0xFF >> len);
    for x in &b[1..len] {
        v = (v << 8) | *x as u64;
    }
    Some((v, len))
}

struct Raw {
    start_ms: i64,
    dur_ms: Option<i64>,
    data: Vec<u8>,
}

/// Reads the whole file and returns the cues of the wanted subtitle track, in SRT-style times.
/// `progress(bytes read)` is called every few megabytes. Stops with an error when `cancel` is set.
pub fn extract(r: &mut dyn Read, want: &Want, progress: &mut dyn FnMut(u64), cancel: &AtomicBool) -> Result<(SubTrack, Vec<Cue>), String> {
    let mut src = Src { r, pos: 0, last_report: 0, progress };
    let mut scale: u64 = 1_000_000;
    let mut tracks: Vec<(u64, SubTrack)> = Vec::new(); // (track type, track)
    let mut chosen: Option<SubTrack> = None;
    let mut cluster_tc: i64 = 0;
    let mut blocks: Vec<Raw> = Vec::new();
    // the last block read belongs to the block group now open (its BlockDuration is for it)
    let mut group_ours = false;
    let mut first = true;
    let io = |e: std::io::Error| format!("Could not read the video file ({}).", e);
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("Cancelled.".to_string());
        }
        let id = match src.id() {
            Ok(Some(id)) => id,
            Ok(None) => break,
            Err(_) if first => return Err("Only subtitles inside MKV or WebM videos can be read for translation.".to_string()),
            // a file that ends early (still downloading): keep what was read
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && chosen.is_some() => break,
            Err(e) => return Err(io(e)),
        };
        if first {
            first = false;
            if id != EBML {
                return Err("Only subtitles inside MKV or WebM videos can be read for translation.".to_string());
            }
        }
        let size = match src.size() {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && chosen.is_some() => break,
            Err(e) => return Err(io(e)),
        };
        // Containers are read through: their children follow one by one.
        if matches!(id, SEGMENT | CLUSTER | TRACKS | TRACK_ENTRY | INFO | BLOCK_GROUP | CONTENT_ENCODINGS | CONTENT_ENCODING | CONTENT_COMPRESSION) {
            match id {
                TRACK_ENTRY => tracks.push((0, SubTrack::default())),
                CONTENT_COMPRESSION => {
                    if let Some((_, t)) = tracks.last_mut() {
                        t.comp = Some(0);
                    }
                }
                CLUSTER => {
                    // the track list is complete once the first cluster starts
                    if chosen.is_none() {
                        let subs: Vec<SubTrack> = tracks.iter().filter(|(k, _)| *k == 0x11).map(|(_, t)| t.clone()).collect();
                        let Some(t) = want.pick(&subs) else { return Err("The subtitle track was not found in the video file.".to_string()) };
                        if !t.is_text() {
                            return Err(format!("This subtitle track is made of pictures ({}), not text, so it cannot be translated.", t.codec));
                        }
                        chosen = Some(t.clone());
                    }
                }
                BLOCK_GROUP => group_ours = false,
                _ => {}
            }
            continue;
        }
        let Some(size) = size else { return Err("The video file has an element of unknown size.".to_string()) };
        let small = size <= 64 * 1024;
        match id {
            TIMECODE_SCALE | TRACK_NUMBER | TRACK_TYPE | COMP_ALGO | TIMECODE | BLOCK_DURATION if small => {
                let v = uint(&src.exact(size as usize).map_err(io)?);
                match id {
                    TIMECODE_SCALE => scale = v.max(1),
                    TRACK_NUMBER => {
                        if let Some((_, t)) = tracks.last_mut() {
                            t.number = v
                        }
                    }
                    TRACK_TYPE => {
                        if let Some((k, _)) = tracks.last_mut() {
                            *k = v
                        }
                    }
                    COMP_ALGO => {
                        if let Some((_, t)) = tracks.last_mut() {
                            t.comp = Some(v)
                        }
                    }
                    TIMECODE => cluster_tc = v as i64,
                    BLOCK_DURATION if group_ours => {
                        if let Some(b) = blocks.last_mut() {
                            b.dur_ms = Some((v as i128 * scale as i128 / 1_000_000) as i64);
                        }
                    }
                    _ => {}
                }
            }
            CODEC_ID | LANGUAGE | LANGUAGE_BCP47 | NAME | COMP_SETTINGS if small => {
                let b = src.exact(size as usize).map_err(io)?;
                if let Some((_, t)) = tracks.last_mut() {
                    let text = String::from_utf8_lossy(&b).trim_end_matches('\0').to_string();
                    match id {
                        CODEC_ID => t.codec = text,
                        LANGUAGE if t.lang.is_empty() => t.lang = text,
                        LANGUAGE_BCP47 => t.lang = text,
                        NAME => t.name = text,
                        COMP_SETTINGS => t.comp_settings = b,
                        _ => {}
                    }
                }
            }
            SIMPLE_BLOCK | BLOCK => {
                let Some(want_no) = chosen.as_ref().map(|t| t.number) else {
                    src.skip(size).map_err(io)?;
                    continue;
                };
                let head_len = size.min(12) as usize;
                let head = match src.exact(head_len) {
                    Ok(h) => h,
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                    Err(e) => return Err(io(e)),
                };
                let Some((track, n)) = vint(&head) else {
                    src.skip(size - head_len as u64).map_err(io)?;
                    continue;
                };
                if track != want_no || head.len() < n + 3 || head[n + 2] & 0x06 != 0 {
                    match src.skip(size - head_len as u64) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                        Err(e) => return Err(io(e)),
                    }
                    continue;
                }
                let rel = i16::from_be_bytes([head[n], head[n + 1]]) as i64;
                let mut data = head[n + 3..].to_vec();
                match src.exact((size - head_len as u64) as usize) {
                    Ok(rest) => data.extend(rest),
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                    Err(e) => return Err(io(e)),
                }
                let start_ms = ((cluster_tc + rel) as i128 * scale as i128 / 1_000_000) as i64;
                blocks.push(Raw { start_ms, dur_ms: None, data });
                group_ours = id == BLOCK;
            }
            _ => match src.skip(size) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && chosen.is_some() => break,
                Err(e) => return Err(io(e)),
            },
        }
    }
    let Some(track) = chosen else { return Err("The video file has no subtitle tracks Reelhouse can read.".to_string()) };
    let cues = to_cues(&track, blocks)?;
    Ok((track, cues))
}

fn unpack(track: &SubTrack, data: Vec<u8>) -> Result<Vec<u8>, String> {
    match track.comp {
        Some(0) => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(&data[..]).read_to_end(&mut out).map_err(|e| format!("Could not unpack a subtitle line ({}).", e))?;
            Ok(out)
        }
        Some(3) => {
            let mut out = track.comp_settings.clone();
            out.extend(data);
            Ok(out)
        }
        _ => Ok(data),
    }
}

/// The text of one ASS/SSA event as it is stored in Matroska ("ReadOrder,Layer,Style,Name,MarginL,MarginR,
/// MarginV,Effect,Text"): override tags removed, line breaks kept. None for drawings and empty lines.
pub fn ass_text(event: &str) -> Option<String> {
    let text = event.splitn(9, ',').nth(8)?;
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let tag_end = rest[i..].find('}').map(|j| i + j + 1).unwrap_or(rest.len());
        let tag = &rest[i..tag_end];
        // \p1 and up start a vector drawing: nothing to read
        if tag.contains("\\p") && tag.split("\\p").skip(1).any(|t| t.chars().next().map(|c| c.is_ascii_digit() && c != '0').unwrap_or(false)) {
            return None;
        }
        rest = &rest[tag_end..];
    }
    out.push_str(rest);
    let out = out.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ");
    let lines: Vec<&str> = out.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() { None } else { Some(lines.join("\n")) }
}

fn srt_time(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

fn to_cues(track: &SubTrack, blocks: Vec<Raw>) -> Result<Vec<Cue>, String> {
    let codec = track.codec.to_ascii_uppercase();
    let ass = codec.contains("ASS") || codec.contains("SSA");
    let mut items: Vec<(i64, Option<i64>, String)> = Vec::new();
    for b in blocks {
        let raw = unpack(track, b.data)?;
        let text = String::from_utf8_lossy(&raw).to_string();
        let text = if ass { ass_text(&text) } else { Some(text.trim().to_string()).filter(|t| !t.is_empty()) };
        if let Some(t) = text {
            items.push((b.start_ms, b.dur_ms, t));
        }
    }
    items.sort_by_key(|x| x.0);
    let mut cues = Vec::new();
    for i in 0..items.len() {
        let (start, dur, text) = &items[i];
        let end = match dur {
            Some(d) if *d > 0 => start + d,
            // no duration given: until the next line, at most 5 s
            _ => items.get(i + 1).map(|n| n.0.min(start + 5000)).filter(|e| e > start).unwrap_or(start + 3000),
        };
        cues.push(Cue { start: srt_time(*start), end: srt_time(end), text: text.clone() });
    }
    if cues.is_empty() {
        return Err("The subtitle track has no lines yet (the video may still be downloading).".to_string());
    }
    Ok(cues)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(id: u64, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let idb = id.to_be_bytes();
        let skip = idb.iter().position(|b| *b != 0).unwrap();
        v.extend(&idb[skip..]);
        // 8-byte size
        v.push(0x01);
        v.extend(&(body.len() as u64).to_be_bytes()[1..]);
        v.extend(body);
        v
    }
    fn u(id: u64, x: u64) -> Vec<u8> {
        el(id, &x.to_be_bytes())
    }
    fn block(track: u8, rel: i16, data: &[u8]) -> Vec<u8> {
        let mut b = vec![0x80 | track];
        b.extend(rel.to_be_bytes());
        b.push(0x80);
        b.extend(data);
        b
    }

    fn file(comp: bool) -> Vec<u8> {
        let mut tracks = Vec::new();
        tracks.extend(el(TRACK_ENTRY, &[u(TRACK_NUMBER, 1), u(TRACK_TYPE, 1), el(CODEC_ID, b"V_MPEG4/ISO/AVC")].concat()));
        let mut sub = [u(TRACK_NUMBER, 2), u(TRACK_TYPE, 0x11), el(CODEC_ID, b"S_TEXT/UTF8"), el(LANGUAGE, b"rus")].concat();
        tracks.extend(el(TRACK_ENTRY, &sub));
        sub = [u(TRACK_NUMBER, 3), u(TRACK_TYPE, 0x11), el(CODEC_ID, b"S_TEXT/ASS"), el(LANGUAGE, b"eng")].concat();
        if comp {
            sub.extend(el(CONTENT_ENCODINGS, &el(CONTENT_ENCODING, &el(CONTENT_COMPRESSION, &u(COMP_ALGO, 0)))));
        }
        tracks.extend(el(TRACK_ENTRY, &sub));
        let pack = |s: &str| -> Vec<u8> {
            if !comp {
                return s.as_bytes().to_vec();
            }
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            std::io::Write::write_all(&mut e, s.as_bytes()).unwrap();
            e.finish().unwrap()
        };
        let mut cluster = u(TIMECODE, 1000);
        cluster.extend(el(SIMPLE_BLOCK, &block(1, 0, &[0u8; 5000])));
        cluster.extend(el(SIMPLE_BLOCK, &block(2, 0, "Привет".as_bytes())));
        let g = [el(BLOCK, &block(3, 500, &pack("0,0,Default,,0,0,0,,{\\i1}The volcano\\Nhas just erupted"))), u(BLOCK_DURATION, 2000)].concat();
        cluster.extend(el(BLOCK_GROUP, &g));
        cluster.extend(el(BLOCK_GROUP, &[el(BLOCK, &block(3, 900, &pack("1,0,Sign,,0,0,0,,{\\p1}m 0 0 l 10 10"))), u(BLOCK_DURATION, 100)].concat()));
        let mut seg = el(INFO, &u(TIMECODE_SCALE, 1_000_000));
        seg.extend(el(TRACKS, &tracks));
        seg.extend(el(0x1941A469, &[0u8; 300])); // attachments, skipped
        seg.extend(el(CLUSTER, &cluster));
        [el(EBML, &[0x42, 0x86, 0x81, 0x01]), el(SEGMENT, &seg)].concat()
    }

    #[test]
    fn reads_the_ass_track_by_number() {
        for comp in [false, true] {
            let f = file(comp);
            let (t, cues) = extract(&mut &f[..], &Want { src_id: Some(3), nth: 0 }, &mut |_| {}, &AtomicBool::new(false)).unwrap();
            assert_eq!(t.lang, "eng");
            assert_eq!(cues.len(), 1);
            assert_eq!(cues[0].start, "00:00:01,500");
            assert_eq!(cues[0].end, "00:00:03,500");
            assert_eq!(cues[0].text, "The volcano\nhas just erupted");
        }
    }

    #[test]
    fn falls_back_to_the_nth_subtitle_track() {
        let f = file(false);
        let (t, cues) = extract(&mut &f[..], &Want { src_id: None, nth: 0 }, &mut |_| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(t.number, 2);
        assert_eq!(cues[0].text, "Привет");
        assert_eq!(cues[0].end, "00:00:04,000");
    }

    #[test]
    fn not_a_matroska_file() {
        let e = extract(&mut &b"\x00\x00\x00\x18ftypmp42"[..], &Want { src_id: None, nth: 0 }, &mut |_| {}, &AtomicBool::new(false)).unwrap_err();
        assert!(e.contains("MKV"));
    }

    #[test]
    fn ass_lines() {
        assert_eq!(ass_text("0,0,Default,,0,0,0,,Hello\\hthere").as_deref(), Some("Hello there"));
        assert_eq!(ass_text("0,0,Default,,0,0,0,,{\\an8}Top, with comma").as_deref(), Some("Top, with comma"));
        assert_eq!(ass_text("0,0,Default,,0,0,0,,{\\p0}x").as_deref(), Some("x"));
        assert_eq!(ass_text("0,0,Default,,0,0,0,,"), None);
    }
}
