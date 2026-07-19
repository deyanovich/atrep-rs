#!/usr/bin/env bash
# Generate a conversion corpus with pandoc and drive it through
# the atrep importers. Divergences and rejections are findings
# (coverage gaps in our subsets, or pandoc constructs outside
# the canonical forms), not necessarily bugs - the harness
# reports, it does not gate.
#
#   scripts/pandoc-corpus.sh [seed.md ...]
#
# Each Markdown seed is converted by pandoc into every format we
# import (markdown, html, rst, org) and fed to `atrep endo`.
set -u
cd "$(dirname "$0")/.."
command -v pandoc >/dev/null || { echo "pandoc not found"; exit 1; }
cargo build -q --release
AT=target/release/atrep
CORPUS=target/pandoc-corpus
mkdir -p "$CORPUS"

seeds=("$@")
[ ${#seeds[@]} -eq 0 ] && seeds=(README.md)

pass=0 fail=0
for seed in "${seeds[@]}"; do
    base=$(basename "${seed%.*}")
    for fmt in markdown:md html:html rst:rst org:org; do
        writer=${fmt%%:*} ext=${fmt##*:}
        out="$CORPUS/$base.$ext"
        pandoc "$seed" -t "$writer" -o "$out" 2>/dev/null || continue
        if err=$("$AT" endo "$out" -o "$CORPUS/$base.$ext.atd" 2>&1); then
            echo "PASS  $base.$ext"
            pass=$((pass+1))
        else
            echo "FAIL  $base.$ext  ${err#atrep: error: }"
            fail=$((fail+1))
        fi
    done
done
echo "----"
echo "$pass imported, $fail rejected (rejections are findings)"
