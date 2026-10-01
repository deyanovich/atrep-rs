//! at-epimerismos: token-level parsing as the corpora carry it.
//!
//! A token is an endo-diaphane whose first inline is a pack
//! monosim (`@@.@!=(школа)@!/(S)@!%(f,inan=sg,nom)Школа.@@`); a
//! sentence is an onymized endo-diaphane holding tokens and the
//! untokenized text between them. This module holds the model
//! (`Corpus`), its extraction from a document and its embedding
//! into one, and the importers and exporters for the three
//! corpus formats whose output the template language cannot
//! produce: the Russian National Corpus XML, OpenCorpora XML and
//! PROIEL XML. Identity is untouched either way: litosis unwraps
//! the diaphanes and strips the monosims.

use crate::dendron::{Annotations, Block, Document, Inline, Strophe};
use crate::endo::{Tok, attr, collapse_ws, decode_entities, skip_element, tokenize_xml};
use crate::error::Result;

pub const PERIODOS: &str = "!.";
pub const LEXEMA: &str = "!=";
pub const MEROS: &str = "!/";
pub const PAREPOMENA: &str = "!%";
pub const SEMASIA: &str = "!&";
pub const KEPHALE: &str = "!>";
pub const SCHESIS: &str = "!-";

/// The token-level sims (periodos opens a sentence, not a token).
const PACK: &[&str] = &[LEXEMA, MEROS, PAREPOMENA, SEMASIA, KEPHALE, SCHESIS];

// ---------------------------------------------------------------
// Model
// ---------------------------------------------------------------

/// One parsing of a token (a token may carry several).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsing {
    pub lexema: Option<String>,
    pub meros: Option<String>,
    pub parepomena: Option<String>,
    pub semasia: Option<String>,
    pub kephale: Option<String>,
    pub schesis: Option<String>,
}

impl Parsing {
    fn is_empty(&self) -> bool {
        self == &Parsing::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub form: String,
    pub parsings: Vec<Parsing>,
    pub onym: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Token(Token),
    Text(String),
    /// A citation coordinate changing inside the sentence.
    Milestone {
        scheme: String,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sentence {
    /// The source's sentence id, or a running number.
    pub id: Option<String>,
    pub pieces: Vec<Piece>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unit {
    Sentence(Sentence),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusBlock {
    /// A heading of the given depth (1 = outermost).
    Heading {
        depth: usize,
        text: String,
    },
    Paragraph(Vec<Unit>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Corpus {
    pub title: Option<String>,
    pub author: Option<String>,
    pub blocks: Vec<CorpusBlock>,
}

/// Whether a value can be a monosim parameter: non-empty, no
/// whitespace, parentheses balanced (they nest inside a parameter).
fn param_ok(v: &str) -> bool {
    !v.is_empty() && !v.contains(char::is_whitespace) && crate::parser::balanced_parens(v)
}

fn mono(symbol: &str, value: &str) -> Option<Inline> {
    param_ok(value).then(|| Inline::Monosim {
        symbol: symbol.to_string(),
        param: value.to_string(),
        ann: Annotations::default(),
    })
}

// ---------------------------------------------------------------
// Embedding: model -> document
// ---------------------------------------------------------------

fn token_inline(tok: &Token) -> Inline {
    let mut content: Vec<Inline> = Vec::new();
    for p in &tok.parsings {
        content.extend(p.lexema.as_deref().and_then(|v| mono(LEXEMA, v)));
        content.extend(p.meros.as_deref().and_then(|v| mono(MEROS, v)));
        content.extend(p.parepomena.as_deref().and_then(|v| mono(PAREPOMENA, v)));
        content.extend(p.semasia.as_deref().and_then(|v| mono(SEMASIA, v)));
        content.extend(p.kephale.as_deref().and_then(|v| mono(KEPHALE, v)));
        content.extend(p.schesis.as_deref().and_then(|v| mono(SCHESIS, v)));
    }
    if !tok.form.is_empty() {
        content.push(Inline::Text(tok.form.clone()));
    }
    if content.is_empty() || !matches!(content.first(), Some(Inline::Monosim { .. })) {
        // A token without any parsing has no diaphane to justify
        // itself: its form flows as text (an onym keeps it).
        if tok.onym.is_none() {
            return Inline::Text(tok.form.clone());
        }
    }
    Inline::EndoDiaphane {
        content,
        ann: Annotations {
            onym: tok.onym.clone(),
            genoses: Vec::new(),
        },
    }
}

fn sentence_inline(s: &Sentence, n: usize) -> Inline {
    let mut content: Vec<Inline> = Vec::new();
    let id =
        s.id.clone()
            .filter(|id| param_ok(id))
            .unwrap_or_else(|| n.to_string());
    content.push(Inline::Monosim {
        symbol: PERIODOS.to_string(),
        param: id,
        ann: Annotations::default(),
    });
    for piece in &s.pieces {
        match piece {
            Piece::Token(t) => content.push(token_inline(t)),
            Piece::Text(t) => content.push(Inline::Text(t.clone())),
            Piece::Milestone { scheme, value } => content.push(Inline::Milestone {
                scheme: scheme.clone(),
                value: value.clone(),
                ann: Annotations::default(),
            }),
        }
    }
    Inline::EndoDiaphane {
        content,
        ann: Annotations::default(),
    }
}

fn paragraph_block(units: &[Unit], counter: &mut usize) -> Block {
    let mut inlines: Vec<Inline> = Vec::new();
    for unit in units {
        match unit {
            Unit::Sentence(s) => {
                *counter += 1;
                inlines.push(sentence_inline(s, *counter));
            }
            Unit::Text(t) => inlines.push(Inline::Text(t.clone())),
        }
    }
    Block::Paragraph(inlines)
}

fn solo(symbol: &str, text: &str) -> Block {
    Block::Paragraph(vec![Inline::Endo {
        symbol: symbol.to_string(),
        content: vec![Inline::Text(text.to_string())],
        bracket_matching: true,
        ann: Annotations::default(),
    }])
}

fn heading_symbol(depth: usize) -> &'static str {
    match depth {
        0 | 1 => "#",
        2 => "##",
        3 => "###",
        _ => "####",
    }
}

/// Build a litogramma document from the model: front matter,
/// then headings nesting their following blocks by depth.
pub fn corpus_to_document(corpus: &Corpus) -> Document {
    let mut blocks: Vec<Block> = Vec::new();
    if let Some(t) = &corpus.title {
        blocks.push(solo("=", t));
    }
    if let Some(a) = &corpus.author {
        blocks.push(solo("=:", a));
    }
    // A stack of open headings: (depth, block-with-children).
    let mut stack: Vec<(usize, Block)> = Vec::new();
    fn push_into(stack: &mut [(usize, Block)], top: &mut Vec<Block>, block: Block) {
        match stack.last_mut() {
            Some((_, Block::Para { children, .. })) => children.push(block),
            _ => top.push(block),
        }
    }
    fn close_to(stack: &mut Vec<(usize, Block)>, top: &mut Vec<Block>, depth: usize) {
        while stack.last().is_some_and(|(d, _)| *d >= depth) {
            let (_, block) = stack.pop().unwrap();
            push_into(stack, top, block);
        }
    }
    let mut counter = 0usize;
    for cb in &corpus.blocks {
        match cb {
            CorpusBlock::Heading { depth, text } => {
                let depth = (*depth).max(1);
                close_to(&mut stack, &mut blocks, depth);
                stack.push((
                    depth,
                    Block::Para {
                        symbol: heading_symbol(depth).to_string(),
                        taxis: None,
                        lemma: vec![Inline::Text(text.clone())],
                        children: Vec::new(),
                        hypograph: Vec::new(),
                        bracket_matching: true,
                        ann: Annotations::default(),
                    },
                ));
            }
            CorpusBlock::Paragraph(units) => {
                let block = paragraph_block(units, &mut counter);
                push_into(&mut stack, &mut blocks, block);
            }
        }
    }
    close_to(&mut stack, &mut blocks, 0);
    Document {
        dialect_id: "litogramma".to_string(),
        dialect_version: None,
        blocks,
    }
}

// ---------------------------------------------------------------
// Extraction: document -> model
// ---------------------------------------------------------------

fn text_of(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                text_of(content, out)
            }
            Inline::VerbatimInline { content, .. } => out.push_str(content),
            _ => {}
        }
    }
}

fn is_token(content: &[Inline]) -> bool {
    matches!(content.first(), Some(Inline::Monosim { symbol, .. }) if PACK.contains(&symbol.as_str()))
}

fn is_sentence(content: &[Inline]) -> bool {
    matches!(content.first(), Some(Inline::Monosim { symbol, .. }) if symbol == PERIODOS)
        || content
            .iter()
            .any(|x| matches!(x, Inline::EndoDiaphane { content, .. } if is_token(content)))
}

fn token_of(content: &[Inline], ann: &Annotations) -> Token {
    let mut parsings: Vec<Parsing> = Vec::new();
    let mut form = String::new();
    for inline in content {
        match inline {
            Inline::Monosim { symbol, param, .. } if PACK.contains(&symbol.as_str()) => {
                let sym = symbol.as_str();
                let needs_new = parsings.is_empty()
                    || sym == LEXEMA
                    || match (sym, parsings.last().unwrap()) {
                        (MEROS, p) => p.meros.is_some(),
                        (PAREPOMENA, p) => p.parepomena.is_some(),
                        (SEMASIA, p) => p.semasia.is_some(),
                        (KEPHALE, p) => p.kephale.is_some(),
                        (SCHESIS, p) => p.schesis.is_some(),
                        _ => false,
                    };
                if needs_new {
                    parsings.push(Parsing::default());
                }
                let p = parsings.last_mut().unwrap();
                let slot = match sym {
                    LEXEMA => &mut p.lexema,
                    MEROS => &mut p.meros,
                    PAREPOMENA => &mut p.parepomena,
                    SEMASIA => &mut p.semasia,
                    KEPHALE => &mut p.kephale,
                    _ => &mut p.schesis,
                };
                *slot = Some(param.clone());
            }
            other => text_of(std::slice::from_ref(other), &mut form),
        }
    }
    parsings.retain(|p| !p.is_empty());
    Token {
        form,
        parsings,
        onym: ann.onym.clone(),
    }
}

fn push_text(pieces: &mut Vec<Piece>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Piece::Text(t)) = pieces.last_mut() {
        t.push_str(text);
    } else {
        pieces.push(Piece::Text(text.to_string()));
    }
}

fn sentence_of(content: &[Inline]) -> Sentence {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut id: Option<String> = None;
    for inline in content {
        match inline {
            Inline::Monosim { symbol, param, .. } if symbol == PERIODOS => {
                if id.is_none() {
                    id = Some(param.clone());
                }
            }
            Inline::EndoDiaphane { content, ann } if is_token(content) => {
                pieces.push(Piece::Token(token_of(content, ann)));
            }
            Inline::Milestone { scheme, value, .. } => pieces.push(Piece::Milestone {
                scheme: scheme.clone(),
                value: value.clone(),
            }),
            Inline::Monosim { .. } | Inline::OnymAnchor(_) | Inline::Deixis { .. } => {}
            other => {
                let mut t = String::new();
                text_of(std::slice::from_ref(other), &mut t);
                push_text(&mut pieces, &t);
            }
        }
    }
    Sentence { id, pieces }
}

fn units_of(inlines: &[Inline]) -> Vec<Unit> {
    let mut units: Vec<Unit> = Vec::new();
    let push_text = |units: &mut Vec<Unit>, text: &str| {
        if text.is_empty() {
            return;
        }
        if let Some(Unit::Text(t)) = units.last_mut() {
            t.push_str(text);
        } else {
            units.push(Unit::Text(text.to_string()));
        }
    };
    for inline in inlines {
        match inline {
            Inline::EndoDiaphane { content, ann } if is_token(content) => {
                // A token outside any sentence: a sentence of one.
                units.push(Unit::Sentence(Sentence {
                    id: None,
                    pieces: vec![Piece::Token(token_of(content, ann))],
                }));
            }
            Inline::EndoDiaphane { content, .. } if is_sentence(content) => {
                units.push(Unit::Sentence(sentence_of(content)));
            }
            Inline::Monosim { .. }
            | Inline::OnymAnchor(_)
            | Inline::Deixis { .. }
            | Inline::Milestone { .. } => {}
            other => {
                let mut t = String::new();
                text_of(std::slice::from_ref(other), &mut t);
                push_text(&mut units, &t);
            }
        }
    }
    units
}

fn heading_depth(symbol: &str) -> Option<usize> {
    Some(match symbol {
        "==" => 1,
        "===" => 2,
        "#" => 1,
        "##" => 2,
        "###" => 3,
        "####" => 4,
        _ => return None,
    })
}

fn collect(blocks: &[Block], depth: usize, corpus: &mut Corpus) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => {
                // Front matter stands alone in a paragraph.
                if let [
                    Inline::Endo {
                        symbol, content, ..
                    },
                ] = inlines.as_slice()
                {
                    match symbol.as_str() {
                        "=" if corpus.title.is_none() => {
                            let mut t = String::new();
                            text_of(content, &mut t);
                            corpus.title = Some(t);
                            continue;
                        }
                        "=:" if corpus.author.is_none() => {
                            let mut t = String::new();
                            text_of(content, &mut t);
                            corpus.author = Some(t);
                            continue;
                        }
                        "=_" | "=;" | "=#=" => continue,
                        _ => {}
                    }
                }
                let units = units_of(inlines);
                if !units.is_empty() {
                    corpus.blocks.push(CorpusBlock::Paragraph(units));
                }
            }
            Block::Para {
                symbol,
                lemma,
                children,
                ..
            } => match heading_depth(symbol) {
                Some(d) => {
                    let mut t = String::new();
                    text_of(lemma, &mut t);
                    let depth = depth + 1;
                    let _ = d;
                    corpus.blocks.push(CorpusBlock::Heading { depth, text: t });
                    collect(children, depth, corpus);
                }
                None => collect(children, depth, corpus),
            },
            Block::Stichoi { strophes, .. } => {
                for Strophe(lines) in strophes {
                    for line in lines {
                        let units = units_of(line);
                        if !units.is_empty() {
                            corpus.blocks.push(CorpusBlock::Paragraph(units));
                        }
                    }
                }
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                collect(children, depth, corpus)
            }
            _ => {}
        }
    }
}

/// Extract the corpus model from a document: headings by nesting
/// depth, paragraphs as sentences and text, front matter as
/// title and author.
pub fn corpus_of(doc: &Document) -> Corpus {
    let mut corpus = Corpus::default();
    collect(&doc.blocks, 0, &mut corpus);
    corpus
}

// ---------------------------------------------------------------
// XML helpers
// ---------------------------------------------------------------

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn text_until(toks: &[Tok], mut i: usize, until: &str) -> (String, usize) {
    let mut out = String::new();
    let mut depth = 0usize;
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(n) if n == until => {
                if depth == 0 {
                    return (out, i + 1);
                }
                depth -= 1;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == until && !self_closing => depth += 1,
            Tok::Text(t) => out.push_str(&decode_entities(t)),
            _ => {}
        }
        i += 1;
    }
    (out, i)
}

fn is_blank(tok: &Tok) -> bool {
    matches!(tok, Tok::Text(t) if t.trim().is_empty())
}

// ---------------------------------------------------------------
// Russian National Corpus
// ---------------------------------------------------------------

/// Split an RNC `gr` string into the part of speech and the rest,
/// keeping a leading `=` on the rest when the string had one
/// (`S,f,inan=sg,nom` -> `S` + `f,inan=sg,nom`; `NUM=ciph` ->
/// `NUM` + `=ciph`).
fn split_gr(gr: &str) -> (String, Option<String>) {
    match gr.find([',', '=']) {
        Some(i) => {
            let rest = if gr[i..].starts_with(',') {
                &gr[i + 1..]
            } else {
                &gr[i..]
            };
            (
                gr[..i].to_string(),
                (!rest.is_empty()).then(|| rest.to_string()),
            )
        }
        None => (gr.to_string(), None),
    }
}

fn join_gr(meros: Option<&str>, parepomena: Option<&str>) -> Option<String> {
    match (meros, parepomena) {
        (Some(m), Some(p)) if p.starts_with('=') => Some(format!("{m}{p}")),
        (Some(m), Some(p)) => Some(format!("{m},{p}")),
        (Some(m), None) => Some(m.to_string()),
        (None, Some(p)) => Some(p.trim_start_matches([',', '=']).to_string()),
        (None, None) => None,
    }
}

fn rnc_token(toks: &[Tok], mut i: usize) -> (Token, usize) {
    let mut parsings: Vec<Parsing> = Vec::new();
    let mut form = String::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(n) if n == "w" => {
                return (
                    Token {
                        form: collapse_ws(&form),
                        parsings,
                        onym: None,
                    },
                    i + 1,
                );
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "ana" => {
                let mut p = Parsing {
                    lexema: attr(attrs, "lex").map(str::to_string),
                    ..Parsing::default()
                };
                if let Some(gr) = attr(attrs, "gr") {
                    let (m, rest) = split_gr(gr);
                    p.meros = (!m.is_empty()).then_some(m);
                    p.parepomena = rest;
                }
                p.semasia =
                    attr(attrs, "sem").map(|s| s.split_whitespace().collect::<Vec<_>>().join("_"));
                parsings.push(p);
                i += 1;
                if !self_closing {
                    // The word may sit inside the ana element.
                    let (inner, next) = text_until(toks, i, "ana");
                    form.push_str(&inner);
                    i = next;
                }
            }
            Tok::Text(t) => {
                form.push_str(&decode_entities(t));
                i += 1;
            }
            _ => i += 1,
        }
    }
    (
        Token {
            form: collapse_ws(&form),
            parsings,
            onym: None,
        },
        i,
    )
}

fn rnc_sentence(toks: &[Tok], mut i: usize) -> (Sentence, usize) {
    let mut pieces: Vec<Piece> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "se" => {
                return (Sentence { id: None, pieces }, i + 1);
            }
            Tok::Open { name, .. } if name == "w" => {
                let (tok, next) = rnc_token(toks, i + 1);
                pieces.push(Piece::Token(tok));
                i = next;
            }
            Tok::Text(t) => {
                push_text(&mut pieces, &collapse_ws(&decode_entities(t)));
                i += 1;
            }
            _ => i += 1,
        }
    }
    (Sentence { id: None, pieces }, i)
}

/// Import a Russian National Corpus XML document (the XHTML-shaped
/// export: `head` metadata, `body` paragraphs of `se` sentences
/// of `w` words with `ana` analyses) as litogramma.
pub fn rnc_to_document(xml: &str) -> Result<Document> {
    let toks = tokenize_xml(xml)?;
    let mut corpus = Corpus::default();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Open { name, .. } if name == "title" => {
                let (t, next) = text_until(&toks, i + 1, "title");
                let t = collapse_ws(&t);
                if corpus.title.is_none() && !t.trim().is_empty() {
                    corpus.title = Some(t.trim().to_string());
                }
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "meta" => {
                match (attr(attrs, "name"), attr(attrs, "content")) {
                    (Some("author"), Some(c)) if corpus.author.is_none() => {
                        corpus.author = Some(decode_entities(c));
                    }
                    (Some("header"), Some(c)) if corpus.title.is_none() => {
                        corpus.title = Some(decode_entities(c));
                    }
                    _ => {}
                }
                i += 1;
            }
            Tok::Open { name, .. } if name == "p" => {
                let mut units: Vec<Unit> = Vec::new();
                i += 1;
                while i < toks.len() {
                    match &toks[i] {
                        Tok::Close(n) if n == "p" => {
                            i += 1;
                            break;
                        }
                        Tok::Open { name, .. } if name == "se" => {
                            let (s, next) = rnc_sentence(&toks, i + 1);
                            units.push(Unit::Sentence(s));
                            i = next;
                        }
                        Tok::Text(t) => {
                            let t = collapse_ws(&decode_entities(t));
                            if !t.trim().is_empty() || !units.is_empty() {
                                if let Some(Unit::Text(prev)) = units.last_mut() {
                                    prev.push_str(&t);
                                } else if !t.trim().is_empty() {
                                    units.push(Unit::Text(t));
                                } else {
                                    units.push(Unit::Text(" ".to_string()));
                                }
                            }
                            i += 1;
                        }
                        _ => i += 1,
                    }
                }
                // Trailing whitespace between the last sentence and
                // the paragraph's close carries nothing.
                while matches!(units.last(), Some(Unit::Text(t)) if t.trim().is_empty()) {
                    units.pop();
                }
                if !units.is_empty() {
                    corpus.blocks.push(CorpusBlock::Paragraph(units));
                }
            }
            _ => i += 1,
        }
    }
    Ok(corpus_to_document(&corpus))
}

fn rnc_export(corpus: &Corpus) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html>\n<head>\n");
    if let Some(a) = &corpus.author {
        out.push_str(&format!("<meta content=\"{}\" name=\"author\"/>\n", esc(a)));
    }
    if let Some(t) = &corpus.title {
        out.push_str(&format!("<title>{}</title>\n", esc(t)));
    }
    out.push_str("</head>\n<body>\n");
    for block in &corpus.blocks {
        match block {
            CorpusBlock::Heading { text, .. } => {
                out.push_str(&format!("<p>{}</p>\n", esc(text)));
            }
            CorpusBlock::Paragraph(units) => {
                out.push_str("<p>");
                for unit in units {
                    match unit {
                        Unit::Text(t) => out.push_str(&esc(t)),
                        Unit::Sentence(s) => {
                            out.push_str("<se>");
                            for piece in &s.pieces {
                                match piece {
                                    Piece::Text(t) => out.push_str(&esc(t)),
                                    Piece::Milestone { .. } => {}
                                    Piece::Token(tok) => {
                                        out.push_str("<w>");
                                        for p in &tok.parsings {
                                            out.push_str("<ana");
                                            if let Some(l) = &p.lexema {
                                                out.push_str(&format!(" lex=\"{}\"", esc(l)));
                                            }
                                            if let Some(gr) =
                                                join_gr(p.meros.as_deref(), p.parepomena.as_deref())
                                            {
                                                out.push_str(&format!(" gr=\"{}\"", esc(&gr)));
                                            }
                                            if let Some(sem) = &p.semasia {
                                                out.push_str(&format!(
                                                    " sem=\"{}\"",
                                                    esc(&sem.replace('_', " "))
                                                ));
                                            }
                                            out.push_str("/>");
                                        }
                                        out.push_str(&esc(&tok.form));
                                        out.push_str("</w>");
                                    }
                                }
                            }
                            out.push_str("</se>");
                        }
                    }
                }
                out.push_str("</p>\n");
            }
        }
    }
    out.push_str("</body>\n</html>\n");
    out
}

// ---------------------------------------------------------------
// OpenCorpora
// ---------------------------------------------------------------

/// Split a sentence's source text around its tokens, in order:
/// the text between tokens (whitespace, mostly) becomes text
/// pieces; a token not found from the current offset is placed
/// without a gap.
fn align(source: &str, tokens: Vec<Token>) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut pos = 0usize;
    for tok in tokens {
        if let Some(idx) = source[pos..].find(&tok.form) {
            if idx > 0 {
                push_text(&mut pieces, &source[pos..pos + idx]);
            }
            pos += idx + tok.form.len();
        }
        pieces.push(Piece::Token(tok));
    }
    if pos < source.len() {
        push_text(&mut pieces, &source[pos..]);
    }
    pieces
}

fn opencorpora_token(toks: &[Tok], mut i: usize, form: String) -> (Token, usize) {
    let mut parsings: Vec<Parsing> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(n) if n == "token" => break,
            Tok::Open { name, attrs, .. } if name == "l" => {
                let mut p = Parsing {
                    lexema: attr(attrs, "t").map(decode_entities),
                    ..Parsing::default()
                };
                let mut grams: Vec<String> = Vec::new();
                i += 1;
                while i < toks.len() {
                    match &toks[i] {
                        Tok::Close(n) if n == "l" => break,
                        Tok::Open { name, attrs, .. } if name == "g" => {
                            if let Some(v) = attr(attrs, "v") {
                                grams.push(v.to_string());
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                if !grams.is_empty() {
                    p.meros = Some(grams.remove(0));
                }
                if !grams.is_empty() {
                    p.parepomena = Some(grams.join(","));
                }
                parsings.push(p);
            }
            _ => {}
        }
        i += 1;
    }
    (
        Token {
            form,
            parsings,
            onym: None,
        },
        i + 1,
    )
}

/// Import an OpenCorpora XML export (`annotation/text/paragraphs/
/// paragraph/sentence` with `source` and `tokens`) as litogramma.
pub fn opencorpora_to_document(xml: &str) -> Result<Document> {
    let toks = tokenize_xml(xml)?;
    let mut corpus = Corpus::default();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Open { name, attrs, .. } if name == "text" => {
                if let Some(name) = attr(attrs, "name")
                    && corpus.title.is_none()
                    && !name.is_empty()
                {
                    corpus.title = Some(decode_entities(name));
                }
                i += 1;
            }
            Tok::Open { name, .. } if name == "paragraph" => {
                let mut units: Vec<Unit> = Vec::new();
                i += 1;
                while i < toks.len() {
                    match &toks[i] {
                        Tok::Close(nm) if nm == "paragraph" => {
                            i += 1;
                            break;
                        }
                        Tok::Open { name, attrs, .. } if name == "sentence" => {
                            let sid = attr(attrs, "id").map(str::to_string);
                            let mut source = String::new();
                            let mut tokens: Vec<Token> = Vec::new();
                            i += 1;
                            while i < toks.len() {
                                match &toks[i] {
                                    Tok::Close(nm) if nm == "sentence" => {
                                        i += 1;
                                        break;
                                    }
                                    Tok::Open { name, .. } if name == "source" => {
                                        let (t, next) = text_until(&toks, i + 1, "source");
                                        source = t;
                                        i = next;
                                    }
                                    Tok::Open { name, attrs, .. } if name == "token" => {
                                        let form = attr(attrs, "text")
                                            .map(decode_entities)
                                            .unwrap_or_default();
                                        let (tok, next) = opencorpora_token(&toks, i + 1, form);
                                        tokens.push(tok);
                                        i = next;
                                    }
                                    _ => i += 1,
                                }
                            }
                            if !units.is_empty() {
                                units.push(Unit::Text(" ".to_string()));
                            }
                            units.push(Unit::Sentence(Sentence {
                                id: sid,
                                pieces: align(&source, tokens),
                            }));
                        }
                        _ => i += 1,
                    }
                }
                if !units.is_empty() {
                    corpus.blocks.push(CorpusBlock::Paragraph(units));
                }
            }
            _ => i += 1,
        }
    }
    Ok(corpus_to_document(&corpus))
}

fn opencorpora_export(corpus: &Corpus) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<annotation version=\"2.0\" revision=\"0\">\n",
    );
    out.push_str(&format!(
        "<text id=\"1\" parent=\"0\" name=\"{}\">\n<paragraphs>\n",
        esc(corpus.title.as_deref().unwrap_or(""))
    ));
    let mut para_id = 0usize;
    let mut sent_id = 0usize;
    let mut tok_id = 0usize;
    let mut lemma_id = 0usize;
    let mut emit_sentence = |out: &mut String, s: &Sentence| {
        sent_id += 1;
        let sid =
            s.id.as_deref()
                .filter(|id| id.chars().all(|c| c.is_ascii_digit()))
                .map(str::to_string)
                .unwrap_or_else(|| sent_id.to_string());
        let mut source = String::new();
        for piece in &s.pieces {
            match piece {
                Piece::Text(t) => source.push_str(t),
                Piece::Token(tok) => source.push_str(&tok.form),
                Piece::Milestone { .. } => {}
            }
        }
        out.push_str(&format!(
            "<sentence id=\"{sid}\">\n<source>{}</source>\n<tokens>\n",
            esc(&source)
        ));
        for piece in &s.pieces {
            if let Piece::Token(tok) = piece {
                tok_id += 1;
                out.push_str(&format!(
                    "<token id=\"{tok_id}\" text=\"{f}\">\n<tfr rev_id=\"{tok_id}\" t=\"{f}\">\n",
                    f = esc(&tok.form)
                ));
                for p in &tok.parsings {
                    lemma_id += 1;
                    out.push_str(&format!(
                        "<v><l id=\"{lemma_id}\" t=\"{}\">",
                        esc(p.lexema.as_deref().unwrap_or(&tok.form))
                    ));
                    if let Some(m) = &p.meros {
                        out.push_str(&format!("<g v=\"{}\"/>", esc(m)));
                    }
                    if let Some(f) = &p.parepomena {
                        for g in f.split(',') {
                            out.push_str(&format!("<g v=\"{}\"/>", esc(g)));
                        }
                    }
                    out.push_str("</l></v>\n");
                }
                out.push_str("</tfr>\n</token>\n");
            }
        }
        out.push_str("</tokens>\n</sentence>\n");
    };
    for block in &corpus.blocks {
        para_id += 1;
        out.push_str(&format!("<paragraph id=\"{para_id}\">\n"));
        match block {
            CorpusBlock::Heading { text, .. } => {
                emit_sentence(
                    &mut out,
                    &Sentence {
                        id: None,
                        pieces: vec![Piece::Text(text.clone())],
                    },
                );
            }
            CorpusBlock::Paragraph(units) => {
                for unit in units {
                    match unit {
                        Unit::Sentence(s) => emit_sentence(&mut out, s),
                        Unit::Text(t) if !t.trim().is_empty() => emit_sentence(
                            &mut out,
                            &Sentence {
                                id: None,
                                pieces: vec![Piece::Text(t.trim().to_string())],
                            },
                        ),
                        Unit::Text(_) => {}
                    }
                }
            }
        }
        out.push_str("</paragraph>\n");
    }
    out.push_str("</paragraphs>\n</text>\n</annotation>\n");
    out
}

// ---------------------------------------------------------------
// PROIEL
// ---------------------------------------------------------------

/// A citation part as a milestone value: spaces become
/// underscores (`MATT 1.1` -> `MATT_1.1`), reversibly.
fn citation_value(c: &str) -> Option<String> {
    let v: String = c.split_whitespace().collect::<Vec<_>>().join("_");
    crate::sigil::is_valid_milestone_value(&v).then_some(v)
}

/// Import a PROIEL XML treebank (`source/div/sentence/token` with
/// lemma, part of speech, morphology, head and relation, the
/// text between tokens in presentation attributes) as
/// litogramma: one paragraph per sentence, divisions as
/// headings, the citation part as a `proiel` milestone wherever
/// it changes.
pub fn proiel_to_document(xml: &str) -> Result<Document> {
    let toks = tokenize_xml(xml)?;
    let mut corpus = Corpus::default();
    let mut i = 0;
    let mut depth = 0usize;
    let mut citation: Option<String> = None;
    let mut in_annotation = false;
    while i < toks.len() {
        match &toks[i] {
            Tok::Open {
                name, self_closing, ..
            } if name == "annotation" => {
                // The tagset declaration: nothing of the text.
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(&toks, i + 1, "annotation".to_string())?;
                }
                let _ = in_annotation;
                in_annotation = false;
            }
            Tok::Open { name, .. } if name == "title" && depth == 0 => {
                let (t, next) = text_until(&toks, i + 1, "title");
                if corpus.title.is_none() {
                    corpus.title = Some(collapse_ws(&t).trim().to_string());
                }
                i = next;
            }
            Tok::Open { name, .. } if name == "author" && depth == 0 => {
                let (t, next) = text_until(&toks, i + 1, "author");
                if corpus.author.is_none() {
                    corpus.author = Some(collapse_ws(&t).trim().to_string());
                }
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "div" => {
                if !*self_closing {
                    depth += 1;
                    // A div's own title, if it opens with one.
                    let mut j = i + 1;
                    while j < toks.len() && is_blank(&toks[j]) {
                        j += 1;
                    }
                    if let Some(Tok::Open { name, .. }) = toks.get(j)
                        && name == "title"
                    {
                        let (t, next) = text_until(&toks, j + 1, "title");
                        corpus.blocks.push(CorpusBlock::Heading {
                            depth,
                            text: collapse_ws(&t).trim().to_string(),
                        });
                        i = next;
                        continue;
                    }
                }
                i += 1;
            }
            Tok::Close(name) if name == "div" => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "sentence" => {
                let sid = attr(attrs, "id").unwrap_or("0").to_string();
                let mut pieces: Vec<Piece> = Vec::new();
                if let Some(b) = attr(attrs, "presentation-before") {
                    push_text(&mut pieces, &decode_entities(b));
                }
                let after = attr(attrs, "presentation-after").map(decode_entities);
                i += 1;
                if !*self_closing {
                    while i < toks.len() {
                        match &toks[i] {
                            Tok::Close(n) if n == "sentence" => {
                                i += 1;
                                break;
                            }
                            Tok::Open {
                                name,
                                attrs,
                                self_closing,
                            } if name == "token" => {
                                let form = attr(attrs, "form").map(decode_entities);
                                let sc = *self_closing;
                                if let Some(form) = form.filter(|f| !f.is_empty()) {
                                    if let Some(c) = attr(attrs, "citation-part")
                                        && citation.as_deref() != Some(c)
                                    {
                                        citation = Some(c.to_string());
                                        if let Some(v) = citation_value(c) {
                                            pieces.push(Piece::Milestone {
                                                scheme: "proiel".to_string(),
                                                value: v,
                                            });
                                        }
                                    }
                                    if let Some(b) = attr(attrs, "presentation-before") {
                                        push_text(&mut pieces, &decode_entities(b));
                                    }
                                    let p = Parsing {
                                        lexema: attr(attrs, "lemma").map(decode_entities),
                                        meros: attr(attrs, "part-of-speech").map(str::to_string),
                                        parepomena: attr(attrs, "morphology").map(str::to_string),
                                        semasia: None,
                                        kephale: attr(attrs, "head-id").map(|h| format!("t{h}")),
                                        schesis: attr(attrs, "relation").map(str::to_string),
                                    };
                                    pieces.push(Piece::Token(Token {
                                        form,
                                        parsings: vec![p],
                                        onym: attr(attrs, "id").map(|id| format!("t{id}")),
                                    }));
                                    if let Some(a) = attr(attrs, "presentation-after") {
                                        push_text(&mut pieces, &decode_entities(a));
                                    }
                                }
                                i += 1;
                                if !sc {
                                    i = skip_element(&toks, i, "token".to_string())?;
                                }
                            }
                            _ => i += 1,
                        }
                    }
                }
                if let Some(a) = after {
                    push_text(&mut pieces, &a);
                }
                // Heads pointing at tokens that did not import
                // (empty tokens) drop.
                let onyms: std::collections::HashSet<String> = pieces
                    .iter()
                    .filter_map(|p| match p {
                        Piece::Token(t) => t.onym.clone(),
                        _ => None,
                    })
                    .collect();
                for piece in &mut pieces {
                    if let Piece::Token(t) = piece {
                        for p in &mut t.parsings {
                            if p.kephale.as_ref().is_some_and(|h| !onyms.contains(h)) {
                                p.kephale = None;
                            }
                        }
                    }
                }
                // Trailing space after the last token is the
                // sentence separator; the paragraph does not keep it.
                if let Some(Piece::Text(t)) = pieces.last_mut() {
                    let trimmed = t.trim_end().to_string();
                    if trimmed.is_empty() {
                        pieces.pop();
                    } else {
                        *t = trimmed;
                    }
                }
                corpus
                    .blocks
                    .push(CorpusBlock::Paragraph(vec![Unit::Sentence(Sentence {
                        id: Some(sid),
                        pieces,
                    })]));
            }
            _ => i += 1,
        }
    }
    Ok(corpus_to_document(&corpus))
}

fn proiel_export(corpus: &Corpus) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<proiel schema-version=\"2.0\">\n<source id=\"atrep\" language=\"und\">\n",
    );
    if let Some(t) = &corpus.title {
        out.push_str(&format!("<title>{}</title>\n", esc(t)));
    }
    if let Some(a) = &corpus.author {
        out.push_str(&format!("<author>{}</author>\n", esc(a)));
    }
    // Token ids first, so heads can be resolved across the text.
    let mut ids: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut next_id = 0usize;
    for block in &corpus.blocks {
        if let CorpusBlock::Paragraph(units) = block {
            for unit in units {
                if let Unit::Sentence(s) = unit {
                    for piece in &s.pieces {
                        if let Piece::Token(t) = piece {
                            next_id += 1;
                            if let Some(o) = &t.onym {
                                ids.insert(o.clone(), next_id);
                            }
                        }
                    }
                }
            }
        }
    }
    let mut open_divs: Vec<usize> = Vec::new();
    let mut sent_id = 0usize;
    let mut tok_id = 0usize;
    let mut citation: Option<String> = None;
    let mut any_div = false;
    for block in &corpus.blocks {
        match block {
            CorpusBlock::Heading { depth, text } => {
                while open_divs.last().is_some_and(|d| *d >= *depth) {
                    open_divs.pop();
                    out.push_str("</div>\n");
                }
                open_divs.push(*depth);
                any_div = true;
                out.push_str(&format!("<div>\n<title>{}</title>\n", esc(text)));
            }
            CorpusBlock::Paragraph(units) => {
                if !any_div {
                    out.push_str("<div>\n");
                    open_divs.push(0);
                    any_div = true;
                }
                for unit in units {
                    let Unit::Sentence(s) = unit else { continue };
                    sent_id += 1;
                    let sid =
                        s.id.as_deref()
                            .filter(|id| id.chars().all(|c| c.is_ascii_digit()))
                            .map(str::to_string)
                            .unwrap_or_else(|| sent_id.to_string());
                    out.push_str(&format!("<sentence id=\"{sid}\">\n"));
                    // The text before the first token is that
                    // token's presentation-before; every other gap
                    // is the preceding token's presentation-after.
                    let mut pending_before = String::new();
                    let mut tokens: Vec<(String, &Token, String)> = Vec::new();
                    let mut last_citation: Vec<Option<String>> = Vec::new();
                    for piece in &s.pieces {
                        match piece {
                            Piece::Milestone { scheme, value } if scheme == "proiel" => {
                                citation = Some(value.replace('_', " "));
                            }
                            Piece::Milestone { .. } => {}
                            Piece::Text(t) => {
                                if let Some(last) = tokens.last_mut() {
                                    last.2.push_str(t);
                                } else {
                                    pending_before.push_str(t);
                                }
                            }
                            Piece::Token(tok) => {
                                tokens.push((
                                    std::mem::take(&mut pending_before),
                                    tok,
                                    String::new(),
                                ));
                                last_citation.push(citation.clone());
                            }
                        }
                    }
                    for ((before, tok, after), cite) in tokens.into_iter().zip(last_citation) {
                        tok_id += 1;
                        out.push_str(&format!(
                            "<token id=\"{tok_id}\" form=\"{}\"",
                            esc(&tok.form)
                        ));
                        if let Some(c) = cite {
                            out.push_str(&format!(" citation-part=\"{}\"", esc(&c)));
                        }
                        if let Some(p) = tok.parsings.first() {
                            if let Some(l) = &p.lexema {
                                out.push_str(&format!(" lemma=\"{}\"", esc(l)));
                            }
                            if let Some(m) = &p.meros {
                                out.push_str(&format!(" part-of-speech=\"{}\"", esc(m)));
                            }
                            if let Some(f) = &p.parepomena {
                                out.push_str(&format!(" morphology=\"{}\"", esc(f)));
                            }
                            if let Some(h) = p.kephale.as_ref().and_then(|h| ids.get(h)) {
                                out.push_str(&format!(" head-id=\"{h}\""));
                            }
                            if let Some(r) = &p.schesis {
                                out.push_str(&format!(" relation=\"{}\"", esc(r)));
                            }
                        }
                        if !before.is_empty() {
                            out.push_str(&format!(" presentation-before=\"{}\"", esc(&before)));
                        }
                        if !after.is_empty() {
                            out.push_str(&format!(" presentation-after=\"{}\"", esc(&after)));
                        }
                        out.push_str("/>\n");
                    }
                    out.push_str("</sentence>\n");
                }
            }
        }
    }
    for _ in open_divs {
        out.push_str("</div>\n");
    }
    out.push_str("</source>\n</proiel>\n");
    out
}

// ---------------------------------------------------------------
// CoNLL-U (Universal Dependencies)
// ---------------------------------------------------------------

/// Import a CoNLL-U file: each sentence a paragraph, `sent_id`
/// the periodos, `text` the source aligned around the tokens,
/// LEMMA/UPOS(+XPOS)/FEATS/HEAD/DEPREL as lexema/meros/
/// parepomena/kephale/schesis. Multiword ranges and empty nodes
/// are skipped; DEPS and MISC (beyond spacing) are recorded loss.
pub fn conllu_to_document(text: &str) -> Result<Document> {
    let mut corpus = Corpus::default();
    let mut sent_no = 0usize;
    for block in text.split("\n\n") {
        let mut sid: Option<String> = None;
        let mut source: Option<String> = None;
        let mut rows: Vec<Vec<String>> = Vec::new();
        for line in block.lines() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            if let Some(comment) = line.strip_prefix('#') {
                let comment = comment.trim();
                if let Some(v) = comment.strip_prefix("sent_id") {
                    sid = Some(v.trim_start_matches([' ', '=']).trim().to_string());
                } else if let Some(v) = comment.strip_prefix("text")
                    && !comment.starts_with("text_")
                {
                    source = Some(v.trim_start_matches([' ', '=']).trim().to_string());
                }
                continue;
            }
            let fields: Vec<String> = line.split('\t').map(str::to_string).collect();
            if fields.len() < 8 {
                continue;
            }
            rows.push(fields);
        }
        if rows.is_empty() {
            continue;
        }
        sent_no += 1;
        let field = |row: &[String], i: usize| -> Option<String> {
            row.get(i)
                .filter(|v| !v.is_empty() && v.as_str() != "_")
                .cloned()
        };
        let mut tokens: Vec<Token> = Vec::new();
        let mut no_space: Vec<bool> = Vec::new();
        for row in &rows {
            let id = &row[0];
            if id.contains('-') || id.contains('.') {
                continue;
            }
            let upos = field(row, 3);
            let xpos = field(row, 4);
            let meros = match (upos, xpos) {
                (Some(u), Some(x)) => Some(format!("{u}/{x}")),
                (Some(u), None) => Some(u),
                (None, Some(x)) => Some(format!("/{x}")),
                (None, None) => None,
            };
            let head = field(row, 6)
                .filter(|h| h != "0")
                .map(|h| format!("t{sent_no}-{h}"));
            let p = Parsing {
                lexema: field(row, 2),
                meros,
                parepomena: field(row, 5).map(|f| f.replace('|', ",")),
                semasia: None,
                kephale: head,
                schesis: field(row, 7),
            };
            no_space
                .push(field(row, 9).is_some_and(|m| m.split('|').any(|x| x == "SpaceAfter=No")));
            tokens.push(Token {
                form: row[1].clone(),
                parsings: vec![p],
                onym: Some(format!("t{sent_no}-{id}")),
            });
        }
        let pieces = match source {
            Some(src) => align(&src, tokens),
            None => {
                let mut pieces: Vec<Piece> = Vec::new();
                let n = tokens.len();
                for (k, tok) in tokens.into_iter().enumerate() {
                    pieces.push(Piece::Token(tok));
                    if k + 1 < n && !no_space[k] {
                        pieces.push(Piece::Text(" ".to_string()));
                    }
                }
                pieces
            }
        };
        corpus
            .blocks
            .push(CorpusBlock::Paragraph(vec![Unit::Sentence(Sentence {
                id: sid,
                pieces,
            })]));
    }
    Ok(corpus_to_document(&corpus))
}

fn conllu_export(corpus: &Corpus) -> String {
    let mut out = String::new();
    let mut sent_no = 0usize;
    for block in &corpus.blocks {
        let CorpusBlock::Paragraph(units) = block else {
            continue;
        };
        for unit in units {
            let Unit::Sentence(s) = unit else { continue };
            let tokens: Vec<(usize, &Token)> = s
                .pieces
                .iter()
                .enumerate()
                .filter_map(|(k, p)| match p {
                    Piece::Token(t) => Some((k, t)),
                    _ => None,
                })
                .collect();
            if tokens.is_empty() {
                continue;
            }
            sent_no += 1;
            let sid = s.id.clone().unwrap_or_else(|| sent_no.to_string());
            let mut source = String::new();
            for piece in &s.pieces {
                match piece {
                    Piece::Text(t) => source.push_str(t),
                    Piece::Token(t) => source.push_str(&t.form),
                    Piece::Milestone { .. } => {}
                }
            }
            out.push_str(&format!("# sent_id = {sid}\n# text = {source}\n"));
            let ids: std::collections::HashMap<&str, usize> = tokens
                .iter()
                .enumerate()
                .filter_map(|(n, (_, t))| t.onym.as_deref().map(|o| (o, n + 1)))
                .collect();
            for (n, (k, tok)) in tokens.iter().enumerate() {
                let p = tok.parsings.first();
                let (upos, xpos) = match p.and_then(|p| p.meros.as_deref()) {
                    Some(m) => match m.split_once('/') {
                        Some((u, x)) => (u.to_string(), x.to_string()),
                        None => (m.to_string(), "_".to_string()),
                    },
                    None => ("_".to_string(), "_".to_string()),
                };
                let upos = if upos.is_empty() {
                    "_".to_string()
                } else {
                    upos
                };
                let head = p
                    .and_then(|p| p.kephale.as_deref())
                    .and_then(|h| ids.get(h))
                    .map(|h| h.to_string())
                    .unwrap_or_else(|| "0".to_string());
                let space_after = match s.pieces.get(k + 1) {
                    Some(Piece::Text(t)) => t.starts_with(char::is_whitespace),
                    Some(_) => false,
                    None => true,
                };
                out.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t_\t{}\n",
                    n + 1,
                    tok.form,
                    p.and_then(|p| p.lexema.as_deref()).unwrap_or("_"),
                    upos,
                    xpos,
                    p.and_then(|p| p.parepomena.as_deref())
                        .map(|f| f.replace(',', "|"))
                        .unwrap_or_else(|| "_".to_string()),
                    head,
                    p.and_then(|p| p.schesis.as_deref()).unwrap_or("_"),
                    if space_after { "_" } else { "SpaceAfter=No" },
                ));
            }
            out.push('\n');
        }
    }
    out
}

// ---------------------------------------------------------------
// Export entry
// ---------------------------------------------------------------

/// Export a document to one of the corpus formats: `rnc`,
/// `opencorpora`, `proiel`, `conllu`. None for any other target.
pub fn export(doc: &Document, target: &str) -> Option<Result<String>> {
    let corpus = corpus_of(doc);
    match target {
        "rnc" => Some(Ok(rnc_export(&corpus))),
        "opencorpora" => Some(Ok(opencorpora_export(&corpus))),
        "proiel" => Some(Ok(proiel_export(&corpus))),
        "conllu" => Some(Ok(conllu_export(&corpus))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gr_splits_and_joins() {
        assert_eq!(
            split_gr("S,f,inan=sg,nom"),
            ("S".into(), Some("f,inan=sg,nom".into()))
        );
        assert_eq!(split_gr("NUM=ciph"), ("NUM".into(), Some("=ciph".into())));
        assert_eq!(split_gr("PR"), ("PR".into(), None));
        assert_eq!(
            join_gr(Some("S"), Some("f,inan=sg,nom")).as_deref(),
            Some("S,f,inan=sg,nom")
        );
        assert_eq!(
            join_gr(Some("NUM"), Some("=ciph")).as_deref(),
            Some("NUM=ciph")
        );
    }
}
