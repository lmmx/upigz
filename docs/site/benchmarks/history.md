---
description: "upigz's decoding speed, CPU time and peak memory, commit by commit on master: the CI benchmark's absolute figures on one runner CPU model at a time."
hide:
  - navigation
  - toc
---

# History

How upigz's decoding speed, CPU time and peak memory have changed on master, measured on
GitHub's runners by the CI benchmark (`.github/workflows/bench.yml`,
[`bench/ci_bench.py`](https://github.com/lmmx/upigz/blob/master/bench/ci_bench.py)). The
machinery, the records' layout and these charts are lmmx/mibz2's.

- **The workload.** One gzip member: the first 256 MiB of the dated Wikidata JSON dump of
  16 November 2026, whole lines, compressed with `gzip -6 -n` (40 MB). Every arm's output is
  checked against its SHA-256 before any timing.
- **The runs.** Each push to master that touches the decoder decodes it whole with that
  commit's `upigz zcat` at 1, 2 and 4 threads, beside the other decoders
  ([Alternatives](alternatives.md)), for 10 shuffled rounds on one runner. At 1 thread upigz
  runs its byte kernel, with no speculation.
- **One runner CPU model at a time.** GitHub's runners vary in CPU model. A line joins only
  records from the same model, and the chart shows the model with the most records by
  default. These figures are a trend, not a ranking: a ranking is decided on the i9-10900K
  ([the research charter](https://github.com/lmmx/upigz/blob/master/docs/team/research-charter/README.md)).
- **A line never joins** another bench meaning (a version that redefined the figures;
  [the benchmark's changelog](https://github.com/lmmx/upigz/blob/master/bench/CHANGELOG.md))
  or another content of a corpus.
- **Merge by merge, chained** (the chart at the top). Each pull request's benchmark runs its
  head against its base on one runner, both builds, for 10 rounds in a shuffled order. Each
  merge keeps that paired change ([`bench/links.py`](https://github.com/lmmx/upigz/blob/master/bench/links.py)),
  and the chart multiplies the changes. The band is the product's 95% interval. Merges from
  before pull requests were benchmarked are backfilled: the merge against its first parent,
  run the same way. A merge that left the decoder alone is an A/A pair, drawn hollow, and
  adds nothing.
- **Absolute figures** (the charts below it) are master's records: each merge's upigz in
  MB/s, CPU seconds and peak RSS, on whichever runner the run drew. The backfilled merges
  share one runner, so they are directly comparable.

<div id="bench-history" data-src="../../assets/bench-history.json" markdown>
Loading the benchmark history…
</div>

The raw records are on the
[`bench-results`](https://github.com/lmmx/upigz/tree/bench-results) branch: `records.jsonl`, a
line a recorded commit; `runs/<commit>.json`, its figures; `raw/<commit>.jsonl`, every timed
run.
