"""Compose the archived, real Slint frames into a looping animation preview."""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


root = Path(__file__).resolve().parent.parent / "docs" / "screenshots"
sample = Image.open(root / "entry-open-settled.png").convert("RGBA")
width, height = sample.size
mini_sample = Image.open(root / "entry-mini-rest.png").convert("RGBA")
mini_width, mini_height = mini_sample.size
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
    ("close-mid", 30), ("close-fx-110", 20), ("close-fx-130", 40),
    ("close-fx-170", 30),
]]
sequence += [(None, 450, "收起")]
frames, durations = [], []
mini_open = {
    "open-early": "mini-exit-40", "open-fx-40": "mini-exit-40",
    "open-fx-60": "mini-exit-60", "open-mid": "mini-exit",
    "open-fx-100": "mini-exit-100",
}
mini_close = {
    "close-fx-20": "mini-return-20", "close-early": "mini-return-early",
    "close-fx-50": "mini-return-50", "close-mid": "mini-return-mid",
    "close-fx-110": "mini-return-110", "close-fx-130": "mini-return-130",
    "close-fx-170": "mini-return-170",
}
capsule_x = mini_width + 48
for name, duration, label in sequence:
    canvas = Image.new("RGBA", (mini_width + width + 72, height + 72), "#101114")
    draw = ImageDraw.Draw(canvas)
    draw.text((16, 8), label, font=font, fill="#d7dae0")
    draw.text((16, 32), "Mini", font=font, fill="#a1a1aa")
    draw.text((capsule_x, 32), "悬浮窗", font=font, fill="#a1a1aa")
    mini_name = (mini_open.get(name) if name else "mini-rest") if label == "展开" else (mini_close.get(name) if name else "mini-return-rest")
    if mini_name:
        mini = Image.open(root / f"entry-{mini_name}.png").convert("RGBA")
        if mini.size != mini_sample.size:
            raise ValueError(f"Mini dimensions changed: {mini_name}")
        canvas.alpha_composite(mini, (16, 56 + round(50 * width / 96 - mini_height / 2)))
    if name:
        frame = Image.open(root / f"entry-{name}.png").convert("RGBA")
        if frame.size != sample.size:
            raise ValueError(f"Frame dimensions changed: {name}")
        canvas.alpha_composite(frame, (capsule_x, 56))
    frames.append(canvas.convert("RGB"))
    durations.append(duration)
destination = root / "entry-ring-split.gif"
frames[0].save(destination, save_all=True, append_images=frames[1:],
               duration=durations, loop=0, disposal=2)
with Image.open(destination) as preview:
    # GIF encoders merge identical readbacks while retaining their durations.
    assert preview.n_frames >= 6
    encoded_frames = preview.n_frames
print(f"Generated {destination}: {len(frames)} real samples / {encoded_frames} encoded frames")
