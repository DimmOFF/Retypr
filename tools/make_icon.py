#!/usr/bin/env python3
"""Генерирует assets/retypr.ico: синий круг с белой буквой R (PNG внутри ICO, 16–256 px)."""
import struct, zlib, os

GLYPH = ["1111.", "1...1", "1...1", "1111.", "1.1..", "1..1.", "1...1"]
BLUE = (0x2E, 0x7D, 0xF6)
SS = 4  # суперсэмплинг для сглаживания


def render(size, ss=SS):
    n = size * ss
    # маска буквы в нормированных координатах (0..1)
    gw, gh = 5, 7
    scale = 0.62 / gh            # высота буквы ≈ 62% диаметра
    ox = 0.5 - gw * scale / 2
    oy = 0.5 - gh * scale / 2

    def in_glyph(u, v):
        gx, gy = (u - ox) / scale, (v - oy) / scale
        if 0 <= gx < gw and 0 <= gy < gh:
            return GLYPH[int(gy)][int(gx)] == "1"
        return False

    px = bytearray()
    for y in range(size):
        row = bytearray()
        for x in range(size):
            cover = glyph = 0
            for sy in range(ss):
                for sx in range(ss):
                    u = (x * ss + sx + 0.5) / n
                    v = (y * ss + sy + 0.5) / n
                    if (u - .5) ** 2 + (v - .5) ** 2 <= 0.49 ** 2:
                        cover += 1
                        if in_glyph(u, v):
                            glyph += 1
            a = round(255 * cover / (ss * ss))
            if cover:
                g = glyph / cover
                r_, g_, b_ = (round(BLUE[i] * (1 - g) + 255 * g) for i in range(3))
            else:
                r_ = g_ = b_ = 0
            row += bytes((r_, g_, b_, a))
        px += b"\x00" + row  # PNG filter 0
    return bytes(px)


def png(size, ss=None):
    ss = ss or (SS if size <= 256 else 2)
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(render(size, ss), 9)) + chunk(b"IEND", b"")


sizes = [16, 24, 32, 48, 64, 256]
images = [png(s) for s in sizes]
out = struct.pack("<HHH", 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
for s, img in zip(sizes, images):
    out += struct.pack("<BBBBHHII", 0 if s >= 256 else s, 0 if s >= 256 else s, 0, 0, 1, 32, len(img), offset)
    offset += len(img)
out += b"".join(images)

path = os.path.join(os.path.dirname(__file__), "..", "assets", "retypr.ico")
with open(path, "wb") as f:
    f.write(out)
print("written", os.path.abspath(path), len(out), "bytes")

# 1024 px PNG: source for the macOS .icns (see build-macos.sh)
png_path = os.path.join(os.path.dirname(__file__), "..", "assets", "retypr-1024.png")
with open(png_path, "wb") as f:
    f.write(png(1024))
print("written", os.path.abspath(png_path))
