"""1 MiB-member gzip of a corpus file with a given zlib strategy (and its .idx).
usage: members_strategy.py <file> <strategy: huff|rle|fixed> <out>"""
import struct, sys, zlib
p, strat, outp = sys.argv[1:4]
S = {'huff': zlib.Z_HUFFMAN_ONLY, 'rle': zlib.Z_RLE, 'fixed': zlib.Z_FIXED}[strat]
data = open(p, 'rb').read(); idx = []; off = 0
with open(outp, 'wb') as f:
    for i in range(0, len(data), 1 << 20):
        d = data[i:i + (1 << 20)]
        c = zlib.compressobj(6, zlib.DEFLATED, -15, 9, S)
        m = struct.pack('<BBBBIBB', 0x1f, 0x8b, 8, 0, 0, 0, 0xff) + c.compress(d) + c.flush() + struct.pack('<II', zlib.crc32(d), len(d))
        f.write(m); idx.append((off, len(m), len(d))); off += len(m)
open(outp + '.idx', 'w').write(''.join('%d %d %d\n' % t for t in idx))
