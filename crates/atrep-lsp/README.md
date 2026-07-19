# atrep-lsp

Language server for **atrep** documents (`.atd`/`.atk`) and
dialektos definitions (`.dia`/`.lektos`), over the `atrep`
engine. stdio transport.

v1 surface:

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

Not yet: atramento (the loose syntax compiles via `atrep endo`;
its own LSP comes later), incremental sync, completions, hover,
go-to-definition for onyms/deixes.

## Build

```
cargo build -p atrep-lsp --release
```

The binary is `atrep-lsp`; it speaks LSP on stdin/stdout.

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
