import Zip.Spec.LZ77

/-!
# Speculative decoding: resolution with an unknown window, then substitution

upigz decodes a chunk of one DEFLATE member from a block boundary before the 32 KiB of output
that precedes it is known. Back-references into that unknown history copy *markers*, one per
window position, and the markers propagate through later copies. Once the true window is known,
substituting it for the markers gives the bytes.

This file proves the LZ77 core of that claim (obligation (a)3 in upigz's
`docs/journal/2026-11-22-speculative-theorem.md`) against lean-zip's own `resolveLZ77`:

* `resolve` is lean-zip's `Deflate.Spec.resolveLZ77`, line for line, over any alphabet `α`.
* `resolve_byte` shows that, at `α = UInt8`, it *is* `resolveLZ77`.
* `resolve_map` is the key fact: resolution is natural in the alphabet. Mapping the output
  through any `f` equals resolving the mapped symbols from the mapped history. Copying moves
  positions, not values, so it can't tell the alphabets apart.
* `speculative_resolve` is the corollary upigz relies on. Decoding over `markers ⊕ bytes` from a
  history of window markers, then substituting the true window `W`, equals decoding the bytes
  from `W` itself.

What this does not cover is listed at the end of the file.
-/

namespace Upigz.Speculative

open Deflate.Spec

/-- LZ77 symbols over an arbitrary alphabet (lean-zip's `LZ77Symbol` has `α = UInt8`). -/
inductive Sym (α : Type) where
  | literal (a : α)
  | reference (length distance : Nat)
  | endOfBlock

/-- Map the literals of a symbol; references and end of block are untouched. -/
def Sym.map {α β : Type} (f : α → β) : Sym α → Sym β
  | .literal a => .literal (f a)
  | .reference l d => .reference l d
  | .endOfBlock => .endOfBlock

/-- lean-zip's `resolveLZ77`, line for line, over any inhabited alphabet. -/
def resolve {α : Type} [Inhabited α] : List (Sym α) → List α → Option (List α)
  | [], acc => some acc
  | .literal b :: rest, acc => resolve rest (acc ++ [b])
  | .endOfBlock :: _, acc => some acc
  | .reference len dist :: rest, acc =>
    if dist == 0 || dist > acc.length then none
    else
      let start := acc.length - dist
      let copied := List.ofFn fun (i : Fin len) =>
        acc[start + (i.val % dist)]!
      resolve rest (acc ++ copied)

/-- lean-zip's symbols, read as `Sym UInt8`. -/
def ofLZ77 : LZ77Symbol → Sym UInt8
  | .literal b => .literal b
  | .reference l d => .reference l d
  | .endOfBlock => .endOfBlock

/-- At `α = UInt8`, `resolve` is lean-zip's `resolveLZ77`. -/
theorem resolve_byte : ∀ (syms : List LZ77Symbol) (acc : List UInt8),
    resolve (syms.map ofLZ77) acc = resolveLZ77 syms acc
  | [], acc => rfl
  | .literal b :: rest, acc => by
      simp only [List.map_cons, ofLZ77, resolve, resolveLZ77]
      exact resolve_byte rest _
  | .endOfBlock :: _, acc => rfl
  | .reference len dist :: rest, acc => by
      simp only [List.map_cons, ofLZ77, resolve, resolveLZ77]
      split
      · rfl
      · exact resolve_byte rest _

/-- Indexing a mapped list within bounds maps the element. -/
theorem getElem!_map {α β : Type} [Inhabited α] [Inhabited β] (f : α → β) (l : List α)
    (k : Nat) (h : k < l.length) : (l.map f)[k]! = f l[k]! := by
  have h' : k < (l.map f).length := by simpa using h
  rw [getElem!_pos (l.map f) k h', getElem!_pos l k h]
  simp

/-- **Resolution is natural in the alphabet.** For any `f`, mapping the resolved output
    through `f` equals resolving the mapped symbols from the mapped history. Every copy reads
    an in-bounds position (a reference with `0 < dist ≤ acc.length` reads
    `acc.length - dist + i % dist < acc.length`), so the `Inhabited` default is never used and
    `f` commutes with every read. -/
theorem resolve_map {α β : Type} [Inhabited α] [Inhabited β] (f : α → β) :
    ∀ (syms : List (Sym α)) (acc : List α),
      (resolve syms acc).map (List.map f) = resolve (syms.map (Sym.map f)) (acc.map f)
  | [], acc => rfl
  | .literal b :: rest, acc => by
      simp only [List.map_cons, Sym.map, resolve]
      rw [resolve_map f rest]
      simp
  | .endOfBlock :: _, acc => rfl
  | .reference len dist :: rest, acc => by
      simp only [List.map_cons, Sym.map, resolve, List.length_map]
      split
      · rfl
      · rename_i h
        rw [resolve_map f rest]
        congr 1
        rw [List.map_append, List.map_ofFn]
        congr 1
        apply congrArg List.ofFn
        funext i
        have hd : dist ≠ 0 ∧ dist ≤ acc.length := by
          simp only [Bool.or_eq_true, beq_iff_eq, decide_eq_true_eq, not_or, Nat.not_lt] at h
          exact h
        have hlt : acc.length - dist + i.val % dist < acc.length := by
          have := Nat.mod_lt i.val (Nat.pos_of_ne_zero hd.1)
          omega
        simp only [Function.comp]
        rw [getElem!_map f acc _ hlt]

/-- A speculative symbol for a window of `n` bytes: a marker for window position `w`
    (`0` is `n` bytes back), or a byte. -/
abbrev Spec (n : Nat) := Sum (Fin n) UInt8

instance (n : Nat) : Inhabited (Spec n) := ⟨.inr 0⟩

/-- The history a speculative decode starts from: one marker per window position. -/
def markers (n : Nat) : List (Spec n) := List.ofFn fun (w : Fin n) => .inl w

/-- Substitute the true window `W` for the markers; bytes pass through. -/
def subst {n : Nat} (W : List UInt8) : Spec n → UInt8
  | .inl w => W[w.val]!
  | .inr b => b

theorem markers_subst (n : Nat) (W : List UInt8) (hW : W.length = n) :
    (markers n).map (subst W) = W := by
  apply List.ext_getElem
  · simp [markers, hW]
  · intro k h1 h2
    simp only [markers, List.getElem_map, List.getElem_ofFn, subst]
    rw [getElem!_pos W k h2]

/-- **Speculative decoding is sound,** for any window size `n`. Decode a block's symbols over
    `markers ⊕ bytes`, starting from the window markers; substitute the true window `W` into the
    result. That equals decoding the same symbols over bytes, starting from `W`. This includes a
    failed decode: a reference before the window fails in both. Through `resolve_byte`, the
    right-hand side is lean-zip's `resolveLZ77`. -/
theorem speculative_resolve_n (n : Nat) (syms : List LZ77Symbol) (W : List UInt8)
    (hW : W.length = n) :
    (resolve ((syms.map ofLZ77).map (Sym.map Sum.inr)) (markers n)).map (List.map (subst W))
      = resolveLZ77 syms W := by
  rw [resolve_map (subst W), markers_subst n W hW, ← resolve_byte, List.map_map]
  have hid : (Sym.map (subst (n := n) W) ∘ Sym.map Sum.inr) = (id : Sym UInt8 → Sym UInt8) := by
    funext s; cases s <;> rfl
  rw [hid, List.map_id]

/-- The window size of DEFLATE: 32 KiB. -/
abbrev window : Nat := 32768

/-- **Speculative decoding is sound** at DEFLATE's 32 KiB window: what upigz's parallel decoder
    does for each chunk after the first, then its resolve pass. -/
theorem speculative_resolve (syms : List LZ77Symbol) (W : List UInt8) (hW : W.length = window) :
    (resolve ((syms.map ofLZ77).map (Sym.map Sum.inr)) (markers window)).map
        (List.map (subst W))
      = resolveLZ77 syms W :=
  speculative_resolve_n window syms W hW

/-!
## Not covered here (the next obligations)

* **The Huffman and block layer.** These theorems take the symbol list as given. That speculative
  decode reads the *same* symbols and bits as sequential decode, (a)1 in the journal, holds
  because lean-zip's `decodeSymbols` and `decodeDynamicTables` never read the history. Stating it
  needs lean-zip's `Deflate.Spec.decode.go` split at a block boundary.
* **Full history against its last 32 KiB.** Sequential decode resolves against all earlier
  output, not just its last 32 KiB. Bridging the two needs a decoder-side `dist ≤ 32768` bound
  and a lemma that `resolveLZ77` from `P ++ W` equals `P ++` the result from `W` when every
  distance stays within reach.
* **The chaining lemma** for chunk boundaries (obligation (b)).
* **The Rust code.** These are statements about Lean definitions. The Rust decoders are tied to
  them by differential testing (`spectest`) and byte-exact checks, not by proof.
-/

end Upigz.Speculative
