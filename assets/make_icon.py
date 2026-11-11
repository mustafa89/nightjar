"""Draws the app icon (1024 px) and builds assets/icon.icns with iconutil.

A macOS-style rounded square: a deep night gradient and a ring of spectrum bars, cyan to pink,
round a glowing centre, as the Spectrum Halo visualizer draws it.
"""
import math
import os
import subprocess
import tempfile

from PIL import Image, ImageDraw, ImageFilter

S = 1024
here = os.path.dirname(os.path.abspath(__file__))


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


pad, radius = 100, 185
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([pad, pad, S - pad - 1, S - pad - 1], radius, fill=255)

bg = Image.new("RGBA", (S, S))
d = ImageDraw.Draw(bg)
for y in range(S):
    d.line([(0, y), (S, y)], fill=lerp((10, 15, 31, 255), (30, 24, 66, 255), y / S))

art = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(art)
cx = cy = S / 2
base, n = 170, 48
cyan, violet, pink = (41, 211, 255), (177, 108, 255), (255, 79, 216)
for i in range(n):
    a = i / n * math.tau
    x = abs((i / n) * 2 - 1)
    h = 60 + 150 * (0.55 + 0.45 * math.sin(x * 9.0 + 1.3)) * (1 - x * 0.4)
    col = lerp(cyan, violet, x * 2) if x < 0.5 else lerp(violet, pink, x * 2 - 1)
    r0, r1 = base, base + h
    d.line([(cx + math.cos(a) * r0, cy + math.sin(a) * r0), (cx + math.cos(a) * r1, cy + math.sin(a) * r1)], fill=col + (255,), width=20)
d.ellipse([cx - base + 22, cy - base + 22, cx + base - 22, cy + base - 22], outline=violet + (255,), width=10)

glow = art.filter(ImageFilter.GaussianBlur(28))
core = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(core).ellipse([cx - 110, cy - 110, cx + 110, cy + 110], fill=pink + (150,))
core = core.filter(ImageFilter.GaussianBlur(60))

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
body = Image.alpha_composite(Image.alpha_composite(Image.alpha_composite(bg, core), glow), art)
img.paste(body, (0, 0), mask)
img.save(os.path.join(here, "icon.png"))

with tempfile.TemporaryDirectory() as tmp:
    iconset = os.path.join(tmp, "icon.iconset")
    os.mkdir(iconset)
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            px = size * scale
            name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
            img.resize((px, px), Image.LANCZOS).save(os.path.join(iconset, name))
    subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(here, "icon.icns")], check=True)
