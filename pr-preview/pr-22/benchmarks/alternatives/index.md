# Alternatives

Loading the chart…

upigz beside other gzip decoders on master's latest recorded commit, decoded in the same rounds on one GitHub runner ([History](<https://lmmx.github.io/upigz/pr-preview/pr-22/benchmarks/history/index.md>) has the method). A ratio above 1× is the alternative taking longer (or more memory) than upigz at the same thread count.

| decoder | what runs | threads |
| --- | --- | --- |
| rapidgzip | 0.16.2, from PyPI: `rapidgzip -P T --verify -dc` | chunk-parallel |
| igzip | ISA-L 2.31's command-line tool: `igzip -dc` | single-threaded |
| libdeflate | `libdeflate-gunzip -c` (Ubuntu's libdeflate-tools) | single-threaded |
| pigz | `pigz -dc` (its decode is one thread, with reading, writing and the check in others) | single-threaded |
| gzip | GNU gzip: `gzip -dc` | single-threaded |

**Single-threaded decoders run at 1 thread only.** At 2 and 4 threads only upigz and rapidgzip run.

**These are a runner's figures, not a ranking.** The research host's and the i9's figures are on the [Benchmarks](<https://lmmx.github.io/upigz/pr-preview/pr-22/benchmarks/index.md>) page; a ranking is decided on the i9-10900K.

The latest record: [`2913d1e`](<https://github.com/lmmx/upigz/commit/2913d1e31452ff4471a8ce8ff3c0fb7b14e4d847>) (2026-11-22).

**bench 1.0.0**, 10 rounds, on AMD EPYC 7763 64-Core Processor (4 cores) · [run](<https://github.com/lmmx/upigz/actions/runs/45061665505>). Each arm decodes the whole file end to end (CRC checked), output to /dev/null; ratios are each arm's time over upigz's at the same thread count, paired by round, with a 95% interval (below 1 is faster than upigz).

Versions: rustc 1.99.1, rapidgzip 0.16.2, igzip 2.31.1, libdeflate 1.23, pigz 2.8, gzip 1.14.

## wikidata

wd-256.json.gz: 40,095,641 bytes compressed, 268,385,295 decoded. wikidata-20261116-all.json.gz: the first 256 MiB, whole lines, gzip -6 -n (bench/ci\_corpus.py)

| threads | arm | MB/s | wall s | CPU s | peak RSS MB | wall vs upigz |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | upigz | 514 | 0.522 | 0.522 | 553.3 | 1 (reference) |
| 1 | rapidgzip | 835 | 0.321 | 0.321 | 81.3 | 0.613 \[0.606, 0.621\] |
| 1 | igzip | 1058 | 0.254 | 0.253 | 24.6 | 0.485 \[0.479, 0.491\] |
| 1 | libdeflate | 824 | 0.326 | 0.326 | 295.7 | 0.622 \[0.615, 0.629\] |
| 1 | pigz | 524 | 0.512 | 0.706 | 24.6 | 0.974 \[0.956, 0.992\] |
| 1 | gzip | 577 | 0.465 | 0.465 | 24.6 | 0.888 \[0.878, 0.898\] |
| 2 | upigz | 771 | 0.348 | 0.648 | 196.0 | 1 (reference) |
| 2 | rapidgzip | 612 | 0.438 | 0.834 | 191.0 | 1.291 \[1.232, 1.353\] |
| 4 | upigz | 882 | 0.304 | 1.033 | 261.0 | 1 (reference) |
| 4 | rapidgzip | 699 | 0.384 | 1.248 | 342.5 | 1.254 \[1.237, 1.271\] |
