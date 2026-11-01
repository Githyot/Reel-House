#!/usr/bin/env python3
"""List button rows of the QML front end that may not fit their text, for a review by eye.

The owner saw buttons run into each other once the interface is in Portuguese (longer words than English).
This finds the places where that can happen:

  * every `Segmented { ... }` (linked toggle buttons) and whether it is given `fitWidth` (room it may use);
  * every `Row`, `RowLayout` or `Flow` that holds two or more buttons (FlatButton, Button, ModeButton,
    PrefButton, ToolButton, Segmented), with the fixed numbers it or its buttons use for `width`,
    `implicitWidth` or `Layout.preferredWidth` — a fixed width cannot grow for a longer translation.

A fixed width is fine for an icon-only button (glyph and no text); those are marked "icon" and skipped
with --text-only.

Usage:  python3 tools/ui_audit.py [qt/qml] [--text-only]
"""
import os
import re
import sys

BUTTONS = ("FlatButton", "Button", "ModeButton", "PrefButton", "ToolButton", "Segmented", "MenuLabel", "LearnButton")
ROWS = ("Row", "RowLayout", "Flow")
FIXED = re.compile(r"\b(width|implicitWidth|Layout\.preferredWidth|Layout\.minimumWidth)\s*:\s*(\d+(?:\.\d+)?)\s*(?:;|$|\})")
OPEN = re.compile(r"(?<![\w.])([A-Z]\w*)\s*\{")


def blocks(text):
    """Yield (type name, start offset, end offset) of every `Type { ... }` block, nested ones too."""
    stack = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        # skip strings and comments: braces inside them do not count
        if c in "\"'":
            q = c
            i += 1
            while i < n and text[i] != q:
                i += 2 if text[i] == "\\" else 1
        elif text.startswith("//", i):
            i = text.find("\n", i)
            if i < 0:
                break
        elif text.startswith("/*", i):
            i = text.find("*/", i) + 1
            if i <= 0:
                break
        elif c == "{":
            m = None
            # the type name right before this brace
            back = text[max(0, i - 80):i + 1]
            mm = list(OPEN.finditer(back))
            if mm and mm[-1].end() == len(back):
                m = mm[-1].group(1)
            stack.append((m, i))
        elif c == "}":
            if stack:
                name, start = stack.pop()
                if name:
                    yield name, start, i
        i += 1


def line_of(text, off):
    return text.count("\n", 0, off) + 1


def direct_children(text, start, end):
    """The blocks directly inside [start, end] (one level down)."""
    out = []
    depth = 0
    i = start + 1
    while i < end:
        c = text[i]
        if c in "\"'":
            q = c
            i += 1
            while i < end and text[i] != q:
                i += 2 if text[i] == "\\" else 1
        elif c == "{":
            if depth == 0:
                back = text[max(0, i - 80):i + 1]
                mm = list(OPEN.finditer(back))
                name = mm[-1].group(1) if mm and mm[-1].end() == len(back) else None
                out.append([name, i, None])
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0 and out and out[-1][2] is None:
                out[-1][2] = i
        i += 1
    return [tuple(x) for x in out if x[2] is not None]


def own_props(body):
    """The text of a block without its nested blocks (only its own properties)."""
    out = []
    depth = 0
    for c in body:
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        elif depth == 0:
            out.append(c)
    return "".join(out)


def audit(path, text_only):
    text = open(path, encoding="utf-8").read()
    found = []
    for name, s, e in blocks(text):
        if name == "Segmented":
            props = own_props(text[s + 1:e])
            fit = "fitWidth" in props
            fixed = FIXED.findall(props)
            found.append((line_of(text, s), "Segmented", ("fits itself (fitWidth)" if fit else "natural width, no fitWidth") + ("; fixed " + ", ".join(f"{k}={v}" for k, v in fixed) if fixed else "")))
        elif name in ROWS:
            kids = [k for k in direct_children(text, s, e) if k[0] in BUTTONS or (k[0] == "Repeater" and re.search(r"delegate\s*:\s*(FlatButton|Button|ModeButton)", text[k[1]:k[2]]))]
            if len(kids) < 2 and not any(k[0] == "Repeater" for k in kids):
                continue
            notes = []
            own = FIXED.findall(own_props(text[s + 1:e]))
            if own:
                notes.append(f"{name} " + ", ".join(f"{k}={v}" for k, v in own))
            for kn, ks, ke in kids:
                props = own_props(text[ks + 1:ke])
                fixed = FIXED.findall(props)
                if not fixed:
                    continue
                # `text:` of the button itself (not `ToolTip.text:`), and not an empty string
                has_text = re.search(r"(?<![.\w])text\s*:\s*(?!\"\")", props) is not None
                kind = "text" if has_text else "icon"
                if text_only and kind == "icon":
                    continue
                notes.append(f"L{line_of(text, ks)} {kn} [{kind}] " + ", ".join(f"{k}={v}" for k, v in fixed))
            if notes:
                found.append((line_of(text, s), f"{name} of {len(kids)} buttons", "; ".join(notes)))
        elif name in ("Text", "Lbl", "Label") and not text_only:
            # a translated label in a fixed width (or a share of its parent) that neither elides nor wraps runs into its neighbours
            props = own_props(text[s + 1:e])
            if "Core.tr(" not in props and "tr(" not in props:
                continue
            if re.search(r"\belide\s*:|\bwrapMode\s*:|fontSizeMode", props):
                continue
            w = re.search(r"(?<![.\w])width\s*:\s*([^;\n]+)", props)
            if w and re.search(r"\d|/", w.group(1)):
                found.append((line_of(text, s), f"{name} (translated, no elide)", "width: " + w.group(1).strip()[:60]))
    return sorted(found)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    text_only = "--text-only" in sys.argv
    root = args[0] if args else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "qt", "qml")
    total = 0
    for f in sorted(os.listdir(root)):
        if not f.endswith(".qml"):
            continue
        rows = audit(os.path.join(root, f), text_only)
        for ln, what, note in rows:
            print(f"{f}:{ln}: {what}: {note}")
            total += 1
    print(f"-- {total} places to look at", file=sys.stderr)


if __name__ == "__main__":
    main()
