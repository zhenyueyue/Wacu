"""Arrange unmodified native snapshots for visual comparison. Requires Pillow."""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

root = Path(__file__).resolve().parent.parent
shots = root / "docs" / "screenshots"
cases = [
    ("胶囊", "capsule-selected.png"),
    ("用量快览", "quick-detail.png"),
    ("用量详情", "full-detail.png"),
    ("常规设置", "settings-general.png"),
]
snapshots = [(label, Image.open(shots / filename).convert("RGBA")) for label, filename in cases]
width = sum(snapshot.width for _, snapshot in snapshots) + 32 * (len(snapshots) + 1)
height = max(snapshot.height for _, snapshot in snapshots) + 96
canvas = Image.new("RGB", (width, height), "#f3f7fb")
draw = ImageDraw.Draw(canvas)
font = ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 18)
x = 32
for label, snapshot in snapshots:
    draw.text((x + 12, 22), label, fill="#26324d", font=font)
    canvas.paste(snapshot, (x, 64), snapshot)
    x += snapshot.width + 32
canvas.save(shots / "overview.png")
print(shots / "overview.png")
