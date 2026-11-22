"""Write multi-member gzip variants of a corpus file, with an index of members.

out: <f>.m1M.gz (1 MiB raw per member, level 6), <f>.bgzf.gz (BGZF: 65280 raw per member,
level 6, BC extra field, EOF block), and <name>.idx with lines "comp_off comp_len raw_len".
"""
import struct, sys, zlib

def member(data, level, bgzf):
    c = zlib.compressobj(level, zlib.DEFLATED, -15, 9)
    body = c.compress(data) + c.flush()
    trailer = struct.pack('<II', zlib.crc32(data), len(data) & 0xffffffff)
    if bgzf:
        bsize = 12 + 6 + len(body) + 8 - 1 + 6  # total block size - 1
        hdr = struct.pack('<BBBBIBBH', 0x1f, 0x8b, 8, 4, 0, 0, 0xff, 6) + b'BC' + struct.pack('<HH', 2, bsize - 6 + 6 - 6)
        # recompute exactly: header 18 bytes
        total = 18 + len(body) + 8
        hdr = struct.pack('<BBBBIBBH', 0x1f, 0x8b, 8, 4, 0, 0, 0xff, 6) + b'BC' + struct.pack('<HH', 2, total - 1)
    else:
        hdr = struct.pack('<BBBBIBB', 0x1f, 0x8b, 8, 0, 0, 0, 0xff)
    return hdr + body + trailer

def write(path, data, chunk, bgzf):
    idx = []
    with open(path, 'wb') as f:
        off = 0
        for i in range(0, len(data), chunk):
            m = member(data[i:i + chunk], 6, bgzf)
            f.write(m); idx.append((off, len(m), min(chunk, len(data) - i))); off += len(m)
        if bgzf:
            f.write(bytes.fromhex('1f8b08040000000000ff0600424302001b0003000000000000000000'))
    with open(path + '.idx', 'w') as f:
        for t in idx: f.write('%d %d %d\n' % t)

for p in sys.argv[1:]:
    data = open(p, 'rb').read()
    write(p + '.m1M.gz', data, 1 << 20, False)
    write(p + '.bgzf.gz', data, 65280, True)
