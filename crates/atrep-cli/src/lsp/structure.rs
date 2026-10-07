//! Structural features over the engine's outline (`atrep outline`
//! data: block spans recorded by the parser, milestones, onyms,
//! deixes) and the resolved dialektos: document symbols, folding
//! ranges, go-to-definition and references over onyms, selection
//! ranges (the enclosing-block chain), and hover on sim symbols.
//! Documents only — definition files have no outline. The grammar
//! is never re-derived from text here: everything positional comes
//! from the parse, and columns are recovered by locating the
//! literal spelling on the already-known line.

use std::path::Path;

use atrep::dialektos::{Dialektos, SimForm};
use atrep::outline::{self, Outline, OutlineBlockNamed};
use atrep::scan::{self, TokKind};
use lsp_types::{
    DocumentSymbol, FoldingRange, Hover, HoverContents, MarkupContent, MarkupKind, Position, Range,
    SelectionRange, SymbolKind,
};
use unicode_normalization::UnicodeNormalization;

/// A parsed document's structure, ready to answer requests.
pub struct Structure {
    pub outline: Outline,
    pub dial: Dialektos,
    lines: Vec<String>,
}

/// Parse the buffer and assemble its outline. `None` for
/// definition sources and for documents that do not parse (the
/// diagnostics channel already reports the error). The parse sees
/// the NFC form, as `check` does; lines are kept as the buffer
/// has them, since positions refer to it.
pub fn analyze(source: &str, path: &Path) -> Option<Structure> {
    if atrep::is_definition_source(source) {
        return None;
    }
    let normalized: String = source.nfc().collect();
    let (doc, blocks, dial) = atrep::parser::parse_document_outline(&normalized, path).ok()?;
    let outline = outline::assemble(&doc, blocks, &normalized, &dial);
    Some(Structure {
        outline,
        dial,
        lines: source.lines().map(str::to_string).collect(),
    })
}

impl Structure {
    fn line(&self, n1: usize) -> &str {
        n1.checked_sub(1)
            .and_then(|i| self.lines.get(i))
            .map(String::as_str)
            .unwrap_or("")
    }

    /// The whole of 1-based line `n1` as a range.
    fn line_range(&self, n1: usize) -> Range {
        let l = n1.saturating_sub(1) as u32;
        Range::new(
            Position::new(l, 0),
            Position::new(l, utf16_len(self.line(n1))),
        )
    }

    /// Lines `start..=end` (1-based, inclusive) as a range.
    fn span_range(&self, start: usize, end: usize) -> Range {
        Range::new(
            Position::new(start.saturating_sub(1) as u32, 0),
            Position::new(end.saturating_sub(1) as u32, utf16_len(self.line(end))),
        )
    }

    /// Range of the first `needle` on 1-based line `n1`, or the
    /// whole line when the spelling is not found there.
    fn locate(&self, n1: usize, needle: &str) -> Range {
        let text = self.line(n1);
        let l = n1.saturating_sub(1) as u32;
        match text.find(needle) {
            Some(byte) => {
                let start = utf16_len(&text[..byte]);
                Range::new(
                    Position::new(l, start),
                    Position::new(l, start + utf16_len(needle)),
                )
            }
            None => self.line_range(n1),
        }
    }

    // ------------------------------------------------ symbols

    /// Hierarchical document symbols: the block tree, with the
    /// milestones and standalone onym anchors inside each block
    /// as leaf children (so a coordinate is one symbol-search
    /// away — the Topos jump).
    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        let mut roots: Vec<DocumentSymbol> = Vec::new();
        // (depth, symbol) stack; a block closes when the next
        // block's depth is not deeper than its own.
        let mut stack: Vec<(usize, DocumentSymbol)> = Vec::new();
        let blocks = &self.outline.blocks;
        // Computed once: it costs blocks x onyms, and every block
        // consults it.
        let standalone = self.standalone_onyms();
        for (i, b) in blocks.iter().enumerate() {
            while let Some((d, _)) = stack.last()
                && *d >= b.depth
            {
                let (_, done) = stack.pop().unwrap();
                attach(&mut stack, &mut roots, done);
            }
            let mut sym = self.block_symbol(b);
            // Point items owned by this block: those on its lines
            // that no deeper block claims.
            let children = self.point_children(b, &blocks[i + 1..], &standalone);
            if !children.is_empty() {
                sym.children = Some(children);
            }
            stack.push((b.depth, sym));
        }
        while let Some((_, done)) = stack.pop() {
            attach(&mut stack, &mut roots, done);
        }
        // Point items outside every block.
        for p in self.free_points() {
            roots.push(p);
        }
        roots.sort_by_key(|s| (s.range.start.line, s.range.start.character));
        roots
    }

    #[allow(deprecated)]
    fn block_symbol(&self, b: &OutlineBlockNamed) -> DocumentSymbol {
        let base = b
            .name
            .clone()
            .or_else(|| b.symbol.clone())
            .unwrap_or_else(|| b.kind.to_string());
        let name = if b.lemma.is_empty() {
            base
        } else {
            format!("{base} {}", b.lemma)
        };
        let mut detail = String::new();
        if let Some(s) = &b.symbol {
            detail.push('@');
            detail.push_str(s);
        }
        for g in &b.genoses {
            detail.push('.');
            detail.push_str(g);
        }
        if let Some(o) = &b.onym {
            detail.push_str(&format!(" ({o})"));
        }
        DocumentSymbol {
            name,
            detail: (!detail.is_empty()).then_some(detail),
            kind: match b.kind {
                "para" => SymbolKind::NAMESPACE,
                "stichoi" => SymbolKind::ARRAY,
                "diaphane" => SymbolKind::PACKAGE,
                _ => SymbolKind::MODULE,
            },
            tags: None,
            deprecated: None,
            range: self.span_range(b.start, b.end),
            selection_range: self.line_range(b.start),
            children: None,
        }
    }

    /// Milestones and standalone anchors on `b`'s lines that fall
    /// inside no deeper block (`rest` = the blocks after `b` in
    /// document order; the nested ones come first; `standalone` =
    /// `standalone_onyms()`, computed by the caller).
    fn point_children(
        &self,
        b: &OutlineBlockNamed,
        rest: &[OutlineBlockNamed],
        standalone: &[(&str, usize)],
    ) -> Vec<DocumentSymbol> {
        let claimed = |line: usize| -> bool {
            rest.iter()
                .take_while(|n| n.start <= b.end)
                .any(|n| n.start <= line && line <= n.end)
        };
        let mut out = Vec::new();
        for m in &self.outline.milestones {
            if m.line >= b.start && m.line <= b.end && !claimed(m.line) {
                out.push(self.point_symbol(&m.key, m.line, SymbolKind::KEY));
            }
        }
        for o in standalone {
            if o.1 >= b.start && o.1 <= b.end && !claimed(o.1) {
                out.push(self.point_symbol(o.0, o.1, SymbolKind::CONSTANT));
            }
        }
        out.sort_by_key(|s| (s.range.start.line, s.range.start.character));
        out
    }

    fn free_points(&self) -> Vec<DocumentSymbol> {
        let inside = |line: usize| {
            self.outline
                .blocks
                .iter()
                .any(|b| b.start <= line && line <= b.end)
        };
        let mut out = Vec::new();
        for m in &self.outline.milestones {
            if m.line > 0 && !inside(m.line) {
                out.push(self.point_symbol(&m.key, m.line, SymbolKind::KEY));
            }
        }
        for (key, line) in self.standalone_onyms() {
            if line > 0 && !inside(line) {
                out.push(self.point_symbol(key, line, SymbolKind::CONSTANT));
            }
        }
        out
    }

    /// Onyms that are not a block's own (those show in the block's
    /// detail instead).
    fn standalone_onyms(&self) -> Vec<(&str, usize)> {
        self.outline
            .onyms
            .iter()
            .filter(|o| {
                o.line > 0
                    && !self
                        .outline
                        .blocks
                        .iter()
                        .any(|b| b.end == o.line && b.onym.as_deref() == Some(o.key.as_str()))
            })
            .map(|o| (o.key.as_str(), o.line))
            .collect()
    }

    #[allow(deprecated)]
    fn point_symbol(&self, key: &str, line: usize, kind: SymbolKind) -> DocumentSymbol {
        let needle = if kind == SymbolKind::KEY {
            format!("\"{key}\"")
        } else {
            format!("({key})")
        };
        let range = self.locate(line, &needle);
        DocumentSymbol {
            name: key.to_string(),
            detail: None,
            kind,
            tags: None,
            deprecated: None,
            range,
            selection_range: range,
            children: None,
        }
    }

    // ------------------------------------------------ folding

    /// One folding range per multi-line block: the opening line
    /// stays visible, the rest folds.
    pub fn folding_ranges(&self) -> Vec<FoldingRange> {
        self.outline
            .blocks
            .iter()
            .filter(|b| b.end > b.start)
            .map(|b| FoldingRange {
                start_line: (b.start - 1) as u32,
                start_character: None,
                end_line: (b.end - 1) as u32,
                end_character: None,
                kind: None,
                collapsed_text: None,
            })
            .collect()
    }

    // ------------------------------------------------ onyms

    /// The onym named in a `(onym)` group under the cursor.
    fn onym_at(&self, pos: Position) -> Option<String> {
        let text = self.line(pos.line as usize + 1);
        let cursor = utf16_to_char(text, pos.character);
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '(' {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != ')' && chars[j] != '(' {
                    j += 1;
                }
                if j < chars.len() && chars[j] == ')' && j > i + 1 && cursor >= i && cursor <= j {
                    let name: String = chars[i + 1..j].iter().collect();
                    if !name.chars().any(char::is_whitespace) {
                        return Some(name);
                    }
                    return None;
                }
            }
            i += 1;
        }
        None
    }

    /// Where the onym under the cursor is declared: the block's
    /// episim line or the standalone anchor.
    pub fn definition(&self, pos: Position) -> Option<Range> {
        let name = self.onym_at(pos)?;
        let decl = self
            .outline
            .onyms
            .iter()
            .find(|o| o.key == name && o.line > 0)?;
        Some(self.locate(decl.line, &format!("({name})")))
    }

    /// Every deixis pointing at the onym under the cursor, plus
    /// its declaration when asked.
    pub fn references(&self, pos: Position, include_declaration: bool) -> Vec<Range> {
        let Some(name) = self.onym_at(pos) else {
            return Vec::new();
        };
        let suffix = format!("({name})");
        let mut out: Vec<Range> = self
            .outline
            .deixes
            .iter()
            .filter(|d| d.line > 0 && d.key.ends_with(&suffix))
            .map(|d| self.locate(d.line, &d.key))
            .collect();
        if include_declaration
            && let Some(decl) = self
                .outline
                .onyms
                .iter()
                .find(|o| o.key == name && o.line > 0)
        {
            out.push(self.locate(decl.line, &suffix));
        }
        out.sort_by_key(|r| (r.start.line, r.start.character));
        out
    }

    // ------------------------------------------------ selection

    /// The enclosing-block chain at a position, innermost first:
    /// the structural text objects (`is`/`as`, and the section
    /// `iS`/`aS` as its parent) in LSP terms.
    pub fn selection_range(&self, pos: Position) -> SelectionRange {
        let line = pos.line as usize + 1;
        let mut enclosing: Vec<&OutlineBlockNamed> = self
            .outline
            .blocks
            .iter()
            .filter(|b| b.start <= line && line <= b.end)
            .collect();
        // Outermost first, so the fold builds parents outward.
        enclosing.sort_by_key(|b| b.depth);
        let mut range = SelectionRange {
            range: self.line_range(line),
            parent: None,
        };
        let mut chain: Vec<Range> = enclosing
            .iter()
            .map(|b| self.span_range(b.start, b.end))
            .collect();
        chain.reverse(); // innermost first
        // Build from the outermost: each wraps the previous.
        let mut parent: Option<Box<SelectionRange>> = None;
        for r in chain.iter().rev() {
            parent = Some(Box::new(SelectionRange { range: *r, parent }));
        }
        range.parent = parent;
        range
    }

    // ------------------------------------------------ hover

    /// The sim under the cursor, described from the dialektos:
    /// name, symbol pair, form, and the definition's descriptions.
    pub fn hover(&self, source: &str, pos: Position) -> Option<Hover> {
        let toks = scan::scan(source, Some(&self.dial));
        let tok = toks.iter().find(|t| {
            t.kind == TokKind::Keyword
                && t.line == pos.line
                && t.start <= pos.character
                && pos.character < t.start + t.len
        })?;
        let text = self.line(pos.line as usize + 1);
        // The token spans the sigil run plus the symbol (and the
        // closing sigil on an episim); the dialektos knows symbols.
        let raw = utf16_slice(text, tok.start, tok.len);
        let symbol = raw.trim_matches(|c| c == '@' || c == '\\').to_string();
        let def = self
            .dial
            .sims
            .get(&symbol)
            .or_else(|| self.dial.sims.values().find(|d| d.episymbol() == symbol))?;
        let form = match &def.form {
            SimForm::Endo => "endo-simmere",
            SimForm::Mono { .. } => "monosim",
            SimForm::Para { .. } => "para-simmere",
        };
        let mut value = format!("**{}** — {form}", def.name);
        match &def.form {
            SimForm::Mono { .. } => value.push_str(&format!("  \n`@{}(…)`", def.symbol)),
            _ => value.push_str(&format!("  \n`@{} … {}@`", def.symbol, def.episymbol())),
        }
        if let Some(s) = &def.short_desc {
            value.push_str("\n\n");
            value.push_str(s);
        }
        if let Some(l) = &def.long_desc {
            value.push_str("\n\n");
            value.push_str(l.trim());
        }
        value.push_str(&format!("\n\n*{}*", self.dial.id));
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(Range::new(
                Position::new(tok.line, tok.start),
                Position::new(tok.line, tok.start + tok.len),
            )),
        })
    }
}

fn attach(
    stack: &mut [(usize, DocumentSymbol)],
    roots: &mut Vec<DocumentSymbol>,
    done: DocumentSymbol,
) {
    match stack.last_mut() {
        Some((_, parent)) => parent.children.get_or_insert_with(Vec::new).push(done),
        None => roots.push(done),
    }
}

fn utf16_len(s: &str) -> u32 {
    s.chars().map(|c| c.len_utf16() as u32).sum()
}

/// Char index for a UTF-16 column (clamped to the line).
fn utf16_to_char(s: &str, col: u32) -> usize {
    let mut acc = 0u32;
    for (i, c) in s.chars().enumerate() {
        if acc >= col {
            return i;
        }
        acc += c.len_utf16() as u32;
    }
    s.chars().count()
}

fn utf16_slice(s: &str, start: u32, len: u32) -> String {
    let a = utf16_to_char(s, start);
    let b = utf16_to_char(s, start + len);
    s.chars().skip(a).take(b.saturating_sub(a)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "@@@!litogramma\n\n\
        @# The Charges\n\
        @(\"steph:17a\")How you have been affected@^!(n1).\n\n\
        @^!\nThe famous opening.\n!^@(n1)\n\n\
        @## First Accusers\nMore prose here.\n##@\n\
        #@\n\n\
        @# The Defence\n@(\"steph:18a\")From the beginning.\n#@\n";

    fn structure() -> Structure {
        analyze(DOC, Path::new("t.atd")).unwrap()
    }

    #[test]
    fn definition_source_has_no_structure() {
        assert!(analyze("@@@!atrep\n\n@=== note\n", Path::new("x.dia")).is_none());
    }

    #[test]
    fn symbols_nest_with_points_inside() {
        let syms = structure().document_symbols();
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["section The Charges", "section The Defence"]);
        let charges = &syms[0];
        assert_eq!(charges.range.start.line, 2);
        assert_eq!(charges.range.end.line, 12);
        assert_eq!(charges.detail.as_deref(), Some("@#"));
        let kids: Vec<(&str, SymbolKind)> = charges
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();
        assert_eq!(
            kids,
            [
                ("steph:17a", SymbolKind::KEY),
                ("manuscript-note", SymbolKind::NAMESPACE),
                ("subsection First Accusers", SymbolKind::NAMESPACE),
            ]
        );
        let note = &charges.children.as_ref().unwrap()[1];
        assert_eq!(note.detail.as_deref(), Some("@^! (n1)"));
        // The milestone symbol selects its literal on line 4.
        let ms = &charges.children.as_ref().unwrap()[0];
        assert_eq!(ms.range.start, Position::new(3, 2));
    }

    #[test]
    fn folding_covers_multiline_blocks() {
        let folds = structure().folding_ranges();
        let spans: Vec<(u32, u32)> = folds.iter().map(|f| (f.start_line, f.end_line)).collect();
        assert_eq!(spans, [(2, 12), (5, 7), (9, 11), (14, 16)]);
    }

    #[test]
    fn deixis_goes_to_its_declaration_and_back() {
        let s = structure();
        // Cursor inside `(n1)` of the deixis on line 4.
        let line4 = DOC.lines().nth(3).unwrap();
        let col = utf16_len(&line4[..line4.find("(n1)").unwrap()]) + 1;
        let def = s.definition(Position::new(3, col)).unwrap();
        assert_eq!(def.start.line, 7);
        // References from the declaration line back to the deixis.
        let refs = s.references(Position::new(7, 5), true);
        let lines: Vec<u32> = refs.iter().map(|r| r.start.line).collect();
        assert_eq!(lines, [3, 7]);
        assert!(s.definition(Position::new(3, 2)).is_none());
    }

    #[test]
    fn selection_chain_walks_outward() {
        let s = structure();
        let sel = s.selection_range(Position::new(10, 3)); // "More prose here."
        assert_eq!(sel.range.start.line, 10);
        let sub = sel.parent.unwrap();
        assert_eq!((sub.range.start.line, sub.range.end.line), (9, 11));
        let sec = sub.parent.unwrap();
        assert_eq!((sec.range.start.line, sec.range.end.line), (2, 12));
        assert!(sec.parent.is_none());
    }

    #[test]
    fn hover_describes_the_sim() {
        let s = structure();
        let h = s.hover(DOC, Position::new(2, 1)).unwrap();
        let HoverContents::Markup(m) = h.contents else {
            panic!("markup expected");
        };
        assert!(
            m.value.starts_with("**section** — para-simmere"),
            "{}",
            m.value
        );
        assert!(m.value.contains("*litogramma*"));
        // The closing episim resolves to the same sim.
        let h2 = s.hover(DOC, Position::new(12, 0)).unwrap();
        let HoverContents::Markup(m2) = h2.contents else {
            panic!()
        };
        assert!(m2.value.starts_with("**section**"));
        assert!(s.hover(DOC, Position::new(3, 20)).is_none());
    }
}
