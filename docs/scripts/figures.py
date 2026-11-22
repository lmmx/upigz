"""Build the result charts and their data file from the raw rounds in runs/.

The single source of truth is runs/*.txt (one line per timed run: "round arm ns bytes"). This
script recomputes each ratio the way scripts/ratio.py does (the geometric mean of per-round
time ratios against a reference arm, round 0 dropped, a 95% t interval on the log scale) and
writes:

- docs/site/assets/data/results.json, the numbers;
- docs/figures/parallel.svg and docs/figures/single-thread.svg, inline-SVG charts styled by the
  site's .bf-fig classes (light and dark), included into pages with pymdownx.snippets.

usage: python docs/scripts/figures.py [--check]   (--check: fail if the outputs are stale)
"""

import json
import math
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
T975 = {1: 12.71, 2: 4.30, 3: 3.18, 4: 2.78, 5: 2.57, 6: 2.45, 7: 2.36, 8: 2.31, 9: 2.26, 10: 2.23,
        12: 2.18, 15: 2.13, 20: 2.09, 25: 2.06, 30: 2.04, 40: 2.02, 60: 2.00, 120: 1.98}


def t975(df):
    return T975[min((k for k in T975 if k >= df), default=120)]


def summarise(path, ref, skip=1):
    runs, size = defaultdict(dict), 0
    for line in (ROOT / path).read_text().splitlines():
        r, arm, ns, b = line.split()
        if int(r) < skip:
            continue
        runs[int(r)][arm] = int(ns)
        size = int(b)
    out = {}
    arms = sorted({a for d in runs.values() for a in d})
    for a in arms:
        ts = sorted(d[a] for d in runs.values() if a in d)
        lr = [math.log(d[a] / d[ref]) for d in runs.values() if a in d and ref in d]
        n = len(lr)
        m = sum(lr) / n
        sd = math.sqrt(sum((x - m) ** 2 for x in lr) / (n - 1)) if n > 1 else 0.0
        h = t975(n - 1) * sd / math.sqrt(n) if n > 1 else 0.0
        out[a] = {"gbps": round(size / ts[len(ts) // 2], 3), "ratio": round(math.exp(m), 3),
                  "lo": round(math.exp(m - h), 3), "hi": round(math.exp(m + h), 3), "rounds": n}
    return {"source": path, "reference": ref, "bytes": size, "arms": out}


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def parallel_svg(d):
    a = d["arms"]
    rows = [  # (label, arm, class, note)
        ("upigz -P4", "upigz-P4-1M", "bf-green", "1 MiB chunks"),
        ("upigz -P3", "upigz-P3-2M", "bf-green", "2 MiB chunks"),
        ("rapidgzip -P4", "rapidgzip-P4", "bf-steel", "reference"),
        ("igzip, 1 thread", "igzip-1t", "bf-steel", ""),
        ("upigz -P2", "upigz-P2-2M", "bf-green", "2 MiB chunks"),
    ]
    W, left, top, rh, bw = 720, 132, 30, 40, 400
    xmax = 1.4
    h = top + rh * len(rows) + 34
    s = [f'<svg viewBox="0 0 {W} {h}" role="img" aria-label="'
         + esc("; ".join(f"{l}: {a[k]['gbps']:.2f} GB/s, {a[k]['ratio']:.3f} of rapidgzip -P4's time" for l, k, _, _ in rows))
         + '">']
    s.append(f'<text x="{left}" y="16" class="bf-tm">GB/s, end to end with CRC (higher is better)</text>')
    s.append(f'<text x="{W}" y="16" class="bf-tm" text-anchor="end">time vs rapidgzip -P4 [95% CI]</text>')
    for i, (label, k, cls, note) in enumerate(rows):
        y = top + i * rh
        v = a[k]
        w = bw * v["gbps"] / xmax
        s.append(f'<text x="{left - 10}" y="{y + 15}" class="bf-t-strong" text-anchor="end">{esc(label)}</text>')
        if note:
            s.append(f'<text x="{left - 10}" y="{y + 29}" class="bf-tm" text-anchor="end">{esc(note)}</text>')
        s.append(f'<rect x="{left}" y="{y + 4}" width="{bw}" height="16" rx="4" class="bf-track"/>')
        s.append(f'<rect x="{left}" y="{y + 4}" width="{w:.1f}" height="16" rx="4" class="{cls}"/>')
        s.append(f'<text x="{left + w + 6:.1f}" y="{y + 17}" class="bf-t">{v["gbps"]:.2f}</text>')
        r = "1.000" if k == d["reference"] else f'{v["ratio"]:.3f} [{v["lo"]:.3f}, {v["hi"]:.3f}]'
        s.append(f'<text x="{W}" y="{y + 17}" class="bf-t-strong" text-anchor="end">{r}</text>')
    yb = top + rh * len(rows) + 6
    s.append(f'<path d="M{left} {yb}H{left + bw}" class="bf-axis"/>')
    for g in (0, 0.5, 1.0):
        x = left + bw * g / xmax
        s.append(f'<path d="M{x:.1f} {yb}v5" class="bf-axis"/><text x="{x:.1f}" y="{yb + 18}" class="bf-tm" text-anchor="middle">{g:.1f}</text>')
    s.append("</svg>")
    return "".join(s)


def single_svg(rows_by_corpus, title):
    """A dot-and-whisker chart, one group per corpus. Each group's label has a line of its own
    above its rows, so it never collides with the row labels."""
    W, left, top, rh, gh, gap = 720, 190, 34, 22, 22, 12
    x0, x1, pw = 0.9, 1.55, 470
    X = lambda v: left + pw * (v - x0) / (x1 - x0)
    label, s = [], []
    s.append(f'<text x="{left}" y="14" class="bf-tm">{esc(title)}</text>')
    y = top
    for cname, rows in rows_by_corpus:
        s.append(f'<text x="0" y="{y + 14}" class="bf-ts">{esc(cname)}</text>')
        y += gh
        for an, v, cls in rows:
            cy = y + 10
            s.append(f'<text x="{left - 10}" y="{cy + 4}" class="bf-t" text-anchor="end">{esc(an)}</text>')
            s.append(f'<path d="M{X(v["lo"]):.1f} {cy}H{X(v["hi"]):.1f}" class="bf-whisker"/>')
            s.append(f'<circle cx="{X(v["ratio"]):.1f}" cy="{cy}" r="5" class="{cls}"/>')
            s.append(f'<text x="{W}" y="{cy + 4}" class="bf-t-strong" text-anchor="end">{v["ratio"]:.3f}</text>')
            label.append(f"{cname}, {an}: {v['ratio']:.3f} [{v['lo']:.3f}, {v['hi']:.3f}]")
            y += rh
        y += gap
    s.append(f'<path d="M{X(1.0):.1f} {top - 6}V{y - gap + 4}" class="bf-ref"/>')
    yb = y - gap + 8
    s.append(f'<path d="M{left} {yb}H{left + pw}" class="bf-axis"/>')
    for g in (1.0, 1.1, 1.2, 1.3, 1.4, 1.5):
        x = X(g)
        s.append(f'<path d="M{x:.1f} {yb}v5" class="bf-axis"/><text x="{x:.1f}" y="{yb + 18}" class="bf-tm" text-anchor="middle">{g:.1f}</text>')
    head = f'<svg viewBox="0 0 {W} {yb + 24}" role="img" aria-label="' + esc("; ".join(label)) + '">'
    return head + "".join(s) + "</svg>"


CORPORA = (("Wikidata JSON", "wikidata", "wikidata.json"), ("source code", "source", "source.tar"),
           ("binaries", "binary", "binary.tar"))


def build():
    par = summarise("runs/race-big4.txt", "rapidgzip-P4")
    safe = summarise("runs/race-safe.txt", "rapidgzip-P4")
    # the first single-thread race (the post of 2026-11-22 shows it as published)
    single = {k: summarise(f"runs/sota-{f}.txt", "libdeflate") for _, k, f in CORPORA}
    # the kernel after #16, both builds, against libdeflate in the same binary
    now = {b: {k: summarise(f"runs/k1-{b}-{f}.txt", "libdeflate") for _, k, f in CORPORA}
           for b in ("unchecked", "release")}
    data = {
        "host": "research host: shared 4-vCPU cloud VM (Sapphire Rapids-class Xeon, 2.1 GHz, no PMU); indicative only",
        "parallel": par, "parallel_safe": safe, "single_thread": single, "single_thread_now": now,
    }
    first = [(c, [("upigz kernel", single[k]["arms"]["fast"], "bf-green"),
                  ("ISA-L 2.31", single[k]["arms"]["isal"], "bf-steel"),
                  ("zlib-rs 0.6.8", single[k]["arms"]["zlib-rs"], "bf-steel")]) for c, k, _ in CORPORA]
    current = [(c, [("upigz, unchecked build", now["unchecked"][k]["arms"]["fast"], "bf-green"),
                    ("upigz, safe build", now["release"][k]["arms"]["fast"], "bf-green"),
                    ("ISA-L 2.31 (first race)", single[k]["arms"]["isal"], "bf-steel"),
                    ("zlib-rs 0.6.8 (first race)", single[k]["arms"]["zlib-rs"], "bf-steel")]) for c, k, _ in CORPORA]
    t = "time vs libdeflate 1.26.1, one thread, in memory (lower is better; 1.0 = libdeflate)"
    return {
        "docs/site/assets/data/results.json": json.dumps(data, indent=2) + "\n",
        "docs/figures/parallel.svg": parallel_svg(par) + "\n",
        "docs/figures/single-thread.svg": single_svg(current, t) + "\n",
        "docs/figures/single-thread-2026-11-22-first.svg": single_svg(first, t) + "\n",
    }


if __name__ == "__main__":
    outs = build()
    stale = [p for p, c in outs.items() if not (ROOT / p).exists() or (ROOT / p).read_text() != c]
    if "--check" in sys.argv:
        if stale:
            sys.exit("stale figure outputs (run python docs/scripts/figures.py): " + ", ".join(stale))
    else:
        for p, c in outs.items():
            (ROOT / p).write_text(c)
        print("wrote", ", ".join(outs))
