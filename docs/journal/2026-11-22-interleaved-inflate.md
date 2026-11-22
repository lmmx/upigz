# Does mibz2's interleaving idea work for gzip? (2026-11-22)

**Verdict: no, not within a thread.** On real data, decoding K independent DEFLATE members
round-robin in one thread is never faster than decoding them one after another. It is often
slower: K=2 takes 0.98–1.09× and K=4 1.04–1.16× the sequential time, across three corpora and
two member layouts. The mechanism exists: on symbol-dense streams (one literal per byte),
interleaving makes decoding about 1.28× faster. But real gzip data is match-dominated
(0.12–0.25 symbols per output byte), so the serial bit-position chain that interleaving hides
is only about a third of the decode time.

The parallelism that pays is across cores. On independent members, libdeflate gets 2.1 GB/s
from 2 threads and 3.2 GB/s from 3, against 1.16 GB/s on one. Per the brief's rule ("only if
step 2 wins"), the speculative single-member step was not started.

Host: a shared 4-vCPU cloud VM (Intel Xeon @ 2.1 GHz, model not exposed). It has no hardware
PMU: `perf_event_open` returns ENOENT for every hardware event, so the mechanism below is
inferred from controlled variants and generated code, not from counters. Timings are
indicative. The ratios are paired, but the absolute GB/s will move on the i9.

## Question and the rule fixed before the data

Louis asked whether mibz2's first idea, interleaved chains for memory-level parallelism,
works for gzip. In mibz2, the interleaved inverse-BWT walks hide DRAM latency. In inflate, the
serial chain is the bit position (each code's start depends on the previous code's length),
and window copies mostly hit L1/L2. So interleaving could only hide *decode* latency, via
instruction-level parallelism across streams.

The rule, from the brief: run the speculative single-member work only if K-way interleaving
in one thread beats sequential decoding of the same K members. Ratios are reported with 95%
intervals over paired, shuffled-order rounds. It was stated in the brief rather than frozen
in a registration file, which this scratch setup doesn't have yet.

## Setup

- **Probe decoder** (`src/inflate.rs`): a minimal safe-Rust inflate.
  - 11-bit litlen and 8-bit distance tables with subtables.
  - Lengths and distances precombined with their extra bits.
  - libdeflate's branch-free 8-byte refill (≥56 bits per symbol, one refill per symbol).
  - 8-byte word copies when distance ≥8; fill when distance =1; byte loop otherwise.
  - The hot loop is `hotN::<B>`: N streams advanced round-robin, B symbols at a time. It is
    unrolled by macro with literal indices, so each stream's state is a scalar. **K=1 is the
    same code**, so `ours-k1` vs `ours-kN` isolates interleaving.
- **Baselines, in memory, raw DEFLATE body into a preallocated buffer, no CRC:** libdeflate
  1.26.1 (`libdeflater`), zlib-ng (`libz-ng-sys` 1.1.33), zlib-rs 0.6.8 (`libz-rs-sys`) and
  miniz_oxide 0.9.1.
- **Correctness:** every arm on every file is checked byte-exact against the original before
  timing, with output buffers poisoned between arms. Each of our members must end exactly at
  its trailer.
- **Corpus**, 64 MiB of each:
  - real Wikidata JSON, the head of `latest-all.json.gz`;
  - source code (Python stdlib plus `/usr/include` as a tar);
  - binaries (`/usr/bin` plus shared libraries as a tar).

  Each comes as a single member at gzip -6 and -9, as 1 MiB members (level 6, 64 per file),
  and as BGZF (65 280 raw bytes per member, about 1 029 members). Literal-only controls use
  zlib's `Z_HUFFMAN_ONLY` and `Z_RLE` strategies.
- **Harness** (`src/main.rs`): one binary, pinned to one core (`taskset -c 2`). Each round
  runs every arm once in shuffled order; round 0 is dropped. The ratio is the geometric mean
  of per-round time ratios, with a t interval on the log scale (`scripts/ratio.py`).
  `ours-k1-aa` is an A/A duplicate: it lands within ±4% of 1.0 everywhere, so the harness
  resolves differences of about 5%.

Found on the way: **the real `latest-all.json.gz` is a 22-byte member holding `[\n`, followed
by one 157 GB member.** The target case is single-member, so independent members can't come
for free.

## Results

### 1. Interleaving on real data (time vs `ours-k1`; below 1 = faster)

| file | k2 | k4 | A/A |
|---|---|---|---|
| wikidata m1M (first run, rolled loop) | 1.245 [1.188, 1.305] | 1.177 [1.128, 1.228] | 1.018 [0.949, 1.092] |
| wikidata m1M (unrolled) | 1.038 [0.996, 1.081] | 1.108 [1.043, 1.178] | 1.027 [0.980, 1.077] |
| wikidata BGZF | 1.023 [0.924, 1.133] | 1.084 [1.021, 1.151] | 0.966 [0.900, 1.036] |
| source m1M | 0.982 [0.914, 1.054] | 1.100 [1.006, 1.204] | 0.967 [0.933, 1.002] |
| source BGZF | 0.998 [0.955, 1.042] | 1.102 [1.046, 1.162] | 1.018 [0.956, 1.083] |
| binary m1M | 1.035 [0.980, 1.093] | 1.042 [0.989, 1.098] | 0.973 [0.900, 1.052] |
| binary BGZF | 1.073 [1.010, 1.140] | 1.129 [1.066, 1.194] | 1.023 [0.963, 1.088] |

The rows from wikidata BGZF down come from the grid rounds (`runs/grid-*`), with 9 paired rounds each.

K=6 and K=8 get steadily worse: 1.19 and 1.29 on Wikidata m1M. Batching (B=4 symbols per
stream per turn) is no better than B=1.

**The first run was invalid as a test, and why matters.** LLVM did not unroll the runtime
`for i in 0..N` loop, so each stream's bit buffer lived in a stack array indexed at run time.
That put a store-and-reload on every symbol, directly on the serial chain. `hot::<8,1>` was 624
instructions, about one copy of the step. Unrolling by macro (a 295-instruction body per
stream) took K=2 from 1.245 to 1.038. The rolled form is kept as the `-r` arm: K=4 rolled
costs 1.180. Lesson: **interleaving in Rust has to be unrolled by hand.** Check the
disassembly, not the source.

### 2. Symbol-dense controls: the mechanism exists

| Wikidata m1M re-encoded | k2 | k3 | k4 | k8 | libdeflate |
|---|---|---|---|---|---|
| `Z_HUFFMAN_ONLY` | 0.781 [0.734, 0.831] | 0.798 | **0.775** [0.716, 0.839] | 0.886 | 0.599 |
| `Z_RLE` (also ~all literals) | **0.737** [0.691, 0.785] | 0.778 | 0.784 | — | 0.624 |

With one predictable literal per byte, `ours-k1` runs at about 9 cycles per symbol at the
2.1 GHz nominal clock, the latency of refill → lookup → shift. Interleaving 2–4 streams brings
that to about 7, which is likely the instruction-throughput bound of this kernel, since each
literal carries bounds checks. Past K=4, register pressure wins and the gain fades.

**libdeflate on one stream (0.60×) still beats our best interleave.** Its literal path (several
literals per refill, entries pre-shifted for the `bitsleft -= entry` trick) shortens the chain
more than interleaving hides it.

### 3. Why real data doesn't benefit: the symbol census (`--features census`)

| | symbols/byte | literal share of symbols | bytes from matches | mean match len | dist <8 |
|---|---|---|---|---|---|
| Wikidata | **0.121** | 0.695 | 91.6% | 24.7 | 1.4% |
| source | 0.237 | 0.623 | 85.3% | 9.6 | 3.2% |
| binary | 0.249 | 0.642 | 84.0% | 9.4 | 3.0% |

At about 9 cycles per symbol, the chain costs Wikidata roughly 1.1 of its 3.0 cycles per byte
(36%), and source or binary roughly 2.1 of 5.3 (40%). The rest is match copying and its
data-dependent branches: the literal-vs-match branch, copy loops with variable trip counts,
and the short-distance paths. Within one stream, the out-of-order core already overlaps those
copies with the next symbol's decode, because a copy feeds nothing on the bit chain.

So the most interleaving could hide is the 36–40% that is chain latency. In practice it hides
none of it, because the extra streams compete for the same load/store and branch resources
and every mispredict flushes all of them. This is inference: no PMU here, so the
branch-mispredict and port-pressure split should be confirmed on the i9 with
`perf stat -e branch-misses,cycles,uops_dispatched.port_*`.

### 4. State of the art, single thread

In memory (raw DEFLATE, time vs libdeflate; GB/s for libdeflate):

| file | libdeflate GB/s | zlib-rs | zlib-ng | ours-k1 | miniz_oxide |
|---|---|---|---|---|---|
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

CLI, end to end (page-cache read, inflate, CRC, write to /dev/null), GB/s, 8 paired rounds
(`scripts/cli_ab.py`):

| file | igzip (ISA-L 2.31) | libdeflate-gunzip | rapidgzip -P1 | rapidgzip -P4 | pigz | gzip |
|---|---|---|---|---|---|---|
| wikidata -6 | **0.821** | 0.671 | 0.497 | 0.540 | 0.346 | 0.198 |
| source -6 | 0.539 | 0.469 | 0.387 | **0.590** | 0.209 | 0.137 |
| binary -6 | 0.469 | 0.416 | 0.353 | **0.488** | 0.186 | 0.128 |
| wikidata BGZF | 0.856 | **1.060** | 0.513 | 0.883 | 0.324 | 0.198 |
| wikidata m1M | 1.041 | **1.143** | 0.552 | 1.034 | 0.375 | 0.206 |

Two things in this table are effects of how the tools handle memory, not of their decoders:

- **libdeflate-gunzip runs at 0.67 GB/s on the single-member file but 1.14 on the same bytes
  in 1 MiB members.** In memory, with a reused buffer, it is 1.13 on the single member. The
  difference is consistent with page faults on first touch of its fresh whole-file output
  allocation (64 MiB is about 16k faults on a VM). This is mibz2's minflt lever again: owned,
  reused output buffers matter.
- **rapidgzip -P4 barely scales** here. 10 MB of compressed Wikidata is only a few of its
  chunks, the VM is shared, and P1 is about 1.4× slower than igzip. Its parallel numbers need a
  corpus of gigabytes, not 64 MiB.

### 5. Across cores (in memory, 1 MiB members dealt round-robin to threads)

| | t1 | t2 | t3 | t4 |
|---|---|---|---|---|
| wikidata GB/s | 1.158 | 2.103 | **3.231** | 2.746 |
| binary GB/s | 0.593 | 1.123 | 1.502 | 1.633 |

Three threads scale nearly linearly. t4 on a shared 4-vCPU VM contends with everything else,
so it says nothing about the i9.

## What has legs, and what to do next

1. **Drop single-thread interleaving as a lever.** Not worth carrying on real data. Revisit
   only for a kernel or data that is symbol-dense, such as huffman-only or literal-heavy
   blocks, and even then libdeflate's literal tricks get more.
2. **The single-thread kernel should match libdeflate.** Its techniques: decoding several
   literals per refill, `bitsleft -= entry` with pre-shifted entries, preloading the next
   entry before the current copy finishes, and 16- or 32-byte copies with an overrun budget.
   Its time is the per-worker cost of everything parallel, so it is the denominator of the
   whole speedrun. The open single-thread question is igzip against libdeflate in memory on
   the i9: igzip wins the single-member CLI runs here.
3. **The parallelism that pays is across cores, and the target is single-member** (the
   Wikidata dump is one 157 GB member). So the next experiment is the speculative chunk
   decode the brief made conditional, now conditioned on cores rather than ILP:
   - the cost of finding block starts on real data;
   - how fast unresolved-window markers thin out along a chunk (if they vanish within
     hundreds of KiB, the 16-bit marker pass is short and the rest runs at full speed);
   - the cost of the marker-resolution pass.

   Measure it against rapidgzip on a multi-GB slice of the dump, not 64 MiB.
4. **Memory traffic is visible already:** first-touch faults on large output buffers cost
   libdeflate-gunzip about 40% of its CLI throughput here. Reuse owned buffers, size the ring
   of ready bytes, and fuse CRC into the copy or resolve pass.
5. **On the i9, with counters:** re-run `ours-k1` / `ours-k2` / `ours-k4` with
   `branch-misses`, `cycles` and port-utilisation counters. That would confirm or overturn the
   mispredict and port-pressure explanation, which is inferred here.

## Reproduce

```
cargo build --release
python3 -I scripts/members.py corpus/{wikidata.json,source.tar,binary.tar}   # m1M + BGZF + .idx
python3 -I scripts/members_strategy.py corpus/wikidata.json huff corpus/wikidata.json.huff.gz
taskset -c 2 target/release/inflate-ilp corpus/wikidata.json.m1M.gz corpus/wikidata.json 12 \
    ours-k1,ours-k1-aa,ours-k2,ours-k4,ours-k4-r,libdeflate,zlib-ng > runs/x.txt
python3 scripts/ratio.py runs/x.txt ours-k1
cargo build --release --features census --target-dir target/census   # the symbol census
```

Raw rounds are in `runs/`. Corpus provenance: Wikidata is the first 64 MiB (whole lines) of
`latest-all.json.gz` as served on 2026-11-22. Source and binary are tars of this container's
Python stdlib, `/usr/include`, `/usr/bin` and `/usr/lib/x86_64-linux-gnu`, each cut at 64 MiB.
