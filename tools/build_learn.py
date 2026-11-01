#!/usr/bin/env python3
"""Build the course packs of Reelhouse's Learn section: assets/learn/{jpn,eng,por,spa}.json.

The format is the contract in docs/learn-contract.md, section 1. One addition: every sentence also carries
"lem", a list parallel to "tok" giving the word ("w") each token belongs to ("" for punctuation), because the
tokens are surface forms ("食べ", "comeu") while words are dictionary forms ("食べる", "comer").

Sources and licences (CREDITS.md, written next to the packs, repeats them with the per-speaker audio credits):
  * Tatoeba, https://tatoeba.org, exports from https://downloads.tatoeba.org/exports/ (per_language/<lang>/...):
      sentences_detailed, <a>-<b>_links, sentences_with_audio, tags, user_languages, and users_sentences.csv
      (ratings). Sentences: CC BY 2.0 FR. Audio: each recording has its own licence; only recordings under
      CC BY 4.0, CC BY-SA 4.0, CC BY-NC 4.0 or CC BY-NC-ND 3.0 are kept (all allow non-commercial sharing
      with attribution); recordings without a licence ("for Tatoeba only") are dropped.
  * JMdict, Electronic Dictionary Research and Development Group, https://www.edrdg.org/pub/Nihongo/JMdict.gz,
      CC BY-SA 4.0: Japanese -> English and Spanish glosses.
  * Wiktionary through kaikki.org (wiktextract), https://kaikki.org/dictionary/{English,Portuguese,Spanish}/,
      CC BY-SA 4.0 (and GFDL): English translation tables (eng -> jpn/por/spa), Portuguese and Spanish entries
      (-> English glosses), inflection links ("form of") used to check lemmas.
  * wordfreq (pip), data CC BY-SA 4.0: word frequency ranks.
  * fugashi + unidic-lite (pip; UniDic BSD licence): Japanese tokens, dictionary forms and readings.
  * simplemma (pip, MIT): lemmas for English, Portuguese and Spanish.
  Because JMdict, Wiktionary and wordfreq are CC BY-SA, the generated packs are CC BY-SA 4.0 as a whole.

How word translations are chosen (never invented): candidates only come from dictionaries (JMdict glosses,
Wiktionary glosses and translation tables, also pivoting through the English sense of a translation table);
Tatoeba's aligned sentence pairs then pick the candidate that actually co-occurs with the word. A pure
co-occurrence guess is used only when no dictionary has anything and the evidence is strong (>= 5 pairs, Dice >= 0.3).
Words may lack translations into some languages.

Course design: sentences need translations into at least two of the other three languages (direct Tatoeba
links first, then one hop through another course language), pass quality filters (no negative ratings, no
"@check"/vulgar/archaic tags, native or rated authors preferred, no swearing/violence, Brazilian Portuguese
only: European forms like "estou a fazer", "tu"/"vós" verbs, "facto", "comboio" are dropped) and are short.
Lessons are built greedily: each picks sentences that need only a few new words near the current frequency
rank (wordfreq), up to 6 new words, then 9 sentences using only words known so far; particles, Japanese
endings, articles and names ("free" words: pos particle/aux/suffix/prefix/article/name) are never "new words".
Every 5 lessons make a unit; a unit leans towards a topic (keyword lists below) that names it.

How to run (Python 3.10+; unidic-lite does not build under Debian's system pip, so use a venv):
  python3 -m venv ~/.venvs/learn
  ~/.venvs/learn/bin/pip install fugashi unidic-lite wordfreq simplemma orjson
  ~/.venvs/learn/bin/python -I tools/build_learn.py --cache ~/.cache/reelhouse-learn
  ~/.venvs/learn/bin/python -I tools/check_learn.py
The first run downloads ~5.3 GB (the kaikki.org dumps are 5 GB; afterwards only the small extracts in
<cache>/extract are read, and the dumps can be deleted) and takes ~15 minutes; later runs ~8 minutes and
need ~5 GB of RAM. Options: --langs jpn,eng (subset), --words 1500 (target words per course),
--max-sentences 2500, --out assets/learn. LEARN_DEBUG="jpn:年,eng:you" logs how those words' translations
were chosen.
"""
import argparse
import bz2
import gzip
import json
import math
import os
import re
import sys
import time
import urllib.request
import xml.etree.ElementTree as ET
from collections import Counter, defaultdict

try:
    import orjson
    jloads = orjson.loads
except ImportError:  # slower but works
    jloads = json.loads

LANGS = ["jpn", "eng", "por", "spa"]
ISO2 = {"jpn": "ja", "eng": "en", "por": "pt", "spa": "es"}
WIKI_CODE = {"ja": "jpn", "pt": "por", "es": "spa"}
KAIKKI = {"eng": "English", "por": "Portuguese", "spa": "Spanish"}
TATOEBA = "https://downloads.tatoeba.org/exports"
JMDICT_URL = "https://www.edrdg.org/pub/Nihongo/JMdict.gz"
KAIKKI_URL = "https://kaikki.org/dictionary/{0}/kaikki.org-dictionary-{0}.jsonl"
PAIRS = [("jpn", "eng"), ("jpn", "por"), ("jpn", "spa"), ("eng", "por"), ("eng", "spa"), ("por", "spa")]

# Audio licences that allow sharing the recording (with attribution); "" / \N means "for use on Tatoeba only".
AUDIO_OK = {"CC BY 4.0", "CC BY-SA 4.0", "CC BY-NC 4.0", "CC BY-NC-ND 3.0"}

# Words that never count as "new words" of a lesson (they may still appear in any sentence).
FREE_POS = {"particle", "aux", "suffix", "prefix", "name", "article"}
ARTICLES = {
    "eng": {"a", "an", "the"},
    "por": {"o", "a", "os", "as", "um", "uma", "uns", "umas"},
    "spa": {"el", "la", "los", "las", "un", "una", "unos", "unas"},
}

CREDITS = ("Sentences and audio: Tatoeba (tatoeba.org) contributors - sentences CC BY 2.0 FR, each recording under "
           "its own licence (CC BY 4.0, CC BY-SA 4.0, CC BY-NC 4.0 or CC BY-NC-ND 3.0; see CREDITS.md). Words: "
           "JMdict (EDRDG) CC BY-SA 4.0; Wiktionary via kaikki.org CC BY-SA 4.0; frequencies: wordfreq CC BY-SA 4.0; "
           "Japanese analysis: UniDic. This pack: CC BY-SA 4.0.")


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, file=sys.stderr, flush=True)


def fetch(url, dest):
    """Download url to dest once (kept in the cache)."""
    if os.path.exists(dest):
        return dest
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    log("download", url)
    tmp = dest + ".part"
    with urllib.request.urlopen(url) as r, open(tmp, "wb") as f:
        while True:
            b = r.read(1 << 20)
            if not b:
                break
            f.write(b)
    os.replace(tmp, dest)
    return dest


def tsv(path):
    opener = bz2.open if path.endswith(".bz2") else open
    with opener(path, "rt", encoding="utf-8", errors="replace") as f:
        for line in f:
            yield line.rstrip("\n").split("\t")


def hira(s):
    """Katakana -> hiragana (ー and other characters unchanged)."""
    return "".join(chr(ord(c) - 0x60) if "ァ" <= c <= "ヶ" else c for c in s)


HAS_KANJI = re.compile(r"[㐀-鿿々〆]")

# --------------------------------------------------------------------------------------------------------------
# Tatoeba


BAD_TAG = re.compile(r"^@|vulgar|slang|rude|offensive|archaic|old-fashioned|dated|old kana|unnatural|wrong grammar|"
                     r"outdated|Portuguese from Portugal|European Portuguese|copyright|not a sentence|sexual|"
                     r"profanity|insult|swear", re.I)
BR_TAG = re.compile(r"Brazil", re.I)
# Sentences about these things make poor beginner material; checked on the English side.
GRIM = re.compile(r"\b(kill\w*|murder\w*|suicide|rape\w*|sex\w*|drugs?|cocaine|heroin|guns?|shoot\w*|shot|corpse|"
                  r"porn\w*|naked|nude|terroris\w*|bomb\w*|slut|whore|fuck\w*|shit\w*|bitch\w*|penis|vagina|"
                  r"blood|dead|die|died|death|stab\w*|strangl\w*|prison|jail|police|cops?|hitler|nazis?|"
                  r"weapons?|knife|poison\w*|abuse\w*|torture\w*|bastard|asshole|damn)\b", re.I)

# Swear words and violence the English check cannot see (the English translation is often softened).
CRUDE = {
    "por": re.compile(r"(?i)\b(merda|porra|caralho|foda\w*|fod[ei]\w*|puta\w*|cu|bunda|viad\w*|buceta|bosta|"
                      r"cacete|mat(?:ar|ou|ei|aram|amos|ará|aria|ando|ado)|morr\w*|mort[oa]s?)\b"),
    "spa": re.compile(r"(?i)\b(culo|mierda|puta\w*|joder|jod\w*|coño|carajo|cabr[oó]n\w*|pendej\w*|chingad\w*|"
                      r"verga|mat(?:ar|ó|é|aron|amos|ará|aría|ando|ado)|muert[oa]s?|morir\w*)\b"),
    "jpn": re.compile(r"クソ|くそ|糞|殺|死|ちんこ|まんこ|ぶっ|てめえ|畜生|ちくしょう|血|銃|麻薬|自殺|強姦"),
    "eng": GRIM,
}

# Brazilian Portuguese heuristics. Tatoeba's "por" mixes Portugal and Brazil; the course wants Brazil.
PT_EU = re.compile(r"""(?ix)
  \b(?:estou|estás|está|estamos|estão|estava|estavas|estávamos|estavam|estive|esteve|estivemos|estiveram|estar|
       ando|andas|anda|andamos|andam|andava|continua|continuo|continuam|fico|fica|ficou|ficamos|ficam)
     \s+a\s+\w+(?:ar|er|ir|or)\b
| \b(?:tu|vós|contigo|convosco|connosco|teu|tua|teus|tuas|vosso|vossa|vossos|vossas)\b
| \b(?:és|estás|tens|vais|queres|podes|sabes|fazes|dizes|vens|vês|dás|gostas|falas|achas|precisas|sentes|
       conheces|deves|foste|fizeste|disseste|tiveste|estiveste|viste|compraste|eras|estavas|tinhas|querias|
       podias|sabias|fazias|sejas|estejas|tenhas|vás|queiras|possas|saibas|faças|digas|venhas|vejas)\b
| \w-(?:te|vos|me|lhe|lhes)\b
| \b(?:vos|sois|estais|tendes|ides|podeis|quereis|sabeis|fazeis|dizeis|vedes|vindes|dais|falais|sejais|estejais|
       tenhais|ireis|sereis|tereis|haveis|deveis|ficais)\b
| \b(?:facto|factos|comboios?|autocarros?|telemóve(?:l|is)|pequeno-almoço|raparigas?|frigorífico|ecrã|equipas?|
       utilizador(?:es)?|sumos?|gelados?|miúd[oa]s?|chávena|montra|casa\s+de\s+banho|contactos?|óptim[oa]s?|
       acç(?:ão|ões)|actual|actualmente|direct(?:or|ora|o|amente)|exact[oa]|exactamente|object[oa]s?|projectos?|
       registo|ficheiros?|guarda-redes|camisola|talho|bicha|fixe)\b
| \w*(?:ónic|ómic|ómen|émio|énero|ónim|ómod|ómetr|ténis|bebé|adémi|ónia|ónio)\w*
""")
BR_HINT = re.compile(r"""(?ix)
  \b(?:você|vocês|a\s+gente|ônibus|trem|celular|banheiro|geladeira|café\s+da\s+manhã|time|tela|usuários?|sucos?|
       sorvetes?|moça|garot[oa]s?|pra|pro|legal|tá|né|\w+ndo)\b
| \w*(?:ônic|ômic|ômen|êmio|ênero|ônim|ômod|ômetr|ênis|bebê|adêmi|ônia|ônio)\w*
""")


class Tatoeba:
    """Sentences of the four languages, their links, audio, tags and quality signals."""

    def __init__(self, raw):
        self.raw = raw
        self.links = defaultdict(set)
        for a, b in PAIRS:
            path = fetch(f"{TATOEBA}/per_language/{a}/{a}-{b}_links.tsv.bz2", f"{raw}/{a}-{b}_links.tsv.bz2")
            for row in tsv(path):
                if len(row) >= 2:
                    x, y = int(row[0]), int(row[1])
                    self.links[x].add(y)
                    self.links[y].add(x)
        log("links", len(self.links))
        self.text, self.owner, self.lang = {}, {}, {}
        self.tags = defaultdict(set)
        self.audio = {}
        self.audio_dropped = Counter()
        self.natives = defaultdict(set)
        pt_users, br_users = set(), set()
        for lang in LANGS:
            p = fetch(f"{TATOEBA}/per_language/{lang}/{lang}_sentences_detailed.tsv.bz2",
                      f"{raw}/{lang}_sentences_detailed.tsv.bz2")
            n = 0
            for row in tsv(p):
                if len(row) < 4:
                    continue
                sid = int(row[0])
                if sid not in self.links:  # only sentences translated into another course language matter
                    continue
                self.text[sid] = row[2].strip()
                self.owner[sid] = row[3] if row[3] != "\\N" else ""
                self.lang[sid] = lang
                n += 1
            p = fetch(f"{TATOEBA}/per_language/{lang}/{lang}_tags.tsv.bz2", f"{raw}/{lang}_tags.tsv.bz2")
            for row in tsv(p):
                if len(row) >= 2 and row[0].isdigit():
                    self.tags[int(row[0])].add(row[1])
            p = fetch(f"{TATOEBA}/per_language/{lang}/{lang}_sentences_with_audio.tsv.bz2",
                      f"{raw}/{lang}_sentences_with_audio.tsv.bz2")
            for row in tsv(p):
                if len(row) < 3 or not row[0].isdigit():
                    continue
                sid, aid, user = int(row[0]), int(row[1]), row[2]
                lic = row[3] if len(row) > 3 else ""
                url = row[4] if len(row) > 4 else ""
                if lic not in AUDIO_OK:
                    self.audio_dropped[lang] += 1
                    continue
                self.audio.setdefault(sid, (aid, user, lic, url if url != "\\N" else ""))
            p = fetch(f"{TATOEBA}/per_language/{lang}/{lang}_user_languages.tsv.bz2",
                      f"{raw}/{lang}_user_languages.tsv.bz2")
            for row in tsv(p):
                if len(row) >= 3 and row[1] == "5":
                    self.natives[lang].add(row[2])
                    if lang == "por" and len(row) > 3:
                        if re.search(r"portugal|europe|lisbo|porto\b", row[3], re.I):
                            pt_users.add(row[2])
                        elif re.search(r"bra[sz]il", row[3], re.I):
                            br_users.add(row[2])
            log("sentences", lang, n)
        # Ratings by users: 1 = OK, -1 = unsure/bad.
        self.rating = {}
        p = fetch(f"{TATOEBA}/users_sentences.csv", f"{raw}/users_sentences.csv")
        for row in tsv(p):
            if len(row) >= 3 and row[1].isdigit():
                sid = int(row[1])
                if sid in self.text:
                    pos, neg = self.rating.get(sid, (0, 0))
                    if row[2] == "1":
                        pos += 1
                    elif row[2] == "-1":
                        neg += 1
                    self.rating[sid] = (pos, neg)
        # Portuguese owners: classify by their own writing when they did not say where they are from.
        hits = defaultdict(lambda: [0, 0])
        for sid, lang in self.lang.items():
            if lang == "por":
                t = self.text[sid]
                h = hits[self.owner[sid]]
                h[0] += bool(PT_EU.search(t))
                h[1] += bool(BR_HINT.search(t))
        self.pt_owner, self.br_owner = set(pt_users), set(br_users)
        for u, (pt, br) in hits.items():
            if u in pt_users or u in br_users:
                continue
            if pt >= 3 and pt >= 2 * br:
                self.pt_owner.add(u)
            elif br >= 3 and br >= 2 * pt:
                self.br_owner.add(u)
        self._q = {}

    def quality(self, sid):
        """None when the sentence should not be used, else a score (higher is better)."""
        if sid in self._q:
            return self._q[sid]
        q = None
        t = self.text.get(sid)
        lang = self.lang.get(sid)
        if t and len(t) <= 160:
            tags = self.tags.get(sid, ())
            pos, neg = self.rating.get(sid, (0, 0))
            bad = any(BAD_TAG.search(x) for x in tags) or (neg > 0 and neg >= pos)
            if lang in CRUDE and CRUDE[lang].search(t):
                bad = True
            if lang == "por" and not bad:
                if PT_EU.search(t) or (self.owner[sid] in self.pt_owner and not BR_HINT.search(t)):
                    bad = True
            if not bad:
                q = min(pos, 3) * 0.5 + (1.0 if self.owner[sid] in self.natives[lang] else 0.0)
                q += 0.5 if "OK" in tags else 0.0
                if lang == "por":
                    q += 0.5 * bool(BR_HINT.search(t)) + 0.5 * (self.owner[sid] in self.br_owner)
                    q += 0.5 * any(BR_TAG.search(x) for x in tags)
        self._q[sid] = q
        return q

    def translations(self, sid):
        """{lang: sid} of the best translation into each other course language (direct link first, then one hop)."""
        lang = self.lang[sid]
        out = {}
        direct = self.links.get(sid, ())
        for m in LANGS:
            if m == lang:
                continue
            best, bscore = None, None
            for t in direct:
                if self.lang.get(t) == m and self.quality(t) is not None:
                    sc = (1, self.quality(t), -len(self.text[t]))
                    if bscore is None or sc > bscore:
                        best, bscore = t, sc
            if best is None:
                for c in direct:
                    if self.lang.get(c) not in (None, m) and self.quality(c) is not None:
                        for t in self.links.get(c, ()):
                            if t != sid and self.lang.get(t) == m and self.quality(t) is not None:
                                sc = (0, self.quality(t), -len(self.text[t]))
                                if bscore is None or sc > bscore:
                                    best, bscore = t, sc
            if best is not None:
                out[m] = best
        if lang != "eng" and "eng" in out and GRIM.search(self.text[out["eng"]]):
            return {}
        return out


# --------------------------------------------------------------------------------------------------------------
# Dictionaries


def norm_gloss(g):
    """'to eat (food); to consume.' -> 'to eat; to consume' (short, without notes)."""
    g = re.sub(r"\([^()]*\)|\[[^\]]*\]|\{[^}]*\}", "", g)
    g = re.sub(r"\s+", " ", g).strip(" .;,:")
    return g


def load_jmdict(path):
    """JMdict entries: {'k': [...], 'r': [...], 'common': bool, 'eng': [[gloss]], 'spa': [[gloss]], 'pos': [...]}."""
    log("parsing JMdict")
    entries = []
    with gzip.open(path) as f:
        for _, el in ET.iterparse(f, events=("end",)):
            if el.tag != "entry":
                continue
            e = {"k": [x.text for x in el.iter("keb")], "r": [x.text for x in el.iter("reb")],
                 "common": any(p.text in ("ichi1", "news1", "spec1", "spec2", "gai1")
                               for p in list(el.iter("ke_pri")) + list(el.iter("re_pri"))),
                 "uk": False, "eng": [], "spa": [], "pos": []}
            for s in el.iter("sense"):
                gl = s.findall("gloss")
                if not gl:
                    continue
                lang = gl[0].get("{http://www.w3.org/XML/1998/namespace}lang", "eng")
                if lang == "eng":
                    e["eng"].append([g.text for g in gl if g.text])
                    e["pos"].extend(p.text for p in s.findall("pos") if p.text)
                    e["uk"] = e["uk"] or any("kana alone" in (m.text or "") for m in s.findall("misc"))
                elif lang == "spa":
                    e["spa"].append([g.text for g in gl if g.text])
            entries.append(e)
            el.clear()
    log("JMdict entries", len(entries))
    return entries


def extract_kaikki(path, lang, vocab, out):
    """One pass over a kaikki.org dump, keeping what the builder needs for the words in vocab."""
    if os.path.exists(out):
        with gzip.open(out, "rt", encoding="utf-8") as f:
            return json.load(f)
    log("extracting", path, "(slow, once)")
    info = {}
    synsets = []
    keep_tr = {"ja", "pt", "es"}
    with open(path, "rb") as f:
        for line in f:
            d = jloads(line)
            w = d.get("word")
            if not w or (w not in vocab and w.lower() not in vocab):
                continue
            pos = d.get("pos", "")
            it = info.setdefault(w, {"pos": [], "form_of": [], "gl": []})
            if pos not in it["pos"]:
                it["pos"].append(pos)
            for s in d.get("senses", []):
                tags = s.get("tags", [])
                fo = s.get("form_of") or (s.get("alt_of") if "alt-of" in tags else None)
                if fo:
                    for x in fo:
                        if x.get("word") and x["word"] not in it["form_of"]:
                            it["form_of"].append(x["word"])
                    continue
                gls = s.get("glosses") or []
                if gls and not any(t in tags for t in ("obsolete", "archaic", "rare", "vulgar", "slang", "Portugal",
                                                      "dated", "derogatory", "offensive")):
                    if len(it["gl"]) < 12 and [pos, gls[0]] not in it["gl"]:
                        it["gl"].append([pos, gls[0]])
                if lang == "eng":
                    members = defaultdict(list)
                    for t in s.get("translations", []):
                        code = t.get("lang_code") or t.get("code")
                        tw = t.get("word")
                        ttags = t.get("tags", []) + t.get("raw_tags", [])
                        if code not in keep_tr or not tw:
                            continue
                        if any(x in ttags for x in ("vulgar", "slang", "archaic", "obsolete", "dated", "rare",
                                                   "humble", "honorific", "derogatory", "offensive")):
                            continue
                        if code == "pt" and "Portugal" in ttags and "Brazil" not in ttags:
                            continue
                        m = WIKI_CODE[code]
                        item = [tw, t.get("alt", "")] if code == "ja" else [tw]
                        if item not in members[m]:
                            members[m].append(item)
                    if members:
                        synsets.append([w, pos, dict(members)])
    data = {"info": info, "synsets": synsets}
    with gzip.open(out, "wt", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False)
    log("extracted", len(info), "words,", len(synsets), "synsets")
    return data


# --------------------------------------------------------------------------------------------------------------
# Tokenizers and lemmas


EUR_TOKEN = re.compile(r"\w+(?:['\-]\w+)*|[^\w\s]")
EUR_OK = re.compile(r"^[¿¡]?[^\W\d_][^\d_]*$")
EUR_CHARS = re.compile(r"^[\w\s'\-.,!?;:¿¡]+$")
NO_SPACE_BEFORE = set(".,!?;:")
NO_SPACE_AFTER = set("¿¡")
JP_OK = re.compile(r"^[぀-ヿ㐀-鿿々〆ー。、！？「」『』・…]+$")


def join_eur(toks):
    """The joining rule the app uses: single spaces, except before . , ! ? ; : and after ¿ ¡."""
    out = ""
    for i, t in enumerate(toks):
        if i == 0 or t in NO_SPACE_BEFORE or toks[i - 1] in NO_SPACE_AFTER:
            out += t
        else:
            out += " " + t
    return out


def is_punct(tok):
    return not any(c.isalnum() for c in tok)


class JpAnalyzer:
    def __init__(self):
        import fugashi
        self.tagger = fugashi.Tagger()
        self.cache = {}
        # UniDic lemma -> the dictionary form as most often written in the pool ("為る" -> "する",
        # "良い" -> "いい"), so "いう" and "言う" are one word; filled by set_display().
        self.display = {}
        self.display_read = {}
        self.display_key = {}

    def set_display(self, analyses):
        forms, reads = defaultdict(Counter), defaultdict(Counter)
        for ana in analyses:
            for a in ana:
                if a[1]:
                    forms[a[1]][a[5]] += 1
                    # 中 is なか as a noun, ちゅう as a suffix: the reading comes from the word uses.
                    reads[(a[1], a[5])][a[3]] += 1 if a[4] in FREE_POS else 100
        keys = defaultdict(Counter)
        dreads = defaultdict(Counter)
        for key, c in forms.items():
            disp = c.most_common(1)[0][0]
            self.display[key] = disp
            dreads[disp].update(reads[(key, disp)])
            keys[disp][key.split("-")[0]] += sum(c.values())
        self.display_read = {d: c.most_common(1)[0][0] for d, c in dreads.items()}
        self.display_key = {d: c.most_common(1)[0][0] for d, c in keys.items()}

    def word(self, a):
        """The word (display lemma) of an analysed token."""
        return self.display.get(a[1], a[5]) if a[1] else ""

    POS = {"名詞": "noun", "動詞": "verb", "形容詞": "adjective", "形状詞": "adjective", "副詞": "adverb",
           "代名詞": "pronoun", "助詞": "particle", "助動詞": "aux", "連体詞": "determiner", "接続詞": "conjunction",
           "感動詞": "interjection", "接尾辞": "suffix", "接頭辞": "prefix"}
    # UniDic reads 私 as わたくし; learners say わたし.
    READ_FIX = {"私": "わたし"}

    def analyze(self, text):
        """[(surface, unidic_lemma, reading_of_surface, base_reading, pos, base_form)] or None when unusable."""
        if text in self.cache:
            return self.cache[text]
        out = []
        for w in self.tagger(text):
            f = w.feature
            s = w.surface
            if w.white_space:
                out = None
                break
            p1, p2 = f.pos1, f.pos2
            if p1 in ("補助記号", "記号", "空白"):
                out.append((s, "", s, "", "punct", ""))
                continue
            base = f.orthBase if f.orthBase and f.orthBase != "*" else s
            key = f.lemma if f.lemma and f.lemma != "*" else base
            pos = "name" if p2 == "固有名詞" else self.POS.get(p1, "other")
            kana = f.kana if f.kana and f.kana != "*" else s
            kbase = f.kanaBase if f.kanaBase and f.kanaBase != "*" else kana
            rs = s if not HAS_KANJI.search(s) else self.READ_FIX.get(s, hira(kana))
            rl = base if not HAS_KANJI.search(base) else self.READ_FIX.get(base, hira(kbase))
            out.append((s, key, rs, rl, pos, base))
        self.cache[text] = out
        return out


CLOSED_POS = {"pron", "det", "article", "prep", "contraction", "conj", "particle", "postp", "num"}


class EurAnalyzer:
    """Regex tokens + lemmas (simplemma, checked against Wiktionary's inflection links)."""

    def __init__(self, lang, kinfo, caps):
        import simplemma
        self.sm = simplemma
        self.lang = lang
        self.iso = ISO2[lang]
        self.kinfo = kinfo     # word -> {"pos": [...], "form_of": [...]}
        self.caps = caps       # lowercase -> capitalised form, for words always written capitalised
        self.cache = {}

    def lemma(self, tok):
        low = tok.lower()
        if low in self.cache:
            return self.cache[low]
        if low in self.caps:
            lem = self.caps[low]
        elif "'" in low:
            # "Tom's" stays a name; "don't", "I'm" are words of their own.
            head = low.split("'")[0]
            lem = self.caps[head] + tok[len(head):] if head in self.caps else low
            if lem.startswith("i'"):
                lem = "I" + lem[1:]
        elif low in ARTICLES.get(self.lang, ()):
            lem = low
        else:
            info = self.kinfo.get(low)
            lem = self.sm.lemmatize(low, lang=self.iso) or low
            lem = lem.lower()
            if info and lem != low:
                own = [p for p in info["pos"] if p]
                if own and own[0] in CLOSED_POS:
                    # Pronouns, prepositions, contractions keep their own form ("me", "lo", "da").
                    lem = low
                elif lem not in info["form_of"]:
                    # simplemma disagrees with Wiktionary: keep the word itself if it is a dictionary word,
                    # else follow Wiktionary's own "form of" link.
                    if info["gl"] or not info["form_of"]:
                        lem = low
                    else:
                        lem = info["form_of"][0].lower()
        self.cache[low] = lem
        return lem

    def analyze(self, text):
        toks = EUR_TOKEN.findall(text)
        return [(t, "" if is_punct(t) else self.lemma(t)) for t in toks]


# --------------------------------------------------------------------------------------------------------------
# Building


class Sent:
    __slots__ = ("sid", "text", "toks", "lems", "reading", "content", "audio", "q", "tr", "n")


# Wiktionary part-of-speech tags -> the names used in the packs (never one of FREE_POS: those are decided by
# the builder's own lists).
POS_NAMES = {"noun": "noun", "verb": "verb", "adj": "adjective", "adv": "adverb", "pron": "pronoun",
             "prep": "preposition", "conj": "conjunction", "det": "determiner", "article": "determiner",
             "intj": "interjection", "num": "number", "name": "noun", "contraction": "contraction",
             "particle": "adverb", "phrase": "phrase"}
# Japanese parts of speech -> words found in JMdict's part-of-speech descriptions.
JM_POS = {"verb": ("verb",), "adjective": ("adjective", "adjectival"), "noun": ("noun",), "adverb": ("adverb",),
          "pronoun": ("pronoun",), "particle": ("particle",), "aux": ("auxiliary", "copula"),
          "interjection": ("interjection",), "suffix": ("suffix", "counter"), "prefix": ("prefix",),
          "conjunction": ("conjunction",), "determiner": ("pre-noun",)}


def word_pos_eur(lang, lem, kinfo):
    if lem in ARTICLES.get(lang, ()):
        return "article"
    info = kinfo.get(lem) or kinfo.get(lem.lower())
    if lem[:1].isupper() and lem != "I" and not lem.startswith("I'"):
        if not info or "name" in info["pos"]:
            return "name"
    if not info or not info["pos"]:
        return ""
    for p in info["pos"]:
        if p in POS_NAMES:
            return POS_NAMES[p]
    return ""


def make_pool(T, lang, an):
    """All usable sentences of one language, tokenised."""
    pool = []
    for sid, l in T.lang.items():
        if l != lang or T.quality(sid) is None:
            continue
        text = T.text[sid]
        if lang == "jpn":
            if not JP_OK.match(text) or len(text) > 28:
                continue
            ana = an.analyze(text)
            if not ana or len(ana) > 18 or "".join(a[0] for a in ana) != text:
                continue
            toks = [a[0] for a in ana]
            lems = [a[1] for a in ana]
            reading = "".join(a[2] for a in ana)
        else:
            text = re.sub(r"\s+", " ", text.replace("’", "'").replace("‘", "'"))
            if not EUR_OK.match(text) or not EUR_CHARS.match(text):
                continue
            ana = an.analyze(text)
            words = [a for a in ana if a[1]]
            if not 2 <= len(words) <= 10 or join_eur([a[0] for a in ana]) != text:
                continue
            if any(t.startswith("'") or t.endswith("'") or t.startswith("-") or t.endswith("-") for t, _ in ana):
                continue
            toks = [a[0] for a in ana]
            lems = [a[1] for a in ana]
            reading = None
        tr = T.translations(sid)
        if len(tr) < 2:
            continue
        s = Sent()
        s.sid, s.text, s.toks, s.lems, s.reading, s.tr = sid, text, toks, lems, reading, tr
        s.audio = T.audio.get(sid)
        s.q = T.quality(sid)
        s.n = len(toks)
        s.content = None
        pool.append(s)
    pool.sort(key=lambda s: s.sid)
    return pool


def build_course(lang, pool, wpos, wrank, target_words, max_sents, topic_of):
    """Greedy course: each lesson unlocks sentences that need only a few new, frequent words. Each unit
    leans towards one topic (food, family ...) by pulling that topic's words forward by up to ~150 ranks.
    Returns (lessons, unit themes)."""
    free = {w for w, p in wpos.items() if p in FREE_POS}
    cands = sorted({l for s in pool for l in s.lems if l and l not in free}, key=lambda w: wrank[w])
    rpos = {w: i for i, w in enumerate(cands)}
    usable = []
    for s in pool:
        s.content = frozenset(l for l in s.lems if l and l not in free)
        if s.content and max(rpos[w] for w in s.content) < 8000:
            usable.append(s)
    log(lang, "usable sentences", len(usable), "candidate words", len(cands))
    by_word = defaultdict(list)
    for i, s in enumerate(usable):
        for w in s.content:
            by_word[w].append(i)
    uc = [len(s.content) for s in usable]
    # Words are learnt roughly in frequency order, so a sentence whose rarest word is outside the window can be
    # skipped without looking at which of its words are still unknown.
    eff = dict(rpos)  # rank used for choosing: known words 0, words of the unit's topic pulled forward
    maxr = [max(eff[w] for w in s.content) for s in usable]
    near = {i for i, c in enumerate(uc) if c <= 3}
    themes = []
    PULL = 150

    def start_unit():
        nonlocal maxr
        recent = set(themes[-3:])
        if not themes:  # the first unit is plain "Basics": the most frequent words, no topic
            themes.append(None)
            maxr = [max(eff[w] for w in s.content) for s in usable]
            return
        for w in cands:  # the previous unit's pull ends
            if w not in known_set:
                eff[w] = rpos[w]
        counts = Counter(topic_of[w] for w in cands[:len(known) + PULL]
                         if w not in known_set and w in topic_of)
        theme = None
        for t, c in counts.most_common():
            if c >= 4 and t not in recent:
                theme = t
                break
        themes.append(theme)
        if theme:
            for w in cands[:len(known) + PULL]:
                if topic_of.get(w) == theme and w not in known_set:
                    eff[w] = max(0, rpos[w] - PULL)
        maxr = [max(eff[w] for w in s.content) for s in usable]
    ready = set()
    used = set()
    known = []
    known_set = set()
    lessons = []
    total = 0
    W = 24

    def learn(w):
        known.append(w)
        known_set.add(w)
        eff[w] = 0
        for i in by_word[w]:
            uc[i] -= 1
            if uc[i] <= 3:
                near.add(i)
            if uc[i] == 0:
                ready.add(i)
                near.discard(i)

    while len(known) < target_words and total < max_sents:
        if len(lessons) % 5 == 0:
            start_unit()
        theme = themes[-1]
        new, chosen = [], []
        early = len(lessons) < 50
        while len(new) < 6:
            budget = 6 - len(new)
            window = len(known) + W
            best, bsc = None, None
            while best is None and window <= len(cands) + W * 4:
                for i in near:
                    if maxr[i] >= window or uc[i] > budget or uc[i] == 0 or i in used:
                        continue
                    s = usable[i]
                    U = [w for w in s.content if w not in known_set]
                    mx = max(eff[w] for w in U)
                    if mx >= window:
                        continue
                    sc = (-mx / window + (0.9 if early else 0.4) * (s.audio is not None) + 0.15 * min(s.q, 3)
                          - 0.05 * s.n + 0.1 * len(U)
                          + (0.3 * sum(1 for w in U if topic_of.get(w) == theme) if theme else 0.0))
                    if bsc is None or sc > bsc:
                        best, bsc = i, sc
                if best is None:
                    window = int(window * 1.5) + 1
            if best is None:
                break
            for w in usable[best].content:
                if w not in known_set:
                    new.append(w)
                    learn(w)
            used.add(best)
            chosen.append(best)
        if not new:
            break
        # Fill the lesson: ready sentences that practise this lesson's words, then recent review.
        newset = set(new)
        covered = Counter(w for i in chosen for w in usable[i].content if w in newset)
        pick = sorted({i for w in new for i in by_word[w] if i in ready and i not in used},
                      key=lambda i: (-(usable[i].audio is not None), -min(usable[i].q, 3), usable[i].n))
        for i in pick:
            if len(chosen) >= 9:
                break
            ws = usable[i].content & newset
            if len(chosen) >= 6 and all(covered[w] >= 2 for w in ws):
                continue
            chosen.append(i)
            used.add(i)
            covered.update(ws)
        if len(chosen) < 6:
            recent = set(known[-30:])
            rev = sorted((i for i in ready if i not in used),
                         key=lambda i: (-len(usable[i].content & recent), -(usable[i].audio is not None), usable[i].n))
            for i in rev[:6 - len(chosen)]:
                chosen.append(i)
                used.add(i)
        chosen.sort(key=lambda i: (usable[i].n, -(usable[i].audio is not None)))
        lessons.append((new, [usable[i] for i in chosen]))
        total += len(chosen)
    log(lang, "lessons", len(lessons), "words", len(known), "sentences", total, "themes", Counter(themes))
    return lessons, themes


# Topic keyword lists (English lemmas) for unit titles.
TOPICS = [
    ("food", ("Food", "Comida", "Comida", "たべもの"),
     "eat food bread rice meat fish fruit apple egg milk coffee tea drink breakfast lunch dinner cook restaurant "
     "delicious hungry sugar salt vegetable cake beer wine soup cheese meal orange banana juice taste kitchen "
     "sweet bottle cup glass menu dish plate chicken pork beef potato tomato onion butter oil pepper chocolate "
     "candy cookie lemon grape strawberry snack thirsty bake fry boil sandwich pizza salad tasty bitter sour"),
    ("family", ("Family", "Família", "Familia", "かぞく"),
     "family mother father mom mum dad parent son daughter brother sister wife husband child kid baby "
     "grandmother grandfather grandma grandpa uncle aunt cousin marry marriage wedding married divorce nephew "
     "niece twin relative boyfriend girlfriend born birth birthday"),
    ("time", ("Time", "Tempo", "Tiempo", "じかん"),
     "hour minute today tomorrow yesterday morning night evening afternoon monday tuesday wednesday thursday "
     "friday saturday sunday clock early late weekend week month calendar noon midnight date january february "
     "march april june july august september october november december schedule"),
    ("nature", ("Nature & weather", "Natureza e clima", "Naturaleza y clima", "しぜんとてんき"),
     "rain snow sun wind weather sky cloud tree flower mountain river sea nature lake forest spring summer "
     "autumn fall winter season storm hot cold warm cool island beach ocean earth moon star stone rock grass "
     "leaf hill wave"),
    ("travel", ("Travel", "Viagem", "Viajes", "りょこう"),
     "travel trip train bus car plane airplane airport station ticket hotel map road city country visit drive "
     "fly street bike bicycle ship boat taxi passport luggage tour tourist abroad foreign journey bridge "
     "highway subway traffic"),
    ("home", ("Home", "Casa", "Casa", "いえ"),
     "house home room door window bed table chair key wall floor apartment garden sleep bathroom bath shower "
     "roof stairs sofa couch desk lamp light clean wash furniture neighbor neighbour"),
    ("body", ("Body & health", "Corpo e saúde", "Cuerpo y salud", "からだとけんこう"),
     "head hand eye ear mouth foot leg arm face hair body tooth teeth sick doctor hospital medicine pain health "
     "ill nose heart tired finger skin back stomach neck shoulder fever hurt injury nurse healthy exercise"),
    ("school", ("School", "Escola", "Escuela", "がっこう"),
     "school student teacher study learn class book read write test exam homework university lesson question "
     "answer pen pencil paper dictionary library college grade subject math science history page notebook "
     "teach"),
    ("work", ("Work", "Trabalho", "Trabajo", "しごと"),
     "work job office company boss business meeting employee worker career salary manager factory customer "
     "project contract colleague"),
    ("money", ("Shopping & money", "Compras e dinheiro", "Compras y dinero", "かいもの"),
     "money pay buy sell shop store price expensive cheap bank cost dollar yen market spend save rich poor "
     "shopping euro"),
    ("feelings", ("Feelings", "Sentimentos", "Sentimientos", "きもち"),
     "love happy sad angry afraid hate feel friend smile cry laugh glad sorry worry lonely fun fear surprise "
     "excited bored nervous proud jealous anxious upset joy"),
    ("animals", ("Animals", "Animais", "Animales", "どうぶつ"),
     "dog cat bird horse animal cow pig mouse rabbit monkey lion tiger bear elephant snake insect wolf sheep "
     "duck pet zoo"),
    ("clothes", ("Clothes", "Roupas", "Ropa", "ふく"),
     "clothes shirt shoe hat dress wear coat skirt jacket pants trousers sock glove tie suit uniform pocket "
     "boot"),
    ("colors", ("Colors", "Cores", "Colores", "いろ"),
     "red blue green white black yellow color colour pink purple brown gray grey"),
    ("talk", ("Talking", "Conversa", "Conversación", "かいわ"),
     "say speak talk tell ask answer call phone letter word language english japanese spanish portuguese name "
     "conversation email message voice sentence meaning explain translate"),
    ("leisure", ("Free time", "Lazer", "Tiempo libre", "しゅみ"),
     "music song sing play game movie film sport swim run dance party piano guitar tennis soccer football "
     "baseball hobby concert picture photo camera painting museum vacation holiday novel"),
]
TOPIC_WORDS = [(key, names, set(words.split())) for key, names, words in TOPICS]
UNIT_WORD = {"eng": "Unit {n}", "por": "Unidade {n}", "spa": "Unidad {n}", "jpn": "ユニット{n}"}


def topic_of_key(k):
    for key, names, ws in TOPIC_WORDS:
        if k in ws:
            return key
    return None


def unit_title(n, eng_keys, theme):
    counts = Counter(topic_of_key(k) for k in eng_keys)
    title = {l: UNIT_WORD[l].format(n=n) for l in LANGS}
    best = max(TOPIC_WORDS, key=lambda t: counts[t[0]])
    if theme and counts[theme] >= 2:
        best = next(t for t in TOPIC_WORDS if t[0] == theme)
    if counts[best[0]] >= (2 if best[0] == theme else 3):
        for l, name in zip(["eng", "por", "spa", "jpn"], best[1]):
            title[l] += ("・" if l == "jpn" else " · ") + name
    elif n == 1:
        for l, name in zip(["eng", "por", "spa", "jpn"], ["Basics", "Básico", "Básico", "きほん"]):
            title[l] += ("・" if l == "jpn" else " · ") + name
    return title


# --------------------------------------------------------------------------------------------------------------
# Word translations


DEBUG = set(os.environ.get("LEARN_DEBUG", "").split(","))
STOP_EN = set("to a an the of one one's oneself someone something some in on at for with by as or and from "
              "into out up it its".split())


class Translator:
    def __init__(self, ctx):
        self.ctx = ctx
        self.co = {}      # (X, Y) -> (n_x Counter, n_y Counter, co dict x -> Counter)

    def probes(self, lang, text):
        """The most specific lemma of each alternative of a dictionary text ("in, on, at" -> in, on, at), in
        lang's lemma space, for the co-occurrence check."""
        an = self.ctx["an"][lang]
        zf = self.ctx["zipf"][lang]
        out = []
        for alt in re.split(r"[;,/]", text)[:4]:
            if lang == "jpn":
                ana = an.analyze(alt.strip()) or []
                lems = [an.word(a) for a in ana if a[1]]
                content = [an.word(a) for a in ana if a[1] and a[4] not in FREE_POS]
            else:
                lems = [l for _, l in an.analyze(alt) if l]
                content = [l for l in lems if l.lower() not in STOP_EN and l not in ARTICLES[lang]]
            if content:
                out.append(min(content, key=lambda l: zf(l)))
            elif lems:
                out.append(lems[0])
        return out

    def cooc(self, X, Y, words):
        key = (X, Y)
        if key in self.co:
            return self.co[key]
        log("co-occurrence", X, Y)
        T, an = self.ctx["T"], self.ctx["an"]
        nx, ny, co = Counter(), Counter(), defaultdict(Counter)
        lemcache = self.ctx["lemcache"]

        def lemset(sid, lang):
            v = lemcache.get(sid)
            if v is None:
                text = T.text[sid]
                if lang == "jpn":
                    ana = an[lang].analyze(text) or []
                    v = frozenset(an[lang].word(a) for a in ana if a[1])
                else:
                    v = frozenset(l for _, l in an[lang].analyze(text.replace("’", "'")) if l)
                lemcache[sid] = v
            return v

        pairs = 0
        for s in self.ctx["pools"][X]:
            t = s.tr.get(Y)
            if t is None:
                continue
            lx = frozenset(l for l in s.lems if l)
            ly = lemset(t, Y)
            pairs += 1
            nx.update(lx)
            ny.update(ly)
            for x in lx:
                if x in words:
                    co[x].update(ly)
        log("  pairs", pairs)
        self.co[key] = (nx, ny, co)
        return self.co[key]

    def dice(self, X, Y, x, y):
        nx, ny, co = self.co[(X, Y)]
        c = co.get(x, {}).get(y, 0)
        if not c:
            return 0.0, 0
        return 2.0 * c / (nx[x] + ny[y]), c

    def gloss_candidates(self, X, x):
        """English meanings of a Portuguese/Spanish word from its Wiktionary entry (and the entry it inflects)."""
        info = self.ctx["kinfo"][X].get(x)
        out, seen = [], set()
        if not info:
            return out
        entries = [(info, 1.0)] + [(self.ctx["kinfo"][X].get(b), 0.7) for b in info["form_of"][:2]]
        for e, base in entries:
            k = 0
            for pos, g in (e or {}).get("gl", []):
                if pos in ("character", "letter", "symbol", "suffix", "prefix", "abbrev"):
                    continue
                g = clean_gloss(g)
                if not g or g in seen:
                    continue
                seen.add(g)
                # Descriptions ("used before ...", "first-person pronoun") are worse answers than translations.
                desc = re.search(r"(?i)^(used|indicates|a |an |the |plural|singular)|pronoun|letter|name of|"
                                 r"person|introduces", g)
                out.append((g, base - 0.08 * k - (0.5 if desc else 0.0), pos))
                k += 1
        return out

    def candidates(self, X, Y, x, xread, xpos):
        """{kind: [(text, prior, pos)]}. 'own': the word's own dictionary entry; 'member': translation tables that
        list the word (or, for English, words whose meaning is it); 'pivot': through the word's English meaning."""
        ctx = self.ctx
        syn, sidx = ctx["synsets"], ctx["syn_index"]
        own, member, other, pivot = [], [], [], []
        if X == "jpn" and Y in ("eng", "spa"):
            for e in ctx["jm_pick"](x, xread, xpos)[:1]:
                if e[Y]:
                    gl = [norm_gloss(g) for g in e[Y][0][:3]]
                    gl = [g for g in gl if g]
                    if Y == "spa":
                        gl = [g for g in gl if len(g) <= 30 and "japon" not in g]
                    if gl:
                        own.append(("; ".join(gl[:2]) if Y == "eng" else ", ".join(gl[:2]), 1.0, ""))
        if X in ("por", "spa") and Y == "eng":
            own.extend(self.gloss_candidates(X, x))
        if X == "eng":
            heads = [x] + ctx["kinfo"]["eng"].get(x, {}).get("form_of", [])[:2]
            # Words whose own first meaning is x ("yo": "I") are as good as x's tables, which lack the
            # most common words (their translations live on Wiktionary subpages the dump does not include);
            # a word found both ways is the surest ("you": "tú" rather than the impersonal "una").
            rev = ctx["rev"][Y].get(x.lower(), [])
            k = 0
            for h in heads:
                for sid in ctx["eng_syn"].get(h, ()):
                    for item in syn[sid][2].get(Y, ()):
                        own.append((item[0], 1.0 - min(k, 12) * 0.03 + (0.3 if item[0] in rev else 0), syn[sid][1]))
                    k += 1
            for w in rev:
                own.append((w, 0.9, ""))
        elif Y == "eng":
            for key in [(X, x)] + ([("jpn-read", xread)] if X == "jpn" and xread else []):
                for sid in sidx.get(key, ()):
                    member.append((syn[sid][0], 0.8, syn[sid][1]))
        else:
            # Tables listing the word; those whose English sense is the word's own English meaning are the
            # surest ("年" is listed under "year" and "age": its JMdict meaning says "year").
            heads = self.eng_heads(X, x, xread, xpos)
            for key in [(X, x)] + ([("jpn-read", xread)] if X == "jpn" and xread else []):
                for sid in sidx.get(key, ()):
                    anchored = syn[sid][0] in heads
                    for item in syn[sid][2].get(Y, ()):
                        (member if anchored else other).append(
                            (item[0], (1.3 if anchored else 0.7) - (0.3 if key[0] != X else 0), ""))
            for k, g in enumerate(heads):
                for sid in ctx["eng_syn"].get(g, ())[:6]:
                    for item in syn[sid][2].get(Y, ()):
                        pivot.append((item[0], 0.7, ""))
                # The target word's own first meaning is this word's own first meaning: a sense-aligned match.
                for w in ctx["rev"][Y].get(g.lower(), ()):
                    pivot.append((w, 1.2 if k == 0 else 0.8, ""))
        return {"own": own, "member": member, "other": other, "pivot": pivot}

    def eng_heads(self, X, x, xread, xpos):
        """The English words a word means (from its own entry), to pivot through."""
        heads = []
        c = self.candidates(X, "eng", x, xread, xpos)
        for text, prior, pos in sorted(c["own"], key=lambda t: -t[1]) + c["member"]:
            h = re.sub(r"^(to|a|an|the) ", "", norm_gloss(re.split(r"[;,]", text)[0]))
            if h and len(h.split()) <= 2 and h not in heads:
                heads.append(h)
            if len(heads) >= 3:
                break
        return heads

    def rank(self, X, Y, x, lst, need_evidence, min_dice=0.03):
        zf = self.ctx["zipf"][Y]
        best = {}
        for text, prior, pos in lst:
            text = re.sub(r"[（(][^()（）]*[)）]", "", text).strip()
            if not text or "/" in text:
                continue
            ps = self.probes(Y, text)
            d, c, p = max([self.dice(X, Y, x, p) + (p,) for p in ps] or [(0.0, 0, None)])
            if need_evidence and (c < 2 or d < min_dice):
                continue
            zipf = min(zf(p) / 7.0, 1.0) if p else 0.0
            # A plain word beats a phrase built on it ("あなた" rather than "貴方達", "君達").
            sc = prior + 2.0 * d + 0.2 * zipf + (0.1 if text == p else 0.0)
            if text not in best or sc > best[text][0]:
                best[text] = (sc, d, p, pos or (best[text][3] if text in best else ""))
        return sorted(best.items(), key=lambda kv: -kv[1][0])

    def pick(self, Y, ranked):
        text, (sc, d, p, pos) = ranked[0]
        if Y != "eng" and len(ranked) > 1:
            t2, (sc2, d2, p2, _) = ranked[1]
            if d2 >= 0.1 and d >= 0.1 and sc2 >= 0.85 * sc and p2 != p and len(text + t2) <= 30:
                text = text + ", " + t2
        return text, pos

    def translate(self, X, Y, x, xread, xpos, words):
        """(text, kind, pos) of the best translation of word x of language X into Y, or (None, None, None)."""
        self.cooc(X, Y, words)
        nx, ny, co = self.co[(X, Y)]
        cands = self.candidates(X, Y, x, xread, xpos)
        # With enough aligned sentences a right answer shows up in them; a dictionary sense that never does
        # (English "I" -> "interestadual", the Interstate) is set aside for better-supported candidates.
        frequent = nx[x] >= 30
        own = self.rank(X, Y, x, cands["own"], False)
        if f"{X}:{x}" in DEBUG:  # LEARN_DEBUG="jpn:年,eng:I" shows how a word's translations were chosen
            log("debug", X, x, "->", Y, "pairs", nx[x], "own", own[:5],
                "member", self.rank(X, Y, x, cands["member"] + cands["other"], frequent)[:5],
                "pivot", self.rank(X, Y, x, cands["pivot"], True)[:5])
        if xpos in ("name", "article"):
            if own and own[0][1][1] >= (0.15 if xpos == "article" else 0.05):
                return self.pick(Y, own) + ("own",)
            return None, None, None
        if xpos in FREE_POS:   # Japanese particles and endings: only their own JMdict description
            return (self.pick(Y, own) + ("own",)) if own else (None, None, None)
        # JMdict's English meaning of a Japanese word is taken as is (it is one entry, chosen by lemma/reading).
        if own and (not frequent or own[0][1][1] >= 0.02 or (X, Y) == ("jpn", "eng")):
            return self.pick(Y, own) + ("own",)
        # Tables listing the word under another English sense need clearer evidence than sense-aligned ones.
        other = (self.rank(X, Y, x, cands["member"], frequent) + self.rank(X, Y, x, cands["pivot"], True)
                 + self.rank(X, Y, x, cands["other"], frequent, 0.05))
        other.sort(key=lambda kv: -kv[1][0])
        if other:
            return self.pick(Y, other) + ("member",)
        # No dictionary candidate is supported: accept a strong corpus association only.
        row = co.get(x)
        if row:
            fy = self.ctx["free"][Y]
            scored = [(2.0 * c / (nx[x] + ny[y]), c, y) for y, c in row.items() if c >= 5 and y not in fy]
            if scored:
                d, c, y = max(scored)
                if d >= 0.3:
                    return y, "", "corpus"
        if own and (not frequent or self.dice(X, Y, x, own[0][1][2])[1] >= 1):
            return self.pick(Y, own) + ("own",)
        return None, None, None


DESC = re.compile(r"(?i)^(used|indicates|introduces|forms|a |an |the |plural|singular)|pronoun|letter|name of|"
                  r"person|accented|preposition|conjunction|article|nominative|accusative")


def clean_gloss(g):
    """A Wiktionary gloss made short enough for a word card ("first-person singular pronoun; I" -> "I"),
    or "" when it is only a description."""
    g = norm_gloss(g)
    if ":" in g:  # "A reflexive pronoun: oneself, himself, ..." -> "oneself, himself, herself"
        g = ", ".join(x.strip() for x in g.split(":", 1)[1].split(",")[:3])
    parts = [x.strip() for x in g.split(";") if x.strip()]
    good = [x for x in parts if len(x) <= 30 and not DESC.search(x)]
    if good:
        return "; ".join(good[:2])
    return parts[0] if parts and len(parts[0]) <= 40 else ""


# --------------------------------------------------------------------------------------------------------------


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache", default=os.path.expanduser("~/.cache/reelhouse-learn"))
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "learn"))
    ap.add_argument("--langs", default=",".join(LANGS))
    ap.add_argument("--words", type=int, default=1500)
    ap.add_argument("--max-sentences", type=int, default=2500)
    args = ap.parse_args()
    cache = args.cache
    out_dir = os.path.abspath(args.out)
    os.makedirs(out_dir, exist_ok=True)
    build_langs = args.langs.split(",")

    import wordfreq
    T = Tatoeba(os.path.join(cache, "tatoeba"))

    # Words of each language as written in Tatoeba (to keep the dictionary extracts small), and the words
    # that are always written with a capital letter (names, "I", "Japan", "English" ...).
    vocab = {l: set() for l in ("eng", "por", "spa")}
    capc = {l: Counter() for l in ("eng", "por", "spa")}
    capform = {l: defaultdict(Counter) for l in ("eng", "por", "spa")}
    lowc = {l: Counter() for l in ("eng", "por", "spa")}
    for sid, lang in T.lang.items():
        if lang == "jpn":
            continue
        toks = EUR_TOKEN.findall(T.text[sid].replace("’", "'"))
        for i, t in enumerate(toks):
            if is_punct(t):
                continue
            vocab[lang].add(t.lower())
            vocab[lang].add(t)
            if i > 0 and toks[i - 1] not in ".!?¿¡\"":
                (capc if t[:1].isupper() else lowc)[lang][t.lower()] += 1
                if t[:1].isupper():
                    capform[lang][t.lower()][t] += 1
    caps = {}
    for lang in vocab:
        caps[lang] = {}
        for low, c in capc[lang].items():
            if c >= 3 and c >= 9 * lowc[lang][low] and low not in ARTICLES[lang]:
                caps[lang][low] = capform[lang][low].most_common(1)[0][0]
        if lang == "eng":
            caps[lang]["i"] = "I"
    kd = {}
    for lang in ("eng", "por", "spa"):
        name = KAIKKI[lang]
        src = os.path.join(cache, f"kaikki-{name}.jsonl")
        ext = os.path.join(cache, "extract", f"kaikki-{lang}.json.gz")
        if not os.path.exists(ext):
            fetch(KAIKKI_URL.format(name), src)
        os.makedirs(os.path.dirname(ext), exist_ok=True)
        kd[lang] = extract_kaikki(src, lang, vocab[lang], ext)
    kinfo = {lang: kd[lang]["info"] for lang in kd}
    synsets = kd["eng"]["synsets"]
    syn_index = defaultdict(list)
    eng_syn = defaultdict(list)
    for i, (w, pos, members) in enumerate(synsets):
        eng_syn[w].append(i)
        for m, items in members.items():
            for item in items:
                syn_index[(m, item[0])].append(i)
                if m == "jpn" and len(item) > 1 and item[1]:
                    syn_index[("jpn-read", hira(item[1]))].append(i)

    jm = load_jmdict(fetch(JMDICT_URL, os.path.join(cache, "JMdict.gz")))
    jm_index = defaultdict(list)
    for i, e in enumerate(jm):
        for k in e["k"] + e["r"]:
            jm_index[k].append(i)

    def jm_pick(word, reading, pos):
        """JMdict entries for a Japanese word, best first: same UniDic lemma ("いる" is 居る, not 要る), same
        reading, same part of speech, common words. Particles and endings only match entries of their kind
        (otherwise "たい" would be the sea bream)."""
        key = an["jpn"].display_key.get(word, "")
        ids = set(jm_index.get(word, ()))
        if HAS_KANJI.search(key):
            ids |= set(jm_index.get(key, ()))
        es = [jm[i] for i in sorted(ids)]
        kana_only = not HAS_KANJI.search(word)
        want = JM_POS.get(pos, ())

        def pos_ok(e):
            return any(k in p for p in e["pos"] for k in want)
        if pos in ("particle", "aux", "suffix", "prefix"):
            es = [e for e in es if pos_ok(e)]

        def score(e):
            return (key in e["k"] if HAS_KANJI.search(key) else 0, (reading in e["r"]) if reading else 0,
                    pos_ok(e), e["common"], (e["uk"] or not e["k"]) if kana_only else (word in e["k"]),
                    bool(e["eng"]))
        return sorted(es, key=score, reverse=True)

    # Reverse glosses: English head word -> words of jpn/por/spa whose first meaning is that word.
    rev = {"jpn": defaultdict(list), "por": defaultdict(list), "spa": defaultdict(list)}
    for e in jm:
        if not e["common"] or not e["eng"]:
            continue
        w = e["k"][0] if e["k"] and not e["uk"] else e["r"][0]
        for g in e["eng"][0][:2]:
            h = re.sub(r"^(to|a|an|the) ", "", norm_gloss(g)).lower()
            if h and " " not in h and len(rev["jpn"][h]) < 4:
                rev["jpn"][h].append(w)
    for lang in ("por", "spa"):
        for w, info in kinfo[lang].items():
            if w != w.lower() or not info["gl"]:
                continue
            for pos, g in info["gl"][:2]:
                h = re.sub(r"^(to|a|an|the) ", "", re.split(r"[;,]", clean_gloss(g))[0].strip()).lower()
                if h and " " not in h and len(rev[lang][h]) < 6:
                    rev[lang][h].append(w)

    an = {"jpn": JpAnalyzer()}
    for lang in ("eng", "por", "spa"):
        an[lang] = EurAnalyzer(lang, kinfo[lang], caps[lang])

    fdict = {l: wordfreq.get_frequency_dict(ISO2[l]) for l in LANGS}

    def zipf_fn(lang):
        d = fdict[lang]

        def z(w):
            f = d.get(w) or d.get(w.lower()) or 0.0
            return math.log10(f * 1e9) if f > 0 else 0.0
        return z
    zipf = {l: zipf_fn(l) for l in LANGS}

    log("tokenising pools")
    pools = {}
    for lang in LANGS:
        pools[lang] = make_pool(T, lang, an[lang])
        if lang == "jpn":
            an["jpn"].set_display(an["jpn"].analyze(s.text) for s in pools["jpn"])
            for s in pools["jpn"]:
                s.lems = [an["jpn"].word(a) for a in an["jpn"].analyze(s.text)]
        log("pool", lang, len(pools[lang]))

    # Word facts per language: frequency (sum over the forms seen), pos, reading.
    winfo = {}
    for lang in LANGS:
        forms = defaultdict(Counter)
        pos = defaultdict(Counter)
        for s in pools[lang]:
            if lang == "jpn":
                for a in an["jpn"].analyze(s.text):
                    if a[1]:
                        w = an["jpn"].word(a)
                        forms[w][a[0]] += 1
                        pos[w][a[4]] += 1
            else:
                for t, l in zip(s.toks, s.lems):
                    if l:
                        forms[l][t.lower()] += 1
        d = fdict[lang]
        # A written form shared by several words ("に" particle / 煮る, "como" / comer) gives each word only
        # its share of the form's frequency, by how often the form is that word in the pool.
        form_total = Counter()
        for l, fs in forms.items():
            for f, n in fs.items():
                form_total[f] += n
        freq = {}
        for l, fs in forms.items():
            freq[l] = sum(d.get(f.lower(), 0.0) * n / form_total[f] for f, n in fs.items())
        order = sorted(freq, key=lambda l: (-freq[l], l))
        rank = {l: i + 1 for i, l in enumerate(order)}
        if lang == "jpn":
            wpos = {l: c.most_common(1)[0][0] for l, c in pos.items()}
            wread = {l: an["jpn"].display_read.get(l, l) for l in pos}
        else:
            wpos = {l: word_pos_eur(lang, l, kinfo[lang]) for l in freq}
            wread = {}
        winfo[lang] = {"rank": rank, "pos": wpos, "read": wread}

    free = {l: {w for w, p in winfo[l]["pos"].items() if p in FREE_POS} for l in LANGS}

    def eng_keys(lang, w, read, pos):
        """English head words of a word's first dictionary meanings (to find its topic before translating)."""
        if lang == "eng":
            return [w.lower()]
        if lang == "jpn":
            es = jm_pick(w, read, pos)
            gl = es[0]["eng"][0][:3] if es and es[0]["eng"] else []
        else:
            info = kinfo[lang].get(w) or {}
            gl = [clean_gloss(g) for _, g in info.get("gl", [])[:2]]
        out = []
        for g in gl:
            for part in re.split(r"[;,]", norm_gloss(g)):
                h = re.sub(r"^(to|a|an|the) ", "", part.strip()).lower()
                if h:
                    out.append(h)
        return out
    ctx = {"T": T, "an": an, "pools": pools, "zipf": zipf, "synsets": synsets, "syn_index": syn_index,
           "eng_syn": eng_syn, "kinfo": kinfo, "jm_pick": jm_pick, "rev": rev, "free": free, "lemcache": {}}
    tr = Translator(ctx)

    audio_users = Counter()
    for lang in build_langs:
        wi = winfo[lang]
        topic_of = {}
        for w, p in wi["pos"].items():
            if p not in FREE_POS:
                for k in eng_keys(lang, w, wi["read"].get(w, ""), p):
                    t = topic_of_key(k)
                    if t:
                        topic_of[w] = t
                        break
        lessons, themes = build_course(lang, pools[lang], wi["pos"], wi["rank"], args.words, args.max_sentences,
                                       topic_of)
        used_sents = [s for _, ss in lessons for s in ss]
        words = []
        seen = set()
        for new, ss in lessons:
            for w in new:
                if w not in seen:
                    seen.add(w)
                    words.append(w)
            for s in ss:
                for l in s.lems:
                    if l and l not in seen:
                        seen.add(l)
                        words.append(l)
        wordset = set(words)
        word_tr = {}
        stats = Counter()
        for w in words:
            read = wi["read"].get(w, "")
            t = {}
            for Y in LANGS:
                if Y == lang:
                    continue
                text, tpos, kind = tr.translate(lang, Y, w, read, wi["pos"].get(w, ""), wordset)
                if text:
                    t[Y] = text
                    stats[(Y, kind)] += 1
                    # The part of speech of the dictionary sense that was chosen is the best guess for the word.
                    if lang != "jpn" and tpos and Y == ("por" if lang == "eng" else "eng"):
                        p = POS_NAMES.get(tpos)
                        if p and wi["pos"].get(w) not in FREE_POS:
                            wi["pos"][w] = p
            word_tr[w] = t
        log(lang, "translation sources", dict(stats))

        out_words = []
        for w in sorted(words, key=lambda w: wi["rank"][w]):
            o = {"w": w}
            if lang == "jpn":
                o["r"] = wi["read"].get(w, w)
            o["rank"] = wi["rank"][w]
            if wi["pos"].get(w):
                o["pos"] = wi["pos"][w]
            o["tr"] = word_tr[w]
            out_words.append(o)
        out_sents = []
        for s in sorted(used_sents, key=lambda s: s.sid):
            o = {"id": s.sid, "t": s.text}
            if lang == "jpn":
                o["r"] = s.reading
            o["tok"] = s.toks
            o["lem"] = s.lems
            o["tr"] = {m: T.text[t] for m, t in s.tr.items()}
            if s.audio:
                o["audio"] = s.audio[0]
                audio_users[(lang, s.audio[1], s.audio[2], s.audio[3])] += 1
            out_sents.append(o)
        units = []
        for u in range(0, len(lessons), 5):
            n = u // 5 + 1
            group = lessons[u:u + 5]
            keys = []
            for new, _ in group:
                for w in new:
                    if lang == "eng":
                        keys.append(w.lower())
                    elif word_tr[w].get("eng"):
                        h = re.sub(r"^(to|a|an|the) ", "", word_tr[w]["eng"].split(";")[0].split(",")[0].strip())
                        keys.append(h.lower())
            units.append({"id": n, "title": unit_title(n, keys, themes[u // 5]),
                          "lessons": [{"id": f"{n}-{k + 1}", "words": new, "sentences": [s.sid for s in ss]}
                                      for k, (new, ss) in enumerate(group)]})
        pack = {"lang": lang, "version": 1, "credits": CREDITS, "words": out_words, "sentences": out_sents,
                "units": units}
        path = os.path.join(out_dir, f"{lang}.json")
        with open(path, "w", encoding="utf-8") as f:
            json.dump(pack, f, ensure_ascii=False, separators=(",", ":"))
        log("wrote", path, os.path.getsize(path), "bytes")

    write_credits(out_dir, T, audio_users)


def write_credits(out_dir, T, audio_users):
    lines = [
        "# Learn course packs: credits and licences",
        "",
        "The course packs in this folder (`jpn.json`, `eng.json`, `por.json`, `spa.json`) are generated by",
        "`tools/build_learn.py` from the open data below. As they contain material under CC BY-SA, the packs as a",
        "whole are distributed under **CC BY-SA 4.0** (https://creativecommons.org/licenses/by-sa/4.0/).",
        "",
        "## Sentences: Tatoeba",
        "",
        "Sentences and their translations come from Tatoeba (https://tatoeba.org), written by its contributors and",
        "released under **CC BY 2.0 FR** (https://creativecommons.org/licenses/by/2.0/fr/); a few are CC0. Every",
        "sentence keeps its Tatoeba id: its page, with the author, is `https://tatoeba.org/sentences/show/<id>`.",
        "Data: https://downloads.tatoeba.org/exports/ (export of 2026-10-10).",
        "",
        "## Audio: Tatoeba contributors",
        "",
        "The `audio` field is the id of a Tatoeba recording, played from `https://tatoeba.org/audio/download/<id>`.",
        "Only recordings whose licence allows sharing are referenced; recordings without a licence (\"for use on",
        "Tatoeba only\") were left out. Several licences are **non-commercial** (NC) and one is **no-derivatives**",
        "(ND): the recordings are played unchanged, and a commercial edition of the app would have to drop them.",
        "",
        "| course | speaker | licence | recordings | attribution |",
        "|---|---|---|---|---|",
    ]
    for (lang, user, lic, url), n in sorted(audio_users.items(), key=lambda kv: (kv[0][0], -kv[1])):
        link = url or f"https://tatoeba.org/user/profile/{user}"
        lines.append(f"| {lang} | {user} | {lic} | {n} | {link} |")
    lines += [
        "",
        "## Words",
        "",
        "- **JMdict** (Japanese-multilingual dictionary), Electronic Dictionary Research and Development Group,",
        "  https://www.edrdg.org/edrdg/licence.html, **CC BY-SA 4.0**: English and Spanish meanings of Japanese words.",
        "- **Wiktionary** contributors, through kaikki.org / wiktextract (Tatu Ylonen), https://kaikki.org,",
        "  **CC BY-SA 4.0** and GFDL: translation tables of the English Wiktionary (English -> Japanese, Portuguese,",
        "  Spanish), English meanings of Portuguese and Spanish words, inflection links.",
        "- **wordfreq** (Robyn Speer), https://github.com/rspeer/wordfreq, data **CC BY-SA 4.0**: word frequency",
        "  ranks (it includes data from SUBTLEX, Google Books, Wikipedia, OpenSubtitles and others).",
        "- **UniDic** (NINJAL) through fugashi and unidic-lite, BSD licence: Japanese word boundaries, dictionary",
        "  forms and readings.",
        "- **simplemma** (Adrien Barbaresi), MIT licence: dictionary forms of English, Portuguese and Spanish words.",
        "",
        "Word translations were chosen automatically among dictionary candidates, using Tatoeba's aligned sentences;",
        "a few come only from strong co-occurrence in those sentences. Some words have no translation into some",
        "languages.",
        "",
    ]
    with open(os.path.join(out_dir, "CREDITS.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines))


if __name__ == "__main__":
    main()
