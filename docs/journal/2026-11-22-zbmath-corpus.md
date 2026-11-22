# The zbMATH Open KG dump as a corpus: one member, and a hard case (2026-11-22)

*Research host, after its restart: a shared 4-vCPU VM. Indicative; the i9 decides.*

The C1a pre-data post is
[#2 comment 7082185184](https://github.com/lmmx/upigz/pull/2#issuecomment-7082185184).
[zbmath-pq](https://github.com/lmmx/zbmath-pq) is upigz's first downstream user. It will decode
this dump through a switchable backend: rapidgzip now, upigz once it's ready.

## The file

- **Source:** `zbmath-kg-ttl-2026.tar.gz` from Zenodo record 21497976 (concept DOI
  10.5281/zenodo.21497975).
- **Size and digests:** 1,621,106,686 bytes; md5 `0846954acba8f9149a30cb91910d3c94`, which
  matches the record; sha256 `c88225a0…5b962079`.
- **Rebuilding it:** `bench/zbmath_corpus.py fetch DIR` downloads it and checks all three.
- **Decoded:** 10,765,271,040 bytes, sha256 `6f0781f6…c08f07be`: a tar of 11 Turtle files
  (`out-ttl-v2/out-{1..8,401..404}-v2.ttl`).

**Its gzip structure:**
- It is **one member**. zlib (`scripts/members_census.py`) finds exactly one 10.77 GB member.
- It is not BGZF: there's no FEXTRA field.
- zbmath-pq's reconnaissance counts "11 members", but those are tar members, not gzip members.

So it is upigz's target case exactly, like the Wikidata dump: one large member that only
speculative decoding can split.

## The race

`scripts/race_zbmath.py`: 6 timed rounds after one warm-up, shuffled, whole decodes to
`/dev/null`, CRC checked by every arm (`rapidgzip --verify`). Every arm's output was first
checked against the decoded sha256. That includes upigz's ISIZE check, which is modulo 2³²
here, since the output is over 4 GiB. Raw rounds are in `runs/zbmath-race.txt`.

| | GB/s | time vs rapidgzip -P4 | time vs igzip | CPU s | peak RSS |
|---|---|---|---|---|---|
| **upigz -P4 (safe, the default)** | **1.49** | **0.766** [0.743, 0.788] | 0.972 [0.951, 0.993] | 26.1 | 1882 MiB |
| upigz -P4 (unchecked) | 1.65 | 0.695 [0.678, 0.713] | 0.883 [0.859, 0.907] | 23.4 | 1882 MiB |
| igzip 2.31, 1 thread | 1.45 | 0.788 [0.753, 0.824] | 1.000 | 7.4 | 13 MiB |
| rapidgzip 0.16.2 -P1 | 1.22 | 0.929 [0.910, 0.949] | 1.180 | 8.7 | 146 MiB |
| rapidgzip 0.16.2 -P4 | 1.12 | 1.000 | 1.269 [1.214, 1.327] | 33.7 | 596 MiB |

## What it shows

- **upigz beats rapidgzip on its real downstream input:** 0.766× the time at 4 threads, with
  22% less CPU.
- **This is a hard case for every parallel decoder,** because the text decodes fast on one
  thread: igzip reaches 1.45 GB/s on a single core. rapidgzip on four threads is slower than on
  one. upigz's four threads barely beat one igzip thread (0.972×), for 3.5× the CPU.
  - Roughly, each upigz worker in its 16-bit pass decodes at under a third of a single-thread
    kernel's speed.
  - The 16-bit pass-1 kernel (FRONTIER F3) is the binding cost here. The re-decode bet (F1)
    and pass 1's cost alone (F2) are the same question from another side.
- **Memory:** upigz's peak RSS is 1.88 GB, because `zcat` reads the whole 1.6 GB compressed file
  into memory before decoding. For zbmath-pq, which streams the tarball into a parser, that's a
  real cost. Reading the input through a bounded window, as the output already is, goes on the
  frontier.
- **The earlier baselines** (zbmath-pq's reconnaissance: `gzip -dc` 55.3 s, rapidgzip -P4
  13.9 s wall and 41 s CPU) are from another session's machine and unrecorded command lines.
  On this host rapidgzip -P4 took 9.6 s wall and 33.7 s CPU. The two aren't comparable, and
  this race doesn't rely on them.

## Next

1. Add this file to the i9 race's next registration, beside Wikidata. Its single-thread speed
   makes it the case where parallel overhead shows first.
2. **F3, the 16-bit pass-1 kernel,** before the re-decode bet: on this corpus it decides whether
   four cores are worth more than one.
3. Make the compressed input to `zcat` bounded or memory-mapped, so peak RSS doesn't scale with
   the file.
