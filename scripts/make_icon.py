# Zero-dependency 1024x1024 PNG: sticky-note yellow with a folded bottom-right corner.
import zlib, struct, sys
W = H = 1024
F = 240  # fold size
def px(x, y):
    if x > W - F and y > H - F and (x - (W - F)) + (y - (H - F)) > F:
        return (230, 196, 66)   # darker fold
    return (255, 221, 92)       # pinleaf yellow
raw = bytearray()
for y in range(H):
    raw.append(0)  # filter type 0 for the scanline
    row = bytearray()
    for x in range(W):
        row += bytes(px(x, y))
    raw += row
def chunk(typ, data):
    return (struct.pack(">I", len(data)) + typ + data
            + struct.pack(">I", zlib.crc32(typ + data) & 0xffffffff))
png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
png += chunk(b"IEND", b"")
out = sys.argv[1]
open(out, "wb").write(png)
print("wrote", out, len(png), "bytes")
