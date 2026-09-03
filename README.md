# atrep-rs

Pilot Rust implementation of the **Atrep** core: parser, Dendron
(AST), kanonizo (canonicalization), litosis (content hashing),
metamorphoses (exomorphosis engine; Markdown, HTML, and
reStructuredText endomorphoses), and morphisms (.hom/.iso with
group rules and transitive route resolution, spec v0.11).

## Spec reference

This implementation targets the **Atreptos Platform Specification
v0.12.1** (the Dialektoi Network and Witnesses & Coordinates
freezes plus their addenda), as published in the `atrep/spec`
repository:

- tag: `v0.12.1`
- commit: `3c72861`

Where behavior is underspecified at that commit, the choices made
here are documented under "Spec gaps" below, as input for the next
spec revision.

## Layout

- `crates/atrep` — library: `sigil` (escaping, symbol
  classes), `dialektos` (`.lektos`/`.dia` definitions, local
  inheritance and import), `parser` (document to Dendron),
  `dendron` (AST + canonical serializer), `kanonizo`, `litosis`.
- `crates/atrep-cli` — two binaries, one crate: the `atrep`
  CLI, and the `atrep-lsp` language server (diagnostics +
  semantic-token highlighting for `.atd`/`.atk` documents and
  `.dia`/`.lektos` definitions, and — over the outline —
  document symbols, folding, onym definition and references,
  selection ranges, and sim hover for documents; see
  `crates/atrep-cli/LSP.md` for editor configuration). Both
  ship with `cargo install atrep-cli` and the PyPI `atrep-cli`
  wheel.

## Usage

```
atrep check <file>                 parse + validate a document
                                   (.atd/.atk) or definition
                                   (.lektos/.dia)
atrep kanonizo <file> [-o out]     .atd -> .atk
                                   (+ .atk.tar.gz when media
                                    is bundled);
                                   .dia -> .lektos for
                                   definition files
              [--timeout <secs>]   remote-fetch timeout (30)
              [--retries <n>]      remote-fetch retries (2)
atrep morph <file> <dialektos> [-o out]
                                   map a kanon to another dialektos
                                   via .hom/.iso rules or a
                                   lineage-derived embedding
atrep litos <file> [--save-litos-file]
                                   print the litos ID (SHA-256)
atrep exo <file> <target> [-o out]
                                   render a kanon to an external
                                   format via <dialektos>.<target>.exo
                                   (.atd input is kanonized first)
atrep endo <file> [-o out]         import Markdown (.md), HTML
                                   (.html), reStructuredText
                                   (.rst), or TEI (.xml/.tei) as
                                   at-markdown/at-html/at-rst/
                                   litogramma documents
```

Dialektoi resolve local-only, per the v0.10 spec: the parser looks
for `<dialect-id>.lektos` (falling back to `<dialect-id>.dia`) in
the directory of the file being processed. Remote *resources*
(transclusion targets and media) may be `http(s)` URLs.

`scripts/pandoc-corpus.sh` uses pandoc as a specimen
generator: each Markdown seed converts into every imported
format and runs through `atrep endo`; rejections are coverage
findings, not gated failures.

The crate builds for `wasm32-unknown-unknown` with
`--no-default-features`: the `net` feature (remote resource
fetching) and the `bundle` feature (`.atk.tar.gz` archive
output) are on by default and off for wasm targets.

Milestones and zygosis (spec v0.12): `@("scheme:value")` is
the core global anchor — the quoted sibling of the onym
anchor, kanonizo-exempt (duplicates rejected) and
litosis-surviving. Exos render it through the `*milestone
[<scheme>]` pattern with `@(scheme)`/`@(value)` slots. `atrep
zygo a.atk b.atk --scheme stephanus --as grc,eng -o bi.atd`
weaves witness kanons of one dialektos into the zygoma:
milestone-headed segments in the merged coordinate order
(order-consistency checked, contradictions are errors), each
witness's slice on a `zyg-<id>`-tagged diaphane, blocks and
even inline phrases split cleanly at cut points, headings and
speech prefixes hoisted so they don't repeat. The TEI importer
emits core milestones (scheme from resp, unit as a
presentation genos).

Controlled vocabularies (spec v0.11): a dialektos may define
named term sets with multilingual aliases, bound to a sim's
lemma or genoses; kanonizo canonicalizes aliases
case-insensitively (authoring language cannot fork hashes),
vocabulary-bound genoses survive litosis as semantic content,
and exo rules gain term-keyed variants (`@-> :(auctor)`).
Bibliogramma drives it: Latin canonical terms, ~40 languages of
aliases, biblatex as the lossless export and classic BibTeX as
the lossy one.

Resolution is storage-agnostic behind the `source::Source` trait:
`DirSource` is the classical directory context, `MemorySource`
serves definitions, morphisms, and exos from an in-memory map
(embedded engines, registry services, wasm targets with no
filesystem). The `*_from` variants (`dialektos::resolve_from`,
`morph::resolve_morph_from` / `resolve_route_from`,
`exo::resolve_exo_from`) take any source; the path-based entry
points delegate to `DirSource`.

## Conformance status (spec v0.10)

Implemented:

- The three basic forms (endo-simmere, para-simmere, monosim) with
  onym/genos/taxis annotations; both sigils (`@` / `\`) with the
  inactive-sigil escaping rules (`\@`, `@\`, `\|`); bracket
  matching in episims; core stichoi (`@@@=`); comments; diaphanes;
  axiomata; verbatim and semantic inclusion (inline, block, and
  file forms); enmedia; the deixis (`@<sym>(onym)`, spec v0.10.1) —
  a core pointing reference to an onymized para-simmere, validated
  against its target's sim, surviving litosis with its target's
  onym under fresh declaration-order numbering (so stripped
  metadata cannot influence the litos ID), and rendered via the
  `*deixis <symbol>` exo pattern.
- Dialektos definitions in the `atrep` meta-dialektos: ostensive
  sim definitions (including line-structured grammata via the
  `stichos` keyword), descriptions, parent references, full
  inheritance, sim import (single, aliased, list, exclusion) —
  local-only, with conflict and cycle detection.
- Kanonizo: shebang removal, sigil canonicalization with escape
  recomputation, Unicode NFC, recursive transclusion expansion
  with cycle detection (local paths and URLs), axioma expansion,
  axioma-enlexis resolution against onymized simmeres, onym
  canonicalization (`o1`, `o2`, ...), taxis evaluation and
  validation, comment and unreferenced-diaphane removal,
  paragraph collapsing, whitespace normalization, media bundling
  (`media/m<n>.<ext>`, manifest, deterministic `.atk.tar.gz`).
- Remote resources over `http(s)`: transclusion targets and
  enmedia, with configurable timeout/retry, extension inference
  (source extension, then Content-Type, then magic bytes), and
  source-URL + checksum provenance in the media manifest.
- Definition-file kanonizo (`.dia` -> `.lektos`): inheritance and
  imports expanded, canonical sigil, sim definitions sorted by
  name; re-emitting a canonical `.lektos` is byte-identical.
- Litosis and the litos ID (SHA-256), including enmedia content
  hashing (`@@@@[SHA256:...]`).
- Exomorphosis (spec v0.10.1, optional conformance class): `.exo`
  files in the `atrep-exo` meta-dialektos — symbol-keyed template
  rules with genos-suffix specificity, structural `*` patterns
  (including `*solo` and `*deixis`), sigil slots with raw and
  hanging-prefix forms, conditional sections (`@[ ... ]@`,
  emitted only when every slot inside renders non-empty),
  single-pass escape tables, englossis rule-set switching,
  strict unhandled-pattern errors, deterministic output. Rule
  inheritance follows the dialektos lineage (spec v0.10.2 draft):
  parents' effective rule sets are pulled in declaration order
  (later ops overriding earlier), filtered and alias-remapped
  per import kind, then overlaid by the dialektos's own file; a
  dialektos with no `.exo` of its own still renders through
  inherited rules, and the most derived non-empty escape table
  applies. The spec's worked example (`exempli.html.exo`) is a
  golden test.
- A standard library, embedded in the binary and used when local
  resolution fails: `at-html`, a syntax mapper between a useful
  subset of HTML and atrep (each sim corresponds to one HTML
  element; its exomorphosis and the HTML endomorphosis are
  inverses over the canonical subset), and `at-markdown`
  (realizing the dialektos the spec's inheritance examples treat
  as hypothetical), which is pure lineage: every sim is inherited
  from at-html — containers excluded (markdown is flat), emphasis
  and strong aliased to `*`/`**` — and its HTML rendering is
  acquired entirely through rule inheritance, with no HTML `.exo`
  of its own. The library also carries the literary target
  itself, promoted to core: `litogramma` and `bibliogramma`,
  vendored byte-identical from the normative litogramma repo (a
  drift-guard test compares the copies whenever the sibling
  checkout is present), together with their base exomorphoses
  (`html`, `gemtext`, `latex`; `bib`, `bibtex`) and the
  litogramma↔at-html/at-docbook/at-tei homs — so the
  litogramma-targeting importers (TEI, JATS, BibTeX, atramento)
  have their target resolvable everywhere, with no local
  definition files. The
  publisher-register exos (Loeb, OCT, Penguin, GOST, …) remain
  in the litogramma repo.
- Morphisms (spec v0.11, optional conformance class):
  `.hom` (directional, possibly lossy) and `.iso` (rename-only,
  bijective, one file serving both directions) in the
  atrep-hom/atrep-iso meta-dialektoi — rename with form-
  compatibility validation, unwrap (wrapper dissolves, content
  splices), drop; implicit identity for same-symbol
  form-compatible sims; core forms pass through; deixes follow
  their target sim's rule (removed with it on drop/unwrap); the
  output kanon is re-validated (taxis runs, deixis targets).
  Embedding morphisms are derived from the dialektos lineage
  with no file at all: at-markdown => at-html morphs purely from
  its inheritance declarations, and the std
  `at-html.at-markdown.hom` provides the lossy reverse — the
  flat-content round trip is byte-identical. Renames may append
  genoses (admonition kinds survive as presentation hints on
  generic containers) and the extract rule (`@<# <sym>
  <endo-sym>`) dissolves a lemma-bearing para-simmere into a
  flat heading plus content. Morphisms apply **transitively**:
  when no direct morphism resolves, the shortest route through
  the known dialektoi applies in validated stages (ambiguous
  routes are errors, never silent choices) — at-rst =>
  at-markdown composes through at-html with no direct file. A
  pair may carry several morphisms as named variants
  (`<a>.<b>.<variant>.hom`, `atrep morph ... --variant <name>`)
  — distinct representations of the source in the target. The
  std graph carries its first **isomorphism**: at-djot and
  at-markdown share one sim inventory under different symbols
  (`at-djot.at-markdown.iso`), so the pair converts losslessly
  both ways, round-trips to the identity morphism, and
  collapses into one equivalence class.
  The algebra's one sequence-level action is the **group
  family** (`@>#` / `@><` / `@>-` / `@>:`), the flat-to-nested
  converse of extract: a solo-heading paragraph triggers a
  region spanning the following siblings, wrapped into a
  para-simmere with the trigger content as lemma (or discarded,
  or the region dropped, or the trigger relabeled/deleted, or —
  operand-less `@>#`, the v0.11.1 deplain — stripped to a plain
  paragraph with the region untouched);
  rules nest by file order, the trigger's onym and genoses
  transfer to the wrapper, and deixes follow. The
  group-then-extract round trip recovers the flat document
  exactly. Morphisms are **statically validated at
  resolution**, with no documents present: every rule against
  both dialektoi, plus *totality* — every source sim accounted
  for by a rule, an implicit identity, or a group trigger, with
  `@--` marking deliberate drops (derived embeddings stay
  partial by nature).
  Morphisms **compose symbolically**: the table algebra is
  three actions (rename, drop, dissolve — with the lemma
  discarded, emitted plain, or extracted into a heading endo)
  and is closed under composition, group dispositions chasing
  the second factor's table, so a route fuses into a single
  composite morphism applied in one traversal (`apply_route`
  does this by default; staged application is the reference
  semantics it must match). Grouping bounds fusion: a grouping
  second factor fuses only across a rename-only first factor,
  and never across a non-injective rename onto a trigger (peer
  triggers would share a rank the serialized form cannot
  express; spec v0.11.1) — everywhere else `apply_route` falls
  back to staged application automatically.
  `atrep compose a b [c ...]` writes
  the composite as an ordinary `.hom` file in normal form —
  group rules in rank order, table rules sorted, implicit
  identities elided, with `@=via` declarations recording the
  hops (provenance, excluded from the `normal_form` equality
  surface) — making morphism equality a byte comparison.
- Endomorphoses (pilot, engine-side): **atramento**, the
  Markdown-like authoring surface for litogramma (`.atr`,
  text-to-text: a litogramma superset whose sugar — typewriter
  emphasis with flanking rules, ATX headings with a declared
  base and implied closure, Markdown lists/quotes/fences,
  autolinks, note callout/definition/inline sugar, verse fences,
  drama auto-closure — compiles away; the compiler is an
  idempotent projection and canonical litogramma passes through
  byte-identical; spec in the atramento repo); CommonMark-subset,
  HTML-subset, and reStructuredText-subset importers into
  `at-markdown`/`at-html`/`at-rst` (strict over the canonical
  subsets; RST footnotes become deixis callouts, fixed adornment
  table for title levels, hyperlink references with embedded
  URIs as the visible-URL link), Org-mode and Djot subset
  importers into the `at-org` and `at-djot` std dialektoi, a
  DocBook subset importer into `at-docbook` (`.dbk`; quote and
  link included); the `at-tei` std dialektos
  applies the bibliogramma pattern to TEI — ~15 form sims with
  the TEI element identity in vocabulary-bound genoses
  (`@,x,@.persname`), a `tei` exo emitting real TEI XML, and a
  litogramma hom giving litogramma TEI export via morph+exo, a
  BibTeX importer into
  `bibliogramma` (`.bib`), a JATS subset importer (`.jats`) into
  litogramma with the ref-list embedded as a bibliogramma
  englossis — the BibTeX-isomorphic bibliography
  extension defined beside litogramma (entry lemma = citation
  key, genos = entry type, fields as lemma-named sims) — the
  first *nested* std
  dialektos: one uniform section sim nests, so
  structure-preserving morphisms to and from litogramma need
  no flattening — (`.org`: star headings, the emphasis
  family, quote/verse/example/src blocks with language genoses,
  pipe tables as stichoi rows, footnotes as deixis callouts,
  links), plus a TEI P5 basic-subset importer
  into `litogramma` (`.xml`/`.tei`): teiHeader front matter,
  nested div sectioning with type/depth laddering, lg/l verse,
  sp/speaker drama with stage directions, epigraphs with
  attribution, and notes as deixis callouts with footnote
  bodies. Roundtrips are tested in all three
  formats: the canonical form of each is a fixed point of
  export∘import.
  The `at-usfm` std dialektos maps scripture (`.usfm`/`.sfm`):
  one dialektos, three export surfaces (`usfm`, `usx`, `osis`).
  Books are vocabulary-bound lemmas (the 66 canonical plus 21
  deuterocanonical USFM codes with English-name aliases, so
  `@# john` and `@# jhn` hash identically); chapters and verses
  are milestone monosims — text flows across verse boundaries,
  the classic overlapping hierarchy; heading markers ride a
  `markers` vocabulary on one typed solo-block sim while
  paragraph-class markers (`\m`, `\pi1`, `\li1`, …) accumulate
  verse flow as typed paragraphs; character markers (`\wj`,
  `\nd`, `\add`, …) are genos-typed phrases, wordlist wrappers
  (`\w …|attrs`, `\+w`) unwrap to their text; footnotes and
  cross-references sit inline at their anchor with `\fr`/`\xo`
  references as typed sub-phrases; poetry lines wrap in
  per-line `q`-level phrases inside stichoi so indent levels
  survive. The OSIS exo maps semantics, not styling (`\wj` →
  `<q who="Jesus">`, `\nd` → `<divineName>`, `\add` →
  `<transChange>`). All three surfaces import as well as
  export: `.usfm`/`.sfm`, `.usx`, and `.osis` files (including
  whole-Bible multi-book files, which become one book block
  per book). The USX surface is information-complete — export
  then import lands on the identical litos; OSIS import
  handles both container and milestone chapter/verse forms,
  Paratext-style sID/eID pairs, and pretty-printed sources
  (insignificant whitespace collapses), while milestone-form
  red-letter `<q sID/>` spans are skipped (the overlapping
  hierarchy OSIS itself avoids in container form).
  The `latex` surface prints it: the exo
  emits a semantic macro layer (`\atbook`, `\atchapter`,
  `\atverse`, `\atmarker{s1}{...}`, `\atchar{wj}{...}`,
  `\atq`, `\atnote`) and carries the `atrep-bible` document
  class as an **asset section** (`@=* <filename> <sentinel>`
  ... sentinel) — a presentation companion delivered
  byte-verbatim as an unconditional auxiliary output, so the
  exo stays one artifact under one content address. The class
  compiles the layer into a two-column dense print Bible:
  drop-number chapters with the first verse suppressed (or
  centered `PSALM n` headings when the book declares `\cl`),
  superscript verses, q-level poetry ladders with right-set
  *Selah*, italic superscriptions, per-column footnotes with
  bold references, running heads from `\h` (falling back to
  `\mt1`), and a `redletter` class option. Genoses interpolate
  as macro arguments, so vocabulary growth touches only the
  class — unknown markers fall through to a visible fallback,
  never an error. Named **exo variants**
  (`<dialektos>.<target>.<variant>.exo`, `atrep exo ...
  --variant <name>`) overlay the base rule set — the
  `redletter` edition redefines only the document rule and
  inherits everything else, class asset included. The class
  sets corner running heads the print way (every verse marks
  `BOOK c:v`; first reference on the verso's outer edge, last
  on the recto's), opens each book at the top of a fresh
  column, sets chapters as lettrine drop numbers, gives
  cross-references their own symbol series beside the numbered
  translation notes, prints a two-column dotted Contents page
  when the document holds more than one book (with PDF
  bookmarks via hyperref), switches to a covering serif for
  Hebrew (right-to-left) and polytonic Greek runs in
  footnotes, and offers a `9pt` option for a denser setting
  and an `a5` exo variant (the classic 148×210 hand-Bible
  format at 9pt). The geometry is calibrated against
  distributed print Bibles (measured block proportions,
  margins, and baseline pitch), and an ink-zone check renders
  every page and asserts nothing prints into the gutter or
  margins.
  `scripts/bible-pdf.sh <src>` typesets any
  USFM/USX/OSIS source (single book or whole Bible) to PDF;
  the complete WEB runs 942 pages, the KJV 853.
  Validated against the complete World English Bible (81
  books through all three surfaces: USFM byte-level fixed
  point with litos identity, USX import litos-identical to
  the USFM-derived kanon, OSIS import→export fixed point,
  well-formed XML throughout) and against the whole-Bible
  KJV OSIS from a foreign toolchain (Haiola, 10 MB, 66 books
  in one file — imports in ~0.2 s and passes the same
  battery).
- Strict error handling: every error condition enumerated in the
  spec's Parsing chapter maps to a typed error with file/line/
  column and, for transcluded content, the inclusion chain.

The spec's worked-example appendix ("From Deltos to Litos") is the
golden test (`crates/atrep/tests/golden.rs`), including a
kanonizo idempotency check. Remote fetching is covered by
`tests/remote.rs` against an ephemeral localhost listener.

Not yet implemented:

- The `.endo` file format (still unspecified by the spec; the
  Markdown and HTML importers are engine-side, as the spec
  allows).

Known Markdown-pilot limitations: exported lists are loose (a
blank line between items); ordered lists must start at 1 (taxis
validation); text paragraphs beginning with a literal list
marker like `1. ` are not escaped and would re-import as items.

Known parsing limitation: paragraph boundaries are detected
line-first, so a verbatim inline (`@@"..."@@`) cannot span a line
that itself looks like a block construct.

## Spec gaps found while implementing

Decisions this pilot had to make where spec v0.10 is silent;
flagged as input for the next spec revision:

1. **Kanon blank-line layout.** "Removes extraneous blank lines"
   does not say which blank lines remain. Pilot rule: one blank
   line between sibling blocks, none around compact single-line
   blocks (enmedia and monosim-shaped forms) — this reproduces
   the spec's worked-example appendix byte-for-byte.
2. **Bracket-matching disablement** has no declared syntax in
   `.lektos` files. The pilot infers it from the ostensive
   definition: an unflipped shown episymbol means matching is
   disabled.
3. **Onym-referencing monosims** are not marked as such in sim
   definitions. The pilot treats any monosim parameter that
   matches a declared onym as a reference (and rewrites it during
   onym canonicalization).
4. **Axioma definitions in the kanon**: the spec says references
   are expanded but not whether definitions remain. The pilot
   removes them (the kanon is fully expanded, so they are dead
   weight).
5. **Dialektos-defined stichoi**: resolved in spec v0.10.1 — the
   `stichos` ostensive keyword (in place of `grammata`) declares
   line-structured grammata, implemented here. Stichos whitespace
   is likewise resolved (litogramma feedback F2): leading and
   internal whitespace is content, preserved through kanonizo and
   litosis (so it participates in the litos ID); only trailing
   whitespace is trimmed. One caveat: a monosim removed by litosis
   mid-stichos leaves its surrounding spaces as written — in verse,
   spacing is content, so no re-normalization is applied.
6. **Version pinning in the kanon**: the pilot preserves the
   declared version specifier verbatim rather than pinning the
   resolved version.
7. **Remote transclusion fetch policy** is specified for media
   only. The pilot applies the media policy to transclusion too:
   same timeout/retry configuration, and an unavailable resource
   fails kanonizo.
8. **Remote documents have no local directory.** Their dialektos
   declarations and any relative transclusion targets resolve
   against the *including* document's directory.
9. **`.lektos` sim ordering**: the spec says "alphabetically by
   sim name" without a collation. The pilot sorts byte-wise on
   the UTF-8 name, with the symbol (byte-wise) as tie-break —
   duplicate names are legal via aliased imports.
10. **Canonical `.lektos` layout** is otherwise unpinned. The
    pilot emits the declaration, then blank-line-separated sim
    blocks; property sims in ASCII symbol order (`"` short
    description, `""` long description, `^` parent); long
    descriptions always in block form with the body verbatim.
11. **Axioma-enlexis to onymized simmeres**: the spec states the
    before-first-reference rule for axiomata only, so the pilot
    allows forward references to simmere onyms. The verbatim copy
    is the simmere's canonical serialization minus its own onym;
    onyms and taxis nested inside the copy keep their source
    form; an onym whose only reference was the expanded enlexis
    is then dropped as unreferenced.
12. **Standard-library resolution**: spec resolution is
    local-only and defines no standard library. The pilot falls
    back to definitions embedded in the binary (`at-markdown`
    and its `md` exomorphosis) when no local file resolves.
13. **The `at-markdown` sim inventory**: the spec's inheritance
    examples pin `#`..`######`, `-`, and `*` (emphasis) but no
    full definition. The pilot adds `**` (strong), `>`
    (blockquote), and `.` (ordered item, required taxis), and
    maps inline code, fenced code, and images to the core
    enlexis/enmedia forms rather than new sims.
14. **Monosim ostensive keyword**: the spec's examples show
    `(param)`, but litogramma uses semantic keywords (`(onym)`,
    `(depth)`, `(key)`). The pilot accepts any identifier except
    the reserved `taxis` and preserves it in the canonical form.
15. **Comments in definition files**: the atrep-meta chapter
    does not say which comment forms `.dia`/`.lektos` files may
    carry. The pilot accepts line (`@@/`) and block
    (`@@@/ ... /@@@`) comments, both removed by kanonizo.

## Development

```
cargo test        # unit + golden + error-condition tests
cargo clippy      # lint (kept warning-free)
cargo fmt         # rustfmt (kept clean)
```
