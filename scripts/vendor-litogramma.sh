#!/usr/bin/env bash
# Keep the litogramma files shipped in the embedded standard
# library in step with the litogramma repo (the workbench, where
# definitions are edited and tried with local precedence, no
# rebuild). Every file present in both places must be
# byte-identical; the vendored set is derived, not listed: any
# std file the litogramma repo also has.
#
#   scripts/vendor-litogramma.sh [check|pull|push] [<litogramma-dir>]
#
#   check  report drift (default; exit 1 on any difference)
#   pull   litogramma repo -> std   (after editing in the workbench)
#   push   std -> litogramma repo   (after editing std directly)
#
# The test suite runs the same check (tests/litogramma.rs) when
# the sibling repo is present.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
std=$here/crates/atrep/std
mode=${1:-check}
repo=${2:-$here/../litogramma}
[ -d "$repo" ] || { echo "litogramma repo not found at $repo" >&2; exit 2; }

drift=0
for path in "$std"/*; do
    f=$(basename "$path")
    theirs=$repo/$f
    [ -f "$theirs" ] || continue
    if cmp -s "$path" "$theirs"; then
        continue
    fi
    case $mode in
        check) echo "drift: $f"; drift=1 ;;
        pull)  cp "$theirs" "$path"; echo "pulled: $f" ;;
        push)  cp "$path" "$theirs"; echo "pushed: $f" ;;
        *) echo "usage: $0 [check|pull|push] [<litogramma-dir>]" >&2; exit 2 ;;
    esac
done
if [ "$mode" = check ]; then
    [ $drift -eq 0 ] && echo "std and litogramma agree"
    exit $drift
fi
