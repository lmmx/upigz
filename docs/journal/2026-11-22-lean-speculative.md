# The speculative-decode theorem in Lean: the LZ77 core, proved (2026-11-22)

This carries forward [the plan](2026-11-22-speculative-theorem.md) (FRONTIER F6). It is a proof
about Lean definitions, not about the Rust code.

## What is proved (`lean/Upigz/Speculative.lean`, sorry-free)

The file builds against lean-zip's own LZ77 algebra: `Zip/Spec/LZ77.lean`, vendored unchanged
at `2f7a63f` (Apache-2.0). It has no imports, so lean-zip's other 70k lines are not built.

| theorem | statement |
|---|---|
| `resolve_byte` | `resolve`, lean-zip's `resolveLZ77` rewritten over any alphabet `α`, is `resolveLZ77` at `α = UInt8` |
| `resolve_map` | **resolution is natural in the alphabet:** `(resolve syms acc).map (map f) = resolve (syms.map (Sym.map f)) (acc.map f)`, for every `f` |
| `speculative_resolve_n` | for a window of any size `n`: decode over `markers ⊕ bytes` from the `n` window markers, then substitute the true window `W` (`W.length = n`), and the result equals `resolveLZ77 syms W` |
| `speculative_resolve` | the same at `n = 32768` |

`#print axioms` lists `propext` and `Quot.sound` for all of them: no `sorry`, and no axiom
beyond Lean's standard ones.

## Why this is the key step

Every copy in LZ77 resolution reads a *position*. A reference with `0 < dist ≤ acc.length` reads
`acc.length − dist + i % dist`, which is in bounds, so the `Inhabited` default of `acc[k]!` is
never used. Resolution therefore can't tell which alphabet it is running over, and that is
`resolve_map`.

Speculative decoding is that naturality applied to one particular `f`: substituting the window.
Two kinds of input are covered:
- **Copies that land partly in the window and partly in decoded output.**
- **Overlapping copies** (distance smaller than length) that run through markers.

Neither needs a case analysis of its own. A decode that fails, a reference before the start,
fails on both sides alike, since the equation holds between `Option`s.

## Effort

The plan estimated 500–1,500 lines of Lean for the whole of obligation (a). This core took
about 180 lines, comments included, and a handful of compile iterations. The main obstacle was Lean trying to reduce the
literal 32,768. Proving everything for a window of arbitrary size `n`, and instantiating at
32,768 only in the final statement, removed it.

## What's left, from the plan

1. **(a)1 and (a)2, the Huffman and block layer:** speculative decode reads the same symbols and
   bits as sequential decode, and stops at the same block. That holds because lean-zip's
   `decodeSymbols` and `decodeDynamicTables` never read the history, but stating it needs
   lean-zip's `decode.go` split at a block boundary. That requires building the rest of its
   spec: `Deflate.lean`, `Huffman.lean` and their imports, not just this file.
2. **Full history against its last 32 KiB:** sequential decode resolves against all earlier
   output. This needs a decoder-side `dist ≤ 32768` bound, and the lemma
   `resolveLZ77 syms (P ++ W) = (P ++ ·) <$> resolveLZ77 syms W` when every distance stays
   within reach. It's small with the techniques above.
3. **(a)5, window chaining, and (b), the chaining lemma** for chunk boundaries.
4. **The Rust code** stays tied to these definitions by differential testing (`spectest`: 16,046
   block starts, no counterexample) and byte-exact checks, not by proof.
