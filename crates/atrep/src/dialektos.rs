//! Dialektos definitions (spec: chapter "Defining Dialektoi").
//!
//! Parses `.lektos` / `.dia` files written in the built-in `atrep`
//! meta-dialektos: ostensive sim definitions inside `@=== ... ===@`
//! blocks, plus local-only inheritance and sim import (`@@::`).
//!
//! Pilot notes:
//! - Resolution is local-only per the v0.10 spec: `<id>.lektos`
//!   (falling back to `<id>.dia`) in the resolution directory.
//! - A version suffix on declarations/inheritance is accepted but
//!   does not participate in resolution (local files carry no
//!   version dimension).
//! - Bracket-matching disablement has no explicit `.lektos` syntax
//!   in the spec; it is inferred from the ostensive definition: if
//!   the shown episymbol is the unflipped reversal, matching is
//!   disabled.

use std::collections::BTreeMap;
use std::path::Path;

use unicode_normalization::UnicodeNormalization;

use crate::error::{Error, ErrorKind, Location, Result};
use crate::sigil::{self, Sigil};
use crate::source::{DirSource, Source, fetch_normalized};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Optionality {
    Required,
    Optional,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimForm {
    Endo,
    Para {
        taxis: Optionality,
        lemma: Optionality,
        hypograph: Optionality,
        /// Grammata are line-structured (declared with the
        /// `stichos` ostensive keyword instead of `grammata`).
        stichoi: bool,
        /// The lemma auto-registers as the simmere's onym
        /// (declared with the `autonym` property keyword;
        /// kanonizo computes and pins it).
        autonym: bool,
        /// Grammata are rows (declared with the `rows` ostensive
        /// keyword): the children are the row sim declared under
        /// this one, and a plain paragraph of pipe-separated lines
        /// is the shorthand kanonizo expands into rows.
        rows: bool,
        /// Grammata are cells (declared with the `cells` keyword):
        /// the children are the cell sim declared under this one.
        cells: bool,
        /// A `cells`-form sim declared with the `header` property
        /// keyword: the row that names the table's columns.
        header: bool,
    },
    Mono {
        /// The ostensive parameter keyword (`param`, `onym`,
        /// `depth`, ...) - semantic display only.
        keyword: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimDef {
    pub name: String,
    pub symbol: String,
    pub form: SimForm,
    pub bracket_matching: bool,
    /// Parent sim (by symbol) for nested definitions, via `@^(...)`.
    pub parent: Option<String>,
    /// The vocabulary the lemma draws from (`lemma=<vocab>`).
    pub lemma_vocabulary: Option<String>,
    /// The vocabulary the genoses draw from (`@% <vocab>`).
    pub genos_vocabulary: Option<String>,
    pub short_desc: Option<String>,
    pub long_desc: Option<String>,
}

impl SimDef {
    pub fn episymbol(&self) -> String {
        sigil::episymbol(&self.symbol, self.bracket_matching)
    }
}

/// One `@@::` inheritance/import declaration, in source order.
/// The exomorphosis resolver walks these to pull parent rules
/// (spec: Rule Resolution and Inheritance).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InheritOp {
    pub source: String,
    pub kind: InheritKind,
}

/// Result of a plerographic name lookup ([`Dialektos::sim_named`]).
pub enum NamedLookup<'a> {
    None,
    One(&'a SimDef),
    /// The name is shared by several sims — legal, but not
    /// plerographically addressable.
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InheritKind {
    /// `@@::parent` - full inheritance.
    Full,
    /// `@@::parent:-:[s1 s2]` - full inheritance with exclusions.
    Exclude(Vec<String>),
    /// `@@::parent::sym [alias]` - single (possibly aliased) import.
    Import {
        symbol: String,
        alias: Option<String>,
    },
    /// `@@::parent::[s1 s2]` - list import.
    ImportList(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialektos {
    pub id: String,
    /// Sim definitions keyed by symbol (sorted, per the canonical
    /// ASCII-by-symbol ordering).
    pub sims: BTreeMap<String, SimDef>,
    /// The `@@::` declarations, in order. Empty for a canonical
    /// `.lektos` (inheritance is expanded away there).
    pub lineage: Vec<InheritOp>,
    /// Controlled vocabularies (`@==%` blocks), by name.
    pub vocabularies: BTreeMap<String, Vocabulary>,
    /// Glossae: localized sim names by language tag, loaded from
    /// sibling `<id>.<lang>.glossa` files at resolution (spec:
    /// "Glossae"). Symbol -> localized name.
    pub glossae: BTreeMap<String, BTreeMap<String, String>>,
}

/// A controlled vocabulary: canonical terms with multilingual
/// aliases (spec: "Controlled Vocabularies").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vocabulary {
    /// Canonical term -> sorted alias set (aliases lowercased).
    pub terms: BTreeMap<String, std::collections::BTreeSet<String>>,
}

impl Vocabulary {
    /// Canonicalize a term: canonical terms and unknown terms
    /// pass through; aliases (matched case-insensitively) map
    /// to their canonical term.
    pub fn canonicalize<'a>(&'a self, term: &'a str) -> &'a str {
        if self.terms.contains_key(term) {
            return term;
        }
        let lower = term.to_lowercase();
        for (canonical, aliases) in &self.terms {
            if aliases.contains(&lower) {
                return canonical;
            }
        }
        term
    }
}

impl Dialektos {
    /// Look a sim up by its definition name (the plerographic
    /// reference, spec: "Metagraphe"). Duplicate names are legal
    /// in a dialektos but not plerographically addressable.
    pub fn sim_named(&self, name: &str) -> NamedLookup<'_> {
        // The plerographic address space is the union of the
        // primary names and every glossa's localized names; a
        // name matching sims of two different symbols is
        // ambiguous (spec: "Glossae").
        let mut symbol: Option<String> = None;
        let record = |sym: &str, symbol: &mut Option<String>| -> bool {
            match symbol {
                Some(s) => s != sym,
                None => {
                    *symbol = Some(sym.to_string());
                    false
                }
            }
        };
        for def in self.sims.values() {
            if def.name == name && record(&def.symbol, &mut symbol) {
                return NamedLookup::Ambiguous;
            }
        }
        for names in self.glossae.values() {
            for (sym, lname) in names {
                if lname == name && record(sym, &mut symbol) {
                    return NamedLookup::Ambiguous;
                }
            }
        }
        match symbol {
            Some(sym) => NamedLookup::One(&self.sims[&sym]),
            None => NamedLookup::None,
        }
    }

    /// Longest defined symbol that is a prefix of `text`.
    pub fn longest_match(&self, text: &str) -> Option<&SimDef> {
        let run: String = text
            .chars()
            .take_while(|&c| sigil::is_symbolic(c))
            .collect();
        let mut candidate = run.as_str();
        while !candidate.is_empty() {
            if let Some(def) = self.sims.get(candidate) {
                return Some(def);
            }
            let mut chars = candidate.chars();
            chars.next_back();
            candidate = chars.as_str();
        }
        None
    }
}

/// Serialize a dialektos in canonical `.lektos` form: canonical
/// sigil, `@@@!atrep` declaration, sim definition blocks sorted by
/// sim name (byte-wise UTF-8; symbol as tie-break, since duplicate
/// names are legal), inheritance and imports fully expanded (they
/// already are in the parsed representation). Serializing an
/// already-canonical `.lektos` is byte-identical (idempotent).
pub fn serialize(d: &Dialektos) -> String {
    let mut defs: Vec<&SimDef> = d.sims.values().collect();
    defs.sort_by_key(|def| (def.name.as_bytes(), def.symbol.as_bytes()));
    let mut out = String::from("@@@!atrep\n");
    for def in defs {
        out.push('\n');
        serialize_sim(def, &mut out);
    }
    // Vocabularies, sorted by name; terms sorted; aliases sorted
    // (the BTree ordering is the canonical ordering).
    for (name, vocab) in &d.vocabularies {
        out.push('\n');
        out.push_str("@==% ");
        out.push_str(name);
        out.push('\n');
        for (canonical, aliases) in &vocab.terms {
            out.push_str(canonical);
            out.push(':');
            for alias in aliases {
                out.push(' ');
                out.push_str(alias);
            }
            out.push('\n');
        }
        out.push_str("%==@\n");
    }
    out
}

fn serialize_sim(def: &SimDef, out: &mut String) {
    out.push_str("@=== ");
    out.push_str(&def.name);
    out.push('\n');
    match &def.form {
        SimForm::Mono { keyword } => {
            out.push('@');
            out.push_str(&def.symbol);
            out.push('(');
            out.push_str(keyword);
            out.push_str(")\n");
        }
        SimForm::Endo => {
            // The unflipped episymbol round-trips bracket-matching
            // disablement (inferred from the ostensive definition).
            out.push('@');
            out.push_str(&def.symbol);
            out.push_str(" grammata ");
            out.push_str(&def.episymbol());
            out.push_str("@\n");
        }
        SimForm::Para {
            taxis,
            lemma,
            hypograph,
            stichoi,
            autonym,
            rows,
            cells,
            header,
        } => {
            out.push('@');
            out.push_str(&def.symbol);
            match taxis {
                Optionality::Required => out.push_str("(taxis)"),
                Optionality::Optional => out.push_str("[(taxis)]"),
                Optionality::Unsupported => {}
            }
            match (lemma, &def.lemma_vocabulary) {
                (Optionality::Required, None) => out.push_str(" lemma"),
                (Optionality::Optional, None) => out.push_str(" [lemma]"),
                (Optionality::Required, Some(v)) => {
                    out.push_str(" lemma=");
                    out.push_str(v);
                }
                (Optionality::Optional, Some(v)) => {
                    out.push_str(" [lemma=");
                    out.push_str(v);
                    out.push(']');
                }
                (Optionality::Unsupported, _) => {}
            }
            if *autonym {
                out.push_str("\nautonym");
            }
            if *header {
                out.push_str("\nheader");
            }
            out.push_str(if *stichoi {
                "\nstichos\n"
            } else if *rows {
                "\nrows\n"
            } else if *cells {
                "\ncells\n"
            } else {
                "\ngrammata\n"
            });
            out.push_str(&def.episymbol());
            out.push('@');
            match hypograph {
                Optionality::Required => out.push_str(" hypograph"),
                Optionality::Optional => out.push_str(" [hypograph]"),
                Optionality::Unsupported => {}
            }
            out.push('\n');
        }
    }
    if let Some(vocab) = &def.genos_vocabulary {
        out.push_str("@% ");
        out.push_str(vocab);
        out.push('\n');
    }
    // Property child sims in ASCII order of their symbols:
    // `"` (short), `""` (long), `^` (parent).
    if let Some(short) = &def.short_desc {
        out.push_str("@\"");
        out.push_str(short);
        out.push_str("\"@\n");
    }
    if let Some(long) = &def.long_desc {
        // Always block-form; the body is preserved verbatim.
        out.push_str("@\"\"\n");
        out.push_str(long);
        out.push_str("\n\"\"@\n");
    }
    if let Some(parent) = &def.parent {
        out.push_str("@^(");
        out.push_str(parent);
        out.push_str(")\n");
    }
    out.push_str("===@\n");
}

/// Parse a definition file (`.lektos` / `.dia`) directly by path.
/// The dialektos identifier is the file stem; inheritance resolves
/// in the file's directory (local-only).
pub fn parse_file(path: &Path) -> Result<Dialektos> {
    let bytes = std::fs::read(path).map_err(|e| {
        Error::new(ErrorKind::MissingResource(format!(
            "{}: {e}",
            path.display()
        )))
    })?;
    let source = String::from_utf8(bytes).map_err(|_| Error::new(ErrorKind::InvalidUtf8))?;
    parse_source(&source, path)
}

/// Parse a dialektos definition from in-memory source. `path`
/// supplies the definition's identifier (file stem) and the
/// directory that imports resolve against; the file itself is not
/// read.
pub fn parse_source(source: &str, path: &Path) -> Result<Dialektos> {
    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let normalized: String = source.nfc().collect();
    let mut stack = vec![id.clone()];
    parse_definition(&normalized, path, &id, &DirSource::new(dir), &mut stack)
}

/// Resolve a dialektos identifier to a definition in `dir`
/// (local-only, per the v0.10 spec).
pub fn resolve(dir: &Path, id: &str) -> Result<Dialektos> {
    resolve_from(&DirSource::new(dir), id)
}

/// Resolve a dialektos identifier in an arbitrary resolution
/// [`Source`].
pub fn resolve_from(source: &dyn Source, id: &str) -> Result<Dialektos> {
    let mut stack = Vec::new();
    resolve_inner(source, id, &mut stack)
}

/// Embedded standard-library dialektoi, used when no local
/// definition file resolves (pilot decision: the spec's resolution
/// is local-only and does not yet define a standard library).
const STD_DIALEKTOI: &[(&str, &str)] = &[
    ("at-djot", include_str!("../std/at-djot.dia")),
    ("koine", include_str!("../std/koine.dia")),
    ("at-prosa", include_str!("../std/at-prosa.dia")),
    ("at-poesia", include_str!("../std/at-poesia.dia")),
    ("at-drama", include_str!("../std/at-drama.dia")),
    ("at-aphanes", include_str!("../std/at-aphanes.dia")),
    ("at-epimerismos", include_str!("../std/at-epimerismos.dia")),
    ("at-docbook", include_str!("../std/at-docbook.dia")),
    ("at-html", include_str!("../std/at-html.dia")),
    ("at-markdown", include_str!("../std/at-markdown.dia")),
    ("at-org", include_str!("../std/at-org.dia")),
    ("at-rst", include_str!("../std/at-rst.dia")),
    ("at-tei", include_str!("../std/at-tei.dia")),
    ("at-usfm", include_str!("../std/at-usfm.dia")),
    ("litogramma", include_str!("../std/litogramma.dia")),
    ("bibliogramma", include_str!("../std/bibliogramma.dia")),
    ("lexigramma", include_str!("../std/lexigramma.dia")),
];

/// Embedded standard-library glossae: (dialektos, language, source).
/// A pack's glossae travel with it through inheritance, so a
/// litogramma document may spell `@{person}` or `@{лицо}`.
const STD_GLOSSAE: &[(&str, &str, &str)] = &[
    (
        "at-aphanes",
        "en",
        include_str!("../std/at-aphanes.en.glossa"),
    ),
    (
        "at-aphanes",
        "ru",
        include_str!("../std/at-aphanes.ru.glossa"),
    ),
    (
        "at-epimerismos",
        "en",
        include_str!("../std/at-epimerismos.en.glossa"),
    ),
    (
        "at-epimerismos",
        "ru",
        include_str!("../std/at-epimerismos.ru.glossa"),
    ),
];

/// The identifiers of the embedded standard-library dialektoi.
pub(crate) fn std_dialektos_ids() -> &'static [(&'static str, &'static str)] {
    STD_DIALEKTOI
}

/// The identifiers of the embedded standard-library dialektoi
/// (public surface: registries treat these as designated hubs).
pub fn std_ids() -> Vec<&'static str> {
    STD_DIALEKTOI.iter().map(|(id, _)| *id).collect()
}

fn resolve_inner(source: &dyn Source, id: &str, stack: &mut Vec<String>) -> Result<Dialektos> {
    if stack.iter().any(|s| s == id) {
        return Err(Error::new(ErrorKind::TransclusionCycle(format!(
            "dialektos inheritance cycle through `{id}`"
        ))));
    }
    let (name, text) = match fetch_normalized(source, &format!("{id}.lektos"))? {
        Some(text) => (format!("{id}.lektos"), text),
        None => match fetch_normalized(source, &format!("{id}.dia"))? {
            Some(text) => (format!("{id}.dia"), text),
            None => {
                if let Some((_, src)) = STD_DIALEKTOI.iter().find(|(name, _)| *name == id) {
                    let pseudo = Path::new("std:").join(format!("{id}.dia"));
                    stack.push(id.to_string());
                    let result = parse_definition(src, &pseudo, id, source, stack);
                    stack.pop();
                    let mut dial = result?;
                    for (d, lang, text) in STD_GLOSSAE {
                        if *d == id {
                            let names = crate::glossa::parse_glossa_source(text, &dial, lang)?;
                            dial.glossae.insert((*lang).to_string(), names);
                        }
                    }
                    // Sibling glossae in the resolution context
                    // add to (or replace, per language) the
                    // embedded ones.
                    load_glossae(source, &mut dial)?;
                    return Ok(dial);
                }
                return Err(Error::new(ErrorKind::UnresolvableDialektos(id.to_string())));
            }
        },
    };
    stack.push(id.to_string());
    let result = parse_definition(&text, Path::new(&name), id, source, stack);
    stack.pop();
    let mut dial = result?;
    load_glossae(source, &mut dial)?;
    Ok(dial)
}

/// Load the dialektos's glossae from sibling
/// `<id>.<lang>.glossa` files in the resolution context.
fn load_glossae(source: &dyn Source, dial: &mut Dialektos) -> Result<()> {
    let prefix = format!("{}.", dial.id);
    for name in source.names() {
        if let Some(rest) = name.strip_prefix(&prefix)
            && let Some(lang) = rest.strip_suffix(".glossa")
            && !lang.is_empty()
            && !lang.contains('.')
            && let Some(text) = fetch_normalized(source, &name)?
        {
            let names = crate::glossa::parse_glossa_source(&text, dial, lang)?;
            dial.glossae.insert(lang.to_string(), names);
        }
    }
    Ok(())
}

/// Strip a `@<major>[.<minor>]` version suffix from an identifier.
fn strip_version(spec: &str) -> &str {
    match spec.find('@') {
        Some(i) => &spec[..i],
        None => spec,
    }
}

fn parse_definition(
    source: &str,
    path: &Path,
    id: &str,
    ctx: &dyn Source,
    stack: &mut Vec<String>,
) -> Result<Dialektos> {
    let loc = |line: usize| Location {
        file: path.to_path_buf(),
        line,
        col: 1,
    };
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    // Optional shebang.
    if i < lines.len() && lines[i].starts_with("#!") {
        i += 1;
    }
    // Declaration: must declare the `atrep` meta-dialektos.
    let sigil = match lines.get(i).map(|l| l.trim()) {
        Some(l) if l.starts_with("@@@!") => Sigil::Canonical,
        Some(l) if l.starts_with("\\\\\\!") => Sigil::Alias,
        _ => return Err(Error::at(ErrorKind::MissingDeclaration, loc(i + 1))),
    };
    let decl = lines[i].trim();
    let declared = strip_version(&decl[4..]);
    if declared != "atrep" {
        return Err(Error::at(
            ErrorKind::InvalidLektos(format!(
                "definition files must declare the `atrep` meta-dialektos, found `{declared}`"
            )),
            loc(i + 1),
        ));
    }
    let sig = sigil.active();
    i += 1;

    let mut dialektos = Dialektos {
        id: id.to_string(),
        sims: BTreeMap::new(),
        lineage: Vec::new(),
        vocabularies: BTreeMap::new(),
        glossae: BTreeMap::new(),
    };

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
            continue;
        }
        // Block comment `@@@/ ... /@@@` (single- or multi-line).
        let block_open = format!("{sig}{sig}{sig}/");
        let block_close = format!("/{sig}{sig}{sig}");
        if let Some(rest) = line.strip_prefix(&block_open) {
            if !rest.trim_end().ends_with(&block_close) {
                let mut end = i + 1;
                while end < lines.len() && !lines[end].trim_end().ends_with(&block_close) {
                    end += 1;
                }
                if end == lines.len() {
                    return Err(Error::at(
                        ErrorKind::UnmatchedSim(block_open.clone()),
                        loc(i + 1),
                    ));
                }
                i = end;
            }
            i += 1;
            continue;
        }
        let comment = format!("{sig}{sig}/");
        if line.starts_with(&comment) {
            i += 1;
            continue;
        }
        let inherit = format!("{sig}{sig}::");
        if let Some(rest) = line.strip_prefix(&inherit) {
            apply_inheritance(&mut dialektos, rest, ctx, stack, loc(i + 1))?;
            i += 1;
            continue;
        }
        let vocab_open = format!("{sig}==% ");
        if let Some(name) = line.strip_prefix(&vocab_open) {
            let name = name.trim().to_string();
            let close = format!("%=={sig}");
            let mut vocab = dialektos.vocabularies.remove(&name).unwrap_or_default();
            i += 1;
            loop {
                if i >= lines.len() {
                    return Err(Error::at(
                        ErrorKind::InvalidLektos(format!("unterminated vocabulary `{name}`")),
                        loc(i),
                    ));
                }
                let vline = lines[i].trim();
                i += 1;
                if vline == close {
                    break;
                }
                if vline.is_empty() {
                    continue;
                }
                let Some((canonical, aliases)) = vline.split_once(':') else {
                    return Err(Error::at(
                        ErrorKind::InvalidLektos(format!(
                            "vocabulary `{name}`: malformed line `{vline}`"
                        )),
                        loc(i),
                    ));
                };
                let canonical = canonical.trim().to_string();
                if canonical.is_empty() {
                    return Err(Error::at(
                        ErrorKind::InvalidLektos(format!(
                            "vocabulary `{name}`: line `{vline}` names no canonical term"
                        )),
                        loc(i),
                    ));
                }
                let entry = vocab.terms.entry(canonical.clone()).or_default();
                for alias in aliases.split_whitespace() {
                    entry.insert(alias.to_lowercase());
                }
            }
            // Injectivity: an alias may serve one canonical term.
            let mut seen: BTreeMap<&String, &String> = BTreeMap::new();
            for (canonical, aliases) in &vocab.terms {
                for alias in aliases {
                    if let Some(other) = seen.insert(alias, canonical)
                        && other != canonical
                    {
                        return Err(Error::at(
                            ErrorKind::InvalidLektos(format!(
                                "vocabulary `{name}`: alias `{alias}` is claimed by \
                                 both `{other}` and `{canonical}`"
                            )),
                            loc(i),
                        ));
                    }
                }
            }
            dialektos.vocabularies.insert(name, vocab);
            continue;
        }
        let simdef_open = format!("{sig}=== ");
        if let Some(name) = line.strip_prefix(&simdef_open) {
            let name = name.trim().to_string();
            // Names are addressable syntax (spec: "Metagraphe"):
            // the dialektos-identifier grammar applies.
            if !sigil::is_valid_name(&name) {
                return Err(Error::at(
                    ErrorKind::InvalidLektos(format!(
                        "sim name `{name}`: letters, digits, and hyphens only, \
                         beginning and ending alphanumeric"
                    )),
                    loc(i + 1),
                ));
            }
            let start = i + 1;
            // The block close is found structurally by
            // parse_sim_block: a chapter-style sim whose symbol is
            // `===` has an ostensive episim line identical to the
            // block close, so a line scan would truncate early.
            let (def, consumed) = parse_sim_block(&name, &lines[start..], sigil, path, start + 1)?;
            if dialektos.sims.contains_key(&def.symbol) {
                return Err(Error::at(ErrorKind::SimConflict(def.symbol), loc(i + 1)));
            }
            dialektos.sims.insert(def.symbol.clone(), def);
            i = start + consumed;
            continue;
        }
        return Err(Error::at(
            ErrorKind::InvalidLektos(format!("unexpected line: `{line}`")),
            loc(i + 1),
        ));
    }
    // A `lemma=<vocab>` / `@% <vocab>` binding names a vocabulary
    // of this dialektos (its own or an inherited one); an
    // undefined one would make canonicalization a silent no-op.
    for def in dialektos.sims.values() {
        for (what, vocab) in [
            ("lemma", &def.lemma_vocabulary),
            ("genos", &def.genos_vocabulary),
        ] {
            if let Some(vocab) = vocab
                && !dialektos.vocabularies.contains_key(vocab)
            {
                return Err(Error::at(
                    ErrorKind::InvalidLektos(format!(
                        "sim `{}`: {what} vocabulary `{vocab}` is not defined",
                        def.name
                    )),
                    loc(lines.len()),
                ));
            }
        }
    }
    Ok(dialektos)
}

/// Apply a `@@::` inheritance/import line. `rest` is the text after
/// the `@@::` prefix.
fn apply_inheritance(
    dialektos: &mut Dialektos,
    rest: &str,
    ctx: &dyn Source,
    stack: &mut Vec<String>,
    loc: Location,
) -> Result<()> {
    // Forms:
    //   name[@ver]                  full inheritance
    //   name:-:[s1 s2 ...]          inheritance with exclusion
    //   name::sym                   single import
    //   name::sym alias             aliased import
    //   name::[s1 s2 ...]           list import
    let (source_spec, op) = if let Some(idx) = rest.find(":-:") {
        (&rest[..idx], Some(("exclude", &rest[idx + 3..])))
    } else if let Some(idx) = rest.find("::") {
        (&rest[..idx], Some(("import", &rest[idx + 2..])))
    } else {
        (rest, None)
    };
    let source_id = strip_version(source_spec.trim());
    let parent = resolve_inner(ctx, source_id, stack).map_err(|e| Error {
        location: Some(loc.clone()),
        ..e
    })?;

    // The parent's glossae follow the sims brought in: (parent
    // symbol, symbol here), so localized names keep working in
    // the inheriting dialektos's plerographic spelling.
    let parent_glossae = parent.glossae.clone();
    // The parent's vocabularies follow too: a sim's
    // `lemma=<vocab>` / `@% <vocab>` binding must resolve in the
    // inheriting dialektos, or canonicalization silently stops.
    // Full inheritance brings every vocabulary (a local sim may
    // bind one of the parent's); an import brings the bound ones.
    let parent_vocabularies = parent.vocabularies.clone();
    let mut brought: Vec<(String, String)> = Vec::new();

    let mut merge = |def: SimDef| -> Result<()> {
        if dialektos.sims.contains_key(&def.symbol) {
            return Err(Error::at(ErrorKind::SimConflict(def.symbol), loc.clone()));
        }
        dialektos.sims.insert(def.symbol.clone(), def);
        Ok(())
    };

    let recorded = match op {
        None => {
            for def in parent.sims.into_values() {
                brought.push((def.symbol.clone(), def.symbol.clone()));
                merge(def)?;
            }
            InheritKind::Full
        }
        Some(("exclude", list)) => {
            let excluded = parse_symbol_list(list, &loc)?;
            for def in parent.sims.into_values() {
                if !excluded.contains(&def.symbol) {
                    brought.push((def.symbol.clone(), def.symbol.clone()));
                    merge(def)?;
                }
            }
            InheritKind::Exclude(excluded)
        }
        Some(("import", spec)) => {
            let spec = spec.trim();
            if spec.starts_with('[') {
                let symbols = parse_symbol_list(spec, &loc)?;
                for sym in &symbols {
                    let def = parent.sims.get(sym).cloned().ok_or_else(|| {
                        Error::at(ErrorKind::UndefinedSim(sym.clone()), loc.clone())
                    })?;
                    brought.push((sym.clone(), sym.clone()));
                    merge(def)?;
                }
                InheritKind::ImportList(symbols)
            } else {
                let mut parts = spec.split_whitespace();
                let sym = parts.next().unwrap_or("").to_string();
                let alias = parts.next().map(str::to_string);
                if sym.is_empty() || parts.next().is_some() {
                    return Err(Error::at(
                        ErrorKind::InvalidLektos(format!("malformed import: `{spec}`")),
                        loc,
                    ));
                }
                // The alias becomes the sim's symbol here, so the
                // ostensive symbol grammar applies to it.
                if let Some(alias) = &alias {
                    if !alias.chars().all(sigil::is_symbolic) {
                        return Err(Error::at(
                            ErrorKind::InvalidLektos(format!(
                                "import alias `{alias}`: a symbol is a run of symbolic characters"
                            )),
                            loc,
                        ));
                    }
                    if alias.contains('{') || alias.contains('}') {
                        return Err(Error::at(
                            ErrorKind::InvalidLektos(format!(
                                "import alias `{alias}`: `{{` and `}}` are reserved and cannot \
                                 appear in a symbol"
                            )),
                            loc,
                        ));
                    }
                }
                let mut def =
                    parent.sims.get(&sym).cloned().ok_or_else(|| {
                        Error::at(ErrorKind::UndefinedSim(sym.clone()), loc.clone())
                    })?;
                if let Some(alias) = &alias {
                    def.symbol = alias.clone();
                }
                brought.push((sym.clone(), def.symbol.clone()));
                merge(def)?;
                InheritKind::Import { symbol: sym, alias }
            }
        }
        _ => unreachable!(),
    };
    for (lang, names) in &parent_glossae {
        let entry = dialektos.glossae.entry(lang.clone()).or_default();
        for (psym, csym) in &brought {
            if let Some(name) = names.get(psym) {
                entry.entry(csym.clone()).or_insert_with(|| name.clone());
            }
        }
    }
    let whole = matches!(recorded, InheritKind::Full | InheritKind::Exclude(_));
    for (name, vocab) in parent_vocabularies {
        let bound = brought.iter().any(|(_, csym)| {
            let def = &dialektos.sims[csym];
            def.lemma_vocabulary.as_deref() == Some(name.as_str())
                || def.genos_vocabulary.as_deref() == Some(name.as_str())
        });
        if !whole && !bound {
            continue;
        }
        let entry = dialektos.vocabularies.entry(name).or_default();
        for (canonical, aliases) in vocab.terms {
            entry.terms.entry(canonical).or_default().extend(aliases);
        }
    }
    dialektos.lineage.push(InheritOp {
        source: source_id.to_string(),
        kind: recorded,
    });
    Ok(())
}

fn parse_symbol_list(spec: &str, loc: &Location) -> Result<Vec<String>> {
    let spec = spec.trim();
    let inner = spec
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| {
            Error::at(
                ErrorKind::InvalidLektos(format!("expected `[sym ...]`, found `{spec}`")),
                loc.clone(),
            )
        })?;
    Ok(inner.split_whitespace().map(str::to_string).collect())
}

/// Parse the body of one `@=== name ... ===@` block, consuming up
/// to and including the closing `===@` line. Returns the definition
/// and the number of lines consumed.
fn parse_sim_block(
    name: &str,
    body: &[&str],
    sigil: Sigil,
    path: &Path,
    first_line: usize,
) -> Result<(SimDef, usize)> {
    let sig = sigil.active();
    let loc = |offset: usize| Location {
        file: path.to_path_buf(),
        line: first_line + offset,
        col: 1,
    };
    let mut idx = 0;
    let mut lemma_vocabulary: Option<String> = None;
    while idx < body.len() && body[idx].trim().is_empty() {
        idx += 1;
    }
    if idx == body.len() {
        return Err(Error::at(
            ErrorKind::InvalidLektos(format!("sim `{name}`: missing ostensive definition")),
            loc(0),
        ));
    }

    // --- Ostensive definition -----------------------------------
    let first = body[idx].trim();
    let Some(after_sigil) = first.strip_prefix(sig) else {
        return Err(Error::at(
            ErrorKind::InvalidLektos(format!(
                "sim `{name}`: ostensive definition must start with the sigil"
            )),
            loc(idx),
        ));
    };
    let (symbol, after_symbol) = take_ostensive_symbol(after_sigil);
    if symbol.is_empty() {
        return Err(Error::at(
            ErrorKind::InvalidLektos(format!("sim `{name}`: empty symbol")),
            loc(idx),
        ));
    }
    // The braces are reserved for plerographic name references
    // (spec: "Metagraphe").
    if symbol.contains('{') || symbol.contains('}') {
        return Err(Error::at(
            ErrorKind::InvalidLektos(format!(
                "sim `{name}`: `{{` and `}}` are reserved and cannot appear in a symbol"
            )),
            loc(idx),
        ));
    }

    let flipped = sigil::episymbol(&symbol, true);
    let unflipped = sigil::episymbol(&symbol, false);

    let (form, bracket_matching, consumed) = if let Some(keyword) = monosim_keyword(after_symbol) {
        // Monosim: `@sym(<keyword>)`, nothing after the group.
        (
            SimForm::Mono {
                keyword: keyword.to_string(),
            },
            true,
            1,
        )
    } else if let Some((bm, _)) = line_ends_with_episim(after_symbol, &flipped, &unflipped, sig) {
        // Endo: sim and episim on one line, `grammata` in between.
        (SimForm::Endo, bm, 1)
    } else {
        // Para: header line, `grammata` line(s), episim line.
        let (taxis, rest) = take_taxis_marker(after_symbol);
        let header = rest.trim();
        // `lemma=<vocabulary>` binds the lemma to a controlled
        // vocabulary (spec: "Controlled Vocabularies").
        let (header, bound_vocab) = match header
            .strip_prefix("lemma=")
            .map(|v| ("lemma", v))
            .or_else(|| {
                header
                    .strip_prefix("[lemma=")
                    .and_then(|v| v.strip_suffix(']'))
                    .map(|v| ("[lemma]", v))
            }) {
            Some((h, v)) => (h, Some(v.trim().to_string())),
            None => (header, None),
        };
        if bound_vocab.as_deref() == Some("") {
            return Err(Error::at(
                ErrorKind::InvalidLektos(format!("sim `{name}`: `lemma=` names no vocabulary")),
                loc(idx),
            ));
        }
        lemma_vocabulary = bound_vocab;
        let lemma = match header {
            "" => Optionality::Unsupported,
            "lemma" => Optionality::Required,
            "[lemma]" => Optionality::Optional,
            other => {
                return Err(Error::at(
                    ErrorKind::InvalidLektos(format!(
                        "sim `{name}`: unexpected header content `{other}`"
                    )),
                    loc(idx),
                ));
            }
        };
        // Find the episim line.
        let mut close = idx + 1;
        let mut found = None;
        while close < body.len() {
            let line = body[close].trim();
            for (candidate, bm) in [(&flipped, true), (&unflipped, false)] {
                let prefix = format!("{candidate}{sig}");
                if let Some(rest) = line.strip_prefix(prefix.as_str()) {
                    found = Some((bm, rest.trim().to_string(), close));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
            close += 1;
        }
        let Some((bm, tail, close)) = found else {
            return Err(Error::at(
                ErrorKind::InvalidLektos(format!("sim `{name}`: episim line not found")),
                loc(idx),
            ));
        };
        // The grammata keyword line: `grammata` (block-structured)
        // or `stichos` (line-structured; F1 resolution).
        let middles: Vec<&str> = body[idx + 1..close]
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        // Property keyword lines (`autonym`, `header`) precede the
        // one grammata keyword line.
        let (props, body) = match middles.split_last() {
            Some((body, props)) => (props, *body),
            None => (&[][..], ""),
        };
        let mut autonym = false;
        let mut header = false;
        for prop in props {
            match *prop {
                "autonym" => autonym = true,
                "header" => header = true,
                other => {
                    return Err(Error::at(
                        ErrorKind::InvalidLektos(format!(
                            "sim `{name}`: unknown property keyword `{other}` \
                             (expected `autonym` or `header`)"
                        )),
                        loc(idx),
                    ));
                }
            }
        }
        let (stichoi, rows, cells) = match body {
            "grammata" => (false, false, false),
            "stichos" => (true, false, false),
            "rows" => (false, true, false),
            "cells" => (false, false, true),
            _ if header => (false, false, false),
            _ => {
                return Err(Error::at(
                    ErrorKind::InvalidLektos(format!(
                        "sim `{name}`: expected optional property keywords \
                         (`autonym`) followed by a `grammata`, `stichos`, \
                         `rows` or `cells` line in the ostensive definition"
                    )),
                    loc(idx),
                ));
            }
        };
        if header && !cells {
            return Err(Error::at(
                ErrorKind::InvalidLektos(format!(
                    "sim `{name}`: the `header` keyword belongs to a `cells`-form sim"
                )),
                loc(idx),
            ));
        }
        let hypograph = match tail.as_str() {
            "" => Optionality::Unsupported,
            "hypograph" => Optionality::Required,
            "[hypograph]" => Optionality::Optional,
            other => {
                return Err(Error::at(
                    ErrorKind::InvalidLektos(format!(
                        "sim `{name}`: unexpected episim-line content `{other}`"
                    )),
                    loc(close),
                ));
            }
        };
        (
            SimForm::Para {
                taxis,
                lemma,
                hypograph,
                stichoi,
                autonym,
                rows,
                cells,
                header,
            },
            bm,
            close - idx + 1,
        )
    };
    idx += consumed;

    // --- Property child sims ------------------------------------
    let mut def = SimDef {
        name: name.to_string(),
        symbol,
        form,
        bracket_matching,
        parent: None,
        lemma_vocabulary,
        genos_vocabulary: None,
        short_desc: None,
        long_desc: None,
    };
    let close = format!("==={sig}");
    while idx < body.len() {
        let line = body[idx].trim();
        if line.is_empty() {
            idx += 1;
            continue;
        }
        if line == close {
            return Ok((def, idx + 1));
        }
        let long_open = format!("{sig}\"\"");
        let short_open = format!("{sig}\"");
        let parent_open = format!("{sig}^(");
        let genos_vocab_open = format!("{sig}% ");
        if let Some(rest) = line.strip_prefix(genos_vocab_open.as_str()) {
            let vocab = rest.trim();
            if vocab.is_empty() {
                return Err(Error::at(
                    ErrorKind::InvalidLektos(format!("sim `{name}`: `{sig}%` names no vocabulary")),
                    loc(idx),
                ));
            }
            def.genos_vocabulary = Some(vocab.to_string());
            idx += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix(long_open.as_str()) {
            // `@"" ... ""@` long description block.
            let close = format!("\"\"{sig}");
            if let Some(inline) = rest.strip_suffix(close.as_str()) {
                def.long_desc = Some(inline.trim().to_string());
                idx += 1;
            } else {
                let start = idx + 1;
                let mut end = start;
                while end < body.len() && body[end].trim() != close {
                    end += 1;
                }
                if end == body.len() {
                    return Err(Error::at(
                        ErrorKind::UnmatchedSim(long_open.clone()),
                        loc(idx),
                    ));
                }
                def.long_desc = Some(body[start..end].join("\n").trim().to_string());
                idx = end + 1;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix(short_open.as_str()) {
            let close = format!("\"{sig}");
            let Some(desc) = rest.strip_suffix(close.as_str()) else {
                return Err(Error::at(
                    ErrorKind::InvalidLektos(format!("sim `{name}`: malformed short description")),
                    loc(idx),
                ));
            };
            def.short_desc = Some(desc.trim().to_string());
            idx += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix(parent_open.as_str()) {
            let Some(parent) = rest.strip_suffix(')') else {
                return Err(Error::at(
                    ErrorKind::InvalidLektos(format!("sim `{name}`: malformed parent reference")),
                    loc(idx),
                ));
            };
            def.parent = Some(parent.to_string());
            idx += 1;
            continue;
        }
        return Err(Error::at(
            ErrorKind::InvalidLektos(format!(
                "sim `{name}`: unexpected definition content `{line}`"
            )),
            loc(idx),
        ));
    }
    Err(Error::at(
        ErrorKind::UnmatchedSim(format!("{sig}=== {name}")),
        loc(0),
    ))
}

/// Take the symbol from the start of an ostensive definition line
/// (text after the sigil). The symbol is the maximal run of symbolic
/// characters, except that `(` / `[` end it when they introduce an
/// ostensive keyword group (`(taxis)`, `[(taxis)]`, `(param)`).
fn take_ostensive_symbol(text: &str) -> (String, &str) {
    let mut symbol = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if !sigil::is_symbolic(c) {
            break;
        }
        if (c == '(' && (rest.starts_with("(taxis)") || monosim_keyword(rest).is_some()))
            || (c == '[' && rest.starts_with("[(taxis)]"))
        {
            break;
        }
        symbol.push(c);
        rest = &rest[c.len_utf8()..];
    }
    (symbol, rest)
}

/// A monosim ostensive parameter group: `(<identifier>)` ending
/// the line. Any identifier serves as the (semantic) keyword,
/// except the reserved `taxis` (which marks a para header).
fn monosim_keyword(text: &str) -> Option<&str> {
    let inner = text.trim_end().strip_prefix('(')?.strip_suffix(')')?;
    let ok = !inner.is_empty() && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    (ok && inner != "taxis").then_some(inner)
}

/// Take an optional taxis marker from an ostensive para header.
fn take_taxis_marker(text: &str) -> (Optionality, &str) {
    if let Some(rest) = text.strip_prefix("(taxis)") {
        (Optionality::Required, rest)
    } else if let Some(rest) = text.strip_prefix("[(taxis)]") {
        (Optionality::Optional, rest)
    } else {
        (Optionality::Unsupported, text)
    }
}

/// If `text` (after the symbol on an ostensive endo line) ends with
/// one of the two candidate episims, return the inferred
/// bracket-matching flag and the enclosed content.
fn line_ends_with_episim<'t>(
    text: &'t str,
    flipped: &str,
    unflipped: &str,
    sig: char,
) -> Option<(bool, &'t str)> {
    let trimmed = text.trim_end();
    for (candidate, bm) in [(flipped, true), (unflipped, false)] {
        let suffix = format!("{candidate}{sig}");
        if let Some(content) = trimmed.strip_suffix(suffix.as_str())
            && content.contains("grammata")
        {
            return Some((bm, content));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemorySource;

    fn parse_str(src: &str) -> Result<Dialektos> {
        let mut stack = vec![];
        parse_definition(
            src,
            Path::new("test.lektos"),
            "test",
            &MemorySource::new(),
            &mut stack,
        )
    }

    #[test]
    fn atomos() {
        // The elementary dialektos from the spec (Section "atomos").
        let src = "@@@!atrep\n\n\
            @=== inline\n@/ grammata /@\n===@\n\n\
            @=== block\n@-[(taxis)] lemma\ngrammata\n-@ hypograph\n===@\n";
        let d = parse_str(src).unwrap();
        assert_eq!(d.sims.len(), 2);
        let inline = &d.sims["/"];
        assert_eq!(inline.form, SimForm::Endo);
        let block = &d.sims["-"];
        assert_eq!(
            block.form,
            SimForm::Para {
                autonym: false,
                taxis: Optionality::Optional,
                lemma: Optionality::Required,
                hypograph: Optionality::Required,
                stichoi: false,
                rows: false,
                cells: false,
                header: false,
            }
        );
    }

    #[test]
    fn monosim_and_descriptions() {
        let src = "@@@!atrep\n\n\
            @=== ref\n@^(param)\n@\"reference to an anchor\"@\n===@\n";
        let d = parse_str(src).unwrap();
        let r = &d.sims["^"];
        assert_eq!(
            r.form,
            SimForm::Mono {
                keyword: "param".into()
            }
        );
        assert_eq!(r.short_desc.as_deref(), Some("reference to an anchor"));
    }

    #[test]
    fn bracket_matching_inference() {
        // `@<' closed by unflipped `<@`: bracket matching disabled.
        let src = "@@@!atrep\n\n@=== aside\n@< grammata <@\n===@\n";
        let d = parse_str(src).unwrap();
        assert!(!d.sims["<"].bracket_matching);
        // `@[` closed by flipped `]@`: enabled.
        let src2 = "@@@!atrep\n\n@=== bracketed\n@[ grammata ]@\n===@\n";
        let d2 = parse_str(src2).unwrap();
        assert!(d2.sims["["].bracket_matching);
        // Braces are reserved for plerographic name references.
        let src3 = "@@@!atrep\n\n@=== braced\n@{ grammata }@\n===@\n";
        let err = parse_str(src3).unwrap_err();
        assert!(format!("{err}").contains("reserved"), "{err}");
    }

    #[test]
    fn conflict_is_error() {
        let src = "@@@!atrep\n\n\
            @=== a\n@/ grammata /@\n===@\n\n\
            @=== b\n@/ grammata /@\n===@\n";
        let err = parse_str(src).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::SimConflict(_)));
    }

    #[test]
    fn serialize_all_forms() {
        let mk = |name: &str, symbol: &str, form: SimForm, bm: bool| SimDef {
            name: name.into(),
            symbol: symbol.into(),
            form,
            bracket_matching: bm,
            parent: None,
            lemma_vocabulary: None,
            genos_vocabulary: None,
            short_desc: None,
            long_desc: None,
        };
        let mut d = Dialektos {
            id: "t".into(),
            sims: BTreeMap::new(),
            lineage: Vec::new(),
            vocabularies: BTreeMap::new(),
            glossae: BTreeMap::new(),
        };
        let mut block = mk(
            "block",
            "-",
            SimForm::Para {
                autonym: false,
                taxis: Optionality::Optional,
                lemma: Optionality::Required,
                hypograph: Optionality::Required,
                stichoi: false,
                rows: false,
                cells: false,
                header: false,
            },
            true,
        );
        block.short_desc = Some("a block".into());
        block.long_desc = Some("Long text.".into());
        block.parent = Some("~".into());
        d.sims.insert("-".into(), block);
        d.sims.insert(
            "+".into(),
            mk(
                "bare",
                "+",
                SimForm::Para {
                    autonym: false,
                    taxis: Optionality::Unsupported,
                    lemma: Optionality::Unsupported,
                    hypograph: Optionality::Unsupported,
                    stichoi: false,
                    rows: false,
                    cells: false,
                    header: false,
                },
                true,
            ),
        );
        d.sims
            .insert("[".into(), mk("bracket", "[", SimForm::Endo, true));
        d.sims.insert(
            "^".into(),
            mk(
                "anchor",
                "^",
                SimForm::Mono {
                    keyword: "param".into(),
                },
                true,
            ),
        );
        assert_eq!(
            serialize(&d),
            "@@@!atrep\n\
             \n\
             @=== anchor\n\
             @^(param)\n\
             ===@\n\
             \n\
             @=== bare\n\
             @+\n\
             grammata\n\
             +@\n\
             ===@\n\
             \n\
             @=== block\n\
             @-[(taxis)] lemma\n\
             grammata\n\
             -@ hypograph\n\
             @\"a block\"@\n\
             @\"\"\n\
             Long text.\n\
             \"\"@\n\
             @^(~)\n\
             ===@\n\
             \n\
             @=== bracket\n\
             @[ grammata ]@\n\
             ===@\n"
        );
        // Round trip: parsing the canonical form reproduces the
        // same definitions.
        let reparsed = parse_str(&serialize(&d)).unwrap();
        assert_eq!(reparsed.sims, d.sims);
    }

    #[test]
    fn longest_match_prefers_longer() {
        let src = "@@@!atrep\n\n\
            @=== h1\n@# lemma\ngrammata\n#@\n===@\n\n\
            @=== h2\n@## lemma\ngrammata\n##@\n===@\n";
        let d = parse_str(src).unwrap();
        assert_eq!(d.longest_match("## Title").unwrap().symbol, "##");
        assert_eq!(d.longest_match("# Title").unwrap().symbol, "#");
    }
}
