#!/usr/bin/env bash
# Truncate a single-member gzip file inside its deflate stream at several fractions and record
# each decoder's exit status and output size (stdout to a file, stderr dropped).
#   bash scripts/truncation.sh FILE.gz RAPIDGZIP UPIGZ
set -uo pipefail
f=$1; rg=$2; up=$3
n=$(stat -c%s "$f"); d=$(mktemp -d); trap 'rm -rf "$d"' EXIT
printf '%-8s %-22s %-22s %-22s %-22s %-22s %-22s\n' cut rapidgzip-P4 rapidgzip-P1 rapidgzip-P4-verify gzip igzip upigz-zcat
r() { "$@" > "$d/out" 2>/dev/null; echo "exit $? ($(stat -c%s "$d/out") B)"; }
for frac in 0.10 0.50 0.90 0.9999; do
  c=$(python3 -c "print(int($n * $frac))"); head -c "$c" "$f" > "$d/t.gz"
  printf '%-8s %-22s %-22s %-22s %-22s %-22s %-22s\n' "$frac" "$(r "$rg" -P 4 -dc "$d/t.gz")" "$(r "$rg" -P 1 -dc "$d/t.gz")" \
    "$(r "$rg" -P 4 --verify -dc "$d/t.gz")" "$(r gzip -dc "$d/t.gz")" "$(r igzip -dc "$d/t.gz")" "$(r "$up" zcat "$d/t.gz" 4 512)"
done
