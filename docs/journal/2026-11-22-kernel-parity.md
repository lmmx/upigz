# The single-thread kernel reaches libdeflate (unchecked build), 2026-11-22

*The research host after its restart: same VM type, faster than before in absolute terms, so
only same-binary ratios are comparable. Indicative; the i9 decides.*

The C1a pre-data post is
[#2 comment 7082019741](https://github.com/lmmx/upigz/pull/2#issuecomment-7082019741). The
rule: keep a candidate if its time against the current kernel (`fast-base`, the master copy of
`src/fast.rs`, in the same binary) has a 95% interval wholly below 1.00 on Wikidata or on two of
three corpora, and no corpus wholly above 1.02.

## Profile first: instruction counts, not guesses

The host has no PMU, so the profile is `valgrind --tool=cachegrind`: exact instruction counts
per function and per source line. It ran one decode of each 64 MiB corpus at `gzip -6`. Each
row compares upigz's hot loop plus its block headers and table builds with libdeflate's
`deflate_decompress` (header decoding inlined) plus its table builds:

| | upigz | libdeflate | excess |
|---|---|---|---|
| Wikidata | 289M + 28M | 278M + 17M | +23M (+8%) |
| source | 616M + 28M | 569M + 14M | +61M (+10%) |
| binary | 629M + 59M | 589M + 29M | +70M (+11%) |

The largest single line was `extra_fast`, the extraction of a length's or distance's extra
bits: 53.6M instructions, 7.0% of everything on binaries, about 4.5 instructions per call. It
emulated `bzhi`'s "index of 64 or more returns the value whole" case with a branch-free select,
which LLVM did not fold into one `bzhi`.

## The change

A non-exceptional entry consumes at most 28 bits (a 15-bit code plus 13 extra bits), so its
consume count fits in 5 bits. `saved & ((1 << (e & 31)) - 1)` has no out-of-range case, and LLVM
emits it as one `bzhi`. The hot loop now holds 8 of them. The entry constructors are also marked
`#[inline(always)]`, since `lit_entry` still showed up as a called function in the profile.

It is safe code in both builds. Both builds are byte-exact on all 12 corpus files (`-6`, `-9`,
1 MiB members, BGZF), and both are green on `spectest 300`.

## Result (11 rounds, round 0 dropped; `runs/k1-*.txt`)

Time vs `fast-base`, the previous kernel:

| build | Wikidata | source | binary |
|---|---|---|---|
| unchecked | **0.927** [0.908, 0.947] | **0.908** [0.877, 0.941] | **0.926** [0.873, 0.981] |
| safe | **0.969** [0.959, 0.979] | 0.985 [0.941, 1.032] | 1.031 [0.949, 1.121] |

Both pass the rule: the unchecked build on all three corpora, the safe build on Wikidata, with
no corpus wholly above 1.02.

Time vs libdeflate 1.26.1, from the same rounds:

| build | Wikidata | source | binary |
|---|---|---|---|
| unchecked, before | 1.079 [1.070, 1.088] | 1.098 [1.056, 1.141] | 1.113 [1.055, 1.174] |
| **unchecked, after** | **1.000** [0.986, 1.014] | **0.997** [0.961, 1.034] | **1.030** [0.984, 1.078] |
| safe, before | 1.138 [1.118, 1.158] | 1.180 [1.142, 1.218] | 1.143 [1.104, 1.183] |
| safe, after | 1.102 [1.086, 1.119] | 1.162 [1.112, 1.214] | 1.179 [1.096, 1.267] |

The unchecked kernel is now at parity with libdeflate on the research host: every interval
contains 1.00. It is level, not ahead. After the change, cachegrind puts the hot loop at 591M
instructions on binaries against libdeflate's 589M, and libdeflate's figure also includes its
header decoding.

## What's left, in size order

1. **The safe-build tax, 10–18%.** This is the default build, so it's the gap that matters to
   users. The remaining bounds checks are on the input refill, the literal stores and the copy
   chunks. The next attempt is to restructure the fast loop so that LLVM can prove those
   accesses in bounds: one window reborrow per iteration, and lengths capped at 258, which is a
   no-op for any valid stream.
2. **Block headers and table builds, about 3% of instructions.** Our builds run about 23K
   instructions a block on binaries, against roughly 13K for libdeflate.
3. **Passing libdeflate, not just matching it,** needs something libdeflate doesn't do. The
   candidate is multi-literal table entries (ISA-L packs short literal pairs). Any such claim
   waits for the i9.
