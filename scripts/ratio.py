"""Summarise paired rounds: GB/s per arm and per-round paired ratios vs a reference arm.

usage: ratio.py <runs.txt> <ref-arm> [skip-rounds]
Each line of runs.txt: "round arm ns bytes". Ratio = arm time / ref time (below 1 = faster),
as the geometric mean of per-round ratios, with a 95% t interval on the log scale.
"""
import math, sys
from collections import defaultdict

T975 = {1: 12.71, 2: 4.30, 3: 3.18, 4: 2.78, 5: 2.57, 6: 2.45, 7: 2.36, 8: 2.31, 9: 2.26, 10: 2.23,
        12: 2.18, 15: 2.13, 20: 2.09, 25: 2.06, 30: 2.04, 40: 2.02, 60: 2.00, 120: 1.98}
def t(df):
    return T975[min((k for k in T975 if k >= df), default=120)]

path, ref = sys.argv[1], sys.argv[2]
skip = int(sys.argv[3]) if len(sys.argv) > 3 else 1
runs = defaultdict(dict); size = 0
for l in open(path):
    r, a, ns, b = l.split(); r = int(r)
    if r < skip: continue
    runs[r][a] = int(ns); size = int(b)
arms = sorted({a for d in runs.values() for a in d}, key=lambda a: (a != ref, a))
print(f"{'arm':<14}{'GB/s med':>9}{'  ratio vs ' + ref:>22}  {'95% CI':>16}  n")
for a in arms:
    ts = sorted(d[a] for d in runs.values() if a in d)
    med = ts[len(ts) // 2]
    lr = [math.log(d[a] / d[ref]) for d in runs.values() if a in d and ref in d]
    n = len(lr); m = sum(lr) / n
    sd = math.sqrt(sum((x - m) ** 2 for x in lr) / (n - 1)) if n > 1 else float('nan')
    h = t(n - 1) * sd / math.sqrt(n) if n > 1 else float('nan')
    print(f"{a:<14}{size / med:>9.3f}{math.exp(m):>22.3f}  [{math.exp(m - h):.3f}, {math.exp(m + h):.3f}]  {n}")
