"""upigz's CI benchmark results: the summary shape, the statistics and the tables.

Modelled on lmmx/mibz2's bench/report.py, whose result shape (`figures`, `corpora`,
`machine`, ...) docs/scripts/bench_history.py and the site's bench.js read unchanged. Only the
parts those need are here: the meaning of a bench version, a mean and its 95% interval, arm
labels, and the Markdown tables of the Alternatives page.

MEANING: the bench major that last changed what a figure means (bench/CHANGELOG.md). Figures
compare only within one meaning, one runner CPU model and one set of corpora.
"""

import argparse
import json
import math
import statistics
from pathlib import Path

BENCH = "1.0.0"
MEANING = {"decoder": 1}
MARK = "<!-- upigz-bench -->"

T975 = {
    1: 12.71,
    2: 4.30,
    3: 3.18,
    4: 2.78,
    5: 2.57,
    6: 2.45,
    7: 2.36,
    8: 2.31,
    9: 2.26,
    10: 2.23,
    12: 2.18,
    15: 2.13,
    20: 2.09,
    25: 2.06,
    30: 2.04,
    40: 2.02,
    60: 2.00,
    120: 1.98,
}


def t(df: int) -> float:
    """The two-sided 95% t quantile for df degrees of freedom (a table, rounded up)."""
    return T975[min((k for k in T975 if k >= df), default=120)]


# (mibz2's name for it, which docs/scripts/bench_history.py calls)
t95 = t


def mean_ci(values: list[float]) -> tuple[float, float]:
    """The mean and the half-width of its 95% t interval (0 for one value)."""
    m = statistics.fmean(values)
    if len(values) < 2:
        return m, 0.0
    return m, t(len(values) - 1) * statistics.stdev(values) / math.sqrt(len(values))


def meaning_major(kind: str, bench: str) -> int:
    """The bench major that last set the meaning of `kind`'s figures, at bench version `bench`."""
    major = int(bench.split(".")[0])
    return min(major, MEANING[kind])


def label(arm: str) -> str:
    return arm


def _fmt_ratio(r) -> str:
    if not r:
        return ""
    return f"{r[0]:.3f} [{r[1]:.3f}, {r[2]:.3f}]"


def markdown(res: dict) -> str:
    """The result as Markdown: per corpus and thread count, every arm's figures against upigz."""
    m, tc = res["machine"], res.get("toolchain", {})
    run = f" · [run]({res['run']})" if res.get("run") else ""
    lines = [MARK, "## Benchmark: upigz decoder", ""]
    lines.append(
        f"**`{res['head'][:7]}`, bench {res['bench']}**, {res['rounds']} rounds, on {m['cpu']}"
        f" ({m['cores']} cores){run}. Each arm decodes the whole file end to end (CRC checked),"
        " output to /dev/null; ratios are each arm's time over upigz's at the same thread count,"
        " paired by round, with a 95% interval (below 1 is faster than upigz)."
    )
    lines.append("")
    if tc:
        lines.append("Versions: " + ", ".join(f"{k} {v}" for k, v in tc.items()) + ".")
        lines.append("")
    for corpus, cells in res["figures"].items():
        c = res["corpora"][corpus]
        lines.append(f"### {corpus}")
        lines.append("")
        lines.append(
            f"{c.get('file', corpus)}: {c['bytes']:,} bytes compressed, {c['out']:,} decoded. {c.get('source', '')}"
        )
        lines.append("")
        lines.append("| threads | arm | MB/s | wall s | CPU s | peak RSS MB | wall vs upigz |")
        lines.append("|---|---|---|---|---|---|---|")
        for th, cell in sorted(cells.items(), key=lambda kv: int(kv[0])):
            for arm, f in cell["arms"].items():
                if not f.get("n"):
                    continue
                lines.append(
                    f"| {th} | {arm} | {f['mb_s']:.0f} | {f['wall']:.3f} | {f['cpu']:.3f} | {f['rss_mb']:.1f} |"
                    f" {_fmt_ratio(f.get('wall_ratio')) or '1 (reference)'} |"
                )
        lines.append("")
    return "\n".join(lines) + "\n"


STORE_README = """# Benchmark results

Data only, never merged: master's benchmark results (`.github/workflows/bench.yml`,
`bench/ci_bench.py`), in the layout of lmmx/mibz2's bench-results branch.

- `records.jsonl`: a line a master commit the Benchmark workflow ran on: its SHA, date,
  bench version, runner CPU and cores, rounds, corpora and the run's URL.
- `runs/<commit>.json`: its result: every figure, the machine, the toolchain and the corpora's
  sizes and SHA-256.
- `raw/<commit>.jsonl`: the harness's records, one line a run.
- `links.jsonl`, `pr/<commit>.json`, `pr-raw/<commit>.jsonl`: each merged pull request's own
  paired run, head against base (bench/links.py): a link of the history's chain. Merges from
  before the pull request runs are linked by a backfill, their merge against its first parent.

A master record's figure compares only within the bench major that last set its meaning
(MEANING in bench/report.py), one runner CPU model and one set of corpora.
"""


def record(a: argparse.Namespace) -> None:
    """Append a master result to a checkout of the bench-results branch (as lmmx/mibz2's)."""
    store, res = Path(a.store), json.loads(Path(a.result).read_text())
    sha = res["head"]
    for sub in ("runs", "raw"):
        (store / sub).mkdir(parents=True, exist_ok=True)
    readme = store / "README.md"
    if not readme.exists() or readme.read_text() != STORE_README:
        readme.write_text(STORE_README)
    (store / "runs" / f"{sha}.json").write_text(Path(a.result).read_text())
    (store / "raw" / f"{sha}.jsonl").write_text(Path(a.runs).read_text())
    line = {
        "commit": sha,
        "date": res["date"],
        "bench": res["bench"],
        "cpu": res["machine"]["cpu"],
        "cores": res["machine"]["cores"],
        "rounds": res["rounds"],
        "run": res["run"],
        "corpora": sorted(res["corpora"]),
        "parity": res["parity"],
    }
    with open(store / "records.jsonl", "a") as f:
        f.write(json.dumps(line) + "\n")
    print(json.dumps(line))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("record")
    r.add_argument("store")
    r.add_argument("result")
    r.add_argument("runs")
    a = ap.parse_args()
    {"record": record}[a.cmd](a)


if __name__ == "__main__":
    main()
