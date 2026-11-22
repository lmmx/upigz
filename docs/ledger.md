# The footprint and queue ledger

A template, from mibz2's `docs/ledger.md`: what the parallel decoder (`src/par.rs`) allocates,
what of it holds data still to be read, what of it is touched at random, and which stage is
waiting. It has two halves:

- **the ledger:** arithmetic from the code, per chunk and per thread count;
- **the measurements:** high-water marks and queue states from the decoder's own timeline.

Neither exists for upigz yet. The re-decode bet (research-charter/FRONTIER.md F1) turns on these
figures: the DRAM bytes per output byte of marker resolution against a second decode.

## Three quantities, kept apart

- **Reserved** is a `Vec`'s capacity: address space asked of the allocator. Pages never written
  are not resident, so peak RSS can be below it.
- **Live** is the bytes holding data that will still be read: a chunk's 16-bit symbols until
  they are resolved, a chunk's bytes until they are written out, a window until the next
  chunk's resolution has used it.
- **Hot (a model)** is what a stage touches at random: the 32 KiB window during resolution, and
  the 16-bit symbols the window copies read back. This is a model of the working set, not a
  measured cache residency.

**RSS is resident pages, not last-level-cache residency.** None of these figures says what is
in a cache.

## The ledger

**TODO.** What is known from the second journal entry
(`docs/journal/2026-11-22-kernel-and-parallel.md`):
- at most threads + 2 chunks are in flight, with recycled buffers and streamed output;
- chunk k ≥ 1 decodes to `u16` (2 bytes a symbol) from 32,768 window markers; chunk 0 decodes
  straight to bytes;
- each chunk's last 32 KiB is resolved through the previous chunk's window, in order.

## The measurements

**TODO:** peak RSS, minor faults and DRAM bytes per output byte for upigz and rapidgzip, from
the i9's thread sweep (FRONTIER.md F1; the journal's "Open" 5).

## What these figures cannot tell you

- Reserved and live bytes are not cache residency.
- A research-host figure says nothing about the i9's memory system.
