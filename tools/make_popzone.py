"""Builds GTA's popzone.ipl from Burnout's district map.

  python make_popzone.py <districts.npy> <original popzone.ipl> <out popzone.ipl>

Each Burnout district gets a Los Santos city zone (all LSPD-policed, picked
for character), as boxes merged from the district grid. The grid placement
was lined up by hand in tools/viewer/district_aligner.html (mainland and
island are painted at different scales). Everything else falls into one
large ocean box listed last.
"""
import sys
import numpy as np

# Hand alignment (district_aligner.html): cell size x/y, west and north edge.
MAINLAND = dict(cx=32.267, cy=23.767, x0=-4199.0, y0=3837.0)
ISLAND = dict(cx=18.117, cy=14.517, x0=-1031.0, y0=1711.0)
ISLAND_IDS = set(range(18, 23))

# Burnout district -> Los Santos zone label (all VEHICLE_RESPONSE_DEFAULT).
LABEL = {
    0: 'DelPe', 1: 'Mirr', 2: 'Koreat', 3: 'Beach', 4: 'PBluff', 5: 'CHIL',
    6: 'CHIL', 7: 'CHIL', 8: 'Elysian', 9: 'Rockf', 10: 'Elysian', 11: 'CHIL',
    12: 'CHIL', 13: 'Richm', 14: 'Downt', 15: 'LMesa', 16: 'Murri', 17: 'Vesp',
    18: 'DelPe', 19: 'DeLBe', 20: 'Alta', 21: 'Beach', 22: 'PBluff',
}
Z_MIN, Z_MAX = -200.0, 1500.0


def boxes(mask):
    """Greedy rectangles over a boolean grid: (row0, col0, row1, col1)."""
    used = np.zeros_like(mask)
    h, w = mask.shape
    out = []
    for r in range(h):
        for c in range(w):
            if not mask[r, c] or used[r, c]:
                continue
            c1 = c
            while c1 + 1 < w and mask[r, c1 + 1] and not used[r, c1 + 1]:
                c1 += 1
            r1 = r
            while r1 + 1 < h and mask[r1 + 1, c:c1 + 1].all() and not used[r1 + 1, c:c1 + 1].any():
                r1 += 1
            used[r:r1 + 1, c:c1 + 1] = True
            out.append((r, c, r1, c1))
    return out


def main():
    g = np.load(sys.argv[1])
    orig = open(sys.argv[2], encoding='utf-8', errors='replace').read().splitlines()
    zone_of = {}
    for line in orig:
        p = [x.strip() for x in line.split(',')]
        if len(p) >= 9:
            zone_of.setdefault(p[7], p[0])
    lines = ['zone']
    for d, label in LABEL.items():
        a = ISLAND if d in ISLAND_IDS else MAINLAND
        for r0, c0, r1, c1 in boxes(g == d):
            x0 = a['x0'] + c0 * a['cx']; x1 = a['x0'] + (c1 + 1) * a['cx']
            y1 = a['y0'] - r0 * a['cy']; y0 = a['y0'] - (r1 + 1) * a['cy']
            lines.append(f'{zone_of[label]}, {x0:.2f}, {y0:.2f}, {Z_MIN}, {x1:.2f}, {y1:.2f}, {Z_MAX}, {label}, 0')
    n = len(lines) - 1
    lines.append(f"{zone_of['OCEANA']}, -16000, -16000, {Z_MIN}, 16000, 16000, {Z_MAX}, OCEANA, 0")
    lines.append('end')
    open(sys.argv[3], 'w', encoding='utf-8', newline='\r\n').write('\n'.join(lines) + '\n')
    print(f'popzone: {n} district boxes + ocean fallback')


if __name__ == '__main__':
    main()
