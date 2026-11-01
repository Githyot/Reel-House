#!/usr/bin/env python3
"""Fetches the kana sounds of Learn's Characters view from Wikimedia Commons and writes assets/kana/<romaji>.ogg
plus CREDITS.md.

Source: Lingua Libre recordings of Japanese (files "LL-Q5287 (jpn)-<speaker>-<kana>.wav"). For every sound the
script prefers the speaker who recorded the most kana (so the voice stays the same), then fills the gaps with other
speakers. Only files whose licence is CC0, CC BY or CC BY-SA are kept; the licence and the author are read from the
file's description page (its licence categories and "Speaker:" line) and written to CREDITS.md.

It reads the ordinary web pages of commons.wikimedia.org (search and file pages) and the transcoded .ogg of each
recording, slowly (a pause between requests), so it works where the API is rate-limited. Then ffmpeg trims the
silence, makes it mono 24 kHz and encodes small Vorbis files.

    python3 assets/kana/fetch_kana.py [--out assets/kana] [--work /tmp/kana-work] [--pause 2.5]
"""
import argparse
import html
import json
import os
import re
import subprocess
import sys
import time
import urllib.parse
import urllib.request

UA = "Mozilla/5.0 (X11; Linux x86_64) ReelhouseKanaFetch/1.0 (kana sounds for an open-source media app)"
COMMONS = "https://commons.wikimedia.org"

# (sound file name, hiragana); the sounds of を, ぢ and づ are those of お, じ and ず
GOJUON = "a:あ i:い u:う e:え o:お ka:か ki:き ku:く ke:け ko:こ sa:さ shi:し su:す se:せ so:そ ta:た chi:ち tsu:つ te:て to:と na:な ni:に nu:ぬ ne:ね no:の ha:は hi:ひ fu:ふ he:へ ho:ほ ma:ま mi:み mu:む me:め mo:も ya:や yu:ゆ yo:よ ra:ら ri:り ru:る re:れ ro:ろ wa:わ n:ん"
DAKUTEN = "ga:が gi:ぎ gu:ぐ ge:げ go:ご za:ざ ji:じ zu:ず ze:ぜ zo:ぞ da:だ de:で do:ど ba:ば bi:び bu:ぶ be:べ bo:ぼ pa:ぱ pi:ぴ pu:ぷ pe:ぺ po:ぽ"
YOON = "kya:きゃ kyu:きゅ kyo:きょ sha:しゃ shu:しゅ sho:しょ cha:ちゃ chu:ちゅ cho:ちょ nya:にゃ nyu:にゅ nyo:にょ hya:ひゃ hyu:ひゅ hyo:ひょ mya:みゃ myu:みゅ myo:みょ rya:りゃ ryu:りゅ ryo:りょ gya:ぎゃ gyu:ぎゅ gyo:ぎょ ja:じゃ ju:じゅ jo:じょ bya:びゃ byu:びゅ byo:びょ pya:ぴゃ pyu:ぴゅ pyo:ぴょ"
SOUNDS = [tuple(p.split(":")) for p in (GOJUON + " " + DAKUTEN + " " + YOON).split()]

# licence categories of Commons that may be shipped (anything else is skipped)
LICENCES = {
    "CC-Zero": "CC0 1.0",
    "CC-PD-Mark": "Public domain",
    "PD-self": "Public domain",
    "PD-author": "Public domain",
    "PD-user": "Public domain",
    "CC-BY-4.0": "CC BY 4.0",
    "CC-BY-3.0": "CC BY 3.0",
    "CC-BY-2.5": "CC BY 2.5",
    "CC-BY-2.0": "CC BY 2.0",
    "CC-BY-SA-4.0": "CC BY-SA 4.0",
    "CC-BY-SA-3.0": "CC BY-SA 3.0",
    "CC-BY-SA-2.5": "CC BY-SA 2.5",
    "CC-BY-SA-2.0": "CC BY-SA 2.0",
}


def kata(s):
    return "".join(chr(ord(c) + 0x60) if "ぁ" <= c <= "ゖ" else c for c in s)


class Web:
    def __init__(self, pause):
        self.pause = pause
        self.last = 0.0

    def get(self, url, binary=False):
        for attempt in range(6):
            wait = self.pause - (time.time() - self.last)
            if wait > 0:
                time.sleep(wait)
            self.last = time.time()
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            try:
                with urllib.request.urlopen(req, timeout=60) as r:
                    data = r.read()
                    return data if binary else data.decode("utf-8", "replace")
            except urllib.error.HTTPError as e:
                if e.code == 429 or e.code >= 500:
                    delay = max(int(e.headers.get("Retry-After") or 0), 20 * (attempt + 1))
                    print(f"  ({e.code}, waiting {delay} s)", file=sys.stderr)
                    time.sleep(delay)
                    continue
                if e.code == 404:
                    return None
                raise
            except OSError as e:
                print(f"  ({e}, retrying)", file=sys.stderr)
                time.sleep(10 * (attempt + 1))
        return None


def search(web, query, limit=100, offset=0):
    """File titles of a Commons search (namespace File)."""
    url = f"{COMMONS}/w/index.php?" + urllib.parse.urlencode({"search": query, "ns6": 1, "fulltext": 1, "limit": limit, "offset": offset})
    page = web.get(url) or ""
    return sorted(set(html.unescape(t) for t in re.findall(r'title="File:([^"]+)"', page)))


NAME = re.compile(r"^LL-Q5287 \(jpn\)-(.+)-([ぁ-ゖァ-ヺ]{1,2})\.(wav|ogg|oga|flac|mp3)$")


def candidates(web, todo):
    """{hiragana: [(speaker, title)]} for every Lingua Libre recording of a lone kana found (of the sounds in `todo`)."""
    found = {}

    def add(titles):
        for t in titles:
            m = NAME.match(t)
            if not m:
                continue
            k = m.group(2)
            hira = "".join(chr(ord(c) - 0x60) if "ァ" <= c <= "ヶ" else c for c in k)
            found.setdefault(hira, [])
            if (m.group(1), t) not in found[hira]:
                found[hira].append((m.group(1), t))

    # every kana on its own: the search finds the recordings of all speakers
    for snd, hira in SOUNDS:
        if snd not in todo:
            continue
        for k in (hira, kata(hira)):
            add(search(web, f'"LL-Q5287" intitle:"{k}" filetype:audio', limit=100))
        print(f"  {hira}: {len(found.get(hira, []))} recordings", file=sys.stderr)
    # older single-syllable recordings ("Ja-Ka.oga", "Ja-WaniKaniTofuguFemale-ka.oga") fill the gaps
    by_romaji = {snd: hira for snd, hira in SOUNDS}
    older = search(web, r"intitle:/Ja-[A-Za-z]{1,3}\.og[ga]/", limit=200)
    older += search(web, r"intitle:/Ja-WaniKaniTofugu(Female|Male)-[a-z]{1,3}\.og[ga]/", limit=200)
    for t in older:
        m = re.match(r"^Ja-(?:WaniKaniTofugu(Female|Male)-)?([A-Za-z]{1,3})\.og[ga]$", t)
        if m and m.group(2).lower() in by_romaji:
            hira = by_romaji[m.group(2).lower()]
            speaker = "WaniKani/Tofugu" if m.group(1) else "Ja-series"
            found.setdefault(hira, []).append((speaker, t))
    return found


def file_info(web, title):
    """(licence, author, transcoded ogg url, page url) from the description page, or None."""
    page_url = f"{COMMONS}/wiki/" + urllib.parse.quote("File:" + title.replace(" ", "_"))
    page = web.get(page_url)
    if not page:
        return None
    cats = re.search(r'"wgCategories":(\[[^\]]*\])', page)
    cats = json.loads(cats.group(1)) if cats else []
    licence = next((LICENCES[c] for c in cats if c in LICENCES), None)
    speaker = re.search(r"Speaker:\s*(?:<[^>]+>\s*)*([^<\n]+)", page)
    author = html.unescape(speaker.group(1)).strip() if speaker else ""
    if not author:
        m = re.search(r'Lingua Libre pronunciation by ([^"]+)"', page)
        author = m.group(1) if m else ""
    if not author:
        # the Author row of the information table
        m = re.search(r'(?:id="fileinfotpl_aut"[^>]*>.*?|>Author\s*)</td>\s*<td[^>]*>(.*?)</td>', page, re.S)
        author = re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", " ", m.group(1)))).strip() if m else "unknown"
    # the transcoded copies can be downloaded (an .ogg, or an .mp3 for files that are Ogg already)
    ogg = re.search(r'(https://upload\.wikimedia\.org/wikipedia/commons/transcoded/[^"]+?\.(?:ogg|mp3))"', page)
    return licence, author, html.unescape(ogg.group(1)) if ogg else None, page_url


def convert(src, dst):
    # trim the silence at both ends (keeping a little before and after, so players do not cut the attack), mono,
    # 24 kHz, small Vorbis
    af = "silenceremove=start_periods=1:start_threshold=-45dB,areverse,silenceremove=start_periods=1:start_threshold=-45dB,areverse,adelay=60:all=1,apad=pad_dur=0.1"
    r = subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", src, "-af", af, "-ac", "1", "-ar", "24000", "-c:a", "libvorbis", "-q:a", "2", dst], capture_output=True, text=True)
    return r.returncode == 0 and os.path.getsize(dst) > 1000


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default=os.path.dirname(os.path.abspath(__file__)))
    ap.add_argument("--work", default=os.path.join(os.path.expanduser("~"), ".cache", "reelhouse-kana"))
    ap.add_argument("--pause", type=float, default=2.5, help="seconds between requests")
    ap.add_argument("--missing", action="store_true", help="only the sounds that have no file yet (keeps CREDITS.md rows)")
    a = ap.parse_args()
    os.makedirs(a.work, exist_ok=True)
    os.makedirs(a.out, exist_ok=True)
    web = Web(a.pause)
    credits = os.path.join(a.out, "CREDITS.md")
    kept = []
    if a.missing and os.path.exists(credits):
        kept = [l for l in open(credits, encoding="utf-8") if re.match(r"^\| [a-z]+\.ogg \|", l) and os.path.exists(os.path.join(a.out, l.split()[1]))]
    have = {l.split()[1][:-4] for l in kept}
    todo = {snd for snd, _ in SOUNDS if snd not in have}
    print(f"== looking for recordings of {len(todo)} sounds", file=sys.stderr)
    found = candidates(web, todo)
    # the speaker with the most kana goes first (Lingua Libre speakers before the older single files)
    count = {}
    for lst in found.values():
        for sp, _ in lst:
            count[sp] = count.get(sp, 0) + (1 if sp not in ("Ja-series", "WaniKani/Tofugu") else 0)
    rows = []
    for snd, hira in SOUNDS:
        if snd not in todo:
            continue
        dst = os.path.join(a.out, snd + ".ogg")
        for sp, title in sorted(found.get(hira, []), key=lambda x: -count.get(x[0], 0)):
            info = file_info(web, title)
            if not info:
                continue
            licence, author, ogg, page_url = info
            if not licence:
                print(f"  {snd}: {title} skipped (licence not free enough)", file=sys.stderr)
                continue
            if not ogg:
                continue
            data = web.get(ogg, binary=True)
            if not data or not (data.startswith(b"OggS") or data.startswith(b"ID3") or data[:2] == b"\xff\xfb"):
                continue
            raw = os.path.join(a.work, snd + ".src" + os.path.splitext(ogg)[1])
            with open(raw, "wb") as f:
                f.write(data)
            if convert(raw, dst):
                rows.append((snd, hira, title, author, licence, page_url))
                print(f"  {snd}: {title} ({licence}, {author})", file=sys.stderr)
                break
        else:
            print(f"  {snd}: no free recording found", file=sys.stderr)
    with open(credits, "w", encoding="utf-8") as f:
        f.write("# Kana sounds\n\n")
        f.write("Recordings of Japanese syllables from [Lingua Libre](https://lingualibre.org) on Wikimedia Commons, trimmed\n")
        f.write("and converted to mono Ogg Vorbis by `fetch_kana.py`. Each file keeps the licence of its recording (below);\n")
        f.write("the licence of each file was read from its Commons description page. を, ぢ and づ use the sounds of お, じ\n")
        f.write("and ず. A syllable without a free recording has no file (the app stays silent for it).\n\n")
        f.write("| file | kana | recording | speaker | licence |\n|---|---|---|---|---|\n")
        lines = kept + [f"| {snd}.ogg | {hira} | [{title}]({url}) | {author} | {licence} |\n" for snd, hira, title, author, licence, url in rows]
        # chart order
        order = {snd: i for i, (snd, _) in enumerate(SOUNDS)}
        lines.sort(key=lambda l: order.get(l.split()[1][:-4], 999))
        f.writelines(lines)
    print(f"== {len(kept) + len(rows)} of {len(SOUNDS)} sounds in {a.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
