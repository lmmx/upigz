# The research charter

*The contract for upigz's research lines: what they may claim, where they work, and how their
results reach the decoder. The open questions are in [FRONTIER.md](FRONTIER.md).*

## The contract

1. **The research host is indicative; the i9 decides.** A same-binary A/B on the research host
   (the shared 4-vCPU cloud VM of the journals, with no hardware PMU) is evidence for choosing
   what to try next, never for a default, a merge decision or a published ranking. Those are
   decided on the i9-10900K through a **registered recipe** (RULES C5, C6): a `just` recipe
   frozen with its decision rule before its data, queued (PROTOCOL.md §9) and submitted as
   evidence (`just submit <recipe>`).
2. **Same-binary A/B, paired.** Every comparison runs both arms from one harness binary, in
   interleaved rounds in shuffled order, the first round dropped. A ratio is the geometric
   mean of per-round time ratios with a 95% t interval on the log scale (`scripts/ratio.py`),
   and an A/A duplicate arm shows what the harness can resolve. Every arm is checked
   byte-exact against the reference before it is timed.
3. **Decision rules come before data.** A rule is posted (RULES C1a), or frozen in a
   registration for an adoption question (C6), before the run that decides it. It never moves
   after the data is seen (C2). A rule stated in a brief, as in the first journal entry, is
   recorded as such.
4. **Labels.** Every claim is labelled: *proved* (a checked proof), *verified* (checked
   exhaustively on a bounded domain or on real streams, as `spectest` does), *measured* (a
   native timing or count, with its host), *inferred* (from controlled variants or generated
   code, without counters), *refuted* or *open*.

## Where the work happens

- **Each line works on its own `research/<line>` branch.** Its code, scratch harnesses and raw
  runs stay there. Nothing on a research branch is reviewed, priced or registered while it is
  being explored (OWNERSHIP.md §4a), and nothing there ships.
- **A line hands off a port spec,** not a merge. The spec says, for the change it proposes to
  `src/`:
  - what it changes, and the invariant it relies on (with its label);
  - the research-host evidence, with its runs and the commit that made them;
  - the falsifier: the registered recipe and decision rule that would decide it on the i9;
  - what it costs to build, and what it leaves open.

  The Coordinator, the Pathfinder or the Optimiser builds the port as an ordinary PR, under
  PERFORMANCE-POLICY.md and RULES B5. The line's author doesn't review that PR.
- **The record.** A line's findings go into a dated journal entry under `docs/journal/` when
  they settle a question, and into [FRONTIER.md](FRONTIER.md)'s row for it. Raw runs go in
  `runs/` (or the line's branch), with the commands that made them.

## Method

The cycle (conjecture, derivation, counterexample or prototype, advance or abandon) and the
lessons behind it are mibz2's: its
[`docs/team/research-charter/METHOD.md`](https://github.com/lmmx/mibz2/blob/master/docs/team/research-charter/METHOD.md).
Its mathematics is the inverse BWT's, and doesn't carry over; its discipline does. In short:
- start from the obligation (the cost a decoder pays), not from a favoured algorithm;
- prefer the cheapest falsifier: a census, a bound, a counting probe, before a prototype;
- a failed prototype closes its construction, not the idea: say which failed;
- check the generated code, not the source (the first journal entry's rolled loop is the
  example).
