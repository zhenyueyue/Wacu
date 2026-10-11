"""Compose native window readbacks at their captured desktop coordinates."""
from pathlib import Path
import json
from PIL import Image, ImageDraw, ImageFont

root = Path(__file__).resolve().parent.parent
font = ImageFont.truetype('C:/Windows/Fonts/msyh.ttc', 14)
for count in range(4):
    directory = root / 'target' / f'handoff-frames-{count}'
    scenes = json.loads((directory / 'scenes.json').read_text(encoding='utf-8'))
    bounds = []
    for scene in scenes:
        for window in scene['windows']:
            with Image.open(directory / (window['name'] + '.png')) as frame:
                bounds.append((window['x'], window['y'], window['x'] + frame.width, window['y'] + frame.height))
    left = min(b[0] for b in bounds) - 16
    top = min(b[1] for b in bounds) - 16
    width = max(b[2] for b in bounds) - left + 16
    height = max(b[3] for b in bounds) - top + 16
    frames, durations = [], []
    for index, scene in enumerate(scenes):
        canvas = Image.new('RGBA', (width, height + 40), '#101114')
        for window in scene['windows']:
            frame = Image.open(directory / (window['name'] + '.png')).convert('RGBA')
            canvas.alpha_composite(frame, (window['x'] - left, window['y'] - top + 40))
        ImageDraw.Draw(canvas).text((16, 8), f'{count} 个额度环 · 分路汇入 Logo · 实际速度', font=font, fill='#e4e4e7')
        frames.append(canvas.convert('RGB'))
        interval = scenes[index + 1]['time'] - scene['time'] if index + 1 < len(scenes) else 400
        durations.append(max(20, round(interval / 10) * 10))
    durations[0] = max(350, durations[0])
    output = root / 'docs' / 'screenshots' / f'entry-logo-flow-count-{count}.gif'
    frames[0].save(output, save_all=True, append_images=frames[1:], duration=durations, loop=0, disposal=2)
    with Image.open(output) as animation:
        assert animation.n_frames >= 14, 'Native preview must contain distinct intermediate frames'
        print(f'{count} rings: {len(frames)} samples / {animation.n_frames} encoded frames; {output.stat().st_size} bytes')
