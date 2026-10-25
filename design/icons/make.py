#!/usr/bin/env python3
"""Draws Sluis's two status icons as GNOME symbolic SVGs (fills only, one
colour, so GTK recolours them: green when free, amber when chained).

  sluis-chained-symbolic  two raised fists, wrists cuffed and chained together
  sluis-freed-symbolic    the same fists apart, the chain snapped, a burst

Original drawing; the idea follows the reference the user supplied.
GTK's symbolic renderer ignores <g transform>, so every shape is a point
list that is moved and rotated here and written out as plain coordinates.
Usage: make.py OUTDIR   (app_icon.py imports the shapes from here)
"""
import math
import os
import sys


def rr(x, y, w, h, r, steps=6):
    """Rounded rectangle as a list of points (clockwise)."""
    r = min(r, w / 2, h / 2)
    pts = []
    for cx, cy, start in ((x + w - r, y + r, -90), (x + w - r, y + h - r, 0),
                          (x + r, y + h - r, 90), (x + r, y + r, 180)):
        for i in range(steps + 1):
            a = math.radians(start + 90 * i / steps)
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def poly(*xy):
    return [(xy[i], xy[i + 1]) for i in range(0, len(xy), 2)]


def move(shapes, dx=0.0, dy=0.0, angle=0.0, flip=1):
    """Mirror (flip=-1), rotate by angle degrees, then translate."""
    a = math.radians(angle)
    c, s = math.cos(a), math.sin(a)
    return [[(dx + c * x * flip - s * y, dy + s * x * flip + c * y) for x, y in shape] for shape in shapes]


def d(shapes):
    return " ".join("M" + " L".join(f"{x:.2f},{y:.2f}" for x, y in shape) + " Z" for shape in shapes)


class Drawing:
    def __init__(self, scale=1.0):
        self.paths, self.scale = [], scale

    def add(self, shapes, evenodd=False):
        k = self.scale
        shapes = [[(32 + (x - 32) * k, 40 + (y - 40) * k) for x, y in shape] for shape in shapes]
        rule = ' fill-rule="evenodd"' if evenodd else ""
        self.paths.append(f'<path d="{d(shapes)}"{rule}/>')

    def svg(self):
        return ('<?xml version="1.0" encoding="UTF-8"?>\n'
                '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">'
                + "".join(self.paths) + "</svg>\n")


GAP = 1.4  # space that separates touching parts so they read as separate


def arm_parts(dx, dy, angle=0.0, flip=1):
    """One raised fist and forearm, as named shape lists; local origin is the
    wrist centre at the top of the cuff. Fist seen from the front: four
    knuckles on top, the thumb folded across the palm. "fist" is meant to be
    filled evenodd: the finger gaps and thumb fold are holes in it."""
    m = lambda shapes: move(shapes, dx, dy, angle, flip)
    fw, gap_f = 4.1, 1.2                     # finger width, gap between fingers
    tops = (-30.0, -32.0, -31.6, -30.6)      # pinky ... index (pinky shortest)
    fingers = [rr(-10 + i * (fw + gap_f), top, fw, -21.6 - top, fw / 2) for i, top in enumerate(tops)]
    palm = [rr(-10, -21.6, 20, 10.6, 4.0)]
    thumb_fold = [rr(-7.5, -19.4, 10.8, 1.3, 0.65), rr(3.3, -19.4, 1.3, 5.2, 0.65)]
    return {
        "fist": m(fingers + palm + thumb_fold),
        "fist_outline": m(fingers + palm),
        "wrist": m([rr(-6.5, -13.0, 13, 13.0 - GAP, 2)]),
        "cuff": m([rr(-8.5, 0, 17, 7, 1.8)]),
        "forearm": m([rr(-6.5, 7 + GAP, 13, 40, 2)]),
    }


def arm(dr, dx, dy, angle=0.0, flip=1):
    p = arm_parts(dx, dy, angle, flip)
    dr.add(p["fist"], evenodd=True)
    for part in ("wrist", "cuff", "forearm"):
        dr.add(p[part])


FREED_ARMS = ((15.5, -13, 1), (48.5, 13, -1))   # dx, angle, mirror; wrists at y=41


def link(cx, cy, w, h, t):
    """Chain link seen from the front: outer ring + hole (use evenodd)."""
    return [rr(cx - w / 2, cy - h / 2, w, h, h / 2), rr(cx - w / 2 + t, cy - h / 2 + t, w - 2 * t, h - 2 * t, (h - 2 * t) / 2)]


def open_link(cx, cy, rx=3.6, ry=3.0, t=1.5, opening=40):
    """A snapped link: a ring with its far side broken open (towards +x)."""
    steps = 24
    angles = [math.radians(opening + (360 - 2 * opening) * i / steps) for i in range(steps + 1)]
    outer = [(cx + rx * math.cos(a), cy + ry * math.sin(a)) for a in angles]
    inner = [(cx + (rx - t) * math.cos(a), cy + (ry - t) * math.sin(a)) for a in reversed(angles)]
    return [outer + inner]


def side_link(cx, cy):
    """Chain link seen edge-on: a short bar."""
    return [rr(cx - 2.0, cy - 1.25, 4.0, 2.5, 1.25)]


def main(outdir):
    out = os.path.join(outdir, "hicolor", "scalable", "status")
    os.makedirs(out, exist_ok=True)
    # chained: fists side by side, cuffs joined by an unbroken chain
    chained = Drawing(scale=0.9)
    L, R, Y = 16.5, 47.5, 37.0
    arm(chained, L, Y)
    arm(chained, R, Y)
    chained.add(link(28.4, Y + 3.5, 7.0, 5.8, 1.5), evenodd=True)
    chained.add(side_link(32.0, Y + 3.5))
    chained.add(link(35.6, Y + 3.5, 7.0, 5.8, 1.5), evenodd=True)

    # freed: fists apart and leaning out, chain snapped, burst between them
    freed = Drawing(scale=0.9)
    for dx, angle, flip in FREED_ARMS:
        arm(freed, dx, 41.0, angle, flip)
        m = lambda shapes: move(shapes, dx, 41.0, angle, flip)
        freed.add(m(open_link(11.4, 3.5)))         # snapped link, open towards the middle

    cx, cy = 32.0, 34.0
    for deg, r0, length in ((-90, 5.0, 9.0), (-58, 5.5, 7.5), (-122, 5.5, 7.5), (-28, 7.5, 4.5), (-152, 7.5, 4.5)):
        a = math.radians(deg)
        ux, uy, px, py = math.cos(a), math.sin(a), -math.sin(a), math.cos(a)
        w0, w1 = 0.45, 1.3
        x0, y0 = cx + ux * r0, cy + uy * r0
        x1, y1 = cx + ux * (r0 + length), cy + uy * (r0 + length)
        freed.add([poly(x0 + px * w0, y0 + py * w0, x1 + px * w1, y1 + py * w1,
                        x1 - px * w1, y1 - py * w1, x0 - px * w0, y0 - py * w0)])
    # two flying chips of the snapped link
    freed.add([poly(29.6, 41.6, 31.2, 40.6, 31.9, 42.2, 30.3, 43.1)])
    freed.add([poly(32.6, 39.4, 34.4, 39.9, 33.9, 41.6, 32.2, 41.1)])

    for name, drawing in (("sluis-chained-symbolic", chained), ("sluis-freed-symbolic", freed)):
        with open(os.path.join(out, name + ".svg"), "w") as f:
            f.write(drawing.svg())
        print(os.path.join(out, name + ".svg"))


if __name__ == "__main__":
    main(sys.argv[1])
