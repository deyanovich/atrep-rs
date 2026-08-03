//! Document outline: the structural skeleton of a parsed
//! document with source-line spans, for structure-aware editor
//! tooling (litogramma-vim's Pinax/Topos navigation; later the
//! LSP's documentSymbol/foldingRange). Block spans are recorded
//! by the parser during the real parse — the grammar is never
//! re-derived from text. Milestone, onym, and deixis lines are
//! recovered after the parse by locating each dendron node's
//! literal spelling inside the enclosing spans, walking forward
//! in document order.

use crate::dendron::{Block, Document, Inline};
use crate::dialektos::Dialektos;

/// One structural block, in document order (depth-first).
#[derive(Debug, Clone)]
pub struct OutlineBlock {
    /// `para`, `stichoi`, `diaphane`, or `englossis`.
    pub kind: &'static str,
    /// The sim symbol (`None` for the core stichoi form and
    /// the transparent wrappers).
    pub symbol: Option<String>,
    /// Nesting depth, 0 = top level.
    pub depth: usize,
    /// 1-based source lines, inclusive.
    pub start: usize,
    pub end: usize,
    /// Plain text of the lemma (empty when absent).
    pub lemma: String,
    pub onym: Option<String>,
    pub genoses: Vec<String>,
}

/// A located point item.
#[derive(Debug, Clone)]
pub struct OutlinePoint {
    /// Milestone: `scheme:value`. Onym anchor / block onym: the
    /// onym. Deixis: `symbol(onym)`.
    pub key: String,
    /// 1-based source line (0 when the literal was not found —
    /// possible only for text shadowed inside verbatim regions).
    pub line: usize,
}

/// The assembled outline.
#[derive(Debug, Clone, Default)]
pub struct Outline {
    pub dialektos: String,
    pub blocks: Vec<OutlineBlockNamed>,
    pub milestones: Vec<OutlinePoint>,
    pub onyms: Vec<OutlinePoint>,
    pub deixes: Vec<OutlinePoint>,
}

/// [`OutlineBlock`] with the sim's definition name resolved.
#[derive(Debug, Clone)]
pub struct OutlineBlockNamed {
    pub kind: &'static str,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub depth: usize,
    pub start: usize,
    pub end: usize,
    pub lemma: String,
    pub onym: Option<String>,
    pub genoses: Vec<String>,
}

/// Plain text of an inline run (markup stripped, endo content
/// retained).
pub(crate) fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for i in inlines {
        match i {
            Inline::Text(t) => out.push_str(t),
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                out.push_str(&inline_text(content));
            }
            Inline::VerbatimInline { content, .. } => out.push_str(content),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Assemble the outline: name the recorded blocks against the
/// dialektos and locate milestones, onyms, and deixes.
pub fn assemble(
    doc: &Document,
    blocks: Vec<OutlineBlock>,
    source: &str,
    dial: &Dialektos,
) -> Outline {
    let named = blocks
        .into_iter()
        .map(|b| OutlineBlockNamed {
            name: b
                .symbol
                .as_deref()
                .and_then(|s| dial.sims.get(s))
                .map(|d| d.name.clone()),
            kind: b.kind,
            symbol: b.symbol,
            depth: b.depth,
            start: b.start,
            end: b.end,
            lemma: b.lemma,
            onym: b.onym,
            genoses: b.genoses,
        })
        .collect::<Vec<_>>();
    // Children are recorded before their parents (completion
    // order); the outline is document order.
    let mut named = named;
    named.sort_by(|a: &OutlineBlockNamed, b: &OutlineBlockNamed| {
        a.start.cmp(&b.start).then(b.end.cmp(&a.end))
    });

    // Locate point items: walk the dendron in document order and
    // find each item's literal spelling, advancing a line cursor
    // so repeated spellings resolve in order. The document may
    // use the alias sigil; both spellings are searched.
    let lines: Vec<&str> = source.lines().collect();
    let mut cursor = 0usize; // 0-based line to search from
    let mut locate = |needles: &[String]| -> usize {
        for (off, l) in lines[cursor.min(lines.len())..].iter().enumerate() {
            if needles.iter().any(|n| l.contains(n.as_str())) {
                let found = cursor + off;
                cursor = found; // items may share a line
                return found + 1;
            }
        }
        0
    };
    let mut milestones = Vec::new();
    let mut onyms = Vec::new();
    let mut deixes = Vec::new();
    // Block onyms come from the recorded spans (the episim line
    // carries the declaration); only standalone anchors need
    // textual location.
    for b in &named {
        if let Some(o) = &b.onym {
            onyms.push(OutlinePoint {
                key: o.clone(),
                line: b.end,
            });
        }
    }
    walk_blocks(&doc.blocks, &mut |item| match item {
        Item::Milestone(scheme, value) => {
            let line = locate(&[
                format!("@(\"{scheme}:{value}\")"),
                format!("\\(\"{scheme}:{value}\")"),
            ]);
            milestones.push(OutlinePoint {
                key: format!("{scheme}:{value}"),
                line,
            });
        }
        Item::Onym(o) => {
            let line = locate(&[format!("({o})")]);
            onyms.push(OutlinePoint {
                key: o.to_string(),
                line,
            });
        }
        Item::Deixis(sym, o) => {
            let line = locate(&[format!("{sym}({o})"), format!("{{{sym}}}({o})")]);
            deixes.push(OutlinePoint {
                key: format!("{sym}({o})"),
                line,
            });
        }
    });
    Outline {
        dialektos: doc.dialect_id.clone(),
        blocks: named,
        milestones,
        onyms,
        deixes,
    }
}

enum Item<'a> {
    Milestone(&'a str, &'a str),
    Onym(&'a str),
    Deixis(&'a str, &'a str),
}

fn walk_blocks<'a>(blocks: &'a [Block], f: &mut impl FnMut(Item<'a>)) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => walk_inlines(inlines, f),
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                walk_inlines(lemma, f);
                walk_blocks(children, f);
                walk_inlines(hypograph, f);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                walk_inlines(lemma, f);
                for strophe in strophes {
                    for line in &strophe.0 {
                        walk_inlines(line, f);
                    }
                }
                walk_inlines(hypograph, f);
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                walk_blocks(children, f);
            }
            _ => {}
        }
    }
}

fn walk_inlines<'a>(inlines: &'a [Inline], f: &mut impl FnMut(Item<'a>)) {
    for inline in inlines {
        match inline {
            Inline::Milestone { scheme, value, .. } => f(Item::Milestone(scheme, value)),
            Inline::OnymAnchor(o) => f(Item::Onym(o)),
            Inline::Deixis { symbol, onym, .. } => f(Item::Deixis(symbol, onym)),
            Inline::Endo { content, ann, .. } => {
                walk_inlines(content, f);
                if let Some(o) = &ann.onym {
                    f(Item::Onym(o));
                }
            }
            Inline::EndoDiaphane { content, .. } => walk_inlines(content, f),
            _ => {}
        }
    }
}
