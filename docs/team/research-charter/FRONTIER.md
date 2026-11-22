# The research frontier

*A living record. A research line reads it at the start of a cycle and updates its row at the
end. It holds the open questions, each with what is known, the cheapest next measurement and
what would settle it. Evidence lives in the journals (`docs/journal/`) and `runs/`; this file
points to it. The contract is the [charter](README.md): research-host figures are indicative,
and the i9 decides through registered recipes.*

Every figure below is from the research host (a shared 4-vCPU cloud VM, no hardware PMU) and
is quoted from the journal named in its row.

## The frontier

| # | question | what is known | next | settles it | status |
|---|---|---|---|---|---|
| F1 | **The re-decode bet:** instead of resolving markers, run pass 1 into a cache-resident 16-bit ring that keeps only the chunk's final window, then decode each chunk again, to bytes, from its now-known window | Two decodes of compute, but about 1.6 B/B of DRAM traffic against roughly 5 B/B for the current marker-resolve design (estimate; [kernel and parallel](../../journal/2026-11-22-kernel-and-parallel.md), "Open" 1). Its pass 2 is ordinary inflate from a block boundary with a known history, which needs less new proof than marker resolution ([theorem](../../journal/2026-11-22-speculative-theorem.md), "Engineering value" 2) | F2 first | A thread sweep on the i9, where the marker-resolve design should go memory-bound; the verdict rule registered before the run | open: the candidate key idea |
| F2 | **Pass 1's cost alone**, against a byte decode of the same chunk | Not measured in isolation. On Wikidata markers never die (the last marker lands at the end of the 16 MiB span in every sample; 7–48% of output symbols are markers), so a 16-bit pass over the whole chunk is unavoidable there ([kernel and parallel](../../journal/2026-11-22-kernel-and-parallel.md)) | Time pass 1 into a ring that keeps only the final window, same-binary A/B against the byte kernel on the same chunks | Its ratio to a byte decode prices F1 before anything is built | open: measure first |
| F3 | **The 16-bit pass-1 kernel is slow** | One-thread `par` is 0.47 GB/s, against 1.03 for the byte kernel (same journal, "Open" 2). It is the same generic kernel as the byte path (`Sym` trait) | Find where the `u16` path loses: generated code, copy widths, the marker path | A pass-1 kernel within a stated factor of the byte kernel on the research host, then confirmed on the i9 | open |
| F4 | **Parallel robustness** the prototype doesn't have | Finder false positives error out rather than recover; chunks shorter than a window; multi-member files and the dump's leading member (22 bytes, holding `[\n`, before one 157 GB member); corrupt-input error parity with zlib; no fuzzing yet (same journal, "Open" 3). The chaining lemma says a false positive can only cost time, never correctness, if the pipeline falls back by merging chunks ([theorem](../../journal/2026-11-22-speculative-theorem.md), (b)) | Recover from a rejected candidate by merging into the previous chunk; a fuzz target over `spectest`'s generator | `spectest` and a fuzz campaign find no counterexample; corrupt inputs give zlib's error class | open: needed before any release |
| F5 | **Single-thread against libdeflate** | upigz's kernel is second single-thread in memory: ahead of ISA-L, zlib-rs and zlib-ng, behind libdeflate by 4–12% (unchecked build) or about 20% (safe build) ([kernel and parallel](../../journal/2026-11-22-kernel-and-parallel.md)). Left: the remaining table-build cost; multi-literal entries (ISA-L-style packed literals); the safe-build tax without `unsafe`, by restructuring so LLVM can prove the bounds ("Open" 4). Also open: igzip against libdeflate in memory on the i9 ([interleaved inflate](../../journal/2026-11-22-interleaved-inflate.md), "What has legs" 2) | Each lever as a same-binary A/B against libdeflate | A registered single-thread recipe on the i9 | open |
| F6 | **The Lean speculative-decode theorem**: composition and resolution from a true block boundary, plus the chaining lemma, formalised against lean-zip | The statement holds under testing: `spectest 6000` checked 16,046 block starts (7,556 with history shorter than 32 KiB) on both decoders with no counterexample. Estimated at about 500–1,500 lines of Lean, reusing lean-zip's spec and its equivalence to the native decoder (`kim-em/lean-zip` @ 2f7a63f3) ([theorem](../../journal/2026-11-22-speculative-theorem.md)) | Start it as a separate, bounded line on its own `research/` branch, scoped to (a) plus the chaining lemma | The proof checks | open: not on the performance critical path, and never holds benchmark work |

## Closed

| question | verdict | where |
|---|---|---|
| Does mibz2's interleaving (K independent members round-robin in one thread) speed up gzip decoding? | **No, not within a thread** on real data: K=2 takes 0.98–1.09× and K=4 1.04–1.16× the sequential time. The mechanism exists on symbol-dense streams (about 1.28× faster), but real gzip data is match-dominated. Reopen only for symbol-dense data or kernels | [interleaved inflate](../../journal/2026-11-22-interleaved-inflate.md) |

## Pending leads

An inbox for directions Louis introduces between cycles. A lead is an invitation, not an
assignment: a line reads it at its next cycle boundary and decides how it shapes the next cycle.

| lead | why it might matter | references | status |
|---|---|---|---|

## Cycle log

| date | line | what changed | next |
|---|---|---|---|
| 2026-11-22 | (seeding) | the frontier recorded from the three journal entries of 2026-11-22 | F2, then F1's registered rule |
