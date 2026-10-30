#!/usr/bin/env python3
"""mksprites.py OUT -- !BCPL's !Sprites, drawn here: the application's icon
(a window, "BC" over "PL" in white on rust) and the file type's: &A78, a
BCPL program or module (a page with a rust band and the letters).  From
MacModula2's box/app/mksprites.py.  34 x 34 pixels, 32 bpp (&00BBGGRR) with
a 1 bpp mask, as rosgd's ports/rospdf/tools/mksprites.py makes !PDF's."""
import struct
import sys

W = H = 34
MODE = 6 << 27 | 90 << 14 | 90 << 1 | 1     # 32 bpp, 90 x 90 dpi
GLYPHS = {
    "B": ["110", "101", "110", "101", "110"],
    "C": ["011", "100", "100", "100", "011"],
    "P": ["110", "101", "110", "100", "100"],
    "L": ["100", "100", "100", "100", "111"],
}
RUST = (0xB0, 0x40, 0x10)
WHITE = (0xFF, 0xFF, 0xFF)
GREY = (0x60, 0x60, 0x60)
LIGHT = (0xC0, 0xC0, 0xC0)


def letters(px, text, x, y, scale, colour):
    for i, ch in enumerate(text):
        for gy, row in enumerate(GLYPHS[ch]):
            for gx, bit in enumerate(row):
                if bit == "1":
                    for dy in range(scale):
                        for dx in range(scale):
                            px[x + (i * 4 + gx) * scale + dx, y + gy * scale + dy] = colour


def app():
    px = {}
    for y in range(3, 31):
        for x in range(2, 32):
            px[x, y] = GREY if x in (2, 31) or y in (3, 30) else RUST
    for y in range(4, 8):                       # the title bar
        for x in range(3, 31):
            px[x, y] = LIGHT
    letters(px, "BC", 10, 9, 2, WHITE)
    letters(px, "PL", 10, 20, 2, WHITE)
    return px


def page():
    px = {}
    for y in range(1, 33):
        for x in range(6, 28):
            px[x, y] = GREY if x in (6, 27) or y in (1, 32) else WHITE
    for y in range(5, 29):
        for x in range(7, 27):
            px[x, y] = RUST
    letters(px, "BC", 10, 6, 2, WHITE)
    letters(px, "PL", 10, 17, 2, WHITE)
    return px


def sprite(name, px):
    img = bytearray()
    for y in range(H):
        for x in range(W):
            r, g, b = px.get((x, y), (0, 0, 0))
            img += bytes((r, g, b, 0))
    mask_words = (W + 31) // 32
    mask = bytearray()
    for y in range(H):
        bits = 0
        for x in range(W):
            if (x, y) in px:
                bits |= 1 << x
        mask += bits.to_bytes(mask_words * 4, "little")
    head = 44
    size = head + len(img) + len(mask)
    nm = name.encode("latin-1")[:12].ljust(12, b"\0")
    return struct.pack("<I", size) + nm + struct.pack(
        "<IIIIIII", W - 1, H - 1, 0, 31, head, head + len(img), MODE) + img + mask


def main():
    sprites = [sprite("!bcpl", app()), sprite("file_a78", page())]
    body = b"".join(sprites)
    data = struct.pack("<III", len(sprites), 16, 16 + len(body)) + body
    with open(sys.argv[1], "wb") as fh:
        fh.write(data)


if __name__ == "__main__":
    main()
