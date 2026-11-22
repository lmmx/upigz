# upigz

[![Documentation](https://img.shields.io/badge/docs-lmmx.github.io%2Fupigz-blue)](https://lmmx.github.io/upigz/)
[![MIT](https://img.shields.io/badge/licence-MIT-green)](https://github.com/lmmx/upigz/blob/master/LICENSE)

μpigz: fast parallel gzip/DEFLATE decompression in Rust. Its target is the single-member gzip
file: the Wikidata JSON dump is one 157 GB member, so there are no independent members to hand
out to cores. upigz splits the one member into chunks anyway and decodes them in parallel:

- **a finder** scans each chunk, bit by bit, for a plausible dynamic-block header;
- **pass 1** decodes each chunk without knowing the 32 KiB of history before it, writing 16-bit
  symbols in which an unknown history byte is a marker;
- **the windows** are handed on in order: each chunk's last 32 KiB resolved through the previous
  chunk's;
- **resolution and CRC** run in parallel, and the CRC-32 and ISIZE are checked against the
  trailer.

A candidate block start is used only if the previous chunk's decode lands on it exactly, so a
false positive from the finder can cost time but never correctness (the chaining lemma,
`docs/journal/2026-11-22-speculative-theorem.md`).

It is a research prototype, not a library yet. Its single-thread kernel (`src/fast.rs`) is
libdeflate-style: entry-driven, several literals per refill, a branch-free 8-byte refill.

## Numbers so far

These come from the research host: a shared 4-vCPU cloud VM (Sapphire Rapids-class Xeon at
2.1 GHz, 4 real cores, no hardware PMU). Ratios are paired, shuffled-order rounds with 95% t
intervals, and every arm is checked byte-exact before timing. **They are indicative only: the
i9-10900K workstation, with 10 cores and 20 threads, decides the real ranking**, through
registered recipes.

On the 1.34 GB single-member Wikidata file (the real dump's head, recompressed with
`gzip -6`), end to end (page-cache read, decode, CRC-32 and ISIZE checked, bytes to /dev/null):

| | GB/s | time vs rapidgzip -P4 |
|---|---|---|
| **upigz -P4, 1 MiB chunks (unchecked build)** | **1.21** | **0.677 [0.616, 0.744]** |
| **upigz -P4, 1 MiB chunks (safe build)** | **1.15** | **0.750 [0.698, 0.807]** (separate race) |
| upigz -P3 | 0.94 | 0.864 [0.810, 0.921] |
| rapidgzip 0.16.2 -P4 (default / `--verify`) | 0.80 / 0.82 | 1.000 / 0.980 |
| igzip (ISA-L 2.31), 1 thread | 0.81 | 1.003 |

Single thread, in memory (time vs libdeflate 1.26.1; GB/s for libdeflate):

| | Wikidata -6 | source -6 | binary -6 |
|---|---|---|---|
| libdeflate | 1.106 GB/s | 0.657 | 0.546 |
| upigz kernel (unchecked) | 1.123 [1.066, 1.183] | 1.093 [1.065, 1.122] | 1.043 [0.979, 1.112] |
| upigz kernel (safe) | about 1.19–1.23 | | |
| ISA-L 2.31 | 1.172 | 1.138 | 1.149 |
| zlib-rs 0.6.8 | 1.366 | 1.410 | 1.391 |

rapidgzip scales poorly on this host (its P2 is slower than its sequential P1 path), which is
one more reason the 4-core figures can't rank the decoders. The method, the corpora and the
raw rounds are in the journals (`docs/journal/`) and `runs/`; the
[benchmarks page](https://lmmx.github.io/upigz/benchmarks/) collects them.

## Build and run

It needs Rust (the toolchain is pinned in `rust-toolchain.toml`) and ISA-L's development
files for the `isal` comparison arm (`libisal-dev` on Debian and Ubuntu). The build targets
`x86-64-v3` (AVX2 and BMI2; `.cargo/config.toml`).

```sh
cargo build --release
target/release/upigz zcat file.gz 4 1024 > file      # 4 threads, 1024 KiB chunks
target/release/upigz spectest 50                     # the speculative-decode counterexample hunt
```

`zcat <file.gz> <threads> <chunk KiB> [cap]` is the streaming parallel decoder: at most `cap`
chunks in flight (default threads + 2), output to stdout, the CRC-32 and ISIZE checked.
`UPIGZ_STATS=1` prints the chunk count and the finder's time. The other modes are the research
harnesses the journals use: the paired in-memory A/B (`upigz <file.gz> <raw> <rounds>
<arm,...>`), `par`, `cat` and `markers`.

`cargo build --release --features unchecked` builds the research arm, whose fast loop's
accesses are unchecked (the safety argument is in `src/fast.rs`).

## Status and limits

A prototype. What it doesn't do yet (`docs/team/research-charter/FRONTIER.md`, F4):
- a file with more than one gzip member, including the Wikidata dump's 22-byte leading one;
- recovering from a finder false positive (it errors out instead of merging chunks);
- chunks shorter than a window;
- error parity with zlib on corrupt input;
- fuzzing.

It is not published on crates.io, and its command line will change.

## Documentation

The site, at [lmmx.github.io/upigz](https://lmmx.github.io/upigz/), has the getting-started
notes, the benchmarks and the research blog. The journals are `docs/journal/`; the research
frontier is `docs/team/research-charter/FRONTIER.md`. The agent team that works on upigz is
described in `docs/team/`, and its working rules in `CLAUDE.md`. The team machinery and the
workstation runner are shared with [mibz2](https://github.com/lmmx/mibz2), a parallel bzip2
decoder.

## Licence

MIT: see [LICENSE](LICENSE).
