//! The Study mode: decks, notes and cards in the style of Anki, a spaced repetition scheduler, and the import of Anki packages (`.apkg`).
//! The collection lives in `<data>/study/collection.json` and the media of the cards in `<data>/study/media/`.
//! The scheduler follows the classic Anki one (learning steps, ease factor, intervals that grow with it).
//! Standard library only.
use crate::epub::Zip;
use crate::json::Json;
use crate::sqlite::{Db, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const NEW_PER_DAY: u32 = 20;
pub const REVIEWS_PER_DAY: u32 = 200;
const STEPS: [i64; 2] = [60, 600];
const RELEARN: i64 = 600;
const AHEAD: i64 = 1200;
const START_EASE: i64 = 2500;

#[derive(Clone, Debug, Default)]
pub struct Deck {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Tmpl {
    pub name: String,
    pub q: String,
    pub a: String,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub id: i64,
    pub name: String,
    pub cloze: bool,
    pub fields: Vec<String>,
    pub tmpls: Vec<Tmpl>,
}

#[derive(Clone, Debug, Default)]
pub struct Note {
    pub id: i64,
    pub mid: i64,
    pub fields: Vec<String>,
    pub tags: String,
}

#[derive(Clone, Debug, Default)]
pub struct Card {
    pub id: i64,
    pub nid: i64,
    pub did: i64,
    pub ord: usize,
    /// 0 new, 1 learning, 2 review, 3 relearning
    pub kind: u8,
    /// new: position; learning: unix seconds; review: day number
    pub due: i64,
    pub ivl: i64,
    pub ease: i64,
    pub reps: u32,
    pub lapses: u32,
    pub step: u8,
    pub susp: bool,
    /// hidden until this day number (buried); 0 = not buried
    pub bury: i64,
    /// colour flag 0..7 (Anki: 1 red, 2 orange, 3 green, 4 blue, 5 pink, 6 turquoise, 7 purple)
    pub flag: u8,
}

impl Card {
    /// Out of the queue today: suspended or buried.
    pub fn hidden(&self, t: i64) -> bool {
        self.susp || self.bury > t
    }
}

#[derive(Clone, Debug, Default)]
pub struct Collection {
    pub decks: Vec<Deck>,
    pub models: Vec<Model>,
    pub notes: Vec<Note>,
    pub cards: Vec<Card>,
    /// the day the counters below belong to
    pub day: i64,
    /// per deck: (new cards seen today, reviews done today)
    pub done: HashMap<i64, (u32, u32)>,
    /// per day: [day, reviews, "again" answers, new cards seen]
    pub history: Vec<[i64; 4]>,
    /// what the last answer changed, for Undo (not saved)
    pub undo: Option<Undo>,
    /// what was deleted lately, newest first, so it can come back (kept TRASH_DAYS days, at most TRASH_MAX entries)
    pub trash: Vec<Trashed>,
}

pub const TRASH_DAYS: i64 = 30;
pub const TRASH_MAX: usize = 200;

/// A deleted note (with its cards) or deck (with the decks, notes and cards under it), as it was.
#[derive(Clone, Debug, Default)]
pub struct Trashed {
    /// unix seconds of the deletion
    pub at: i64,
    /// "note" or "deck"
    pub kind: String,
    /// what the list shows: the deck name, or the front of the note
    pub label: String,
    /// the deck the note was in (by name), to put it back when that deck went away too
    pub deck: String,
    pub decks: Vec<Deck>,
    pub notes: Vec<Note>,
    pub cards: Vec<Card>,
}

#[derive(Clone, Debug)]
pub struct Undo {
    pub idx: usize,
    pub card: Card,
    pub history: Vec<[i64; 4]>,
    pub done: HashMap<i64, (u32, u32)>,
}

pub fn today(now: i64) -> i64 {
    (now - 4 * 3600).div_euclid(86400)
}

pub fn dir(data: &Path) -> PathBuf {
    data.join("study")
}
pub fn media_dir(data: &Path) -> PathBuf {
    dir(data).join("media")
}

// ---------- storage ----------

fn n(v: i64) -> Json {
    Json::Num(v as f64)
}

impl Collection {
    pub fn load(data: &Path) -> Collection {
        let mut c: Collection = std::fs::read_to_string(dir(data).join("collection.json")).ok().and_then(|t| Json::parse(&t).ok()).map(|j| Collection::from_json(&j)).unwrap_or_default();
        // the app's own cards live in "Seu Reels" since 2026-10 (saved with the next change of the collection)
        c.migrate_app_decks();
        c
    }

    pub fn save(&self, data: &Path) {
        let d = dir(data);
        let _ = std::fs::create_dir_all(&d);
        let tmp = d.join("collection.json.tmp");
        if std::fs::write(&tmp, self.to_json().dump()).is_ok() {
            let _ = std::fs::rename(&tmp, d.join("collection.json"));
        }
    }

    fn to_json(&self) -> Json {
        Json::obj(vec![
            ("decks", Json::Arr(self.decks.iter().map(|d| Json::obj(vec![("id", n(d.id)), ("name", d.name.as_str().into())])).collect())),
            (
                "models",
                Json::Arr(
                    self.models
                        .iter()
                        .map(|m| {
                            Json::obj(vec![
                                ("id", n(m.id)),
                                ("name", m.name.as_str().into()),
                                ("cloze", Json::Bool(m.cloze)),
                                ("fields", m.fields.clone().into()),
                                ("tmpls", Json::Arr(m.tmpls.iter().map(|t| Json::obj(vec![("name", t.name.as_str().into()), ("q", t.q.as_str().into()), ("a", t.a.as_str().into())])).collect())),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("notes", Json::Arr(self.notes.iter().map(|x| Json::obj(vec![("id", n(x.id)), ("mid", n(x.mid)), ("f", x.fields.clone().into()), ("t", x.tags.as_str().into())])).collect())),
            (
                "cards",
                Json::Arr(
                    self.cards
                        .iter()
                        .map(|c| Json::Arr(vec![n(c.id), n(c.nid), n(c.did), n(c.ord as i64), n(c.kind as i64), n(c.due), n(c.ivl), n(c.ease), n(c.reps as i64), n(c.lapses as i64), n(c.step as i64), n(c.susp as i64), n(c.bury), n(c.flag as i64)]))
                        .collect(),
                ),
            ),
            ("hist", Json::Arr(self.history.iter().map(|h| Json::Arr(h.iter().map(|x| n(*x)).collect())).collect())),
            ("day", n(self.day)),
            ("done", Json::Arr(self.done.iter().map(|(k, v)| Json::Arr(vec![n(*k), n(v.0 as i64), n(v.1 as i64)])).collect())),
            (
                "trash",
                Json::Arr(
                    self.trash
                        .iter()
                        .map(|t| {
                            let part = Collection { decks: t.decks.clone(), notes: t.notes.clone(), cards: t.cards.clone(), ..Collection::default() };
                            let pj = part.to_json();
                            Json::obj(vec![
                                ("at", n(t.at)),
                                ("kind", t.kind.as_str().into()),
                                ("label", t.label.as_str().into()),
                                ("deck", t.deck.as_str().into()),
                                ("decks", pj.get("decks").clone()),
                                ("notes", pj.get("notes").clone()),
                                ("cards", pj.get("cards").clone()),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    fn from_json(j: &Json) -> Collection {
        let mut c = Collection::default();
        for d in j.get("decks").items() {
            c.decks.push(Deck { id: d.get("id").as_i64().unwrap_or(0), name: d.get("name").str() });
        }
        for m in j.get("models").items() {
            c.models.push(Model {
                id: m.get("id").as_i64().unwrap_or(0),
                name: m.get("name").str(),
                cloze: m.get("cloze").as_bool().unwrap_or(false),
                fields: m.get("fields").strings(),
                tmpls: m.get("tmpls").items().iter().map(|t| Tmpl { name: t.get("name").str(), q: t.get("q").str(), a: t.get("a").str() }).collect(),
            });
        }
        for x in j.get("notes").items() {
            c.notes.push(Note { id: x.get("id").as_i64().unwrap_or(0), mid: x.get("mid").as_i64().unwrap_or(0), fields: x.get("f").strings(), tags: x.get("t").str() });
        }
        for r in j.get("cards").items() {
            let g = |i: usize| r.at(i).as_i64().unwrap_or(0);
            c.cards.push(Card { id: g(0), nid: g(1), did: g(2), ord: g(3) as usize, kind: g(4) as u8, due: g(5), ivl: g(6), ease: g(7), reps: g(8) as u32, lapses: g(9) as u32, step: g(10) as u8, susp: g(11) != 0, bury: g(12), flag: g(13) as u8 });
        }
        for r in j.get("hist").items() {
            c.history.push([r.at(0).as_i64().unwrap_or(0), r.at(1).as_i64().unwrap_or(0), r.at(2).as_i64().unwrap_or(0), r.at(3).as_i64().unwrap_or(0)]);
        }
        c.day = j.get("day").as_i64().unwrap_or(0);
        for r in j.get("done").items() {
            c.done.insert(r.at(0).as_i64().unwrap_or(0), (r.at(1).as_i64().unwrap_or(0) as u32, r.at(2).as_i64().unwrap_or(0) as u32));
        }
        for t in j.get("trash").items() {
            // the parts are stored like a collection of their own
            let part = Collection::from_json(t);
            c.trash.push(Trashed { at: t.get("at").as_i64().unwrap_or(0), kind: t.get("kind").str(), label: t.get("label").str(), deck: t.get("deck").str(), decks: part.decks, notes: part.notes, cards: part.cards });
        }
        c
    }

    /// Add what another collection (an imported package) has and this one lacks.
    pub fn merge(&mut self, o: Collection) -> (usize, usize, usize) {
        let (mut d, mut nn, mut cc) = (0, 0, 0);
        for x in o.decks {
            if !self.decks.iter().any(|y| y.id == x.id) {
                self.decks.push(x);
                d += 1;
            }
        }
        for m in o.models {
            if !self.models.iter().any(|y| y.id == m.id) {
                self.models.push(m);
            }
        }
        let known: std::collections::HashSet<i64> = self.notes.iter().map(|x| x.id).collect();
        let mut added: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for x in o.notes {
            if !known.contains(&x.id) {
                added.insert(x.id);
                self.notes.push(x);
                nn += 1;
            }
        }
        let have: std::collections::HashSet<i64> = self.cards.iter().map(|c| c.id).collect();
        for c in o.cards {
            if !have.contains(&c.id) && (added.contains(&c.nid) || self.notes.iter().any(|x| x.id == c.nid)) {
                self.cards.push(c);
                cc += 1;
            }
        }
        (d, nn, cc)
    }

    /// Take back the last answer. Returns the card index to show again.
    pub fn undo_answer(&mut self) -> Option<usize> {
        let u = self.undo.take()?;
        *self.cards.get_mut(u.idx)? = u.card;
        self.history = u.history;
        self.done = u.done;
        Some(u.idx)
    }

    /// Suspend (or bring back) a card, or all cards of its note.
    pub fn toggle_suspend(&mut self, idx: usize, whole_note: bool) {
        let Some(c) = self.cards.get(idx) else { return };
        let (nid, id, on) = (c.nid, c.id, !c.susp);
        for k in self.cards.iter_mut().filter(|k| if whole_note { k.nid == nid } else { k.id == id }) {
            k.susp = on;
        }
    }

    /// Hide a card (or the cards of its note) until tomorrow.
    pub fn bury(&mut self, idx: usize, whole_note: bool, now: i64) {
        let Some(c) = self.cards.get(idx) else { return };
        let (nid, id, t) = (c.nid, c.id, today(now));
        for k in self.cards.iter_mut().filter(|k| if whole_note { k.nid == nid } else { k.id == id }) {
            k.bury = t + 1;
        }
    }

    pub fn unbury_deck(&mut self, did: i64) {
        let ids = self.deck_ids(did);
        for k in self.cards.iter_mut().filter(|k| ids.contains(&k.did)) {
            k.bury = 0;
        }
    }

    pub fn buried_count(&self, did: i64, now: i64) -> usize {
        let (t, ids) = (today(now), self.deck_ids(did));
        self.cards.iter().filter(|k| ids.contains(&k.did) && k.bury > t).count()
    }

    pub fn set_flag(&mut self, idx: usize, flag: u8) {
        if let Some(c) = self.cards.get_mut(idx) {
            c.flag = if c.flag == flag { 0 } else { flag.min(7) };
        }
    }

    /// Toggle the "marked" tag on the note of a card.
    pub fn toggle_mark(&mut self, idx: usize) {
        let Some(nid) = self.cards.get(idx).map(|c| c.nid) else { return };
        if let Some(n) = self.notes.iter_mut().find(|n| n.id == nid) {
            let mut tags: Vec<String> = n.tags.split_whitespace().map(String::from).collect();
            if let Some(p) = tags.iter().position(|t| t.eq_ignore_ascii_case("marked")) {
                tags.remove(p);
            } else {
                tags.push("marked".into());
            }
            n.tags = tags.join(" ");
        }
    }

    /// Put a card back as new at the end of the new queue.
    pub fn reset_card(&mut self, idx: usize) {
        let pos = self.cards.iter().filter(|c| c.kind == 0).map(|c| c.due).max().unwrap_or(0) + 1;
        if let Some(c) = self.cards.get_mut(idx) {
            c.kind = 0;
            c.due = pos;
            c.ivl = 0;
            c.ease = 0;
            c.reps = 0;
            c.lapses = 0;
            c.step = 0;
        }
    }

    /// Make a card a review that is due in `days` days (0 = today).
    pub fn set_due(&mut self, idx: usize, days: i64, now: i64) {
        let t = today(now);
        if let Some(c) = self.cards.get_mut(idx) {
            if c.kind != 2 {
                c.kind = 2;
                c.ivl = c.ivl.max(1);
                if c.ease == 0 {
                    c.ease = 2500;
                }
            }
            c.due = t + days.max(0);
        }
    }

    pub fn roll_day(&mut self, now: i64) {
        let t = today(now);
        if self.day != t {
            self.day = t;
            self.done.clear();
        }
    }

    // ---------- decks ----------

    pub fn deck_name(&self, did: i64) -> String {
        self.decks.iter().find(|d| d.id == did).map(|d| d.name.clone()).unwrap_or_default()
    }

    /// The deck and the decks under it ("A" holds "A::B").
    pub fn deck_ids(&self, did: i64) -> Vec<i64> {
        let name = self.deck_name(did);
        let prefix = format!("{}::", name);
        self.decks.iter().filter(|d| d.id == did || d.name.starts_with(&prefix)).map(|d| d.id).collect()
    }

    pub fn create_deck(&mut self, name: &str) -> Option<i64> {
        let name = name.trim();
        if name.is_empty() || self.decks.iter().any(|d| d.name == name) {
            return None;
        }
        let mut id = crate::util::now_unix() as i64 * 1000;
        while self.decks.iter().any(|d| d.id == id) {
            id += 1;
        }
        self.decks.push(Deck { id, name: name.to_string() });
        Some(id)
    }

    /// A new name for a deck; the decks under it follow ("A::B" becomes "C::B"). False when the name is empty or taken.
    pub fn rename_deck(&mut self, did: i64, name: &str) -> bool {
        let name = name.trim().trim_matches(':').trim();
        let old = self.deck_name(did);
        if name.is_empty() || old.is_empty() || name == old || self.decks.iter().any(|d| d.name == name) || name.starts_with(&format!("{}::", old)) {
            return false;
        }
        let prefix = format!("{}::", old);
        for d in self.decks.iter_mut() {
            if d.id == did {
                d.name = name.to_string();
            } else if let Some(rest) = d.name.strip_prefix(&prefix) {
                d.name = format!("{}::{}", name, rest);
            }
        }
        true
    }

    /// Delete a deck and the decks under it; it goes to the trash with its cards and notes.
    pub fn delete_deck(&mut self, did: i64, now: i64) {
        let ids = self.deck_ids(did);
        if ids.is_empty() {
            return;
        }
        let label = self.deck_name(did);
        let (gone, kept): (Vec<Card>, Vec<Card>) = std::mem::take(&mut self.cards).into_iter().partition(|c| ids.contains(&c.did));
        self.cards = kept;
        let (decks, keep_decks): (Vec<Deck>, Vec<Deck>) = std::mem::take(&mut self.decks).into_iter().partition(|d| ids.contains(&d.id));
        self.decks = keep_decks;
        let live: std::collections::HashSet<i64> = self.cards.iter().map(|c| c.nid).collect();
        let (keep_notes, notes): (Vec<Note>, Vec<Note>) = std::mem::take(&mut self.notes).into_iter().partition(|x| live.contains(&x.id));
        self.notes = keep_notes;
        self.undo = None;
        self.to_trash(Trashed { at: now, kind: "deck".into(), label, deck: String::new(), decks, notes, cards: gone }, now);
    }

    /// (new, learning, review) to do in a deck (and the decks under it) now.
    pub fn counts(&self, did: i64, now: i64) -> (u32, u32, u32) {
        let t = today(now);
        let ids = self.deck_ids(did);
        let (mut nw, mut ln, mut rv) = (0u32, 0u32, 0u32);
        let mut per: HashMap<i64, (u32, u32)> = HashMap::new();
        for c in self.cards.iter().filter(|c| !c.hidden(t) && ids.contains(&c.did)) {
            let e = per.entry(c.did).or_default();
            match c.kind {
                0 => e.0 += 1,
                1 | 3 => {
                    if c.due <= now + AHEAD {
                        ln += 1
                    }
                }
                _ => {
                    if c.due <= t {
                        e.1 += 1
                    }
                }
            }
        }
        for (d, (a, b)) in per {
            let done = self.done.get(&d).copied().unwrap_or((0, 0));
            nw += a.min(NEW_PER_DAY.saturating_sub(done.0));
            rv += b.min(REVIEWS_PER_DAY.saturating_sub(done.1));
        }
        (nw, ln, rv)
    }

    pub fn total_cards(&self, did: i64) -> usize {
        let ids = self.deck_ids(did);
        self.cards.iter().filter(|c| ids.contains(&c.did)).count()
    }

    /// `counts` and `total_cards` of every deck at once, in one pass over the cards: deck id -> (new, learning,
    /// review, total), each deck holding the decks under it. Calling `counts` + `total_cards` per deck walks all the
    /// cards twice per deck (and once more per deck above it), which froze the Study page for seconds with a big
    /// AnkiWeb collection (hundreds of decks x tens of thousands of cards, on the GUI thread).
    pub fn overview(&self, now: i64) -> HashMap<i64, (u32, u32, u32, usize)> {
        let t = today(now);
        // per deck, its own cards only: (new, learning, review due, total)
        let mut own: HashMap<i64, (u32, u32, u32, usize)> = HashMap::with_capacity(self.decks.len());
        for c in &self.cards {
            let e = own.entry(c.did).or_default();
            e.3 += 1;
            if c.hidden(t) {
                continue;
            }
            match c.kind {
                0 => e.0 += 1,
                1 | 3 => {
                    if c.due <= now + AHEAD {
                        e.1 += 1
                    }
                }
                _ => {
                    if c.due <= t {
                        e.2 += 1
                    }
                }
            }
        }
        // the daily limits apply per deck (as in `counts`), then each deck adds up into itself and every deck above it
        let mut by_name: HashMap<&str, Vec<i64>> = HashMap::with_capacity(self.decks.len());
        for d in &self.decks {
            by_name.entry(d.name.as_str()).or_default().push(d.id);
        }
        let mut out: HashMap<i64, (u32, u32, u32, usize)> = self.decks.iter().map(|d| (d.id, (0, 0, 0, 0))).collect();
        let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for d in &self.decks {
            // a deck id listed twice counts once (as `deck_ids` + `contains` did)
            if !seen.insert(d.id) {
                continue;
            }
            let Some(&(nw, ln, rv, total)) = own.get(&d.id) else { continue };
            let done = self.done.get(&d.id).copied().unwrap_or((0, 0));
            let add = (nw.min(NEW_PER_DAY.saturating_sub(done.0)), ln, rv.min(REVIEWS_PER_DAY.saturating_sub(done.1)), total);
            let mut targets: Vec<i64> = vec![d.id];
            for (i, _) in d.name.match_indices("::") {
                if let Some(ids) = by_name.get(&d.name[..i]) {
                    targets.extend(ids.iter().copied().filter(|x| *x != d.id));
                }
            }
            targets.sort_unstable();
            targets.dedup();
            for id in targets {
                if let Some(o) = out.get_mut(&id) {
                    o.0 += add.0;
                    o.1 += add.1;
                    o.2 += add.2;
                    o.3 += add.3;
                }
            }
        }
        out
    }

    /// Reviews and "again" answers today, over all decks (the history is not kept per deck).
    pub fn today_reviews(&self, now: i64) -> (i64, i64) {
        let t = today(now);
        self.history.iter().find(|h| h[0] == t).map(|h| (h[1], h[2])).unwrap_or((0, 0))
    }

    // ---------- the queue ----------

    /// The index of the next card to study in a deck, if any.
    pub fn next_card(&self, did: i64, now: i64) -> Option<usize> {
        let t = today(now);
        let ids = self.deck_ids(did);
        let mine = |c: &&(usize, &Card)| !c.1.hidden(t) && ids.contains(&c.1.did);
        let all: Vec<(usize, &Card)> = self.cards.iter().enumerate().collect();
        // learning cards that are due
        if let Some((i, _)) = all.iter().filter(mine).filter(|(_, c)| (c.kind == 1 || c.kind == 3) && c.due <= now).min_by_key(|(_, c)| c.due) {
            return Some(*i);
        }
        // reviews due today, within the daily limit of their deck
        let limit_ok = |c: &Card| self.done.get(&c.did).map(|d| d.1).unwrap_or(0) < REVIEWS_PER_DAY;
        if let Some((i, _)) = all.iter().filter(mine).filter(|(_, c)| c.kind == 2 && c.due <= t && limit_ok(c)).min_by_key(|(_, c)| (c.due, c.id)) {
            return Some(*i);
        }
        let new_ok = |c: &Card| self.done.get(&c.did).map(|d| d.0).unwrap_or(0) < NEW_PER_DAY;
        if let Some((i, _)) = all.iter().filter(mine).filter(|(_, c)| c.kind == 0 && new_ok(c)).min_by_key(|(_, c)| (c.due, c.id)) {
            return Some(*i);
        }
        // learning cards a little ahead of time, so the session does not end with a card a few minutes away
        all.iter().filter(mine).filter(|(_, c)| (c.kind == 1 || c.kind == 3) && c.due <= now + AHEAD).min_by_key(|(_, c)| c.due).map(|(i, _)| *i)
    }

    /// Answer a card: 1 again, 2 hard, 3 good, 4 easy.
    pub fn answer(&mut self, idx: usize, ease: u8, now: i64) {
        self.roll_day(now);
        let t = today(now);
        let Some(c) = self.cards.get_mut(idx) else { return };
        let was_new = c.kind == 0;
        let did = c.did;
        let before = c.clone();
        schedule(c, ease, now, t);
        self.undo = Some(Undo { idx, card: before, history: self.history.clone(), done: self.done.clone() });
        match self.history.iter_mut().find(|h| h[0] == t) {
            Some(h) => {
                h[1] += 1;
                h[2] += (ease == 1) as i64;
                h[3] += was_new as i64;
            }
            None => self.history.push([t, 1, (ease == 1) as i64, was_new as i64]),
        }
        let e = self.done.entry(did).or_default();
        if was_new {
            e.0 += 1;
        } else if self.cards[idx].kind == 2 || self.cards[idx].kind == 3 || self.cards[idx].kind == 1 {
            // learning steps do not count against the daily review limit; graduated reviews do
            if self.cards[idx].kind == 2 {
                e.1 += 1;
            }
        }
    }
}

pub const BASIC: i64 = 1_000_000_001;
pub const REVERSED: i64 = 1_000_000_002;
pub const CLOZE: i64 = 1_000_000_003;
pub const CLIP: i64 = 1_000_000_004;

/// The deck of the cards the app makes by itself (see `Collection::app_deck`).
pub const APP_DECK: &str = "Seu Reels";
/// The app's decks before "Seu Reels" (merged into it on load).
const OLD_APP_DECKS: &[&str] = &["Reelhouse::Clips", "Reelhouse::Pictures"];

fn tm(name: &str, q: &str, a: &str) -> Tmpl {
    Tmpl { name: name.into(), q: q.into(), a: a.into() }
}

/// The numbers of the cloze deletions in a text, in order, without repeats.
pub fn cloze_numbers(text: &str) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut rest = text;
    while let Some(p) = rest.find("{{c") {
        let after = &rest[p + 3..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && after[digits.len()..].starts_with("::") {
            let k: usize = digits.parse().unwrap_or(0);
            if k > 0 && !out.contains(&k) {
                out.push(k);
            }
        }
        rest = after;
    }
    out.sort();
    out
}

/// What the stats page shows.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub today: i64,
    pub today_again: i64,
    /// reviews per day for the last 30 days (oldest first)
    pub last30: Vec<i64>,
    pub retention: f64,
    pub new: usize,
    pub learning: usize,
    pub young: usize,
    pub mature: usize,
    pub suspended: usize,
    /// cards due on each of the next 14 days (today first)
    pub forecast: Vec<usize>,
    pub total_reviews: i64,
}

impl Collection {
    /// The note types every collection has.
    pub fn ensure_builtin(&mut self) {
        let mk = |id, name: &str, cloze, fields: &[&str], tmpls: Vec<Tmpl>| Model { id, name: name.into(), cloze, fields: fields.iter().map(|s| s.to_string()).collect(), tmpls };
        if !self.models.iter().any(|m| m.id == BASIC) {
            self.models.push(mk(BASIC, "Basic", false, &["Front", "Back"], vec![tm("Card 1", "{{Front}}", "{{FrontSide}}<hr id=answer>{{Back}}")]));
        }
        if !self.models.iter().any(|m| m.id == REVERSED) {
            self.models.push(mk(REVERSED, "Basic (and reversed card)", false, &["Front", "Back"], vec![tm("Card 1", "{{Front}}", "{{FrontSide}}<hr id=answer>{{Back}}"), tm("Card 2", "{{Back}}", "{{FrontSide}}<hr id=answer>{{Front}}")]));
        }
        if !self.models.iter().any(|m| m.id == CLOZE) {
            self.models.push(mk(CLOZE, "Cloze", true, &["Text", "Extra"], vec![tm("Cloze", "{{cloze:Text}}", "{{cloze:Text}}<br>{{Extra}}")]));
        }
        if !self.models.iter().any(|m| m.id == CLIP) {
            self.models.push(mk(
                CLIP,
                "Reelhouse clip",
                false,
                &["Sentence", "Hiragana", "Katakana", "Kanji", "Meaning", "Description", "Clip", "Image", "Source"],
                vec![tm(
                    "Clip",
                    "{{Image}}<br>{{Clip}}<br><big>{{Sentence}}</big>",
                    "{{FrontSide}}<hr id=answer>{{#Hiragana}}<b>ひらがな</b><br>{{Hiragana}}<br><br>{{/Hiragana}}{{#Katakana}}<b>カタカナ</b><br>{{Katakana}}<br><br>{{/Katakana}}{{#Meaning}}{{Meaning}}<br><br>{{/Meaning}}{{#Kanji}}{{Kanji}}<br><br>{{/Kanji}}{{#Description}}<small>{{Description}}</small><br>{{/Description}}<small>{{Source}}</small>",
                )],
            ));
        }
    }

    /// Adds a clip card to the app's deck "Seu Reels" (made when missing).
    pub fn add_clip(&mut self, fields: Vec<String>, now: i64) -> Result<i64, String> {
        self.ensure_builtin();
        let did = self.app_deck().ok_or("could not make the deck")?;
        self.add_note(did, CLIP, fields, now)?;
        Ok(did)
    }

    /// The deck of every card the app makes by itself (player clips, mined words, Images and Text mode cards):
    /// "Seu Reels", the same name in every language. Made when missing.
    pub fn app_deck(&mut self) -> Option<i64> {
        match self.decks.iter().find(|d| d.name == APP_DECK) {
            Some(d) => Some(d.id),
            None => self.create_deck(APP_DECK),
        }
    }

    /// The app's older decks ("Reelhouse::Clips", "Reelhouse::Pictures") become "Seu Reels": the first one found is
    /// renamed (it keeps its id), the others have their cards moved into it and are removed; an empty "Reelhouse"
    /// parent left behind goes too. Nothing happens once they are gone, so it can run on every load. True when
    /// something changed.
    pub fn migrate_app_decks(&mut self) -> bool {
        let mut changed = false;
        for old in OLD_APP_DECKS {
            let Some(did) = self.decks.iter().find(|d| d.name == *old).map(|d| d.id) else { continue };
            changed = true;
            if !self.decks.iter().any(|d| d.name == APP_DECK) && self.rename_deck(did, APP_DECK) {
                continue;
            }
            let Some(target) = self.app_deck() else { continue };
            let ids = self.deck_ids(did);
            for c in self.cards.iter_mut().filter(|c| ids.contains(&c.did)) {
                c.did = target;
            }
            self.decks.retain(|d| !ids.contains(&d.id));
        }
        if changed {
            if let Some(p) = self.decks.iter().find(|d| d.name == "Reelhouse").map(|d| d.id) {
                if self.deck_ids(p).len() == 1 && !self.cards.iter().any(|c| c.did == p) {
                    self.decks.retain(|d| d.id != p);
                }
            }
        }
        changed
    }

    fn next_id(&self, now: i64) -> i64 {
        let mut id = now * 1000;
        let used = |i: i64, c: &Collection| c.notes.iter().any(|x| x.id == i) || c.cards.iter().any(|x| x.id == i);
        while used(id, self) {
            id += 1;
        }
        id
    }

    fn new_position(&self) -> i64 {
        self.cards.iter().filter(|c| c.kind == 0).map(|c| c.due).max().unwrap_or(0) + 1
    }

    fn make_cards(&mut self, nid: i64, did: i64, ords: Vec<usize>, now: i64) {
        let mut pos = self.new_position();
        for o in ords {
            let id = self.next_id(now);
            self.cards.push(Card { id, nid, did, ord: o, kind: 0, due: pos, ..Card::default() });
            pos += 1;
        }
    }

    pub fn add_note(&mut self, did: i64, mid: i64, mut fields: Vec<String>, now: i64) -> Result<usize, String> {
        self.ensure_builtin();
        let m = self.models.iter().find(|m| m.id == mid).cloned().ok_or("Unknown note type.")?;
        if !self.decks.iter().any(|d| d.id == did) {
            return Err("Choose a deck.".into());
        }
        fields.resize(m.fields.len(), String::new());
        if fields.first().map(|f| f.trim().is_empty()).unwrap_or(true) {
            return Err("The first field is empty.".into());
        }
        let ords: Vec<usize> = if m.cloze {
            let nums = cloze_numbers(&fields[0]);
            if nums.is_empty() {
                return Err("Add a cloze like {{c1::answer}} to the text.".into());
            }
            nums.into_iter().map(|k| k - 1).collect()
        } else {
            (0..m.tmpls.len()).collect()
        };
        let nid = self.next_id(now);
        self.notes.push(Note { id: nid, mid, fields, tags: String::new() });
        let n = ords.len();
        self.make_cards(nid, did, ords, now);
        Ok(n)
    }

    pub fn edit_note(&mut self, nid: i64, fields: Vec<String>, now: i64) -> Result<(), String> {
        let Some(idx) = self.notes.iter().position(|x| x.id == nid) else { return Err("That note is gone.".into()) };
        let mid = self.notes[idx].mid;
        let cloze_model = self.models.iter().find(|m| m.id == mid).map(|m| m.cloze).unwrap_or(false);
        let want = self.models.iter().find(|m| m.id == mid).map(|m| m.fields.len()).unwrap_or(fields.len());
        let mut fields = fields;
        fields.resize(want, String::new());
        if fields.first().map(|f| f.trim().is_empty()).unwrap_or(true) {
            return Err("The first field is empty.".into());
        }
        if cloze_model {
            let did = self.cards.iter().find(|c| c.nid == nid).map(|c| c.did).unwrap_or(0);
            let have: Vec<usize> = self.cards.iter().filter(|c| c.nid == nid).map(|c| c.ord).collect();
            let missing: Vec<usize> = cloze_numbers(&fields[0]).into_iter().map(|k| k - 1).filter(|o| !have.contains(o)).collect();
            if did != 0 && !missing.is_empty() {
                self.make_cards(nid, did, missing, now);
            }
        }
        self.notes[idx].fields = fields;
        Ok(())
    }

    /// Delete a note and its cards; they go to the trash.
    pub fn delete_note(&mut self, nid: i64, now: i64) {
        let (cards, kept): (Vec<Card>, Vec<Card>) = std::mem::take(&mut self.cards).into_iter().partition(|c| c.nid == nid);
        self.cards = kept;
        let (notes, kept): (Vec<Note>, Vec<Note>) = std::mem::take(&mut self.notes).into_iter().partition(|x| x.id == nid);
        self.notes = kept;
        if cards.is_empty() && notes.is_empty() {
            return;
        }
        self.undo = None;
        let front = notes.first().and_then(|x| x.fields.first()).map(|f| strip_tags(f)).unwrap_or_default();
        let mut label: String = front.split_whitespace().collect::<Vec<_>>().join(" ");
        if label.chars().count() > 80 {
            label = label.chars().take(79).collect::<String>() + "…";
        }
        let deck = cards.first().map(|c| self.deck_name(c.did)).unwrap_or_default();
        self.to_trash(Trashed { at: now, kind: "note".into(), label, deck, decks: Vec::new(), notes, cards }, now);
    }

    /// Delete a note and its cards for good, without the trash (repeated cards the app removes by itself).
    pub fn purge_note(&mut self, nid: i64) {
        self.cards.retain(|c| c.nid != nid);
        self.notes.retain(|x| x.id != nid);
        self.undo = None;
    }

    fn to_trash(&mut self, t: Trashed, now: i64) {
        self.trash.insert(0, t);
        self.trash.retain(|t| now - t.at < TRASH_DAYS * 86400);
        self.trash.truncate(TRASH_MAX);
    }

    /// Put an entry of the trash back. A deck whose name is taken now comes back as "Name (restored)"; the cards of a
    /// note whose deck is gone go to a deck of that name again (made when needed). Returns what came back.
    pub fn restore(&mut self, i: usize) -> Option<String> {
        if i >= self.trash.len() {
            return None;
        }
        let mut t = self.trash.remove(i);
        if !t.decks.is_empty() {
            // the top deck is the one with the shortest name; the others are under it
            let top = t.decks.iter().map(|d| d.name.clone()).min_by_key(|n| n.len()).unwrap_or_default();
            if t.decks.iter().any(|d| self.decks.iter().any(|y| y.name == d.name)) {
                let mut k = 1;
                let fresh = loop {
                    let cand = if k == 1 { format!("{} (restored)", top) } else { format!("{} (restored {})", top, k) };
                    let pre = format!("{}::", cand);
                    if !self.decks.iter().any(|y| y.name == cand || y.name.starts_with(&pre)) {
                        break cand;
                    }
                    k += 1;
                };
                let pre = format!("{}::", top);
                for d in t.decks.iter_mut() {
                    if d.name == top {
                        d.name = fresh.clone();
                    } else if let Some(rest) = d.name.strip_prefix(&pre) {
                        d.name = format!("{}::{}", fresh, rest);
                    }
                }
            }
            // a deck id taken meanwhile gets a new one, and its cards follow it (they would land in the other deck)
            let mut moved: HashMap<i64, i64> = HashMap::new();
            for k in 0..t.decks.len() {
                let old = t.decks[k].id;
                let mut id = old;
                while self.decks.iter().any(|y| y.id == id) || t.decks.iter().enumerate().any(|(j, y)| j != k && y.id == id) {
                    id += 1;
                }
                if id != old {
                    t.decks[k].id = id;
                    moved.insert(old, id);
                }
            }
            for c in t.cards.iter_mut() {
                if let Some(id) = moved.get(&c.did) {
                    c.did = *id;
                }
            }
        }
        // cards whose deck no longer exists
        let have: std::collections::HashSet<i64> = self.decks.iter().map(|d| d.id).chain(t.decks.iter().map(|d| d.id)).collect();
        if t.cards.iter().any(|c| !have.contains(&c.did)) {
            let name = if t.deck.is_empty() { "Default".to_string() } else { t.deck.clone() };
            let did = match self.decks.iter().find(|d| d.name == name) {
                Some(d) => d.id,
                None => self.create_deck(&name).unwrap_or(0),
            };
            for c in t.cards.iter_mut().filter(|c| !have.contains(&c.did)) {
                c.did = did;
            }
        }
        let label = t.label.clone();
        self.decks.extend(t.decks);
        let known: std::collections::HashSet<i64> = self.notes.iter().map(|x| x.id).collect();
        self.notes.extend(t.notes.into_iter().filter(|x| !known.contains(&x.id)));
        let known: std::collections::HashSet<i64> = self.cards.iter().map(|c| c.id).collect();
        self.cards.extend(t.cards.into_iter().filter(|c| !known.contains(&c.id)));
        Some(label)
    }

    /// Cards of a deck (and its sub decks) whose text has every word of the query, as (card index).
    pub fn browse(&self, did: i64, query: &str, limit: usize) -> Vec<usize> {
        let ids: std::collections::HashSet<i64> = if did == 0 { self.decks.iter().map(|d| d.id).collect() } else { self.deck_ids(did).into_iter().collect() };
        let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
        // note id -> note, built once (a search used to look each card's note up in the whole list)
        let notes: HashMap<i64, &Note> = if words.is_empty() { HashMap::new() } else { self.notes.iter().map(|x| (x.id, x)).collect() };
        let mut out = Vec::new();
        for (i, c) in self.cards.iter().enumerate() {
            if !ids.contains(&c.did) {
                continue;
            }
            if !words.is_empty() {
                let Some(note) = notes.get(&c.nid) else { continue };
                let text = strip_tags(&note.fields.join(" ")).to_lowercase();
                if !words.iter().all(|w| text.contains(w.as_str())) {
                    continue;
                }
            }
            out.push(i);
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    pub fn stats(&self, did: i64, now: i64) -> Stats {
        let t = today(now);
        let mut s = Stats { today: 0, ..Stats::default() };
        let (mut total, mut again) = (0i64, 0i64);
        for h in &self.history {
            if h[0] == t {
                s.today = h[1];
                s.today_again = h[2];
            }
            if h[0] > t - 30 {
                total += h[1];
                again += h[2];
            }
            s.total_reviews += h[1];
        }
        s.last30 = (0..30).map(|k| self.history.iter().find(|h| h[0] == t - 29 + k).map(|h| h[1]).unwrap_or(0)).collect();
        s.retention = if total > 0 { 1.0 - again as f64 / total as f64 } else { 0.0 };
        s.forecast = vec![0; 14];
        let ids: std::collections::HashSet<i64> = if did == 0 { self.decks.iter().map(|d| d.id).collect() } else { self.deck_ids(did).into_iter().collect() };
        for c in self.cards.iter().filter(|c| ids.contains(&c.did)) {
            if c.susp {
                s.suspended += 1;
                continue;
            }
            match c.kind {
                0 => s.new += 1,
                1 | 3 => s.learning += 1,
                _ => {
                    if c.ivl >= 21 {
                        s.mature += 1
                    } else {
                        s.young += 1
                    }
                    let d = (c.due - t).max(0) as usize;
                    if d < 14 {
                        s.forecast[d] += 1;
                    }
                }
            }
        }
        s
    }
}

/// The scheduler.
pub fn schedule(c: &mut Card, ease: u8, now: i64, today: i64) {
    c.reps += 1;
    let graduate = |c: &mut Card, ivl: i64| {
        c.kind = 2;
        c.step = 0;
        c.ivl = ivl.clamp(1, 36500);
        c.due = today + c.ivl;
        if c.ease == 0 {
            c.ease = START_EASE;
        }
    };
    match c.kind {
        0 | 1 => {
            let step = if c.kind == 1 { c.step as usize } else { 0 };
            match ease {
                1 => {
                    c.kind = 1;
                    c.step = 0;
                    c.due = now + STEPS[0];
                }
                2 => {
                    c.kind = 1;
                    c.step = step as u8;
                    c.due = now + if step == 0 { (STEPS[0] + STEPS[1]) / 2 } else { STEPS[step.min(STEPS.len() - 1)] };
                }
                3 => {
                    if step + 1 < STEPS.len() {
                        c.kind = 1;
                        c.step = (step + 1) as u8;
                        c.due = now + STEPS[step + 1];
                    } else {
                        graduate(c, 1);
                    }
                }
                _ => graduate(c, 4),
            }
        }
        3 => match ease {
            1 | 2 => {
                c.due = now + RELEARN;
            }
            3 => {
                let i = c.ivl.max(1);
                graduate(c, i)
            }
            _ => {
                let i = c.ivl.max(1) + 1;
                graduate(c, i)
            }
        },
        _ => {
            let ivl = c.ivl.max(1);
            let f = c.ease.max(1300);
            match ease {
                1 => {
                    c.lapses += 1;
                    c.ease = (f - 200).max(1300);
                    c.kind = 3;
                    c.step = 0;
                    c.ivl = 1;
                    c.due = now + RELEARN;
                }
                2 => {
                    c.ease = (f - 150).max(1300);
                    let i = ((ivl as f64 * 1.2).round() as i64).max(ivl + 1);
                    graduate(c, i);
                }
                3 => {
                    let i = ((ivl as f64 * f as f64 / 1000.0).round() as i64).max(ivl + 1);
                    graduate(c, i);
                }
                _ => {
                    c.ease = f + 150;
                    let i = ((ivl as f64 * f as f64 / 1000.0 * 1.3).round() as i64).max(ivl + 2);
                    graduate(c, i);
                }
            }
        }
    }
}

/// How long until the card comes back for each answer, as the buttons show it.
pub fn labels(c: &Card, now: i64) -> Vec<String> {
    let t = today(now);
    (1..=4u8)
        .map(|e| {
            let mut x = c.clone();
            schedule(&mut x, e, now, t);
            let secs = if x.kind == 2 { x.ivl * 86400 } else { x.due - now };
            span(secs)
        })
        .collect()
}

pub fn span(secs: i64) -> String {
    if secs < 60 {
        "<1m".into()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else if secs < 86400 * 30 {
        format!("{}d", secs / 86400)
    } else if secs < 86400 * 365 {
        format!("{:.1}mo", secs as f64 / 86400.0 / 30.0)
    } else {
        format!("{:.1}y", secs as f64 / 86400.0 / 365.0)
    }
}

// ---------- the text of a card ----------

/// A word of a Study deck that can show up in a subtitle.
#[derive(Clone, Debug)]
pub struct Hint {
    pub nid: i64,
    pub term: String,
    pub reading: String,
    pub meaning: String,
}

fn has_kanji(s: &str) -> bool {
    s.chars().any(crate::kanji::is_kanji)
}

fn short(s: &str, n: usize) -> String {
    let one = s.lines().next().unwrap_or("").trim().to_string();
    let mut o: String = one.chars().take(n).collect();
    if one.chars().count() > n {
        o.push('…');
    }
    o
}

impl Collection {
    /// Every word with kanji of the decks, longest first, to be found in a subtitle. Notes from the player's Editor give
    /// their words with reading and meaning; other notes give their first field (when short) and the second as meaning.
    pub fn hint_index(&self) -> Vec<Hint> {
        let mut out: Vec<Hint> = vec![];
        for n in &self.notes {
            let Some(m) = self.models.iter().find(|m| m.id == n.mid) else { continue };
            // the clip cards, older and video ones (controller_clip::CLIP_VIDEO), read their fields by name
            if m.id == CLIP || m.id == CLIP + 1 {
                let get = |name: &str| m.fields.iter().position(|f| f == name).and_then(|i| n.fields.get(i)).cloned().unwrap_or_default();
                let meaning = get("Meaning");
                for line in get("Hiragana").split("<br>") {
                    let line = strip_tags(line);
                    let Some((w, r)) = line.split_once('【') else { continue };
                    let (w, r) = (w.trim().to_string(), r.trim_end_matches('】').trim().to_string());
                    if w.is_empty() || !has_kanji(&w) {
                        continue;
                    }
                    let mean = meaning.split("<br>").map(strip_tags).find(|l| l.starts_with(&format!("{}:", w))).map(|l| l[w.len() + 1..].trim().to_string()).unwrap_or_default();
                    out.push(Hint { nid: n.id, term: w, reading: r, meaning: short(&mean, 90) });
                }
            } else if !m.cloze {
                let front = strip_tags(n.fields.first().map(|s| s.as_str()).unwrap_or("")).trim().to_string();
                if front.is_empty() || front.chars().count() > 16 || !has_kanji(&front) {
                    continue;
                }
                let reading = m.fields.iter().enumerate().find(|(_, f)| ["Reading", "Kana", "Furigana", "Hiragana"].contains(&f.as_str())).and_then(|(i, _)| n.fields.get(i)).map(|s| strip_tags(s)).unwrap_or_default();
                let meaning = strip_tags(n.fields.get(1).map(|s| s.as_str()).unwrap_or("")).trim().to_string();
                out.push(Hint { nid: n.id, term: front, reading: short(&reading, 30), meaning: short(&meaning, 90) });
            }
        }
        out.sort_by(|a, b| b.term.chars().count().cmp(&a.term.chars().count()));
        out
    }
}

/// The text of a field without its HTML tags.
pub fn strip_html(s: &str) -> String {
    strip_tags(s)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for ch in s.chars() {
        match ch {
            '<' => inside = true,
            '>' if inside => inside = false,
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
}

/// Cloze deletions `{{c1::text::hint}}` of a field: `ord` (1-based) is the one asked.
pub fn cloze(text: &str, ord: usize, answer: bool) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(p) = rest.find("{{c") {
        out.push_str(&rest[..p]);
        let after = &rest[p + 3..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        let tail = &after[digits.len()..];
        if digits.is_empty() || !tail.starts_with("::") {
            out.push_str("{{c");
            rest = after;
            continue;
        }
        let Some(end) = tail.find("}}") else {
            out.push_str(&rest[p..]);
            rest = "";
            break;
        };
        let body = &tail[2..end];
        let (txt, hint) = match body.split_once("::") {
            Some((a, b)) => (a, Some(b)),
            None => (body, None),
        };
        if digits.parse::<usize>().unwrap_or(0) == ord {
            if answer {
                out.push_str(&format!("<b><font color=\"#3b82f6\">{}</font></b>", txt));
            } else {
                out.push_str(&format!("<b><font color=\"#3b82f6\">[{}]</font></b>", hint.unwrap_or("...")));
            }
        } else {
            out.push_str(txt);
        }
        rest = &tail[end + 2..];
    }
    out.push_str(rest);
    out
}

struct Ctx<'a> {
    names: &'a [String],
    values: &'a [String],
    tags: &'a str,
    deck: &'a str,
    front: &'a str,
    ord: usize,
    answer: bool,
}

fn field<'a>(cx: &Ctx<'a>, name: &str) -> Option<&'a str> {
    cx.names.iter().position(|n| n == name).and_then(|i| cx.values.get(i)).map(|s| s.as_str())
}

fn fill(tpl: &str, cx: &Ctx) -> String {
    let mut out = String::new();
    let mut rest = tpl;
    while let Some(p) = rest.find("{{") {
        out.push_str(&rest[..p]);
        let Some(q) = rest[p + 2..].find("}}") else {
            out.push_str(&rest[p..]);
            return out;
        };
        let tag = rest[p + 2..p + 2 + q].trim().to_string();
        let after = &rest[p + 2 + q + 2..];
        if let Some(name) = tag.strip_prefix('#').or_else(|| tag.strip_prefix('^')) {
            let neg = tag.starts_with('^');
            let close = format!("{{{{/{}}}}}", name);
            let (inner, next) = match after.find(&close) {
                Some(e) => (&after[..e], &after[e + close.len()..]),
                None => (after, ""),
            };
            let has = field(cx, name).map(|v| !strip_tags(v).trim().is_empty()).unwrap_or(false);
            if has != neg {
                out.push_str(&fill(inner, cx));
            }
            rest = next;
            continue;
        }
        if tag.starts_with('/') {
            rest = after;
            continue;
        }
        let (filters, name) = match tag.rsplit_once(':') {
            Some((f, nm)) => (f.to_string(), nm.trim().to_string()),
            None => (String::new(), tag.clone()),
        };
        let text = match name.as_str() {
            "FrontSide" => cx.front.to_string(),
            "Tags" => cx.tags.to_string(),
            "Deck" => cx.deck.to_string(),
            "Subdeck" => cx.deck.rsplit("::").next().unwrap_or("").to_string(),
            "Card" => String::new(),
            _ => {
                let raw = field(cx, &name).unwrap_or("").to_string();
                if filters.split(':').any(|f| f == "cloze") {
                    cloze(&raw, cx.ord, cx.answer)
                } else if filters.split(':').any(|f| f == "type") {
                    String::new()
                } else if filters.split(':').any(|f| f == "text") {
                    strip_tags(&raw)
                } else if let Some(f) = filters.split(':').find(|f| ["furigana", "kana", "kanji"].contains(f)) {
                    // Anki's 漢字[かんじ] filters; the review shows the kanji and its reading on hover (Qt has no <ruby>)
                    furigana(&raw, f == "kana")
                } else {
                    raw
                }
            }
        };
        out.push_str(&text);
        rest = after;
    }
    out.push_str(rest);
    out
}

fn furi_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r" ?([^ >\[\]]+?)\[([^\]]+)\]").unwrap())
}

fn ruby_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?is)<ruby>(?:<rb>)?([^<]*)(?:</rb>)?(?:<rp>[^<]*</rp>)?<rt>([^<]*)</rt>(?:<rp>[^<]*</rp>)?</ruby>").unwrap())
}

/// Anki's furigana syntax (`漢字[かんじ]`, a space before the word ends the text before it): the kanji, or the kana.
pub fn furigana(text: &str, kana: bool) -> String {
    furi_re().replace_all(text, if kana { "$2" } else { "$1" }).into_owned()
}

/// The readings a card knows, for the hiragana / katakana box of the review (KanaHover.js): the furigana of its fields
/// (`漢字[かんじ]`, `<ruby>`), a short word field paired with the kana field after it (Word / Word Reading), and a kanji
/// with okurigana read from the kanji table (一つ = ひとつ). `kanji` gives each kanji its meaning and on / kun readings.
#[allow(clippy::type_complexity)]
pub fn card_readings(fields: &[String], text: &str) -> (Vec<(String, String)>, Vec<(char, String)>) {
    let mut words: Vec<(String, String)> = Vec::new();
    let add = |w: &str, r: &str, words: &mut Vec<(String, String)>| {
        let w = w.trim();
        let start = w.char_indices().find(|(_, c)| crate::kanji::is_kanji(*c)).map(|(i, _)| i);
        let Some(start) = start else { return };
        let w = &w[start..];
        let r = crate::kanji::to_hiragana(r.split([',', '|', ';']).next().unwrap_or("").trim());
        if r.is_empty() || !r.chars().all(is_kana) || words.iter().any(|(x, _)| x == w) {
            return;
        }
        words.push((w.to_string(), r));
    };
    for f in fields {
        let t = strip_tags(f);
        // 勉[べん] 強[きょう]: kanji written one by one also make the whole word (勉強 = べんきょう)
        let (mut chain, mut chain_r, mut end) = (String::new(), String::new(), usize::MAX);
        for c in furi_re().captures_iter(&t) {
            let m = c.get(0).unwrap();
            add(&c[1], &c[2], &mut words);
            let all_kanji = c[1].chars().all(crate::kanji::is_kanji);
            if m.start() != end || !all_kanji {
                if chain.chars().count() > 1 {
                    add(&chain, &chain_r, &mut words);
                }
                chain.clear();
                chain_r.clear();
            }
            if all_kanji {
                chain.push_str(&c[1]);
                chain_r.push_str(&c[2]);
            }
            end = m.end();
        }
        if chain.chars().count() > 1 {
            add(&chain, &chain_r, &mut words);
        }
        for c in ruby_re().captures_iter(f) {
            add(&strip_tags(&c[1]), &strip_tags(&c[2]), &mut words);
        }
    }
    let plain: Vec<String> = fields.iter().map(|f| strip_tags(f).trim().to_string()).collect();
    if let Some(i) = plain.iter().position(|w| has_kanji(w) && w.chars().count() <= 12 && !w.contains(char::is_whitespace) && !w.contains('[')) {
        if let Some(r) = plain[i + 1..].iter().find(|r| !r.is_empty() && r.chars().count() <= 24 && r.chars().all(is_kana)) {
            add(&plain[i].clone(), &r.clone(), &mut words);
        }
    }
    let all = format!("{} {}", plain.join(" "), strip_tags(text));
    for (run, oku) in crate::kanji::candidates(&all) {
        if run.chars().count() != 1 || oku == run || words.iter().any(|(x, _)| *x == oku) {
            continue;
        }
        let Some(k) = run.chars().next().and_then(crate::kanji::info) else { continue };
        let tail = &oku[run.len()..];
        // the longest okurigana of the table that the word starts with: 一つ = ひと.つ
        let found = k.kun.iter().filter_map(|y| y.trim_matches('-').split_once('.')).filter(|(_, t)| tail.starts_with(t)).max_by_key(|(_, t)| t.len());
        if let Some((stem, t)) = found {
            add(&format!("{}{}", run, t), &format!("{}{}", stem, t), &mut words);
        }
    }
    let kanji = crate::kanji::kanji_of(&all)
        .into_iter()
        .filter_map(|c| {
            let k = crate::kanji::info(c)?;
            let mut parts: Vec<String> = Vec::new();
            if !k.meanings.is_empty() {
                parts.push(k.meanings.iter().take(3).cloned().collect::<Vec<_>>().join(", "));
            }
            if !k.on.is_empty() {
                parts.push(format!("音 {}", k.on.iter().take(4).map(|x| crate::kanji::to_katakana(x)).collect::<Vec<_>>().join("、")));
            }
            if !k.kun.is_empty() {
                parts.push(format!("訓 {}", k.kun.iter().take(4).map(|x| kun_text(x)).collect::<Vec<_>>().join("、")));
            }
            Some((c, parts.join("; ")))
        })
        .collect();
    (words, kanji)
}

fn is_kana(c: char) -> bool {
    ('\u{3041}'..='\u{3096}').contains(&c) || ('\u{30a1}'..='\u{30fa}').contains(&c) || c == 'ー'
}

/// ひと.つ = ひと(つ); -ひと = ひと
fn kun_text(k: &str) -> String {
    let k = k.trim_matches('-');
    match k.split_once('.') {
        Some((a, b)) => format!("{}({})", a, b),
        None => k.to_string(),
    }
}

/// The sound files of a side, in order (`[sound:name]` of Anki).
pub fn sounds(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(p) = rest.find("[sound:") {
        let after = &rest[p + 7..];
        match after.find(']') {
            Some(e) => {
                let name = after[..e].trim();
                if !name.is_empty() && !name.contains("..") {
                    out.push(name.to_string());
                }
                rest = &after[e + 1..];
            }
            None => break,
        }
    }
    out
}

/// Sounds become a replay button (as in Anki), scripts are dropped, pictures point at the media folder.
fn clean(html: &str, media: &Path) -> String {
    let mut s = String::new();
    let mut rest = html;
    while let Some(p) = rest.find("[sound:") {
        s.push_str(&rest[..p]);
        match rest[p..].find(']') {
            Some(e) => {
                let name = rest[p + 7..p + e].trim();
                s.push_str(&format!("<a href=\"sound:{}\">&#9654;</a>", name.replace('"', "&quot;")));
                rest = &rest[p + e + 1..];
            }
            None => {
                rest = "";
                break;
            }
        }
    }
    s.push_str(rest);
    // <img src="name"> -> a file URL
    let base = media.display().to_string().replace('\\', "/");
    let mut out = String::new();
    let mut rest = s.as_str();
    while let Some(p) = rest.find("src=\"") {
        out.push_str(&rest[..p + 5]);
        let after = &rest[p + 5..];
        let end = after.find('"').unwrap_or(after.len());
        let src = &after[..end];
        if src.contains("://") || src.starts_with("data:") {
            out.push_str(src);
        } else {
            let slash = if base.starts_with('/') { "" } else { "/" };
            out.push_str(&format!("file://{}{}/{}", slash, base, src.replace(' ', "%20")));
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    // a link to a clip kept in the media folder
    let slash = if base.starts_with('/') { "" } else { "/" };
    let out = out.replace("href=\"clip:", &format!("href=\"file://{}{}/", slash, base));
    // Qt's rich text has no <ruby>: the word shows alone, its reading comes on hover (card_readings)
    let out = ruby_re().replace_all(&out, "$1");
    // a field with Anki's furigana shown without the filter (日本[にほん] 語[ご]): only the kanji, the reading on hover
    furi_re()
        .replace_all(&out, |c: &regex::Captures| if has_kanji(&c[1]) && c[2].chars().all(is_kana) { c[1].to_string() } else { c[0].to_string() })
        .into_owned()
}

impl Collection {
    /// (question, answer) as HTML for a card.
    pub fn render(&self, c: &Card, media: &Path) -> (String, String) {
        let r = self.render_full(c, media);
        (r.q, r.a)
    }

    /// The card with the sounds each side plays by itself: the question's, and on the answer only the new ones
    /// (those that came with {{FrontSide}} are not played again, like Anki).
    pub fn render_full(&self, c: &Card, media: &Path) -> Rendered {
        let (q, a) = self.render_raw(c);
        let qs = sounds(&q);
        let mut left = qs.clone();
        let as_: Vec<String> = sounds(&a)
            .into_iter()
            .filter(|x| match left.iter().position(|y| y == x) {
                Some(i) => {
                    left.remove(i);
                    false
                }
                None => true,
            })
            .collect();
        let clip = clip_file(&a).or_else(|| clip_file(&q));
        Rendered { q: clean(&q, media), a: clean(&a, media), q_sounds: qs, a_sounds: as_, clip }
    }

    fn render_raw(&self, c: &Card) -> (String, String) {
        let Some(note) = self.notes.iter().find(|x| x.id == c.nid) else { return ("(note missing)".into(), String::new()) };
        let Some(model) = self.models.iter().find(|m| m.id == note.mid) else { return ("(note type missing)".into(), String::new()) };
        let tmpl = model.tmpls.get(if model.cloze { 0 } else { c.ord }).or_else(|| model.tmpls.first());
        let Some(t) = tmpl else { return (note.fields.first().cloned().unwrap_or_default(), String::new()) };
        let deck = self.deck_name(c.did);
        let mut cx = Ctx { names: &model.fields, values: &note.fields, tags: &note.tags, deck: &deck, front: "", ord: c.ord + 1, answer: false };
        let q = fill(&t.q, &cx);
        cx.front = &q;
        cx.answer = true;
        let a = fill(&t.a, &cx);
        (q, a)
    }
}

pub struct Rendered {
    pub q: String,
    pub a: String,
    pub q_sounds: Vec<String>,
    pub a_sounds: Vec<String>,
    /// the video clip of a Reelhouse clip card (file name in the media folder)
    pub clip: Option<String>,
}

fn clip_file(html: &str) -> Option<String> {
    let p = html.find("href=\"clip:")?;
    let after = &html[p + 11..];
    let name = &after[..after.find('"')?];
    if name.is_empty() || name.contains("..") {
        None
    } else {
        Some(name.to_string())
    }
}

// ---------- Anki packages ----------

pub struct Imported {
    pub decks: usize,
    pub notes: usize,
    pub cards: usize,
    /// notes and cards changed (pulling from AnkiWeb)
    pub updated: usize,
}

fn field_list(m: &Json) -> Vec<String> {
    let mut f: Vec<(i64, String)> = m.get("flds").items().iter().map(|x| (x.get("ord").as_i64().unwrap_or(0), x.get("name").str())).collect();
    f.sort_by_key(|x| x.0);
    f.into_iter().map(|x| x.1).collect()
}

/// Add the decks, notes, cards (with their scheduling) and media of an `.apkg` to the collection.
pub fn import_apkg(col: &mut Collection, bytes: &[u8], data: &Path) -> Result<Imported, String> {
    let zip = Zip::open(bytes).map_err(|_| "This is not an Anki package (.apkg).".to_string())?;
    let names = zip.names();
    let pick = ["collection.anki21", "collection.anki2"].into_iter().find(|n| names.iter().any(|x| x == n));
    let Some(dbname) = pick else {
        return Err(if names.iter().any(|x| x == "collection.anki21b") {
            "This package uses the newest Anki format. In Anki, export again with \"Support older Anki versions\" ticked.".to_string()
        } else {
            "No collection was found in the package.".to_string()
        });
    };
    let dbbytes = zip.read(dbname)?;
    let out = import_db(col, &dbbytes, false)?;
    // media: a JSON map "0" -> "file.jpg", the files stored under their numbers. Packages of newer Anki versions
    // keep the map as protobuf (MediaEntries) and every file zstd-compressed: both are read too, so the sounds of
    // such decks are not lost (Replay did nothing for them).
    if let Ok(raw) = zip.read("media") {
        let raw = crate::ankiweb::decompress(&raw).unwrap_or(raw);
        let names: Vec<(String, String)> = match std::str::from_utf8(&raw).ok().and_then(|t| Json::parse(t).ok()) {
            Some(map) => map.entries().iter().filter_map(|(k, v)| v.as_str().map(|n| (k.to_string(), n.to_string()))).collect(),
            None => media_entries_pb(&raw).into_iter().enumerate().map(|(i, n)| (i.to_string(), n)).collect(),
        };
        let md = media_dir(data);
        let _ = std::fs::create_dir_all(&md);
        for (num, name) in names {
            if !crate::util::safe_media_name(&name) {
                continue;
            }
            if let Ok(b) = zip.read(&num) {
                let b = crate::ankiweb::decompress(&b).unwrap_or(b);
                let _ = std::fs::write(md.join(&name), b);
            }
        }
    }
    if out.cards == 0 && out.notes == 0 {
        return Err("Nothing new in this package (it may be already imported).".into());
    }
    Ok(out)
}

/// The file names of a protobuf `MediaEntries` (newer `.apkg`): field 1 repeated entries, each with the name in
/// field 1. The n-th entry is stored in the package as the file "n".
fn media_entries_pb(b: &[u8]) -> Vec<String> {
    fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let x = *b.get(*i)?;
            *i += 1;
            v |= ((x & 0x7f) as u64) << shift;
            if x & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }
    // (field, wire type, payload) of each field of a message; varints and fixed fields are skipped over
    fn fields(b: &[u8]) -> Vec<(u64, &[u8])> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let Some(key) = varint(b, &mut i) else { break };
            match key & 7 {
                0 => {
                    if varint(b, &mut i).is_none() {
                        break;
                    }
                }
                1 => i += 8,
                2 => {
                    let Some(len) = varint(b, &mut i) else { break };
                    let end = i.saturating_add(len as usize);
                    if end > b.len() {
                        break;
                    }
                    out.push((key >> 3, &b[i..end]));
                    i = end;
                }
                5 => i += 4,
                _ => break,
            }
        }
        out
    }
    fields(b)
        .into_iter()
        .filter(|(f, _)| *f == 1)
        .map(|(_, entry)| fields(entry).into_iter().find(|(f, _)| *f == 1).map(|(_, n)| String::from_utf8_lossy(n).into_owned()).unwrap_or_default())
        .collect()
}

/// An Anki collection file read into memory (see `parse_db`), ready to be added to a collection with `merge_db`.
pub struct AnkiDb {
    crt_day: i64,
    models: Json,
    decks: Json,
    notes: Vec<(i64, Vec<Value>)>,
    cards: Vec<(i64, Vec<Value>)>,
}

/// Reads an Anki collection (SQLite, schema of `.apkg` / AnkiWeb downloads). This is the slow part of an import (a
/// big collection is many megabytes), so it can run on a worker thread; `merge_db` then changes the collection.
pub fn parse_db(dbbytes: &[u8]) -> Result<AnkiDb, String> {
    let db = Db::open(dbbytes)?;
    let colrow = db.rows("col").into_iter().next().ok_or("The collection is empty.")?.1;
    // col: id crt mod scm ver dty usn ls conf models decks dconf tags
    let crt_day = colrow.get(1).map(|v| v.int()).unwrap_or(0) / 86400;
    let models = Json::parse(&colrow.get(9).map(|v| v.text()).unwrap_or_default()).unwrap_or(Json::Null);
    let decks = Json::parse(&colrow.get(10).map(|v| v.text()).unwrap_or_default()).unwrap_or(Json::Null);
    if models.entries().is_empty() {
        return Err("This collection has no note types in the format this version reads. Export it again with \"Support older Anki versions\" ticked.".into());
    }
    Ok(AnkiDb { crt_day, models, decks, notes: db.rows("notes"), cards: db.rows("cards") })
}

/// Reads an Anki collection (SQLite, schema of `.apkg` / AnkiWeb downloads) into `col`. New decks, note types, notes
/// and cards are added. With `update`, notes and cards already here take the fields and the scheduling of the source
/// (AnkiWeb is the reference when pulling from it).
pub fn import_db(col: &mut Collection, dbbytes: &[u8], update: bool) -> Result<Imported, String> {
    let db = parse_db(dbbytes)?;
    Ok(merge_db(col, &db, update))
}

/// Adds a collection read by `parse_db` to `col` (see `import_db`).
pub fn merge_db(col: &mut Collection, db: &AnkiDb, update: bool) -> Imported {
    let (crt_day, models_j, decks_j) = (db.crt_day, &db.models, &db.decks);
    let mut out = Imported { decks: 0, notes: 0, cards: 0, updated: 0 };
    for (k, d) in decks_j.entries() {
        let id: i64 = k.parse().unwrap_or(0);
        let name = d.get("name").str();
        if id == 0 || name.is_empty() {
            continue;
        }
        if !col.decks.iter().any(|x| x.id == id) {
            // the default deck only matters when something is in it: added below if cards use it
            col.decks.push(Deck { id, name });
            out.decks += 1;
        }
    }
    for (k, m) in models_j.entries() {
        let id: i64 = k.parse().unwrap_or(0);
        if col.models.iter().any(|x| x.id == id) {
            continue;
        }
        let mut tm: Vec<(i64, Tmpl)> = m.get("tmpls").items().iter().map(|t| (t.get("ord").as_i64().unwrap_or(0), Tmpl { name: t.get("name").str(), q: t.get("qfmt").str(), a: t.get("afmt").str() })).collect();
        tm.sort_by_key(|x| x.0);
        col.models.push(Model { id, name: m.get("name").str(), cloze: m.get("type").as_i64().unwrap_or(0) == 1, fields: field_list(m), tmpls: tm.into_iter().map(|x| x.1).collect() });
    }
    // notes: id guid mid mod usn tags flds sfld csum flags data
    // where each note already here is (a lookup per row; searching the list each time is far too slow for a big collection)
    let known: HashMap<i64, usize> = col.notes.iter().enumerate().map(|(i, x)| (x.id, i)).collect();
    let mut fresh: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for (id, r) in db.notes.iter() {
        let id = *id;
        if let Some(&at) = known.get(&id) {
            if update {
                if let Some(n) = col.notes.get_mut(at) {
                    let flds = r.get(6).map(|v| v.text()).unwrap_or_default();
                    let fields: Vec<String> = flds.split('\u{1f}').map(|s| s.to_string()).collect();
                    let tags = r.get(5).map(|v| v.text().trim().to_string()).unwrap_or_default();
                    if n.fields != fields || n.tags != tags {
                        n.fields = fields;
                        n.tags = tags;
                        out.updated += 1;
                    }
                }
                fresh.insert(id);
            }
            continue;
        }
        let mid = r.get(2).map(|v| v.int()).unwrap_or(0);
        let flds = r.get(6).map(|v| v.text()).unwrap_or_default();
        col.notes.push(Note { id, mid, fields: flds.split('\u{1f}').map(|s| s.to_string()).collect(), tags: r.get(5).map(|v| v.text().trim().to_string()).unwrap_or_default() });
        fresh.insert(id);
        out.notes += 1;
    }
    // cards: id nid did ord mod usn type queue due ivl factor reps lapses left odue odid flags data
    let have: HashMap<i64, usize> = col.cards.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
    for (id, r) in db.cards.iter() {
        let id = *id;
        let g = |i: usize| -> i64 { r.get(i).map(|v: &Value| v.int()).unwrap_or(0) };
        let nid = g(1);
        if !fresh.contains(&nid) {
            continue;
        }
        let kind = g(6).clamp(0, 3) as u8;
        let queue = g(7);
        let did = if g(15) != 0 { g(15) } else { g(2) };
        let due = match kind {
            2 => crt_day + g(8),
            _ => g(8),
        };
        if let Some(&at) = have.get(&id) {
            if let Some(c) = col.cards.get_mut(at) {
                let due2 = if g(14) != 0 && kind == 2 { crt_day + g(14) } else { due };
                let (ivl, ease, reps, lapses) = (g(9).max(0), if kind == 0 { 0 } else { g(10).max(1300) }, g(11) as u32, g(12) as u32);
                if c.kind != kind || c.due != due2 || c.reps != reps || c.susp != (queue == -1) || c.did != did {
                    out.updated += 1;
                }
                c.kind = kind;
                c.due = due2;
                c.ivl = ivl;
                c.ease = ease;
                c.reps = reps;
                c.lapses = lapses;
                c.susp = queue == -1;
                c.flag = (g(16) & 7) as u8;
                c.did = did;
            }
            continue;
        }
        if !col.decks.iter().any(|d| d.id == did) {
            col.decks.push(Deck { id: did, name: "Default".into() });
        }
        col.cards.push(Card {
            id,
            nid,
            did,
            ord: g(3).max(0) as usize,
            kind,
            due: if g(14) != 0 && kind == 2 { crt_day + g(14) } else { due },
            ivl: g(9).max(0),
            ease: if kind == 0 { 0 } else { g(10).max(1300) },
            reps: g(11) as u32,
            lapses: g(12) as u32,
            step: 0,
            susp: queue == -1,
            bury: 0,
            flag: (g(16) & 7) as u8,
        });
        out.cards += 1;
    }
    // a deck nothing uses (Anki's empty "Default") is dropped
    let used: std::collections::HashSet<i64> = col.cards.iter().map(|c| c.did).collect();
    col.decks.retain(|d| used.contains(&d.id) || d.name != "Default");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_from_cards() {
        let mut col = Collection::default();
        let f: Vec<String> = ["x", "学校 【がっこう】<br>行く 【いく】", "", "", "<b>学校</b>: school<br><b>行く</b>: to go", "", "", "", ""].iter().map(|x| x.to_string()).collect();
        col.add_clip(f, 1_000_000).unwrap();
        col.add_note(col.decks[0].id, BASIC, vec!["勉強".into(), "study".into()], 1_000_001).unwrap();
        let idx = col.hint_index();
        assert_eq!(idx.len(), 3);
        let h = idx.iter().find(|h| h.term == "学校").unwrap();
        assert_eq!((h.reading.as_str(), h.meaning.as_str()), ("がっこう", "school"));
        assert!(idx.iter().any(|h| h.term == "勉強" && h.meaning == "study"));
    }

    #[test]
    fn readings_for_the_kana_box() {
        assert_eq!(furigana("私[わたし]は 学校[がっこう]に 行[い]く", false), "私は学校に行く");
        assert_eq!(furigana("私[わたし]は 学校[がっこう]", true), "わたしはがっこう");
        let f = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // Kaishi-like note: Word, Word Reading, Word Meaning, Sentence, Sentence Furigana
        let (w, k) = card_readings(&f(&["私", "わたし", "I; me", "私はアンです。", "私[わたし]はアンです。"]), "一つください");
        assert!(w.contains(&("私".to_string(), "わたし".to_string())));
        assert!(w.contains(&("一つ".to_string(), "ひとつ".to_string())), "{:?}", w);
        assert!(k.iter().any(|(c, t)| *c == '私' && t.contains("訓")), "{:?}", k);
        let (w, _) = card_readings(&f(&["日本[にほん] 語[ご]を 勉[べん] 強[きょう]する"]), "");
        assert!(w.contains(&("日本語".to_string(), "にほんご".to_string())) && w.contains(&("勉強".to_string(), "べんきょう".to_string())), "{:?}", w);
        let (w, _) = card_readings(&f(&["<ruby>漢字<rt>かんじ</rt></ruby>"]), "");
        assert_eq!(w, vec![("漢字".to_string(), "かんじ".to_string())]);
        assert!(!clean("<ruby>漢字<rt>かんじ</rt></ruby>", Path::new("/m")).contains("かんじ"));
        assert_eq!(clean("日本[にほん] 語[ご]を 勉[べん] 強[きょう]する [x]", Path::new("/m")), "日本語を勉強する [x]");
    }

    #[test]
    fn clip_card() {
        let mut col = Collection::default();
        let f: Vec<String> = ["学校に行く", "学校 【がっこう】", "学校 【ガッコウ】", "学", "school", "d", "<a href=\"clip:a.mp4\">play</a>", "<img src=\"a.jpg\">", "src"].iter().map(|x| x.to_string()).collect();
        let did = col.add_clip(f, 1_000_000).unwrap();
        assert_eq!(col.deck_name(did), "Seu Reels");
        let (q, a) = col.render(&col.cards[0], std::path::Path::new("/m"));
        assert!(q.contains("file:///m/a.jpg") && q.contains("file:///m/a.mp4") && q.contains("学校に行く"));
        assert!(a.contains("ガッコウ"));
    }

    #[test]
    fn app_decks_merge_into_seu_reels() {
        let mut col = Collection::default();
        col.ensure_builtin();
        let parent = col.create_deck("Reelhouse").unwrap();
        let clips = col.create_deck("Reelhouse::Clips").unwrap();
        let pics = col.create_deck("Reelhouse::Pictures").unwrap();
        let mine = col.create_deck("Japanese").unwrap();
        col.add_note(clips, BASIC, vec!["a".into(), "b".into()], 1000).unwrap();
        col.add_note(pics, BASIC, vec!["c".into(), "d".into()], 1001).unwrap();
        col.add_note(mine, BASIC, vec!["e".into(), "f".into()], 1002).unwrap();
        assert!(col.migrate_app_decks());
        let seu = col.decks.iter().find(|d| d.name == APP_DECK).unwrap().id;
        // the clips deck was renamed (same id); the pictures moved in; the empty parent is gone; others untouched
        assert_eq!(seu, clips);
        assert_eq!(col.cards.iter().filter(|c| c.did == seu).count(), 2);
        assert!(!col.decks.iter().any(|d| d.id == pics || d.id == parent || d.name.starts_with("Reelhouse")));
        assert_eq!(col.deck_name(mine), "Japanese");
        assert_eq!(col.cards.len(), 3);
        // a second run changes nothing
        assert!(!col.migrate_app_decks());
        // with "Seu Reels" already there, an old deck is merged into it
        let again = col.create_deck("Reelhouse::Pictures").unwrap();
        col.add_note(again, BASIC, vec!["g".into(), "h".into()], 1003).unwrap();
        assert!(col.migrate_app_decks());
        assert_eq!(col.cards.iter().filter(|c| c.did == seu).count(), 3);
        assert_eq!(col.app_deck(), Some(seu));
    }

    #[test]
    fn undo_suspend_bury() {
        let mut col = Collection::default();
        col.ensure_builtin();
        let did = col.create_deck("T").unwrap();
        let now = 1_000_000;
        col.add_note(did, BASIC, vec!["a".into(), "b".into()], now).unwrap();
        let before = col.cards[0].clone();
        col.answer(0, 3, now);
        assert!(col.cards[0].kind != 0);
        col.undo_answer().unwrap();
        assert_eq!(col.cards[0].kind, before.kind);
        assert_eq!(col.done.get(&did).copied().unwrap_or_default().0, 0);
        col.bury(0, false, now);
        assert!(col.next_card(did, now).is_none());
        col.unbury_deck(did);
        assert!(col.next_card(did, now).is_some());
        col.toggle_suspend(0, false);
        assert!(col.next_card(did, now).is_none());
    }

    #[test]
    fn scheduler_steps() {
        let mut c = Card::default();
        let now = 1_000_000;
        let t = today(now);
        schedule(&mut c, 3, now, t);
        assert_eq!((c.kind, c.step), (1, 1));
        schedule(&mut c, 3, now + 600, t);
        assert_eq!((c.kind, c.ivl, c.ease), (2, 1, 2500));
        schedule(&mut c, 3, now, t);
        assert_eq!(c.ivl, 3);
        schedule(&mut c, 1, now, t);
        assert_eq!((c.kind, c.lapses, c.ease), (3, 1, 2300));
    }

    #[test]
    fn cloze_and_templates() {
        assert_eq!(cloze("a {{c1::b::h}} c {{c2::d}}", 1, false), "a <b><font color=\"#3b82f6\">[h]</font></b> c d");
        assert_eq!(cloze("a {{c1::b::h}} c {{c2::d}}", 2, true), "a b c <b><font color=\"#3b82f6\">d</font></b>");
        let names = vec!["Front".to_string(), "Back".to_string()];
        let vals = vec!["Q".to_string(), "".to_string()];
        let cx = Ctx { names: &names, values: &vals, tags: "", deck: "D", front: "FS", ord: 1, answer: true };
        assert_eq!(fill("{{FrontSide}}|{{#Back}}has{{/Back}}{{^Back}}none{{/Back}}|{{Front}}", &cx), "FS|none|Q");
    }

    #[test]
    fn import_real_package() {
        let Ok(bytes) = std::fs::read("/tmp/apk2/test.apkg") else { return };
        let tmp = std::env::temp_dir().join("rh_study_test2");
        let mut col = Collection::default();
        let r = import_apkg(&mut col, &bytes, &tmp).unwrap();
        assert_eq!(r.notes, 3000);
        assert_eq!(r.cards, 3300);
        let jp = col.decks.iter().find(|d| d.name == "Idiomas::Japonês").unwrap().id;
        let top = col.decks.iter().find(|d| d.name == "Idiomas").unwrap().id;
        assert_eq!(col.total_cards(top), 3300);
        let now = crate::util::now_unix() as i64;
        let (nw, _, rv) = col.counts(jp, now);
        assert!(nw == NEW_PER_DAY && rv > 0, "{} {}", nw, rv);
        let idx = col.next_card(jp, now).unwrap();
        let (q, a) = col.render(&col.cards[idx], &media_dir(&tmp));
        assert!(!q.is_empty() && (a.contains("Resposta") || a.contains("Paris")), "{} / {}", q, a);
        assert!(!a.contains("[sound"));
        assert!(import_apkg(&mut col, &bytes, &tmp).is_err());
        col.save(&tmp);
        let back = Collection::load(&tmp);
        assert_eq!(back.cards.len(), 3300);
        assert!(tmp.join("study/media/pic.png").exists());
    }
}

// ---------- the game of a review session (combo, XP) ----------

/// XP of a right answer (Hard, Good, Easy); Again gives none.
pub const REVIEW_XP: u32 = 1;
/// From this many right answers in a row ("on fire") each right answer gives double XP.
pub const REVIEW_FIRE: u32 = 10;

/// What a review session has earned so far: the combo of right answers in a row, XP, accuracy. The XP goes to the same
/// daily total, goal and streak as the Learn lessons (learn::Progress::add_xp).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReviewGame {
    pub combo: u32,
    pub best_combo: u32,
    pub xp: u32,
    pub answered: u32,
    pub correct: u32,
    /// per answer, so an undo can take it back: (combo before it, XP it gave, right or not)
    history: Vec<(u32, u32, bool)>,
}

/// The XP of one answer: `combo` is the number of right answers in a row before it.
pub fn review_xp(ease: u8, combo: u32) -> u32 {
    if ease <= 1 {
        0
    } else if combo + 1 >= REVIEW_FIRE {
        REVIEW_XP * 2
    } else {
        REVIEW_XP
    }
}

impl ReviewGame {
    /// An answer (1 Again .. 4 Easy). Returns the XP it is worth.
    pub fn answer(&mut self, ease: u8) -> u32 {
        let right = ease >= 2;
        let xp = review_xp(ease, self.combo);
        self.history.push((self.combo, xp, right));
        self.answered += 1;
        if right {
            self.correct += 1;
            self.combo += 1;
            self.best_combo = self.best_combo.max(self.combo);
        } else {
            self.combo = 0;
        }
        self.xp += xp;
        xp
    }

    /// The XP the boost or the core really gave for the last answer (it may differ from what `answer` returned).
    pub fn set_last_xp(&mut self, xp: u32) {
        if let Some(h) = self.history.last_mut() {
            self.xp = self.xp - h.1 + xp;
            h.1 = xp;
        }
    }

    /// Undoes the last answer. Returns the XP to take back. The best combo stays (it was reached).
    pub fn undo(&mut self) -> u32 {
        let Some((combo, xp, right)) = self.history.pop() else { return 0 };
        self.answered -= 1;
        if right {
            self.correct -= 1;
        }
        self.combo = combo;
        self.xp -= xp;
        xp
    }

    /// Right answers out of all, in percent (100 before any answer).
    pub fn accuracy(&self) -> u32 {
        (self.correct * 100 + self.answered / 2).checked_div(self.answered).unwrap_or(100)
    }
}

#[cfg(test)]
mod tests_game {
    use super::*;

    #[test]
    fn review_xp_combo_and_undo() {
        let mut g = ReviewGame::default();
        assert_eq!(g.answer(3), 1);
        assert_eq!(g.answer(1), 0);
        assert_eq!(g.combo, 0);
        // ten right in a row: the tenth and after are worth double
        let mut xp = 0;
        for _ in 0..12 {
            xp += g.answer(4);
        }
        assert_eq!(xp, 9 + 3 * 2);
        assert_eq!(g.combo, 12);
        assert_eq!(g.best_combo, 12);
        assert_eq!(g.xp, 1 + xp);
        assert_eq!(g.answered, 14);
        assert_eq!(g.accuracy(), 93);
        // undo takes back the XP of the last answer and its combo step
        assert_eq!(g.undo(), 2);
        assert_eq!(g.combo, 11);
        assert_eq!(g.best_combo, 12);
        assert_eq!(g.xp, 1 + xp - 2);
        // a boost doubled the last answer
        g.answer(2);
        g.set_last_xp(4);
        assert_eq!(g.undo(), 4);
        assert_eq!(review_xp(1, 50), 0);
    }

    #[test]
    fn review_xp_feeds_learn_goal_and_streak() {
        use crate::learn::Progress;
        let now = 20_000 * 86400 + 3600;
        let mut p = Progress { goal: 10, streak: 3, streak_day: 19_999, ..Progress::default() };
        let (xp, msgs) = p.add_xp(4, now);
        assert_eq!(xp, 4);
        assert_eq!(msgs, vec!["streak:4".to_string()]);
        assert_eq!(p.xp_on(20_000), 4);
        // the streak counts once a day; the goal is announced when it is crossed
        let (_, msgs) = p.add_xp(6, now + 60);
        assert_eq!(msgs, vec!["goal".to_string(), "chest".to_string()]);
        assert_eq!(p.xp_total, 10);
        p.remove_xp(3, now + 120);
        assert_eq!(p.xp_on(20_000), 7);
        // the boost doubles review XP too
        p.add_boost(now);
        assert_eq!(p.add_xp(2, now + 180).0, 4);
    }
}

#[cfg(test)]
mod tests2 {
    use super::*;

    #[test]
    fn rename_deck_moves_children() {
        let mut col = Collection::default();
        let a = col.create_deck("A").unwrap();
        let b = col.create_deck("A::B").unwrap();
        col.create_deck("C").unwrap();
        assert!(!col.rename_deck(a, "C"));
        assert!(!col.rename_deck(a, "A::X"));
        assert!(col.rename_deck(a, "Z"));
        assert_eq!(col.deck_name(b), "Z::B");
        assert!(col.rename_deck(b, "Top"));
        assert_eq!(col.deck_name(b), "Top");
    }

    #[test]
    fn notes_and_stats() {
        let mut col = Collection::default();
        let did = col.create_deck("Test").unwrap();
        assert_eq!(col.add_note(did, BASIC, vec!["a".into(), "b".into()], 1000).unwrap(), 1);
        assert_eq!(col.add_note(did, REVERSED, vec!["x".into(), "y".into()], 1000).unwrap(), 2);
        assert_eq!(col.add_note(did, CLOZE, vec!["{{c1::a}} {{c3::b}} {{c1::c}}".into(), "".into()], 1000).unwrap(), 2);
        assert!(col.add_note(did, CLOZE, vec!["no cloze".into(), "".into()], 1000).is_err());
        assert!(col.add_note(did, BASIC, vec!["".into(), "b".into()], 1000).is_err());
        assert_eq!(col.cards.len(), 5);
        let nid = col.notes[2].id;
        col.edit_note(nid, vec!["{{c1::a}} {{c2::b}} {{c3::c}}".into(), "".into()], 1001).unwrap();
        assert_eq!(col.cards.iter().filter(|c| c.nid == nid).count(), 3);
        assert_eq!(col.browse(did, "x", 50).len(), 2);
        let now = 2_000_000;
        let i = col.next_card(did, now).unwrap();
        col.answer(i, 1, now);
        col.answer(i, 3, now);
        let st = col.stats(did, now);
        assert_eq!((st.today, st.today_again), (2, 1));
        assert_eq!(st.new, 5);
        col.delete_note(nid, now);
        assert_eq!(col.cards.iter().filter(|c| c.nid == nid).count(), 0);
        assert_eq!(col.trash.len(), 1);
        // back from the trash, and it survives a save
        let back = Collection::from_json(&col.to_json());
        assert_eq!(back.trash.len(), 1);
        assert_eq!(back.trash[0].cards.len(), 3);
        col.restore(0).unwrap();
        assert_eq!(col.cards.iter().filter(|c| c.nid == nid).count(), 3);
        assert!(col.trash.is_empty());
        // a whole deck: gone, then back under a new name when the old one was taken meanwhile
        let name = col.deck_name(did);
        col.delete_deck(did, now);
        assert!(col.decks.iter().all(|d| d.id != did));
        assert_eq!(col.cards.iter().filter(|c| c.nid == nid).count(), 0);
        let taken = col.create_deck(&name).unwrap();
        // the new deck even took the old id: the cards must still follow their own deck
        col.decks.iter_mut().find(|d| d.id == taken).unwrap().id = did;
        col.restore(0).unwrap();
        let restored = col.decks.iter().find(|d| d.name == format!("{} (restored)", name)).map(|d| d.id).unwrap();
        assert_ne!(restored, did);
        assert_eq!(col.cards.iter().filter(|c| c.nid == nid).count(), 3);
        assert!(col.cards.iter().filter(|c| c.nid == nid).all(|c| c.did == restored));
    }
}

#[cfg(test)]
mod tests_media_pb {
    #[test]
    fn reads_media_entries() {
        // MediaEntries { entries: [ {name: "a.mp3", size: 3}, {name: "b.jpg"} ] }
        let e1 = [0x0a, 5, b'a', b'.', b'm', b'p', b'3', 0x10, 3];
        let e2 = [0x0a, 5, b'b', b'.', b'j', b'p', b'g'];
        let mut b = vec![0x0a, e1.len() as u8];
        b.extend_from_slice(&e1);
        b.push(0x0a);
        b.push(e2.len() as u8);
        b.extend_from_slice(&e2);
        assert_eq!(super::media_entries_pb(&b), vec!["a.mp3".to_string(), "b.jpg".to_string()]);
    }
}

#[cfg(test)]
mod tests_overview {
    use super::*;

    /// A big AnkiWeb-like collection: 20 top decks with 15 sub-decks each (one level deeper too), 60k cards.
    fn big() -> Collection {
        let mut col = Collection::default();
        for top in 0..20i64 {
            col.decks.push(Deck { id: 10_000 + top, name: format!("T{}", top) });
            for sub in 0..15i64 {
                col.decks.push(Deck { id: 100_000 + top * 100 + sub, name: format!("T{}::S{}", top, sub) });
                col.decks.push(Deck { id: 900_000 + top * 100 + sub, name: format!("T{}::S{}::D", top, sub) });
            }
        }
        let ids: Vec<i64> = col.decks.iter().map(|d| d.id).collect();
        let now = 1_700_000_000;
        let t = today(now);
        for i in 0..60_000i64 {
            let kind = (i % 4) as u8;
            let due = match kind {
                1 | 3 => now - 100 + (i % 3) * 1000,
                2 => t - 2 + i % 5,
                _ => i,
            };
            col.cards.push(Card { id: i, nid: i, did: ids[(i as usize * 7) % ids.len()], kind, due, susp: i % 31 == 0, bury: if i % 37 == 0 { t + 1 } else { 0 }, ..Card::default() });
        }
        col.done.insert(ids[3], (5, 50));
        col.done.insert(ids[40], (25, 300));
        col
    }

    #[test]
    fn overview_matches_counts_and_is_fast() {
        let col = big();
        let now = 1_700_000_000;
        let t0 = std::time::Instant::now();
        let old: Vec<(i64, (u32, u32, u32, usize))> = col
            .decks
            .iter()
            .map(|d| {
                let (a, b, c) = col.counts(d.id, now);
                (d.id, (a, b, c, col.total_cards(d.id)))
            })
            .collect();
        let slow = t0.elapsed();
        let t1 = std::time::Instant::now();
        let ov = col.overview(now);
        let fast = t1.elapsed();
        for (id, v) in &old {
            assert_eq!(ov.get(id), Some(v), "deck {}", id);
        }
        eprintln!("[study] {} decks, {} cards: counts per deck {:?}, overview {:?}", col.decks.len(), col.cards.len(), slow, fast);
        assert!(fast * 5 < slow, "overview {:?} should be far quicker than {:?}", fast, slow);
    }
}
