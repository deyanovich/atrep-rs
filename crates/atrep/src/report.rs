//! Reports: findings about a valid document that are worth a
//! maintainer's attention but are not errors — `atrep check`
//! prints them as warnings after its OK.

use crate::dendron::{Block, Document, Inline};
use crate::dialektos::{Dialektos, SimForm};

/// A label (a usage or grammar label) with no item in the
/// document's abbreviations list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedLabel {
    /// The label as printed.
    pub label: String,
    /// The headword of the entry it sits in, when inside one.
    pub entry: Option<String>,
}

/// The labels of a dictionary that its abbreviations list does
/// not resolve. The list is the sim named `abbreviations` (the
/// wrapper of a koine definition list; an item's lemma is the
/// abbreviation, several spellings comma-joined), the labels are
/// the sims named `usage-label` and `grammar-label` in the
/// document's dialektos. Without such a list there is nothing to
/// resolve against and the report is empty.
pub fn unresolved_labels(doc: &Document, dial: &Dialektos) -> Vec<UnresolvedLabel> {
    let symbol_of = |name: &str| {
        dial.sims
            .values()
            .find(|d| d.name == name)
            .map(|d| d.symbol.clone())
    };
    let (Some(list_symbol), Some(item_symbol)) =
        (symbol_of("abbreviations"), symbol_of("definition-item"))
    else {
        return Vec::new();
    };
    let label_symbols: Vec<String> = ["usage-label", "grammar-label"]
        .iter()
        .filter_map(|n| symbol_of(n))
        .collect();
    if label_symbols.is_empty() {
        return Vec::new();
    }
    let autonyms: Vec<String> = dial
        .sims
        .values()
        .filter(|d| matches!(d.form, SimForm::Para { autonym: true, .. }))
        .map(|d| d.symbol.clone())
        .collect();

    // The abbreviations: every spelling of every item under the
    // wrapper.
    let mut known: Vec<String> = Vec::new();
    fn items(blocks: &[Block], item_symbol: &str, known: &mut Vec<String>) {
        for block in blocks {
            if let Block::Para {
                symbol,
                lemma,
                children,
                ..
            } = block
            {
                if *symbol == item_symbol {
                    for spelling in plain(lemma).split(',') {
                        let s = spelling.trim();
                        if !s.is_empty() {
                            known.push(s.to_string());
                        }
                    }
                }
                items(children, item_symbol, known);
            }
        }
    }
    fn collect(blocks: &[Block], list_symbol: &str, item_symbol: &str, known: &mut Vec<String>) {
        for block in blocks {
            match block {
                Block::Para {
                    symbol, children, ..
                } if *symbol == list_symbol => items(children, item_symbol, known),
                Block::Para { children, .. } | Block::ParaDiaphane { children, .. } => {
                    collect(children, list_symbol, item_symbol, known)
                }
                _ => {}
            }
        }
    }
    collect(&doc.blocks, &list_symbol, &item_symbol, &mut known);
    if known.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<UnresolvedLabel> = Vec::new();
    fn walk_inlines(
        inlines: &[Inline],
        label_symbols: &[String],
        known: &[String],
        entry: Option<&str>,
        out: &mut Vec<UnresolvedLabel>,
    ) {
        for inline in inlines {
            match inline {
                Inline::Endo {
                    symbol, content, ..
                } => {
                    if label_symbols.contains(symbol) {
                        let label = plain(content).trim().to_string();
                        let resolved = known.iter().any(|k| k == &label);
                        if !label.is_empty()
                            && !resolved
                            && !out
                                .iter()
                                .any(|u| u.label == label && u.entry.as_deref() == entry)
                        {
                            out.push(UnresolvedLabel {
                                label,
                                entry: entry.map(str::to_string),
                            });
                        }
                    }
                    walk_inlines(content, label_symbols, known, entry, out);
                }
                Inline::EndoDiaphane { content, .. } | Inline::EndoAxioma { content, .. } => {
                    walk_inlines(content, label_symbols, known, entry, out)
                }
                _ => {}
            }
        }
    }
    fn walk_blocks(
        blocks: &[Block],
        label_symbols: &[String],
        autonyms: &[String],
        known: &[String],
        entry: Option<&str>,
        out: &mut Vec<UnresolvedLabel>,
    ) {
        for block in blocks {
            match block {
                Block::Paragraph(inlines) => {
                    walk_inlines(inlines, label_symbols, known, entry, out)
                }
                Block::Para {
                    symbol,
                    lemma,
                    children,
                    hypograph,
                    ..
                } => {
                    let headword;
                    let entry = if autonyms.contains(symbol) {
                        headword = plain(lemma).trim().to_string();
                        Some(headword.as_str())
                    } else {
                        entry
                    };
                    walk_inlines(lemma, label_symbols, known, entry, out);
                    walk_blocks(children, label_symbols, autonyms, known, entry, out);
                    walk_inlines(hypograph, label_symbols, known, entry, out);
                }
                Block::Stichoi {
                    lemma,
                    strophes,
                    hypograph,
                    ..
                } => {
                    walk_inlines(lemma, label_symbols, known, entry, out);
                    for strophe in strophes {
                        for line in &strophe.0 {
                            walk_inlines(line, label_symbols, known, entry, out);
                        }
                    }
                    walk_inlines(hypograph, label_symbols, known, entry, out);
                }
                Block::ParaDiaphane { children, .. } => {
                    walk_blocks(children, label_symbols, autonyms, known, entry, out)
                }
                _ => {}
            }
        }
    }
    walk_blocks(
        &doc.blocks,
        &label_symbols,
        &autonyms,
        &known,
        None,
        &mut out,
    );
    out
}

fn plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for i in inlines {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Endo { content, .. }
            | Inline::EndoDiaphane { content, .. }
            | Inline::EndoAxioma { content, .. } => s.push_str(&plain(content)),
            Inline::VerbatimInline { content, .. } => s.push_str(content),
            _ => {}
        }
    }
    s
}
