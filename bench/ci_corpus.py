"""The CI benchmark's corpus: the first 256 MiB (whole lines) of the dated Wikidata JSON dump of
16 November 2026, rebuilt from a byte range of the dump, checked against its registered digest,
and compressed as one member with `gzip -6 -n`. (The i9 race's 1280 MiB corpus is
bench/wikidata_corpus.py; this is a prefix of it, sized for a GitHub runner.)

  python3 bench/ci_corpus.py build OUT_DIR   ->  OUT_DIR/wd-256.json, OUT_DIR/wd-256.json.gz
"""

import hashlib
import os
import subprocess
import sys
import urllib.request
import zlib

URL = "https://dumps.wikimedia.org/wikidatawiki/entities/20261116/wikidata-20261116-all.json.gz"
RANGE = 64 << 20
LIMIT = 256 << 20
RAW_BYTES = 268385295
RAW_SHA256 = "5de4eebf486518c14592d6b3df4f289b26840e8b9fc2fbbc3d9e792b9efea868"


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def build(out):
    os.makedirs(out, exist_ok=True)
    raw = os.path.join(out, "wd-256.json")
    gz = raw + ".gz"
    if not (
        os.path.exists(raw) and os.path.getsize(raw) == RAW_BYTES and sha256(raw) == RAW_SHA256
    ):
        req = urllib.request.Request(
            URL, headers={"Range": f"bytes=0-{RANGE - 1}", "User-Agent": "upigz-bench"}
        )
        with urllib.request.urlopen(req) as r:
            if r.status != 206:
                sys.exit(f"ci_corpus: expected a partial response (206) from {URL}, got {r.status}")
            head = r.read()
        d = zlib.decompressobj(31)
        d.decompress(head)
        if not d.eof:
            sys.exit("ci_corpus: the dump's first member is not the expected '[' member")
        rest, d, text = d.unused_data, zlib.decompressobj(31), bytearray()
        for i in range(0, len(rest), 1 << 20):
            text += d.decompress(rest[i : i + (1 << 20)])
            if len(text) >= LIMIT:
                break
        cut = bytes(text[:LIMIT])
        cut = cut[: cut.rfind(b"\n") + 1]
        with open(raw, "wb") as f:
            f.write(cut)
        got = sha256(raw)
        if len(cut) != RAW_BYTES or got != RAW_SHA256:
            sys.exit(
                f"ci_corpus: rebuilt {len(cut)} bytes, sha256 {got}; registered {RAW_BYTES}, {RAW_SHA256}"
            )
    if not os.path.exists(gz):
        with open(gz + ".tmp", "wb") as f:
            subprocess.run(["gzip", "-6", "-n", "-c", raw], stdout=f, check=True)
        os.replace(gz + ".tmp", gz)
    print(raw)
    print(gz)


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] != "build":
        sys.exit("usage: ci_corpus.py build OUT_DIR")
    build(sys.argv[2])
