"""Compose the archived, real Slint frames into a looping animation preview."""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


root = Path(__file__).resolve().parent.parent / "docs" / "screenshots"
sample = Image.open(root / "entry-open-settled.png").convert("RGBA")
width, height = sample.size
font = ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 12)
sequence = [(None, 180, "展开")]
sequence += [(name, duration, "展开") for name, duration in [
    ("open-early", 10), ("open-fx-40", 20), ("open-fx-60", 20),
    ("open-mid", 20), ("open-fx-100", 20), ("open-fx-120", 40),
    ("open-fx-160", 40), ("open-fx-200", 25), ("open-rest", 35),
    ("open-fx-260", 40), ("open-fx-300", 70), ("open-settled", 550),
]]
sequence += [(name, duration, "收起") for name, duration in [
    ("close-fx-20", 10), ("close-early", 20), ("close-fx-50", 30),
    ("close-mid", 30), ("close-fx-110", 20), ("close-fx-130", 30),
]]
sequence += [(None, 450, "收起")]
frames, durations = [], []
for name, duration, label in sequence:
    canvas = Image.new("RGBA", (width + 48, height + 48), "#101114")
    ImageDraw.Draw(canvas).text((24, 8), label, font=font, fill="#a1a1aa")
    if name:
        frame = Image.open(root / f"entry-{name}.png").convert("RGBA")
        if frame.size != sample.size:
            raise ValueError(f"Frame dimensions changed: {name}")
        canvas.alpha_composite(frame, (24, 30))
    frames.append(canvas.convert("RGB"))
    durations.append(duration)
destination = root / "entry-aurora.gif"
frames[0].save(destination, save_all=True, append_images=frames[1:],
               duration=durations, loop=0, disposal=2)
with Image.open(destination) as preview:
    # GIF encoders merge identical readbacks while retaining their durations.
    assert preview.n_frames >= 6
    encoded_frames = preview.n_frames
print(f"Generated {destination}: {len(frames)} real samples / {encoded_frames} encoded frames")
