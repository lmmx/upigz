# The i9 race: upigz against rapidgzip on one gzip member (registration, 2026-11-22)

Registered before any run. The rule below is fixed now, and nothing about it changes after data
exists. A run that reveals a problem with the rule is reported under this rule, and a new
registration follows.

## Why

On the 4-vCPU research host, upigz -P4 took 0.677× rapidgzip -P4's time on the 1.34 GB
single-member Wikidata file, end to end with CRC checked ([the second entry](kernel-and-parallel.md)).
That host is small and shared, and rapidgzip scales oddly on it: 0.74 GB/s on one thread, 0.48
on two. The i9-10900K workstation (10 cores, 20 threads, dual-channel DDR4) decides whether
upigz is faster than the parallel state of the art. This is the first registered run there.

## The run

`just submit bench-i9-race` on the i9, at the frozen commit (`bench/i9-race.frozen`). The recipe
refuses to run if:
- HEAD's build and workload inputs differ from the frozen candidate's: `src`, `Cargo.toml`,
  `Cargo.lock`, `.cargo`, `build.rs`, `csrc`, the toolchain file, `bench/i9_race.py`,
  `bench/wikidata_corpus.py`, `bench/bundle.sh`, `.just/bench.just` and this registration;
- the checkout has uncommitted changes, or untracked files among those inputs;
- any `CARGO_*` or `RUST*` variable that changes how the builds run is set.

What runs:
- **The corpus:** the first 1280 MiB, cut back to whole lines, of the dated Wikidata dump of
  16 November 2026 (`wikidata-20261116-all.json.gz`). It is rebuilt from a byte range of that
  file and must match its digest: 1,342,125,409 bytes, sha256 `9b4322c6…8abb8a`. It is then
  compressed as one member with `gzip -6 -n`, whose digest is recorded.
  - If Wikimedia has removed the dump, the recipe refuses rather than substitute another.
- **The decoders:**
  - upigz at the frozen commit, as two builds: the safe default (the decided arm) and the
    research `unchecked` build (reported only). Both run `upigz zcat FILE THREADS 1024`.
  - rapidgzip 0.16.2, pinned in a venv, run as `rapidgzip -P THREADS --verify -dc FILE`.
  - igzip (ISA-L) from the system, on one thread.

  Every arm checks the gzip CRC-32: upigz always does, rapidgzip because of `--verify`, igzip
  by default.
- **The arms:**
  - upigz and rapidgzip at 4, 8, 10 and 20 threads;
  - an A/A duplicate of upigz-P10;
  - the unchecked build at 10 threads;
  - igzip on one thread.
- **Validation first:** every arm decodes the corpus once, untimed, with its output hashed
  against the registered digest.
- **Timing:** 20 timed rounds after one warm-up round. Each round runs every arm once, in an
  order shuffled from a fixed seed, with output to `/dev/null`. Each run records its wall time,
  CPU time and peak RSS.

## The rule (`bench/i9_race.py report`)

- **The primary figure** is R = time(upigz-P10) / time(rapidgzip-P10): the geometric mean of
  the per-round ratios over the 20 timed rounds, with a 95% t interval on the log scale.
- **The run is VALID only if:**
  - every arm was byte-exact in the validation pass;
  - every arm has 20 timed rounds, all exiting with status 0;
  - the A/A ratio time(upigz-P10-aa) / time(upigz-P10) has its 95% interval inside
    [0.95, 1.05].
- **VALID PASS:** the run is valid, and R's interval lies wholly below 1.00, meaning upigz
  decodes the file faster than rapidgzip at 10 threads, measurably. **VALID FAIL:** the run is
  valid, and R's interval does not lie wholly below 1.00. **INVALID:** the run is not valid.
- **Reported, not decided:**
  - the same ratio at 4, 8 and 20 threads;
  - igzip (one thread) against upigz-P10;
  - the unchecked build against the safe one at 10 threads;
  - GB/s, CPU time and peak RSS for every arm.

Why 10 threads: it is the i9's physical core count, the configuration a user would choose, and
the one where memory bandwidth starts to bind. 20 threads (hyperthreads) is reported, because
how both decoders behave past the core count is informative but not the claim.

## What each outcome means

- **VALID PASS:** upigz's lead holds on the decisive machine. The post and the benchmarks page
  report the i9 figures, and the thread sweep's shape decides the next lever: the 16-bit pass-1
  kernel, or the re-decode bet (research charter F1, F3).
- **VALID FAIL:** the research host's lead doesn't transfer to 10 cores. The reported ratios at
  4 and 8 threads locate where it is lost, and CPU time and peak RSS say whether the cause is
  compute or memory.
- **INVALID:** nothing is concluded. The problems are fixed, and the run is registered again.
