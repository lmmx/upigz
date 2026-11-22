import Lake
open Lake DSL

package upigzSpec

-- lean-zip's LZ77 symbol algebra, vendored at kim-em/lean-zip 2f7a63f (Apache-2.0):
-- Zip/Spec/LZ77.lean has no imports, so it builds on core Lean alone
lean_lib Zip where
  roots := #[`Zip.Spec.LZ77]

lean_lib Upigz where
  roots := #[`Upigz.Speculative]
