"""The zbMATH Open KG dump, zbmath-pq's real input: fetched from Zenodo and checked.

  python3 bench/zbmath_corpus.py fetch OUT_DIR   ->  OUT_DIR/zbmath-kg-ttl-2026.tar.gz

One gzip member (no FEXTRA, so not BGZF) holding a tar of 11 Turtle files: 1,621,106,686
bytes compressed, 10,765,271,040 bytes decoded. Checked by Zenodo's md5 and by the sha256 of
the file and of its decoded bytes, recorded when it was first fetched (2026-11-22).
"""

import hashlib
import os
import sys
import urllib.request

URL = "https://zenodo.org/records/21497976/files/zbmath-kg-ttl-2026.tar.gz?download=1"
NAME = "zbmath-kg-ttl-2026.tar.gz"
BYTES = 1621106686
MD5 = "0846954acba8f9149a30cb91910d3c94"
SHA256 = "c88225a0e9f742d3725bfe84a0755decf3ef37eb3c6d7e9824ab18875b962079"
RAW_BYTES = 10765271040
RAW_SHA256 = "6f0781f6675cccd3edf74276ad2998a453df1d2e41fcc8d69308774fc08f07be"


def digests(path):
    m, s = hashlib.md5(), hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 22), b""):
            m.update(b)
            s.update(b)
    return m.hexdigest(), s.hexdigest()


def fetch(out):
    os.makedirs(out, exist_ok=True)
    path = os.path.join(out, NAME)
    if not (os.path.exists(path) and os.path.getsize(path) == BYTES and digests(path) == (MD5, SHA256)):
        req = urllib.request.Request(URL, headers={"User-Agent": "upigz-bench"})
        with urllib.request.urlopen(req) as r, open(path + ".tmp", "wb") as f:
            for b in iter(lambda: r.read(1 << 22), b""):
                f.write(b)
        os.replace(path + ".tmp", path)
        got = (os.path.getsize(path),) + digests(path)
        if got != (BYTES, MD5, SHA256):
            sys.exit(f"zbmath_corpus: got {got}; registered {(BYTES, MD5, SHA256)}")
    print(path)


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] != "fetch":
        sys.exit("usage: zbmath_corpus.py fetch OUT_DIR")
    fetch(sys.argv[2])
