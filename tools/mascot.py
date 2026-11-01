#!/usr/bin/env python3
"""Draws "Arara", the blue macaw mascot of Reelhouse, as SVG files: one per mood, plus a blink frame per mood, a second
wing frame for the moods that flap, and an app-logo variant.

The drawing is made of shared parts (tail, body, wings, head, eyes, beak, hat, ribbon, energy-drink can), so every mood
is the same bird. Only plain SVG is used (paths, ellipses, gradients, no filters, masks or clip paths), because Qt's
SVG renderer (QtSvg, SVG Tiny 1.2) draws the files inside the app.

    python3 tools/mascot.py            # writes qt/mascot/*.svg and the fallbacks qt/mascot/webp/*.webp
    python3 tools/mascot.py --png DIR  # also renders PNGs into DIR with rsvg-convert (to look at them)
"""
import math
import os
import subprocess
import sys

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "qt", "mascot")

# ---- colours: a hyacinth macaw (cobalt blue, yellow eye-ring and lappets, dark grey beak) ----
INK = "#142152"          # the outline
BLUE = "#3567dc"
BLUE_LIGHT = "#5b8cf5"
BLUE_DARK = "#2347ad"
BLUE_DEEP = "#1a3690"
BELLY = "#4a7ef0"
YELLOW = "#ffd43b"
YELLOW_DARK = "#f0ae1c"
BEAK = "#3c414e"
BEAK_LIGHT = "#5d6475"
BEAK_DARK = "#262932"
MOUTH = "#4a1426"
TONGUE = "#e5677e"
WHITE = "#ffffff"
PUPIL = "#16182a"
HAT = "#1d1e26"
HAT_LIGHT = "#3a3c4a"
GREEN = "#1fa64a"
GOLD = "#f4b52f"
GOLD_LIGHT = "#ffe27a"
CAN = "#26b04e"
CAN_DARK = "#17863a"
SILVER = "#cfd5de"
TEAR = "#8fd3ff"

W = 5      # main outline width
W2 = 3.5   # details

EYE_L = (94, 98)
EYE_R = (162, 98)
RING = 19
BALL = 12.5


def f(x):
    return ("%.2f" % x).rstrip("0").rstrip(".")


def defs():
    return f"""<defs>
  <radialGradient id="gHead" cx="0.38" cy="0.32" r="0.75">
    <stop offset="0" stop-color="{BLUE_LIGHT}"/><stop offset="0.55" stop-color="{BLUE}"/><stop offset="1" stop-color="{BLUE_DARK}"/>
  </radialGradient>
  <radialGradient id="gBody" cx="0.4" cy="0.3" r="0.8">
    <stop offset="0" stop-color="{BLUE_LIGHT}"/><stop offset="0.5" stop-color="{BLUE}"/><stop offset="1" stop-color="{BLUE_DARK}"/>
  </radialGradient>
  <radialGradient id="gBelly" cx="0.5" cy="0.35" r="0.7">
    <stop offset="0" stop-color="#6d98f7"/><stop offset="1" stop-color="{BELLY}"/>
  </radialGradient>
  <linearGradient id="gWing" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="{BLUE}"/><stop offset="1" stop-color="{BLUE_DEEP}"/>
  </linearGradient>
  <linearGradient id="gTail" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="{BLUE_DARK}"/><stop offset="1" stop-color="{BLUE_DEEP}"/>
  </linearGradient>
  <linearGradient id="gBeak" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="{BEAK_LIGHT}"/><stop offset="0.6" stop-color="{BEAK}"/><stop offset="1" stop-color="{BEAK_DARK}"/>
  </linearGradient>
  <linearGradient id="gCan" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="{CAN_DARK}"/><stop offset="0.35" stop-color="#3fd06a"/><stop offset="1" stop-color="{CAN_DARK}"/>
  </linearGradient>
  <linearGradient id="gHat" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="{HAT_LIGHT}"/><stop offset="0.4" stop-color="{HAT}"/><stop offset="1" stop-color="#0d0d12"/>
  </linearGradient>
  <radialGradient id="gMedal" cx="0.4" cy="0.35" r="0.7">
    <stop offset="0" stop-color="{GOLD_LIGHT}"/><stop offset="1" stop-color="{GOLD}"/>
  </radialGradient>
</defs>"""


# ---------------------------------------------------------------- body parts

def tail():
    return f"""<g id="tail">
  <path d="M146 196 C178 214 204 236 226 254 L200 254 C182 240 164 228 136 214 Z" fill="url(#gTail)" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>
  <path d="M132 204 C158 222 176 238 192 254 L170 254 C156 242 140 230 124 218 Z" fill="{BLUE_DARK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>
</g>"""


def feet():
    out = []
    for cx in (108, 148):
        out.append(f"""<path d="M{cx-14} 240 Q{cx-14} 230 {cx-6} 230 L{cx+6} 230 Q{cx+14} 230 {cx+14} 240 Q{cx+10} 246 {cx+5} 241 Q{cx} 247 {cx-5} 241 Q{cx-10} 246 {cx-14} 240 Z" fill="{BEAK}" stroke="{INK}" stroke-width="{W2}" stroke-linejoin="round"/>""")
    return "<g id=\"feet\">" + "".join(out) + "</g>"


def body():
    return f"""<g id="body">
  <ellipse cx="128" cy="186" rx="58" ry="54" fill="url(#gBody)" stroke="{INK}" stroke-width="{W}"/>
  <ellipse cx="128" cy="196" rx="36" ry="38" fill="url(#gBelly)"/>
  <path d="M108 214 q6 5 12 0 M126 222 q6 5 12 0 M116 232 q6 4 12 0" fill="none" stroke="{BLUE_DARK}" stroke-width="2.5" stroke-linecap="round" opacity="0.6"/>
</g>"""


WING = ("M0 0 C-17 4 -26 28 -23 50 L-19 62 L-13 55 L-8 68 L-2 58 L5 68 C13 50 15 22 9 3 Z")
WING_LINES = "M-12 22 Q-14 40 -13 52 M-3 26 Q-4 44 -6 60 M4 22 Q5 40 3 58"


def wing(side, angle, pivot=None, scale=1.0):
    """A wing hinged at the shoulder. side -1 = viewer's left, 1 = viewer's right; angle 0 points down, positive
    angles lift the wing outwards (up to ~160 = straight up)."""
    px, py = pivot or ((86, 164) if side < 0 else (170, 164))
    flip = "" if side < 0 else " scale(-1 1)"
    return f"""<g transform="translate({px} {py}){flip} rotate({f(angle)}) scale({f(scale)})">
  <path d="{WING}" fill="url(#gWing)" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>
  <path d="{WING_LINES}" fill="none" stroke="{BLUE_DEEP}" stroke-width="2.5" stroke-linecap="round" opacity="0.8"/>
</g>"""


def wing_hand(x, y, angle=0):
    """Two feather tips of a wing wrapped round something (the can), drawn over it."""
    return f"""<g transform="translate({x} {y}) rotate({f(angle)})">
  <path d="M-6 -8 Q8 -10 9 -4 Q10 1 0 0 Z" fill="{BLUE}" stroke="{INK}" stroke-width="{W2-0.5}" stroke-linejoin="round"/>
  <path d="M-6 0 Q8 -1 9 5 Q9 10 -1 8 Z" fill="{BLUE}" stroke="{INK}" stroke-width="{W2-0.5}" stroke-linejoin="round"/>
</g>"""


def can(x, y, angle=0, scale=1.0):
    """The unbranded energy drink: a green can with a lightning bolt and the word ARARA."""
    return f"""<g transform="translate({x} {y}) rotate({f(angle)}) scale({f(scale)})">
  <rect x="-12" y="-19" width="24" height="38" rx="4" fill="url(#gCan)" stroke="{INK}" stroke-width="{W2}"/>
  <rect x="-12" y="-19" width="24" height="5" rx="2" fill="{SILVER}" stroke="{INK}" stroke-width="2"/>
  <rect x="-12" y="14" width="24" height="5" rx="2" fill="{SILVER}" stroke="{INK}" stroke-width="2"/>
  <path d="M2 -12 L-6 0 L-1 0 L-4 9 L5 -3 L0 -3 Z" fill="{YELLOW}" stroke="{INK}" stroke-width="1.4" stroke-linejoin="round"/>
  <text x="0" y="13" font-family="Arial, Helvetica, sans-serif" font-size="5.4" font-weight="bold" fill="{WHITE}" text-anchor="middle" letter-spacing="0.2">ARARA</text>
</g>"""


def ribbon():
    return f"""<g id="ribbon">
  <path d="M104 158 L118 156 L132 196 L122 200 Z" fill="{GREEN}" stroke="{INK}" stroke-width="{W2}" stroke-linejoin="round"/>
  <path d="M110 157 L114 157 L127 197 L125 198 Z" fill="{YELLOW}"/>
  <path d="M152 158 L138 156 L124 196 L134 200 Z" fill="{GREEN}" stroke="{INK}" stroke-width="{W2}" stroke-linejoin="round"/>
  <path d="M146 157 L142 157 L129 197 L131 198 Z" fill="{YELLOW}"/>
  <circle cx="128" cy="204" r="13" fill="url(#gMedal)" stroke="{INK}" stroke-width="{W2}"/>
  <path d="M128 196 L130.4 201.2 L136 201.8 L131.8 205.4 L133 211 L128 208.2 L123 211 L124.2 205.4 L120 201.8 L125.6 201.2 Z" fill="{GOLD_LIGHT}" stroke="#b07a10" stroke-width="1.2" stroke-linejoin="round"/>
</g>"""


# ---------------------------------------------------------------- the head

def crest():
    # three feathers sticking up from the top of the head (left of the hat)
    return f"""<path d="M98 54 C90 40 92 30 100 24 C102 36 106 44 110 50 C108 36 112 28 120 24 C120 36 120 44 122 50 Z" fill="{BLUE}" stroke="{INK}" stroke-width="{W2+0.5}" stroke-linejoin="round"/>"""


def head_shape():
    return f"""<ellipse cx="128" cy="106" rx="68" ry="60" fill="url(#gHead)" stroke="{INK}" stroke-width="{W}"/>
  <path d="M74 132 Q80 150 98 158" fill="none" stroke="{BLUE_DARK}" stroke-width="3" stroke-linecap="round" opacity="0.5"/>
  <path d="M182 132 Q176 150 158 158" fill="none" stroke="{BLUE_DARK}" stroke-width="3" stroke-linecap="round" opacity="0.5"/>"""


def segment(cx, cy, r, d):
    """The part of a circle above the line y = cy + d (an eyelid)."""
    d = max(-r + 0.01, min(r - 0.01, d))
    w = math.sqrt(r * r - d * d)
    large = 1 if d > 0 else 0
    return f"M{f(cx - w)} {f(cy + d)} A{f(r)} {f(r)} 0 {large} 1 {f(cx + w)} {f(cy + d)} Z", (cx - w, cy + d, cx + w, cy + d)


def eye(cx, cy, kind, lid=None, tilt=0, look=(0, 0), side=-1):
    """kind: open | lid | happy | closed | squeeze | wide. lid: how far the upper lid comes down (-BALL..BALL),
    tilt: degrees the lid line leans (positive = the inner end lower, angry/sad shapes use the sign per side)."""
    parts = [f'<circle cx="{cx}" cy="{cy}" r="{RING}" fill="{YELLOW}" stroke="{INK}" stroke-width="{W2+0.5}"/>']
    if kind == "happy":
        parts.append(f'<path d="M{cx-9} {cy+4} Q{cx} {cy-9} {cx+9} {cy+4}" fill="none" stroke="{PUPIL}" stroke-width="5" stroke-linecap="round"/>')
        return "".join(parts)
    if kind == "squeeze":
        # > < eyes (a big yawn, a laugh)
        s = -side
        parts.append(f'<path d="M{cx-7*s} {cy-7} L{cx+6*s} {cy} L{cx-7*s} {cy+7}" fill="none" stroke="{PUPIL}" stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round"/>')
        return "".join(parts)
    r = BALL + (2 if kind == "wide" else 0)
    parts.append(f'<circle cx="{cx}" cy="{cy}" r="{f(r)}" fill="{WHITE}" stroke="{INK}" stroke-width="2.5"/>')
    if kind == "closed":
        parts.append(f'<circle cx="{cx}" cy="{cy}" r="{f(r)}" fill="{YELLOW_DARK}" stroke="{INK}" stroke-width="2.5"/>')
        parts.append(f'<path d="M{cx-10} {cy+1} Q{cx} {cy+9} {cx+10} {cy+1}" fill="none" stroke="{PUPIL}" stroke-width="4" stroke-linecap="round"/>')
        return "".join(parts)
    pr = 5 if kind == "wide" else 7.2
    px, py = cx + look[0], cy + 1.5 + look[1]
    parts.append(f'<circle cx="{f(px)}" cy="{f(py)}" r="{pr}" fill="{PUPIL}"/>')
    parts.append(f'<circle cx="{f(px+2.6)}" cy="{f(py-3)}" r="2.6" fill="{WHITE}"/>')
    if kind == "lid" and lid is not None:
        path, (x1, y1, x2, y2) = segment(cx, cy, r, lid)
        rot = f' transform="rotate({f(tilt * -side)} {cx} {cy})"'
        parts.append(f'<g{rot}><path d="{path}" fill="{YELLOW_DARK}"/>'
                     f'<path d="M{f(x1)} {f(y1)} L{f(x2)} {f(y2)}" stroke="{PUPIL}" stroke-width="3.2" stroke-linecap="round"/></g>')
        parts.append(f'<circle cx="{cx}" cy="{cy}" r="{f(r)}" fill="none" stroke="{INK}" stroke-width="2.5"/>')
    return "".join(parts)


def brows(kind):
    """Little feather brows above the eye rings: flat | sad | up | think | angry | none"""
    if kind == "none":
        return ""
    lx, ly = EYE_L
    rx, ry = EYE_R
    y = ly - RING - 8
    shapes = {
        "flat": ((lx - 12, y + 2, lx + 10, y + 2), (rx - 10, y + 2, rx + 12, y + 2)),
        "soft": ((lx - 11, y + 1, lx + 9, y - 2), (rx - 9, y - 2, rx + 11, y + 1)),
        "sad": ((lx - 12, y + 6, lx + 10, y - 3), (rx - 10, y - 3, rx + 12, y + 6)),
        "up": ((lx - 12, y - 4, lx + 10, y - 6), (rx - 10, y - 6, rx + 12, y - 4)),
        "think": ((lx - 12, y + 3, lx + 10, y + 3), (rx - 10, y - 8, rx + 12, y - 4)),
    }[kind]
    out = []
    for (x1, y1, x2, y2) in shapes:
        mx, my = (x1 + x2) / 2, (y1 + y2) / 2 - 3
        out.append(f'<path d="M{f(x1)} {f(y1)} Q{f(mx)} {f(my)} {f(x2)} {f(y2)}" fill="none" stroke="{BLUE_DEEP}" stroke-width="6" stroke-linecap="round"/>')
    return "".join(out)


def beak(kind):
    """closed | smile | open (cheer, happy) | yawn (very open) | small (surprised 'o') | frown"""
    up_closed = (f'<path d="M106 110 Q128 96 150 110 C160 122 156 146 134 162 Q128 166 126 158 '
                 f'C116 150 98 128 106 110 Z" fill="url(#gBeak)" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
                 f'<path d="M114 112 Q126 106 140 112" fill="none" stroke="{BEAK_LIGHT}" stroke-width="3" stroke-linecap="round" opacity="0.8"/>')
    up_open = (f'<path d="M106 108 Q128 94 150 108 C158 118 154 134 136 146 Q129 150 127 143 '
               f'C116 136 100 122 106 108 Z" fill="url(#gBeak)" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
               f'<path d="M114 110 Q126 104 140 110" fill="none" stroke="{BEAK_LIGHT}" stroke-width="3" stroke-linecap="round" opacity="0.8"/>')
    if kind in ("closed", "smile", "frown"):
        bend = {"closed": 8, "smile": 14, "frown": -2}[kind]
        lower = f'<path d="M108 128 Q112 158 128 162 Q144 158 148 128 Z" fill="{BEAK_DARK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>'
        # the yellow lappets at the base of the lower beak: the macaw's "smile"
        # the yellow lappets at the base of the lower beak, one each side: the macaw's "smile" (bend lifts the ends)
        lap = ""
        for sx in (-1, 1):
            def X(v):
                return f(128 + sx * v)
            lap += (f'<path d="M{X(34)} {f(112 - bend*0.5)} Q{X(36)} {f(136)} {X(14)} {f(154)} L{X(9)} {f(144)} '
                    f'Q{X(24)} {f(132)} {X(22)} {f(114 - bend*0.3)} Z" fill="{YELLOW}" stroke="{INK}" stroke-width="{W2}" stroke-linejoin="round"/>')
        return lower + lap + up_closed
    depth = {"open": 172, "yawn": 186, "small": 166}[kind]
    half = {"open": 22, "yawn": 24, "small": 16}[kind]
    lower = (f'<path d="M{128-half} 126 Q{128-half-2} {depth-8} 128 {depth} Q{128+half+2} {depth-8} {128+half} 126 '
             f'Q128 134 {128-half} 126 Z" fill="{BEAK_DARK}" stroke="{INK}" stroke-width="{W}" stroke-linejoin="round"/>')
    mouth_h = (depth - 130) * 0.42
    inside = (f'<ellipse cx="128" cy="{f(138 + mouth_h*0.6)}" rx="{f(half*0.62)}" ry="{f(mouth_h)}" fill="{MOUTH}"/>'
              f'<ellipse cx="128" cy="{f(140 + mouth_h*1.15)}" rx="{f(half*0.42)}" ry="{f(mouth_h*0.45)}" fill="{TONGUE}"/>')
    lap = (f'<path d="M{128-half-6} 122 Q{128-half} 136 {128-half+4} 140 L{128-half+8} 128 Z" fill="{YELLOW}" stroke="{INK}" stroke-width="{W2-0.5}" stroke-linejoin="round"/>'
           f'<path d="M{128+half+6} 122 Q{128+half} 136 {128+half-4} 140 L{128+half-8} 128 Z" fill="{YELLOW}" stroke="{INK}" stroke-width="{W2-0.5}" stroke-linejoin="round"/>')
    return lower + inside + lap + up_open


def hat(tilt=14, dy=0):
    """The little black top hat, sitting on the right of the head."""
    return f"""<g transform="translate(154 {52+dy}) rotate({f(tilt)})">
  <path d="M-17 -4 L-14 -38 Q0 -42 14 -38 L17 -4 Z" fill="url(#gHat)" stroke="{INK}" stroke-width="{W2+0.5}" stroke-linejoin="round"/>
  <path d="M-15.8 -19 L15.8 -19 L16.5 -9 L-16.5 -9 Z" fill="{GREEN}" stroke="{INK}" stroke-width="2.5" stroke-linejoin="round"/>
  <path d="M-16 -14 L16 -14" stroke="{YELLOW}" stroke-width="2.6"/>
  <ellipse cx="0" cy="-2" rx="30" ry="7" fill="{HAT}" stroke="{INK}" stroke-width="{W2+0.5}"/>
  <path d="M-9 -34 L-10 -16" stroke="#5a5d6e" stroke-width="3" stroke-linecap="round" opacity="0.8"/>
</g>"""


def cheeks(alpha=0.35):
    return (f'<ellipse cx="78" cy="128" rx="9" ry="5" fill="#ff7aa8" opacity="{alpha}"/>'
            f'<ellipse cx="178" cy="128" rx="9" ry="5" fill="#ff7aa8" opacity="{alpha}"/>')


def head(eyes, brow="flat", mouth="closed", tilt=0, hat_tilt=14, blush=False, hat_dy=0):
    le, re = eyes
    inner = [crest(), head_shape(), cheeks() if blush else "", brows(brow), le, re, beak(mouth), hat(hat_tilt, hat_dy)]
    return f'<g id="head" transform="rotate({f(tilt)} 128 150)">' + "".join(inner) + "</g>"


def eyes_of(kind, **kw):
    return (eye(EYE_L[0], EYE_L[1], kind, side=-1, **kw), eye(EYE_R[0], EYE_R[1], kind, side=1, **kw))


# ---------------------------------------------------------------- extras

def zzz():
    def z(x, y, s, sw):
        return (f'<path d="M{x} {y} L{x+s} {y} L{x} {y+s} L{x+s} {y+s}" fill="none" stroke="{INK}" stroke-width="{sw+3}" stroke-linecap="round" stroke-linejoin="round"/>'
                f'<path d="M{x} {y} L{x+s} {y} L{x} {y+s} L{x+s} {y+s}" fill="none" stroke="#9fc0ff" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round"/>')
    return z(44, 46, 20, 5) + z(22, 18, 14, 4) + z(8, 2, 9, 3)


def sparkle(x, y, s, color=YELLOW):
    return (f'<path d="M{x} {y-s} Q{x+s*0.18} {y-s*0.18} {x+s} {y} Q{x+s*0.18} {y+s*0.18} {x} {y+s} '
            f'Q{x-s*0.18} {y+s*0.18} {x-s} {y} Q{x-s*0.18} {y-s*0.18} {x} {y-s} Z" fill="{color}" stroke="{INK}" stroke-width="2" stroke-linejoin="round"/>')


def tear(x, y):
    return f'<path d="M{x} {y} Q{x+7} {y+11} {x} {y+15} Q{x-7} {y+11} {x} {y} Z" fill="{TEAR}" stroke="{INK}" stroke-width="2.2" stroke-linejoin="round"/>'


def bang(x, y, angle):
    return (f'<g transform="translate({x} {y}) rotate({angle})"><path d="M0 -14 L0 2" stroke="{INK}" stroke-width="7" stroke-linecap="round"/>'
            f'<path d="M0 -14 L0 2" stroke="{YELLOW}" stroke-width="3.5" stroke-linecap="round"/>'
            f'<circle cx="0" cy="10" r="3.6" fill="{YELLOW}" stroke="{INK}" stroke-width="2"/></g>')


def dots():
    return (f'<circle cx="206" cy="60" r="5" fill="{WHITE}" stroke="{INK}" stroke-width="2.5"/>'
            f'<circle cx="222" cy="42" r="7.5" fill="{WHITE}" stroke="{INK}" stroke-width="2.5"/>'
            f'<circle cx="238" cy="20" r="11" fill="{WHITE}" stroke="{INK}" stroke-width="2.5"/>'
            f'<path d="M234 15 Q238 9 242 14 Q243 19 238 21 L238 24" fill="none" stroke="{INK}" stroke-width="2.6" stroke-linecap="round"/>'
            f'<circle cx="238" cy="28.5" r="1.6" fill="{INK}"/>')


def motion(x, y, flip=1):
    return (f'<path d="M{x} {y} q{8*flip} -6 {14*flip} -2 M{x+4*flip} {y+12} q{8*flip} -4 {14*flip} 0" fill="none" '
            f'stroke="{INK}" stroke-width="3" stroke-linecap="round" opacity="0.6"/>')


def confetti():
    bits = [(40, 40, "#ff6b6b", 20), (214, 92, GREEN, -30), (30, 110, YELLOW, 45), (226, 150, "#ff9f43", 10), (52, 20, "#54a0ff", -15)]
    return "".join(f'<rect x="{x-4}" y="{y-2.5}" width="8" height="5" rx="1.5" fill="{c}" stroke="{INK}" stroke-width="1.5" transform="rotate({a} {x} {y})"/>' for x, y, c, a in bits)


# ---------------------------------------------------------------- moods

SLEEPY = 0.5   # how far the lids hang in the idle look (the tired-but-kind charm)


def compose(mood, blink=False, frame=0):
    """The whole bird for a mood. blink = the same pose with the eyes shut; frame = second wing position."""
    if mood in ACTIONS:
        return compose_action(mood, blink, frame)
    back, front, extra = [], [], []
    # wings by default hang at the sides
    wl, wr = wing(-1, 12), wing(1, 12)
    eyes = eyes_of("lid", lid=-BALL * SLEEPY)
    brow, mouth, tilt, hat_tilt, blush, hat_dy = "flat", "closed", 0, 14, False, 0
    holding = False

    if mood == "idle":
        mouth, brow = "smile", "soft"
        holding = True
    elif mood == "happy":
        eyes = eyes_of("happy")
        mouth, blush = "open", True
        wl, wr = wing(-1, 30), wing(1, 30)
        extra += [sparkle(40, 66, 11), sparkle(220, 112, 8), sparkle(30, 150, 7)]
    elif mood == "cheer":
        eyes = eyes_of("happy") if frame == 0 else eyes_of("open")
        mouth, blush, brow = "open", True, "up"
        a = 150 if frame == 0 else 128
        wl, wr = wing(-1, a), wing(1, a)
        extra += [confetti(), sparkle(26, 86, 9), sparkle(230, 70, 10)]
    elif mood == "sad":
        eyes = (eye(*EYE_L, "lid", lid=-BALL * 0.25, tilt=-16, side=-1, look=(0, 2)),
                eye(*EYE_R, "lid", lid=-BALL * 0.25, tilt=-16, side=1, look=(0, 2)))
        brow, mouth, hat_tilt, hat_dy = "sad", "frown", 4, 4
        wl, wr = wing(-1, 4), wing(1, 4)
        extra.append(tear(EYE_L[0] - 8, EYE_L[1] + 20))
    elif mood == "tired":
        eyes = eyes_of("squeeze")
        brow, mouth, tilt, hat_tilt = "sad", "yawn", -4, 22
        holding = True
        extra.append(tear(EYE_R[0] + 20, EYE_R[1] + 8))
    elif mood == "sleep":
        eyes = eyes_of("closed")
        brow, mouth, tilt, hat_tilt, blush = "flat", "closed", 9, 26, True
        wl, wr = wing(-1, 4), wing(1, 4)
        extra.append(zzz())
    elif mood == "think":
        eyes = (eye(*EYE_L, "lid", lid=-BALL * 0.35, look=(4, -4), side=-1),
                eye(*EYE_R, "lid", lid=-BALL * 0.35, look=(4, -4), side=1))
        brow, mouth, tilt = "think", "closed", -3
        wr = wing(1, -118, pivot=(172, 172), scale=0.62)
        extra.append(dots())
    elif mood == "wave":
        eyes = eyes_of("lid", lid=-BALL * 0.25)
        mouth, blush = "open", True
        a = 122 if frame == 0 else 94
        wr = wing(1, a)
        extra.append(motion(228, 92 if frame == 0 else 128, 1))
    elif mood == "surprised":
        eyes = eyes_of("wide")
        brow, mouth, hat_tilt, hat_dy = "up", "small", 8, -8
        wl, wr = wing(-1, 48), wing(1, 48)
        extra += [bang(42, 50, -18), bang(28, 80, -32)]
    else:
        raise ValueError(mood)

    if blink and mood not in ("happy", "sleep"):
        if mood == "cheer" and frame == 0:
            pass
        elif mood == "tired":
            pass
        else:
            eyes = eyes_of("closed")

    if holding:
        # the right wing comes forward with the can; the can sits in front of the belly on the right
        wr = wing(1, -4, pivot=(170, 166))
        front.append(can(182, 208, angle=-8, scale=1.2))
        front.append(wing_hand(170, 214, angle=-8))

    order = [tail(), feet(), wl, wr if mood != "think" else "", body(), ribbon()]
    order += front
    order.append(head(eyes, brow, mouth, tilt, hat_tilt, blush, hat_dy))
    if mood == "think":
        # the wing goes up to the chin, in front of the body
        order.append(wr)
    order += extra
    return svg("".join(order))


# ---------------------------------------------------------------- action poses (the macaw on the move)
# These are used by the animations of the app (MascotStage.qml, the banners, the flashcards over the video): the bird
# flies in, peeks at a subtitle, asks for silence, pulls a banner into view, pushes a card away.

SWEAT = "#bfe6ff"


def sweat(x, y, s=1.0):
    return (f'<path d="M{x} {y} Q{f(x+6*s)} {f(y+10*s)} {x} {f(y+14*s)} Q{f(x-6*s)} {f(y+10*s)} {x} {y} Z" fill="{SWEAT}" '
            f'stroke="{INK}" stroke-width="2" stroke-linejoin="round"/>')


def wind(x, y, flip=1, n=3, gap=13, length=22):
    """Short speed lines behind something moving (flip -1 draws them to the left)."""
    out = []
    for i in range(n):
        yy = y + i * gap
        ll = length - abs(i - (n - 1) / 2) * 6
        out.append(f'M{x} {yy} l{f(ll*flip)} 0')
    return f'<path d="{" ".join(out)}" fill="none" stroke="{INK}" stroke-width="3.2" stroke-linecap="round" opacity="0.45"/>'


def magnifier(cx, cy, r, handle_to):
    """The rim of a magnifying glass round (cx, cy) and its handle; the lens itself is drawn by the caller (it sits over
    the enlarged eye)."""
    hx, hy = handle_to
    ang = math.atan2(hy - cy, hx - cx)
    sx, sy = cx + math.cos(ang) * (r + 2), cy + math.sin(ang) * (r + 2)
    return (f'<path d="M{f(sx)} {f(sy)} L{f(hx)} {f(hy)}" stroke="{INK}" stroke-width="15" stroke-linecap="round"/>'
            f'<path d="M{f(sx)} {f(sy)} L{f(hx)} {f(hy)}" stroke="#8a5a2b" stroke-width="9" stroke-linecap="round"/>'
            f'<path d="M{f(sx + (hx-sx)*0.15)} {f(sy + (hy-sy)*0.15 - 2)} L{f(sx + (hx-sx)*0.8)} {f(sy + (hy-sy)*0.8 - 2)}" stroke="#c08a52" stroke-width="2.6" stroke-linecap="round"/>'
            f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{INK}" stroke-width="11"/>'
            f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{SILVER}" stroke-width="6"/>'
            f'<path d="M{f(cx - r*0.62)} {f(cy - r*0.5)} A{r*0.8} {r*0.8} 0 0 1 {f(cx - r*0.05)} {f(cy - r*0.8)}" fill="none" stroke="{WHITE}" stroke-width="4" stroke-linecap="round" opacity="0.85"/>')


def bird(parts, transform):
    return f'<g transform="{transform}">' + "".join(parts) + "</g>"


def compose_action(mood, blink=False, frame=0):
    if mood == "fly":
        # wings spread wide (frame b: wings up, the other half of the flap), feet tucked, leaning into the flight
        a = 104 if frame == 0 else 152
        sc = 1.32
        wl, wr = wing(-1, a, scale=sc), wing(1, a, scale=sc)
        eyes = eyes_of("closed") if blink else eyes_of("lid", lid=-BALL * 0.2, look=(0, 2))
        parts = [tail(), wl, wr, body(), ribbon(), head(eyes, "up", "smile", 0, 18, True)]
        return svg(bird(parts, "translate(128 130) rotate(-7) scale(0.8) translate(-128 -142)"))

    if mood == "shh":
        # a wing tip held up in front of the beak ("shh, no subtitles now"), the eyes glancing to one side
        look = (-6, -1)
        eyes = eyes_of("closed") if blink else (eye(*EYE_L, "lid", lid=-BALL * 0.45, look=look, side=-1),
                                                  eye(*EYE_R, "lid", lid=-BALL * 0.45, look=look, side=1))
        # the right wing comes up to the chin (as in "think") and its longest feather stands across the beak
        arm = wing(1, -122, pivot=(174, 176), scale=0.6)
        finger = (f'<path d="M134 168 C126 150 124 122 128 100 Q132 92 137 99 C141 122 142 148 146 166 Z" fill="url(#gWing)" '
                  f'stroke="{INK}" stroke-width="{W2+0.5}" stroke-linejoin="round"/>'
                  f'<path d="M134 152 Q131 128 132 108" fill="none" stroke="{BLUE_DEEP}" stroke-width="2.5" stroke-linecap="round" opacity="0.8"/>')
        parts = [tail(), feet(), wing(-1, 10), body(), ribbon(), head(eyes, "think", "closed", -2, 12, False)]
        parts += [arm, finger]
        out = "".join(parts)
        # little puffs of "shh" beside the beak
        out += (f'<path d="M188 122 q10 -4 18 0 M192 136 q12 0 20 6 M188 108 q8 -8 16 -8" fill="none" stroke="{INK}" '
                f'stroke-width="3" stroke-linecap="round" opacity="0.5"/>')
        return svg(out)

    if mood == "peek":
        # a magnifying glass in front of the left eye, which looks big through it; the other eye squints
        look = (-4, 0)
        lx, ly = EYE_L
        big = eye(lx, ly, "closed" if blink else "open", look=(-3, -1), side=-1)
        small = eye(*EYE_R, "closed" if blink else "lid", lid=-BALL * 0.75, look=look, side=1)
        lens_r = 37
        handle = (44, 176)
        lens = (f'<g transform="translate({lx} {ly}) scale(1.75) translate({-lx} {-ly})">{big}</g>'
                f'<circle cx="{lx}" cy="{ly}" r="{lens_r}" fill="#bfe3ff" opacity="0.22"/>')
        hold = wing(-1, 70, scale=0.78)
        parts = [tail(), feet(), hold, wing(1, 10), body(), ribbon(),
                 head((big, small), "think", "closed", 3, 18, False)]
        out = "".join(parts)
        out += f'<g transform="rotate(3 128 150)">{lens}{magnifier(lx, ly, lens_r, handle)}</g>'
        out += wing_hand(46, 172, angle=-30)
        return svg(out)

    if mood == "pull":
        # both wings stretched up, holding the edge of something above (a banner, a card) and pulling it down:
        # leaning back, eyes squeezed, a drop of sweat
        eyes = eyes_of("squeeze") if not blink else eyes_of("closed")
        jolt = 0 if frame == 0 else 4
        # the wings go up past the sides of the head, the tips curled over the edge at the top of the picture
        wl = wing(-1, 160 - jolt, pivot=(88, 160), scale=1.62)
        wr = wing(1, 160 - jolt, pivot=(168, 160), scale=1.62)
        parts = [tail(), feet(), wl, wr, body(), ribbon(), head(eyes, "sad", "closed", 0, 22, False)]
        # the feather tips at the ends of the wings, curled over the edge (the grip line is ~20% down the picture)
        parts += [wing_hand(54, 64 + jolt, angle=-90), wing_hand(202, 64 + jolt, angle=-90)]
        out = bird(parts, f"translate(128 {110 + jolt}) rotate(-4) scale(0.86) translate(-128 -128)")
        out += sweat(224, 100) + sweat(30, 118, 0.8)
        return svg(out)

    if mood == "push":
        # leaning forward, both wings stretched out to the right, pushing
        eyes = eyes_of("closed") if blink else (eye(*EYE_L, "lid", lid=-BALL * 0.35, tilt=14, look=(5, 0), side=-1),
                                                  eye(*EYE_R, "lid", lid=-BALL * 0.35, tilt=14, look=(5, 0), side=1))
        reach = 0 if frame == 0 else 7
        back = wing(1, 90, pivot=(168, 158), scale=1.2)
        front = wing(-1, -86, pivot=(118, 170), scale=1.45)
        parts = [tail(), feet(), back, body(), ribbon(), head(eyes, "flat", "closed", 6, 10, False), front]
        out = bird(parts, f"translate({110 + reach} 140) rotate(14) scale(0.8) translate(-128 -136)")
        out += wind(18, 118, -1) + sweat(40, 60, 0.9)
        return svg(out)

    raise ValueError(mood)


def svg(content, title="Arara"):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">\n'
            f'<title>{title}</title>\n{defs()}\n{content}\n</svg>\n')


def logo():
    """The app-logo variant: the macaw's head on a rounded square with a green to blue gradient."""
    bg = f"""<defs>
  <linearGradient id="gLogo" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#3ddc84"/><stop offset="0.5" stop-color="#1fb6a6"/><stop offset="1" stop-color="#2f62e0"/>
  </linearGradient>
  <radialGradient id="gGlow" cx="0.3" cy="0.2" r="0.8">
    <stop offset="0" stop-color="#ffffff" stop-opacity="0.35"/><stop offset="1" stop-color="#ffffff" stop-opacity="0"/>
  </radialGradient>
</defs>
<rect x="8" y="8" width="240" height="240" rx="56" fill="url(#gLogo)"/>
<rect x="8" y="8" width="240" height="240" rx="56" fill="url(#gGlow)"/>"""
    h = head(eyes_of("lid", lid=-BALL * SLEEPY), "flat", "smile", 0, 14)
    content = bg + f'<g transform="translate(128 146) scale(1.18) translate(-128 -106)">{h}</g>'
    return svg(content, "Arara logo")


MOODS = ["idle", "happy", "cheer", "sad", "tired", "sleep", "think", "wave", "surprised"]
# the poses of the animations (see compose_action)
ACTIONS = ["fly", "shh", "peek", "pull", "push"]
# moods with a second frame (-b): wings up / down, a tug, a shove
FLAPS = ["cheer", "wave", "fly", "pull", "push"]


def main():
    os.makedirs(OUT, exist_ok=True)
    files = {}
    for m in MOODS + ACTIONS:
        files[f"arara-{m}.svg"] = compose(m)
        files[f"arara-{m}-blink.svg"] = compose(m, blink=True)
        if m in FLAPS:
            files[f"arara-{m}-b.svg"] = compose(m, frame=1)
    files["arara-logo.svg"] = logo()
    for name, text in files.items():
        with open(os.path.join(OUT, name), "w", encoding="utf-8") as fh:
            fh.write(text)
    print("wrote", len(files), "files to", os.path.normpath(OUT))
    # WebP copies (384 px) for a computer whose Qt has no SVG image plugin: Mascot.qml falls back to them (the app
    # already needs the WebP plugin for the Images mode). Rendered with rsvg-convert, converted with ImageMagick.
    if "--no-fallback" not in sys.argv:
        fb = os.path.join(OUT, "webp")
        os.makedirs(fb, exist_ok=True)
        for name in files:
            png = os.path.join(fb, name.replace(".svg", ".png"))
            subprocess.run(["rsvg-convert", "-w", "384", "-h", "384", os.path.join(OUT, name), "-o", png], check=True)
            subprocess.run(["convert", png, "-quality", "92", "-define", "webp:alpha-quality=100",
                            png[:-4] + ".webp"], check=True)
            os.remove(png)
    if "--png" in sys.argv:
        dest = sys.argv[sys.argv.index("--png") + 1]
        os.makedirs(dest, exist_ok=True)
        for name in files:
            subprocess.run(["rsvg-convert", "-w", "256", "-h", "256", os.path.join(OUT, name), "-o",
                            os.path.join(dest, name.replace(".svg", ".png"))], check=True)


if __name__ == "__main__":
    main()
