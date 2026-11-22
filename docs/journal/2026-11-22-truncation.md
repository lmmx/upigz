# Truncated input: rapidgzip 0.16.2 exits 0 (2026-11-22)

Reported from zbmath-pq's decoder backend work (lmmx/zbmath-pq#2), reproduced here with
`scripts/truncation.sh` (`runs/truncation-wikidata.txt`). The script cuts
`wikidata.json.6.gz`, one 10 MB member, at 10%, 50%, 90% and 99.99% of its bytes, so the cut
lands inside the deflate stream. Then it decodes the cut file with each tool.

| cut | rapidgzip -P4 | rapidgzip -P1 | rapidgzip -P4 --verify | gzip | igzip | upigz zcat |
|---|---|---|---|---|---|---|
| 10% | exit 0 | exit 0 | exit 0 | exit 1 | exit 1 | exit 101 |
| 50% | exit 0 | exit 0 | exit 0 | exit 1 | exit 1 | exit 101 |
| 90% | exit 0 | exit 0 | exit 0 | exit 1 | exit 1 | exit 101 |
| 99.99% | exit 0 | exit 0 | exit 0 | exit 1 | exit 1 | exit 101 |

- **rapidgzip 0.16.2 accepts a truncated stream silently.** It writes everything it could decode
  and exits 0, even with `--verify`. A pipeline that trusts its exit status takes a cut download
  for a complete one. zbmath-pq adds its own end-of-input check for this reason.
- **gzip and igzip** write the same prefix and exit 1.
- **upigz fails, but not cleanly.** It exits 101 from a panic
  (`called Result::unwrap() on an Err value: "input overrun"`), not with a clear error message.
  - Before failing, it writes the chunks it has finished, which is less than the others: a
    chunk is written only once it is resolved and checked.
  - It never exits 0 on truncated input, because the trailer's CRC-32 and ISIZE are always
    checked.
  - What it lacks is zlib's error behaviour: a clear message and a defined exit status. That is
    on the frontier as F4 (parallel robustness: zlib error parity, fuzzing).

**For the comparisons:** every upigz figure so far is against rapidgzip with `--verify`. That
checks the CRC of what was decoded, but not that the stream was complete. So upigz's results
include a check that rapidgzip's don't. The cost of that check is small: one comparison
against the trailer.
