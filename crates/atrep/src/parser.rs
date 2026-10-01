//! Document parser: Atrep source text to a preliminary Dendron
//! (spec: chapters "Basic Syntactic Structures", "Atrep Core
//! Syntax", "Parsing into Atrep AST").
//!
//! The parser is strict: any error condition rejects the document.
//! Comments are dropped here; whitespace normalization and
//! paragraph collapsing happen in kanonizo.

use std::path::{Path, PathBuf};

use crate::dendron::{Annotations, Block, Document, Inline, Strophe, Taxis};
use crate::dialektos::{self, Dialektos, NamedLookup, Optionality, SimDef, SimForm};
use crate::error::{Error, ErrorKind, Location, Result};
use crate::sigil::{self, Sigil};

fn parse_inner(
    source: &str,
    path: &Path,
) -> Result<(Document, Vec<crate::outline::OutlineBlock>, Dialektos)> {
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut lines: Vec<String> = source
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect();
    let mut first = 0;

    // Shebang.
    if lines.first().is_some_and(|l| l.starts_with("#!")) {
        first = 1;
    }

    // Dialektos declaration determines the sigil.
    let decl_line = lines
        .get(first)
        .ok_or_else(|| Error::new(ErrorKind::MissingDeclaration))?
        .clone();
    let (sig, decl_rest) = if let Some(rest) = decl_line.strip_prefix("@@@!") {
        (Sigil::Canonical, rest)
    } else if let Some(rest) = decl_line.strip_prefix("\\\\\\!") {
        (Sigil::Alias, rest)
    } else {
        return Err(Error::at(
            ErrorKind::MissingDeclaration,
            Location {
                file: path.to_path_buf(),
                line: first + 1,
                col: 1,
            },
        ));
    };
    let (dialect_id, dialect_version) = parse_declaration(decl_rest, path, first + 1)?;
    if dialect_id == "atrep" {
        return Err(Error::at(
            ErrorKind::InvalidDeclaration(
                "`atrep` declares a dialektos definition file, not a document; \
                 process it as a definition"
                    .to_string(),
            ),
            Location {
                file: path.to_path_buf(),
                line: first + 1,
                col: 1,
            },
        ));
    }
    let dial = dialektos::resolve(&dir, &dialect_id)?;

    // Blank out consumed lines so indices stay stable.
    for l in lines.iter_mut().take(first + 1) {
        l.clear();
    }

    let mut parser = Parser {
        lines,
        idx: first + 1,
        sigil: sig,
        file: path.to_path_buf(),
        dir,
        depth: 0,
        outline: Vec::new(),
    };
    let blocks = parser.parse_blocks(&dial, None)?;
    let doc = Document {
        dialect_id,
        dialect_version,
        blocks,
    };
    Ok((doc, parser.outline, dial))
}

/// Parse a document from source text. `path` locates error reports
/// and is the base for dialektos resolution (its parent directory).
pub fn parse_document(source: &str, path: &Path) -> Result<Document> {
    parse_inner(source, path).map(|(doc, _, _)| doc)
}

/// [`parse_document`], also returning the recorded outline blocks
/// and the resolved dialektos (see [`crate::outline`]).
pub fn parse_document_outline(
    source: &str,
    path: &Path,
) -> Result<(Document, Vec<crate::outline::OutlineBlock>, Dialektos)> {
    parse_inner(source, path)
}

/// Parse `<dialect-id>[@<version>]`.
fn parse_declaration(rest: &str, path: &Path, line: usize) -> Result<(String, Option<String>)> {
    let loc = Location {
        file: path.to_path_buf(),
        line,
        col: 1,
    };
    let rest = rest.trim_end();
    let (id, version) = match rest.find('@') {
        Some(i) => (&rest[..i], Some(rest[i + 1..].to_string())),
        None => (rest, None),
    };
    if !sigil::is_valid_name(id) {
        return Err(Error::at(
            ErrorKind::InvalidDeclaration(rest.to_string()),
            loc,
        ));
    }
    if let Some(v) = &version {
        let valid_version = !v.is_empty()
            && v.split('.').count() <= 2
            && v.split('.')
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
        if !valid_version {
            return Err(Error::at(
                ErrorKind::InvalidDeclaration(rest.to_string()),
                loc,
            ));
        }
    }
    Ok((id.to_string(), version))
}

/// A resolved sim reference after a monograph sigil: the
/// definition, the reference's source length, and whether it was
/// plerographic (`@{name}`, spec: "Metagraphe").
struct SimRef<'a> {
    def: &'a SimDef,
    /// Byte length of the reference (symbol, or `{name}`).
    bytes: usize,
    /// Char length of the reference (for char-indexed scanners).
    chars: usize,
    /// The braced name as written, for a plerographic reference
    /// (the closer echoes it; a glossa name closes as itself).
    plero: Option<String>,
}

/// Resolve the text after a monograph sigil as a sim reference:
/// brachygraphic (longest symbol match) or plerographic
/// (`{name}`). `Ok(None)` when no symbol matches; an unknown or
/// ambiguous braced name is an error.
fn resolve_sim_ref<'a>(
    dial: &'a Dialektos,
    text: &str,
    loc: Location,
) -> Result<Option<SimRef<'a>>> {
    if let Some(rest) = text.strip_prefix('{') {
        let Some(end) = rest.find('}') else {
            return Err(Error::at(
                ErrorKind::Syntax("unterminated plerographic name reference".into()),
                loc,
            ));
        };
        let name = &rest[..end];
        return match dial.sim_named(name) {
            NamedLookup::One(def) => Ok(Some(SimRef {
                def,
                bytes: name.len() + 2,
                chars: name.chars().count() + 2,
                plero: Some(name.to_string()),
            })),
            NamedLookup::None => Err(Error::at(
                ErrorKind::UndefinedSim(format!("{{{name}}}")),
                loc,
            )),
            NamedLookup::Ambiguous => Err(Error::at(
                ErrorKind::Syntax(format!(
                    "sim name `{name}` is duplicated in this dialektos and \
                     not plerographically addressable"
                )),
                loc,
            )),
        };
    }
    Ok(dial.longest_match(text).map(|def| SimRef {
        def,
        bytes: def.symbol.len(),
        chars: def.symbol.chars().count(),
        plero: None,
    }))
}

/// Whether a para-simmere reference at line start is followed by
/// a parenthesized group that cannot be a taxis - in which case
/// the line begins a paragraph and the group is a deixis
/// reference.
fn deixis_not_taxis(def: &SimDef, rest: &str) -> bool {
    let Some(inner) = rest.strip_prefix('(') else {
        return false;
    };
    let Some(end) = inner.find(')') else {
        return false;
    };
    let group = &inner[..end];
    let taxis_supported = !matches!(
        def.form,
        SimForm::Para {
            taxis: Optionality::Unsupported,
            ..
        }
    );
    let is_taxis_shaped = group.is_empty() || group.chars().all(|c| c.is_ascii_digit());
    !(taxis_supported && is_taxis_shaped)
}

struct Parser {
    lines: Vec<String>,
    idx: usize,
    sigil: Sigil,
    file: PathBuf,
    dir: PathBuf,
    /// Structural recording for the outline (spec-adjacent
    /// tooling surface); depth tracks parse_blocks recursion.
    depth: usize,
    outline: Vec<crate::outline::OutlineBlock>,
}

/// Outcome of classifying a line at para level.
enum ParaStep {
    /// The line begins a block construct which was consumed.
    Block(Block),
    /// The line was consumed without producing a block (comment).
    Skipped,
    /// The line is paragraph material.
    Text,
}

impl Parser {
    fn loc(&self, line: usize) -> Location {
        Location {
            file: self.file.clone(),
            line: line + 1,
            col: 1,
        }
    }

    fn sig(&self) -> char {
        self.sigil.active()
    }

    /// Trigraph string of the active sigil.
    fn tri(&self) -> String {
        std::iter::repeat_n(self.sig(), 3).collect()
    }

    /// Record a completed structural block for the outline
    /// (paragraphs and self-delimiting one-liners are skipped).
    fn record_outline(&mut self, block: &Block, start: usize) {
        use crate::outline::{OutlineBlock, inline_text};
        let end = self.idx.max(start + 1); // idx is one past the closer line
        let (kind, symbol, lemma, ann) = match block {
            Block::Para {
                symbol, lemma, ann, ..
            } => ("para", Some(symbol.clone()), inline_text(lemma), ann),
            Block::Stichoi {
                symbol, lemma, ann, ..
            } => ("stichoi", symbol.clone(), inline_text(lemma), ann),
            Block::ParaDiaphane { ann, .. } => ("diaphane", None, String::new(), ann),
            Block::MonadEnglossis { dialect, ann, .. } => {
                ("englossis", Some(dialect.clone()), String::new(), ann)
            }
            _ => return,
        };
        self.outline.push(OutlineBlock {
            kind,
            symbol,
            depth: self.depth - 1,
            start: start + 1,
            end,
            lemma,
            onym: ann.onym.clone(),
            genoses: ann.genoses.clone(),
        });
    }

    fn parse_blocks(&mut self, dial: &Dialektos, closer: Option<&str>) -> Result<Vec<Block>> {
        self.depth += 1;
        let result = self.parse_blocks_inner(dial, closer);
        self.depth -= 1;
        result
    }

    fn parse_blocks_inner(&mut self, dial: &Dialektos, closer: Option<&str>) -> Result<Vec<Block>> {
        let opened_at = self.idx;
        let mut blocks = Vec::new();
        while self.idx < self.lines.len() {
            let line = self.lines[self.idx].trim_start().to_string();
            if line.is_empty() {
                self.idx += 1;
                continue;
            }
            if let Some(cl) = closer
                && line.starts_with(cl)
            {
                return Ok(blocks);
            }
            let start = self.idx;
            match self.step_para(dial, &line)? {
                ParaStep::Block(b) => {
                    self.record_outline(&b, start);
                    blocks.push(b);
                }
                ParaStep::Skipped => {}
                ParaStep::Text => blocks.push(self.collect_paragraph(dial, closer)?),
            }
        }
        if let Some(cl) = closer {
            return Err(Error::at(
                ErrorKind::UnmatchedSim(format!("block closed by `{cl}` (opened near here)")),
                self.loc(opened_at.saturating_sub(1)),
            ));
        }
        Ok(blocks)
    }

    /// Classify the line at `self.idx` (trimmed as `line`) and, if it
    /// begins a block construct, consume and build it.
    fn step_para(&mut self, dial: &Dialektos, line: &str) -> Result<ParaStep> {
        let sig = self.sig();
        if !line.starts_with(sig) {
            return Ok(ParaStep::Text);
        }
        let run = line.chars().take_while(|&c| c == sig).count();
        if run > 4 {
            return Err(Error::at(
                ErrorKind::SigilRunTooLong(run),
                self.loc(self.idx),
            ));
        }
        let after: String = line.chars().skip(run).collect();
        match run {
            1 => {
                if after.starts_with('(') {
                    return Ok(ParaStep::Text); // standalone onym in a paragraph
                }
                match resolve_sim_ref(dial, &after, self.loc(self.idx))? {
                    Some(r) if matches!(r.def.form, SimForm::Para { .. }) => {
                        // A group after the reference that cannot be
                        // a taxis (any group on a taxis-less sim; a
                        // non-empty non-numeric group otherwise)
                        // means this line starts an ordinary
                        // paragraph containing a deixis.
                        let rest = &after[r.bytes..];
                        if deixis_not_taxis(r.def, rest) {
                            return Ok(ParaStep::Text);
                        }
                        let def = r.def.clone();
                        let plero = r.plero.clone();
                        let rest = rest.to_string();
                        Ok(ParaStep::Block(
                            self.parse_para_simmere(dial, &def, &rest, plero)?,
                        ))
                    }
                    Some(_) => Ok(ParaStep::Text),
                    None => Err(Error::at(
                        ErrorKind::UndefinedSim(format!(
                            "{sig}{}",
                            after
                                .chars()
                                .take_while(|&c| sigil::is_symbolic(c))
                                .collect::<String>()
                        )),
                        self.loc(self.idx),
                    )),
                }
            }
            2 => Ok(ParaStep::Text), // digraph forms are inline
            3 => self.step_trigraph(dial, &after),
            4 => {
                let Some(param) = after.strip_prefix('(').and_then(|r| r.strip_suffix(')')) else {
                    return Err(Error::at(
                        ErrorKind::Syntax("malformed enmedia monosim".into()),
                        self.loc(self.idx),
                    ));
                };
                let param = &self.check_param(param)?;
                self.idx += 1;
                Ok(ParaStep::Block(Block::Enmedia {
                    param: param.to_string(),
                }))
            }
            _ => unreachable!(),
        }
    }

    fn step_trigraph(&mut self, dial: &Dialektos, after: &str) -> Result<ParaStep> {
        let start = self.idx;
        let tri = self.tri();
        match after.chars().next() {
            Some('!') => {
                let rest = &after[1..];
                let Some(inner) = rest.strip_prefix('(') else {
                    return Err(Error::at(
                        ErrorKind::Syntax(
                            "dialektos declaration is only valid on the first content line".into(),
                        ),
                        self.loc(start),
                    ));
                };
                let Some(dialect) = inner.strip_suffix(')') else {
                    return Err(Error::at(
                        ErrorKind::Syntax("malformed monad-englossis header".into()),
                        self.loc(start),
                    ));
                };
                self.check_param(dialect)?;
                let inner_dial = dialektos::resolve(&self.dir, dialect)?;
                self.idx += 1;
                let closer = format!("!{tri}");
                let children = self.parse_blocks(&inner_dial, Some(&closer))?;
                let ann = self.consume_closer(&closer, false)?;
                Ok(ParaStep::Block(Block::MonadEnglossis {
                    dialect: dialect.to_string(),
                    children,
                    ann,
                }))
            }
            Some('/') => {
                // Block comment at para level.
                let close = format!("/{tri}");
                let open_rest = after[1..].trim();
                if let Some(tail) = open_rest.strip_suffix(close.as_str()) {
                    // Single-line standalone comment; anything after
                    // the close would be inline context.
                    if tail.trim_end().ends_with('/') && open_rest == "/" {
                        // degenerate `@@@//@@@`
                    }
                    self.idx += 1;
                    return Ok(ParaStep::Skipped);
                }
                if open_rest.contains(close.as_str()) {
                    // Close mid-line: inline usage, part of a paragraph.
                    return Ok(ParaStep::Text);
                }
                let mut end = self.idx + 1;
                while end < self.lines.len() {
                    let l = self.lines[end].trim();
                    if l == close {
                        self.idx = end + 1;
                        return Ok(ParaStep::Skipped);
                    }
                    if let Some(after_close) = l.strip_suffix(close.as_str())
                        && after_close.trim().is_empty()
                    {
                        self.idx = end + 1;
                        return Ok(ParaStep::Skipped);
                    }
                    if l.contains(close.as_str()) {
                        // Close with trailing text: the comment glues
                        // into a paragraph; let the inline scanner
                        // handle the whole span.
                        return Ok(ParaStep::Text);
                    }
                    end += 1;
                }
                Err(Error::at(
                    ErrorKind::UnmatchedSim(format!("{tri}/")),
                    self.loc(start),
                ))
            }
            Some('"') => {
                if !after[1..].trim().is_empty() {
                    return Err(Error::at(
                        ErrorKind::Syntax("content after monad-enlexis opener".into()),
                        self.loc(start),
                    ));
                }
                self.idx += 1;
                let close = format!("\"{tri}");
                let content_start = self.idx;
                while self.idx < self.lines.len() {
                    if self.lines[self.idx]
                        .trim_start()
                        .starts_with(close.as_str())
                    {
                        let content = self.lines[content_start..self.idx].join("\n");
                        let ann = self.consume_closer(&close, false)?;
                        return Ok(ParaStep::Block(Block::VerbatimBlock {
                            content: if content.is_empty() {
                                content
                            } else {
                                content + "\n"
                            },
                            ann,
                        }));
                    }
                    self.idx += 1;
                }
                Err(Error::at(
                    ErrorKind::UnmatchedSim(format!("{tri}\"")),
                    self.loc(start),
                ))
            }
            Some('.') => {
                self.idx += 1;
                let closer = format!(".{tri}");
                let children = self.parse_blocks(dial, Some(&closer))?;
                let ann = self.consume_closer(&closer, false)?;
                Ok(ParaStep::Block(Block::ParaDiaphane { children, ann }))
            }
            Some(':') => {
                self.idx += 1;
                let closer = format!(":{tri}");
                let children = self.parse_blocks(dial, Some(&closer))?;
                // Closing line carries the mandatory `(onym)`.
                let line = self.lines[self.idx].trim_start().to_string();
                let rest = &line[closer.len()..];
                let Some(onym) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) else {
                    return Err(Error::at(
                        ErrorKind::Syntax("para-axioma requires `(onym)` on its episim".into()),
                        self.loc(self.idx),
                    ));
                };
                if !sigil::is_valid_onym(onym) {
                    return Err(Error::at(
                        ErrorKind::InvalidOnym(onym.to_string()),
                        self.loc(self.idx),
                    ));
                }
                self.idx += 1;
                Ok(ParaStep::Block(Block::ParaAxioma {
                    onym: onym.to_string(),
                    children,
                }))
            }
            Some('=') => self.parse_stichoi(dial, &after[1..]).map(ParaStep::Block),
            Some('(') => {
                let Some(param) = after.strip_prefix('(').and_then(|r| r.strip_suffix(')')) else {
                    return Err(Error::at(
                        ErrorKind::Syntax("malformed trigraph monosim".into()),
                        self.loc(start),
                    ));
                };
                let param = &self.check_param(param)?;
                self.idx += 1;
                Ok(ParaStep::Block(match axioma_ref(param) {
                    Some(onym) => Block::AxiomaRefBlock {
                        onym,
                        enlexis: false,
                    },
                    None => Block::AnaphorEnglossis {
                        target: param.to_string(),
                    },
                }))
            }
            Some('+') => {
                let Some(param) = after[1..]
                    .strip_prefix('(')
                    .and_then(|r| r.strip_suffix(')'))
                else {
                    return Err(Error::at(
                        ErrorKind::Syntax("malformed anaphor-enlexis monosim".into()),
                        self.loc(start),
                    ));
                };
                let param = &self.check_param(param)?;
                self.idx += 1;
                Ok(ParaStep::Block(match axioma_ref(param) {
                    Some(onym) => Block::AxiomaRefBlock {
                        onym,
                        enlexis: true,
                    },
                    None => Block::AnaphorEnlexis {
                        target: param.to_string(),
                    },
                }))
            }
            other => Err(Error::at(
                ErrorKind::UndefinedSim(format!(
                    "{tri}{}",
                    other.map(String::from).unwrap_or_default()
                )),
                self.loc(start),
            )),
        }
    }

    fn parse_stichoi(&mut self, dial: &Dialektos, rest_of_open: &str) -> Result<Block> {
        let start = self.idx;
        let tri = self.tri();
        let lemma_text = rest_of_open.trim();
        let lemma = self.scan_line(dial, lemma_text, start)?;
        self.idx += 1;
        let close = format!("={tri}");
        let mut strophes: Vec<Strophe> = Vec::new();
        let mut current: Vec<Vec<Inline>> = Vec::new();
        loop {
            if self.idx >= self.lines.len() {
                return Err(Error::at(
                    ErrorKind::UnmatchedSim(format!("{tri}=")),
                    self.loc(start),
                ));
            }
            let raw = self.lines[self.idx].clone();
            let trimmed = raw.trim();
            if trimmed.starts_with(close.as_str()) {
                break;
            }
            if trimmed.is_empty() {
                if !current.is_empty() {
                    strophes.push(Strophe(std::mem::take(&mut current)));
                }
            } else {
                // Leading and internal whitespace is authorial
                // content in a stichos; only trailing whitespace
                // is dropped.
                current.push(self.scan_line(dial, raw.trim_end(), self.idx)?);
            }
            self.idx += 1;
        }
        if !current.is_empty() {
            strophes.push(Strophe(current));
        }
        // Closing line: annotations + optional hypograph.
        let line = self.lines[self.idx].trim_start().to_string();
        let rest = line[close.len()..].to_string();
        let (ann, tail) = self.scan_annotations_str(&rest, self.idx)?;
        let hypograph = self.scan_line(dial, tail.trim(), self.idx)?;
        self.idx += 1;
        Ok(Block::Stichoi {
            symbol: None,
            taxis: None,
            lemma,
            strophes,
            hypograph,
            bracket_matching: true,
            ann,
        })
    }

    /// Consume the current line as a para/stichoi episim line:
    /// annotations, then an optional hypograph validated against
    /// the definition.
    fn parse_episim_line(
        &mut self,
        dial: &Dialektos,
        def: &SimDef,
        closer: &str,
        hypo_opt: Optionality,
    ) -> Result<(Annotations, Vec<Inline>)> {
        let episim_line = self.idx;
        let line = self.lines[self.idx].trim_start().to_string();
        let rest = line[closer.len()..].to_string();
        let (ann, tail) = self.scan_annotations_str(&rest, episim_line)?;
        let hypo_text = tail.trim();
        match (hypo_text.is_empty(), hypo_opt) {
            (false, Optionality::Unsupported) => {
                return Err(Error::at(
                    ErrorKind::ComponentViolation(format!(
                        "sim `{}` does not support a hypograph",
                        def.name
                    )),
                    self.loc(episim_line),
                ));
            }
            (true, Optionality::Required) => {
                return Err(Error::at(
                    ErrorKind::ComponentViolation(format!(
                        "sim `{}` requires a hypograph",
                        def.name
                    )),
                    self.loc(episim_line),
                ));
            }
            _ => {}
        }
        let hypograph = self.scan_line(dial, hypo_text, episim_line)?;
        self.idx += 1;
        Ok((ann, hypograph))
    }

    fn parse_para_simmere(
        &mut self,
        dial: &Dialektos,
        def: &SimDef,
        after_ref: &str,
        plero: Option<String>,
    ) -> Result<Block> {
        let start = self.idx;
        let SimForm::Para {
            autonym: _,
            taxis: taxis_opt,
            lemma: lemma_opt,
            hypograph: hypo_opt,
            stichoi: stichoi_grammata,
        } = def.form
        else {
            unreachable!()
        };
        let mut rest = after_ref;

        // Taxis.
        let mut taxis = None;
        if let Some(inner) = rest.strip_prefix('(')
            && let Some(end) = inner.find(')')
        {
            let t = &inner[..end];
            if t.is_empty() {
                taxis = Some(Taxis::Auto);
            } else if t.chars().all(|c| c.is_ascii_digit()) {
                taxis = Some(Taxis::Explicit(t.parse().map_err(|_| {
                    Error::at(
                        ErrorKind::Syntax(format!("invalid taxis `{t}`")),
                        self.loc(start),
                    )
                })?));
            } else {
                return Err(Error::at(
                    ErrorKind::Syntax(format!("invalid taxis `{t}`")),
                    self.loc(start),
                ));
            }
            rest = &inner[end + 1..];
        }
        match (taxis.is_some(), taxis_opt) {
            (true, Optionality::Unsupported) => {
                return Err(Error::at(
                    ErrorKind::ComponentViolation(format!(
                        "sim `{}` does not support a taxis",
                        def.name
                    )),
                    self.loc(start),
                ));
            }
            (false, Optionality::Required) => {
                return Err(Error::at(
                    ErrorKind::ComponentViolation(format!("sim `{}` requires a taxis", def.name)),
                    self.loc(start),
                ));
            }
            _ => {}
        }

        // Lemma.
        let lemma_text = rest.trim();
        match (lemma_text.is_empty(), lemma_opt) {
            (false, Optionality::Unsupported) => {
                return Err(Error::at(
                    ErrorKind::ComponentViolation(format!(
                        "sim `{}` does not support a lemma",
                        def.name
                    )),
                    self.loc(start),
                ));
            }
            (true, Optionality::Required) => {
                return Err(Error::at(
                    ErrorKind::ComponentViolation(format!("sim `{}` requires a lemma", def.name)),
                    self.loc(start),
                ));
            }
            _ => {}
        }
        let lemma = self.scan_line(dial, lemma_text, start)?;

        self.idx += 1;
        // A simmere closes in the spelling it opened with; a
        // plerographic closer echoes the name as written.
        let closer = match &plero {
            Some(name) => format!("{{{name}}}{}", self.sig()),
            None => format!("{}{}", def.episymbol(), self.sig()),
        };

        // Line-structured grammata (`stichos` ostensive keyword):
        // strophes of inline-scanned lines until the episim line.
        if stichoi_grammata {
            let mut strophes: Vec<Strophe> = Vec::new();
            let mut current: Vec<Vec<Inline>> = Vec::new();
            loop {
                if self.idx >= self.lines.len() {
                    return Err(Error::at(
                        ErrorKind::UnmatchedSim(format!("{}{}", self.sig(), def.symbol)),
                        self.loc(start),
                    ));
                }
                let raw = self.lines[self.idx].clone();
                let trimmed = raw.trim();
                if trimmed.starts_with(closer.as_str()) {
                    break;
                }
                if trimmed.is_empty() {
                    if !current.is_empty() {
                        strophes.push(Strophe(std::mem::take(&mut current)));
                    }
                } else {
                    // Whitespace preservation as in the core form.
                    current.push(self.scan_line(dial, raw.trim_end(), self.idx)?);
                }
                self.idx += 1;
            }
            if !current.is_empty() {
                strophes.push(Strophe(current));
            }
            let (ann, hypograph) = self.parse_episim_line(dial, def, &closer, hypo_opt)?;
            return Ok(Block::Stichoi {
                symbol: Some(def.symbol.clone()),
                taxis,
                lemma,
                strophes,
                hypograph,
                bracket_matching: def.bracket_matching,
                ann,
            });
        }

        // Children until the episim line.
        let children = self.parse_blocks(dial, Some(&closer))?;

        let (ann, hypograph) = self.parse_episim_line(dial, def, &closer, hypo_opt)?;

        Ok(Block::Para {
            symbol: def.symbol.clone(),
            taxis,
            lemma,
            children,
            hypograph,
            bracket_matching: def.bracket_matching,
            ann,
        })
    }

    /// Consume the current line as `closer` + annotations. Trailing
    /// text is an error unless `allow_tail` (unused today).
    fn consume_closer(&mut self, closer: &str, allow_tail: bool) -> Result<Annotations> {
        let line = self.lines[self.idx].trim_start().to_string();
        let rest = line[closer.len()..].to_string();
        let (ann, tail) = self.scan_annotations_str(&rest, self.idx)?;
        if !tail.trim().is_empty() && !allow_tail {
            return Err(Error::at(
                ErrorKind::Syntax(format!(
                    "unexpected content after `{closer}`: `{}`",
                    tail.trim()
                )),
                self.loc(self.idx),
            ));
        }
        self.idx += 1;
        Ok(ann)
    }

    /// Collect a paragraph: consecutive text lines up to a blank
    /// line, a closing episim line, or a line beginning a block
    /// construct.
    fn collect_paragraph(&mut self, dial: &Dialektos, closer: Option<&str>) -> Result<Block> {
        let start = self.idx;
        let mut collected: Vec<String> = Vec::new();
        while self.idx < self.lines.len() {
            let line = self.lines[self.idx].trim_start().to_string();
            if line.is_empty() {
                break;
            }
            if let Some(cl) = closer
                && line.starts_with(cl)
            {
                break;
            }
            if !collected.is_empty() && self.is_block_start(dial, &line)? {
                break;
            }
            collected.push(line);
            self.idx += 1;
        }
        let text = collected.join("\n");
        let inlines = self.scan_line(dial, &text, start)?;
        Ok(Block::Paragraph(inlines))
    }

    /// Whether a line begins a para-level block construct (used for
    /// paragraph boundary detection; must not consume anything).
    fn is_block_start(&self, dial: &Dialektos, line: &str) -> Result<bool> {
        let sig = self.sig();
        if !line.starts_with(sig) {
            return Ok(false);
        }
        let run = line.chars().take_while(|&c| c == sig).count();
        let after: String = line.chars().skip(run).collect();
        Ok(match run {
            1 => {
                // Lookahead only: braced-name errors surface in the
                // real parse, never here.
                !after.starts_with('(')
                    && matches!(
                        resolve_sim_ref(dial, &after, self.loc(self.idx)),
                        Ok(Some(r)) if matches!(r.def.form, SimForm::Para { .. })
                    )
            }
            3 => {
                let tri = self.tri();
                match after.chars().next() {
                    Some('/') => {
                        // Only a standalone comment (close at line end
                        // or on a later line) breaks the paragraph.
                        let close = format!("/{tri}");
                        let rest = after[1..].trim();
                        !rest.contains(close.as_str()) || rest.ends_with(close.as_str())
                    }
                    Some('!' | '"' | '.' | ':' | '=' | '(' | '+') => true,
                    _ => false,
                }
            }
            4 => true,
            _ => false,
        })
    }

    /// Validate a raw parameter and decode it: an escaped space
    /// (inactive sigil, then the space) becomes a literal space;
    /// bare whitespace and unbalanced parentheses are errors.
    fn check_param(&self, raw: &str) -> Result<String> {
        let Some(param) = decode_param(raw, self.sigil.inactive()) else {
            return Err(Error::at(
                ErrorKind::MonosimWhitespace(raw.to_string()),
                self.loc(self.idx),
            ));
        };
        if !balanced_parens(&param) {
            return Err(Error::at(
                ErrorKind::Syntax("unbalanced parentheses in monosim parameter".into()),
                self.loc(self.idx),
            ));
        }
        Ok(param)
    }

    /// Scan a single-line (or paragraph) text into inlines.
    fn scan_line(&self, dial: &Dialektos, text: &str, line: usize) -> Result<Vec<Inline>> {
        let mut scanner = Scanner {
            chars: text.chars().collect(),
            pos: 0,
            sigil: self.sigil,
            file: self.file.clone(),
            base_line: line,
        };
        let inlines = scanner.scan(dial, None)?;
        if scanner.pos < scanner.chars.len() {
            return Err(Error::at(
                ErrorKind::Syntax("unexpected trailing content".into()),
                scanner.loc(),
            ));
        }
        Ok(inlines)
    }

    fn scan_annotations_str(&self, text: &str, line: usize) -> Result<(Annotations, String)> {
        let mut scanner = Scanner {
            chars: text.chars().collect(),
            pos: 0,
            sigil: self.sigil,
            file: self.file.clone(),
            base_line: line,
        };
        let ann = scanner.scan_annotations()?;
        let rest: String = scanner.chars[scanner.pos..].iter().collect();
        Ok((ann, rest))
    }
}

/// Decode a raw parameter: the inactive sigil before a space is
/// the escape for a literal space. None when it holds bare
/// whitespace.
pub fn decode_param(raw: &str, inactive: char) -> Option<String> {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == inactive && chars.peek() == Some(&' ') {
            chars.next();
            out.push(' ');
        } else if c.is_whitespace() {
            return None;
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// A parameter as written in the canonical-sigil spelling: a
/// literal space is escaped with the inactive sigil.
pub fn encode_param(param: &str) -> String {
    param.replace(' ', "\\ ")
}

/// Do the parentheses in a parameter nest and close? A parameter
/// ends at the first unmatched `)`, so one that is unbalanced can
/// never be written.
pub fn balanced_parens(param: &str) -> bool {
    let mut depth = 0usize;
    for c in param.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return false,
            ')' => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}

/// `(:r:)` parameter content? Return the onym of an axioma reference.
fn axioma_ref(param: &str) -> Option<String> {
    let inner = param.strip_prefix(':')?.strip_suffix(':')?;
    Some(inner.to_string())
}

// ---------------------------------------------------------------
// Inline scanner
// ---------------------------------------------------------------

struct Scanner {
    chars: Vec<char>,
    pos: usize,
    sigil: Sigil,
    file: PathBuf,
    base_line: usize,
}

impl Scanner {
    fn loc(&self) -> Location {
        let mut line = self.base_line + 1;
        let mut col = 1;
        for &c in &self.chars[..self.pos.min(self.chars.len())] {
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        Location {
            file: self.file.clone(),
            line,
            col,
        }
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(i, c)| self.chars.get(self.pos + i) == Some(&c))
    }

    /// Scan a monosim parameter up to its closing parenthesis.
    /// Parentheses inside the parameter nest: the parameter ends
    /// at the first unmatched `)`, so `@!=(οὕτω(ς))` and a URL
    /// ending in `_(city)` carry their parentheses verbatim.
    /// A space is written escaped, with the inactive sigil before
    /// it (`ye\ olde`), and decodes to a literal space; the flag
    /// reports bare whitespace, which a parameter cannot hold.
    fn take_param(&mut self) -> Option<(String, bool)> {
        let inact = self.sigil.inactive();
        let mut out = String::new();
        let mut depth = 0usize;
        let mut bare_whitespace = false;
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            if c == inact && self.chars.get(self.pos + 1) == Some(&' ') {
                out.push(' ');
                self.pos += 2;
                continue;
            }
            self.pos += 1;
            match c {
                '(' => depth += 1,
                ')' if depth == 0 => return Some((out, bare_whitespace)),
                ')' => depth -= 1,
                c if c.is_whitespace() => bare_whitespace = true,
                _ => {}
            }
            out.push(c);
        }
        None
    }

    fn take_until_str(&mut self, term: &str) -> Option<String> {
        let mut out = String::new();
        while self.pos < self.chars.len() {
            if self.starts_with(term) {
                self.pos += term.chars().count();
                return Some(out);
            }
            out.push(self.chars[self.pos]);
            self.pos += 1;
        }
        None
    }

    fn scan(&mut self, dial: &Dialektos, terminator: Option<&str>) -> Result<Vec<Inline>> {
        let sig = self.sigil.active();
        let inact = self.sigil.inactive();
        let mut inlines: Vec<Inline> = Vec::new();
        let mut text = String::new();
        macro_rules! flush {
            () => {
                if !text.is_empty() {
                    inlines.push(Inline::Text(std::mem::take(&mut text)));
                }
            };
        }
        while self.pos < self.chars.len() {
            if let Some(term) = terminator
                && self.starts_with(term)
            {
                flush!();
                return Ok(inlines);
            }
            let c = self.chars[self.pos];
            if c == inact {
                match self.peek(1) {
                    Some(n) if n == sig => {
                        text.push(sig);
                        self.pos += 2;
                    }
                    Some('|') => {
                        text.push('|');
                        self.pos += 2;
                    }
                    _ => {
                        text.push(inact);
                        self.pos += 1;
                    }
                }
                continue;
            }
            if c == '|' && self.peek(1) == Some(sig) {
                // Ambiguity separator before a sim.
                self.pos += 1;
                continue;
            }
            if c != sig {
                text.push(c);
                self.pos += 1;
                continue;
            }
            // Active sigil run.
            let run = self.chars[self.pos..]
                .iter()
                .take_while(|&&x| x == sig)
                .count();
            if run > 4 {
                return Err(Error::at(ErrorKind::SigilRunTooLong(run), self.loc()));
            }
            match run {
                1 => {
                    if self.peek(1) == Some('(') {
                        flush!();
                        self.pos += 2;
                        if self.peek(0) == Some('"') {
                            // Milestone: the quoted global anchor
                            // `@("scheme:value")` (spec v0.12).
                            self.pos += 1;
                            let Some(id) = self.take_until_str("\")") else {
                                return Err(Error::at(
                                    ErrorKind::Syntax("unterminated milestone".into()),
                                    self.loc(),
                                ));
                            };
                            let Some((scheme, value)) = id.split_once(':') else {
                                return Err(Error::at(
                                    ErrorKind::Syntax(format!(
                                        "milestone `{id}` has no scheme separator `:`"
                                    )),
                                    self.loc(),
                                ));
                            };
                            if !sigil::is_valid_genos(scheme) {
                                return Err(Error::at(
                                    ErrorKind::Syntax(format!(
                                        "invalid milestone scheme `{scheme}`"
                                    )),
                                    self.loc(),
                                ));
                            }
                            if !sigil::is_valid_milestone_value(value) {
                                return Err(Error::at(
                                    ErrorKind::Syntax(format!("invalid milestone value `{value}`")),
                                    self.loc(),
                                ));
                            }
                            let ann = self.scan_annotations()?;
                            if ann.onym.is_some() {
                                return Err(Error::at(
                                    ErrorKind::Syntax(
                                        "a milestone cannot carry an onym suffix".into(),
                                    ),
                                    self.loc(),
                                ));
                            }
                            inlines.push(Inline::Milestone {
                                scheme: scheme.to_string(),
                                value: value.to_string(),
                                ann,
                            });
                            continue;
                        }
                        let Some(onym) = self.take_until_str(")") else {
                            return Err(Error::at(
                                ErrorKind::Syntax("unterminated onym anchor".into()),
                                self.loc(),
                            ));
                        };
                        if !sigil::is_valid_onym(&onym) {
                            return Err(Error::at(ErrorKind::InvalidOnym(onym), self.loc()));
                        }
                        inlines.push(Inline::OnymAnchor(onym));
                        continue;
                    }
                    let rest: String = self.chars[self.pos + 1..].iter().collect();
                    let Some(r) = resolve_sim_ref(dial, &rest, self.loc())? else {
                        let sym: String = rest
                            .chars()
                            .take_while(|&c| sigil::is_symbolic(c))
                            .collect();
                        return Err(Error::at(
                            ErrorKind::UndefinedSim(format!("{sig}{sym}")),
                            self.loc(),
                        ));
                    };
                    let def = r.def.clone();
                    let plero = r.plero.clone();
                    flush!();
                    self.pos += 1 + r.chars;
                    match def.form {
                        SimForm::Endo => {
                            // A simmere closes in the spelling it
                            // opened with; a plerographic closer
                            // echoes the name as written.
                            let term = match &plero {
                                Some(name) => format!("{{{name}}}{sig}"),
                                None => format!("{}{}", def.episymbol(), sig),
                            };
                            let content = self.scan(dial, Some(&term))?;
                            if !self.starts_with(&term) {
                                return Err(Error::at(
                                    ErrorKind::UnmatchedSim(format!("{sig}{}", def.symbol)),
                                    self.loc(),
                                ));
                            }
                            self.pos += term.chars().count();
                            let ann = self.scan_annotations()?;
                            inlines.push(Inline::Endo {
                                symbol: def.symbol.clone(),
                                content,
                                bracket_matching: def.bracket_matching,
                                ann,
                            });
                        }
                        SimForm::Mono { .. } => {
                            if self.peek(0) != Some('(') {
                                return Err(Error::at(
                                    ErrorKind::Syntax(format!(
                                        "monosim `{sig}{}` requires `(param)`",
                                        def.symbol
                                    )),
                                    self.loc(),
                                ));
                            }
                            self.pos += 1;
                            let Some((param, bare_whitespace)) = self.take_param() else {
                                return Err(Error::at(
                                    ErrorKind::Syntax("unterminated monosim parameter".into()),
                                    self.loc(),
                                ));
                            };
                            if bare_whitespace {
                                return Err(Error::at(
                                    ErrorKind::MonosimWhitespace(param),
                                    self.loc(),
                                ));
                            }
                            let ann = self.scan_annotations()?;
                            inlines.push(Inline::Monosim {
                                symbol: def.symbol.clone(),
                                param,
                                ann,
                            });
                        }
                        SimForm::Para { .. } => {
                            // A para-simmere symbol immediately
                            // followed by `(` is a deixis: a core
                            // pointing reference to the onymized
                            // simmere of this sim.
                            if self.peek(0) != Some('(') {
                                return Err(Error::at(ErrorKind::EndoMimicsPara, self.loc()));
                            }
                            self.pos += 1;
                            let Some(onym) = self.take_until_str(")") else {
                                return Err(Error::at(
                                    ErrorKind::Syntax("unterminated deixis reference".into()),
                                    self.loc(),
                                ));
                            };
                            if !sigil::is_valid_onym(&onym) {
                                return Err(Error::at(ErrorKind::InvalidOnym(onym), self.loc()));
                            }
                            let ann = self.scan_annotations()?;
                            inlines.push(Inline::Deixis {
                                symbol: def.symbol.clone(),
                                onym,
                                ann,
                            });
                        }
                    }
                }
                2 => {
                    let two: String = std::iter::repeat_n(sig, 2).collect();
                    match self.peek(2) {
                        Some('/') => {
                            // Line comment: to end of line, then eat
                            // following whitespace.
                            while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                                self.pos += 1;
                            }
                            while self.pos < self.chars.len()
                                && self.chars[self.pos].is_whitespace()
                            {
                                self.pos += 1;
                            }
                        }
                        Some('"') => {
                            flush!();
                            self.pos += 3;
                            let term = format!("\"{two}");
                            let Some(content) = self.take_until_str(&term) else {
                                return Err(Error::at(
                                    ErrorKind::UnmatchedSim(format!("{two}\"")),
                                    self.loc(),
                                ));
                            };
                            let ann = self.scan_annotations()?;
                            inlines.push(Inline::VerbatimInline { content, ann });
                        }
                        Some('.') => {
                            flush!();
                            self.pos += 3;
                            let term = format!(".{two}");
                            let content = self.scan(dial, Some(&term))?;
                            if !self.starts_with(&term) {
                                return Err(Error::at(
                                    ErrorKind::UnmatchedSim(format!("{two}.")),
                                    self.loc(),
                                ));
                            }
                            self.pos += term.chars().count();
                            let ann = self.scan_annotations()?;
                            inlines.push(Inline::EndoDiaphane { content, ann });
                        }
                        Some(':') => {
                            flush!();
                            self.pos += 3;
                            let term = format!(":{two}");
                            let content = self.scan(dial, Some(&term))?;
                            if !self.starts_with(&term) {
                                return Err(Error::at(
                                    ErrorKind::UnmatchedSim(format!("{two}:")),
                                    self.loc(),
                                ));
                            }
                            self.pos += term.chars().count();
                            if self.peek(0) != Some('(') {
                                return Err(Error::at(
                                    ErrorKind::Syntax(
                                        "endo-axioma requires `(onym)` after its episim".into(),
                                    ),
                                    self.loc(),
                                ));
                            }
                            self.pos += 1;
                            let Some(onym) = self.take_until_str(")") else {
                                return Err(Error::at(
                                    ErrorKind::Syntax("unterminated axioma onym".into()),
                                    self.loc(),
                                ));
                            };
                            if !sigil::is_valid_onym(&onym) {
                                return Err(Error::at(ErrorKind::InvalidOnym(onym), self.loc()));
                            }
                            inlines.push(Inline::EndoAxioma { onym, content });
                        }
                        Some('(') => {
                            flush!();
                            self.pos += 3;
                            let Some(param) = self.take_until_str(")") else {
                                return Err(Error::at(
                                    ErrorKind::Syntax("unterminated axioma reference".into()),
                                    self.loc(),
                                ));
                            };
                            let Some(onym) = axioma_ref(&param) else {
                                return Err(Error::at(
                                    ErrorKind::Syntax(format!(
                                        "expected `(:ref:)` axioma reference, found `({param})`"
                                    )),
                                    self.loc(),
                                ));
                            };
                            inlines.push(Inline::AxiomaRef {
                                onym,
                                enlexis: false,
                            });
                        }
                        Some('+') => {
                            flush!();
                            self.pos += 3;
                            if self.peek(0) != Some('(') {
                                return Err(Error::at(
                                    ErrorKind::Syntax("malformed axioma-enlexis reference".into()),
                                    self.loc(),
                                ));
                            }
                            self.pos += 1;
                            let Some(param) = self.take_until_str(")") else {
                                return Err(Error::at(
                                    ErrorKind::Syntax("unterminated axioma reference".into()),
                                    self.loc(),
                                ));
                            };
                            let Some(onym) = axioma_ref(&param) else {
                                return Err(Error::at(
                                    ErrorKind::Syntax(format!(
                                        "expected `(:ref:)` axioma reference, found `({param})`"
                                    )),
                                    self.loc(),
                                ));
                            };
                            inlines.push(Inline::AxiomaRef {
                                onym,
                                enlexis: true,
                            });
                        }
                        other => {
                            return Err(Error::at(
                                ErrorKind::UndefinedSim(format!(
                                    "{two}{}",
                                    other.map(String::from).unwrap_or_default()
                                )),
                                self.loc(),
                            ));
                        }
                    }
                }
                3 => {
                    let tri: String = std::iter::repeat_n(sig, 3).collect();
                    match self.peek(3) {
                        Some('/') => {
                            // Inline block comment.
                            self.pos += 4;
                            let term = format!("/{tri}");
                            if self.take_until_str(&term).is_none() {
                                return Err(Error::at(
                                    ErrorKind::UnmatchedSim(format!("{tri}/")),
                                    self.loc(),
                                ));
                            }
                        }
                        Some('(' | '+') => {
                            return Err(Error::at(ErrorKind::AnaphorInEndoContext, self.loc()));
                        }
                        _ => {
                            return Err(Error::at(
                                ErrorKind::Syntax("trigraph block form in endo-context".into()),
                                self.loc(),
                            ));
                        }
                    }
                }
                4 => {
                    return Err(Error::at(
                        ErrorKind::Syntax("enmedia is only valid in para-context".into()),
                        self.loc(),
                    ));
                }
                _ => unreachable!(),
            }
        }
        flush!();
        if terminator.is_some() {
            // Caller checks starts_with; reaching the end without the
            // terminator is reported there with better context.
        }
        Ok(inlines)
    }

    /// Parse episim suffixes: optional `(onym)`, then chained
    /// `.genos` annotations, then an optional `|` separator.
    fn scan_annotations(&mut self) -> Result<Annotations> {
        let mut ann = Annotations::default();
        if self.peek(0) == Some('(') {
            self.pos += 1;
            let Some(onym) = self.take_until_str(")") else {
                return Err(Error::at(
                    ErrorKind::Syntax("unterminated onym suffix".into()),
                    self.loc(),
                ));
            };
            if !sigil::is_valid_onym(&onym) {
                return Err(Error::at(ErrorKind::InvalidOnym(onym), self.loc()));
            }
            ann.onym = Some(onym);
        }
        while self.peek(0) == Some('.') && self.peek(1).is_some_and(sigil::is_genos_start) {
            self.pos += 1;
            let mut genos = String::new();
            while let Some(c) = self.peek(0) {
                if sigil::is_genos_continue(c) {
                    genos.push(c);
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if !sigil::is_valid_genos(&genos) {
                return Err(Error::at(ErrorKind::InvalidGenos(genos), self.loc()));
            }
            ann.genoses.push(genos);
        }
        if self.peek(0) == Some('|') {
            self.pos += 1; // boundary separator
        }
        Ok(ann)
    }
}
