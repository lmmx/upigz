"""The race corpus: the first 1280 MiB (whole lines) of the Wikidata JSON dump of 16 November
2026, rebuilt from a byte range of the dated dump and checked against its registered digest,
then compressed as one member with `gzip -6 -n`.

  python3 bench/wikidata_corpus.py build OUT_DIR   ->  OUT_DIR/wd-race.json, OUT_DIR/wd-race.json.gz

The dump's head is a 22-byte member holding "[\\n", then one 157 GB member; the first 192 MiB
of the file are fetched (a range request), the second member is stream-decompressed as far as it
goes, and the text is cut at 1280 MiB, back to the last newline. Wikimedia removes dated dumps
after some weeks: if the URL is gone, the build refuses rather than substitute another dump.
"""

import hashlib
import os
import subprocess
import sys
import urllib.request
import zlib

URL = "https://dumps.wikimedia.org/wikidatawiki/entities/20261116/wikidata-20261116-all.json.gz"
RANGE = 192 << 20
LIMIT = 1280 << 20
RAW_BYTES = 1342125409
RAW_SHA256 = "9b4322c6ae2a3614edb66fb94a1a99dcaa4344ac2cf5723199fb650fba8abb8a"


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def build(out):
    os.makedirs(out, exist_ok=True)
    raw = os.path.join(out, "wd-race.json")
    gz = raw + ".gz"
    if not (os.path.exists(raw) and os.path.getsize(raw) == RAW_BYTES and sha256(raw) == RAW_SHA256):
        req = urllib.request.Request(URL, headers={"Range": f"bytes=0-{RANGE - 1}", "User-Agent": "upigz-bench"})
        with urllib.request.urlopen(req) as r:
            if r.status != 206:
                sys.exit(f"wikidata_corpus: expected a partial response (206) from {URL}, got {r.status}")
            head = r.read()
        d = zlib.decompressobj(31)
        d.decompress(head)
        if not d.eof:
            sys.exit("wikidata_corpus: the dump's first member is not the expected 22-byte '[' member")
        rest = d.unused_data
        d = zlib.decompressobj(31)
        text = bytearray()
        for i in range(0, len(rest), 1 << 20):
            text += d.decompress(rest[i:i + (1 << 20)])
            if len(text) >= LIMIT:
                break
        cut = bytes(text[:LIMIT])
        cut = cut[: cut.rfind(b"\n") + 1]
        with open(raw, "wb") as f:
            f.write(cut)
        got = sha256(raw)
        if len(cut) != RAW_BYTES or got != RAW_SHA256:
            sys.exit(f"wikidata_corpus: rebuilt {len(cut)} bytes, sha256 {got}; registered {RAW_BYTES}, {RAW_SHA256}")
    if not os.path.exists(gz):
        with open(gz + ".tmp", "wb") as f:
            subprocess.run(["gzip", "-6", "-n", "-c", raw], stdout=f, check=True)
        os.replace(gz + ".tmp", gz)
    print(raw)
    print(gz)


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] != "build":
        sys.exit("usage: wikidata_corpus.py build OUT_DIR")
    build(sys.argv[2])
