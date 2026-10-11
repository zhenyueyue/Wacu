"""Compare native terminal motion against the resting quota arcs."""
import argparse
import json
import math
from pathlib import Path

from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--counts', type=int, nargs='+', default=[1, 2, 3])
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
for count in args.counts:
    directory = root / 'target' / f'handoff-frames-{count}'
    scenes = json.loads((directory / 'scenes.json').read_text(encoding='utf-8'))
    resting_index = next(i for i, s in enumerate(scenes)
                         if not s['active'] and s['progress'] == 1)
    terminal = next(s for s in reversed(scenes[:resting_index])
                    if s['active'] and s['progress'] >= .95)

    def capsule(scene):
        window = next(w for w in scene['windows'] if w['name'].endswith('capsule'))
        return Image.open(directory / (window['name'] + '.png')).convert('RGB')

    moving, resting = capsule(terminal), capsule(scenes[resting_index])
    assert moving.size == resting.size
    # The capture suite uses the standard preset. Scale follows the actual
    # native framebuffer, rather than assuming the desktop is at 100% DPI.
    scale = resting.width / 96
    radius = (56 - 4 - 4.5) * 24 / 56 * scale
    for row in range(count):
        cx, cy = 48 * scale, (118 + row * 77) * scale
        differences = []
        for y in range(max(0, int(cy - radius - 5 * scale)),
                       min(resting.height, math.ceil(cy + radius + 5 * scale))):
            for x in range(max(0, int(cx - radius - 5 * scale)),
                           min(resting.width, math.ceil(cx + radius + 5 * scale))):
                distance = math.hypot(x + .5 - cx, y + .5 - cy)
                color = resting.getpixel((x, y))
                if abs(distance - radius) > 3.5 * scale or max(color) - min(color) <= 35:
                    continue
                incoming = moving.getpixel((x, y))
                differences.append(max(abs(a - b) for a, b in zip(color, incoming)))
        assert differences, f'Quota {count}/{row} has no colored arc pixels'
        mean = sum(differences) / len(differences)
        maximum = max(differences)
        assert mean <= 4 and maximum <= 32, (
            f'Quota {count}/{row} jumps at rest: mean={mean:.2f}, max={maximum}')
        print(f'{count} rings / row {row}: native terminal match, mean={mean:.2f}, max={maximum}')
