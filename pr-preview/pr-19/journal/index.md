# Journals

Each entry is a dated record of one question: what was asked, the rule fixed before the data, the setup, the results and what comes next. The sources are `docs/journal/` in the repository, and the raw rounds are in `runs/`. Every figure is from the research host, a shared 4-vCPU cloud VM, and is indicative only.

- [Does mibz2's interleaving idea work for gzip?](<https://lmmx.github.io/upigz/pr-preview/pr-19/journal/interleaved-inflate/index.md>): no, not within a thread; the parallelism that pays is across cores.
- [The single-thread kernel and the first parallel win](<https://lmmx.github.io/upigz/pr-preview/pr-19/journal/kernel-and-parallel/index.md>): the kernel within 4–12% of libdeflate, and a streaming parallel decoder ahead of rapidgzip on 4 cores.
- [Speculative decode: the theorem, checked before any proof](<https://lmmx.github.io/upigz/pr-preview/pr-19/journal/speculative-theorem/index.md>): the statement, a counterexample hunt that found none, and a plan for a Lean proof.
