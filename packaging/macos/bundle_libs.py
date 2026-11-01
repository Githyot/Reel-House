#!/usr/bin/env python3
"""Keeps the macOS app bundle self-contained (.github/workflows/macos.yml).

    python3 packaging/macos/bundle_libs.py fix   ReelHouse.app /opt/homebrew
    python3 packaging/macos/bundle_libs.py check ReelHouse.app

fix:   macdeployqt follows absolute library paths, but not the @rpath references that some Homebrew libraries use
       among themselves (libbrotlidec -> @rpath/libbrotlicommon.1.dylib). Every library still taken from outside the
       bundle is copied from Homebrew into Contents/Frameworks and the reference is pointed at the copy, again and
       again until nothing changes.
check: lists every reference that resolves neither inside the bundle nor to the system (/System, /usr/lib) and
       exits with 1 when there is one, so a .dmg that would only work on a Mac with Homebrew is never made.
"""
import glob
import os
import shutil
import subprocess
import sys

SYSTEM = ("/System/", "/usr/lib/")
MAGIC = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"}


def is_macho(p):
    try:
        with open(p, "rb") as f:
            return f.read(4) in MAGIC
    except OSError:
        return False


def otool(*args):
    return subprocess.run(["otool", *args], capture_output=True, text=True, errors="replace").stdout.splitlines()


def install_id(p):
    lines = [l.strip() for l in otool("-D", p)[1:] if l.strip() and not l.strip().endswith(":")]
    return lines[0] if lines else None


def deps(p):
    own, out = install_id(p), []
    for l in otool("-L", p)[1:]:
        l = l.strip()
        if l and not l.endswith(":"):
            d = l.split(" (")[0]
            if d != own and d not in out:
                out.append(d)
    return out


class Bundle:
    def __init__(self, app):
        self.app = os.path.realpath(app)
        self.contents = os.path.join(self.app, "Contents")
        self.exe_dir = os.path.join(self.contents, "MacOS")
        self.fw = os.path.join(self.contents, "Frameworks")
        self.main_rp = []
        for n in sorted(os.listdir(self.exe_dir)):
            p = os.path.join(self.exe_dir, n)
            if os.path.isfile(p) and not os.path.islink(p) and is_macho(p):
                self.main_rp += self.rpaths(p)

    def rpaths(self, p):
        here, lines, r = os.path.dirname(p), otool("-l", p), []
        for i, l in enumerate(lines):
            if "cmd LC_RPATH" in l:
                for k in lines[i + 1:i + 4]:
                    k = k.strip()
                    if k.startswith("path "):
                        r.append(k.split()[1].replace("@loader_path", here).replace("@executable_path", self.exe_dir))
        return r

    def binaries(self):
        for root, _, files in os.walk(self.contents):
            for n in sorted(files):
                p = os.path.join(root, n)
                if not os.path.islink(p) and is_macho(p):
                    yield p

    def inside(self, p):
        return os.path.exists(p) and os.path.realpath(p).startswith(self.contents + os.sep)

    def resolves(self, p, d):
        here = os.path.dirname(p)
        if d.startswith("@rpath/"):
            cands = [os.path.join(r, d[len("@rpath/"):]) for r in self.rpaths(p) + self.main_rp]
        else:
            cands = [d.replace("@executable_path", self.exe_dir).replace("@loader_path", here)]
        return any(self.inside(c) for c in cands)

    def missing(self):
        out, n = [], 0
        for p in self.binaries():
            n += 1
            for d in deps(p):
                if not d.startswith(SYSTEM) and not self.resolves(p, d):
                    out.append((p, d))
        return n, out

    def rel(self, p):
        return os.path.relpath(p, os.path.dirname(self.app))

    def fix(self, brew):
        for _ in range(12):
            _, todo = self.missing()
            if not todo:
                return
            changed = False
            for p, d in todo:
                name = os.path.basename(d)
                if ".framework/" in d:
                    print(f"left as it is (a framework): {self.rel(p)} -> {d}")
                    continue
                dst = os.path.join(self.fw, name)
                if not os.path.exists(dst):
                    cands = ([d] if d.startswith("/") else []) + [os.path.join(brew, "lib", name)] + \
                        sorted(glob.glob(os.path.join(brew, "opt", "*", "lib", name)))
                    src = next((c for c in cands if os.path.isfile(c)), None)
                    if not src:
                        print(f"not found anywhere: {self.rel(p)} -> {d}")
                        continue
                    os.makedirs(self.fw, exist_ok=True)
                    shutil.copy2(os.path.realpath(src), dst)
                    os.chmod(dst, 0o755)
                    subprocess.run(["install_name_tool", "-id", "@executable_path/../Frameworks/" + name, dst],
                                   capture_output=True)
                    print(f"copied {src}")
                    changed = True
                r = subprocess.run(["install_name_tool", "-change", d, "@executable_path/../Frameworks/" + name, p],
                                   capture_output=True, text=True)
                if r.returncode == 0:
                    print(f"pointed {self.rel(p)}: {d} -> @executable_path/../Frameworks/{name}")
                    changed = True
                else:
                    print(f"could not change {d} in {self.rel(p)}: {r.stderr.strip()[:200]}")
            if not changed:
                return

    def check(self):
        n, bad = self.missing()
        print(f"{n} Mach-O files checked, {len(bad)} missing libraries")
        for p, d in bad[:80]:
            print(f"ERROR missing: {self.rel(p)} -> {d}")
        return 1 if bad else 0


def main():
    if len(sys.argv) < 3 or sys.argv[1] not in ("fix", "check"):
        print(__doc__)
        return 2
    b = Bundle(sys.argv[2])
    if sys.argv[1] == "fix":
        b.fix(sys.argv[3] if len(sys.argv) > 3 else "/opt/homebrew")
        return 0
    return b.check()


if __name__ == "__main__":
    sys.exit(main())
