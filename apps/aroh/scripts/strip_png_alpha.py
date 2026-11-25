"""Rewrite 8-bit RGBA PNG files as RGB with no alpha channel, losslessly.

App Store Connect rejects an iOS app icon that has an alpha channel, and `tauri icon`
writes RGBA. Usage: python3 strip_png_alpha.py FILE.png [FILE.png ...]. Each file is
rewritten in place; a file that is already RGB is left as it is.
"""

import struct
import sys
import zlib

SIGNATURE = b"\x89PNG\r\n\x1a\n"


def chunks(data: bytes):
    """Yield (type, payload) for each chunk of a PNG byte string after the signature."""
    at = len(SIGNATURE)
    while at < len(data):
        (length,) = struct.unpack(">I", data[at : at + 4])
        kind = data[at + 4 : at + 8]
        yield kind, data[at + 8 : at + 8 + length]
        at += 12 + length


def chunk(kind: bytes, payload: bytes) -> bytes:
    """Encode one PNG chunk with its length and CRC."""
    crc = zlib.crc32(kind + payload) & 0xFFFFFFFF
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", crc)


def unfilter(raw: bytes, width: int, height: int, bpp: int) -> list[bytearray]:
    """Undo PNG scanline filters; returns one bytearray per row of raw pixel bytes."""
    stride = width * bpp
    rows: list[bytearray] = []
    prev = bytearray(stride)
    at = 0
    for _ in range(height):
        kind = raw[at]
        line = bytearray(raw[at + 1 : at + 1 + stride])
        at += 1 + stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if kind == 1:
                line[i] = (line[i] + a) & 0xFF
            elif kind == 2:
                line[i] = (line[i] + b) & 0xFF
            elif kind == 3:
                line[i] = (line[i] + ((a + b) >> 1)) & 0xFF
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[i] = (line[i] + pred) & 0xFF
            else:
                assert kind == 0, f"unknown filter {kind}"
        rows.append(line)
        prev = line
    return rows


def strip(path: str) -> bool:
    """Rewrite `path` as RGB if it is 8-bit RGBA; returns True when the file changed."""
    data = open(path, "rb").read()
    assert data.startswith(SIGNATURE), f"{path}: not a PNG"
    parts = list(chunks(data))
    width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", parts[0][1])
    if color == 2:
        return False
    assert color == 6 and depth == 8 and interlace == 0, f"{path}: only 8-bit non-interlaced RGBA is handled"
    raw = zlib.decompress(b"".join(p for k, p in parts if k == b"IDAT"))
    rows = unfilter(raw, width, height, 4)
    out = bytearray()
    for row in rows:
        out.append(0)
        for x in range(width):
            assert row[x * 4 + 3] == 255, f"{path}: pixel {x} is not opaque; flatten the source first"
            out += row[x * 4 : x * 4 + 3]
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    png = SIGNATURE + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(bytes(out), 9)) + chunk(b"IEND", b"")
    open(path, "wb").write(png)
    return True


if __name__ == "__main__":
    assert len(sys.argv) > 1, "usage: strip_png_alpha.py FILE.png [FILE.png ...]"
    for name in sys.argv[1:]:
        print(("stripped " if strip(name) else "already RGB ") + name)
