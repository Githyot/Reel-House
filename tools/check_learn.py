#!/usr/bin/env python3
"""Check the Learn course packs (assets/learn/<lang>.json) against docs/learn-contract.md section 1 and print stats.

Usage: python3 tools/check_learn.py [assets/learn]       (exit status 1 when an error is found)

Checks: required keys; tokens join back to the text (Japanese: plain concatenation; others: single spaces except
before . , ! ? ; : and after ¿ ¡); "lem" is parallel to "tok"; every word and sentence id used by a lesson exists;
every lesson sentence uses only words introduced so far (or free words: particles, endings, articles, names);
each sentence has translations into at least two other languages; file size under 6 MB.
"""
import json
import os
import sys
from collections import Counter

LANGS = ["jpn", "eng", "por", "spa"]
FREE_POS = {"particle", "aux", "suffix", "prefix", "name", "article"}
NO_SPACE_BEFORE = set(".,!?;:")
NO_SPACE_AFTER = set("¿¡")


def join_eur(toks):
    out = ""
    for i, t in enumerate(toks):
        if i == 0 or t in NO_SPACE_BEFORE or toks[i - 1] in NO_SPACE_AFTER:
            out += t
        else:
            out += " " + t
    return out


def is_punct(tok):
    return not any(c.isalnum() for c in tok)


def check(path, lang):
    errors, warns = [], []
    size = os.path.getsize(path)
    with open(path, encoding="utf-8") as f:
        pack = json.load(f)
    if size > 6 * 1024 * 1024:
        errors.append(f"file is {size} bytes (> 6 MB)")
    for k in ("lang", "version", "credits", "words", "sentences", "units"):
        if k not in pack:
            errors.append(f"missing key {k}")
    if pack.get("lang") != lang:
        errors.append(f"lang is {pack.get('lang')!r}")
    others = [l for l in LANGS if l != lang]
    words = {}
    for w in pack["words"]:
        if w["w"] in words:
            errors.append(f"duplicate word {w['w']}")
        words[w["w"]] = w
        if not isinstance(w.get("rank"), int):
            errors.append(f"word {w['w']} without rank")
        if lang == "jpn" and not w.get("r"):
            errors.append(f"word {w['w']} without reading")
        if set(w.get("tr", {})) - set(others):
            errors.append(f"word {w['w']} has bad tr keys")
    sents = {}
    for s in pack["sentences"]:
        sid = s["id"]
        if sid in sents:
            errors.append(f"duplicate sentence {sid}")
        sents[sid] = s
        tok, lem = s["tok"], s.get("lem", [])
        joined = "".join(tok) if lang == "jpn" else join_eur(tok)
        if joined != s["t"]:
            errors.append(f"sentence {sid}: tokens join to {joined!r}, text is {s['t']!r}")
        if len(lem) != len(tok):
            errors.append(f"sentence {sid}: lem/tok length differ")
        for t, l in zip(tok, lem):
            if is_punct(t) != (l == "") and lang != "jpn":
                errors.append(f"sentence {sid}: token {t!r} has lemma {l!r}")
            if l and l not in words:
                errors.append(f"sentence {sid}: lemma {l!r} not in words")
        if lang == "jpn" and not s.get("r"):
            errors.append(f"sentence {sid} without reading")
        if len(set(s.get("tr", {})) & set(others)) < 2:
            errors.append(f"sentence {sid} has fewer than 2 translations")
        if "audio" in s and not isinstance(s["audio"], int):
            errors.append(f"sentence {sid}: audio is not an id")
    introduced = set()
    used_sents = set()
    n_lessons = 0
    unit_ids = []
    first_units_audio = Counter()
    lesson_sizes = Counter()
    for u in pack["units"]:
        unit_ids.append(u["id"])
        if set(u["title"]) != set(LANGS):
            errors.append(f"unit {u['id']}: title languages {sorted(u['title'])}")
        for les in u["lessons"]:
            n_lessons += 1
            new = les["words"]
            if len(new) > 6:
                warns.append(f"lesson {les['id']}: {len(new)} new words")
            for w in new:
                if w not in words:
                    errors.append(f"lesson {les['id']}: word {w!r} not in words")
                if w in introduced:
                    errors.append(f"lesson {les['id']}: word {w!r} introduced twice")
                if words.get(w, {}).get("pos") in FREE_POS:
                    warns.append(f"lesson {les['id']}: free word {w!r} listed as new")
            introduced.update(new)
            n = len(les["sentences"])
            lesson_sizes[n] += 1
            if not 6 <= n <= 12:
                warns.append(f"lesson {les['id']}: {n} sentences")
            practised = set()
            for sid in les["sentences"]:
                s = sents.get(sid)
                if s is None:
                    errors.append(f"lesson {les['id']}: sentence {sid} missing")
                    continue
                used_sents.add(sid)
                if u["id"] <= 10:
                    first_units_audio["audio" if "audio" in s else "none"] += 1
                for l in s["lem"]:
                    if not l:
                        continue
                    practised.add(l)
                    if l not in introduced and words.get(l, {}).get("pos") not in FREE_POS:
                        errors.append(f"lesson {les['id']}: sentence {sid} uses {l!r} before it is introduced")
            for w in new:
                if w not in practised:
                    warns.append(f"lesson {les['id']}: new word {w!r} not in its sentences")
    if unit_ids != list(range(1, len(unit_ids) + 1)):
        errors.append("unit ids are not 1..n")
    unused = set(sents) - used_sents
    if unused:
        warns.append(f"{len(unused)} sentences not used by any lesson")

    intro_words = [words[w] for w in introduced if w in words]
    with_audio = sum(1 for s in sents.values() if "audio" in s)
    print(f"== {lang}: {size / 1e6:.2f} MB, {len(words)} words ({len(introduced)} introduced, "
          f"{len(words) - len(introduced)} free/other), {len(sents)} sentences, {len(unit_ids)} units, "
          f"{n_lessons} lessons")
    print(f"   sentences with audio: {100 * with_audio / max(1, len(sents)):.1f}% "
          f"(units 1-10: {100 * first_units_audio['audio'] / max(1, sum(first_units_audio.values())):.1f}%)")
    print("   introduced words with a translation: " + ", ".join(
        f"{o} {100 * sum(1 for w in intro_words if o in w.get('tr', {})) / max(1, len(intro_words)):.1f}%"
        for o in others))
    print("   sentences per lesson: " + ", ".join(f"{k}:{v}" for k, v in sorted(lesson_sizes.items())))
    print("   unit titles: " + "; ".join(u["title"]["eng"] for u in pack["units"][:12]) + " ...")
    for e in errors[:20]:
        print("   ERROR", e)
    if len(errors) > 20:
        print(f"   ... {len(errors) - 20} more errors")
    for w in warns[:10]:
        print("   warn", w)
    if len(warns) > 10:
        print(f"   ... {len(warns) - 10} more warnings")
    return not errors


def main():
    d = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..",
                                                             "assets", "learn")
    ok = True
    for lang in LANGS:
        p = os.path.join(d, f"{lang}.json")
        if not os.path.exists(p):
            print(f"== {lang}: missing {p}")
            ok = False
            continue
        ok = check(p, lang) and ok
    print("OK" if ok else "FAILED")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
