//! FictionBook 2 (FB2), the Russian e-book XML in which most
//! Russian literature circulates: import as litogramma and export
//! from it. Both directions live in code rather than in an `.exo`
//! because FB2 needs what the template language deliberately
//! lacks: the book title and author hoisted into `description`,
//! the notes gathered into their own `body`, loose paragraphs
//! wrapped into sections, note callouts numbered.
//!
//! Correspondences (both ways): `section` (titled) ↔ the `#`
//! ladder by depth, `section` (untitled) ↔ the `_` container,
//! `p` ↔ paragraph, `empty-line` ↔ solo `**`, `subtitle` ↔ solo
//! `#_`, `epigraph` ↔ `"/` (text-author ↔ hypograph), `cite` ↔ `"`
//! (text-author ↔ hypograph), `poem` ↔ `~` (title ↔ lemma,
//! stanza ↔ strophe, v ↔ stichos, text-author ↔ hypograph, date
//! ↔ a following solo `-/`), `table` ↔ `+` (pipe rows),
//! `emphasis` ↔ `/`, `strong` ↔ `*`, `strikethrough`/`sub`/`sup` ↔
//! `,` with that genos, `code` ↔ verbatim inline, `a type="note"`
//! ↔ footnote deixis with the note body in `body name="notes"`,
//! `book-title`/`author`/`annotation` ↔ `=`/`=:`/`="`. Recorded
//! loss: images and binaries, genre, lang, dates and the
//! document/publish info, inline styles, external links' text.

use std::cell::Cell;
use std::collections::HashSet;

use crate::dendron::{Annotations, Block, Document, Inline, Strophe};
use crate::endo::{
    Tok, attr, collapse_ws, decode_entities, skip_element, tokenize_xml, trim_inline_edges,
};
use crate::error::{Error, ErrorKind, Result};

/// Elements nested deeper than this are refused rather than
/// recursed into: a debug build spends some 12 KB of stack per
/// level, so this keeps a 2 MB thread safe, and it is far beyond
/// any book (sections nest a handful deep, inline styles fewer).
const MAX_NEST: usize = 64;

fn fb2_err(msg: String) -> Error {
    Error::new(ErrorKind::Syntax(format!("fb2 import: {msg}")))
}

/// A binary carried by an FB2 file: its id (the name the images
/// refer to), content type, and bytes. On import it is meant to
/// be written as `media/<id>` beside the document, which is what
/// the enmedia parameter names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fb2Media {
    pub id: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

// ---------------------------------------------------------------
// Base64 (RFC 4648, standard alphabet), enough for FB2 binaries
// ---------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn content_type_of(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

/// The file name a binary id gets under `media/`: path
/// separators become dashes, so no id can name a path outside
/// that directory (an image href is a file name, never a path);
/// an id that is only dots names nothing. Applied to the image
/// href and to the binary id alike, so the two still meet.
fn media_name(id: &str) -> Option<String> {
    let name: String = id
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect();
    (!name.is_empty() && name != "." && name != "..").then_some(name)
}

/// The onym a note id gets: itself when it is a valid onym, else
/// a renaming that keeps its alphanumerics (`_1` → `fb2-1`), so
/// the callout and the body still meet.
fn note_onym(id: &str) -> Option<String> {
    if crate::sigil::is_valid_onym(id) {
        return Some(id.to_string());
    }
    let mut out = String::from("fb2");
    let mut gap = true;
    for c in id.chars() {
        if c.is_alphanumeric() {
            if gap {
                out.push('-');
                gap = false;
            }
            out.push(c);
        } else {
            gap = true;
        }
    }
    (out.len() > 3).then_some(out)
}

// ---------------------------------------------------------------
// Import
// ---------------------------------------------------------------

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

fn href(attrs: &[(String, String)]) -> Option<&str> {
    attrs
        .iter()
        .find(|(k, _)| k == "l:href" || k == "xlink:href" || k == "href" || k.ends_with(":href"))
        .map(|(_, v)| v.as_str())
}

fn endo(symbol: &str, content: Vec<Inline>, genoses: Vec<String>) -> Inline {
    Inline::Endo {
        symbol: symbol.to_string(),
        content,
        bracket_matching: true,
        ann: Annotations {
            onym: None,
            genoses,
        },
    }
}

/// The section ids a file declares, from a pass over the tokens
/// before parsing: those in the notes bodies (as the onyms their
/// notes get) and those in the main bodies. A link resolves only
/// to an id that exists; the rest stay text.
fn declared_ids(toks: &[Tok]) -> (HashSet<String>, HashSet<String>) {
    let mut notes: HashSet<String> = HashSet::new();
    let mut sections: HashSet<String> = HashSet::new();
    let mut in_notes: Option<bool> = None;
    for tok in toks {
        match tok {
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "body" && !*self_closing => {
                in_notes = Some(attr(attrs, "name").is_some_and(|n| n != "main"));
            }
            Tok::Close(n) if n == "body" => in_notes = None,
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "section" && !*self_closing => {
                if let Some(id) = attr(attrs, "id") {
                    match in_notes {
                        Some(true) => {
                            if let Some(onym) = note_onym(id) {
                                notes.insert(onym);
                            }
                        }
                        Some(false) if crate::sigil::is_valid_onym(id) => {
                            sections.insert(id.to_string());
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    (notes, sections)
}

fn solo(symbol: &str, content: Vec<Inline>) -> Block {
    Block::Paragraph(vec![endo(symbol, content, Vec::new())])
}

fn para(
    symbol: &str,
    lemma: Vec<Inline>,
    children: Vec<Block>,
    hypograph: Vec<Inline>,
    onym: Option<String>,
) -> Block {
    Block::Para {
        symbol: symbol.to_string(),
        taxis: None,
        lemma,
        children,
        hypograph,
        bracket_matching: true,
        ann: Annotations {
            onym,
            genoses: Vec::new(),
        },
    }
}

fn section_symbol(depth: usize) -> &'static str {
    match depth {
        0 | 1 => "#",
        2 => "##",
        3 => "###",
        _ => "####",
    }
}

struct Importer {
    notes: Vec<Block>,
    /// Onyms of the notes the notes bodies declare.
    note_ids: HashSet<String>,
    /// Ids of the sections the main bodies declare.
    section_ids: HashSet<String>,
    /// Current element nesting, bounded by [`MAX_NEST`].
    nest: Cell<usize>,
}

/// Blocks parsed up to a close, with the title and text-author
/// inlines met on the way, and the next token index.
type Parsed = (Vec<Block>, Vec<Inline>, Vec<Inline>, usize);

impl Importer {
    fn enter(&self) -> Result<()> {
        let n = self.nest.get() + 1;
        if n > MAX_NEST {
            return Err(fb2_err(format!(
                "elements nested deeper than {MAX_NEST} levels"
            )));
        }
        self.nest.set(n);
        Ok(())
    }

    fn leave(&self) {
        self.nest.set(self.nest.get() - 1);
    }

    /// Inline content up to the close of `until`.
    fn inlines(&self, toks: &[Tok], i: usize, until: &str) -> Result<(Vec<Inline>, usize)> {
        self.enter()?;
        let parsed = self.inlines_inner(toks, i, until)?;
        self.leave();
        Ok(parsed)
    }

    fn inlines_inner(
        &self,
        toks: &[Tok],
        mut i: usize,
        until: &str,
    ) -> Result<(Vec<Inline>, usize)> {
        let mut out: Vec<Inline> = Vec::new();
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(n) if n == until => {
                    trim_inline_edges(&mut out);
                    return Ok((out, i + 1));
                }
                Tok::Close(_) => i += 1,
                Tok::Text(t) => {
                    let t = collapse_ws(&decode_entities(t));
                    if !t.is_empty() {
                        out.push(Inline::Text(t));
                    }
                    i += 1;
                }
                Tok::Open {
                    name,
                    attrs,
                    self_closing,
                } => {
                    let name = name.clone();
                    if *self_closing {
                        // image, empty-line inside a paragraph: nothing
                        i += 1;
                        continue;
                    }
                    match name.as_str() {
                        "emphasis" => {
                            let (c, next) = self.inlines(toks, i + 1, "emphasis")?;
                            out.push(endo("/", c, Vec::new()));
                            i = next;
                        }
                        "strong" => {
                            let (c, next) = self.inlines(toks, i + 1, "strong")?;
                            out.push(endo("*", c, Vec::new()));
                            i = next;
                        }
                        "strikethrough" | "sub" | "sup" => {
                            let (c, next) = self.inlines(toks, i + 1, &name)?;
                            out.push(endo(",", c, vec![name.clone()]));
                            i = next;
                        }
                        "code" => {
                            let (t, next) = text_until(toks, i + 1, "code");
                            out.push(Inline::VerbatimInline {
                                content: t,
                                ann: Annotations::default(),
                            });
                            i = next;
                        }
                        "a" => {
                            let target = href(attrs).unwrap_or("").to_string();
                            let (c, next) = self.inlines(toks, i + 1, "a")?;
                            if let Some(id) = target.strip_prefix('#') {
                                if attr(attrs, "type") == Some("note") {
                                    // A note callout: the deixis
                                    // replaces the link text when
                                    // the notes body has the note;
                                    // without a body the text stays.
                                    match note_onym(id).filter(|o| self.note_ids.contains(o)) {
                                        Some(onym) => out.push(Inline::Deixis {
                                            symbol: "^".to_string(),
                                            onym,
                                            ann: Annotations::default(),
                                        }),
                                        None => out.extend(c),
                                    }
                                } else if self.section_ids.contains(id) {
                                    // An internal link: a reference
                                    // to the section.
                                    out.push(Inline::Monosim {
                                        symbol: ">".to_string(),
                                        param: id.to_string(),
                                        ann: Annotations::default(),
                                    });
                                } else {
                                    out.extend(c);
                                }
                            } else if !target.is_empty() {
                                // An external link: the autolink
                                // alone when its text is the URL
                                // (the shape the exporter writes),
                                // else the text with the URL after.
                                let mut text = String::new();
                                plain(&c, &mut text);
                                let link =
                                    endo("><", vec![Inline::Text(target.clone())], Vec::new());
                                if c.is_empty() || text.trim() == target {
                                    out.push(link);
                                } else {
                                    out.extend(c);
                                    out.push(Inline::Text(" (".to_string()));
                                    out.push(link);
                                    out.push(Inline::Text(")".to_string()));
                                }
                            } else {
                                out.extend(c);
                            }
                            i = next;
                        }
                        other => {
                            // style and anything else: transparent
                            let (c, next) = self.inlines(toks, i + 1, other)?;
                            out.extend(c);
                            i = next;
                        }
                    }
                }
            }
        }
        trim_inline_edges(&mut out);
        Ok((out, i))
    }

    /// A title element: one or more `p` lines, joined with spaces.
    fn title_inlines(&self, toks: &[Tok], mut i: usize) -> Result<(Vec<Inline>, usize)> {
        let mut out: Vec<Inline> = Vec::new();
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(n) if n == "title" => return Ok((out, i + 1)),
                Tok::Open { name, .. } if name == "p" => {
                    let (c, next) = self.inlines(toks, i + 1, "p")?;
                    if !out.is_empty() && !c.is_empty() {
                        out.push(Inline::Text(" ".to_string()));
                    }
                    out.extend(c);
                    i = next;
                }
                _ => i += 1,
            }
        }
        Ok((out, i))
    }

    /// Blocks up to the close of `until`, at section depth `depth`.
    /// Returns the blocks, the title inlines met (a section's
    /// own), and the text-author inlines met (cite/epigraph/poem).
    fn blocks(&mut self, toks: &[Tok], i: usize, until: &str, depth: usize) -> Result<Parsed> {
        self.enter()?;
        let parsed = self.blocks_inner(toks, i, until, depth)?;
        self.leave();
        Ok(parsed)
    }

    fn blocks_inner(
        &mut self,
        toks: &[Tok],
        mut i: usize,
        until: &str,
        depth: usize,
    ) -> Result<Parsed> {
        let mut blocks: Vec<Block> = Vec::new();
        let mut title: Vec<Inline> = Vec::new();
        let mut author: Vec<Inline> = Vec::new();
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(n) if n == until => return Ok((blocks, title, author, i + 1)),
                Tok::Close(_) | Tok::Text(_) => i += 1,
                Tok::Open {
                    name,
                    attrs,
                    self_closing,
                } => {
                    let name = name.clone();
                    let attrs = attrs.clone();
                    if *self_closing {
                        if name == "empty-line" {
                            // koine's canonical asterism
                            blocks.push(solo("**", vec![Inline::Text(" * * * ".to_string())]));
                        } else if name == "image"
                            && let Some(id) = href(&attrs)
                                .and_then(|h| h.strip_prefix('#'))
                                .and_then(media_name)
                        {
                            // The binary is written as media/<id>
                            // beside the document (the CLI does);
                            // kanonizo bundles it from there.
                            blocks.push(Block::Enmedia {
                                param: format!("media/{id}"),
                            });
                        }
                        i += 1;
                        continue;
                    }
                    match name.as_str() {
                        "title" => {
                            let (t, next) = self.title_inlines(toks, i + 1)?;
                            title = t;
                            i = next;
                        }
                        "p" => {
                            let (c, next) = self.inlines(toks, i + 1, "p")?;
                            if !c.is_empty() {
                                blocks.push(Block::Paragraph(c));
                            }
                            i = next;
                        }
                        "subtitle" => {
                            let (c, next) = self.inlines(toks, i + 1, "subtitle")?;
                            if !c.is_empty() {
                                blocks.push(solo("#_", c));
                            }
                            i = next;
                        }
                        "text-author" => {
                            let (c, next) = self.inlines(toks, i + 1, "text-author")?;
                            if !author.is_empty() && !c.is_empty() {
                                author.push(Inline::Text(" ".to_string()));
                            }
                            author.extend(c);
                            i = next;
                        }
                        "section" => {
                            let onym = attr(&attrs, "id")
                                .filter(|id| crate::sigil::is_valid_onym(id))
                                .map(str::to_string);
                            let (children, lemma, _, next) =
                                self.blocks(toks, i + 1, "section", depth + 1)?;
                            if lemma.is_empty() {
                                blocks.push(para("_", Vec::new(), children, Vec::new(), onym));
                            } else {
                                blocks.push(para(
                                    section_symbol(depth + 1),
                                    lemma,
                                    children,
                                    Vec::new(),
                                    onym,
                                ));
                            }
                            i = next;
                        }
                        "epigraph" | "cite" => {
                            let symbol = if name == "epigraph" { "\"/" } else { "\"" };
                            let (children, _, hyp, next) =
                                self.blocks(toks, i + 1, &name, depth)?;
                            blocks.push(para(symbol, Vec::new(), children, hyp, None));
                            i = next;
                        }
                        "poem" => {
                            let (poem, next) = self.poem(toks, i + 1)?;
                            blocks.extend(poem);
                            i = next;
                        }
                        "table" => {
                            let (table, next) = self.table(toks, i + 1)?;
                            blocks.push(table);
                            i = next;
                        }
                        "annotation" | "image" => {
                            i = skip_element(toks, i + 1, name.clone())?;
                        }
                        other => {
                            // Unknown container: its blocks flow.
                            let (inner, t, a, next) = self.blocks(toks, i + 1, other, depth)?;
                            blocks.extend(inner);
                            if title.is_empty() {
                                title = t;
                            }
                            author.extend(a);
                            i = next;
                        }
                    }
                }
            }
        }
        Ok((blocks, title, author, i))
    }

    fn poem(&mut self, toks: &[Tok], mut i: usize) -> Result<(Vec<Block>, usize)> {
        let mut before: Vec<Block> = Vec::new();
        let mut after: Vec<Block> = Vec::new();
        let mut lemma: Vec<Inline> = Vec::new();
        let mut hypograph: Vec<Inline> = Vec::new();
        let mut strophes: Vec<Strophe> = Vec::new();
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(n) if n == "poem" => {
                    i += 1;
                    break;
                }
                Tok::Open {
                    name, self_closing, ..
                } => {
                    let name = name.clone();
                    if *self_closing {
                        i += 1;
                        continue;
                    }
                    match name.as_str() {
                        "title" => {
                            let (t, next) = self.title_inlines(toks, i + 1)?;
                            lemma = t;
                            i = next;
                        }
                        "epigraph" => {
                            let (children, _, hyp, next) =
                                self.blocks(toks, i + 1, "epigraph", 0)?;
                            before.push(para("\"/", Vec::new(), children, hyp, None));
                            i = next;
                        }
                        "stanza" => {
                            let mut lines: Vec<Vec<Inline>> = Vec::new();
                            i += 1;
                            while i < toks.len() {
                                match &toks[i] {
                                    Tok::Close(n) if n == "stanza" => {
                                        i += 1;
                                        break;
                                    }
                                    Tok::Open { name, .. } if name == "v" => {
                                        let (c, next) = self.inlines(toks, i + 1, "v")?;
                                        lines.push(c);
                                        i = next;
                                    }
                                    Tok::Open {
                                        name, self_closing, ..
                                    } if !*self_closing
                                        && (name == "title" || name == "subtitle") =>
                                    {
                                        // stanza headings: recorded loss
                                        i = skip_element(toks, i + 1, name.clone())?;
                                    }
                                    _ => i += 1,
                                }
                            }
                            if !lines.is_empty() {
                                strophes.push(Strophe(lines));
                            }
                        }
                        "text-author" => {
                            let (c, next) = self.inlines(toks, i + 1, "text-author")?;
                            if !hypograph.is_empty() && !c.is_empty() {
                                hypograph.push(Inline::Text(" ".to_string()));
                            }
                            hypograph.extend(c);
                            i = next;
                        }
                        "date" => {
                            let (c, next) = self.inlines(toks, i + 1, "date")?;
                            if !c.is_empty() {
                                after.push(solo("-/", c));
                            }
                            i = next;
                        }
                        other => {
                            i = skip_element(toks, i + 1, other.to_string())?;
                        }
                    }
                }
                _ => i += 1,
            }
        }
        let mut out = before;
        out.push(Block::Stichoi {
            symbol: Some("~".to_string()),
            taxis: None,
            lemma,
            strophes,
            hypograph,
            bracket_matching: true,
            ann: Annotations::default(),
        });
        out.extend(after);
        Ok((out, i))
    }

    fn table(&mut self, toks: &[Tok], mut i: usize) -> Result<(Block, usize)> {
        let mut lines: Vec<Vec<Inline>> = Vec::new();
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(n) if n == "table" => {
                    i += 1;
                    break;
                }
                Tok::Open { name, .. } if name == "tr" => {
                    let mut line: Vec<Inline> = Vec::new();
                    i += 1;
                    while i < toks.len() {
                        match &toks[i] {
                            Tok::Close(n) if n == "tr" => {
                                i += 1;
                                break;
                            }
                            Tok::Open { name, .. } if name == "td" || name == "th" => {
                                let n = name.clone();
                                let (c, next) = self.inlines(toks, i + 1, &n)?;
                                line.push(Inline::Text("| ".to_string()));
                                line.extend(c);
                                line.push(Inline::Text(" ".to_string()));
                                i = next;
                            }
                            _ => i += 1,
                        }
                    }
                    line.push(Inline::Text("|".to_string()));
                    lines.push(line);
                }
                _ => i += 1,
            }
        }
        Ok((
            Block::Stichoi {
                symbol: Some("+".to_string()),
                taxis: None,
                lemma: Vec::new(),
                strophes: vec![Strophe(lines)],
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations::default(),
            },
            i,
        ))
    }

    /// A notes body: each section with an id is a footnote body.
    fn notes_body(&mut self, toks: &[Tok], mut i: usize) -> Result<usize> {
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(n) if n == "body" => return Ok(i + 1),
                Tok::Open {
                    name,
                    attrs,
                    self_closing,
                } if name == "section" && !*self_closing => {
                    let onym = attr(attrs, "id").and_then(note_onym);
                    let (children, _, _, next) = self.blocks(toks, i + 1, "section", 0)?;
                    if let Some(onym) = onym {
                        self.notes
                            .push(para("^", Vec::new(), children, Vec::new(), Some(onym)));
                    }
                    i = next;
                }
                _ => i += 1,
            }
        }
        Ok(i)
    }
}

/// Import a FictionBook 2 document as litogramma (binaries
/// dropped; see [`fb2_to_document_with_media`]).
pub fn fb2_to_document(xml: &str) -> Result<Document> {
    fb2_to_document_with_media(xml).map(|(doc, _)| doc)
}

/// Import a FictionBook 2 document as litogramma, returning the
/// binaries its images refer to. Each image block names
/// `media/<id>`; write the binaries there for kanonizo to bundle.
pub fn fb2_to_document_with_media(xml: &str) -> Result<(Document, Vec<Fb2Media>)> {
    let toks = tokenize_xml(xml)?;
    let mut media: Vec<Fb2Media> = Vec::new();
    let (note_ids, section_ids) = declared_ids(&toks);
    let mut imp = Importer {
        notes: Vec::new(),
        note_ids,
        section_ids,
        nest: Cell::new(0),
    };
    let mut title: Vec<Inline> = Vec::new();
    let mut authors: Vec<Vec<Inline>> = Vec::new();
    let mut annotation: Vec<Block> = Vec::new();
    let mut body: Vec<Block> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Open {
                name, self_closing, ..
            } if name == "title-info" && !*self_closing => {
                i += 1;
                while i < toks.len() {
                    match &toks[i] {
                        Tok::Close(n) if n == "title-info" => {
                            i += 1;
                            break;
                        }
                        Tok::Open {
                            name, self_closing, ..
                        } if !*self_closing => match name.as_str() {
                            "book-title" => {
                                let (c, next) = imp.inlines(&toks, i + 1, "book-title")?;
                                title = c;
                                i = next;
                            }
                            "author" => {
                                let mut parts: Vec<String> = Vec::new();
                                i += 1;
                                while i < toks.len() {
                                    match &toks[i] {
                                        Tok::Close(n) if n == "author" => {
                                            i += 1;
                                            break;
                                        }
                                        Tok::Open {
                                            name, self_closing, ..
                                        } if !*self_closing
                                            && matches!(
                                                name.as_str(),
                                                "first-name"
                                                    | "middle-name"
                                                    | "last-name"
                                                    | "nickname"
                                            ) =>
                                        {
                                            let n = name.clone();
                                            let (t, next) = text_until(&toks, i + 1, &n);
                                            let t = collapse_ws(&t).trim().to_string();
                                            if !t.is_empty() {
                                                parts.push(t);
                                            }
                                            i = next;
                                        }
                                        _ => i += 1,
                                    }
                                }
                                if !parts.is_empty() {
                                    authors.push(vec![Inline::Text(parts.join(" "))]);
                                }
                            }
                            "annotation" => {
                                let (blocks, _, _, next) =
                                    imp.blocks(&toks, i + 1, "annotation", 0)?;
                                annotation = blocks;
                                i = next;
                            }
                            other => {
                                i = skip_element(&toks, i + 1, other.to_string())?;
                            }
                        },
                        _ => i += 1,
                    }
                }
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "binary" && !*self_closing => {
                let id = attr(attrs, "id").unwrap_or("").to_string();
                let content_type = attr(attrs, "content-type")
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let (text, next) = text_until(&toks, i + 1, "binary");
                if let Some(id) = media_name(&id)
                    && let Some(bytes) = base64_decode(&text)
                {
                    media.push(Fb2Media {
                        id,
                        content_type,
                        bytes,
                    });
                }
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if matches!(
                name.as_str(),
                "document-info" | "publish-info" | "custom-info" | "stylesheet"
            ) && !*self_closing =>
            {
                i = skip_element(&toks, i + 1, name.clone())?;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "body" && !*self_closing => {
                if attr(attrs, "name").is_some_and(|n| n != "main") {
                    i = imp.notes_body(&toks, i + 1)?;
                    continue;
                }
                // The main body: an optional title (the book's
                // when the description gave none), epigraphs, and
                // sections.
                let (mut blocks, body_title, _, next) = imp.blocks(&toks, i + 1, "body", 0)?;
                if title.is_empty() {
                    title = body_title;
                }
                // A lone untitled section holding no sections is
                // the body's wrapper around loose paragraphs (the
                // shape the exporter writes): it unwraps. One that
                // holds sections is structure and stays a container.
                if blocks.len() == 1
                    && let Some(Block::Para {
                        symbol,
                        lemma,
                        ann,
                        children,
                        ..
                    }) = blocks.first()
                    && symbol == "_"
                    && lemma.is_empty()
                    && ann.onym.is_none()
                    && !children.iter().any(is_section)
                    && let Some(Block::Para { children, .. }) = blocks.pop()
                {
                    blocks = children;
                }
                body.extend(blocks);
                i = next;
            }
            _ => i += 1,
        }
    }
    let mut blocks: Vec<Block> = Vec::new();
    if !title.is_empty() {
        blocks.push(solo("=", title));
    }
    for author in authors {
        blocks.push(solo("=:", author));
    }
    if !annotation.is_empty() {
        blocks.push(para("=\"", Vec::new(), annotation, Vec::new(), None));
    }
    blocks.extend(body);
    blocks.extend(imp.notes);
    Ok((
        Document {
            dialect_id: "litogramma".to_string(),
            dialect_version: None,
            blocks,
        },
        media,
    ))
}

// ---------------------------------------------------------------
// Export
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

fn plain(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                plain(content, out)
            }
            Inline::VerbatimInline { content, .. } => out.push_str(content),
            _ => {}
        }
    }
}

const NOTE_SYMBOLS: &[&str] = &["^", "^^", "^^^", "^!", "|"];
const SECTION_SYMBOLS: &[&str] = &[
    "==", "===", "#", "##", "###", "####", "_", "_=", "_-", "_:", "_/", ":=", ":#",
];

struct Exporter<'a> {
    /// Note onym -> its number, in document order.
    notes: std::collections::HashMap<&'a str, usize>,
    /// Enmedia parameters met, in order (their binaries follow
    /// the bodies).
    images: std::cell::RefCell<Vec<String>>,
}

/// The id an enmedia parameter gets in FB2: its file name.
fn image_id(param: &str) -> &str {
    param.rsplit('/').next().unwrap_or(param)
}

impl<'a> Exporter<'a> {
    fn inlines(&self, inlines: &[Inline], out: &mut String) {
        for inline in inlines {
            self.inline(inline, out);
        }
    }

    fn inline(&self, inline: &Inline, out: &mut String) {
        match inline {
            Inline::Text(t) => out.push_str(&esc(t)),
            Inline::Endo {
                symbol,
                content,
                ann,
                ..
            } => match symbol.as_str() {
                "/" | ":(" => {
                    out.push_str("<emphasis>");
                    self.inlines(content, out);
                    out.push_str("</emphasis>");
                }
                "*" | "#_" => {
                    out.push_str("<strong>");
                    self.inlines(content, out);
                    out.push_str("</strong>");
                }
                "*/" => {
                    out.push_str("<strong><emphasis>");
                    self.inlines(content, out);
                    out.push_str("</emphasis></strong>");
                }
                "," => {
                    let tag = ann
                        .genoses
                        .iter()
                        .find(|g| matches!(g.as_str(), "strikethrough" | "sub" | "sup"))
                        .cloned();
                    match tag {
                        Some(tag) => {
                            out.push_str(&format!("<{tag}>"));
                            self.inlines(content, out);
                            out.push_str(&format!("</{tag}>"));
                        }
                        None => self.inlines(content, out),
                    }
                }
                "><" => {
                    let mut url = String::new();
                    plain(content, &mut url);
                    out.push_str(&format!("<a l:href=\"{u}\">{u}</a>", u = esc(&url)));
                }
                "%%" => {}
                _ => self.inlines(content, out),
            },
            Inline::VerbatimInline { content, .. } => {
                out.push_str("<code>");
                out.push_str(&esc(content));
                out.push_str("</code>");
            }
            Inline::EndoDiaphane { content, .. } => self.inlines(content, out),
            Inline::Deixis { onym, .. } => {
                if let Some(n) = self.notes.get(onym.as_str()) {
                    out.push_str(&format!(
                        "<a l:href=\"#{}\" type=\"note\">[{n}]</a>",
                        esc(onym)
                    ));
                }
            }
            Inline::Monosim { symbol, param, .. } if symbol == ">" => {
                // A reference: the link the HTML exo writes for it.
                out.push_str(&format!("<a l:href=\"#{p}\">[{p}]</a>", p = esc(param)));
            }
            Inline::Monosim { .. }
            | Inline::OnymAnchor(_)
            | Inline::Milestone { .. }
            | Inline::EndoAxioma { .. }
            | Inline::AxiomaRef { .. } => {}
        }
    }

    fn stichoi_poem(
        &self,
        lemma: &[Inline],
        strophes: &[Strophe],
        hypograph: &[Inline],
        out: &mut String,
    ) {
        out.push_str("<poem>\n");
        if !lemma.is_empty() {
            out.push_str("<title><p>");
            self.inlines(lemma, out);
            out.push_str("</p></title>\n");
        }
        for Strophe(lines) in strophes {
            out.push_str("<stanza>\n");
            for line in lines {
                out.push_str("<v>");
                self.inlines(line, out);
                out.push_str("</v>\n");
            }
            out.push_str("</stanza>\n");
        }
        if !hypograph.is_empty() {
            out.push_str("<text-author>");
            self.inlines(hypograph, out);
            out.push_str("</text-author>\n");
        }
        out.push_str("</poem>\n");
    }

    fn stichoi_table(&self, strophes: &[Strophe], out: &mut String) {
        out.push_str("<table>\n");
        for Strophe(lines) in strophes {
            for line in lines {
                let mut text = String::new();
                plain(line, &mut text);
                let cells: Vec<&str> = text
                    .trim()
                    .trim_matches('|')
                    .split('|')
                    .map(str::trim)
                    .collect();
                if cells
                    .iter()
                    .all(|c| c.chars().all(|ch| ch == '-' || ch == ' '))
                {
                    continue;
                }
                out.push_str("<tr>");
                for cell in cells {
                    out.push_str(&format!("<td>{}</td>", esc(cell)));
                }
                out.push_str("</tr>\n");
            }
        }
        out.push_str("</table>\n");
    }

    /// Blocks inside a section (or a cite/epigraph).
    fn blocks(&self, blocks: &[Block], out: &mut String) {
        for block in blocks {
            self.block(block, out);
        }
    }

    fn block(&self, block: &Block, out: &mut String) {
        match block {
            Block::Paragraph(inlines) => {
                if let [
                    Inline::Endo {
                        symbol, content, ..
                    },
                ] = inlines.as_slice()
                {
                    match symbol.as_str() {
                        "**" => {
                            out.push_str("<empty-line/>\n");
                            return;
                        }
                        "#_" => {
                            out.push_str("<subtitle>");
                            self.inlines(content, out);
                            out.push_str("</subtitle>\n");
                            return;
                        }
                        "=" | "=:" | "=_" | "=;" | "=#=" => return,
                        _ => {}
                    }
                }
                out.push_str("<p>");
                self.inlines(inlines, out);
                out.push_str("</p>\n");
            }
            Block::Para {
                symbol,
                lemma,
                children,
                hypograph,
                ann,
                ..
            } => {
                let symbol = symbol.as_str();
                if NOTE_SYMBOLS.contains(&symbol) || symbol == "=\"" {
                    // notes are gathered into their own body; the
                    // abstract went into the description
                    return;
                }
                if SECTION_SYMBOLS.contains(&symbol) {
                    out.push_str("<section");
                    if let Some(o) = &ann.onym {
                        out.push_str(&format!(" id=\"{}\"", esc(o)));
                    }
                    out.push_str(">\n");
                    if !lemma.is_empty() {
                        out.push_str("<title><p>");
                        self.inlines(lemma, out);
                        out.push_str("</p></title>\n");
                    }
                    self.blocks(children, out);
                    out.push_str("</section>\n");
                    return;
                }
                match symbol {
                    "\"" | "\"/" => {
                        let tag = if symbol == "\"" { "cite" } else { "epigraph" };
                        out.push_str(&format!("<{tag}>\n"));
                        self.blocks(children, out);
                        if !hypograph.is_empty() {
                            out.push_str("<text-author>");
                            self.inlines(hypograph, out);
                            out.push_str("</text-author>\n");
                        }
                        out.push_str(&format!("</{tag}>\n"));
                    }
                    ":" | ":!!" | "::" => {
                        // a speech, a character, a term: the label
                        // opens the first paragraph
                        let mut first = true;
                        if children.is_empty() {
                            out.push_str("<p><strong>");
                            self.inlines(lemma, out);
                            out.push_str("</strong></p>\n");
                        }
                        for child in children {
                            if first
                                && !lemma.is_empty()
                                && let Block::Paragraph(inlines) = child
                            {
                                out.push_str("<p><strong>");
                                self.inlines(lemma, out);
                                out.push_str("</strong> ");
                                self.inlines(inlines, out);
                                out.push_str("</p>\n");
                                first = false;
                                continue;
                            }
                            first = false;
                            self.block(child, out);
                        }
                    }
                    ":[" => {
                        for child in children {
                            if let Block::Paragraph(inlines) = child {
                                out.push_str("<p><emphasis>");
                                self.inlines(inlines, out);
                                out.push_str("</emphasis></p>\n");
                            } else {
                                self.block(child, out);
                            }
                        }
                    }
                    _ => {
                        // lists, items, definitions, figures, math,
                        // containers without a section meaning:
                        // the lemma as a paragraph, then the content
                        if !lemma.is_empty() {
                            out.push_str("<p>");
                            self.inlines(lemma, out);
                            out.push_str("</p>\n");
                        }
                        self.blocks(children, out);
                        if !hypograph.is_empty() {
                            out.push_str("<p>");
                            self.inlines(hypograph, out);
                            out.push_str("</p>\n");
                        }
                    }
                }
            }
            Block::Stichoi {
                symbol,
                lemma,
                strophes,
                hypograph,
                ..
            } => match symbol.as_deref() {
                Some("+") => self.stichoi_table(strophes, out),
                _ => self.stichoi_poem(lemma, strophes, hypograph, out),
            },
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                self.blocks(children, out)
            }
            Block::VerbatimBlock { content, .. } => {
                for line in content.lines() {
                    out.push_str(&format!("<p><code>{}</code></p>\n", esc(line)));
                }
            }
            Block::Enmedia { param } => {
                out.push_str(&format!("<image l:href=\"#{}\"/>\n", esc(image_id(param))));
                self.images.borrow_mut().push(param.clone());
            }
            Block::EnmediaHashed { .. }
            | Block::AnaphorEnglossis { .. }
            | Block::AnaphorEnlexis { .. }
            | Block::ParaAxioma { .. }
            | Block::AxiomaRefBlock { .. } => {}
        }
    }
}

fn is_section(block: &Block) -> bool {
    matches!(block, Block::Para { symbol, .. } if SECTION_SYMBOLS.contains(&symbol.as_str()))
}

/// Onymized note bodies at any depth, in document order.
fn collect_notes<'a>(blocks: &'a [Block], notes: &mut Vec<(&'a str, &'a [Block])>) {
    for block in blocks {
        match block {
            Block::Para {
                symbol,
                children,
                ann,
                ..
            } => {
                if NOTE_SYMBOLS.contains(&symbol.as_str()) {
                    if let Some(o) = &ann.onym {
                        notes.push((o.as_str(), children));
                    }
                } else {
                    collect_notes(children, notes);
                }
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                collect_notes(children, notes);
            }
            _ => {}
        }
    }
}

fn is_front_or_note(block: &Block) -> bool {
    match block {
        Block::Paragraph(inlines) => {
            matches!(inlines.as_slice(), [Inline::Endo { symbol, .. }] if matches!(symbol.as_str(), "=" | "=:" | "=_" | "=;" | "=#="))
        }
        Block::Para { symbol, .. } => NOTE_SYMBOLS.contains(&symbol.as_str()) || symbol == "=\"",
        _ => false,
    }
}

/// Export a litogramma document as FictionBook 2, without
/// binaries (images keep their references).
pub fn document_to_fb2(doc: &Document) -> String {
    document_to_fb2_with(doc, &|_| None)
}

/// Export a litogramma document as FictionBook 2; `read_media`
/// maps an enmedia parameter (`media/m1.png`) to its bytes, which
/// become `binary` elements after the bodies.
pub fn document_to_fb2_with(
    doc: &Document,
    read_media: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> String {
    // Front matter and notes (note bodies may sit inside a
    // division; they are gathered from any depth).
    let mut title: Option<&[Inline]> = None;
    let mut authors: Vec<&[Inline]> = Vec::new();
    let mut annotation: Option<&[Block]> = None;
    let mut notes: Vec<(&str, &[Block])> = Vec::new();
    for block in &doc.blocks {
        match block {
            Block::Paragraph(inlines) => {
                if let [
                    Inline::Endo {
                        symbol, content, ..
                    },
                ] = inlines.as_slice()
                {
                    match symbol.as_str() {
                        "=" if title.is_none() => title = Some(content),
                        "=:" => authors.push(content),
                        _ => {}
                    }
                }
            }
            Block::Para {
                symbol, children, ..
            } if symbol == "=\"" && annotation.is_none() => {
                annotation = Some(children);
            }
            _ => {}
        }
    }
    collect_notes(&doc.blocks, &mut notes);
    let exporter = Exporter {
        notes: notes
            .iter()
            .enumerate()
            .map(|(i, (o, _))| (*o, i + 1))
            .collect(),
        images: std::cell::RefCell::new(Vec::new()),
    };
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<FictionBook xmlns=\"http://www.gribuser.ru/xml/fictionbook/2.0\" xmlns:l=\"http://www.w3.org/1999/xlink\">\n<description>\n<title-info>\n",
    );
    for a in authors {
        let mut name = String::new();
        plain(a, &mut name);
        let name = name.trim();
        out.push_str("<author>");
        match name.rsplit_once(' ') {
            Some((first, last)) => out.push_str(&format!(
                "<first-name>{}</first-name><last-name>{}</last-name>",
                esc(first),
                esc(last)
            )),
            None => out.push_str(&format!("<nickname>{}</nickname>", esc(name))),
        }
        out.push_str("</author>\n");
    }
    if let Some(t) = title {
        out.push_str("<book-title>");
        exporter.inlines(t, &mut out);
        out.push_str("</book-title>\n");
    }
    if let Some(blocks) = annotation {
        out.push_str("<annotation>\n");
        exporter.blocks(blocks, &mut out);
        out.push_str("</annotation>\n");
    }
    out.push_str("</title-info>\n</description>\n<body>\n");
    if let Some(t) = title {
        out.push_str("<title><p>");
        exporter.inlines(t, &mut out);
        out.push_str("</p></title>\n");
    }
    // Body: sections as they are; runs of loose blocks wrapped in
    // an untitled section (FB2 allows no paragraph directly in
    // the body).
    let body: Vec<&Block> = doc.blocks.iter().filter(|b| !is_front_or_note(b)).collect();
    let mut i = 0;
    while i < body.len() {
        if is_section(body[i]) {
            exporter.block(body[i], &mut out);
            i += 1;
            continue;
        }
        out.push_str("<section>\n");
        while i < body.len() && !is_section(body[i]) {
            exporter.block(body[i], &mut out);
            i += 1;
        }
        out.push_str("</section>\n");
    }
    out.push_str("</body>\n");
    if !notes.is_empty() {
        out.push_str("<body name=\"notes\">\n");
        for (n, (onym, children)) in notes.iter().enumerate() {
            out.push_str(&format!(
                "<section id=\"{}\">\n<title><p>{}</p></title>\n",
                esc(onym),
                n + 1
            ));
            exporter.blocks(children, &mut out);
            out.push_str("</section>\n");
        }
        out.push_str("</body>\n");
    }
    for param in exporter.images.borrow().iter() {
        if let Some(bytes) = read_media(param) {
            let id = image_id(param);
            out.push_str(&format!(
                "<binary id=\"{}\" content-type=\"{}\">{}</binary>\n",
                esc(id),
                content_type_of(id),
                base64_encode(&bytes)
            ));
        }
    }
    out.push_str("</FictionBook>\n");
    out
}
