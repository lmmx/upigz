# The CI benchmark's changelog

The bench version (`BENCH` in `bench/report.py`) is in every result. A major version changes
what a figure means (`MEANING` in `bench/report.py`): the history's lines never join across
one.

## 1.0.0 (2026-11-22)

- The first CI benchmark, after lmmx/mibz2's. One corpus, `wikidata`: the first 256 MiB of
  the dated Wikidata JSON dump of 2026-11-16, whole lines, compressed as one member with
  `gzip -6 -n` (`bench/ci_corpus.py`).
- Arms: upigz `zcat` and rapidgzip 0.16.2 (`--verify`) at 1, 2 and 4 threads; igzip,
  libdeflate-gunzip, pigz and gzip at 1 thread. Each decodes the whole file to `/dev/null`
  and checks the CRC-32. Every arm's output is checked against the corpus's SHA-256 first.
- 10 timed rounds after one untimed warm-up round, every cell once a round, in an order
  shuffled from a fixed seed. Wall and CPU time and peak RSS from `wait4`. Ratios are paired
  by round, against upigz at the same thread count.
- Pull requests: head against base (`upigz` and `upigz@base`) at 1, 2 and 4 threads, the same
  rounds; kept as links of the history's chain once merged (`bench/links.py`).
