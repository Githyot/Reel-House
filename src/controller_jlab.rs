//! Jlab mode of the Study (jlab.rs): decks in the mode, its settings, the word lookups and the card the review shows.
use super::*;
use crate::jlab::{self, Look};

impl Core {
    pub(super) fn jlab_save(&self) {
        self.jlab.save(&self.profile_dir);
    }

    /// True when the card `idx` of the collection is studied in Jlab mode.
    pub fn jlab_card_on(&self, idx: usize) -> bool {
        let Some(c) = self.study.cards.get(idx) else { return false };
        if self.jlab.decks.is_empty() {
            return false;
        }
        let name = self.study.deck_name(c.did);
        self.jlab.is_on(c.did, &name, &|d| self.study.deck_name(d))
    }

    /// Turns Jlab mode on or off for a deck (and the decks under it).
    pub fn jlab_deck(&mut self, did: i64, on: bool) {
        if !self.study.decks.iter().any(|d| d.id == did) {
            return;
        }
        self.jlab.set_deck(did, on);
        self.jlab_save();
        self.toast(if on { "Jlab mode on" } else { "Jlab mode off" });
        self.events.push(Event::Study);
    }

    pub fn jlab_set(&mut self, key: &str, value: &Json) {
        self.jlab.set(key, value);
        self.jlab_save();
        self.events.push(Event::Study);
    }

    /// A word of the card was tapped: looked up on Jisho in the background.
    pub fn jlab_lookup(&mut self, nid: i64, word: &str) {
        let word = word.trim().to_string();
        if word.is_empty() || !word.chars().any(|c| jlab::is_kana(c) || crate::kanji::is_kanji(c)) {
            return;
        }
        if let Some(l) = self.jlab.notes.get(&nid).and_then(|d| d.looks.iter().find(|l| l.word == word)).cloned() {
            // already looked up: it becomes the gap again
            self.jlab.add_look(nid, l);
            self.jlab_save();
            self.events.push(Event::Study);
            return;
        }
        self.jlab_busy = word.clone();
        self.events.push(Event::Study);
        self.spawn(move |tx, http| {
            let r = jlab::lookup(&*http, &word);
            send(tx, Msg::JlabLooked(nid, word, r));
        });
    }

    pub(super) fn on_jlab_looked(&mut self, nid: i64, word: String, r: Option<Look>) {
        if self.jlab_busy == word {
            self.jlab_busy.clear();
        }
        match r {
            Some(l) => {
                self.jlab.add_look(nid, l);
                self.jlab_save();
            }
            None => self.toast("Word not found in the dictionary."),
        }
        self.events.push(Event::Study);
    }

    pub fn jlab_unlook(&mut self, nid: i64, word: &str) {
        self.jlab.remove_look(nid, word);
        self.jlab_save();
        self.events.push(Event::Study);
    }

    /// After the answer of card `idx`: a card remembered long enough moves on (listening → reading → retired).
    pub(super) fn jlab_after_answer(&mut self, idx: usize) {
        if !self.jlab_card_on(idx) {
            self.jlab.undo = None;
            return;
        }
        let Some(c) = self.study.cards.get(idx) else { return };
        let (nid, kind, ivl) = (c.nid, c.kind, c.ivl);
        match self.jlab.after_answer(nid, kind, ivl) {
            Some(1) => {
                // the reading card starts as a new card
                self.study.reset_card(idx);
                // first in the queue of new cards: it comes up next time, not after every new card of the deck
                let first = self.study.cards.iter().filter(|c| c.kind == 0).map(|c| c.due).min().unwrap_or(0);
                if let Some(c) = self.study.cards.get_mut(idx) {
                    c.due = first - 1;
                }
                self.toast("This listening card is now a reading card.");
            }
            Some(_) => {
                if let Some(c) = self.study.cards.get_mut(idx) {
                    c.susp = true;
                }
                self.toast("You know this sentence: the card retires.");
            }
            None => {}
        }
        self.jlab_save();
    }

    pub(super) fn jlab_undo(&mut self) {
        if self.jlab.undo.is_some() {
            self.jlab.undo_answer();
            self.jlab_save();
        }
    }

    /// The card as the Jlab view of the review shows it (None when the card is not in Jlab mode).
    pub fn jlab_card_json(&self, idx: usize, answer: bool) -> Option<Json> {
        if !self.jlab_card_on(idx) {
            return None;
        }
        let c = self.study.cards.get(idx)?;
        let note = self.study.notes.iter().find(|n| n.id == c.nid)?;
        let model = self.study.models.iter().find(|m| m.id == note.mid)?;
        let f = jlab::pick_fields(&model.fields, &note.fields);
        let get = |i: Option<usize>| i.and_then(|i| note.fields.get(i)).map(|s| s.as_str()).unwrap_or("");
        let data = self.jlab.note(c.nid);
        let st = &self.jlab.settings;
        let segs = jlab::sentence(get(Some(f.sentence)), get(f.reading));
        let split = |t: &str| self.knowledge.tokens("jpn", t);
        let mut toks = jlab::words(&segs, Some(&split));
        if toks.iter().all(|t| t.punct()) {
            // the tokenizer found nothing it knows: script changes cut the words
            toks = jlab::words(&segs, None);
        }
        jlab::fill_readings(&mut toks, &data.looks);
        let learned: Vec<String> = if st.kana == "learned" { self.knowledge.char_rows("jpn").iter().filter(|r| r.right > 0).map(|r| r.word.clone()).collect() } else { vec![] };
        let kana = jlab::kana_set(&st.kana, &learned);
        let known = jlab::known_kanji(st.kanji);
        let reading_phase = data.phase >= 1;
        let level = if reading_phase { st.read } else { st.listen };
        let gap = if reading_phase { data.cloze.clone() } else { String::new() };
        let mut gap_done = false;
        let tokens: Vec<Json> = toks
            .iter()
            .map(|t| {
                let (shown, ruby) = jlab::show(t, level, &kana, &known);
                let text = t.text();
                let is_gap = !gap.is_empty() && !gap_done && (text == gap || (text.contains(&gap) && gap.chars().count() > 1) || (gap.contains(&text) && t.has_kanji()));
                if is_gap {
                    gap_done = true;
                }
                let looked = data.looks.iter().find(|l| l.word == text || (t.has_kanji() && l.word.contains(&text)));
                Json::obj(vec![
                    ("t", shown.into()),
                    ("r", ruby.into()),
                    ("w", text.into()),
                    ("punct", t.punct().into()),
                    ("gap", is_gap.into()),
                    ("tip", looked.map(|l| l.meaning.clone()).unwrap_or_default().into()),
                ])
            })
            .collect();
        let dict_tok = |l: &Look| -> (String, String) {
            let segs = jlab::align(&l.word, &l.reading).unwrap_or_else(|| vec![jlab::Seg { text: l.word.clone(), reading: if l.word.chars().any(crate::kanji::is_kanji) { l.reading.clone() } else { String::new() } }]);
            jlab::show(&jlab::Tok { segs }, st.dict, &kana, &known)
        };
        let looks: Vec<Json> = data
            .looks
            .iter()
            .map(|l| {
                let (shown, ruby) = dict_tok(l);
                Json::obj(vec![("word", l.word.as_str().into()), ("shown", shown.into()), ("ruby", ruby.into()), ("reading", l.reading.as_str().into()), ("meaning", l.meaning.as_str().into()), ("gap", (l.word == data.cloze).into())])
            })
            .collect();
        let hint = if gap.is_empty() { String::new() } else { data.looks.iter().find(|l| l.word == gap).map(|l| l.meaning.clone()).unwrap_or_default() };
        let read_ok = toks.iter().all(|t| t.read_ok());
        Some(Json::obj(vec![
            ("nid", c.nid.into()),
            ("phase", (data.phase as i64).into()),
            ("level", (level as i64).into()),
            ("tokens", Json::Arr(tokens)),
            // on the front of a listening card the sentence may be hidden (audio only)
            ("showText", (answer || reading_phase || st.listen_text).into()),
            ("translation", jlab::plain(get(f.translation)).into()),
            ("looks", Json::Arr(looks)),
            ("hint", hint.into()),
            ("gapOn", (!gap.is_empty() && gap_done).into()),
            ("readOk", read_ok.into()),
            ("busy", self.jlab_busy.as_str().into()),
            ("retireDays", st.retire_days.into()),
        ]))
    }

    /// The state of the mode for the pages: decks in it and the settings.
    pub fn jlab_state(&self) -> Json {
        let s = &self.jlab.settings;
        Json::obj(vec![
            ("decks", Json::Arr(self.jlab.decks.iter().map(|d| (*d).into()).collect())),
            ("listen", (s.listen as i64).into()),
            ("read", (s.read as i64).into()),
            ("dict", (s.dict as i64).into()),
            ("kana", s.kana.as_str().into()),
            ("kanji", (s.kanji as i64).into()),
            ("kanjiMax", (jlab::kanji_order().len() as i64).into()),
            ("retireDays", s.retire_days.into()),
            ("listenText", s.listen_text.into()),
        ])
    }
}
