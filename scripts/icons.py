"""Draws the PocketDrop app icons as PNGs, using only the standard library.

    python3 scripts/icons.py public/icons

The droplet is the convex hull of a large circle (its body) and a small one
(its rounded tip), drawn from a signed distance so edges are anti-aliased.

- icon-<size>.png: indigo droplet on a transparent background (browser tab,
  desktop install).
- maskable-<size>.png: white droplet on full-bleed indigo, kept inside the
  central safe zone so Android can crop it to any launcher shape.
"""

import math
import struct
import sys
import zlib

INDIGO = (79, 70, 229)
WHITE = (255, 255, 255)


def droplet_distance(x, y, cx, cy, r_body, r_tip, height):
    """Signed distance to a droplet whose body circle is centred at (cx, cy)
    and whose tip circle sits `height` above it (negative inside)."""
    # Local coordinates with the tip pointing up (+y), mirrored around the axis.
    px, py = abs(x - cx), cy - y
    b = (r_body - r_tip) / height
    a = math.sqrt(1 - b * b)
    k = -b * px + a * py
    if k < 0:
        return math.hypot(px, py) - r_body
    if k > a * height:
        return math.hypot(px, py - height) - r_tip
    return a * px + b * py - r_body


def render(size, background, droplet_color, scale):
    """scale: droplet size relative to the icon (1.0 fills the height)."""
    total = size * scale
    r_body = total * 0.36
    r_tip = total * 0.045
    height = total - r_body - r_tip
    cx = size / 2
    # Centre the whole shape (tip top to body bottom) vertically.
    cy = size / 2 + total / 2 - r_body

    rows = []
    for y in range(size):
        row = bytearray([0])  # filter: none
        for x in range(size):
            d = droplet_distance(x + 0.5, y + 0.5, cx, cy, r_body, r_tip, height)
            coverage = min(1.0, max(0.0, 0.5 - d))
            if background is None:
                row += bytes((*droplet_color, round(255 * coverage)))
            else:
                mixed = (round(bg + (fg - bg) * coverage) for bg, fg in zip(background, droplet_color))
                row += bytes((*mixed, 255))
        rows.append(bytes(row))
    return png(size, b"".join(rows))


def png(size, raw):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)  # 8-bit RGBA
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def main(out_dir):
    for size in (192, 512):
        with open(f"{out_dir}/icon-{size}.png", "wb") as f:
            f.write(render(size, None, INDIGO, 0.86))
        # The maskable safe zone is a circle of 80% diameter; 0.56 keeps the
        # droplet comfortably inside it.
        with open(f"{out_dir}/maskable-{size}.png", "wb") as f:
            f.write(render(size, INDIGO, WHITE, 0.56))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "public/icons")
