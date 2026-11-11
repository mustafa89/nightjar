"""Nightjar's icon with a red play badge, as ytmusic/icon.icns."""
import os
import subprocess
import tempfile

from PIL import Image, ImageDraw

here = os.path.dirname(os.path.abspath(__file__))
img = Image.open(os.path.join(here, "..", "assets", "icon.png")).convert("RGBA")
S = img.width
d = ImageDraw.Draw(img)
r = 150
cx, cy = S - 100 - r + 20, S - 100 - r + 20
d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(255, 0, 51, 255), outline=(255, 255, 255, 255), width=14)
t = r * 0.42
d.polygon([(cx - t * 0.7, cy - t), (cx - t * 0.7, cy + t), (cx + t, cy)], fill=(255, 255, 255, 255))
with tempfile.TemporaryDirectory() as tmp:
    iconset = os.path.join(tmp, "icon.iconset")
    os.mkdir(iconset)
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            px = size * scale
            name = f"icon_{size}x{size}{'@2x' if scale == 2 else ''}.png"
            img.resize((px, px), Image.LANCZOS).save(os.path.join(iconset, name))
    subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(here, "icon.icns")], check=True)
