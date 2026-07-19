#!/usr/bin/env bash
# Typeset scripture as a print-style PDF: any .usfm/.sfm/.usx/
# .osis source (single book or whole Bible) through the at-usfm
# pipeline and the atrep-bible document class.
#
#   scripts/bible-pdf.sh <source> [<out.pdf>]
#
# Requires xelatex (or lualatex via TEX=lualatex).
set -euo pipefail
src=${1:?usage: bible-pdf.sh <source.usfm|usx|osis> [out.pdf]}
out=${2:-${src%.*}.pdf}
tex_engine=${TEX:-xelatex}
bin=$(dirname "$0")/../target/release/atrep
[ -x "$bin" ] || bin=atrep

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
name=$(basename "${out%.pdf}")

"$bin" endo "$src" -o "$work/$name.atd" >/dev/null
"$bin" kanonizo "$work/$name.atd" -o "$work/$name.atk" >/dev/null
"$bin" exo "$work/$name.atk" latex -o "$work/$name.tex" >/dev/null
(cd "$work" && "$tex_engine" -interaction=nonstopmode -halt-on-error "$name.tex" >/dev/null \
            && "$tex_engine" -interaction=nonstopmode -halt-on-error "$name.tex" >/dev/null)
cp "$work/$name.pdf" "$out"
echo "$out"
