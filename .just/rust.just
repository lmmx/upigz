build:
    cargo build --release

# The research arm: the fast loop's accesses unchecked (safety argument in src/fast.rs)
build-unchecked:
    cargo build --release --features unchecked --target-dir target/unchecked

check:
    cargo check --all-targets

# -------------------------------------

clippy:
    cargo clippy --all-targets --all-features --target-dir target/clippy-all-features -- -D warnings

# -------------------------------------

test *args:
    cargo test --release {{args}}

# The speculative-decode counterexample hunt (docs/journal/2026-11-22-speculative-theorem.md):
# N random streams, every block start checked on both decoders. CI runs 50.
spectest n="50": build
    target/release/upigz spectest {{n}}

# -------------------------------------

docs:
    cargo doc --all-features --no-deps --document-private-items --keep-going

# -------------------------------------

clean:
    cargo clean
