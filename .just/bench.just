# Workstation bench recipes.
#
# Each recipe here follows CLAUDE.md's "Workstation recipes: one tarball back": one new
# timestamped directory per attempt with its own console log, nothing written to the home
# directory, and `bash bench/bundle.sh "$DIR"` at the end. A recipe whose run is evidence is run
# as `just submit <recipe>` (.just/submit.just), queued in bench/workstation-queue.txt and
# frozen, with its decision rule, before its data (docs/team/research-charter/README.md).

# The i9 race (docs/journal/2026-11-22-i9-race-registration.md, registered before any run):
# upigz's streaming parallel decoder against rapidgzip 0.16.2 at 4, 8, 10 and 20 threads, and
# igzip on one thread, on the 1.34 GB single-member Wikidata file, 20 paired rounds, judged by
# bench/i9_race.py. Needs uv, gzip, igzip (ISA-L) and libisal-dev on the machine. About 15
# minutes, most of it the first corpus build; it ends with one .tar.gz.
#   just submit bench-i9-race
# A DRY value is a check of the recipe itself (two rounds), never evidence:
#   just bench-i9-race 1
bench-i9-race DRY="":
    #!/usr/bin/env bash
    set -euo pipefail
    B=$PWD/target/i9-race
    W=$B/$(date -u +%Y%m%dT%H%M%SZ)
    mkdir -p "$B"
    mkdir "$W" || { echo "bench-i9-race: $W already exists; not reusing an attempt's directory"; exit 1; }
    exec > >(tee -a "$W/console.log") 2>&1
    outcome="INCOMPLETE: stopped before the run finished (see console.log)"
    finish() {
      status=$?
      echo "$outcome" > "$W/outcome.txt"
      echo "bench-i9-race: OUTCOME: $outcome"
      bash bench/bundle.sh "$W"
      case "$outcome" in
        "VALID "*) exit 0 ;;
        INVALID*) exit 1 ;;
        *) [ "$status" -ne 0 ] && exit "$status"; exit 2 ;;
      esac
    }
    trap finish EXIT
    fail() { outcome="REFUSED: $*"; echo "bench-i9-race: $*"; exit 1; }
    rounds=20; dry="{{DRY}}"
    if [ -n "$dry" ]; then
      rounds=2
      echo "bench-i9-race: DRY RUN: two rounds; not evidence" | tee "$W/DRY-RUN.txt"
    fi
    # the frozen candidate (bench/i9-race.frozen): HEAD's build and workload inputs must match it
    frozen=bench/i9-race.frozen
    cand=$(awk '$1 == "candidate" {print $2}' "$frozen")
    measured=(src Cargo.toml Cargo.lock .cargo build.rs csrc rust-toolchain rust-toolchain.toml
      bench/i9_race.py bench/wikidata_corpus.py bench/bundle.sh .just/bench.just
      docs/journal/2026-11-22-i9-race-registration.md)
    if [ -z "$dry" ]; then
      [[ $cand =~ ^[0-9a-f]{40}$ ]] || fail "$frozen names no frozen candidate yet; nothing to run"
      git cat-file -e "$cand^{commit}" || fail "fetch the branch: the frozen candidate is missing"
      git diff --quiet "$cand" HEAD -- "${measured[@]}" || fail "HEAD's build or workload inputs differ from the frozen candidate $cand"
    fi
    overrides=$(env | cut -d= -f1 | grep -E '^(CARGO_|RUST)' | grep -vxE 'CARGO_HOME|RUSTUP_HOME|RUST_BACKTRACE|RUST_LOG|CARGO_(HTTP|NET|TERM)_[A-Z_]+' || true)
    [ -z "$overrides" ] || fail "unset these, so the builds run the registered way: $(echo $overrides)"
    [ -z "$(git status --porcelain --untracked-files=no)" ] || fail "the checkout has uncommitted changes"
    [ -z "$(git status --porcelain --untracked-files=all -- "${measured[@]}")" ] || fail "untracked files among the build and workload inputs"
    for t in uv gzip igzip; do command -v "$t" >/dev/null || fail "$t is not installed"; done
    head=$(git rev-parse HEAD)
    printf 'head %s\ncandidate %s\n' "$head" "${cand:-unset (dry run)}" > "$W/commits.txt"
    # the two upigz builds: the safe default (the decided arm) and the research `unchecked` one
    mkdir "$W/bin"
    cargo build -q --release --target-dir "$B/target-safe" || fail "the safe build failed"
    cp "$B/target-safe/release/upigz" "$W/bin/upigz"
    cargo build -q --release --features unchecked --target-dir "$B/target-unchecked" || fail "the unchecked build failed"
    cp "$B/target-unchecked/release/upigz" "$W/bin/upigz-unchecked"
    # rapidgzip, pinned, in a venv under target/ (uv's cache too: nothing in the home directory)
    export UV_CACHE_DIR=$B/uv-cache
    [ -x "$B/venv/bin/rapidgzip" ] || { uv venv -q -p 3.13 "$B/venv" && uv pip install -q --python "$B/venv/bin/python" rapidgzip==0.16.2; } || fail "could not install rapidgzip 0.16.2"
    "$B/venv/bin/rapidgzip" --version 2>&1 | grep -q "0\.16\.2" || fail "rapidgzip is not 0.16.2"
    # the corpus, rebuilt from the dated dump and checked against its registered digest
    python3 -I bench/wikidata_corpus.py build "$B/corpus" || fail "the corpus could not be rebuilt"
    raw_sha=$(python3 -I -c 'import sys; sys.path.insert(0, "bench"); import wikidata_corpus as w; print(w.RAW_SHA256)')
    {
      sha256sum "$W/bin/upigz" "$W/bin/upigz-unchecked" "$B/venv/bin/rapidgzip" "$(command -v igzip)" "$B/corpus/wd-race.json.gz"
      echo "raw sha256 $raw_sha"
      "$B/venv/bin/rapidgzip" --version 2>&1 | head -1
      igzip --version 2>&1 | head -1
      gzip --version | head -1
    } > "$W/inputs.txt"
    cat "$W/inputs.txt"
    lscpu > "$W/lscpu.txt"
    { uname -a; free -m; cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo "governor: unknown"; uptime; } > "$W/machine.txt"
    python3 -I bench/i9_race.py run "$W/runs.jsonl" --corpus "$B/corpus/wd-race.json.gz" --raw-sha256 "$raw_sha" \
      --rounds "$rounds" --upigz "$W/bin/upigz" --upigz-unchecked "$W/bin/upigz-unchecked" \
      --rapidgzip "$B/venv/bin/rapidgzip" --igzip "$(command -v igzip)"
    set +e
    python3 -I bench/i9_race.py report "$W/runs.jsonl" --out "$W/i9-race"
    rc=$?
    set -e
    if [ "$rc" -le 1 ] && [ -s "$W/i9-race.json" ]; then
      outcome=$(python3 -c 'import json,sys; v=json.load(open(sys.argv[1])); print(v["verdict"] + ("" if v["valid"] else ": " + "; ".join(v["problems"])))' "$W/i9-race.json")
      [ -n "$dry" ] && outcome="DRY: $outcome (two rounds, not evidence)"
    else
      outcome="REPORT ERROR: i9_race.py exited $rc without writing its report"
    fi
