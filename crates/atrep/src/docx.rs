//! Word (`.docx`) -> litogramma.
//!
//! A Word document is an OPC package: a zip of XML parts. The
//! import reads the declared structure and nothing else — no
//! style-name guessing, no geometry — the way the text level's
//! own reader does:
//!
//! - A paragraph is a heading iff it carries an outline level,
//!   directly (`w:outlineLvl`) or through its style chain
//!   (`styles.xml`, along `w:basedOn`); levels nest as the koine
//!   section ladder (`#` .. `####`). The `Title`, `Subtitle`,
//!   `Author` and `Abstract` styles are the front matter.
//! - Lists come from declared numbering (`w:numPr`), ordered or
//!   unordered by `numbering.xml`'s `w:numFmt`, nested by `w:ilvl`.
//! - Block quotes are the closed set of the major producers'
//!   declared style ids (`Quote`, `IntenseQuote`, pandoc's
//!   `BlockText`, LibreOffice's `Quotations`), resolved through
//!   `w:basedOn`; `SourceCode` is a verbatim block.
//! - Bold and italic runs (`w:b`, `w:i`, or the `Strong` /
//!   `Emphasis` character styles) are koine strong and emphasis;
//!   `VerbatimChar` is inline verbatim.
//! - Footnotes and endnotes are first class: the reference is a
//!   deixis, the body (from `footnotes.xml` / `endnotes.xml`)
//!   lands in the notes region at the document's end. A comment
//!   (`w:commentReference`, body in `comments.xml`) is a
//!   manuscript note, the same way.
//! - Tracked changes read as the accepted view: `w:ins` content
//!   is text, `w:del` is skipped.
//! - Fields: an `XE "term"` instruction is a hidden index mark;
//!   a `CITATION tag` is a cite, its cached result the cite's
//!   span; a `BIBLIOGRAPHY` field's generated listing is dropped;
//!   every other field keeps its cached result as text.
//! - The `b:Sources` custom XML part is the bibliography: one
//!   bibliogramma entry per source, campi named in English
//!   (bibliogramma canonicalizes), the genus from `b:SourceType`.
//! - A table is the table model: rows (`w:tblHeader` rows as
//!   header rows), cells holding blocks, `w:gridSpan` and
//!   `w:vMerge` as the span monosims, the merged-away cells
//!   omitted.
//! - Images (`w:drawing`, `a:blip r:embed`) are enmedia blocks
//!   named `media/<file>`, the bytes returned beside the document;
//!   a caption paragraph after one makes a figure.
//! - Hyperlinks are koine links (external) or refs to bookmark
//!   anchors (internal). `docProps/core.xml` supplies the title
//!   and creator when the body declares none.
//! - Out of scope, dropped: headers, footers, text boxes, section
//!   and page geometry, run styling beyond bold and italic.

use std::collections::{HashMap, HashSet};
use std::io::Read;

use crate::dendron::{Annotations, Block, Document, Inline, Taxis};
use crate::endo::{Tok, decode_entities, tokenize_xml, trim_inline_edges};

/// An attribute by name. The tokenizer lowercases attribute names
/// (the HTML rule), so a camel-cased OOXML name is looked up in
/// lower case.
fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let lower = name.to_ascii_lowercase();
    attrs
        .iter()
        .find(|(n, _)| *n == lower)
        .map(|(_, v)| v.as_str())
}
use crate::error::{Error, ErrorKind, Result};

/// A media file of the document: `name` is the file name under
/// `media/` the enmedia blocks refer to.
#[derive(Debug, Clone)]
pub struct DocxMedia {
    pub name: String,
    pub bytes: Vec<u8>,
}

fn docx_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("docx import: {msg}")))
}

// ---------------------------------------------------------------
// The package
// ---------------------------------------------------------------

struct Package {
    parts: HashMap<String, Vec<u8>>,
}

fn read_package(bytes: &[u8]) -> Result<Package> {
    let cursor = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| docx_err(format!("not a zip: {e}")))?;
    let mut parts = HashMap::new();
    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| docx_err(format!("zip entry {i}: {e}")))?;
        if file.is_dir() {
            continue;
        }
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| docx_err(format!("{}: {e}", file.name())))?;
        parts.insert(file.name().to_string(), data);
    }
    if !parts.contains_key("word/document.xml") {
        return Err(docx_err(
            "word/document.xml is missing: not a Word document".to_string(),
        ));
    }
    Ok(Package { parts })
}

impl Package {
    fn text(&self, name: &str) -> Option<String> {
        self.parts
            .get(name)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
    fn toks(&self, name: &str) -> Result<Vec<Tok>> {
        match self.text(name) {
            Some(xml) => tokenize_xml(&xml),
            None => Ok(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------
// Declarations: styles, numbering, relationships
// ---------------------------------------------------------------

#[derive(Default)]
struct Styles {
    outline: HashMap<String, u8>,
    based_on: HashMap<String, String>,
    ids: HashSet<String>,
}

impl Styles {
    fn parse(toks: &[Tok]) -> Styles {
        let mut st = Styles::default();
        let mut current: Option<String> = None;
        for tok in toks {
            match tok {
                Tok::Open { name, attrs, .. } if name == "w:style" => {
                    let id = attr(attrs, "w:styleId").unwrap_or("").to_string();
                    st.ids.insert(id.clone());
                    current = Some(id);
                }
                Tok::Close(name) if name == "w:style" => current = None,
                Tok::Open { name, attrs, .. } if name == "w:basedOn" => {
                    if let (Some(id), Some(v)) = (&current, attr(attrs, "w:val")) {
                        st.based_on.insert(id.clone(), v.to_string());
                    }
                }
                Tok::Open { name, attrs, .. } if name == "w:outlineLvl" => {
                    if let (Some(id), Some(v)) = (&current, attr(attrs, "w:val"))
                        && let Ok(n) = v.parse::<u8>()
                    {
                        st.outline.insert(id.clone(), n);
                    }
                }
                _ => {}
            }
        }
        st
    }

    /// The style and its ancestors, nearest first.
    fn chain<'a>(&'a self, id: &'a str) -> Vec<&'a str> {
        let mut out = vec![id];
        let mut cur = id;
        for _ in 0..16 {
            match self.based_on.get(cur) {
                Some(next) if !out.contains(&next.as_str()) => {
                    out.push(next);
                    cur = next;
                }
                _ => break,
            }
        }
        out
    }

    fn outline(&self, id: &str) -> Option<u8> {
        self.chain(id)
            .into_iter()
            .find_map(|s| self.outline.get(s).copied())
    }

    /// The nearest declared kind on the style chain: a style based
    /// on Title that is Author is an author line, not a title.
    fn kind(&self, id: &str) -> Option<&'static str> {
        const KINDS: &[&str] = &[
            "Title",
            "Subtitle",
            "Author",
            "Abstract",
            "SourceCode",
            "Caption",
            "ImageCaption",
            "TableCaption",
            "Quote",
            "IntenseQuote",
            "BlockText",
            "Quotations",
        ];
        self.chain(id)
            .into_iter()
            .find_map(|s| KINDS.iter().find(|k| **k == s).copied())
    }
}

#[derive(Default)]
struct Numbering {
    num_to_abstract: HashMap<String, String>,
    /// abstractNumId -> numFmt per level.
    levels: HashMap<String, HashMap<u8, String>>,
}

impl Numbering {
    fn parse(toks: &[Tok]) -> Numbering {
        let mut nb = Numbering::default();
        let mut abstract_id: Option<String> = None;
        let mut level: Option<u8> = None;
        let mut num_id: Option<String> = None;
        for tok in toks {
            match tok {
                Tok::Open { name, attrs, .. } if name == "w:abstractNum" => {
                    abstract_id = attr(attrs, "w:abstractNumId").map(str::to_string);
                }
                Tok::Close(name) if name == "w:abstractNum" => abstract_id = None,
                Tok::Open { name, attrs, .. } if name == "w:lvl" => {
                    level = attr(attrs, "w:ilvl").and_then(|v| v.parse().ok());
                }
                Tok::Close(name) if name == "w:lvl" => level = None,
                Tok::Open { name, attrs, .. } if name == "w:numFmt" => {
                    if let (Some(a), Some(l), Some(v)) = (&abstract_id, level, attr(attrs, "w:val"))
                    {
                        nb.levels
                            .entry(a.clone())
                            .or_default()
                            .insert(l, v.to_string());
                    }
                }
                Tok::Open { name, attrs, .. } if name == "w:num" => {
                    num_id = attr(attrs, "w:numId").map(str::to_string);
                }
                Tok::Close(name) if name == "w:num" => num_id = None,
                Tok::Open { name, attrs, .. } if name == "w:abstractNumId" => {
                    if let (Some(n), Some(v)) = (&num_id, attr(attrs, "w:val")) {
                        nb.num_to_abstract.insert(n.clone(), v.to_string());
                    }
                }
                _ => {}
            }
        }
        nb
    }

    /// Ordered when the level's format is a numbering; unresolvable
    /// numbering reads as unordered.
    fn ordered(&self, num_id: &str, ilvl: u8) -> bool {
        self.num_to_abstract
            .get(num_id)
            .and_then(|a| self.levels.get(a))
            .and_then(|levels| levels.get(&ilvl))
            .is_some_and(|fmt| fmt != "bullet" && fmt != "none")
    }
}

#[derive(Default)]
struct Rels {
    /// rId -> (target, external)
    targets: HashMap<String, (String, bool)>,
}

impl Rels {
    fn parse(toks: &[Tok]) -> Rels {
        let mut rels = Rels::default();
        for tok in toks {
            if let Tok::Open { name, attrs, .. } = tok
                && name == "Relationship"
                && let (Some(id), Some(target)) = (attr(attrs, "Id"), attr(attrs, "Target"))
            {
                let external = attr(attrs, "TargetMode") == Some("External");
                rels.targets
                    .insert(id.to_string(), (target.to_string(), external));
            }
        }
        rels
    }
}

// ---------------------------------------------------------------
// The reading
// ---------------------------------------------------------------

/// What a paragraph declares itself to be.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParaKind {
    Plain,
    Heading(u8),
    Title,
    Subtitle,
    Author,
    Abstract,
    Quote,
    Code,
    Caption,
    ListItem { ordered: bool, level: u8 },
}

/// One paragraph as read: its kind, its inlines, and the blocks
/// it carries beside itself (an image).
struct Para {
    kind: ParaKind,
    inlines: Vec<Inline>,
    extras: Vec<Block>,
}

/// One block as read, before the sections and lists are built.
enum Item {
    Para(Para),
    Table(Block),
}

/// A field being read: the instruction, and the cached result
/// collected while inside it.
struct Field {
    instr: String,
    in_result: bool,
    result: Vec<Inline>,
}

struct Ctx<'p> {
    pkg: &'p Package,
    styles: Styles,
    numbering: Numbering,
    rels: Rels,
    /// Note bodies by id, read from their parts on demand.
    footnotes: HashMap<String, Vec<Block>>,
    endnotes: HashMap<String, Vec<Block>>,
    comments: HashMap<String, Vec<Block>>,
    /// The notes referenced, in order of first reference:
    /// (symbol, onym, body).
    notes: Vec<(String, String, Vec<Block>)>,
    /// "symbol:id" -> onym, for a note referenced twice.
    note_onyms: HashMap<String, String>,
    media: Vec<DocxMedia>,
    media_names: HashSet<String>,
    field: Option<Field>,
    /// Text runs inside a BIBLIOGRAPHY field's result are dropped
    /// until the field ends, across paragraphs.
    dropping: bool,
}

/// A bookmark name as an onym: alphanumeric runs joined by
/// hyphens, a leading non-alphanumeric run dropped.
fn onym_of(name: &str) -> Option<String> {
    let mut out = String::new();
    let mut gap = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    crate::sigil::is_valid_onym(&out).then_some(out)
}

fn endo(symbol: &str, content: Vec<Inline>) -> Inline {
    Inline::Endo {
        symbol: symbol.to_string(),
        content,
        bracket_matching: true,
        ann: Annotations::default(),
    }
}

fn mono(symbol: &str, param: &str) -> Inline {
    Inline::Monosim {
        symbol: symbol.to_string(),
        param: param.to_string(),
        ann: Annotations::default(),
    }
}

fn para_block(
    symbol: &str,
    lemma: Vec<Inline>,
    children: Vec<Block>,
    onym: Option<String>,
) -> Block {
    Block::Para {
        symbol: symbol.to_string(),
        taxis: None,
        lemma,
        children,
        hypograph: Vec::new(),
        bracket_matching: true,
        ann: Annotations {
            onym,
            genoses: Vec::new(),
        },
    }
}

fn solo(symbol: &str, content: Vec<Inline>) -> Block {
    Block::Paragraph(vec![endo(symbol, content)])
}

/// Run formatting in force.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Fmt {
    bold: bool,
    italic: bool,
    code: bool,
}

impl<'p> Ctx<'p> {
    fn new(pkg: &'p Package) -> Result<Self> {
        let styles = Styles::parse(&pkg.toks("word/styles.xml")?);
        let numbering = Numbering::parse(&pkg.toks("word/numbering.xml")?);
        let rels = Rels::parse(&pkg.toks("word/_rels/document.xml.rels")?);
        Ok(Ctx {
            pkg,
            styles,
            numbering,
            rels,
            footnotes: HashMap::new(),
            endnotes: HashMap::new(),
            comments: HashMap::new(),
            notes: Vec::new(),
            note_onyms: HashMap::new(),
            media: Vec::new(),
            media_names: HashSet::new(),
            field: None,
            dropping: false,
        })
    }

    /// Read a notes part (footnotes, endnotes, comments): id ->
    /// body blocks, the reference-mark runs skipped.
    fn read_notes(&mut self, part: &str, element: &str) -> Result<HashMap<String, Vec<Block>>> {
        let toks = self.pkg.toks(part)?;
        let mut out = HashMap::new();
        let mut i = 0;
        while i < toks.len() {
            match &toks[i] {
                Tok::Open {
                    name,
                    attrs,
                    self_closing: false,
                } if name == element => {
                    let ty = attr(attrs, "w:type").unwrap_or("");
                    let id = attr(attrs, "w:id").unwrap_or("").to_string();
                    let (items, next) = self.read_blocks(&toks, i + 1, element)?;
                    if ty.is_empty() || ty == "normal" {
                        out.insert(id, assemble(items));
                    }
                    i = next;
                }
                _ => i += 1,
            }
        }
        Ok(out)
    }

    /// A note reference: the body is fetched, registered in order
    /// of first reference, and the callout returned.
    fn note(&mut self, kind: &str, id: &str) -> Result<Option<Inline>> {
        let (symbol, prefix, part, element, store) = match kind {
            "footnote" => ("^", "f", "word/footnotes.xml", "w:footnote", 0),
            "endnote" => ("^^^", "e", "word/endnotes.xml", "w:endnote", 1),
            _ => ("^!", "c", "word/comments.xml", "w:comment", 2),
        };
        let loaded = match store {
            0 => !self.footnotes.is_empty(),
            1 => !self.endnotes.is_empty(),
            _ => !self.comments.is_empty(),
        };
        if !loaded {
            let bodies = self.read_notes(part, element)?;
            match store {
                0 => self.footnotes = bodies,
                1 => self.endnotes = bodies,
                _ => self.comments = bodies,
            }
        }
        let bodies = match store {
            0 => &self.footnotes,
            1 => &self.endnotes,
            _ => &self.comments,
        };
        let Some(body) = bodies.get(id) else {
            return Ok(None);
        };
        // One onym per note, in order of first reference, numbered
        // per family (f1, e1, c1), whatever the part's own ids.
        let key = format!("{symbol}:{id}");
        let onym = match self.note_onyms.get(&key) {
            Some(o) => o.clone(),
            None => {
                let n = self.notes.iter().filter(|(s, _, _)| *s == symbol).count() + 1;
                let onym = format!("{prefix}{n}");
                self.note_onyms.insert(key, onym.clone());
                self.notes
                    .push((symbol.to_string(), onym.clone(), body.clone()));
                onym
            }
        };
        Ok(Some(Inline::Deixis {
            symbol: symbol.to_string(),
            onym,
            ann: Annotations::default(),
        }))
    }

    /// An image relationship: the media file registered, its
    /// enmedia name returned.
    fn image(&mut self, rid: &str) -> Option<String> {
        let (target, external) = self.rels.targets.get(rid)?.clone();
        if external {
            return None;
        }
        let part = format!(
            "word/{}",
            target.trim_start_matches("/word/").trim_start_matches('/')
        );
        let bytes = self.pkg.parts.get(&part)?.clone();
        let base = target.rsplit('/').next().unwrap_or(&target).to_string();
        if base.is_empty() || base == "." || base == ".." {
            return None;
        }
        if self.media_names.insert(base.clone()) {
            self.media.push(DocxMedia {
                name: base.clone(),
                bytes,
            });
        }
        Some(format!("media/{base}"))
    }

    // ----- blocks -------------------------------------------------

    /// The blocks of a container until its closing tag.
    fn read_blocks(
        &mut self,
        toks: &[Tok],
        mut i: usize,
        until: &str,
    ) -> Result<(Vec<Item>, usize)> {
        let mut items: Vec<Item> = Vec::new();
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(name) if name == until => return Ok((items, i + 1)),
                Tok::Open {
                    name,
                    self_closing: true,
                    ..
                } if name == "w:p" => {
                    i += 1;
                }
                Tok::Open { name, .. } if name == "w:p" => {
                    let (p, next) = self.read_paragraph(toks, i + 1)?;
                    items.push(Item::Para(p));
                    i = next;
                }
                Tok::Open { name, .. } if name == "w:tbl" => {
                    let (t, next) = self.read_table(toks, i + 1)?;
                    items.push(Item::Table(t));
                    i = next;
                }
                // Containers read through; deletions skipped.
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if matches!(
                    name.as_str(),
                    "w:sdt" | "w:sdtContent" | "w:ins" | "w:customXml" | "w:smartTag" | "w:body"
                ) =>
                {
                    i += 1;
                }
                Tok::Close(name)
                    if matches!(
                        name.as_str(),
                        "w:sdt"
                            | "w:sdtContent"
                            | "w:ins"
                            | "w:customXml"
                            | "w:smartTag"
                            | "w:body"
                    ) =>
                {
                    i += 1;
                }
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if matches!(
                    name.as_str(),
                    "w:del"
                        | "w:sdtPr"
                        | "w:sdtEndPr"
                        | "w:sectPr"
                        | "mc:AlternateContent"
                        | "w:tblPr"
                        | "w:tblGrid"
                        | "w:trPr"
                        | "w:tcPr"
                        | "w:moveFrom"
                ) =>
                {
                    i = skip(toks, i + 1, name)?;
                }
                Tok::Open {
                    name,
                    attrs,
                    self_closing: true,
                } if name == "w:bookmarkStart" => {
                    if let Some(onym) = attr(attrs, "w:name")
                        .filter(|n| *n != "_GoBack")
                        .and_then(onym_of)
                    {
                        items.push(Item::Para(Para {
                            kind: ParaKind::Plain,
                            inlines: vec![Inline::OnymAnchor(onym)],
                            extras: Vec::new(),
                        }));
                    }
                    i += 1;
                }
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if name == "w:bookmarkStart" => {
                    i = skip(toks, i + 1, name)?;
                }
                _ => i += 1,
            }
        }
        Err(docx_err(format!("unterminated <{until}>")))
    }

    fn read_paragraph(&mut self, toks: &[Tok], mut i: usize) -> Result<(Para, usize)> {
        let mut style: Option<String> = None;
        let mut outline: Option<u8> = None;
        let mut num: Option<(String, u8)> = None;
        // Paragraph properties (pretty-printed XML puts whitespace
        // before them).
        while matches!(toks.get(i), Some(Tok::Text(t)) if t.trim().is_empty()) {
            i += 1;
        }
        if let Some(Tok::Open {
            name,
            self_closing: false,
            ..
        }) = toks.get(i)
            && name == "w:pPr"
        {
            let mut j = i + 1;
            let mut in_num = false;
            while j < toks.len() {
                match &toks[j] {
                    Tok::Close(n) if n == "w:pPr" => {
                        j += 1;
                        break;
                    }
                    Tok::Open { name, attrs, .. } if name == "w:pStyle" => {
                        style = attr(attrs, "w:val").map(str::to_string);
                    }
                    Tok::Open { name, attrs, .. } if name == "w:outlineLvl" => {
                        outline = attr(attrs, "w:val").and_then(|v| v.parse().ok());
                    }
                    Tok::Open { name, .. } if name == "w:numPr" => in_num = true,
                    Tok::Close(name) if name == "w:numPr" => in_num = false,
                    Tok::Open { name, attrs, .. } if in_num && name == "w:ilvl" => {
                        let l = attr(attrs, "w:val")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                        num = Some((num.map(|(id, _)| id).unwrap_or_default(), l));
                    }
                    Tok::Open { name, attrs, .. } if in_num && name == "w:numId" => {
                        let id = attr(attrs, "w:val").unwrap_or("").to_string();
                        num = Some((id, num.map(|(_, l)| l).unwrap_or(0)));
                    }
                    Tok::Open {
                        name,
                        self_closing: false,
                        ..
                    } if name == "w:rPr" => {
                        j = skip(toks, j + 1, name)?;
                        continue;
                    }
                    _ => {}
                }
                j += 1;
            }
            i = j;
        }
        let mut inlines: Vec<Inline> = Vec::new();
        let mut extras: Vec<Block> = Vec::new();
        let next = self.read_runs(toks, i, "w:p", Fmt::default(), &mut inlines, &mut extras)?;
        trim_inline_edges(&mut inlines);
        let kind = if let Some((id, level)) = num
            && !id.is_empty()
            && id != "0"
        {
            ParaKind::ListItem {
                ordered: self.numbering.ordered(&id, level),
                level,
            }
        } else if let Some(level) =
            outline.or_else(|| style.as_deref().and_then(|s| self.styles.outline(s)))
        {
            ParaKind::Heading(level)
        } else {
            match style.as_deref().and_then(|s| self.styles.kind(s)) {
                Some("Title") => ParaKind::Title,
                Some("Subtitle") => ParaKind::Subtitle,
                Some("Author") => ParaKind::Author,
                Some("Abstract") => ParaKind::Abstract,
                Some("SourceCode") => ParaKind::Code,
                Some("Caption" | "ImageCaption" | "TableCaption") => ParaKind::Caption,
                Some("Quote" | "IntenseQuote" | "BlockText" | "Quotations") => ParaKind::Quote,
                _ => ParaKind::Plain,
            }
        };
        Ok((
            Para {
                kind,
                inlines,
                extras,
            },
            next,
        ))
    }

    // ----- runs ---------------------------------------------------

    /// Push text under the formatting in force, merging with a
    /// preceding run of the same formatting.
    fn push_text(&mut self, out: &mut Vec<Inline>, fmt: Fmt, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.dropping {
            return;
        }
        if let Some(field) = &mut self.field
            && field.in_result
        {
            push_fmt(&mut field.result, fmt, text);
            return;
        }
        push_fmt(out, fmt, text);
    }

    #[allow(clippy::too_many_arguments)]
    fn read_runs(
        &mut self,
        toks: &[Tok],
        mut i: usize,
        until: &str,
        fmt: Fmt,
        out: &mut Vec<Inline>,
        extras: &mut Vec<Block>,
    ) -> Result<usize> {
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(name) if name == until => return Ok(i + 1),
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if name == "w:r" => {
                    i = self.read_run(toks, i + 1, fmt, out, extras)?;
                }
                Tok::Open {
                    name,
                    attrs,
                    self_closing,
                } if name == "w:hyperlink" => {
                    let target = attr(attrs, "r:id")
                        .and_then(|id| self.rels.targets.get(id).cloned())
                        .filter(|(_, external)| *external)
                        .map(|(t, _)| t);
                    let anchor = attr(attrs, "w:anchor").map(str::to_string);
                    let mut content = Vec::new();
                    i = if *self_closing {
                        i + 1
                    } else {
                        self.read_runs(toks, i + 1, "w:hyperlink", fmt, &mut content, extras)?
                    };
                    trim_inline_edges(&mut content);
                    if let Some(url) = target {
                        let shown = plain(&content);
                        if content.is_empty() || shown == url {
                            out.push(endo("><", vec![Inline::Text(url)]));
                        } else {
                            out.extend(content);
                            out.push(Inline::Text(" (".to_string()));
                            out.push(endo("><", vec![Inline::Text(url)]));
                            out.push(Inline::Text(")".to_string()));
                        }
                    } else if let Some(onym) = anchor.as_deref().and_then(onym_of) {
                        if plain(&content).trim() != onym {
                            out.extend(content);
                        }
                        out.push(mono(">", &onym));
                    } else {
                        out.extend(content);
                    }
                }
                Tok::Open {
                    name,
                    attrs,
                    self_closing,
                } if name == "w:fldSimple" => {
                    let instr = attr(attrs, "w:instr").unwrap_or("").to_string();
                    let mut result = Vec::new();
                    i = if *self_closing {
                        i + 1
                    } else {
                        self.read_runs(toks, i + 1, "w:fldSimple", fmt, &mut result, extras)?
                    };
                    self.field_result(&instr, result, out);
                }
                Tok::Open {
                    name,
                    attrs,
                    self_closing: true,
                } if name == "w:bookmarkStart" => {
                    if let Some(onym) = attr(attrs, "w:name")
                        .filter(|n| *n != "_GoBack")
                        .and_then(onym_of)
                    {
                        out.push(Inline::OnymAnchor(onym));
                    }
                    i += 1;
                }
                // Read through.
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if matches!(
                    name.as_str(),
                    "w:ins" | "w:smartTag" | "w:sdt" | "w:sdtContent" | "w:customXml" | "w:moveTo"
                ) =>
                {
                    i += 1;
                }
                Tok::Close(name)
                    if matches!(
                        name.as_str(),
                        "w:ins"
                            | "w:smartTag"
                            | "w:sdt"
                            | "w:sdtContent"
                            | "w:customXml"
                            | "w:moveTo"
                    ) =>
                {
                    i += 1;
                }
                // Skipped whole.
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if matches!(
                    name.as_str(),
                    "w:del"
                        | "w:moveFrom"
                        | "w:sdtPr"
                        | "w:sdtEndPr"
                        | "mc:AlternateContent"
                        | "w:pPr"
                        | "w:rPr"
                ) =>
                {
                    i = skip(toks, i + 1, name)?;
                }
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if name == "w:r" => unreachable!(),
                _ => i += 1,
            }
        }
        Err(docx_err(format!("unterminated <{until}>")))
    }

    fn read_run(
        &mut self,
        toks: &[Tok],
        mut i: usize,
        base: Fmt,
        out: &mut Vec<Inline>,
        extras: &mut Vec<Block>,
    ) -> Result<usize> {
        let mut fmt = base;
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(name) if name == "w:r" => return Ok(i + 1),
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if name == "w:rPr" => {
                    let mut j = i + 1;
                    while j < toks.len() {
                        match &toks[j] {
                            Tok::Close(n) if n == "w:rPr" => break,
                            Tok::Open { name, attrs, .. } if name == "w:b" || name == "w:bCs" => {
                                fmt.bold = !matches!(attr(attrs, "w:val"), Some("0" | "false"));
                            }
                            Tok::Open { name, attrs, .. } if name == "w:i" || name == "w:iCs" => {
                                fmt.italic = !matches!(attr(attrs, "w:val"), Some("0" | "false"));
                            }
                            Tok::Open { name, attrs, .. } if name == "w:rStyle" => {
                                match attr(attrs, "w:val") {
                                    Some("Strong") => fmt.bold = true,
                                    Some("Emphasis") => fmt.italic = true,
                                    Some("VerbatimChar" | "SourceCode" | "HTMLCode") => {
                                        fmt.code = true
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    i = j + 1;
                }
                Tok::Open { name, .. } if name == "w:t" => {
                    let (text, next) = text_of(toks, i + 1, "w:t");
                    self.push_text(out, fmt, &text);
                    i = next;
                }
                Tok::Open { name, .. } if name == "w:delText" || name == "w:delInstrText" => {
                    i = skip_any(toks, i, name)?;
                }
                Tok::Open { name, .. } if name == "w:tab" => {
                    self.push_text(out, fmt, " ");
                    i = skip_any(toks, i, name)?;
                }
                Tok::Open { name, attrs, .. } if name == "w:br" || name == "w:cr" => {
                    if attr(attrs, "w:type") != Some("page") {
                        self.push_text(out, fmt, " ");
                    }
                    i = skip_any(toks, i, name)?;
                }
                Tok::Open { name, attrs, .. }
                    if matches!(
                        name.as_str(),
                        "w:footnoteReference" | "w:endnoteReference" | "w:commentReference"
                    ) =>
                {
                    let kind = name.trim_start_matches("w:").trim_end_matches("Reference");
                    if let Some(id) = attr(attrs, "w:id")
                        && let Some(callout) = self.note(kind, id)?
                        && !self.dropping
                    {
                        out.push(callout);
                    }
                    i = skip_any(toks, i, name)?;
                }
                Tok::Open { name, attrs, .. } if name == "w:fldChar" => {
                    match attr(attrs, "w:fldCharType") {
                        Some("begin") => {
                            self.field = Some(Field {
                                instr: String::new(),
                                in_result: false,
                                result: Vec::new(),
                            });
                        }
                        Some("separate") => {
                            if let Some(f) = &mut self.field {
                                f.in_result = true;
                                if f.instr.trim_start().starts_with("BIBLIOGRAPHY") {
                                    self.dropping = true;
                                }
                            }
                        }
                        Some("end") => {
                            self.dropping = false;
                            if let Some(f) = self.field.take() {
                                self.field_result(&f.instr, f.result, out);
                            }
                        }
                        _ => {}
                    }
                    i = skip_any(toks, i, name)?;
                }
                Tok::Open { name, .. } if name == "w:instrText" => {
                    let (text, next) = text_of(toks, i + 1, "w:instrText");
                    if let Some(f) = &mut self.field {
                        f.instr.push_str(&text);
                    }
                    i = next;
                }
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if name == "w:drawing" || name == "w:pict" || name == "w:object" => {
                    // The image behind the drawing, as an enmedia
                    // block beside the paragraph.
                    let end = skip(toks, i + 1, name)?;
                    let rid = toks[i..end].iter().find_map(|t| match t {
                        Tok::Open { name, attrs, .. } if name == "a:blip" => {
                            attr(attrs, "r:embed").map(str::to_string)
                        }
                        Tok::Open { name, attrs, .. } if name == "v:imagedata" => {
                            attr(attrs, "r:id").map(str::to_string)
                        }
                        _ => None,
                    });
                    if let Some(rid) = rid
                        && let Some(param) = self.image(&rid)
                        && !self.dropping
                    {
                        extras.push(Block::Enmedia { param });
                    }
                    i = end;
                }
                Tok::Open { name, attrs, .. } if name == "w:sym" => {
                    if let Some(c) = attr(attrs, "w:char")
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .and_then(char::from_u32)
                    {
                        self.push_text(out, fmt, &c.to_string());
                    }
                    i = skip_any(toks, i, name)?;
                }
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if matches!(
                    name.as_str(),
                    "mc:AlternateContent" | "w:ruby" | "w:fldSimple"
                ) =>
                {
                    i = skip(toks, i + 1, name)?;
                }
                _ => i += 1,
            }
        }
        Err(docx_err("unterminated <w:r>".to_string()))
    }

    /// A field's instruction with its cached result: XE is a hidden
    /// index mark, CITATION a cite (the result its span),
    /// BIBLIOGRAPHY nothing, anything else its result as text.
    fn field_result(&mut self, instr: &str, mut result: Vec<Inline>, out: &mut Vec<Inline>) {
        let instr = instr.trim();
        let mut words = instr.split_whitespace();
        match words.next() {
            Some("XE") => {
                let term = quoted_arg(instr);
                if !term.is_empty() {
                    out.push(mono("%%", &term));
                }
            }
            Some("CITATION") => {
                let tag = words.next().unwrap_or("").trim_matches('"');
                if tag.is_empty() {
                    out.extend(result);
                    return;
                }
                trim_inline_edges(&mut result);
                let cite = mono(">[", tag);
                if result.is_empty() {
                    out.push(cite);
                } else {
                    let mut content = vec![cite];
                    content.extend(result);
                    out.push(Inline::EndoDiaphane {
                        content,
                        ann: Annotations::default(),
                    });
                }
            }
            Some("BIBLIOGRAPHY") => {}
            Some("HYPERLINK") => {
                let url = quoted_arg(instr);
                trim_inline_edges(&mut result);
                if url.is_empty() {
                    out.extend(result);
                } else if result.is_empty() || plain(&result) == url {
                    out.push(endo("><", vec![Inline::Text(url)]));
                } else {
                    out.extend(result);
                    out.push(Inline::Text(" (".to_string()));
                    out.push(endo("><", vec![Inline::Text(url)]));
                    out.push(Inline::Text(")".to_string()));
                }
            }
            _ => out.extend(result),
        }
    }

    // ----- tables -------------------------------------------------

    fn read_table(&mut self, toks: &[Tok], mut i: usize) -> Result<(Block, usize)> {
        struct Cell {
            hspan: usize,
            vmerge: Option<bool>, // Some(true) restart, Some(false) continue
            blocks: Vec<Block>,
        }
        let mut rows: Vec<(bool, Vec<Cell>)> = Vec::new();
        while i < toks.len() {
            match &toks[i] {
                Tok::Close(name) if name == "w:tbl" => break,
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if name == "w:tr" => {
                    let mut header = false;
                    let mut cells: Vec<Cell> = Vec::new();
                    let mut j = i + 1;
                    while j < toks.len() {
                        match &toks[j] {
                            Tok::Close(n) if n == "w:tr" => {
                                j += 1;
                                break;
                            }
                            Tok::Open {
                                name,
                                self_closing: false,
                                ..
                            } if name == "w:trPr" => {
                                let end = skip(toks, j + 1, name)?;
                                header = toks[j..end]
                                    .iter()
                                    .any(|t| matches!(t, Tok::Open { name, attrs, .. } if name == "w:tblHeader" && !matches!(attr(attrs, "w:val"), Some("0" | "false"))));
                                j = end;
                            }
                            Tok::Open {
                                name,
                                self_closing: false,
                                ..
                            } if name == "w:tc" => {
                                let mut hspan = 1;
                                let mut vmerge = None;
                                let mut k = j + 1;
                                while matches!(toks.get(k), Some(Tok::Text(t)) if t.trim().is_empty())
                                {
                                    k += 1;
                                }
                                if let Some(Tok::Open {
                                    name,
                                    self_closing: false,
                                    ..
                                }) = toks.get(k)
                                    && name == "w:tcPr"
                                {
                                    let end = skip(toks, k + 1, name)?;
                                    for t in &toks[k..end] {
                                        if let Tok::Open { name, attrs, .. } = t {
                                            if name == "w:gridSpan" {
                                                hspan = attr(attrs, "w:val")
                                                    .and_then(|v| v.parse().ok())
                                                    .unwrap_or(1)
                                                    .max(1);
                                            } else if name == "w:vMerge" {
                                                vmerge =
                                                    Some(attr(attrs, "w:val") == Some("restart"));
                                            }
                                        }
                                    }
                                    k = end;
                                }
                                let (items, next) = self.read_blocks(toks, k, "w:tc")?;
                                cells.push(Cell {
                                    hspan,
                                    vmerge,
                                    blocks: assemble(items),
                                });
                                j = next;
                            }
                            Tok::Open {
                                name,
                                self_closing: false,
                                ..
                            } if matches!(
                                name.as_str(),
                                "w:sdt" | "w:sdtContent" | "w:ins" | "w:customXml"
                            ) =>
                            {
                                j += 1
                            }
                            Tok::Open {
                                name,
                                self_closing: false,
                                ..
                            } if matches!(name.as_str(), "w:del" | "w:sdtPr" | "w:tblPrEx") => {
                                j = skip(toks, j + 1, name)?;
                            }
                            _ => j += 1,
                        }
                    }
                    rows.push((header, cells));
                    i = j;
                }
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if matches!(name.as_str(), "w:tblPr" | "w:tblGrid" | "w:sdtPr") => {
                    i = skip(toks, i + 1, name)?;
                }
                _ => i += 1,
            }
        }
        if i >= toks.len() {
            return Err(docx_err("unterminated <w:tbl>".to_string()));
        }
        // Vertical merges: a restart cell spans the continuation
        // cells below it in the same grid column, which are
        // omitted.
        let positions: Vec<Vec<usize>> = rows
            .iter()
            .map(|(_, cells)| {
                let mut col = 0;
                cells
                    .iter()
                    .map(|c| {
                        let at = col;
                        col += c.hspan;
                        at
                    })
                    .collect()
            })
            .collect();
        let mut out_rows: Vec<Block> = Vec::new();
        for (r, (header, cells)) in rows.iter().enumerate() {
            let mut out_cells: Vec<Block> = Vec::new();
            for (c, cell) in cells.iter().enumerate() {
                let col = positions[r][c];
                let vspan = match cell.vmerge {
                    Some(false) => continue,
                    Some(true) => {
                        let mut n = 1;
                        for (rr, (_, below)) in rows.iter().enumerate().skip(r + 1) {
                            let cont = below
                                .iter()
                                .enumerate()
                                .any(|(cc, b)| positions[rr][cc] == col && b.vmerge == Some(false));
                            if cont {
                                n += 1;
                            } else {
                                break;
                            }
                        }
                        n
                    }
                    None => 1,
                };
                let mut blocks = cell.blocks.clone();
                let mut spans: Vec<Inline> = Vec::new();
                if cell.hspan > 1 {
                    spans.push(mono("+>", &cell.hspan.to_string()));
                }
                if vspan > 1 {
                    spans.push(mono("+_", &vspan.to_string()));
                }
                if !spans.is_empty() {
                    match blocks.first_mut() {
                        Some(Block::Paragraph(p)) => {
                            spans.push(Inline::Text(" ".to_string()));
                            spans.append(p);
                            *p = spans;
                        }
                        _ => blocks.insert(0, Block::Paragraph(spans)),
                    }
                }
                out_cells.push(para_block("+:", Vec::new(), blocks, None));
            }
            out_rows.push(para_block(
                if *header { "+=" } else { "+-" },
                Vec::new(),
                out_cells,
                None,
            ));
        }
        Ok((para_block("+", Vec::new(), out_rows, None), i + 1))
    }
}

// ---------------------------------------------------------------
// Inline helpers
// ---------------------------------------------------------------

fn push_fmt(out: &mut Vec<Inline>, fmt: Fmt, text: &str) {
    if fmt.code {
        out.push(Inline::VerbatimInline {
            content: text.to_string(),
            ann: Annotations::default(),
        });
        return;
    }
    let symbol = match (fmt.bold, fmt.italic) {
        (true, true) => Some("*/"),
        (true, false) => Some("*"),
        (false, true) => Some("/"),
        (false, false) => None,
    };
    match symbol {
        None => match out.last_mut() {
            Some(Inline::Text(t)) => t.push_str(text),
            _ => out.push(Inline::Text(text.to_string())),
        },
        Some(sym) => {
            if let Some(Inline::Endo {
                symbol, content, ..
            }) = out.last_mut()
                && symbol == sym
            {
                match content.last_mut() {
                    Some(Inline::Text(t)) => t.push_str(text),
                    _ => content.push(Inline::Text(text.to_string())),
                }
            } else {
                out.push(endo(sym, vec![Inline::Text(text.to_string())]));
            }
        }
    }
}

fn plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for i in inlines {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                s.push_str(&plain(content))
            }
            Inline::VerbatimInline { content, .. } => s.push_str(content),
            _ => {}
        }
    }
    s
}

/// The first double-quoted argument of a field instruction.
fn quoted_arg(instr: &str) -> String {
    let Some(start) = instr.find('"') else {
        return instr.split_whitespace().nth(1).unwrap_or("").to_string();
    };
    let rest = &instr[start + 1..];
    rest[..rest.find('"').unwrap_or(rest.len())]
        .trim()
        .to_string()
}

/// The text of an element, entities decoded, until its close.
fn text_of(toks: &[Tok], mut i: usize, until: &str) -> (String, usize) {
    let mut out = String::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(n) if n == until => return (out, i + 1),
            Tok::Text(t) => out.push_str(&decode_entities(t)),
            _ => {}
        }
        i += 1;
    }
    (out, i)
}

/// Skip to after the close of the element opened at `i - 1`.
fn skip(toks: &[Tok], mut i: usize, name: &str) -> Result<usize> {
    let mut depth = 1;
    while i < toks.len() {
        match &toks[i] {
            Tok::Open {
                name: n,
                self_closing: false,
                ..
            } if n == name => depth += 1,
            Tok::Close(n) if n == name => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(docx_err(format!("unterminated <{name}>")))
}

/// Skip the element at `i`, self-closing or not.
fn skip_any(toks: &[Tok], i: usize, name: &str) -> Result<usize> {
    match &toks[i] {
        Tok::Open {
            self_closing: true, ..
        } => Ok(i + 1),
        _ => skip(toks, i + 1, name),
    }
}

// ---------------------------------------------------------------
// Assembly: sections, lists, quotes, figures
// ---------------------------------------------------------------

fn section_symbol(level: u8) -> &'static str {
    match level {
        0 => "#",
        1 => "##",
        2 => "###",
        _ => "####",
    }
}

/// Build the block tree from the paragraphs as read: headings
/// open sections that nest by level, runs of list items become
/// lists nested by level, runs of quote paragraphs one blockquote,
/// an image followed by a caption a figure.
fn assemble(items: Vec<Item>) -> Vec<Block> {
    // (level, section block) — the open sections, outermost first.
    let mut stack: Vec<(u8, Block)> = Vec::new();
    let mut root: Vec<Block> = Vec::new();
    let mut quote: Vec<Block> = Vec::new();
    let mut list: Vec<(bool, u8, Vec<Block>)> = Vec::new(); // open lists: ordered, level, items
    let mut pending_image: Option<Block> = None;
    // A caption paragraph right before a table names it; one right
    // after it captions it.
    let mut pending_caption: Option<Vec<Inline>> = None;
    let mut after_table = false;

    fn target<'a>(stack: &'a mut [(u8, Block)], root: &'a mut Vec<Block>) -> &'a mut Vec<Block> {
        match stack.last_mut() {
            Some((_, Block::Para { children, .. })) => children,
            _ => root,
        }
    }
    fn flush_quote(quote: &mut Vec<Block>, stack: &mut [(u8, Block)], root: &mut Vec<Block>) {
        if !quote.is_empty() {
            let q = para_block("\"", Vec::new(), std::mem::take(quote), None);
            target(stack, root).push(q);
        }
    }
    fn close_lists(
        list: &mut Vec<(bool, u8, Vec<Block>)>,
        down_to: usize,
        stack: &mut [(u8, Block)],
        root: &mut Vec<Block>,
    ) {
        while list.len() > down_to {
            let (ordered, _, items) = list.pop().unwrap();
            let block = list_block(ordered, items);
            match list.last_mut() {
                Some((_, _, parent_items)) => {
                    // Nest under the last item of the enclosing list.
                    if let Some(Block::Para { children, .. }) = parent_items.last_mut() {
                        children.push(block);
                    } else {
                        parent_items.push(block);
                    }
                }
                None => target(stack, root).push(block),
            }
        }
    }
    fn flush_image(pending: &mut Option<Block>, stack: &mut [(u8, Block)], root: &mut Vec<Block>) {
        if let Some(img) = pending.take() {
            target(stack, root).push(img);
        }
    }

    let mut items = items.into_iter().peekable();
    while let Some(item) = items.next() {
        match item {
            Item::Table(mut t) => {
                flush_quote(&mut quote, &mut stack, &mut root);
                close_lists(&mut list, 0, &mut stack, &mut root);
                flush_image(&mut pending_image, &mut stack, &mut root);
                if let (Some(title), Block::Para { lemma, .. }) = (pending_caption.take(), &mut t) {
                    *lemma = title;
                }
                target(&mut stack, &mut root).push(t);
                after_table = true;
            }
            Item::Para(p) => {
                let Para {
                    kind,
                    inlines,
                    extras,
                } = p;
                if kind == ParaKind::Caption && !inlines.is_empty() {
                    if after_table {
                        if let Some(Block::Para { hypograph, .. }) =
                            target(&mut stack, &mut root).last_mut()
                        {
                            *hypograph = inlines;
                            after_table = false;
                            continue;
                        }
                    } else if pending_image.is_none()
                        && matches!(items.peek(), Some(Item::Table(_)))
                    {
                        pending_caption = Some(inlines);
                        continue;
                    }
                }
                after_table = false;
                // A caption right after an image makes a figure.
                if kind == ParaKind::Caption
                    && let Some(img) = pending_image.take()
                {
                    let mut children = vec![img];
                    if !inlines.is_empty() {
                        children.push(Block::Paragraph(inlines));
                    }
                    // The figure's episim is the same left angle:
                    // bracket matching off.
                    target(&mut stack, &mut root).push(Block::Para {
                        symbol: "<".to_string(),
                        taxis: None,
                        lemma: Vec::new(),
                        children,
                        hypograph: Vec::new(),
                        bracket_matching: false,
                        ann: Annotations::default(),
                    });
                    continue;
                }
                flush_image(&mut pending_image, &mut stack, &mut root);
                if kind != ParaKind::Quote {
                    flush_quote(&mut quote, &mut stack, &mut root);
                }
                if !matches!(kind, ParaKind::ListItem { .. }) {
                    close_lists(&mut list, 0, &mut stack, &mut root);
                }
                // An image-only paragraph: the image stands as a
                // block, held for a following caption.
                let mut extras = extras;
                if inlines.is_empty() && extras.len() == 1 && kind != ParaKind::Caption {
                    pending_image = extras.pop();
                    continue;
                }
                let blocks_of = |inlines: Vec<Inline>, extras: Vec<Block>| {
                    let mut v = Vec::new();
                    if !inlines.is_empty() {
                        v.push(Block::Paragraph(inlines));
                    }
                    v.extend(extras);
                    v
                };
                match kind {
                    ParaKind::Heading(level) => {
                        while stack.last().is_some_and(|(l, _)| *l >= level) {
                            let (_, done) = stack.pop().unwrap();
                            target(&mut stack, &mut root).push(done);
                        }
                        // A bookmark standing alone right before the
                        // heading names the section.
                        let onym = match target(&mut stack, &mut root).last() {
                            Some(Block::Paragraph(p))
                                if matches!(p.as_slice(), [Inline::OnymAnchor(_)]) =>
                            {
                                let Some(Block::Paragraph(p)) = target(&mut stack, &mut root).pop()
                                else {
                                    unreachable!()
                                };
                                match p.into_iter().next() {
                                    Some(Inline::OnymAnchor(o)) => Some(o),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };
                        stack.push((
                            level,
                            para_block(section_symbol(level), inlines, Vec::new(), onym),
                        ));
                        target(&mut stack, &mut root).extend(extras);
                    }
                    ParaKind::Title => target(&mut stack, &mut root).push(solo("=", inlines)),
                    ParaKind::Subtitle => target(&mut stack, &mut root).push(solo("=_", inlines)),
                    ParaKind::Author => target(&mut stack, &mut root).push(solo("=:", inlines)),
                    ParaKind::Abstract => {
                        let body = blocks_of(inlines, extras);
                        target(&mut stack, &mut root).push(para_block(
                            "=\"",
                            Vec::new(),
                            body,
                            None,
                        ));
                    }
                    ParaKind::Code => target(&mut stack, &mut root).push(Block::VerbatimBlock {
                        content: plain(&inlines),
                        ann: Annotations::default(),
                    }),
                    ParaKind::Quote => quote.extend(blocks_of(inlines, extras)),
                    ParaKind::ListItem { ordered, level } => {
                        // Close deeper lists; open the level's list.
                        while list.last().is_some_and(|(_, l, _)| *l > level) {
                            let n = list.len() - 1;
                            close_lists(&mut list, n, &mut stack, &mut root);
                        }
                        match list.last() {
                            Some((o, l, _)) if *l == level && *o == ordered => {}
                            Some((_, l, _)) if *l == level => {
                                let n = list.len() - 1;
                                close_lists(&mut list, n, &mut stack, &mut root);
                                list.push((ordered, level, Vec::new()));
                            }
                            _ => list.push((ordered, level, Vec::new())),
                        }
                        let body = blocks_of(inlines, extras);
                        let n =
                            list.last().map(|(_, _, items)| items.len()).unwrap_or(0) as u64 + 1;
                        let item = Block::Para {
                            symbol: if ordered { ".-" } else { "-" }.to_string(),
                            taxis: ordered.then_some(Taxis::Explicit(n)),
                            lemma: Vec::new(),
                            children: body,
                            hypograph: Vec::new(),
                            bracket_matching: true,
                            ann: Annotations::default(),
                        };
                        list.last_mut().unwrap().2.push(item);
                    }
                    ParaKind::Plain | ParaKind::Caption => {
                        target(&mut stack, &mut root).extend(blocks_of(inlines, extras));
                    }
                }
            }
        }
    }
    flush_quote(&mut quote, &mut stack, &mut root);
    close_lists(&mut list, 0, &mut stack, &mut root);
    flush_image(&mut pending_image, &mut stack, &mut root);
    while let Some((_, done)) = stack.pop() {
        target(&mut stack, &mut root).push(done);
    }
    root
}

fn list_block(ordered: bool, items: Vec<Block>) -> Block {
    para_block(if ordered { ".." } else { "--" }, Vec::new(), items, None)
}

// ---------------------------------------------------------------
// The bibliography (b:Sources)
// ---------------------------------------------------------------

fn sources(pkg: &Package) -> Result<Vec<Block>> {
    let mut names: Vec<&String> = pkg
        .parts
        .keys()
        .filter(|n| n.starts_with("customXml/") && n.ends_with(".xml") && !n.contains("itemProps"))
        .collect();
    names.sort();
    let mut entries: Vec<Block> = Vec::new();
    for name in names {
        let Some(xml) = pkg.text(name) else { continue };
        if !xml.contains("b:Sources") {
            continue;
        }
        let toks = tokenize_xml(&xml)?;
        let mut i = 0;
        while i < toks.len() {
            match &toks[i] {
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if name == "b:Source" => {
                    let (entry, next) = source_entry(&toks, i + 1)?;
                    entries.extend(entry);
                    i = next;
                }
                _ => i += 1,
            }
        }
    }
    Ok(entries)
}

fn source_entry(toks: &[Tok], mut i: usize) -> Result<(Option<Block>, usize)> {
    let mut tag = String::new();
    let mut kind = String::new();
    let mut fields: Vec<(&'static str, String)> = Vec::new();
    let mut authors: Vec<String> = Vec::new();
    let mut editors: Vec<String> = Vec::new();
    // Inside b:Author: which role (Author / Editor) a name list
    // belongs to.
    let mut role: Option<&'static str> = None;
    let mut depth_author = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "b:Source" => {
                i += 1;
                break;
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "b:Author" => {
                depth_author += 1;
                if depth_author == 2 {
                    role = Some("author");
                }
                i += 1;
            }
            Tok::Close(name) if name == "b:Author" => {
                depth_author -= 1;
                if depth_author == 1 {
                    role = None;
                }
                i += 1;
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if depth_author == 1
                && matches!(name.as_str(), "b:Editor" | "b:Translator" | "b:Compiler") =>
            {
                role = Some(if name == "b:Editor" {
                    "editor"
                } else {
                    "other"
                });
                i += 1;
            }
            Tok::Close(name)
                if matches!(name.as_str(), "b:Editor" | "b:Translator" | "b:Compiler") =>
            {
                role = None;
                i += 1;
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "b:Person" => {
                let end = skip(toks, i + 1, name)?;
                let part = |what: &str| {
                    let mut k = i;
                    while k < end {
                        if let Tok::Open { name, .. } = &toks[k]
                            && name == what
                        {
                            return text_of(toks, k + 1, what).0;
                        }
                        k += 1;
                    }
                    String::new()
                };
                let last = part("b:Last");
                let first = part("b:First");
                let middle = part("b:Middle");
                let mut given = first;
                if !middle.is_empty() {
                    if !given.is_empty() {
                        given.push(' ');
                    }
                    given.push_str(&middle);
                }
                let name = match (last.is_empty(), given.is_empty()) {
                    (false, false) => format!("{last}, {given}"),
                    (false, true) => last,
                    (true, false) => given,
                    (true, true) => String::new(),
                };
                if !name.is_empty() {
                    match role {
                        Some("editor") => editors.push(name),
                        Some("other") => {}
                        _ => authors.push(name),
                    }
                }
                i = end;
            }
            Tok::Open { name, .. } if name == "b:Corporate" => {
                let (text, next) = text_of(toks, i + 1, "b:Corporate");
                if !text.is_empty() {
                    match role {
                        Some("editor") => editors.push(text),
                        Some("other") => {}
                        _ => authors.push(text),
                    }
                }
                i = next;
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name.starts_with("b:") && depth_author == 0 => {
                let n = name.clone();
                let (text, next) = text_of(toks, i + 1, &n);
                let text = text.trim().to_string();
                match n.as_str() {
                    "b:Tag" => tag = text,
                    "b:SourceType" => kind = text,
                    "b:Title" => fields.push(("title", text)),
                    "b:BookTitle" => fields.push(("booktitle", text)),
                    "b:JournalName" | "b:PeriodicalTitle" => fields.push(("journal", text)),
                    "b:Year" => fields.push(("year", text)),
                    "b:City" => fields.push(("location", text)),
                    "b:Publisher" => fields.push(("publisher", text)),
                    "b:Volume" => fields.push(("volume", text)),
                    "b:Issue" => fields.push(("number", text)),
                    "b:Pages" => fields.push(("pages", text)),
                    "b:Edition" => fields.push(("edition", text)),
                    "b:URL" => fields.push(("url", text)),
                    "b:DOI" => fields.push(("doi", text)),
                    "b:Institution" => fields.push(("institution", text)),
                    "b:Comments" => fields.push(("note", text)),
                    _ => {}
                }
                i = next;
            }
            _ => i += 1,
        }
    }
    if tag.is_empty() {
        return Ok((None, i));
    }
    let genus = match kind.as_str() {
        "Book" => "book",
        "BookSection" => "incollection",
        "JournalArticle" | "ArticleInAPeriodical" => "article",
        "ConferenceProceedings" => "inproceedings",
        "Report" => "report",
        "InternetSite" | "DocumentFromInternetSite" => "online",
        _ => "misc",
    };
    let mut all: Vec<(&'static str, String)> = Vec::new();
    if !authors.is_empty() {
        all.push(("author", authors.join(" and ")));
    }
    if !editors.is_empty() {
        all.push(("editor", editors.join(" and ")));
    }
    all.extend(fields);
    let children: Vec<Block> = all
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(name, value)| Block::Para {
            symbol: ":".to_string(),
            taxis: None,
            lemma: vec![Inline::Text(name.to_string())],
            children: vec![Block::Paragraph(vec![Inline::Text(value)])],
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations::default(),
        })
        .collect();
    Ok((
        Some(Block::Para {
            symbol: "&".to_string(),
            taxis: None,
            lemma: vec![Inline::Text(tag)],
            children,
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations {
                onym: None,
                genoses: vec![genus.to_string()],
            },
        }),
        i,
    ))
}

// ---------------------------------------------------------------
// Documents
// ---------------------------------------------------------------

/// Import a Word document from its bytes.
pub fn docx_to_document(bytes: &[u8]) -> Result<Document> {
    docx_to_document_with_media(bytes).map(|(d, _)| d)
}

/// Import a Word document and return its images beside it, named
/// as the enmedia blocks refer to them (`media/<name>`).
pub fn docx_to_document_with_media(bytes: &[u8]) -> Result<(Document, Vec<DocxMedia>)> {
    let pkg = read_package(bytes)?;
    let mut ctx = Ctx::new(&pkg)?;
    let toks = pkg.toks("word/document.xml")?;
    let body_start = toks
        .iter()
        .position(|t| matches!(t, Tok::Open { name, .. } if name == "w:body"))
        .ok_or_else(|| docx_err("word/document.xml has no w:body".to_string()))?;
    let (items, _) = ctx.read_blocks(&toks, body_start + 1, "w:body")?;
    let mut blocks = assemble(items);
    // Front matter from the core properties when the body declares
    // none.
    let has_title = blocks.iter().any(|b| matches!(b, Block::Paragraph(p) if matches!(p.as_slice(), [Inline::Endo { symbol, .. }] if symbol == "=")));
    let has_author = blocks.iter().any(|b| matches!(b, Block::Paragraph(p) if matches!(p.as_slice(), [Inline::Endo { symbol, .. }] if symbol == "=:")));
    if let Some(core) = pkg.text("docProps/core.xml") {
        let core = tokenize_xml(&core)?;
        let find = |what: &str| {
            core.iter()
                .position(|t| matches!(t, Tok::Open { name, .. } if name == what))
                .map(|k| text_of(&core, k + 1, what).0.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let mut front: Vec<Block> = Vec::new();
        if !has_title && let Some(title) = find("dc:title") {
            front.push(solo("=", vec![Inline::Text(title)]));
        }
        if !has_author && let Some(creator) = find("dc:creator") {
            front.push(solo("=:", vec![Inline::Text(creator)]));
        }
        if !front.is_empty() {
            front.append(&mut blocks);
            blocks = front;
        }
    }
    // Notes, in order of first reference, then the bibliography.
    for (symbol, onym, body) in std::mem::take(&mut ctx.notes) {
        blocks.push(para_block(&symbol, Vec::new(), body, Some(onym)));
    }
    let entries = sources(&pkg)?;
    if !entries.is_empty() {
        blocks.push(Block::MonadEnglossis {
            dialect: "bibliogramma".to_string(),
            children: entries,
            ann: Annotations::default(),
        });
    }
    Ok((
        Document {
            dialect_id: "litogramma".to_string(),
            dialect_version: None,
            blocks,
        },
        ctx.media,
    ))
}

// ===============================================================
// litogramma -> Word
// ===============================================================
//
// A built-in exomorphosis: a .docx is several XML parts in a zip,
// with ids the parts share (notes, relationships, bookmarks), which
// the template language cannot produce. The style set is minimal
// and fixed — Title, Author, Heading 1–4, Quote, Source Code — so
// Word and LibreOffice recognise the structure; registers are a
// follow-up. Notes are footnotes and endnotes, a manuscript note a
// comment, a cite a CITATION field over the b:Sources part written
// from the embedded bibliogramma, an index mark an XE field, a
// table the table model with the merged-away cells re-inserted as
// OOXML requires.

fn xml(text: &str) -> String {
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

/// The pixel size of a PNG, JPEG or GIF, for the drawing's extent.
fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h));
    }
    if bytes.starts_with(b"GIF8") && bytes.len() >= 10 {
        let w = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let h = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        return Some((w, h));
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xff {
                return None;
            }
            let marker = bytes[i + 1];
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            if matches!(
                marker,
                0xc0 | 0xc1
                    | 0xc2
                    | 0xc3
                    | 0xc5
                    | 0xc6
                    | 0xc7
                    | 0xc9
                    | 0xca
                    | 0xcb
                    | 0xcd
                    | 0xce
                    | 0xcf
            ) {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

fn content_type_of(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RunFmt {
    Plain,
    Bold,
    Italic,
    BoldItalic,
    Code,
}

struct Writer<'a> {
    read_media: &'a dyn Fn(&str) -> Option<Vec<u8>>,
    body: String,
    footnotes: Vec<(String, String)>, // (onym, xml of the note's paragraphs)
    endnotes: Vec<(String, String)>,
    comments: Vec<(String, String)>,
    note_ids: HashMap<String, (String, usize)>, // onym -> (family, id)
    rels: Vec<(String, String, String, bool)>,  // (rId, type, target, external)
    media: Vec<(String, Vec<u8>)>,              // (name, bytes)
    media_rids: HashMap<String, String>,
    bookmark_id: usize,
    num_id: usize,
    numbering: Vec<(usize, bool)>, // (numId, ordered)
    bibliography: Vec<Block>,
    title: Option<String>,
    creator: Option<String>,
    /// The note bodies by onym, found before the body is written.
    note_bodies: HashMap<String, (String, Vec<Block>)>, // onym -> (symbol, blocks)
    in_note: bool,
}

const NOTE_SYMBOLS: &[&str] = &["^", "^^", "^^^", "^!", "|"];

impl<'a> Writer<'a> {
    fn new(read_media: &'a dyn Fn(&str) -> Option<Vec<u8>>) -> Self {
        let mut w = Writer {
            read_media,
            body: String::new(),
            footnotes: Vec::new(),
            endnotes: Vec::new(),
            comments: Vec::new(),
            note_ids: HashMap::new(),
            rels: Vec::new(),
            media: Vec::new(),
            media_rids: HashMap::new(),
            bookmark_id: 0,
            num_id: 2,
            numbering: Vec::new(),
            bibliography: Vec::new(),
            title: None,
            creator: None,
            note_bodies: HashMap::new(),
            in_note: false,
        };
        for (id, ty, target) in [
            ("rId1", "styles", "styles.xml"),
            ("rId2", "numbering", "numbering.xml"),
            ("rId3", "footnotes", "footnotes.xml"),
            ("rId4", "endnotes", "endnotes.xml"),
            ("rId5", "comments", "comments.xml"),
        ] {
            w.rels
                .push((id.to_string(), ty.to_string(), target.to_string(), false));
        }
        w
    }

    fn rel(&mut self, ty: &str, target: &str, external: bool) -> String {
        let id = format!("rId{}", self.rels.len() + 1);
        self.rels
            .push((id.clone(), ty.to_string(), target.to_string(), external));
        id
    }

    // ----- gathering ---------------------------------------------

    /// Note bodies and the bibliography sit anywhere (the end, by
    /// litogramma's canon); they are lifted out before writing.
    fn gather(&mut self, blocks: &[Block]) {
        for block in blocks {
            match block {
                Block::Para {
                    symbol,
                    children,
                    ann,
                    ..
                } if NOTE_SYMBOLS.contains(&symbol.as_str()) => {
                    if let Some(onym) = &ann.onym {
                        self.note_bodies
                            .insert(onym.clone(), (symbol.clone(), children.clone()));
                    }
                }
                Block::Para { children, .. } | Block::ParaDiaphane { children, .. } => {
                    self.gather(children)
                }
                Block::MonadEnglossis {
                    dialect, children, ..
                } if dialect == "bibliogramma" => {
                    self.bibliography.extend(children.iter().cloned());
                }
                _ => {}
            }
        }
    }

    // ----- paragraphs --------------------------------------------

    fn paragraph(&mut self, style: Option<&str>, num: Option<(usize, usize)>, inlines: &[Inline]) {
        let mut ppr = String::new();
        if let Some(s) = style {
            ppr.push_str(&format!("<w:pStyle w:val=\"{s}\"/>"));
        }
        if let Some((num_id, level)) = num {
            ppr.push_str(&format!(
                "<w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{num_id}\"/></w:numPr>"
            ));
        }
        let runs = self.runs(inlines, RunFmt::Plain);
        self.body.push_str("<w:p>");
        if !ppr.is_empty() {
            self.body.push_str(&format!("<w:pPr>{ppr}</w:pPr>"));
        }
        self.body.push_str(&runs);
        self.body.push_str("</w:p>");
    }

    fn text_run(fmt: RunFmt, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }
        let rpr = match fmt {
            RunFmt::Plain => String::new(),
            RunFmt::Bold => "<w:rPr><w:b/></w:rPr>".to_string(),
            RunFmt::Italic => "<w:rPr><w:i/></w:rPr>".to_string(),
            RunFmt::BoldItalic => "<w:rPr><w:b/><w:i/></w:rPr>".to_string(),
            RunFmt::Code => "<w:rPr><w:rStyle w:val=\"VerbatimChar\"/></w:rPr>".to_string(),
        };
        format!(
            "<w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
            xml(text)
        )
    }

    fn combine(fmt: RunFmt, symbol: &str) -> RunFmt {
        match (fmt, symbol) {
            (RunFmt::Code, _) => RunFmt::Code,
            (RunFmt::Plain, "*") => RunFmt::Bold,
            (RunFmt::Plain, "/") => RunFmt::Italic,
            (_, "*/") => RunFmt::BoldItalic,
            (RunFmt::Bold, "/") | (RunFmt::Italic, "*") => RunFmt::BoldItalic,
            (f, _) => f,
        }
    }

    fn field(instr: &str, result: &str) -> String {
        format!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            xml(instr),
            Self::text_run(RunFmt::Plain, result)
        )
    }

    fn runs(&mut self, inlines: &[Inline], fmt: RunFmt) -> String {
        let mut out = String::new();
        for inline in inlines {
            match inline {
                Inline::Text(t) => out.push_str(&Self::text_run(fmt, t)),
                Inline::VerbatimInline { content, .. } => {
                    out.push_str(&Self::text_run(RunFmt::Code, content))
                }
                Inline::Endo {
                    symbol, content, ..
                } => match symbol.as_str() {
                    "><" => {
                        let url = plain(content);
                        let rid = self.rel("hyperlink", &url, true);
                        out.push_str(&format!(
                            "<w:hyperlink r:id=\"{rid}\">{}</w:hyperlink>",
                            Self::text_run(RunFmt::Plain, &url).replace(
                                "<w:r>",
                                "<w:r><w:rPr><w:rStyle w:val=\"Hyperlink\"/></w:rPr>"
                            )
                        ));
                    }
                    "%" => {
                        // A visible index entry: the text, then
                        // the mark.
                        let term = plain(content);
                        out.push_str(&self.runs(content, fmt));
                        out.push_str(&Self::field(&format!("XE \"{term}\""), ""));
                    }
                    "#_" => out.push_str(&self.runs(content, RunFmt::Bold)),
                    ">:" | "\"\"" | "," | "$" | ":-" | ":_" | ":." | ":(" | "=%" => {
                        out.push_str(&self.runs(content, fmt));
                    }
                    s => {
                        let f = Self::combine(fmt, s);
                        out.push_str(&self.runs(content, f));
                    }
                },
                Inline::EndoDiaphane { content, .. } => {
                    // A diaphane opened by a cite is the cite with
                    // its span; any other renders its content.
                    if let [Inline::Monosim { symbol, param, .. }, rest @ ..] = content.as_slice()
                        && symbol == ">["
                    {
                        let span = plain(rest);
                        let shown = if span.is_empty() { param.clone() } else { span };
                        out.push_str(&Self::field(&format!("CITATION {param}"), &shown));
                    } else {
                        out.push_str(&self.runs(content, fmt));
                    }
                }
                Inline::Monosim { symbol, param, .. } => match symbol.as_str() {
                    ">[" => out.push_str(&Self::field(&format!("CITATION {param}"), "")),
                    "%%" => out.push_str(&Self::field(&format!("XE \"{param}\""), "")),
                    ">" | ">>" | ">>>" | ">>>>" => {
                        out.push_str(&format!(
                            "<w:hyperlink w:anchor=\"{}\">{}</w:hyperlink>",
                            xml(param),
                            Self::text_run(RunFmt::Plain, param).replace(
                                "<w:r>",
                                "<w:r><w:rPr><w:rStyle w:val=\"Hyperlink\"/></w:rPr>"
                            )
                        ));
                    }
                    _ => {}
                },
                Inline::Deixis { onym, .. } => {
                    if let Some(r) = self.note_reference(onym) {
                        out.push_str(&r);
                    }
                }
                Inline::OnymAnchor(onym) => {
                    let id = self.bookmark_id;
                    self.bookmark_id += 1;
                    out.push_str(&format!(
                        "<w:bookmarkStart w:id=\"{id}\" w:name=\"{}\"/><w:bookmarkEnd w:id=\"{id}\"/>",
                        xml(onym)
                    ));
                }
                Inline::EndoAxioma { content, .. } => out.push_str(&self.runs(content, fmt)),
                Inline::Milestone { .. } | Inline::AxiomaRef { .. } => {}
            }
        }
        out
    }

    /// A note callout: the body is written to its part (once) and
    /// the reference run returned; a note inside a note stays text.
    fn note_reference(&mut self, onym: &str) -> Option<String> {
        if self.in_note {
            return None;
        }
        let (symbol, blocks) = self.note_bodies.get(onym)?.clone();
        let family = match symbol.as_str() {
            "^" | "|" => "footnote",
            "^^" | "^^^" => "endnote",
            _ => "comment",
        };
        let id = match self.note_ids.get(onym) {
            Some((_, id)) => *id,
            None => {
                let (list, style) = match family {
                    "footnote" => (&mut self.footnotes, "FootnoteText"),
                    "endnote" => (&mut self.endnotes, "EndnoteText"),
                    _ => (&mut self.comments, "CommentText"),
                };
                let id = list.len() + 1;
                // Render the body in a nested writer sharing the
                // relationships and media.
                let saved = std::mem::take(&mut self.body);
                self.in_note = true;
                self.blocks(&blocks, Some(style));
                self.in_note = false;
                let mut xml_body = std::mem::replace(&mut self.body, saved);
                // The reference mark opens the first paragraph.
                let mark = match family {
                    "footnote" => {
                        "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r>"
                    }
                    "endnote" => {
                        "<w:r><w:rPr><w:rStyle w:val=\"EndnoteReference\"/></w:rPr><w:endnoteRef/></w:r>"
                    }
                    _ => "<w:r><w:annotationRef/></w:r>",
                };
                if let Some(pos) = xml_body.find("</w:pPr>") {
                    xml_body.insert_str(pos + "</w:pPr>".len(), mark);
                } else if let Some(pos) = xml_body.find("<w:p>") {
                    xml_body.insert_str(pos + "<w:p>".len(), mark);
                } else {
                    xml_body = format!("<w:p>{mark}</w:p>");
                }
                let list = match family {
                    "footnote" => &mut self.footnotes,
                    "endnote" => &mut self.endnotes,
                    _ => &mut self.comments,
                };
                list.push((onym.to_string(), xml_body));
                self.note_ids
                    .insert(onym.to_string(), (family.to_string(), id));
                id
            }
        };
        Some(match family {
            "footnote" => format!(
                "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"{id}\"/></w:r>"
            ),
            "endnote" => format!(
                "<w:r><w:rPr><w:rStyle w:val=\"EndnoteReference\"/></w:rPr><w:endnoteReference w:id=\"{id}\"/></w:r>"
            ),
            _ => format!(
                "<w:commentRangeStart w:id=\"{id}\"/><w:commentRangeEnd w:id=\"{id}\"/><w:r><w:commentReference w:id=\"{id}\"/></w:r>"
            ),
        })
    }

    // ----- blocks ------------------------------------------------

    fn blocks(&mut self, blocks: &[Block], style: Option<&str>) {
        for block in blocks {
            self.block(block, style);
        }
    }

    fn lemma_paragraph(&mut self, style: Option<&str>, lemma: &[Inline], bold: bool) {
        if lemma.is_empty() {
            return;
        }
        if bold {
            let runs = self.runs(lemma, RunFmt::Bold);
            self.body.push_str("<w:p>");
            if let Some(s) = style {
                self.body
                    .push_str(&format!("<w:pPr><w:pStyle w:val=\"{s}\"/></w:pPr>"));
            }
            self.body.push_str(&runs);
            self.body.push_str("</w:p>");
        } else {
            self.paragraph(style, None, lemma);
        }
    }

    fn list(&mut self, items: &[Block], level: usize, num_id: usize) {
        for item in items {
            if let Block::Para { children, .. } = item {
                let mut first = true;
                for child in children {
                    match child {
                        Block::Paragraph(inlines) if first => {
                            self.paragraph(Some("ListParagraph"), Some((num_id, level)), inlines);
                        }
                        Block::Para {
                            symbol, children, ..
                        } if symbol == "--" || symbol == ".." => {
                            let ordered = symbol == "..";
                            let inner = if ordered { self.fresh_num(true) } else { 1 };
                            self.list(children, level + 1, inner);
                        }
                        other => self.block(other, Some("ListParagraph")),
                    }
                    first = false;
                }
                if first {
                    self.paragraph(Some("ListParagraph"), Some((num_id, level)), &[]);
                }
            }
        }
    }

    fn fresh_num(&mut self, ordered: bool) -> usize {
        let id = self.num_id;
        self.num_id += 1;
        self.numbering.push((id, ordered));
        id
    }

    fn image(&mut self, param: &str) {
        let Some(bytes) = (self.read_media)(param) else {
            self.paragraph(None, None, &[Inline::Text(format!("[{param}]"))]);
            return;
        };
        let name = param.rsplit('/').next().unwrap_or(param).to_string();
        let rid = match self.media_rids.get(&name) {
            Some(r) => r.clone(),
            None => {
                let rid = self.rel("image", &format!("media/{name}"), false);
                self.media_rids.insert(name.clone(), rid.clone());
                self.media.push((name.clone(), bytes.clone()));
                rid
            }
        };
        // Extent: the pixel size at 96 dpi, capped at the text
        // width (6 inches); EMU per inch 914400.
        let (w, h) = image_size(&bytes).unwrap_or((800, 600));
        let max_w = 6.0 * 914400.0;
        let mut cx = w as f64 / 96.0 * 914400.0;
        let mut cy = h as f64 / 96.0 * 914400.0;
        if cx > max_w {
            cy *= max_w / cx;
            cx = max_w;
        }
        let (cx, cy) = (cx as u64, cy as u64);
        let n = self.media.len();
        self.body.push_str(&format!(
            "<w:p><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"{name}\"/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"{name}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
            name = xml(&name)
        ));
    }

    fn table(&mut self, rows: &[Block]) {
        // The grid, as kanonizo settled it; the cells a vertical
        // span covers are written back as vMerge continuations.
        let mut columns = 0usize;
        // Per row: header?, cells as (columns, rows, blocks).
        type CellSpec<'c> = (usize, usize, &'c Vec<Block>);
        let mut rows_spec: Vec<(bool, Vec<CellSpec>)> = Vec::new();
        for row in rows {
            let Block::Para {
                symbol, children, ..
            } = row
            else {
                continue;
            };
            let header = symbol == "+=";
            let mut cells = Vec::new();
            let mut width = 0;
            for cell in children {
                let Block::Para {
                    children: blocks, ..
                } = cell
                else {
                    continue;
                };
                let (h, v) = spans_of(blocks);
                width += h;
                cells.push((h, v, blocks));
            }
            columns = columns.max(width);
            rows_spec.push((header, cells));
        }
        let mut reserved: Vec<usize> = vec![0; columns.max(1)];
        self.body.push_str("<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid>");
        for _ in 0..columns.max(1) {
            self.body.push_str("<w:gridCol/>");
        }
        self.body.push_str("</w:tblGrid>");
        for (header, cells) in &rows_spec {
            self.body.push_str("<w:tr>");
            if *header {
                self.body.push_str("<w:trPr><w:tblHeader/></w:trPr>");
            }
            let mut col = 0;
            let mut placed: Vec<(usize, usize, usize)> = Vec::new();
            let mut it = cells.iter().peekable();
            while col < columns {
                if reserved[col] > 0 {
                    self.body
                        .push_str("<w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>");
                    col += 1;
                    continue;
                }
                let Some((h, v, blocks)) = it.next() else {
                    self.body.push_str("<w:tc><w:p/></w:tc>");
                    col += 1;
                    continue;
                };
                let mut tcpr = String::new();
                if *h > 1 {
                    tcpr.push_str(&format!("<w:gridSpan w:val=\"{h}\"/>"));
                }
                if *v > 1 {
                    tcpr.push_str("<w:vMerge w:val=\"restart\"/>");
                }
                self.body.push_str("<w:tc>");
                if !tcpr.is_empty() {
                    self.body.push_str(&format!("<w:tcPr>{tcpr}</w:tcPr>"));
                }
                let start = self.body.len();
                self.blocks(&strip_spans(blocks), None);
                if self.body.len() == start {
                    self.body.push_str("<w:p/>");
                }
                self.body.push_str("</w:tc>");
                placed.push((col, *h, *v));
                col += h;
            }
            let _ = it.peek();
            self.body.push_str("</w:tr>");
            for n in reserved.iter_mut() {
                *n = n.saturating_sub(1);
            }
            for (c, h, v) in placed {
                if v > 1 {
                    for n in &mut reserved[c..(c + h).min(columns)] {
                        *n = v - 1;
                    }
                }
            }
        }
        self.body.push_str("</w:tbl>");
        // A table directly before the end of a cell or the body
        // needs a paragraph after it.
        self.body.push_str("<w:p/>");
    }

    fn block(&mut self, block: &Block, style: Option<&str>) {
        match block {
            Block::Paragraph(inlines) => {
                // A solo endo-simmere: the title line, an author,
                // a section break.
                if let [
                    Inline::Endo {
                        symbol, content, ..
                    },
                ] = inlines.as_slice()
                {
                    match symbol.as_str() {
                        "=" => {
                            self.title.get_or_insert_with(|| plain(content));
                            self.paragraph(Some("Title"), None, content);
                            return;
                        }
                        "=_" => {
                            self.paragraph(Some("Subtitle"), None, content);
                            return;
                        }
                        "=:" => {
                            self.creator.get_or_insert_with(|| plain(content));
                            self.paragraph(Some("Author"), None, content);
                            return;
                        }
                        "=;" | "-/" => {
                            self.paragraph(Some("Author"), None, content);
                            return;
                        }
                        "**" => {
                            self.body
                                .push_str("<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>");
                            self.body.push_str(&Self::text_run(RunFmt::Plain, "* * *"));
                            self.body.push_str("</w:p>");
                            return;
                        }
                        _ => {}
                    }
                }
                self.paragraph(style, None, inlines)
            }
            Block::VerbatimBlock { content, .. } => {
                for line in content.lines() {
                    self.paragraph(Some("SourceCode"), None, &[Inline::Text(line.to_string())]);
                }
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                self.lemma_paragraph(style, lemma, true);
                for strophe in strophes {
                    let mut runs = String::new();
                    for (n, line) in strophe.0.iter().enumerate() {
                        if n > 0 {
                            runs.push_str("<w:r><w:br/></w:r>");
                        }
                        runs.push_str(&self.runs(line, RunFmt::Plain));
                    }
                    self.body.push_str("<w:p>");
                    if let Some(s) = style {
                        self.body
                            .push_str(&format!("<w:pPr><w:pStyle w:val=\"{s}\"/></w:pPr>"));
                    }
                    self.body.push_str(&runs);
                    self.body.push_str("</w:p>");
                }
                if !hypograph.is_empty() {
                    self.paragraph(style, None, hypograph);
                }
            }
            Block::ParaDiaphane { children, .. } => self.blocks(children, style),
            Block::Enmedia { param } => self.image(param),
            Block::MonadEnglossis {
                dialect, children, ..
            } if dialect != "bibliogramma" => self.blocks(children, style),
            Block::Para {
                symbol,
                lemma,
                children,
                hypograph,
                ann,
                ..
            } => {
                let s = symbol.as_str();
                if NOTE_SYMBOLS.contains(&s) && ann.onym.is_some() && !self.in_note {
                    // Written into its part when referenced.
                    return;
                }
                match s {
                    "=" => {
                        self.title
                            .get_or_insert_with(|| plain(lemma_or(lemma, children)));
                        self.title_like("Title", lemma, children);
                    }
                    "=_" => self.title_like("Subtitle", lemma, children),
                    "=:" => {
                        self.creator
                            .get_or_insert_with(|| plain(lemma_or(lemma, children)));
                        self.title_like("Author", lemma, children);
                    }
                    "=;" | "-/" => self.title_like("Author", lemma, children),
                    "=\"" => self.blocks(children, Some("Abstract")),
                    "=#=" => {}
                    "==" | "===" | "#" | ":=" => {
                        self.lemma_paragraph(Some("Heading1"), lemma, false);
                        self.blocks(children, None);
                    }
                    "##" | ":#" => {
                        self.lemma_paragraph(Some("Heading2"), lemma, false);
                        self.blocks(children, None);
                    }
                    "###" => {
                        self.lemma_paragraph(Some("Heading3"), lemma, false);
                        self.blocks(children, None);
                    }
                    "####" => {
                        self.lemma_paragraph(Some("Heading4"), lemma, false);
                        self.blocks(children, None);
                    }
                    "\"" | "\"/" | ":[" => {
                        self.lemma_paragraph(Some("Quote"), lemma, true);
                        self.blocks(children, Some("Quote"));
                        if !hypograph.is_empty() {
                            self.paragraph(Some("Quote"), None, hypograph);
                        }
                    }
                    "--" | ".." => {
                        let ordered = s == "..";
                        let num = if ordered { self.fresh_num(true) } else { 1 };
                        self.list(children, 0, num);
                    }
                    "::;" => self.blocks(children, style),
                    "::" => {
                        self.lemma_paragraph(style, lemma, true);
                        self.blocks(children, style);
                    }
                    ";" => self.blocks(children, Some("ListParagraph")),
                    "+" => {
                        self.lemma_paragraph(Some("Caption"), lemma, false);
                        self.table(children);
                        if !hypograph.is_empty() {
                            self.paragraph(Some("Caption"), None, hypograph);
                        }
                    }
                    "<" => {
                        self.lemma_paragraph(Some("Caption"), lemma, false);
                        self.blocks(children, Some("Caption"));
                        if !hypograph.is_empty() {
                            self.paragraph(Some("Caption"), None, hypograph);
                        }
                    }
                    "$$" => self.blocks(children, style),
                    ":" | ":~" => {
                        // A speech: the speaker opens it.
                        if lemma.is_empty() {
                            self.blocks(children, style);
                        } else {
                            let mut first = lemma.clone();
                            first.push(Inline::Text(" ".to_string()));
                            let mut rest = children.iter();
                            match rest.next() {
                                Some(Block::Paragraph(p)) => {
                                    let mut inl = vec![endo("*", first)];
                                    inl.extend(p.iter().cloned());
                                    self.paragraph(style, None, &inl);
                                }
                                Some(other) => {
                                    self.lemma_paragraph(style, lemma, true);
                                    self.block(other, style);
                                }
                                None => self.lemma_paragraph(style, lemma, true),
                            }
                            for b in rest {
                                self.block(b, style);
                            }
                        }
                    }
                    _ => {
                        // Any other para-simmere: its lemma, then
                        // its content, then its hypograph.
                        self.lemma_paragraph(style, lemma, true);
                        self.blocks(children, style);
                        if !hypograph.is_empty() {
                            self.paragraph(style, None, hypograph);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn title_like(&mut self, style: &str, lemma: &[Inline], children: &[Block]) {
        let inlines = lemma_or(lemma, children);
        self.paragraph(Some(style), None, inlines);
    }

    // ----- the package -------------------------------------------

    fn sources_xml(&self) -> Option<String> {
        if self.bibliography.is_empty() {
            return None;
        }
        let mut out = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<b:Sources xmlns:b=\"http://schemas.openxmlformats.org/officeDocument/2006/bibliography\" xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/bibliography\" SelectedStyle=\"\\APASixthEditionOfficeOnline.xsl\" StyleName=\"APA\" Version=\"6\">",
        );
        for entry in &self.bibliography {
            let Block::Para {
                lemma,
                children,
                ann,
                ..
            } = entry
            else {
                continue;
            };
            let tag = plain(lemma).trim().to_string();
            if tag.is_empty() {
                continue;
            }
            let kind = match ann.genoses.first().map(String::as_str) {
                Some("book" | "liber") => "Book",
                Some("article" | "commentarius") => "JournalArticle",
                Some("incollection" | "membrum") => "BookSection",
                Some("inproceedings" | "relatio") => "ConferenceProceedings",
                Some("report" | "renuntiatio") => "Report",
                Some("online") => "InternetSite",
                _ => "Misc",
            };
            out.push_str(&format!(
                "<b:Source><b:Tag>{}</b:Tag><b:SourceType>{kind}</b:SourceType>",
                xml(&tag)
            ));
            let mut authors: Vec<String> = Vec::new();
            let mut editors: Vec<String> = Vec::new();
            let mut rest: Vec<(String, String)> = Vec::new();
            for field in children {
                let Block::Para {
                    lemma, children, ..
                } = field
                else {
                    continue;
                };
                let name = plain(lemma).trim().to_string();
                let value = children
                    .iter()
                    .map(|b| match b {
                        Block::Paragraph(p) => plain(p),
                        _ => String::new(),
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                match name.as_str() {
                    "author" | "auctor" => authors.extend(value.split(" and ").map(str::to_string)),
                    "editor" => editors.extend(value.split(" and ").map(str::to_string)),
                    "title" | "titulus" => rest.push(("Title".into(), value)),
                    "booktitle" | "titulus-libri" => rest.push(("BookTitle".into(), value)),
                    "journal" | "journaltitle" | "ephemeris" => {
                        rest.push(("JournalName".into(), value))
                    }
                    "year" | "annus" => rest.push(("Year".into(), value)),
                    "date" | "datum" => rest.push(("Year".into(), value)),
                    "location" | "address" | "locus" => rest.push(("City".into(), value)),
                    "publisher" | "officina" => rest.push(("Publisher".into(), value)),
                    "volume" | "volumen" => rest.push(("Volume".into(), value)),
                    "number" | "numerus" => rest.push(("Issue".into(), value)),
                    "pages" | "paginae" => rest.push(("Pages".into(), value)),
                    "edition" | "editio" => rest.push(("Edition".into(), value)),
                    "url" => rest.push(("URL".into(), value)),
                    "doi" => rest.push(("DOI".into(), value)),
                    "institution" | "institutum" => rest.push(("Institution".into(), value)),
                    "note" | "nota" => rest.push(("Comments".into(), value)),
                    _ => {}
                }
            }
            let names = |list: &[String]| {
                let mut s = String::new();
                for n in list {
                    let n = n.trim();
                    if n.is_empty() {
                        continue;
                    }
                    match n.split_once(", ") {
                        Some((last, first)) => s.push_str(&format!(
                            "<b:Person><b:Last>{}</b:Last><b:First>{}</b:First></b:Person>",
                            xml(last),
                            xml(first)
                        )),
                        None => {
                            s.push_str(&format!("<b:Person><b:Last>{}</b:Last></b:Person>", xml(n)))
                        }
                    }
                }
                s
            };
            if !authors.is_empty() || !editors.is_empty() {
                out.push_str("<b:Author>");
                if !authors.is_empty() {
                    out.push_str(&format!(
                        "<b:Author><b:NameList>{}</b:NameList></b:Author>",
                        names(&authors)
                    ));
                }
                if !editors.is_empty() {
                    out.push_str(&format!(
                        "<b:Editor><b:NameList>{}</b:NameList></b:Editor>",
                        names(&editors)
                    ));
                }
                out.push_str("</b:Author>");
            }
            for (k, v) in rest {
                if !v.is_empty() {
                    out.push_str(&format!("<b:{k}>{}</b:{k}>", xml(&v)));
                }
            }
            out.push_str("</b:Source>");
        }
        out.push_str("</b:Sources>");
        Some(out)
    }

    fn finish(self) -> Result<Vec<u8>> {
        use std::io::Write;
        const NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"";
        let decl = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
        let document = format!(
            "{decl}<w:document {NS}><w:body>{}<w:sectPr/></w:body></w:document>",
            self.body
        );
        let notes_part = |root: &str, element: &str, sep: &str, notes: &[(String, String)]| {
            let mut s = format!("{decl}<w:{root} {NS}>");
            s.push_str(&format!(
                "<w:{element} w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:{element}><w:{element} w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:{sep}/></w:r></w:p></w:{element}>"
            ));
            for (n, (_, body)) in notes.iter().enumerate() {
                s.push_str(&format!(
                    "<w:{element} w:id=\"{}\">{body}</w:{element}>",
                    n + 1
                ));
            }
            s.push_str(&format!("</w:{root}>"));
            s
        };
        let footnotes = notes_part(
            "footnotes",
            "footnote",
            "continuationSeparator",
            &self.footnotes,
        );
        let endnotes = notes_part(
            "endnotes",
            "endnote",
            "continuationSeparator",
            &self.endnotes,
        );
        let mut comments = format!("{decl}<w:comments {NS}>");
        for (n, (_, body)) in self.comments.iter().enumerate() {
            comments.push_str(&format!(
                "<w:comment w:id=\"{}\" w:author=\"atrep\" w:initials=\"a\">{body}</w:comment>",
                n + 1
            ));
        }
        comments.push_str("</w:comments>");
        let styles = format!("{decl}{}", STYLES);
        let mut numbering = format!("{decl}<w:numbering {NS}>{}", ABSTRACT_NUMS);
        numbering.push_str("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>");
        for (id, ordered) in &self.numbering {
            let abs = if *ordered { 1 } else { 0 };
            numbering.push_str(&format!(
                "<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"{abs}\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride></w:num>"
            ));
        }
        numbering.push_str("</w:numbering>");
        let sources = self.sources_xml();
        let mut rels = self.rels.clone();
        if sources.is_some() {
            rels.push((
                format!("rId{}", rels.len() + 1),
                "customXml".to_string(),
                "../customXml/item1.xml".to_string(),
                false,
            ));
        }
        let mut doc_rels = format!(
            "{decl}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">"
        );
        for (id, ty, target, external) in &rels {
            let mode = if *external {
                " TargetMode=\"External\""
            } else {
                ""
            };
            doc_rels.push_str(&format!(
                "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{ty}\" Target=\"{}\"{mode}/>",
                xml(target)
            ));
        }
        doc_rels.push_str("</Relationships>");
        let root_rels = format!(
            "{decl}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/></Relationships>"
        );
        let core = format!(
            "{decl}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">{}{}</cp:coreProperties>",
            self.title
                .as_deref()
                .map(|t| format!("<dc:title>{}</dc:title>", xml(t)))
                .unwrap_or_default(),
            self.creator
                .as_deref()
                .map(|c| format!("<dc:creator>{}</dc:creator>", xml(c)))
                .unwrap_or_default()
        );
        let mut types = format!(
            "{decl}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>"
        );
        let mut seen_ext: HashSet<String> = HashSet::new();
        for (name, _) in &self.media {
            let ext = name
                .rsplit('.')
                .next()
                .unwrap_or("bin")
                .to_ascii_lowercase();
            if seen_ext.insert(ext.clone()) {
                types.push_str(&format!(
                    "<Default Extension=\"{ext}\" ContentType=\"{}\"/>",
                    content_type_of(name)
                ));
            }
        }
        for (part, ty) in [
            (
                "/word/document.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            ),
            (
                "/word/styles.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
            ),
            (
                "/word/numbering.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
            ),
            (
                "/word/footnotes.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
            ),
            (
                "/word/endnotes.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml",
            ),
            (
                "/word/comments.xml",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
            ),
            (
                "/docProps/core.xml",
                "application/vnd.openxmlformats-package.core-properties+xml",
            ),
        ] {
            types.push_str(&format!(
                "<Override PartName=\"{part}\" ContentType=\"{ty}\"/>"
            ));
        }
        if sources.is_some() {
            types.push_str("<Override PartName=\"/customXml/itemProps1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.customXmlProperties+xml\"/>");
        }
        types.push_str("</Types>");

        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut put = |name: &str, data: &[u8]| -> Result<()> {
            zip.start_file(name, opts)
                .and_then(|_| zip.write_all(data).map_err(zip::result::ZipError::Io))
                .map_err(|e| docx_err(format!("writing {name}: {e}")))
        };
        put("[Content_Types].xml", types.as_bytes())?;
        put("_rels/.rels", root_rels.as_bytes())?;
        put("word/document.xml", document.as_bytes())?;
        put("word/_rels/document.xml.rels", doc_rels.as_bytes())?;
        put("word/styles.xml", styles.as_bytes())?;
        put("word/numbering.xml", numbering.as_bytes())?;
        put("word/footnotes.xml", footnotes.as_bytes())?;
        put("word/endnotes.xml", endnotes.as_bytes())?;
        put("word/comments.xml", comments.as_bytes())?;
        put("docProps/core.xml", core.as_bytes())?;
        for (name, bytes) in &self.media {
            put(&format!("word/media/{name}"), bytes)?;
        }
        if let Some(sources) = sources {
            put("customXml/item1.xml", sources.as_bytes())?;
            put(
                "customXml/itemProps1.xml",
                format!("{decl}<ds:datastoreItem ds:itemID=\"{{B1C2D3E4-0000-4000-8000-000000000001}}\" xmlns:ds=\"http://schemas.openxmlformats.org/officeDocument/2006/customXml\"><ds:schemaRefs><ds:schemaRef ds:uri=\"http://schemas.openxmlformats.org/officeDocument/2006/bibliography\"/></ds:schemaRefs></ds:datastoreItem>").as_bytes(),
            )?;
            put(
                "customXml/_rels/item1.xml.rels",
                format!("{decl}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps\" Target=\"itemProps1.xml\"/></Relationships>").as_bytes(),
            )?;
        }
        let cursor = zip
            .finish()
            .map_err(|e| docx_err(format!("finishing the zip: {e}")))?;
        Ok(cursor.into_inner())
    }
}

/// A cell's spans from the monosims opening its first paragraph.
fn spans_of(blocks: &[Block]) -> (usize, usize) {
    let mut h = 1;
    let mut v = 1;
    if let Some(Block::Paragraph(p)) = blocks.first() {
        for inline in p.iter().take_while(|i| matches!(i, Inline::Monosim { .. })) {
            if let Inline::Monosim { symbol, param, .. } = inline {
                let n = param.parse::<usize>().unwrap_or(1).max(1);
                match symbol.as_str() {
                    "+>" => h = n,
                    "+_" => v = n,
                    _ => {}
                }
            }
        }
    }
    (h, v)
}

/// The cell's blocks without the span monosims.
fn strip_spans(blocks: &[Block]) -> Vec<Block> {
    let mut out = blocks.to_vec();
    if let Some(Block::Paragraph(p)) = out.first_mut() {
        p.retain(
            |i| !matches!(i, Inline::Monosim { symbol, .. } if symbol == "+>" || symbol == "+_"),
        );
        if let Some(Inline::Text(t)) = p.first_mut() {
            *t = t.trim_start().to_string();
            if t.is_empty() {
                p.remove(0);
            }
        }
        if p.is_empty() {
            out.remove(0);
        }
    }
    out
}

/// A solo endo-sim's content (the title line) or a para-sim's
/// lemma, whichever the block carries.
fn lemma_or<'b>(lemma: &'b [Inline], children: &'b [Block]) -> &'b [Inline] {
    if !lemma.is_empty() {
        return lemma;
    }
    match children.first() {
        Some(Block::Paragraph(p)) => p,
        _ => lemma,
    }
}

const STYLES: &str = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:cs="Times New Roman"/><w:sz w:val="24"/><w:lang w:val="en-US"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:jc w:val="center"/><w:spacing w:before="240" w:after="240"/></w:pPr><w:rPr><w:b/><w:sz w:val="40"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:jc w:val="center"/></w:pPr><w:rPr><w:i/><w:sz w:val="28"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Author"><w:name w:val="Author"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:jc w:val="center"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Abstract"><w:name w:val="Abstract"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:ind w:left="720" w:right="720"/></w:pPr><w:rPr><w:sz w:val="22"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="360" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before="200" w:after="80"/><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:b/><w:sz w:val="26"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading4"><w:name w:val="heading 4"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val="3"/></w:pPr><w:rPr><w:b/><w:i/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:ind w:left="720" w:right="720"/></w:pPr><w:rPr><w:i/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="SourceCode"><w:name w:val="Source Code"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:cs="Courier New"/><w:sz w:val="20"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Caption"><w:name w:val="caption"/><w:basedOn w:val="Normal"/><w:qFormat/><w:rPr><w:i/><w:sz w:val="20"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="ListParagraph"><w:name w:val="List Paragraph"/><w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:ind w:left="720"/><w:contextualSpacing/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="FootnoteText"><w:name w:val="footnote text"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:sz w:val="20"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="EndnoteText"><w:name w:val="endnote text"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:sz w:val="20"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="CommentText"><w:name w:val="annotation text"/><w:basedOn w:val="Normal"/><w:rPr><w:sz w:val="20"/></w:rPr></w:style>
<w:style w:type="character" w:default="1" w:styleId="DefaultParagraphFont"><w:name w:val="Default Paragraph Font"/></w:style>
<w:style w:type="character" w:styleId="Strong"><w:name w:val="Strong"/><w:qFormat/><w:rPr><w:b/></w:rPr></w:style>
<w:style w:type="character" w:styleId="Emphasis"><w:name w:val="Emphasis"/><w:qFormat/><w:rPr><w:i/></w:rPr></w:style>
<w:style w:type="character" w:styleId="VerbatimChar"><w:name w:val="Verbatim Char"/><w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:cs="Courier New"/><w:sz w:val="20"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="FootnoteReference"><w:name w:val="footnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>
<w:style w:type="character" w:styleId="EndnoteReference"><w:name w:val="endnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>
<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>
</w:styles>"#;

const ABSTRACT_NUMS: &str = r#"<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="hybridMultilevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="&#8226;"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl><w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="&#9702;"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="1440" w:hanging="360"/></w:pPr></w:lvl><w:lvl w:ilvl="2"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="&#9642;"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="2160" w:hanging="360"/></w:pPr></w:lvl><w:lvl w:ilvl="3"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="&#8226;"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="2880" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum><w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="hybridMultilevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl><w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%2."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="1440" w:hanging="360"/></w:pPr></w:lvl><w:lvl w:ilvl="2"><w:start w:val="1"/><w:numFmt w:val="lowerRoman"/><w:lvlText w:val="%3."/><w:lvlJc w:val="right"/><w:pPr><w:ind w:left="2160" w:hanging="180"/></w:pPr></w:lvl><w:lvl w:ilvl="3"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%4."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="2880" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum>"#;

/// Export a litogramma document as a Word document; `read_media`
/// maps an enmedia parameter (`media/fence.png`) to its bytes.
pub fn document_to_docx(
    doc: &Document,
    read_media: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Vec<u8>> {
    let mut w = Writer::new(read_media);
    w.gather(&doc.blocks);
    w.blocks(&doc.blocks, None);
    w.finish()
}
