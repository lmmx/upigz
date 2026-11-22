[upigz · Research blog](<https://lmmx.github.io/upigz/blog/index.md>)

# One gzip member, four cores: decoding Wikidata 1.48× faster than rapidgzip

On a 4-core research VM, upigz decodes a 1.34 GB single-member Wikidata gzip in 0.68× rapidgzip's time, CRC checked, and its one-thread kernel is within 4–12% of libdeflate.

22 November 2026 · 6 min read · [Research in progress](<#what-isnt-known-yet>)

upigz is a gzip decoder in Rust, built for one hard case: a single huge gzip member, like the 157 GB Wikidata JSON dump. These are its first measurements, all taken on a shared 4-vCPU cloud VM rather than the 10-core workstation that will decide the real ranking.

- **Parallel:** decoding a 1.34 GB single-member Wikidata file end to end, CRC-32 and size checked, upigz on 4 threads takes **0.677× the time of rapidgzip on 4 threads** (95% CI \[0.616, 0.744\]): **1.21 GB/s against 0.80**. The build with no `unsafe` code takes 0.750× \[0.698, 0.807\].
- **One thread:** upigz's kernel takes **1.04–1.12× libdeflate's time** on Wikidata, source code and binaries. That's second place: ahead of ISA-L and zlib-rs, not yet level with libdeflate.
- **Not yet known:** how either result holds on the 10-core, 20-thread i9 workstation. No run there has been registered yet.

Below: [why one member is the hard case](<#why-one-member-is-the-hard-case>), [the parallel result](<#the-parallel-result>), [the one-thread kernel](<#the-one-thread-kernel>), [what didn't work](<#what-didnt-work>) and [what isn't known yet](<#what-isnt-known-yet>).

## Why one member is the hard case

A gzip file made of many small members, as BGZF files in bioinformatics are, is easy to decode in parallel: each member is independent. The Wikidata dump isn't like that. Its head is a 22-byte member holding `[` and a newline, followed by one member of 157 GB.

Inside one member, DEFLATE gives nothing for free. Blocks start at arbitrary bit positions with no marker, and a match may copy from anywhere in the previous 32 KiB of output, including output of earlier blocks. A worker that starts decoding in the middle doesn't know that history.

upigz does what rapidgzip does:

1. It finds a block start near each chunk boundary.
2. It decodes from there with 32,768 placeholders, *markers*, standing for the unknown history.
3. It hands each chunk's final 32 KiB to the next chunk in order, and replaces the markers with real bytes.

Each chunk must stop exactly where the next one starts. That check makes a false block match harmless: a candidate is used only if the previous chunk's real decode lands on it.

## The parallel result

End to end on the 1.34 GB single-member Wikidata file (`gzip -6`): read from the page cache, decode, check CRC-32 and size, write to /dev/null. Six paired rounds in shuffled order (`runs/race-big4.txt`). The time ratio is the geometric mean of per-round ratios, with a 95% interval. rapidgzip 0.16.2's default and `--verify` forms ran at the same speed (0.980 \[0.884, 1.085\]).

On four threads upigz runs at 1.48× rapidgzip's throughput. On three threads it already beats rapidgzip on four: 0.864× the time \[0.810, 0.921\]. Removing all `unsafe` code from the decoder costs a little: in a separate race the safe build took 0.750× rapidgzip's time \[0.698, 0.807\], against 0.722 for the unchecked build in the same race.

The decoding itself wasn't what made the difference. Two memory and scheduling fixes were:

- **Bounded memory.** The first version kept every chunk alive at once: about 2.7 GB of 16-bit symbols plus 1.34 GB of bytes, first touched in a fresh process. It took 1.59× rapidgzip's time. Capping the chunks in flight at the thread count plus two, recycling their buffers and streaming the output brought that to 0.849×.
- **A serial chain removed.** Each chunk's window came from the previous chunk's symbols, but a worker could take those symbols to resolve that chunk first, so every resolution waited for the one before. Deriving the next window before queuing a chunk brought the time to 0.677×.

## The one-thread kernel

One thread, in memory, decoding 64 MiB inputs at `gzip -6` into a reused buffer, no CRC. Eleven paired rounds per corpus (`runs/sota-*.txt`). The dashed line is libdeflate; whiskers are 95% intervals. libdeflate's own A/A control landed at 0.987–1.006.

upigz's kernel follows libdeflate's design: each table entry says how many bits to consume, the next entry is looked up before the current match is copied, and up to three literals decode on one refill of the bit buffer. Two steps closed most of the gap:

- **Falling from literals straight into a match**, without refilling the bit buffer, as libdeflate does.
- **Building each block's decode tables libdeflate's way**, filling the shortest codes first and doubling. Block headers and table builds had cost 9–15% of the whole decode, because the blocks are small (30–60 KB of output each).

It is now second: it takes 1.123× libdeflate's time on Wikidata, 1.093× on source code and 1.043× on binaries \[0.979, 1.112\]. ISA-L takes 1.14–1.17× and zlib-rs 1.37–1.41×. These figures are from the research build with unchecked memory access; with bounds checks the kernel is about 1.2× libdeflate's time.

## What didn't work

- **Interleaving within one thread.** mibz2, upigz's bzip2 sibling, gained a lot by walking several independent chains in one thread. For gzip, decoding 2–8 independent members round-robin was never faster than one after another on real data: 0.98–1.13× the time. The idea does work on streams of literals only (about 1.28× faster), but real gzip data is mostly matches: 0.12–0.25 symbols per output byte, with 84–92% of bytes coming from matches.
- **Waiting for the markers to die.** If the unknown history stopped being referenced after a few hundred KiB, a worker could switch to plain bytes and skip most of the 16-bit pass. On source code and binaries markers do die out (median 1.6 and 4.4 MiB). On Wikidata JSON they never do: 7–48% of all output symbols were still markers across 16 MiB spans, because the same keys and structure keep being copied forward.

## What isn't known yet

- **Every number here is from a shared 4-vCPU cloud VM** with no hardware counters. On that host rapidgzip scales oddly: in one run it decoded at 0.74 GB/s on one thread but 0.48 on two. The i9-10900K workstation decides, through recipes whose decision rules are written down before the data. None has been registered or run yet.
- **The parallel decoder is a prototype.** If the block finder matches a false block, it stops with an error instead of recovering. It doesn't handle chunks shorter than 32 KiB, multi-member files or corrupt input yet, and hasn't been fuzzed.
- **Correctness so far** is byte-exact output and the CRC check on every timed run. It is also a counterexample hunt: 16,046 restarts from block boundaries in 6,000 random streams, with no mismatch ([the theorem](<https://lmmx.github.io/upigz/journal/speculative-theorem/index.md>)).

Next, the bet that upigz is built around: instead of storing 16-bit symbols for the whole chunk and resolving them later, decode each chunk twice. The first pass keeps only its final window, in cache; the second is an ordinary byte decode from the known history, straight into the output. That spends twice the decoding work to move, by our estimate, about a third as much data through memory: roughly 1.6 bytes per output byte instead of 5. The thread sweep on the i9 will show whether that trade pays.

The evidence is in the journals: [interleaving](<https://lmmx.github.io/upigz/journal/interleaved-inflate/index.md>), [the kernel and the first parallel win](<https://lmmx.github.io/upigz/journal/kernel-and-parallel/index.md>) and [the speculative-decode theorem](<https://lmmx.github.io/upigz/journal/speculative-theorem/index.md>).
