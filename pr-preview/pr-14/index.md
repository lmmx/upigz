# upigz

μpigz decodes gzip in parallel, including the case other parallel decoders find hardest: one large member, such as the Wikidata JSON dump's single 157 GB member. It splits the member into chunks, decodes each one before it knows the 32 KiB of history its matches may reach back into, and then fixes up what it couldn't know.

- **A finder** scans each chunk for a plausible dynamic-block header.
- **Pass 1** decodes each chunk into 16-bit symbols, where an unknown history byte is a marker.
- **The windows** are handed on in order, each chunk's last 32 KiB resolved through the previous chunk's.
- **Resolution and the CRC** run in parallel; the CRC-32 and ISIZE are checked against the trailer.

A chunk start is used only if the previous chunk's decode lands on it exactly, so a wrong guess by the finder can cost time but never correctness.

On a 4-core research host it decodes a 1.34 GB single-member Wikidata file at 1.21 GB/s end to end, against rapidgzip's 0.80 on the same cores: [the benchmarks](<https://lmmx.github.io/upigz/pr-preview/pr-14/benchmarks/index.md>) have the figures and their caveats. They are indicative only. A 10-core workstation decides the ranking.

upigz is a research prototype: see [getting started](<https://lmmx.github.io/upigz/pr-preview/pr-14/getting-started/index.md>) for building it and what it doesn't do yet.

## Where to go

- [Getting started](<https://lmmx.github.io/upigz/pr-preview/pr-14/getting-started/index.md>): building upigz and running its parallel `zcat`.
- [Benchmarks](<https://lmmx.github.io/upigz/pr-preview/pr-14/benchmarks/index.md>): the research host's figures, single thread and parallel.
- [Journals](<https://lmmx.github.io/upigz/pr-preview/pr-14/journal/index.md>): the research record, entry by entry.
- [Blog](<https://lmmx.github.io/upigz/pr-preview/pr-14/blog/index.md>): what the experiments find, as they find it.
- The source is on [GitHub](<https://github.com/lmmx/upigz>).
