#!/usr/bin/env python3
"""Sluis logomark: the app icon next to a hand-drawn "sluis" wordmark.

The letters aren't set in a font. Each one is a monoline stroke with round
ends, drawn here as an SVG path, so the logo looks the same everywhere and
needs no font files. The dot of the i is warm, like the light that comes
through the hole in the app icon's wall.

Usage: make.py            writes the SVGs and PNGs next to this file
"""
import os
import re

import cairo
import gi

gi.require_version("Rsvg", "2.0")
from gi.repository import Rsvg  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
ICON = os.path.join(ROOT, "data/icons/hicolor/scalable/apps/io.github.rayytor.Sluis.svg")

W = 14                    # stroke width
TOP, MID, BOT = 47, 70, 93  # x-height centrelines: top, middle, baseline
ASC = 4                   # ascender top (centreline)

INK = {"light": "#241f31", "dark": "#ffffff"}
CARD_BG = "#1d1b22"


def s(x):
    return (f"M{x+40} {TOP+6}C{x+37} {TOP+2} {x+31} {TOP} {x+25} {TOP}"
            f"C{x+16} {TOP} {x+10} {TOP+4} {x+10} {TOP+11.5}"
            f"C{x+10} {MID-4} {x+17} {MID-2} {x+25} {MID}"
            f"C{x+33} {MID+2} {x+40} {MID+4} {x+40} {BOT-11.5}"
            f"C{x+40} {BOT-4} {x+34} {BOT} {x+25} {BOT}"
            f"C{x+17} {BOT} {x+11} {BOT-3} {x+8} {BOT-7}")


def l(x):
    return f"M{x} {ASC}V{BOT-12}C{x} {BOT-4} {x+4} {BOT} {x+11} {BOT}"


def u(x):
    return (f"M{x} {TOP}V{MID+2}C{x} {BOT-7} {x+8} {BOT} {x+18} {BOT}"
            f"C{x+28} {BOT} {x+36} {BOT-7} {x+36} {MID+2}V{TOP}M{x+36} {TOP}V{BOT}")


def i(x):
    return f"M{x} {TOP}V{BOT}"


def dot(x):
    """The i's dot: a warm disc, like the light through the hole in the icon."""
    return f'<circle cx="{x}" cy="{TOP - W / 2 - 16:g}" r="{W / 2 + 1:g}" fill="url(#dot)"/>'


def wordmark(ink, dx=0.0, dy=0.0):
    x = dx
    letters = [s(x), l(x + 63), u(x + 95), i(x + 163), s(x + 182)]
    d = "".join(letters)
    return (f'<g transform="translate(0 {dy:g})">'
            f'<path d="{d}" fill="none" stroke="{ink}" stroke-width="{W}" '
            f'stroke-linecap="round" stroke-linejoin="round"/>'
            f"{dot(x + 163)}</g>")


DEFS = ('<defs><linearGradient id="dot" x1="0" y1="0" x2="1" y2="1">'
        '<stop offset="0" stop-color="#ff8a00"/><stop offset="1" stop-color="#e01b5a"/>'
        "</linearGradient></defs>")
WORD_W, WORD_H = 232, 100   # ink box of the wordmark, stroke included


def icon_svg(x, y, size):
    src = open(ICON).read()
    inner = re.sub(r"^.*?<svg[^>]*>", "", src, count=1, flags=re.S).rsplit("</svg>", 1)[0]
    return f'<svg x="{x:g}" y="{y:g}" width="{size:g}" height="{size:g}" viewBox="0 0 1254 1254">{inner}</svg>'


def doc(w, h, body, bg=None):
    rect = f'<rect width="100%" height="100%" fill="{bg}"/>' if bg else ""
    return (f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
            f'width="{w:g}" height="{h:g}" viewBox="0 0 {w:g} {h:g}">'
            f"<title>Sluis</title>{DEFS}{rect}{body}</svg>\n")


def lockup(ink, pad=8, bg=None, extra_w=0, extra_h=0):
    # the icon's tile fills roughly 13%..87% of its box; let it overhang the padding
    size = 136
    ix, iy = pad - 0.10 * size, pad + 54 - 0.52 * size
    wx = pad + 0.80 * size + 24
    w = wx + WORD_W + pad + extra_w
    h = WORD_H + 2 * pad + extra_h
    body = (f'<g transform="translate({extra_w/2:g} {extra_h/2:g})">'
            + icon_svg(ix, iy, size) + wordmark(ink, wx + 1, pad) + "</g>")
    return doc(w, h, body, bg)


def render(svg_path, png_path, scale):
    h = Rsvg.Handle.new_from_file(svg_path)
    ok, w, ht = h.get_intrinsic_size_in_pixels()
    surf = cairo.ImageSurface(cairo.FORMAT_ARGB32, int(w * scale), int(ht * scale))
    r = Rsvg.Rectangle()
    r.x, r.y, r.width, r.height = 0, 0, w * scale, ht * scale
    h.render_document(cairo.Context(surf), r)
    surf.write_to_png(png_path)


def write(name, text):
    path = os.path.join(HERE, name)
    with open(path, "w") as f:
        f.write(text)
    return path


def main():
    for theme, ink in INK.items():
        logo = write(f"sluis-logo-{theme}.svg", lockup(ink))
        render(logo, logo[:-4] + ".png", 4)
        word = write(f"sluis-wordmark-{theme}.svg",
                     doc(WORD_W + 16, WORD_H + 16, wordmark(ink, 9, 8)))
        render(word, word[:-4] + ".png", 4)
    # a 16:9 card with a solid background, for Reddit and other social posts
    base = lockup(INK["dark"])
    bw = float(re.search(r'width="([\d.]+)"', base).group(1))
    cw = bw + 160
    card = write("sluis-card.svg", lockup(INK["dark"], bg=CARD_BG, extra_w=160,
                                          extra_h=cw * 9 / 16 - (WORD_H + 16)))
    render(card, card[:-4] + ".png", 1600 / cw)


if __name__ == "__main__":
    main()
