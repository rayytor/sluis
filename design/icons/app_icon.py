#!/usr/bin/env python3
"""Sluis's app icon, following the GNOME App Icon Guidelines:
128 px full-colour SVG on a simple base shape with a darker bottom side,
light from the top, GNOME palette colours only, no text; plus the matching
symbolic icon. The motif is the status icon's: two fists breaking free.

Usage: app_icon.py APP_ID OUTDIR
writes OUTDIR/hicolor/scalable/apps/APP_ID.svg
       OUTDIR/hicolor/symbolic/apps/APP_ID-symbolic.svg
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import make  # noqa: E402  (shared hand and chain shapes)

# GNOME HIG palette
BLUE = ("#99c1f1", "#62a0ea", "#3584e4", "#1c71d8", "#1a5fb4")
YELLOW = ("#f9f06b", "#f8e45c", "#f6d32d", "#f5c211", "#e5a50a")
LIGHT = ("#ffffff", "#f6f5f4", "#deddda", "#c0bfbc", "#9a9996")
DARK = ("#77767b", "#5e5c64", "#3d3846", "#241f31", "#000000")

CX, CY, R = 64.0, 58.0, 52.0   # face of the base circle
SIDE = 8.0                      # visible bottom side (depth)
S, OY = 1.25, 62.0              # status-icon units -> canvas


def canvas(shapes):
    return [[(CX + (x - 32) * S, OY + (y - 40) * S) for x, y in shape] for shape in shapes]


def path(shapes, fill, evenodd=False, extra=""):
    rule = ' fill-rule="evenodd"' if evenodd else ""
    return f'<path d="{make.d(canvas(shapes))}" fill="{fill}"{rule}{extra}/>'


def star(cx, cy, points, r_out, r_in, twist=0.0):
    pts = []
    for i in range(points * 2):
        r = r_out if i % 2 == 0 else r_in
        a = math.radians(-90 + twist + i * 180 / points)
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def full_colour():
    arms = [make.arm_parts(dx, 41.0, angle, flip) for dx, angle, flip in make.FREED_ARMS]
    links = [make.move(make.open_link(11.4, 3.5), dx, 41.0, angle, flip) for dx, angle, flip in make.FREED_ARMS]
    # a lighter band across the top of each cuff (light from above)
    shine = [make.move([make.rr(-7.6, 0.9, 15.2, 2.0, 1.0)], dx, 41.0, angle, flip) for dx, angle, flip in make.FREED_ARMS]

    body = []
    # burst behind the hands, centred on the break
    bx, by = CX + (32 - 32) * S, OY + (33 - 40) * S
    burst = " ".join(f"{x:.2f},{y:.2f}" for x, y in star(bx, by, 12, 36, 21))
    core = " ".join(f"{x:.2f},{y:.2f}" for x, y in star(bx, by, 12, 24, 15, twist=15))
    body.append(f'<polygon points="{burst}" fill="url(#burst)"/>')
    body.append(f'<polygon points="{core}" fill="{YELLOW[0]}" opacity="0.7"/>')

    # soft shadow of the arms on the burst and base
    shadow = []
    for p in arms:
        shadow += p["fist_outline"] + p["wrist"] + p["cuff"] + p["forearm"]
    shadow = [[(x + 0.6, y + 1.6) for x, y in s] for s in shadow]
    body.append(path(shadow, DARK[4], extra=' opacity="0.18"'))

    for p, link, band in zip(arms, links, shine):
        body.append(path(p["forearm"] + p["wrist"], "url(#skin)"))
        body.append(path(p["fist_outline"], LIGHT[3]))        # shows through the finger gaps
        body.append(path(p["fist"], "url(#skin)", evenodd=True))
        body.append(path(p["cuff"], DARK[1]))
        body.append(path(band, DARK[0]))
        body.append(path(link, LIGHT[2]))
    # chips of the snapped link
    body.append(path([make.poly(29.6, 41.6, 31.2, 40.6, 31.9, 42.2, 30.3, 43.1)], LIGHT[2]))
    body.append(path([make.poly(32.6, 39.4, 34.4, 39.9, 33.9, 41.6, 32.2, 41.1)], LIGHT[2]))

    return f'''<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">
  <defs>
    <linearGradient id="face" x1="0" y1="{CY - R}" x2="0" y2="{CY + R}" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="{BLUE[1]}"/>
      <stop offset="1" stop-color="{BLUE[3]}"/>
    </linearGradient>
    <linearGradient id="skin" x1="0" y1="20" x2="0" y2="112" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="{LIGHT[0]}"/>
      <stop offset="1" stop-color="{LIGHT[2]}"/>
    </linearGradient>
    <radialGradient id="burst" cx="{CX}" cy="{OY - 7 * S}" r="36" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="{YELLOW[1]}"/>
      <stop offset="1" stop-color="{YELLOW[3]}"/>
    </radialGradient>
    <clipPath id="inside"><circle cx="{CX}" cy="{CY}" r="{R}"/></clipPath>
  </defs>
  <circle cx="{CX}" cy="{CY + SIDE}" r="{R}" fill="{BLUE[4]}"/>
  <circle cx="{CX}" cy="{CY}" r="{R}" fill="url(#face)"/>
  <g clip-path="url(#inside)">
    {"".join(body)}
  </g>
  <circle cx="{CX}" cy="{CY}" r="{R - 0.5}" fill="none" stroke="{LIGHT[0]}" stroke-opacity="0.15"/>
</svg>
'''


def symbolic():
    """The freed status icon, at the 16 px symbolic size."""
    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                           "hicolor", "scalable", "status", "sluis-freed-symbolic.svg")) as f:
        svg = f.read()
    return svg.replace('width="64" height="64"', 'width="16" height="16"')


def main(app_id, outdir):
    make.main(os.path.dirname(os.path.abspath(__file__)))   # status icons first (symbolic reuses one)
    for sub, name, content in (("scalable", app_id + ".svg", full_colour()),
                               ("symbolic", app_id + "-symbolic.svg", symbolic())):
        d = os.path.join(outdir, "hicolor", sub, "apps")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, name), "w") as f:
            f.write(content)
        print(os.path.join(d, name))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
