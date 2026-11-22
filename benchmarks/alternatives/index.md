# Alternatives

Loading the chart…

upigz beside other gzip decoders on master's latest recorded commit, decoded in the same rounds on one GitHub runner ([History](<https://lmmx.github.io/upigz/benchmarks/history/index.md>) has the method). A ratio above 1× is the alternative taking longer (or more memory) than upigz at the same thread count.

| decoder | what runs | threads |
| --- | --- | --- |
| rapidgzip | 0.16.2, from PyPI: `rapidgzip -P T --verify -dc` | chunk-parallel |
| igzip | ISA-L 2.31's command-line tool: `igzip -dc` | single-threaded |
| libdeflate | `libdeflate-gunzip -c` (Ubuntu's libdeflate-tools) | single-threaded |
| pigz | `pigz -dc` (its decode is one thread, with reading, writing and the check in others) | single-threaded |
| gzip | GNU gzip: `gzip -dc` | single-threaded |

**Single-threaded decoders run at 1 thread only.** At 2 and 4 threads only upigz and rapidgzip run.

**These are a runner's figures, not a ranking.** The research host's and the i9's figures are on the [Benchmarks](<https://lmmx.github.io/upigz/benchmarks/index.md>) page; a ranking is decided on the i9-10900K.

The latest record: [`7e092a6`](<https://github.com/lmmx/upigz/commit/7e092a6ea42377f0f127aa970f9f35c1b10dabfc>) (2026-11-22).

**bench 1.0.0**, 10 rounds, on AMD EPYC 7763 64-Core Processor (4 cores) · [run](<https://github.com/lmmx/upigz/actions/runs/45063025782>). Each arm decodes the whole file end to end (CRC checked), output to /dev/null; ratios are each arm's time over upigz's at the same thread count, paired by round, with a 95% interval (below 1 is faster than upigz).

Versions: rustc 1.99.1, rapidgzip 0.16.2, igzip 2.31.1, libdeflate 1.23, pigz 2.8, gzip 1.14.

## wikidata

wd-256.json.gz: 40,095,641 bytes compressed, 268,385,295 decoded. wikidata-20261116-all.json.gz: the first 256 MiB, whole lines, gzip -6 -n (bench/ci\_corpus.py)

| threads | arm | MB/s | wall s | CPU s | peak RSS MB | wall vs upigz |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | upigz | 515 | 0.521 | 0.521 | 553.3 | 1 (reference) |
| 1 | rapidgzip | 833 | 0.322 | 0.322 | 81.3 | 0.621 \[0.615, 0.627\] |
| 1 | igzip | 1052 | 0.255 | 0.255 | 24.7 | 0.491 \[0.488, 0.493\] |
| 1 | libdeflate | 814 | 0.330 | 0.330 | 295.7 | 0.634 \[0.630, 0.637\] |
| 1 | pigz | 522 | 0.514 | 0.708 | 24.7 | 0.992 \[0.977, 1.007\] |
| 1 | gzip | 576 | 0.466 | 0.466 | 24.7 | 0.897 \[0.893, 0.902\] |
| 2 | upigz | 778 | 0.345 | 0.644 | 189.4 | 1 (reference) |
| 2 | rapidgzip | 614 | 0.437 | 0.834 | 178.4 | 1.270 \[1.256, 1.283\] |
| 4 | upigz | 879 | 0.305 | 1.036 | 261.4 | 1 (reference) |
| 4 | rapidgzip | 702 | 0.382 | 1.249 | 334.2 | 1.235 \[1.173, 1.300\] |
