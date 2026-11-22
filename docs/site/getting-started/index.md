---
description: "Building upigz from source and running its parallel zcat, and what the prototype doesn't do yet."
---

# Getting started

upigz is a research prototype: it is not on crates.io, and its command line will change.

## Build

You need Rust (the version is pinned in `rust-toolchain.toml`, so `rustup` fetches it) and
ISA-L's development files, for the `isal` comparison arm (`libisal-dev` on Debian and Ubuntu).
The build targets `x86-64-v3`, that is AVX2 and BMI2 (`.cargo/config.toml`).

```sh
git clone https://github.com/lmmx/upigz
cd upigz
cargo build --release
```

## Decode a file

```sh
target/release/upigz zcat file.gz 4 1024 > file
```

The arguments are the file, the thread count and the chunk size in KiB. An optional fourth
caps how many chunks are in flight at once (by default, threads + 2). The output goes to
stdout, and the CRC-32 and ISIZE are checked against the trailer: a mismatch aborts.
`UPIGZ_STATS=1` prints the number of chunks and the finder's time to stderr.

## Check the speculative decode

```sh
target/release/upigz spectest 50
```

This builds 50 random DEFLATE streams and checks, at every block start, that decoding without
the history and then filling it in gives exactly the sequential decoder's output. CI runs it on
every push.

## Not yet

- Files with more than one gzip member (the Wikidata dump starts with a 22-byte one).
- Recovering when the finder's guess is wrong: today it stops with an error.
- Chunks shorter than a 32 KiB window.
- The same errors as zlib on corrupt input.
- Fuzzing.
