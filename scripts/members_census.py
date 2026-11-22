"""Count a gzip file's members with zlib (the reference): offsets, compressed and raw sizes,
and the sha256 of the whole decoded stream. usage: members_census.py FILE.gz"""
import hashlib, sys, zlib
f = open(sys.argv[1], "rb")
h = hashlib.sha256(); members = []; off = 0; raw = 0
d = zlib.decompressobj(31); start = 0; mraw = 0; buf = b""
while True:
    chunk = buf or f.read(1 << 22)
    buf = b""
    if not chunk:
        break
    out = d.decompress(chunk)
    h.update(out); mraw += len(out)
    if d.eof:
        used = len(chunk) - len(d.unused_data)
        end = off + used
        members.append((start, end - start, mraw)); raw += mraw
        buf = d.unused_data; off = end; start = end; mraw = 0
        d = zlib.decompressobj(31)
    else:
        off += len(chunk)
print(f"members {len(members)}; raw bytes {raw}; sha256 {h.hexdigest()}")
for m in members[:5]:
    print("  member at %d: %d compressed, %d raw" % m)
