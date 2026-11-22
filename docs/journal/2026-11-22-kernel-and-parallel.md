# The single-thread kernel and the first parallel win (2026-11-22, session 2)

Same host as the first entry: a shared 4-vCPU VM (Sapphire Rapids-class Xeon at 2.1 GHz,
4 real cores, no hardware PMU). The build targets `x86-64-v3` (AVX2 + BMI2, the i9's ISA, no
AVX-512). All ratios are paired, shuffled-order rounds with 95% t intervals on the log scale.
Every arm is checked byte-exact against the reference before timing.

## Headline

On the 1.34 GB single-member Wikidata file (the real dump's head, recompressed with
`gzip -6`), measured end to end (page-cache read, decode, CRC-32 and ISIZE checked, bytes to
/dev/null):

| | GB/s | time vs rapidgzip -P4 |
|---|---|---|
| **upigz -P4, 1 MiB chunks (unchecked build)** | **1.21** | **0.677 [0.616, 0.744]** |
| **upigz -P4, 1 MiB chunks (safe build)** | **1.15** | **0.750 [0.698, 0.807]** (separate race) |
| upigz -P3 | 0.94 | 0.864 [0.810, 0.921] |
| rapidgzip 0.16.2 -P4 (default / `--verify`) | 0.80 / 0.82 | 1.000 / 0.980 |
| igzip (ISA-L 2.31), 1 thread | 0.81 | 1.003 |

So upigz runs at **1.33–1.48× rapidgzip's throughput on the same 4 cores**, and at P3 it
already beats rapidgzip at P4. This is one host with 4 cores. rapidgzip scales poorly here
(P2 is slower than its sequential P1 path), so the i9's 10 cores and 20 threads decide the
real ranking.

Single thread, in memory (time vs libdeflate 1.26.1; GB/s for libdeflate):

| | Wikidata -6 | source -6 | binary -6 |
|---|---|---|---|
| libdeflate | 1.106 GB/s | 0.657 | 0.546 |
| upigz kernel (unchecked) | 1.123 [1.066, 1.183] | 1.093 [1.065, 1.122] | 1.043 [0.979, 1.112] |
| upigz kernel (safe) | about 1.19–1.23 | | |
| ISA-L 2.31 | 1.172 | 1.138 | 1.149 |
| zlib-rs 0.6.8 | 1.366 | 1.410 | 1.391 |

Single-thread, upigz is second: ahead of ISA-L, zlib-rs and zlib-ng, behind libdeflate by
4–12% (unchecked) or about 20% (safe).

## The kernel (`src/fast.rs`): what moved it, step by step

Ratios are unchecked-build time vs libdeflate, on Wikidata / source / binary.

| step | ratios |
|---|---|
| v1: entry-driven consume, preload, 3 literals per refill, margins, word copies | 1.28 / 1.17 / 1.23 |
| unconditional first copy chunk (two 16-byte chunks, then libdeflate's 5×8) | no change |
| `bzhi`/`shrx` field extraction without masks, subtract the whole entry | no change |
| **libdeflate's literal→match fallthrough** (no refill on the transition) | **1.21 / 1.13 / 1.15** |
| **table build: libdeflate's doubling method for complete codes** | **1.07 / 1.09 / 1.09** |
| generic (inlined) entry constructors, 4 precode symbols per refill | 1.06 / 1.08 / 1.14 (noisy run) |

The safety tax (bounds checks the fast loop's margins make redundant) was 4–8% at v1, and is
about 10–15% now that the rest is faster.

Profiling without a PMU: timing the block headers showed **dynamic headers plus table builds
were 9–15% of decode time** at v1. Blocks average only 30–60 KB of output, so per-block fixed
costs matter. That is now down to 3.5–6.5%.

## Parallel (`src/par.rs`)

### What the marker data showed first (`markers` mode)

These are speculative runs from real block starts with an unknown window. Each was checked
symbol by symbol: literals equal the reference, and every marker resolves to the right byte.

- **Wikidata: markers never die.** The last marker lands at the end of the 16 MiB span in
  every sample, and 7–48% of output symbols are markers. Source and binary die out: median
  1.6 and 4.4 MiB, p90 8.5 and 10.2 MiB.
- **Markers are fragmented**: runs average about 2 symbols, and 24–60% of Wikidata matches copy
  at least one marker. Only 1–6K distinct window bytes are ever referenced.
- So "switch to bytes once markers die" fails on the target, and so does an out-of-band
  marker-run table. On Wikidata a 16-bit pass over the whole chunk is unavoidable.

### The pipeline

1. **Finder.** A parallel bit-by-bit scan per chunk. The prefilter checks BFINAL=0, BTYPE=2,
   HLIT and HDIST in range, and a complete precode. Then a strict header parse requires
   complete litlen and distance codes and an end-of-block code. It costs 9–23 ms for 186
   chunks on 195 MB of input.
2. **Pass 1.** Chunk 0 decodes straight to bytes. Chunk k≥1 decodes to `u16` from 32 768
   window markers, with the same generic kernel as the byte path (`Sym` trait), until a block
   starts at or past the next chunk's start. It must land on that start exactly, which
   checks the finder.
3. **Windows.** Each chunk's last 32 KiB is resolved through the previous chunk's window, in
   order.
4. **Resolve and CRC** in parallel. Per-chunk CRC-32s are combined in order
   (`crc32fast::Hasher::combine`); the CRC and ISIZE are checked against the trailer.

### Getting from 0.55 to 1.21 GB/s end to end: the same lessons as mibz2

| version | upigz -P4 time vs rapidgzip -P4 |
|---|---|
| all chunks alive at once, decode then CRC then write run in sequence (1.69 GB/s in memory, warm buffers) | 1.591 [1.194, 2.119] |
| **bounded**: at most threads+2 chunks in flight, recycled buffers, streamed output | 0.849 [0.793, 0.909] |
| resolution as a queued task (a regression: it exposed a serial chain) | 1.130 [1.029, 1.241] |
| **window hand-off taken off the resolution chain** (next window derived before a chunk is queued; separate cursors for windows and writes) | **0.677 [0.616, 0.744]** |

The first version lost because of about a million first-touch page faults (around 2.7 GB of
16-bit buffers plus 1.34 GB of bytes) and phases that ran one after another. The in-memory
harness hid this because it reuses warm buffers.

The third row is a who-waits-on-whom bug. The window for chunk k+1 came from chunk k's
symbols, but a worker could already have taken those symbols to resolve chunk k. That made
every resolution wait for the previous one.

## Open, in order

1. **The re-decode bet** (the candidate key idea; see the session chat). Pass 1 into a
   cache-resident 16-bit ring that keeps only the final window, then a real byte decode per
   chunk from its known window. That costs two decodes of compute but about 1.6 B/B of DRAM
   traffic, against roughly 5 B/B for the current marker-resolve design.
   - Measure first: pass 1 alone, against a byte decode.
   - The decisive test is a thread sweep on the i9, where the marker-resolve design should go
     memory-bound.
2. **The 16-bit pass-1 kernel is slow.** One-thread `par` is 0.47 GB/s, against 1.03 for the
   byte kernel.
3. **Robustness the prototype doesn't have:**
   - finder false positives (they error out; they don't recover);
   - chunks shorter than a window;
   - multi-member files and the dump's 2-byte leading member;
   - corrupt-input error parity with zlib;
   - fuzzing.
4. **Closing libdeflate single-thread:**
   - the remaining table-build cost;
   - multi-literal entries (ISA-L-style packed literals);
   - removing the safe-build tax without `unsafe` (restructuring so that LLVM can prove the
     bounds).
5. **On the i9:** the full thread sweep (1–20), DRAM bytes per output byte, peak RSS and
   minflt for upigz and rapidgzip. Then register the re-decode verdict rule before running.
