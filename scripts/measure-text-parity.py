#!/usr/bin/env python3
"""Manual harness: text-overlay preview-vs-export parity (Step B).

Generates ASS exactly like `write_text_overlays_ass()` (post-fix: corrected
Fontsize, letter Spacing, WrapStyle 2, halved outline) for each of the 10
font styles, renders with FFmpeg `ass` filter + bundled fontsdir, and measures
the ink bounding box via pixel scan.

Preview reference is headless Chrome DOM measurement with the same @font-face
files and the same CSS as `getTextLayerStyle()` (`white-space: pre`,
per-style `lineHeight`/`letterSpacing`). If Chrome is unavailable, falls back
to PIL FreeType measurement (less accurate for Cormorant/Caveat/Orbitron;
see text_overlay_parity_report.md) with a warning.

Usage:
    python scripts/measure-text-parity.py [--text "REVENANT GO BRRRRR"]
        [--size 48] [--width 1280] [--height 720] [--out tmp/parity]

Requires: ffmpeg with libass, Python PIL + fontTools, Chrome/Edge (optional
but recommended for the preview reference).
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONT_ROOT = os.path.join(REPO, "src-tauri", "resources", "fonts", "text-overlay")

# (style_key, family_dir, regular_file, ass_name, win/UPM, letterSpacing em)
STYLES = [
    ("clean", "fira-sans", "FiraSans-Regular.ttf", "Fira Sans", 1.2, 0.0),
    ("minimal", "lato", "Lato-Regular.ttf", "Lato", 1.416, 0.04),
    ("caption", "inter", "Inter-Regular.ttf", "Inter", 1.4302, 0.0),
    ("meme", "anton", "Anton-Regular.ttf", "Anton", 1.7334, 0.0),
    ("creator", "montserrat", "Montserrat-Regular.ttf", "Montserrat", 1.562, 0.0),
    ("gaming", "exo-2", "Exo2-Regular.ttf", "Exo 2", 1.469, 0.03),
    ("cyberpunk", "orbitron", "Orbitron-Regular.ttf", "Orbitron", 1.254, 0.03),
    ("cinematic", "cormorant-garamond", "CormorantGaramond-Regular.ttf",
     "Cormorant Garamond", 1.38, 0.0),
    ("retro", "bungee", "Bungee-Regular.ttf", "Bungee", 2.574, 0.0),
    ("handwritten", "caveat", "Caveat-Regular.ttf", "Caveat", 1.289, 0.0),
]

LINE_HEIGHT = {
    "meme": 0.95, "retro": 0.95, "handwritten": 1.05,
}


def esc_filter_path(p):
    return p.replace("\\", "/").replace(":", "\\:").replace("'", "\\'")


def format_ass_float(v):
    r = round(v * 100) / 100
    if r == int(r):
        return str(int(r))
    s = f"{r:.2f}".rstrip("0").rstrip(".")
    return s


def write_text_overlay_ass(path, entries, play_res_x, play_res_y):
    """Mirror of Rust write_text_overlays_ass() (post-fix)."""
    body = "[Script Info]\nScriptType: v4.00+\n"
    body += f"PlayResX: {play_res_x}\nPlayResY: {play_res_y}\n"
    body += "ScaledBorderAndShadow: yes\nWrapStyle: 2\n\n"
    body += ("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, "
             "SecondaryColour, OutlineColour, BackColour, Bold, Italic, "
             "Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, "
             "BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, "
             "MarginV, Encoding\n")
    for text, style, _x, _y in entries:
        body += (
            f"Style: {style['name']},{style['font_name']},{style['font_size']},"
            f"{style['primary']},&H000000FF,{style['outline_c']},"
            f"{style['back']},{style['bold']},{style['italic']},0,0,100,100,"
            f"{format_ass_float(style['spacing'])},0,1,"
            f"{format_ass_float(style['outline'])},0,5,0,0,0,1\n"
        )
    body += "\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, "
    body += "MarginR, MarginV, Effect, Text\n"
    for i, (text, style, x, y) in enumerate(entries):
        px = round(x * play_res_x)
        py = round(y * play_res_y)
        t = (text.replace("\\", "\\\\").replace("{", "\\{")
                 .replace("}", "\\}").replace("\r\n", "\\N")
                 .replace("\n", "\\N").replace("\r", "\\N"))
        body += (f"Dialogue: {i},0:00:00.00,0:00:05.00,{style['name']},,0,0,0,,"
                 f"{{\\an5\\pos({px},{py})}}{t}\n")
    with open(path, "w", encoding="utf-8") as f:
        f.write(body)


def render_ass(ass_path, fonts_dir, out_png, width, height):
    vf = f"ass='{esc_filter_path(ass_path)}':fontsdir='{esc_filter_path(fonts_dir)}'"
    cmd = ["ffmpeg", "-y", "-v", "error", "-f", "lavfi",
           "-i", f"color=c=black:s={width}x{height}:d=1:r=30",
           "-vf", vf, "-frames:v", "1", out_png]
    subprocess.run(cmd, check=True)


def measure_bbox(png_path, thresh=30):
    from PIL import Image
    im = Image.open(png_path).convert("RGB")
    w, h = im.size
    px = im.load()
    minx, miny, maxx, maxy = w, h, -1, -1
    for y in range(h):
        for x in range(w):
            r, g, b = px[x, y]
            if r > thresh or g > thresh or b > thresh:
                if x < minx:
                    minx = x
                if x > maxx:
                    maxx = x
                if y < miny:
                    miny = y
                if y > maxy:
                    maxy = y
    if maxx < 0:
        return (0, 0)
    return (maxx - minx + 1, maxy - miny + 1)


def find_chrome():
    for cand in [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ]:
        if os.path.isfile(cand):
            return cand
    for name in ["chrome", "google-chrome", "msedge", "chromium"]:
        p = shutil.which(name)
        if p:
            return p
    return None


def chrome_preview_widths(text, size, tmpdir):
    """DOM widths via headless Chrome using the bundled @font-face files."""
    chrome = find_chrome()
    if chrome is None:
        return None
    faces = []
    for key, famdir, regfile, _ass, _ratio, _ls in STYLES:
        src = os.path.join(FONT_ROOT, famdir, regfile).replace("\\", "/")
        uri = "file:///" + src.replace(" ", "%20").replace(":", "", 1) \
            if False else "file:///" + src.replace("\\", "/")
        # Build a file URI that Chrome accepts on all platforms.
        uri = "file:///" + src.replace("\\", "/").replace(" ", "%20")
        faces.append(f'@font-face {{ font-family: "P_{key}"; src: url("{uri}"); }}')
    ls_map = {k: ls for k, _d, _f, _a, _r, ls in STYLES}
    lh_map = dict(LINE_HEIGHT)
    html = ("<!DOCTYPE html><html><head><meta charset='utf-8'><style>"
            + "\n".join(faces)
            + "div.m{position:absolute;visibility:hidden;white-space:pre;}"
            + "</style></head><body><div id='out'>pending</div><script>\n"
            + "async function measure(family,px,text,lsPx,lh){\n"
            + " await document.fonts.load(`400 ${px}px \"${family}\"`,text);\n"
            + " const d=document.createElement('div');d.className='m';\n"
            + " d.style.fontFamily=`\"${family}\"`;d.style.fontSize=px+'px';\n"
            + " d.style.fontWeight='400';d.style.fontStyle='normal';\n"
            + " d.style.lineHeight=lh;if(lsPx)d.style.letterSpacing=lsPx+'px';\n"
            + " d.style.whiteSpace='pre';d.textContent=text;\n"
            + " document.body.appendChild(d);const w=d.offsetWidth;d.remove();return w;}\n"
            + "(async()=>{\n"
            + f" const TEXT={json.dumps(text)};const PX={size};\n"
            + f" const LS={json.dumps({k: size * v for k, v in ls_map.items() if v})};\n"
            + f" const LH={json.dumps(lh_map)};\n"
            + " const FAMS=" + json.dumps([k for k, *_ in STYLES]) + ";\n"
            + " const res={};for(const f of FAMS){\n"
            + "  res[f]=await measure('P_'+f,PX,TEXT,LS[f]||0,LH[f]||'1.15');}\n"
            + " document.getElementById('out').textContent='RESULT:'+JSON.stringify(res);})();\n"
            + "</script></body></html>")
    html_path = os.path.join(tmpdir, "preview.html")
    with open(html_path, "w", encoding="utf-8") as f:
        f.write(html)
    uri = "file:///" + html_path.replace("\\", "/").replace(" ", "%20")
    out = subprocess.run(
        [chrome, "--headless=new", "--no-sandbox", "--disable-gpu",
         "--virtual-time-budget=10000", "--dump-dom", uri],
        capture_output=True, text=True)
    for line in out.stdout.splitlines():
        if "RESULT:" in line:
            payload = line.split("RESULT:", 1)[1]
            # Strip trailing HTML.
            end = payload.find("</div>")
            if end != -1:
                payload = payload[:end]
            return json.loads(payload)
    return None


def pil_preview_widths(text, size):
    from PIL import Image, ImageDraw, ImageFont
    out = {}
    for key, famdir, regfile, _ass, _ratio, ls in STYLES:
        font = ImageFont.truetype(os.path.join(FONT_ROOT, famdir, regfile),
                                  size=size)
        tmp = Image.new("RGB", (4000, 400), (0, 0, 0))
        d = ImageDraw.Draw(tmp)
        bb = d.textbbox((0, 0), text, font=font)
        out[key] = (bb[2] - bb[0]) + len(text) * size * ls
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--text", default="REVENANT GO BRRRRR")
    ap.add_argument("--size", type=int, default=48)
    ap.add_argument("--width", type=int, default=1280)
    ap.add_argument("--height", type=int, default=720)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    tmpdir = args.out or tempfile.mkdtemp(prefix="parity_")
    os.makedirs(tmpdir, exist_ok=True)

    print(f"text={args.text!r} nominal={args.size} frame={args.width}x{args.height}")
    preview = chrome_preview_widths(args.text, args.size, tmpdir)
    if preview is None:
        print("WARNING: Chrome unavailable, falling back to PIL "
              "(less accurate for cinematic/handwritten/cyberpunk).")
        preview = pil_preview_widths(args.text, args.size)
    else:
        print("preview reference: headless Chrome DOM widths")

    print(f"{'style':12s} {'nom':>3s} {'ass':>3s} {'sp':>5s} "
          f"{'preview':>7s} {'export':>6s} {'err%':>7s}  status")
    failures = 0
    for key, famdir, _reg, ass, ratio, ls in STYLES:
        nominal = float(args.size)
        corrected = round(nominal * ratio)
        spacing = nominal * ls
        style = {"name": "TextOverlay1", "font_name": ass,
                 "font_size": corrected, "primary": "&H00FFFFFF",
                 "outline_c": "&H00000000", "back": "&HFF000000",
                 "bold": 0, "italic": 0, "spacing": spacing, "outline": 0.0}
        ass_path = os.path.join(tmpdir, f"{key}.ass")
        png_path = os.path.join(tmpdir, f"{key}.png")
        write_text_overlay_ass(ass_path, [(args.text, style, 0.5, 0.5)],
                               args.width, args.height)
        render_ass(ass_path, os.path.join(FONT_ROOT, famdir), png_path,
                   args.width, args.height)
        ew, _eh = measure_bbox(png_path)
        prev = preview[key]
        err = (ew / prev - 1.0) * 100.0 if prev else 0.0
        ok = abs(err) <= 2.0
        failures += 0 if ok else 1
        print(f"{key:12s} {args.size:3d} {corrected:3d} {spacing:5.2f} "
              f"{prev:7.1f} {ew:6d} {err:+7.2f}  {'PASS' if ok else 'FAIL'}")
    if failures:
        print(f"{failures} style(s) outside 2%")
        sys.exit(1)
    print("ALL PASS within 2%")


if __name__ == "__main__":
    main()
