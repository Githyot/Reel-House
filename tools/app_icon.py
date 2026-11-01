#!/usr/bin/env python3
"""Draws the app icon of ReelHouse: Arara, the blue macaw mascot (tools/mascot.py), smiling and eating popcorn from a
striped cinema bucket, and writes every size and format the platforms want.

    python3 tools/app_icon.py              # SVG sources + every PNG/ICO (needs rsvg-convert and ImageMagick)
    python3 tools/app_icon.py --svg-only   # only the SVG sources in qt/icon/

Outputs
  qt/icon/reelhouse-app.svg            the icon on its rounded square (Windows, Linux, the window icon)
  qt/icon/reelhouse-app-macos.svg      the same inside Apple's grid (a squircle with a margin and a soft shadow)
  qt/icon/reelhouse-foreground.svg     the macaw alone on a 108 dp canvas (Android adaptive foreground)
  qt/icon/reelhouse-background.svg     the background alone (Android adaptive background)
  qt/icon/reelhouse-round.svg          the icon on a disc (Android round icon, the loading screens)
  qt/icon/reelhouse-icon.png, reelhouse.ico, themes/reelhouse-<theme>-<256|512>.png and .svg
  qt/android/res/mipmap-*/ic_launcher*.png, drawable-*/splash_logo.png,
  drawable-xhdpi/banner.png (Android TV), drawable/icon.png (legacy)
  packaging/macos/reelhouse.iconset/*.png (16 to 1024 px, for iconutil)
  qt/ios/Assets.xcassets/AppIcon.appiconset/reelhouse-ios-1024.png (no alpha)

Plain SVG only (paths, circles, gradients, no filters, masks or clip paths), like the mascot, so QtSvg draws it too.
"""
import os
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mascot as m  # noqa: E402

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
ICON = os.path.join(ROOT, "qt", "icon")
ANDROID_RES = os.path.join(ROOT, "qt", "android", "res")

f = m.f
INK = m.INK
RED = "#ef3b4a"
RED_DARK = "#c42536"
CREAM = "#fff7e0"
CREAM_SHADE = "#f6dfa6"
BUTTER = "#ffc93c"
BG_TOP = "#ffd84d"       # a sunny yellow, the colour that makes the blue macaw and the red bucket pop
BG_BOTTOM = "#ff9d2e"
BG_DARK = "#1b1530"      # the app's own window colour (splash background)


def icon_defs():
    return f"""<defs>
  <linearGradient id="gIconBg" x1="0" y1="0" x2="0.35" y2="1">
    <stop offset="0" stop-color="{BG_TOP}"/><stop offset="1" stop-color="{BG_BOTTOM}"/>
  </linearGradient>
  <radialGradient id="gIconGlow" cx="0.5" cy="0.42" r="0.55">
    <stop offset="0" stop-color="#fff6c8" stop-opacity="0.95"/><stop offset="1" stop-color="#fff6c8" stop-opacity="0"/>
  </radialGradient>
  <linearGradient id="gBucketRed" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="{RED_DARK}"/><stop offset="0.35" stop-color="{RED}"/><stop offset="1" stop-color="{RED_DARK}"/>
  </linearGradient>
  <linearGradient id="gBucketWhite" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="#e9e3f0"/><stop offset="0.35" stop-color="#ffffff"/><stop offset="1" stop-color="#ddd6e6"/>
  </linearGradient>
</defs>"""


# ---------------------------------------------------------------- popcorn

def puff(x, y, s=1.0, rot=0):
    """One popped kernel: four bumps merged into one cloud (the outline is drawn under the fill, so the bumps join),
    a shade at the bottom and a dab of butter."""
    bumps = [(-6, 1, 7.5), (5, -3, 8), (1, -10, 6.5), (7, 6, 6), (-3, 8, 6)]
    outline = "".join(f'<circle cx="{f(bx)}" cy="{f(by)}" r="{f(r + 1.9)}" fill="{INK}"/>' for bx, by, r in bumps)
    fill = "".join(f'<circle cx="{f(bx)}" cy="{f(by)}" r="{f(r)}" fill="{CREAM}"/>' for bx, by, r in bumps)
    shade = (f'<path d="M-9 6 Q-2 13 9 9" fill="none" stroke="{CREAM_SHADE}" stroke-width="4" stroke-linecap="round"/>'
             f'<ellipse cx="2" cy="-4" rx="3.2" ry="2.4" fill="{BUTTER}" opacity="0.9"/>')
    return f'<g transform="translate({f(x)} {f(y)}) rotate({f(rot)}) scale({f(s)})">{outline}{fill}{shade}</g>'


def bucket(cx=128, top=196, bottom=262, wt=118, wb=88, stripes=5):
    """The striped cinema bucket (red and white), its rim and the heap of popcorn spilling over the top."""
    out = []
    heap = [(-44, 0, 1.0, -20), (-26, -10, 1.1, 10), (-6, -16, 1.15, -5), (16, -12, 1.1, 25), (36, -4, 1.05, -10),
            (48, 6, 0.95, 15), (-50, 10, 0.9, 30), (-16, -2, 1.1, 40), (6, -2, 1.1, -30), (28, 2, 1.05, 5)]
    out += [puff(cx + dx, top + dy, s, r) for dx, dy, s, r in heap]
    # the body of the bucket, cut into vertical stripes that follow the taper
    def at(t, y):
        w = wt + (wb - wt) * (y - top) / (bottom - top)
        return cx - w / 2 + t * w
    out.append(f'<path d="M{f(at(0, top))} {top} L{f(at(1, top))} {top} L{f(at(1, bottom))} {f(bottom - 6)} '
               f'Q{f(at(1, bottom))} {bottom} {f(at(1, bottom) - 6)} {bottom} L{f(at(0, bottom) + 6)} {bottom} '
               f'Q{f(at(0, bottom))} {bottom} {f(at(0, bottom))} {f(bottom - 6)} Z" fill="url(#gBucketWhite)"/>')
    n = stripes * 2 - 1
    for i in range(0, n, 2):
        t0, t1 = i / n, (i + 1) / n
        out.append(f'<path d="M{f(at(t0, top))} {top} L{f(at(t1, top))} {top} L{f(at(t1, bottom))} {bottom} '
                   f'L{f(at(t0, bottom))} {bottom} Z" fill="url(#gBucketRed)"/>')
    out.append(f'<path d="M{f(at(0, top))} {top} L{f(at(1, top))} {top} L{f(at(1, bottom))} {f(bottom - 6)} '
               f'Q{f(at(1, bottom))} {bottom} {f(at(1, bottom) - 6)} {bottom} L{f(at(0, bottom) + 6)} {bottom} '
               f'Q{f(at(0, bottom))} {bottom} {f(at(0, bottom))} {f(bottom - 6)} Z" fill="none" stroke="{INK}" '
               f'stroke-width="{m.W}" stroke-linejoin="round"/>')
    # the rim
    rx = wt / 2 + 5
    out.append(f'<rect x="{f(cx - rx)}" y="{top - 5}" width="{f(rx * 2)}" height="12" rx="6" fill="{RED}" '
               f'stroke="{INK}" stroke-width="{m.W - 0.5}"/>')
    out.append(f'<path d="M{f(cx - rx + 8)} {top - 1} L{f(cx + rx - 30)} {top - 1}" stroke="#ff8a95" stroke-width="3" '
               f'stroke-linecap="round"/>')
    # a little star on the front, a hint of the medal
    out.append(m.sparkle(cx, top + 34, 11, m.YELLOW))
    return "".join(out)


def macaw():
    """The macaw from the chest up, wings round the bucket, beaming, a kernel in the beak, kernels flying about.
    In the mascot's own 256 x 256 space."""
    eyes = (m.eye(*m.EYE_L, "happy", side=-1), m.eye(*m.EYE_R, "happy", side=1))
    head = m.head(eyes, brow="up", mouth="open", tilt=-5, hat_tilt=16, blush=True)
    # the kernel the bird is munching, at the tip of the open beak
    munch = puff(152, 156, 0.72, 18)
    flying = puff(48, 70, 0.75, -25) + puff(214, 96, 0.68, 30) + puff(30, 128, 0.55, 10)
    sparkles = m.sparkle(206, 40, 9) + m.sparkle(56, 30, 7) + m.sparkle(226, 150, 6)
    parts = [
        m.wing(-1, -22, pivot=(84, 168)),
        m.wing(1, -22, pivot=(172, 168)),
        m.body(),
        m.ribbon(),
        head,
        bucket(),
        m.wing_hand(76, 208, angle=-70),
        f'<g transform="translate(180 208) scale(-1 1)">{m.wing_hand(0, 0, angle=-70)}</g>',
        munch,
        flying,
        sparkles,
    ]
    return "".join(parts)


def svg(content, size=256, title="ReelHouse"):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" width="{size}" height="{size}">\n'
            f'<title>{title}</title>\n{m.defs()}\n{icon_defs()}\n{content}\n</svg>\n')


def background(x, y, w, h, r):
    return (f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" rx="{f(r)}" fill="url(#gIconBg)"/>'
            f'<rect x="{f(x)}" y="{f(y)}" width="{f(w)}" height="{f(h)}" rx="{f(r)}" fill="url(#gIconGlow)"/>')


def placed(scale, cx=128, cy=134):
    # the drawing spans y 8 (hat) .. 262 (bucket); 135 is its middle
    """The macaw scaled round its middle and centred on (cx, cy)."""
    return f'<g transform="translate({f(cx)} {f(cy)}) scale({f(scale)}) translate(-128 -135)">{macaw()}</g>'


def app_icon():
    """Full-bleed rounded square (Windows, Linux, the window icon)."""
    return svg(background(4, 4, 248, 248, 56) + placed(0.9, 128, 130), title="ReelHouse")


def macos_icon():
    """Apple's grid: the shape is 824/1024 of the canvas, centred, with a soft drop shadow under it."""
    s = 256 * 824 / 1024
    o = (256 - s) / 2
    shadow = "".join(f'<rect x="{f(o - k)}" y="{f(o + 3 + k * 0.6)}" width="{f(s + 2 * k)}" height="{f(s + 2 * k)}" '
                     f'rx="{f(s * 0.225 + k)}" fill="#000" opacity="0.05"/>' for k in (0, 1.5, 3))
    return svg(shadow + background(o, o, s, s, s * 0.225) + placed(0.76, 128, 130), title="ReelHouse")


def foreground():
    """Android adaptive foreground: 108 dp canvas; what matters must sit inside the 66 dp circle in the middle."""
    return svg(placed(0.64, 128, 128), title="ReelHouse foreground")


def background_only():
    return svg(background(0, 0, 256, 256, 0), title="ReelHouse background")


def round_icon():
    """The icon on a disc: Android's legacy round icon and the loading screens (Android 12+ shows the launcher icon in
    a circle while the app starts, so the app's own splash and loading screen use the same disc and the switch from
    one to the next does not show)."""
    disc = ('<circle cx="128" cy="128" r="122" fill="url(#gIconBg)"/>'
            '<circle cx="128" cy="128" r="122" fill="url(#gIconGlow)"/>')
    return svg(disc + placed(0.86, 128, 130), title="ReelHouse")


def tv_banner():
    """Android TV home-screen banner (320 x 180 dp at xhdpi): the macaw on the left, the name on the right."""
    content = (background(0, 0, 640, 360, 0) +
               f'<g transform="translate(146 184) scale(1.12) translate(-128 -135)">{macaw()}</g>'
               f'<text x="282" y="204" font-family="Inter Display, Inter, DejaVu Sans, sans-serif" font-weight="800" '
               f'font-size="62" fill="{INK}" letter-spacing="-1">ReelHouse</text>')
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 360" width="640" height="360">\n'
            f'<title>ReelHouse</title>\n{m.defs()}\n{icon_defs()}\n{content}\n</svg>\n')


# ---------------------------------------------------------------- rasterising

def monochrome(src_png, dst_png):
    """Android 13 themed icon: the system paints one colour through the alpha, so the dark outlines become see-through
    lines inside a solid silhouette (that keeps the eyes, the beak and the bucket readable)."""
    subprocess.run(["convert", src_png, "(", "+clone", "-colorspace", "gray", "-level", "8%,45%", ")",
                    "(", "-clone", "0", "-alpha", "extract", ")", "-delete", "0", "-compose", "multiply", "-composite",
                    "-background", "black", "-alpha", "shape", dst_png], check=True)


def png(src, dst, size, bg=None):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    cmd = ["rsvg-convert", "-w", str(size), "-h", str(size)]
    if bg:
        cmd += ["-b", bg]
    subprocess.run(cmd + [src, "-o", dst], check=True)


def main():
    files = {
        "reelhouse-app.svg": app_icon(),
        "reelhouse-app-macos.svg": macos_icon(),
        "reelhouse-foreground.svg": foreground(),
        "reelhouse-background.svg": background_only(),
        "reelhouse-round.svg": round_icon(),
    }
    for name, text in files.items():
        with open(os.path.join(ICON, name), "w", encoding="utf-8") as fh:
            fh.write(text)
    print("wrote", len(files), "SVG files to", ICON)
    if "--svg-only" in sys.argv:
        return

    app = os.path.join(ICON, "reelhouse-app.svg")
    mac = os.path.join(ICON, "reelhouse-app-macos.svg")
    fg = os.path.join(ICON, "reelhouse-foreground.svg")
    bgs = os.path.join(ICON, "reelhouse-background.svg")
    rnd = os.path.join(ICON, "reelhouse-round.svg")

    # the window icon and the per-theme copies (the window icon follows the theme; they are all the same now)
    png(app, os.path.join(ICON, "reelhouse-icon.png"), 256)
    # the disc of the loading screen inside the app (LoadingScreen.qml)
    png(rnd, os.path.join(ICON, "reelhouse-round-512.png"), 512)
    for theme in ("windows", "gnome", "android", "apple"):
        src = mac if theme == "apple" else app
        shutil.copyfile(src, os.path.join(ICON, "themes", f"reelhouse-{theme}.svg"))
        for size in (256, 512):
            png(src, os.path.join(ICON, "themes", f"reelhouse-{theme}-{size}.png"), size)

    # macOS: the iconset iconutil turns into reelhouse.icns (.github/workflows/macos.yml), every size drawn from the SVG
    iconset = os.path.join(ROOT, "packaging", "macos", "reelhouse.iconset")
    for size in (16, 32, 128, 256, 512):
        png(mac, os.path.join(iconset, f"icon_{size}x{size}.png"), size)
        png(mac, os.path.join(iconset, f"icon_{size}x{size}@2x.png"), size * 2)

    # Windows .ico: every size Explorer and the taskbar ask for
    with tempfile.TemporaryDirectory() as tmp:
        sizes = (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)
        pngs = []
        for s in sizes:
            p = os.path.join(tmp, f"{s}.png")
            png(app, p, s)
            pngs.append(p)
        subprocess.run(["convert", *pngs, os.path.join(ICON, "reelhouse.ico")], check=True)

    # iOS: 1024 px, no transparency (the store refuses alpha); iOS draws its own rounded mask
    ios = os.path.join(ROOT, "qt", "ios", "Assets.xcassets", "AppIcon.appiconset", "reelhouse-ios-1024.png")
    with tempfile.TemporaryDirectory() as tmp:
        full = os.path.join(tmp, "ios.svg")
        with open(full, "w", encoding="utf-8") as fh:
            fh.write(svg(background(0, 0, 256, 256, 0) + placed(0.94, 128, 132)))
        png(full, ios, 1024, bg=BG_TOP)
        subprocess.run(["convert", ios, "-alpha", "off", ios], check=True)

    # Android: adaptive layers (108 dp) + legacy square and round icons (48 dp) per density
    dens = {"mdpi": 1, "hdpi": 1.5, "xhdpi": 2, "xxhdpi": 3, "xxxhdpi": 4}
    with tempfile.TemporaryDirectory() as tmp:
        ban = os.path.join(tmp, "banner.svg")
        with open(ban, "w", encoding="utf-8") as fh:
            fh.write(tv_banner())
        os.makedirs(os.path.join(ANDROID_RES, "drawable-xhdpi"), exist_ok=True)
        subprocess.run(["rsvg-convert", "-w", "320", "-h", "180", ban, "-o",
                        os.path.join(ANDROID_RES, "drawable-xhdpi", "banner.png")], check=True)
        for d, k in dens.items():
            mip = os.path.join(ANDROID_RES, f"mipmap-{d}")
            png(fg, os.path.join(mip, "ic_launcher_foreground.png"), round(108 * k))
            png(bgs, os.path.join(mip, "ic_launcher_background.png"), round(108 * k))
            monochrome(os.path.join(mip, "ic_launcher_foreground.png"), os.path.join(mip, "ic_launcher_monochrome.png"))
            png(app, os.path.join(mip, "ic_launcher.png"), round(48 * k))
            png(rnd, os.path.join(mip, "ic_launcher_round.png"), round(48 * k))
            # the splash logo: 160 dp, drawn at its own size in the middle of the screen (never stretched)
            png(rnd, os.path.join(ANDROID_RES, f"drawable-{d}", "splash_logo.png"), round(160 * k))
    # the old drawable stays for the TV banner and anything that still names @drawable/icon
    png(app, os.path.join(ANDROID_RES, "drawable", "icon.png"), 512)
    print("wrote the PNG, ICO and Android files")


if __name__ == "__main__":
    main()
