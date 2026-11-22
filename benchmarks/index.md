# Benchmarks

Every figure on this page comes from the research host, a shared 4-vCPU cloud VM (a Sapphire Rapids-class Xeon at 2.1 GHz, 4 real cores, no hardware PMU), and is copied unchanged from the journals. **They are indicative only.** The i9-10900K workstation, with 10 cores and 20 threads, decides the real ranking, through registered recipes that don't exist yet.

How the figures are made:

- **Paired rounds.** Every arm runs from one harness binary, once a round in shuffled order; the first round is dropped.
- **Ratios** are the geometric mean of per-round time ratios, with a 95% t interval on the log scale (`scripts/ratio.py`). Below 1 is faster. An A/A duplicate arm lands within about ±4%, so the harness resolves differences of about 5%.
- **Correctness first.** Every arm is checked byte-exact against the reference before timing.
- **The build** targets `x86-64-v3` (AVX2 and BMI2, the i9's instruction set; no AVX-512).

The corpora are 64 MiB each of real Wikidata JSON (the head of `latest-all.json.gz`), source code and binaries, as a single member at `gzip -6` and `-9`, as 1 MiB members (`m1M`) and as BGZF, and a 1.34 GB single-member Wikidata file for the parallel runs.

## Parallel, end to end

On the 1.34 GB single-member Wikidata file (the real dump's head, recompressed with `gzip -6`): page-cache read, decode, CRC-32 and ISIZE checked, bytes to /dev/null. From [the second entry](<https://lmmx.github.io/upigz/journal/kernel-and-parallel/index.md>).

|  | GB/s | time vs rapidgzip -P4 |
| --- | --- | --- |
| **upigz -P4, 1 MiB chunks (unchecked build)** | **1.21** | **0.677 \[0.616, 0.744\]** |
| **upigz -P4, 1 MiB chunks (safe build)** | **1.15** | **0.750 \[0.698, 0.807\]** (separate race) |
| upigz -P3 | 0.94 | 0.864 \[0.810, 0.921\] |
| rapidgzip 0.16.2 -P4 (default / `--verify`) | 0.80 / 0.82 | 1.000 / 0.980 |
| igzip (ISA-L 2.31), 1 thread | 0.81 | 1.003 |

rapidgzip scales poorly on this host (its P2 is slower than its sequential P1 path), so the i9's 10 cores decide the real ranking.

How the parallel decoder got there, upigz -P4 against rapidgzip -P4:

| version | upigz -P4 time vs rapidgzip -P4 |
| --- | --- |
| all chunks alive at once, decode then CRC then write run in sequence (1.69 GB/s in memory, warm buffers) | 1.591 \[1.194, 2.119\] |
| **bounded**: at most threads+2 chunks in flight, recycled buffers, streamed output | 0.849 \[0.793, 0.909\] |
| resolution as a queued task (a regression: it exposed a serial chain) | 1.130 \[1.029, 1.241\] |
| **window hand-off taken off the resolution chain** (next window derived before a chunk is queued; separate cursors for windows and writes) | **0.677 \[0.616, 0.744\]** |

## Single thread, in memory

Time vs libdeflate 1.26.1; GB/s for libdeflate. From [the second entry](<https://lmmx.github.io/upigz/journal/kernel-and-parallel/index.md>).

|  | Wikidata -6 | source -6 | binary -6 |
| --- | --- | --- | --- |
| libdeflate | 1.106 GB/s | 0.657 | 0.546 |
| upigz kernel (unchecked) | 1.123 \[1.066, 1.183\] | 1.093 \[1.065, 1.122\] | 1.043 \[0.979, 1.112\] |
| upigz kernel (safe) | about 1.19–1.23 |  |  |
| ISA-L 2.31 | 1.172 | 1.138 | 1.149 |
| zlib-rs 0.6.8 | 1.366 | 1.410 | 1.391 |

What moved the kernel, step by step (unchecked-build time vs libdeflate, on Wikidata / source / binary):

| step | ratios |
| --- | --- |
| v1: entry-driven consume, preload, 3 literals per refill, margins, word copies | 1.28 / 1.17 / 1.23 |
| unconditional first copy chunk (two 16-byte chunks, then libdeflate's 5×8) | no change |
| `bzhi`/`shrx` field extraction without masks, subtract the whole entry | no change |
| **libdeflate's literal→match fallthrough** (no refill on the transition) | **1.21 / 1.13 / 1.15** |
| **table build: libdeflate's doubling method for complete codes** | **1.07 / 1.09 / 1.09** |
| generic (inlined) entry constructors, 4 precode symbols per refill | 1.06 / 1.08 / 1.14 (noisy run) |

## Earlier: the first kernel and the field

These are from [the first entry](<https://lmmx.github.io/upigz/journal/interleaved-inflate/index.md>), before the kernel above: `ours-k1` is the first probe decoder.

In memory (raw DEFLATE, time vs libdeflate; GB/s for libdeflate):

| file | libdeflate GB/s | zlib-rs | zlib-ng | ours-k1 | miniz\_oxide |
| --- | --- | --- | --- | --- | --- |
| wikidata -6 | 1.131 | 1.35 | 1.43 | 1.81 | 1.86 |
| wikidata -9 | 1.174 | 1.40 | 1.47 | 1.86 | 1.89 |
| wikidata m1M | 1.210 | 1.33 | 1.46 | 1.75 | 1.88 |
| wikidata BGZF | 1.053 | 1.38 | 1.43 | 1.79 | 1.98 |
| source -6 | 0.686 | 1.36 | 1.52 | 1.74 | 2.02 |
| source -9 | 0.683 | 1.36 | 1.57 | 1.76 | 2.09 |
| source m1M | 0.692 | 1.32 | 1.47 | 1.72 | 2.00 |
| source BGZF | 0.632 | 1.34 | 1.45 | 1.64 | 1.89 |
| binary -6 | 0.574 | 1.37 | 1.49 | 1.67 | 2.02 |
| binary -9 | 0.574 | 1.39 | 1.48 | 1.72 | 1.93 |
| binary m1M | 0.583 | 1.33 | 1.40 | 1.62 | 1.96 |
| binary BGZF | 0.553 | 1.37 | 1.45 | 1.61 | 1.97 |

The command-line tools, end to end (page-cache read, inflate, CRC, write to /dev/null), GB/s, 8 paired rounds (`scripts/cli_ab.py`):

| file | igzip (ISA-L 2.31) | libdeflate-gunzip | rapidgzip -P1 | rapidgzip -P4 | pigz | gzip |
| --- | --- | --- | --- | --- | --- | --- |
| wikidata -6 | **0.821** | 0.671 | 0.497 | 0.540 | 0.346 | 0.198 |
| source -6 | 0.539 | 0.469 | 0.387 | **0.590** | 0.209 | 0.137 |
| binary -6 | 0.469 | 0.416 | 0.353 | **0.488** | 0.186 | 0.128 |
| wikidata BGZF | 0.856 | **1.060** | 0.513 | 0.883 | 0.324 | 0.198 |
| wikidata m1M | 1.041 | **1.143** | 0.552 | 1.034 | 0.375 | 0.206 |

Across cores, in memory, 1 MiB members dealt round-robin to threads:

|  | t1 | t2 | t3 | t4 |
| --- | --- | --- | --- | --- |
| wikidata GB/s | 1.158 | 2.103 | **3.231** | 2.746 |
| binary GB/s | 0.593 | 1.123 | 1.502 | 1.633 |

## Interleaving within a thread (closed)

Decoding K independent members round-robin in one thread, as time vs `ours-k1` (below 1 is faster). It doesn't pay on real data. From [the first entry](<https://lmmx.github.io/upigz/journal/interleaved-inflate/index.md>).

| file | k2 | k4 | A/A |
| --- | --- | --- | --- |
| wikidata m1M (first run, rolled loop) | 1.245 \[1.188, 1.305\] | 1.177 \[1.128, 1.228\] | 1.018 \[0.949, 1.092\] |
| wikidata m1M (unrolled) | 1.038 \[0.996, 1.081\] | 1.108 \[1.043, 1.178\] | 1.027 \[0.980, 1.077\] |
| wikidata BGZF | 1.023 \[0.924, 1.133\] | 1.084 \[1.021, 1.151\] | 0.966 \[0.900, 1.036\] |
| source m1M | 0.982 \[0.914, 1.054\] | 1.100 \[1.006, 1.204\] | 0.967 \[0.933, 1.002\] |
| source BGZF | 0.998 \[0.955, 1.042\] | 1.102 \[1.046, 1.162\] | 1.018 \[0.956, 1.083\] |
| binary m1M | 1.035 \[0.980, 1.093\] | 1.042 \[0.989, 1.098\] | 0.973 \[0.900, 1.052\] |
| binary BGZF | 1.073 \[1.010, 1.140\] | 1.129 \[1.066, 1.194\] | 1.023 \[0.963, 1.088\] |

On symbol-dense controls the mechanism exists:

| Wikidata m1M re-encoded | k2 | k3 | k4 | k8 | libdeflate |
| --- | --- | --- | --- | --- | --- |
| `Z_HUFFMAN_ONLY` | 0.781 \[0.734, 0.831\] | 0.798 | **0.775** \[0.716, 0.839\] | 0.886 | 0.599 |
| `Z_RLE` (also \~all literals) | **0.737** \[0.691, 0.785\] | 0.778 | 0.784 | — | 0.624 |

Real gzip data is match-dominated, which is why:

|  | symbols/byte | literal share of symbols | bytes from matches | mean match len | dist \<8 |
| --- | --- | --- | --- | --- | --- |
| Wikidata | **0.121** | 0.695 | 91.6% | 24.7 | 1.4% |
| source | 0.237 | 0.623 | 85.3% | 9.6 | 3.2% |
| binary | 0.249 | 0.642 | 84.0% | 9.4 | 3.0% |
