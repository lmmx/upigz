# Benchmark results

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
