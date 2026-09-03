# atrep-lsp

Language server for **atrep** documents (`.atd`/`.atk`) and
dialektos definitions (`.dia`/`.lektos`), over the `atrep`
engine. stdio transport. It is the second binary of the
`atrep-cli` crate — a thin transport over the engine, released
and distributed together with the `atrep` CLI (crates.io and
the PyPI `atrep-cli` wheel both carry it).

Surface:

- **Diagnostics** — the buffer is parsed on open and on every
  change (`atrep` `check_source`: documents and definitions
  are routed on the declaration line); the first parse error is
  published with its source location.
- **Semantic tokens** (full-document) — lexical highlighting of
  the declaration (`@@@!id@version`), sim and episim symbols,
  monosim parameters (numeric taxis vs. onym), comments
  (single- and multi-line, at any sigil-run depth), and escaped
  sigils. When the document's dialektos resolves (locally beside
  the file, or from the embedded std set — litogramma included),
  symbols are matched against it with the parser's own
  longest-match rule, and undefined sims render distinctly.
  Alias-sigil (`\`) documents are handled.

Structural features, for documents whose outline assembles (the
engine's `atrep outline` data: block spans recorded by the
parser, milestones, onyms, deixes — dialektos-generic, so they
work for litogramma, lexigramma, at-docbook, or any dialektos
that resolves):

- **Document symbols** — the nested block tree (sim name plus
  lemma; symbol, genoses, and onym in the detail), with each
  block's milestones and standalone onym anchors as leaf
  children, so a coordinate such as `steph:17a` is one symbol
  search away.
- **Folding ranges** — one per multi-line block; the opening
  line stays visible.
- **Go to definition** — on a `(onym)` group: the onym's
  declaration (the block's episim line or the standalone
  anchor). **References** — every deixis pointing at the onym,
  plus the declaration when the client asks for it.
- **Selection ranges** — the enclosing-block chain at the
  cursor, innermost first: the structural text objects.
- **Hover** on a sim symbol or episim — the sim's name, form
  (endo-simmere, para-simmere, monosim), symbol pair, and the
  definition's descriptions from the resolved dialektos.

Not yet: atramento (the loose syntax compiles via `atrep endo`;
its own LSP comes later), incremental sync, completions, rename.

## Install

Any of:

```
cargo install atrep-cli        # crates.io: atrep + atrep-lsp
pip install atrep-cli          # PyPI wheel, same two binaries
cargo build -p atrep-cli --release   # from a checkout
```

The binary is `atrep-lsp`; it speaks LSP on stdin/stdout.
Editor package managers that install from PyPI or crates.io
(mason.nvim, for one) can fetch it through either registry.

## Editor configuration

**Helix** (`languages.toml`):

```toml
[language-server.atrep]
command = "atrep-lsp"

[[language]]
name = "atrep"
scope = "source.atrep"
file-types = ["atd", "atk", "dia", "lektos"]
language-servers = ["atrep"]
```

**Neovim** (0.11+):

```lua
vim.filetype.add({ extension = { atd = "atrep", atk = "atrep",
                                 dia = "atrep", lektos = "atrep" } })
vim.lsp.config("atrep", {
  cmd = { "atrep-lsp" },
  filetypes = { "atrep" },
})
vim.lsp.enable("atrep")
```

**VS Code**: needs a thin client extension (planned); any generic
LSP client extension pointed at `atrep-lsp` with the file types
above works meanwhile.
