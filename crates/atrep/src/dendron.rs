//! The Dendron: Atrep's AST (spec: chapter "Dendron"), plus the
//! canonical (kanon) serializer.
//!
//! The tree is sigil-agnostic; the serializer always emits the
//! canonical sigil `@`, with literal `@` escaped as `\@` and a
//! literal `|` escaped as `\|` when it directly follows a sim
//! boundary (spec: Sigil Escaping; Genos / pipe separator).

/// Suffix annotations shared by simmeres.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Annotations {
    pub onym: Option<String>,
    pub genoses: Vec<String>,
}

impl Annotations {
    pub fn is_empty(&self) -> bool {
        self.onym.is_none() && self.genoses.is_empty()
    }
}

/// Taxis: ordinal of a simmere among ordered siblings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taxis {
    /// `(...)` left blank: autonumbered, resolved by kanonizo.
    Auto,
    Explicit(u64),
}

/// Inline (endo-context) content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    /// Dialektos-defined endo-simmere.
    Endo {
        symbol: String,
        content: Vec<Inline>,
        /// Whether the sim's episymbol flips brackets (from the
        /// dialektos definition; needed to re-emit the episim).
        bracket_matching: bool,
        ann: Annotations,
    },
    /// Lexema-enlexis `@@"..."@@` (verbatim inline).
    VerbatimInline {
        content: String,
        ann: Annotations,
    },
    /// Endo-diaphane `@@. ... .@@` (transparent inline wrapper).
    EndoDiaphane {
        content: Vec<Inline>,
        ann: Annotations,
    },
    /// Dialektos-defined monosim `@<sym>(<param>)`.
    Monosim {
        symbol: String,
        param: String,
        ann: Annotations,
    },
    /// Standalone onym anchor `@(<ref>)`.
    OnymAnchor(String),
    /// Milestone `@("scheme:value")`: a global anchor in an
    /// external reference scheme, kanonizo-exempt and
    /// litosis-surviving (spec v0.12).
    Milestone {
        scheme: String,
        value: String,
        ann: Annotations,
    },
    /// Deixis `@<sym>(<onym>)`: a core pointing reference to the
    /// onymized para-simmere of the given sim.
    Deixis {
        symbol: String,
        onym: String,
        ann: Annotations,
    },
    /// Endo-axioma definition `@@: ... :@@(<ref>)` (pre-kanonizo).
    EndoAxioma {
        onym: String,
        content: Vec<Inline>,
    },
    /// Axioma reference `@@(:r:)` / `@@+(:r:)` (pre-kanonizo).
    AxiomaRef {
        onym: String,
        enlexis: bool,
    },
}

/// A strophe: a stanza of stichos lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strophe(pub Vec<Vec<Inline>>);

/// Block (para-context) content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Implicit para-simmere: a plain paragraph.
    Paragraph(Vec<Inline>),
    /// Dialektos-defined para-simmere.
    Para {
        symbol: String,
        taxis: Option<Taxis>,
        lemma: Vec<Inline>,
        children: Vec<Block>,
        hypograph: Vec<Inline>,
        /// Whether the sim's episymbol flips brackets (from the
        /// dialektos definition; needed to re-emit the episim).
        bracket_matching: bool,
        ann: Annotations,
    },
    /// A line-structured para-simmere: the core stichoi form
    /// `@@@= ... =@@@` (`symbol: None`) or a dialektos-defined
    /// stichoi sim (`symbol: Some`, declared with the `stichos`
    /// ostensive keyword).
    Stichoi {
        symbol: Option<String>,
        taxis: Option<Taxis>,
        lemma: Vec<Inline>,
        strophes: Vec<Strophe>,
        hypograph: Vec<Inline>,
        /// See [`Block::Para::bracket_matching`]; unused for the
        /// core form.
        bracket_matching: bool,
        ann: Annotations,
    },
    /// Para-diaphane `@@@. ... .@@@` (transparent block wrapper).
    ParaDiaphane {
        children: Vec<Block>,
        ann: Annotations,
    },
    /// Monad-enlexis `@@@" ... "@@@` (verbatim block).
    VerbatimBlock { content: String, ann: Annotations },
    /// Monad-englossis `@@@!(<dialect>) ... !@@@` (semantic block).
    MonadEnglossis {
        dialect: String,
        children: Vec<Block>,
        ann: Annotations,
    },
    /// Enmedia `@@@@(<file>)` (media inclusion).
    Enmedia { param: String },
    /// Enmedia after litosis: `@@@@[SHA256:<hex>]`.
    EnmediaHashed { sha256: String },
    /// Anaphor-englossis `@@@(<file>)` (pre-kanonizo).
    AnaphorEnglossis { target: String },
    /// Anaphor-enlexis `@@@+(<file>)` (pre-kanonizo).
    AnaphorEnlexis { target: String },
    /// Para-axioma definition `@@@: ... :@@@(<ref>)` (pre-kanonizo).
    ParaAxioma { onym: String, children: Vec<Block> },
    /// Block axioma reference `@@@(:r:)` / `@@@+(:r:)` (pre-kanonizo).
    AxiomaRefBlock { onym: String, enlexis: bool },
}

/// A parsed Atrep document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub dialect_id: String,
    /// Version exactly as declared (e.g. `2.1`), if declared.
    pub dialect_version: Option<String>,
    pub blocks: Vec<Block>,
}

// ---------------------------------------------------------------
// Kanon serialization
// ---------------------------------------------------------------

/// Serialize an inline fragment in canonical form (used by
/// axioma-enlexis expansion, which wraps content verbatim).
pub fn serialize_inline_fragment(inlines: &[Inline]) -> String {
    let mut out = String::new();
    push_inlines(inlines, &mut out, false);
    out
}

/// Serialize a block fragment in canonical form.
pub fn serialize_block_fragment(blocks: &[Block]) -> String {
    let mut out = String::new();
    serialize_children(blocks, &mut out);
    out
}

/// Serialize a document in canonical form.
///
/// Paragraph and inline whitespace is assumed already normalized
/// (single spaces) by the parser/kanonizo; this function is purely
/// structural.
pub fn serialize(doc: &Document) -> String {
    let mut out = String::new();
    out.push_str("@@@!");
    out.push_str(&doc.dialect_id);
    if let Some(v) = &doc.dialect_version {
        out.push('@');
        out.push_str(v);
    }
    out.push('\n');
    if !doc.blocks.is_empty() {
        out.push('\n');
        serialize_children(&doc.blocks, &mut out);
    }
    out
}

/// Whether a block serializes as a single self-delimiting line that
/// needs no blank-line separation from its siblings.
fn is_compact(block: &Block) -> bool {
    matches!(
        block,
        Block::Enmedia { .. }
            | Block::EnmediaHashed { .. }
            | Block::AnaphorEnglossis { .. }
            | Block::AnaphorEnlexis { .. }
            | Block::AxiomaRefBlock { .. }
    )
}

fn serialize_block(block: &Block, out: &mut String) {
    match block {
        Block::Paragraph(inlines) => {
            push_inlines(inlines, out, false);
            out.push('\n');
        }
        Block::Para {
            symbol,
            taxis,
            lemma,
            children,
            hypograph,
            bracket_matching,
            ann,
        } => {
            out.push('@');
            out.push_str(symbol);
            if let Some(t) = taxis {
                match t {
                    Taxis::Auto => out.push_str("()"),
                    Taxis::Explicit(n) => out.push_str(&format!("({n})")),
                }
            }
            if !lemma.is_empty() {
                out.push(' ');
                push_inlines(lemma, out, false);
            }
            out.push('\n');
            serialize_children(children, out);
            out.push_str(&crate::sigil::episymbol(symbol, *bracket_matching));
            out.push('@');
            push_annotations(ann, out);
            if !hypograph.is_empty() {
                out.push(' ');
                push_inlines(hypograph, out, true);
            }
            out.push('\n');
        }
        Block::Stichoi {
            symbol,
            taxis,
            lemma,
            strophes,
            hypograph,
            bracket_matching,
            ann,
        } => {
            match symbol {
                Some(sym) => {
                    out.push('@');
                    out.push_str(sym);
                    if let Some(t) = taxis {
                        match t {
                            Taxis::Auto => out.push_str("()"),
                            Taxis::Explicit(n) => out.push_str(&format!("({n})")),
                        }
                    }
                }
                None => out.push_str("@@@="),
            }
            if !lemma.is_empty() {
                out.push(' ');
                push_inlines(lemma, out, false);
            }
            out.push('\n');
            for (i, strophe) in strophes.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                for line in &strophe.0 {
                    push_inlines(line, out, false);
                    out.push('\n');
                }
            }
            match symbol {
                Some(sym) => {
                    out.push_str(&crate::sigil::episymbol(sym, *bracket_matching));
                    out.push('@');
                }
                None => out.push_str("=@@@"),
            }
            push_annotations(ann, out);
            if !hypograph.is_empty() {
                out.push(' ');
                push_inlines(hypograph, out, true);
            }
            out.push('\n');
        }
        Block::ParaDiaphane { children, ann } => {
            out.push_str("@@@.\n");
            serialize_children(children, out);
            out.push_str(".@@@");
            push_annotations(ann, out);
            out.push('\n');
        }
        Block::VerbatimBlock { content, ann } => {
            out.push_str("@@@\"\n");
            out.push_str(content);
            if !content.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("\"@@@");
            push_annotations(ann, out);
            out.push('\n');
        }
        Block::MonadEnglossis {
            dialect,
            children,
            ann,
        } => {
            out.push_str("@@@!(");
            out.push_str(dialect);
            out.push_str(")\n");
            serialize_children(children, out);
            out.push_str("!@@@");
            push_annotations(ann, out);
            out.push('\n');
        }
        Block::Enmedia { param } => {
            out.push_str("@@@@(");
            out.push_str(param);
            out.push_str(")\n");
        }
        Block::EnmediaHashed { sha256 } => {
            out.push_str("@@@@[SHA256:");
            out.push_str(sha256);
            out.push_str("]\n");
        }
        Block::AnaphorEnglossis { target } => {
            out.push_str("@@@(");
            out.push_str(target);
            out.push_str(")\n");
        }
        Block::AnaphorEnlexis { target } => {
            out.push_str("@@@+(");
            out.push_str(target);
            out.push_str(")\n");
        }
        Block::ParaAxioma { onym, children } => {
            out.push_str("@@@:\n");
            serialize_children(children, out);
            out.push_str(":@@@(");
            out.push_str(onym);
            out.push_str(")\n");
        }
        Block::AxiomaRefBlock { onym, enlexis } => {
            if *enlexis {
                out.push_str("@@@+(:");
            } else {
                out.push_str("@@@(:");
            }
            out.push_str(onym);
            out.push_str(":)\n");
        }
    }
}

/// Serialize block children with blank lines between siblings,
/// except around compact single-line blocks. The spec does not pin
/// down the kanon's blank-line layout beyond "removes extraneous
/// blank lines"; this rule reproduces the spec's worked-example
/// appendix and is deterministic.
fn serialize_children(children: &[Block], out: &mut String) {
    for (i, child) in children.iter().enumerate() {
        if i > 0 && !is_compact(child) && !is_compact(&children[i - 1]) {
            out.push('\n');
        }
        serialize_block(child, out);
    }
}

fn push_annotations(ann: &Annotations, out: &mut String) {
    if let Some(onym) = &ann.onym {
        out.push('(');
        out.push_str(onym);
        out.push(')');
    }
    for genos in &ann.genoses {
        out.push('.');
        out.push_str(genos);
    }
}

/// Emit inline content. `after_boundary` is true when the emitted
/// text directly follows a sim boundary (an episim, or the start of
/// a hypograph after an episim), where a leading literal `|` would
/// read as a separator, and a leading `.`/`(` would re-parse as a
/// genos/onym suffix.
fn push_inlines(inlines: &[Inline], out: &mut String, after_boundary: bool) {
    push_inlines_in(inlines, out, after_boundary, None);
}

fn push_inlines_in(
    inlines: &[Inline],
    out: &mut String,
    mut after_boundary: bool,
    terminator: Option<&str>,
) {
    let mut after_genos = false;
    for (i, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::Text(t) => {
                // Text opening with a genos-continue char directly
                // after an emitted genos would extend it: separate.
                if after_genos
                    && t.chars()
                        .next()
                        .is_some_and(crate::sigil::is_genos_continue)
                {
                    out.push('|');
                }
                // A trailing literal `|` directly before a sim would
                // be consumed as an ambiguity separator: escape it.
                let next_is_sim = inlines
                    .get(i + 1)
                    .is_some_and(|n| !matches!(n, Inline::Text(_)));
                push_escaped_text(t, out, after_boundary, next_is_sim);
                // Text ending with the enclosing episymbol directly
                // before a sim would fuse into a premature close
                // token; the separator keeps the tail literal.
                if next_is_sim && terminator.is_some_and(|term| out.ends_with(term)) {
                    out.push('|');
                }
            }
            Inline::Endo {
                symbol,
                content,
                bracket_matching,
                ann,
            } => {
                out.push('@');
                out.push_str(symbol);
                let epi = crate::sigil::episymbol(symbol, *bracket_matching);
                push_inlines_in(content, out, false, Some(&epi));
                out.push_str(&epi);
                out.push('@');
                push_annotations(ann, out);
            }
            Inline::VerbatimInline { content, ann } => {
                out.push_str("@@\"");
                out.push_str(content);
                out.push_str("\"@@");
                push_annotations(ann, out);
            }
            Inline::EndoDiaphane { content, ann } => {
                out.push_str("@@.");
                push_inlines(content, out, false);
                out.push_str(".@@");
                push_annotations(ann, out);
            }
            Inline::Monosim { symbol, param, ann } => {
                out.push('@');
                out.push_str(symbol);
                out.push('(');
                out.push_str(param);
                out.push(')');
                push_annotations(ann, out);
            }
            Inline::OnymAnchor(onym) => {
                out.push_str("@(");
                out.push_str(onym);
                out.push(')');
            }
            Inline::Milestone { scheme, value, ann } => {
                out.push_str("@(\"");
                out.push_str(scheme);
                out.push(':');
                out.push_str(value);
                out.push_str("\")");
                push_annotations(ann, out);
            }
            Inline::Deixis { symbol, onym, ann } => {
                out.push('@');
                out.push_str(symbol);
                out.push('(');
                out.push_str(onym);
                out.push(')');
                push_annotations(ann, out);
            }
            Inline::EndoAxioma { onym, content } => {
                out.push_str("@@:");
                push_inlines(content, out, false);
                out.push_str(":@@(");
                out.push_str(onym);
                out.push(')');
            }
            Inline::AxiomaRef { onym, enlexis } => {
                if *enlexis {
                    out.push_str("@@+(:");
                } else {
                    out.push_str("@@(:");
                }
                out.push_str(onym);
                out.push_str(":)");
            }
        }
        // After any non-text inline that ends in a sim boundary, a
        // following literal pipe must be escaped.
        after_boundary = !matches!(inline, Inline::Text(_));
        after_genos = match inline {
            Inline::Endo { ann, .. }
            | Inline::Monosim { ann, .. }
            | Inline::Deixis { ann, .. }
            | Inline::VerbatimInline { ann, .. }
            | Inline::EndoDiaphane { ann, .. }
            | Inline::Milestone { ann, .. } => !ann.genoses.is_empty(),
            _ => false,
        };
    }
}

/// Escape text for canonical output: `@` becomes `\@`. Directly
/// after a sim boundary, a leading `|` becomes `\|`, and a leading
/// `.`/`(` that would re-parse as a genos/onym suffix is preceded by
/// the `|` separator. A trailing `|` directly before a sim is
/// escaped so it is not consumed as a separator. A literal `\` needs
/// no escaping (any following `@` is itself emitted escaped, so the
/// sequence stays unambiguous).
fn push_escaped_text(text: &str, out: &mut String, after_boundary: bool, next_is_sim: bool) {
    if after_boundary {
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some('.'), Some(c)) if crate::sigil::is_genos_start(c) => out.push('|'),
            (Some('('), _) => out.push('|'),
            _ => {}
        }
    }
    let n = text.chars().count();
    for (i, c) in text.chars().enumerate() {
        match c {
            '@' => out.push_str("\\@"),
            '|' if i == 0 && after_boundary => out.push_str("\\|"),
            '|' if i + 1 == n && next_is_sim => out.push_str("\\|"),
            _ => out.push(c),
        }
    }
}

/// Serialize in the plerographic spelling (spec: "Metagraphe"):
/// dialektos-defined sims spell as `{name}` references, core
/// forms keep their fixed sigils, and the two spellings parse to
/// the identical document. Errors when a used sim's name is
/// duplicated in the dialektos (duplicate names are legal but
/// not plerographically addressable) or its symbol is undefined.
pub fn serialize_plerographic(
    doc: &Document,
    dial: &crate::dialektos::Dialektos,
) -> crate::error::Result<String> {
    serialize_plerographic_in(doc, dial, None)
}

/// [`serialize_plerographic`] in a language: sims spell by their
/// names in the requested glossa, falling back to the primary
/// name where the glossa is silent (spec: "Glossae").
pub fn serialize_plerographic_in(
    doc: &Document,
    dial: &crate::dialektos::Dialektos,
    lang: Option<&str>,
) -> crate::error::Result<String> {
    use crate::error::{Error, ErrorKind};
    if let Some(lang) = lang
        && !dial.glossae.contains_key(lang)
    {
        return Err(Error::new(ErrorKind::UnresolvableDialektos(format!(
            "{}.{lang}.glossa",
            dial.id
        ))));
    }
    let mut doc = doc.clone();
    plero_blocks(&mut doc.blocks, dial, lang)?;
    Ok(serialize(&doc))
}

fn plero_symbol(
    symbol: &mut String,
    dial: &crate::dialektos::Dialektos,
    lang: Option<&str>,
) -> crate::error::Result<()> {
    use crate::error::{Error, ErrorKind};
    let Some(def) = dial.sims.get(symbol.as_str()) else {
        return Err(Error::new(ErrorKind::UndefinedSim(symbol.clone())));
    };
    let name = lang
        .and_then(|l| dial.glossae.get(l))
        .and_then(|names| names.get(symbol.as_str()))
        .map(String::as_str)
        .unwrap_or(&def.name);
    // The chosen name must fold back to this symbol uniquely
    // across the whole plerographic address space, or the
    // respelling would not round-trip.
    match dial.sim_named(name) {
        crate::dialektos::NamedLookup::One(d) if d.symbol == *symbol => {
            *symbol = format!("{{{name}}}");
            Ok(())
        }
        _ => Err(Error::new(ErrorKind::Syntax(format!(
            "sim name `{name}` is not unambiguously addressable in `{}`",
            dial.id
        )))),
    }
}

fn plero_blocks(
    blocks: &mut [Block],
    dial: &crate::dialektos::Dialektos,
    lang: Option<&str>,
) -> crate::error::Result<()> {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => plero_inlines(inlines, dial, lang)?,
            Block::Para {
                symbol,
                lemma,
                children,
                hypograph,
                ..
            } => {
                plero_symbol(symbol, dial, lang)?;
                plero_inlines(lemma, dial, lang)?;
                plero_blocks(children, dial, lang)?;
                plero_inlines(hypograph, dial, lang)?;
            }
            Block::Stichoi {
                symbol,
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                if let Some(symbol) = symbol {
                    plero_symbol(symbol, dial, lang)?;
                }
                plero_inlines(lemma, dial, lang)?;
                for strophe in strophes {
                    for line in &mut strophe.0 {
                        plero_inlines(line, dial, lang)?;
                    }
                }
                plero_inlines(hypograph, dial, lang)?;
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                plero_blocks(children, dial, lang)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn plero_inlines(
    inlines: &mut [Inline],
    dial: &crate::dialektos::Dialektos,
    lang: Option<&str>,
) -> crate::error::Result<()> {
    for inline in inlines {
        match inline {
            Inline::Endo {
                symbol, content, ..
            } => {
                plero_symbol(symbol, dial, lang)?;
                plero_inlines(content, dial, lang)?;
            }
            Inline::Monosim { symbol, .. } | Inline::Deixis { symbol, .. } => {
                plero_symbol(symbol, dial, lang)?;
            }
            Inline::EndoDiaphane { content, .. } => plero_inlines(content, dial, lang)?,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guards_episymbol_fusion() {
        // Text ending with the enclosing episymbol directly
        // before a nested sim would serialize as a premature
        // close token; the ambiguity separator keeps it literal.
        let doc = Document {
            dialect_id: "d".into(),
            dialect_version: None,
            blocks: vec![Block::Paragraph(vec![Inline::Endo {
                symbol: ",".into(),
                content: vec![
                    Inline::Text("down to the grave,".into()),
                    Inline::Endo {
                        symbol: "^".into(),
                        content: vec![Inline::Text("Gr. Hades.".into())],
                        bracket_matching: true,
                        ann: Annotations::default(),
                    },
                ],
                bracket_matching: true,
                ann: Annotations::default(),
            }])],
        };
        assert_eq!(
            serialize(&doc),
            "@@@!d

@,down to the grave,|@^Gr. Hades.^@,@
"
        );
    }

    #[test]
    fn guards_genos_fusion() {
        // Text opening with a genos-continue char directly after
        // an emitted genos would extend the genos; the separator
        // keeps it text ("even" + "to" from the KJV).
        let doc = Document {
            dialect_id: "d".into(),
            dialect_version: None,
            blocks: vec![Block::Paragraph(vec![
                Inline::Endo {
                    symbol: ",".into(),
                    content: vec![Inline::Text("even".into())],
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec!["add".into()],
                    },
                },
                Inline::Text("to the mercy seatward".into()),
            ])],
        };
        assert_eq!(
            serialize(&doc),
            "@@@!d

@,even,@.add|to the mercy seatward
"
        );
    }

    #[test]
    fn escapes_at_sign() {
        let doc = Document {
            dialect_id: "d".into(),
            dialect_version: None,
            blocks: vec![Block::Paragraph(vec![Inline::Text(
                "mail me@example.com".into(),
            )])],
        };
        assert_eq!(serialize(&doc), "@@@!d\n\nmail me\\@example.com\n");
    }

    #[test]
    fn escapes_boundary_pipe() {
        let doc = Document {
            dialect_id: "d".into(),
            dialect_version: None,
            blocks: vec![Block::Paragraph(vec![
                Inline::Endo {
                    symbol: "/".into(),
                    content: vec![Inline::Text("www".into())],
                    bracket_matching: true,
                    ann: Annotations::default(),
                },
                Inline::Text("|.example.com".into()),
            ])],
        };
        assert_eq!(serialize(&doc), "@@@!d\n\n@/www/@\\|.example.com\n");
    }
}
