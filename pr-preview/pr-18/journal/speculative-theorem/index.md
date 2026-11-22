# Speculative decode: the theorem, checked before any proof (2026-11-22)

Sources:

- the lean-zip inventory (`kim-em/lean-zip` @ 2f7a63f3, read only);
- `spectest` in `src/main.rs`, a programmatic counterexample hunt.

## Setting

Let `S` be a valid raw DEFLATE stream (as bits). Sequential inflate produces `out`, and visits blocks `B_0 … B_m`; block `B_j` starts at bit `p_j` with output offset `o_j`. The decoder reads blocks one after another, so `p_j` are exactly the bit positions at which the sequential decoder reads a block header. A position that merely parses as a header is not one of them.

The symbolic window is `M = (μ_0 … μ_32767)`, where `μ_w` stands for the byte `32768 − w` before the chunk start. `D(p, M)` is speculative decode from bit `p` over the alphabet `bytes ∪ M`: copies move symbols, so markers propagate through overlapping copies too. `resolve(Y, W)` replaces each `μ_w` in `Y` with `W[w]`.

## (a) Composition and resolution, given a valid boundary

For every block start `p_j`, and every later block start `p_q` (or the end of the stream):

1. **Same path.** `D(p_j, M)` decodes successfully, and reads exactly the same block headers and the same bits as the sequential decoder, stopping at `p_q`. This holds because symbol decoding (headers, Huffman tables, codes) never reads the history. In lean-zip only `resolveLZ77` touches `acc`; `decodeSymbols` and `decodeDynamicTables` never do.
2. **Length.** `|D(p_j, M)| = o_q − o_j`.
3. **Resolution.** `resolve(D(p_j, M), W_j) = out[o_j .. o_q)`, where `W_j = out[o_j − 32768 .. o_j)`.
4. **Absent history is never referenced.** If `o_j < 32768`, no `μ_w` with `w < 32768 − o_j` occurs in `D(p_j, M)`. That follows from `S` being valid: the sequential decoder would have rejected such a distance. For an *invalid* `S` this is exactly where speculative and sequential decoding can disagree, so the statement needs `S` valid, or a side condition.
5. **Window chaining.** `W_q = resolve(last 32 KiB of D(p_j, M), W_j)`, when `o_q − o_j ≥ 32768`. This is the serial hand-off: 32 KiB of work per chunk.

## (b) Finding boundaries

Here a chunk-start candidate `s` comes from a heuristic finder.

**Chaining lemma.** Let `p` be a true block start. Suppose `D(p, ·)` reaches a block start at exactly bit `s` (the first block start `≥ s` *equals* `s`). Then `s` is a true block start. The reason is that decoding is deterministic and (a)1 says `D` follows the sequential path.

**Corollary (pipeline soundness).** Chunk 0 starts at the stream start, which is a true boundary. By induction over chunks, every candidate accepted by the equality check is true. A false positive is never used; it can only cost time, by falling back to merging chunks. **The finder therefore needs no proof for correctness, only for performance.** Its heuristics (precode completeness, code completeness) bound wasted work, not the result.

## Evidence before proof

`spectest 6000` builds random streams with miniz\_oxide at levels 0–10, so stored, fixed and dynamic blocks all occur. The data is random, text-like, run-heavy and overlap-heavy (periodic patterns with period 1–12, repeats up to 40 KiB back).

At **every** block start (16,046 of them, 7,556 with history shorter than 32 KiB) it checks (a)1–4 on both the reference-style `spec.rs` decoder and the production `u16` kernel in `fast.rs`. It found **no counterexample**. Earlier, the `markers` mode had checked (a)3 symbol by symbol on 48 real-corpus chunks of 16 MiB.

## The proof opportunity in lean-zip (estimate from the inventory: medium, about 500–1,500 lines)

What it reuses:

- **The spec, and two-way equivalence to the native decoder.** `inflateLoop_correct` / `inflateLoop_complete` hold from any bit position with any prior output, and `inflate_ok_iff_decodesTo` covers arbitrary streams.
- **Lemmas about history and suffixes:** `resolveLZ77_shift`, `resolveLZ77_append_eobFree`, `decode_go_suffix` and `decode_go_acc_prefix`.

What has to be new:

- a version of `resolveLZ77` and `decode.go` that works over any symbol alphabet;
- a lemma that resolution commutes with substituting the true window;
- a decoder-side `dist ≤ 32768` bound;
- a both-directions lemma that the full history is equivalent to its last 32 KiB;
- a decoder that stops at a given block;
- (a)5 and the chaining lemma.

Obstacles:

- everything is hard-wired to `UInt8`, so the generic alphabet should be a copy, not a refactor;
- `alignToByte` works from the number of bits remaining, so a chunk must be a suffix of the stream's bits plus a stop point;
- the `maxOutputSize` bookkeeping counts the earlier history.

## Engineering value

1. **Safety checks can be narrower.** With the chaining lemma, correctness never depends on the finder. rapidgzip-style validation (decoding far to confirm a candidate) is purely a performance choice.
2. **The re-decode design needs less new proof than marker resolution.** Its pass 2 is ordinary inflate from a block boundary with a known history. lean-zip already has that (`inflateLoop_correct`), given the 32 KiB-suffix lemma. Pass 1 needs (a) only for the final window.
3. **What it can't do:** prove that the Rust code is correct. That link stays differential testing (`spectest`, the byte-exact harness) and fuzzing. And it says nothing about speed.

## Decision

The statement holds under testing and is precise enough to formalise. It isn't on the performance critical path, so I'd **start it as a separate, bounded line** of about 500–1,500 lines of Lean, scoped to (a) plus the chaining lemma. It should run on its own `research/` branch once the repo exists, and shouldn't hold up benchmark work.
