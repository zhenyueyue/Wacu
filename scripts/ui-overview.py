"""Arrange unmodified native snapshots for visual comparison. Requires Pillow."""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

root = Path(__file__).resolve().parent.parent
shots = root / "docs" / "screenshots"
cases = [
    ("胶囊", "capsule-selected.png", 72),
    ("用量快览", "quick-detail.png", 296),
    ("用量详情", "full-detail.png", 296),
    ("常规设置", "settings-general.png", 424),
]
canvas = Image.new("RGB", (1248, 688), "#f3f7fb")
draw = ImageDraw.Draw(canvas)
font = ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 18)
x = 32
for label, filename, width in cases:
    draw.text((x + 12, 22), label, fill="#26324d", font=font)
    snapshot = Image.open(shots / filename).convert("RGBA")
    canvas.paste(snapshot, (x, 64), snapshot)
    x += width + 32
canvas.save(shots / "overview.png")
print(shots / "overview.png")
