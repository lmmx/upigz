---
description: "upigz beside other gzip decoders on master's latest CI benchmark run: rapidgzip, igzip, libdeflate, pigz and gzip, with paired ratios and 95% intervals."
hide:
  - navigation
  - toc
---

# Alternatives

<div id="alternatives-hero" data-src="../../assets/alternatives.json" markdown>
Loading the chart…
</div>

upigz beside other gzip decoders on master's latest recorded commit, decoded in the same
rounds on one GitHub runner ([History](history.md) has the method). A ratio above 1× is the
alternative taking longer (or more memory) than upigz at the same thread count.

| decoder | what runs | threads |
|---|---|---|
| rapidgzip | 0.16.2, from PyPI: `rapidgzip -P T --verify -dc` | chunk-parallel |
| igzip | ISA-L 2.31's command-line tool: `igzip -dc` | single-threaded |
| libdeflate | `libdeflate-gunzip -c` (Ubuntu's libdeflate-tools) | single-threaded |
| pigz | `pigz -dc` (its decode is one thread, with reading, writing and the check in others) | single-threaded |
| gzip | GNU gzip: `gzip -dc` | single-threaded |

**Single-threaded decoders run at 1 thread only.** At 2 and 4 threads only upigz and
rapidgzip run.

**These are a runner's figures, not a ranking.** The research host's and the i9's figures
are on the [Benchmarks](index.md) page; a ranking is decided on the i9-10900K.

--8<-- "docs/generated/alternatives.md"
