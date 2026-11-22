# upigz's Lean proofs

`Upigz/Speculative.lean` proves the LZ77 core of upigz's speculative decoding. A chunk decoded
from a block boundary over window markers, with the true window substituted afterwards, gives
exactly what decoding from that window gives. It is stated against lean-zip's own
`Deflate.Spec.resolveLZ77`. The plan and its remaining obligations are in
`docs/journal/2026-11-22-speculative-theorem.md` and `docs/journal/2026-11-22-lean-speculative.md`.

`Zip/Spec/LZ77.lean` is vendored unchanged from
[kim-em/lean-zip](https://github.com/kim-em/lean-zip) at `2f7a63f38195bc667a926881b55d10c9f8a88eeb`,
under the Apache License 2.0 (`Zip/LICENSE-lean-zip`). It has no imports, so it builds on core
Lean alone, with no need to build all of lean-zip.

Build, with Lean `v4.35.0-rc2` (`lean-toolchain`, the version lean-zip pins):

    cd lean && lake build

The axioms check: `#print axioms Upigz.Speculative.speculative_resolve` lists `propext` and
`Quot.sound` only; there is no `sorry`.
