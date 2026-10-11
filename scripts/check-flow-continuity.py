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

    mini_rest = next(s for s in scenes[resting_index + 1:]
                     if not s['active'] and s['progress'] == 0)
    spin = [s for s in scenes if s['active'] and s['progress'] < 0]
    assert spin and spin[-1]['mini_turn'] > .995, 'Mini must finish a complete turn'
    assert all(a['mini_turn'] <= b['mini_turn'] for a, b in zip(spin, spin[1:]))
    accents = [(34, 197, 94), (255, 138, 76), (139, 92, 246)]
    probes = [mini_rest] + [min(spin, key=lambda s: abs(s['mini_turn'] - turn))
                            for turn in [.25, .5, .75]]
    for scene in probes:
        window = next(w for w in scene['windows'] if w['name'].endswith('mini'))
        frame = Image.open(directory / (window['name'] + '.png')).convert('RGB')
        scale = frame.width / 66
        for row, service_id in enumerate(scene['mini_ids']):
            for fraction in [.25, .5, .75]:
                angle = -math.pi / 2 + (row + fraction) * 2 * math.pi / count
                angle += scene['mini_turn'] * 2 * math.pi
                x = math.floor(frame.width / 2 + 29 * scale * math.cos(angle))
                y = math.floor(frame.height / 2 + 29 * scale * math.sin(angle))
                pixels = [frame.getpixel((px, py))
                          for py in range(max(0, y - 1), min(frame.height, y + 2))
                          for px in range(max(0, x - 1), min(frame.width, x + 2))]
                color = min(pixels, key=lambda c: sum((a - b) ** 2
                                                     for a, b in zip(c, accents[service_id])))
                matched = min(range(3), key=lambda i: sum((a - b) ** 2
                                                         for a, b in zip(color, accents[i])))
                assert max(color) - min(color) > 35 and matched == service_id, (
                    f'Mini sector {row} has wrong color at turn {scene["mini_turn"]:.3f}')
    print(f'{count} Mini sectors: equal allocation and native rotating colors verified')
