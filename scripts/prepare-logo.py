"""Prepare faithful, antialiased 2x UI assets from the supplied brand image."""
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw

root = Path(__file__).resolve().parent.parent
source = Image.open(root / "docs" / "logo.png").convert("RGBA")
mask = Image.new("L", source.size, 0)
ImageDraw.Draw(mask).ellipse((0, 0, source.width - 1, source.height - 1), fill=255)
source.putalpha(ImageChops.multiply(source.getchannel("A"), mask))
for name, logical_size in [("compact", 32), ("standard", 40), ("spacious", 48)]:
    size = logical_size * 2
    icon = source.resize((size, size), Image.Resampling.LANCZOS)
    icon.save(root / "assets" / f"logo-{name}.png", optimize=True)
