//! Autocards (learnjapanese.moe/autocards): sentence mining into the user's own Anki through AnkiConnect.
//! The word card is made by Yomitan (Expression, Sentence); Reelhouse then completes the newest card of the mining deck
//! with the picture of the frame, the sentence audio (with padding) and the lines around the sentence as context.
//! When no fresh card is there, a sentence card is added with the same fields. Field names default to Lapis.
use crate::http::{Http, Request};
use crate::json::Json;

#[derive(Clone, Debug)]
pub struct Cfg {
    pub url: String,
    pub deck: String,
    pub model: String,
    pub f_sentence: String,
    pub f_expression: String,
    pub f_picture: String,
    pub f_audio: String,
}

/// A card younger than this is the one Yomitan just made (note ids are their creation time in ms).
pub const FRESH_MS: i64 = 15 * 60 * 1000;

/// AnkiConnect could not be reached (Anki closed, or the add-on missing): the card goes to the app's own deck instead.
pub const NOT_ANSWERING: &str = "Anki is not answering: open Anki with the AnkiConnect add-on (2055492159).";

/// One AnkiConnect call (API version 6): the `result`, or its `error`.
pub fn invoke(http: &dyn Http, url: &str, action: &str, params: Json) -> Result<Json, String> {
    let body = Json::obj(vec![("action", Json::Str(action.to_string())), ("version", Json::Num(6.0)), ("params", params)]).dump();
    let r = http.send(&Request::post(url).json_body(body).timeout(30)).map_err(|_| NOT_ANSWERING.to_string())?;
    let j = r.json()?;
    match j.get("error") {
        Json::Null => Ok(j.get("result").clone()),
        e => Err(format!("AnkiConnect: {}", e.str())),
    }
}

/// Stores a file in Anki's media folder; returns the name Anki kept.
pub fn store_media(http: &dyn Http, url: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
    let p = Json::obj(vec![("filename", Json::Str(name.to_string())), ("data", Json::Str(crate::suwayomi::base64(bytes)))]);
    let r = invoke(http, url, "storeMediaFile", p)?;
    Ok(if r.str().is_empty() { name.to_string() } else { r.str() })
}

/// The newest note of the deck, when it was made in the last minutes: (id, its fields as name -> value).
pub fn fresh_note(http: &dyn Http, cfg: &Cfg, now_ms: i64) -> Result<Option<(i64, Vec<(String, String)>)>, String> {
    let q = format!("\"deck:{}\" added:1", cfg.deck.replace('"', "\\\""));
    let ids = invoke(http, &cfg.url, "findNotes", Json::obj(vec![("query", Json::Str(q))]))?;
    let Some(id) = ids.items().iter().filter_map(|x| x.as_i64()).max() else { return Ok(None) };
    if now_ms - id > FRESH_MS {
        return Ok(None);
    }
    let info = invoke(http, &cfg.url, "notesInfo", Json::obj(vec![("notes", Json::Arr(vec![Json::Num(id as f64)]))]))?;
    let n = info.items().first().cloned().unwrap_or(Json::Null);
    let fields = n.get("fields").entries().iter().map(|(k, v)| (k.clone(), v.get("value").str())).collect();
    Ok(Some((id, fields)))
}

/// The sentence with its context: earlier lines, the line itself (Yomitan's version, with the word in bold, when it has
/// one), later lines; the context lines are smaller and grey, like the Autocards cards.
pub fn sentence_with_context(prev: &[String], line: &str, next: &[String]) -> String {
    let ctx = |v: &[String]| v.iter().map(|l| format!("<span style=\"color:#888;font-size:85%\">{}</span>", html(l))).collect::<Vec<_>>();
    let mut parts = ctx(prev);
    parts.push(line.to_string());
    parts.extend(ctx(next));
    parts.join("<br>")
}

pub fn html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\n', "<br>")
}

pub struct Mined {
    pub sentence_line: String,
    pub prev: Vec<String>,
    pub next: Vec<String>,
    pub picture: Option<(String, Vec<u8>)>,
    pub audio: Option<(String, Vec<u8>)>,
}

/// Sends the media and completes the fresh card, or adds a sentence card. Returns what was done, for the toast.
pub fn export(http: &dyn Http, cfg: &Cfg, m: &Mined, now_ms: i64) -> Result<String, String> {
    let _ = invoke(http, &cfg.url, "version", Json::obj(vec![]))?;
    let pic = match &m.picture {
        Some((n, b)) => Some(store_media(http, &cfg.url, n, b)?),
        None => None,
    };
    let snd = match &m.audio {
        Some((n, b)) => Some(store_media(http, &cfg.url, n, b)?),
        None => None,
    };
    let mut fields: Vec<(String, Json)> = Vec::new();
    if let Some(p) = &pic {
        fields.push((cfg.f_picture.clone(), Json::Str(format!("<img src=\"{}\">", p))));
    }
    if let Some(a) = &snd {
        fields.push((cfg.f_audio.clone(), Json::Str(format!("[sound:{}]", a))));
    }
    match fresh_note(http, cfg, now_ms)? {
        Some((id, have)) => {
            let yomitan_sentence = have.iter().find(|(k, _)| k == &cfg.f_sentence).map(|(_, v)| v.clone()).unwrap_or_default();
            let line = if yomitan_sentence.trim().is_empty() { html(&m.sentence_line) } else { yomitan_sentence };
            if !m.prev.is_empty() || !m.next.is_empty() || have.iter().all(|(k, v)| k != &cfg.f_sentence || v.trim().is_empty()) {
                fields.push((cfg.f_sentence.clone(), Json::Str(sentence_with_context(&m.prev, &line, &m.next))));
            }
            let obj = Json::Obj(fields);
            invoke(http, &cfg.url, "updateNoteFields", Json::obj(vec![("note", Json::obj(vec![("id", Json::Num(id as f64)), ("fields", obj)]))]))?;
            Ok("Picture, sentence audio and context added to the last Anki card.".to_string())
        }
        None => {
            fields.push((cfg.f_sentence.clone(), Json::Str(sentence_with_context(&m.prev, &html(&m.sentence_line), &m.next))));
            fields.push((cfg.f_expression.clone(), Json::Str(html(&m.sentence_line))));
            let note = Json::obj(vec![
                ("deckName", Json::Str(cfg.deck.clone())),
                ("modelName", Json::Str(cfg.model.clone())),
                ("fields", Json::Obj(fields)),
                ("tags", Json::Arr(vec![Json::Str("reelhouse".into()), Json::Str("autocards".into())])),
                ("options", Json::obj(vec![("allowDuplicate", Json::Bool(true))])),
            ]);
            invoke(http, &cfg.url, "addNote", Json::obj(vec![("note", note)]))?;
            Ok("No new Yomitan card in the last minutes: a sentence card was added to the mining deck.".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_is_escaped_and_grey() {
        let s = sentence_with_context(&["a<b".into()], "<b>語</b>", &[]);
        assert!(s.starts_with("<span style=\"color:#888;font-size:85%\">a&lt;b</span><br>"));
        assert!(s.ends_with("<b>語</b>"));
    }
}
