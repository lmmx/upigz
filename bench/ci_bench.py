#!/usr/bin/env python3
"""upigz's CI benchmark: whole-file decodes of one gzip member by upigz and the other gzip
decoders, in paired rounds on one runner, written in lmmx/mibz2's result shape (bench/report.py)
so docs/scripts/bench_history.py and the site's charts read it unchanged.

    python3 bench/ci_bench.py run OUT_DIR --corpus GZ --raw RAW --upigz BIN --rapidgzip BIN
        [--rounds 10] [--threads 1,2,4] [--head SHA] [--run URL]

Every arm first decodes the corpus once, untimed, and its output's sha256 must equal the raw
file's (`same_bytes`). Then each round runs every (arm, threads) cell once, in an order shuffled
from a fixed seed, output to /dev/null, recording wall and CPU time and peak RSS (wait4).
Round 0 is a warm-up, dropped. Writes OUT_DIR/result.json (the summary), OUT_DIR/raw.jsonl
(one line a timed run) and OUT_DIR/summary.md (its tables); `bench/report.py record` files them
on the bench-results branch.

Arms: upigz (`zcat`, at every thread count; 1 thread is its byte kernel, no speculation),
rapidgzip 0.16.2 (`-P t --verify`, every thread count), and the single-threaded igzip,
libdeflate-gunzip, pigz and gzip at 1 thread. Every arm checks the gzip CRC-32.
"""

import argparse
import hashlib
import json
import math
import os
import platform
import random
import re
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import report  # noqa: E402

SEED = 20261122


def cmds(a, gz):
    single = {
        "igzip": ["igzip", "-dc", gz],
        "libdeflate": ["libdeflate-gunzip", "-c", gz],
        "pigz": ["pigz", "-dc", gz],
        "gzip": ["gzip", "-dc", gz],
    }
    cells = {}
    for t in a.threads:
        cells[("upigz", t)] = [a.upigz, "zcat", gz, str(t), "1024"]
        cells[("rapidgzip", t)] = [a.rapidgzip, "-P", str(t), "--verify", "-dc", gz]
    for name, argv in single.items():
        cells[(name, 1)] = argv
    return cells


def file_sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def validate(argv):
    p = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    h = hashlib.sha256()
    for b in iter(lambda: p.stdout.read(1 << 20), b""):
        h.update(b)
    return p.wait(), h.hexdigest()


def timed(argv, null):
    with tempfile.TemporaryFile() as e:
        t0 = time.perf_counter()
        p = subprocess.Popen(argv, stdout=null, stderr=e)
        _, st, ru = os.wait4(p.pid, 0)
        wall = time.perf_counter() - t0
        code = os.waitstatus_to_exitcode(st)
        e.seek(0)
        err = e.read().decode(errors="replace")[-300:] if code else ""
    return wall, ru.ru_utime + ru.ru_stime, ru.ru_maxrss, code, err


def ratio(xs, ys):
    """[geometric mean of x/y over paired rounds, 95% interval]"""
    lr = [math.log(x / y) for x, y in zip(xs, ys) if x > 0 and y > 0]
    if len(lr) < 2:
        return None
    m, h = report.mean_ci(lr)
    return [round(math.exp(m), 4), round(math.exp(m - h), 4), round(math.exp(m + h), 4)]


def version(argv):
    """The first version number a tool prints about itself (`rustc -V`: 1.99.1)."""
    try:
        out = subprocess.run(argv, capture_output=True, text=True, timeout=30)
        m = re.search(r"\d+\.\d+(?:\.\d+)?", out.stdout + out.stderr)
        return m.group(0) if m else "unknown"
    except Exception:
        return "unknown"


def machine():
    cpu = "unknown"
    for line in open("/proc/cpuinfo"):
        if line.startswith("model name"):
            cpu = line.split(":", 1)[1].strip()
            break
    return {"cpu": cpu, "cores": os.cpu_count(), "kernel": platform.release()}


def run(a):
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    gz, raw = a.corpus, a.raw
    # streamed, so this process stays small: a forked child's peak RSS counts its pages until exec
    raw_sha, gz_sha = file_sha(raw), file_sha(gz)
    out_bytes = os.path.getsize(raw)
    cells = cmds(a, gz)
    same = {}
    for (arm, t), argv in cells.items():
        code, sha = validate(argv)
        same[(arm, t)] = code == 0 and sha == raw_sha
        print(f"validate {arm} @{t}: {'byte-exact' if same[(arm, t)] else 'NOT EXACT'}", flush=True)
    rng = random.Random(SEED)
    order = list(cells)
    runs = []
    with open(os.devnull, "wb") as null:
        for r in range(a.rounds + 1):
            rng.shuffle(order)
            for arm, t in order:
                wall, cpu, rss, code, err = timed(cells[(arm, t)], null)
                if r == 0:
                    continue
                runs.append(
                    {
                        "round": r,
                        "arm": arm,
                        "corpus": "wikidata",
                        "threads": t,
                        "wall": round(wall, 5),
                        "cpu": round(cpu, 4),
                        "rss_kb": rss,
                        "out": out_bytes,
                        "err": err if code else None,
                    }
                )
            print(f"round {r}/{a.rounds}", flush=True)
    # the summary, in mibz2's shape
    figures = {"wikidata": {}}
    ghz = None
    for t in sorted({t for _, t in cells}):
        arms = {}
        ref = [x for x in runs if x["arm"] == "upigz" and x["threads"] == t]
        ref_by = {x["round"]: x for x in ref}
        for arm in [a_ for a_, tt in cells if tt == t]:
            rs = [x for x in runs if x["arm"] == arm and x["threads"] == t and not x["err"]]
            if not rs:
                continue
            walls = sorted(x["wall"] for x in rs)
            med = lambda v: sorted(v)[len(v) // 2]
            f = {
                "arm": arm,
                "parallel": arm in ("upigz", "rapidgzip"),
                "threads_run": t,
                "same_bytes": same[(arm, t)],
                "n": len(rs),
                "wall": round(med(walls), 4),
                "wall_best": round(walls[0], 4),
                "cpu": round(med([x["cpu"] for x in rs]), 4),
                "rss_mb": round(max(x["rss_kb"] for x in rs) / 1024, 1),
                "out": out_bytes,
                "mb_s": round(out_bytes / 1e6 / med(walls), 1),
                "errors": [],
            }
            if arm != "upigz" and ref_by:
                pair = [(x, ref_by[x["round"]]) for x in rs if x["round"] in ref_by]
                for k, key in (("wall", "wall"), ("cpu", "cpu"), ("rss", "rss_kb")):
                    f[f"{k}_ratio"] = ratio([p[0][key] for p in pair], [p[1][key] for p in pair])
            arms[arm] = f
        figures["wikidata"][str(t)] = {
            "arms": arms,
            "same_bytes": all(v["same_bytes"] for v in arms.values()),
        }
    res = {
        "bench": report.BENCH,
        "mode": "master",
        "head": a.head,
        "base": None,
        "date": datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
        "run": a.run,
        "rounds": a.rounds,
        "threads": a.threads,
        "machine": machine(),
        "toolchain": {
            "rustc": version(["rustc", "-V"]),
            "rapidgzip": version([a.rapidgzip, "--version"]),
            "igzip": version(["igzip", "--version"]),
            "libdeflate": version(["libdeflate-gunzip", "-V"]),
            "pigz": version(["pigz", "--version"]),
            "gzip": version(["gzip", "--version"]),
        },
        "arms": sorted({arm for arm, _ in cells} - {"upigz"}),
        "parallel": {
            arm: arm in ("upigz", "rapidgzip")
            for arm in ["upigz"] + sorted({arm for arm, _ in cells} - {"upigz"})
        },
        "reference": "upigz",
        "corpora": {
            "wikidata": {
                "file": os.path.basename(gz),
                "bytes": os.path.getsize(gz),
                "out": out_bytes,
                "sha256": raw_sha,
                "gz_sha256": gz_sha,
                "source": "wikidata-20261116-all.json.gz: the first 256 MiB, whole lines, gzip -6 -n (bench/ci_corpus.py)",
            }
        },
        "parity": all(same.values()),
        "figures": figures,
    }
    (out / "result.json").write_text(json.dumps(res, indent=1) + "\n")
    (out / "raw.jsonl").write_text("".join(json.dumps(x) + "\n" for x in runs))
    (out / "summary.md").write_text(report.markdown(res))
    print(report.markdown(res))
    return 0 if res["parity"] else 1


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("out")
    r.add_argument("--corpus", required=True)
    r.add_argument("--raw", required=True)
    r.add_argument("--upigz", required=True)
    r.add_argument("--rapidgzip", required=True)
    r.add_argument("--rounds", type=int, default=10)
    r.add_argument("--threads", type=lambda s: [int(x) for x in s.split(",")], default=[1, 2, 4])
    r.add_argument("--head", default="unknown")
    r.add_argument("--run")
    a = ap.parse_args()
    sys.exit(run(a))


if __name__ == "__main__":
    main()
