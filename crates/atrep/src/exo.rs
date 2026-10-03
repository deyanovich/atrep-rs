//! Exomorphosis: rendering a kanon to an external format via
//! `.exo` rule files written in the `atrep-exo` meta-dialektos
//! (spec: chapter "Metamorphoses", v0.10.1).
//!
//! Pilot notes:
//! - Resolution is local-only (`<dialektos>.<target>.exo` next to
//!   the document), with a fallback to the embedded standard
//!   library (see `STD_EXOS`).
//! - Rule inheritance follows the dialektos lineage (spec: Rule
//!   Resolution and Inheritance): parents' effective rule sets are
//!   pulled in declaration order (later ops overriding earlier),
//!   filtered and alias-remapped per import kind, then overlaid by
//!   the dialektos's own file. The most derived non-empty escape
//!   table applies.

use std::collections::HashMap;
use std::path::Path;

use crate::dendron::{Block, Document, Inline, Taxis};
use crate::dialektos::{self, Dialektos, InheritKind, SimForm};
use crate::error::{Error, ErrorKind, Location, Result};
use crate::sigil::{self, Sigil};
use crate::source::{DirSource, Source, fetch_normalized};

/// Embedded standard-library exomorphoses: (dialektos, target,
/// source).
const STD_EXOS: &[(&str, &str, &str)] = &[
    // The unseen pack renders nothing in place, for every host
    // target; hosts inherit these through the lineage.
    (
        "at-aphanes",
        "latex",
        include_str!("../std/at-aphanes.latex.exo"),
    ),
    (
        "at-aphanes",
        "html",
        include_str!("../std/at-aphanes.html.exo"),
    ),
    (
        "at-aphanes",
        "gemtext",
        include_str!("../std/at-aphanes.gemtext.exo"),
    ),
    (
        "at-aphanes",
        "epub",
        include_str!("../std/at-aphanes.epub.exo"),
    ),
    (
        "at-aphanes",
        "kindle",
        include_str!("../std/at-aphanes.kindle.exo"),
    ),
    (
        "at-aphanes",
        "tei",
        include_str!("../std/at-aphanes.tei.exo"),
    ),
    (
        "at-aphanes",
        "usfm",
        include_str!("../std/at-aphanes.usfm.exo"),
    ),
    (
        "at-aphanes",
        "usx",
        include_str!("../std/at-aphanes.usx.exo"),
    ),
    (
        "at-aphanes",
        "osis",
        include_str!("../std/at-aphanes.osis.exo"),
    ),
    (
        "at-aphanes",
        "docbook",
        include_str!("../std/at-aphanes.docbook.exo"),
    ),
    (
        "at-epimerismos",
        "latex",
        include_str!("../std/at-epimerismos.latex.exo"),
    ),
    (
        "at-epimerismos",
        "html",
        include_str!("../std/at-epimerismos.html.exo"),
    ),
    (
        "at-epimerismos",
        "gemtext",
        include_str!("../std/at-epimerismos.gemtext.exo"),
    ),
    (
        "at-epimerismos",
        "epub",
        include_str!("../std/at-epimerismos.epub.exo"),
    ),
    (
        "at-epimerismos",
        "kindle",
        include_str!("../std/at-epimerismos.kindle.exo"),
    ),
    (
        "at-epimerismos",
        "tei",
        include_str!("../std/at-epimerismos.tei.exo"),
    ),
    (
        "at-epimerismos",
        "docbook",
        include_str!("../std/at-epimerismos.docbook.exo"),
    ),
    ("at-html", "html", include_str!("../std/at-html.html.exo")),
    ("at-html", "xhtml", include_str!("../std/at-html.xhtml.exo")),
    (
        "at-markdown",
        "md",
        include_str!("../std/at-markdown.md.exo"),
    ),
    ("at-djot", "dj", include_str!("../std/at-djot.dj.exo")),
    (
        "at-docbook",
        "docbook",
        include_str!("../std/at-docbook.docbook.exo"),
    ),
    ("at-org", "org", include_str!("../std/at-org.org.exo")),
    ("at-rst", "rst", include_str!("../std/at-rst.rst.exo")),
    ("at-tei", "tei", include_str!("../std/at-tei.tei.exo")),
    ("at-usfm", "usfm", include_str!("../std/at-usfm.usfm.exo")),
    ("at-usfm", "usx", include_str!("../std/at-usfm.usx.exo")),
    ("at-usfm", "osis", include_str!("../std/at-usfm.osis.exo")),
    ("at-usfm", "latex", include_str!("../std/at-usfm.latex.exo")),
    ("koine", "latex", include_str!("../std/koine.latex.exo")),
    (
        "at-usfm",
        "latex.redletter",
        include_str!("../std/at-usfm.latex.redletter.exo"),
    ),
    (
        "at-usfm",
        "latex.a5",
        include_str!("../std/at-usfm.latex.a5.exo"),
    ),
    (
        "at-usfm",
        "latex.ru",
        include_str!("../std/at-usfm.latex.ru.exo"),
    ),
    (
        "litogramma",
        "html",
        include_str!("../std/litogramma.html.exo"),
    ),
    (
        "litogramma",
        "gemtext",
        include_str!("../std/litogramma.gemtext.exo"),
    ),
    (
        "litogramma",
        "latex",
        include_str!("../std/litogramma.latex.exo"),
    ),
    (
        "bibliogramma",
        "bib",
        include_str!("../std/bibliogramma.bib.exo"),
    ),
    (
        "bibliogramma",
        "tei",
        include_str!("../std/bibliogramma.tei.exo"),
    ),
    (
        "bibliogramma",
        "bibtex",
        include_str!("../std/bibliogramma.bibtex.exo"),
    ),
];

/// Slot component names.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SlotName {
    Lemma,
    Grammata,
    Hypograph,
    TaxisSlot,
    Onym,
    Genoses,
    Param,
    Dialect,
    Content,
    Cells,
    Scheme,
    Value,
    /// A sim-name slot (spec: Slots and Templates): the name of a
    /// monosim-form sim of the source dialektos, resolved to its
    /// symbol at parse time. Renders the parameters of the node's
    /// own monosim children of that sim, space-joined — the
    /// mechanism by which an unseen annotation (at-aphanes)
    /// becomes an attribute of the element its host emits.
    Named(String),
}

impl SlotName {
    fn parse(name: &str) -> Option<SlotName> {
        Some(match name {
            "lemma" => SlotName::Lemma,
            "grammata" => SlotName::Grammata,
            "hypograph" => SlotName::Hypograph,
            "taxis" => SlotName::TaxisSlot,
            "onym" => SlotName::Onym,
            "genoses" => SlotName::Genoses,
            "param" => SlotName::Param,
            "cells" => SlotName::Cells,
            "dialect" => SlotName::Dialect,
            "content" => SlotName::Content,
            "scheme" => SlotName::Scheme,
            "value" => SlotName::Value,
            _ => return None,
        })
    }
}

/// One template segment.
#[derive(Debug, Clone)]
enum Seg {
    Lit(String),
    Slot {
        name: SlotName,
        raw: bool,
        prefix: Option<String>,
        /// A gate slot `@(?name)`: tested for the conditional
        /// section it stands in, rendered as nothing.
        gate: bool,
    },
    /// Conditional section `@[ ... ]@`: emitted only when every
    /// slot inside renders non-empty. No nesting.
    Group(Vec<Seg>),
}

/// A rule pattern key (genos suffixes are per-variant, not part of
/// the key).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PatternKey {
    /// Dialektos sim, by symbol.
    Sim(String),
    /// Reserved `*`-prefixed structural pattern, by name.
    Structural(String),
    /// `*solo <symbol>`: an endo-simmere or monosim standing alone
    /// in a paragraph; the rule replaces the paragraph.
    Solo(String),
    /// `*milestone [<scheme>]`: a milestone, optionally keyed
    /// by its reference scheme.
    MilestoneKey(Option<String>),
    /// `*deixis <symbol>`: a deixis pointing at the given sim.
    DeixisKey(String),
    /// `*row <symbol> <separator>`: cell interpretation of a
    /// stichoi sim's lines (the separator lives on the rule).
    Row(String),
    /// `*cell <symbol>`: one cell of a row.
    Cell(String),
    /// `<symbol>(<term>)`: a variant for nodes of a
    /// vocabulary-lemma sim whose canonical lemma is the term.
    TermKey(String, String),
}

impl std::fmt::Display for PatternKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternKey::Sim(s) => write!(f, "{s}"),
            PatternKey::Structural(n) => write!(f, "*{n}"),
            PatternKey::Solo(s) => write!(f, "*solo {s}"),
            PatternKey::MilestoneKey(Some(s)) => write!(f, "*milestone {s}"),
            PatternKey::MilestoneKey(None) => write!(f, "*milestone"),
            PatternKey::DeixisKey(s) => write!(f, "*deixis {s}"),
            PatternKey::Row(s) => write!(f, "*row {s}"),
            PatternKey::Cell(s) => write!(f, "*cell {s}"),
            PatternKey::TermKey(s, t) => write!(f, "{s}({t})"),
        }
    }
}

const STRUCTURAL_NAMES: &[&str] = &[
    "document",
    "paragraph",
    "text",
    "stichoi",
    "strophe",
    "stichos",
    "verbatim",
    "verbatim-inline",
    "diaphane",
    "diaphane-inline",
    "englossis",
    "media",
    "onym-anchor",
];

#[derive(Debug, Clone)]
struct Rule {
    genoses: Vec<String>,
    template: Vec<Seg>,
    /// `*row` rules: the declared cell separator.
    separator: Option<char>,
}

/// A parsed exomorphosis definition.
#[derive(Debug)]
pub struct Exo {
    pub source: String,
    pub target: String,
    escape: HashMap<char, String>,
    rules: HashMap<PatternKey, Vec<Rule>>,
    /// Associated exos (`@+dialect=>target`): englossis content
    /// of that dialektos renders through its own exo for the
    /// associated target into an auxiliary output, instead of
    /// inline (a LaTeX export routing embedded bibliogramma
    /// references into the companion .bib).
    pub associates: Vec<(String, String)>,
    /// Asset sections (`@=* <filename> <sentinel>`): static
    /// presentation companions (a LaTeX class, a stylesheet)
    /// carried byte-verbatim, delivered as unconditional
    /// auxiliary outputs.
    pub assets: Vec<(String, String)>,
}

// ---------------------------------------------------------------
// Definition parsing
// ---------------------------------------------------------------

/// Resolve the effective exomorphosis for `(dialect, target)` in
/// `dir`: inherited rules per the dialektos lineage, overlaid by
/// the dialektos's own `.exo` file (local-only, with a fallback to
/// the embedded standard library).
pub fn resolve_exo(dir: &Path, dialect: &str, target: &str) -> Result<Exo> {
    resolve_exo_from(&DirSource::new(dir), dialect, target)
}

/// Resolve a named exo variant: the base exomorphosis overlaid
/// by `<dialect>.<target>.<variant>.exo` (own rules, escape
/// table, and assets override; everything else inherits). The
/// presentation counterpart of morphism variants.
pub fn resolve_exo_variant(
    dir: &Path,
    dialect: &str,
    target: &str,
    variant: Option<&str>,
) -> Result<Exo> {
    resolve_exo_variant_from(&DirSource::new(dir), dialect, target, variant)
}

/// [`resolve_exo_variant`] over an arbitrary [`Source`].
pub fn resolve_exo_variant_from(
    ctx: &dyn Source,
    dialect: &str,
    target: &str,
    variant: Option<&str>,
) -> Result<Exo> {
    let base = resolve_exo_from(ctx, dialect, target)?;
    let Some(variant) = variant else {
        return Ok(base);
    };
    let dial = dialektos::resolve_from(ctx, dialect)?;
    let name = format!("{dialect}.{target}.{variant}.exo");
    let own = if let Some(normalized) = fetch_normalized(ctx, &name)? {
        Some(parse_exo_source(
            &normalized,
            Path::new(&name),
            dialect,
            target,
            &dial,
        )?)
    } else {
        let key = format!("{target}.{variant}");
        STD_EXOS
            .iter()
            .find(|(d, t, _)| *d == dialect && *t == key)
            .map(|(_, _, src)| {
                let pseudo = Path::new("std:").join(&name);
                parse_exo_source(src, &pseudo, dialect, target, &dial)
            })
            .transpose()?
    };
    match own {
        Some(v) => Ok(overlay(base, v)),
        None => Err(Error::new(ErrorKind::UnresolvableExo(format!(
            "{dialect}=>{target} variant `{variant}`"
        )))),
    }
}

/// [`resolve_exo`] over an arbitrary resolution [`Source`].
pub fn resolve_exo_from(ctx: &dyn Source, dialect: &str, target: &str) -> Result<Exo> {
    resolve_exo_inner(ctx, dialect, target)?
        .ok_or_else(|| Error::new(ErrorKind::UnresolvableExo(format!("{dialect}=>{target}"))))
}

fn resolve_exo_inner(ctx: &dyn Source, dialect: &str, target: &str) -> Result<Option<Exo>> {
    // Definition-level cycle detection in dialektos resolution
    // guards this recursion: a cyclic lineage cannot resolve.
    let dial = dialektos::resolve_from(ctx, dialect)?;
    let mut inherited: Option<Exo> = None;
    for op in &dial.lineage {
        if let Some(parent) = resolve_exo_inner(ctx, &op.source, target)? {
            let filtered = filter_remap(parent, &op.kind);
            inherited = Some(match inherited {
                Some(base) => overlay(base, filtered),
                None => filtered,
            });
        }
    }
    let own = load_own_exo(ctx, dialect, target, &dial)?;
    Ok(match (inherited, own) {
        (None, None) => None,
        (Some(mut merged), None) => {
            merged.source = dialect.to_string();
            merged.target = target.to_string();
            Some(merged)
        }
        (None, Some(own)) => Some(own),
        (Some(merged), Some(own)) => Some(overlay(merged, own)),
    })
}

/// The dialektos's own `.exo` file, if any: local file first, then
/// the embedded standard library.
fn load_own_exo(
    ctx: &dyn Source,
    dialect: &str,
    target: &str,
    dial: &Dialektos,
) -> Result<Option<Exo>> {
    let name = format!("{dialect}.{target}.exo");
    if let Some(normalized) = fetch_normalized(ctx, &name)? {
        return parse_exo_source(&normalized, Path::new(&name), dialect, target, dial).map(Some);
    }
    for (d, t, src) in STD_EXOS {
        if *d == dialect && *t == target {
            let pseudo = Path::new("std:").join(format!("{dialect}.{target}.exo"));
            return parse_exo_source(src, &pseudo, dialect, target, dial).map(Some);
        }
    }
    Ok(None)
}

/// Whether a pattern key is keyed on one of the given symbols.
/// Structural patterns are not.
fn key_on_symbols(key: &PatternKey, symbols: &[String]) -> bool {
    match key {
        PatternKey::Sim(s)
        | PatternKey::Solo(s)
        | PatternKey::DeixisKey(s)
        | PatternKey::Row(s)
        | PatternKey::Cell(s)
        | PatternKey::TermKey(s, _) => symbols.iter().any(|x| x == s),
        PatternKey::Structural(_) | PatternKey::MilestoneKey(_) => false,
    }
}

/// Filter and alias-remap a parent's effective rule set per one
/// inheritance declaration: full inheritance pulls everything,
/// exclusion drops the excluded symbols' rules, imports pull only
/// the imported symbols' rules (remapped along an alias).
fn filter_remap(mut exo: Exo, kind: &InheritKind) -> Exo {
    match kind {
        InheritKind::Full => exo,
        InheritKind::Exclude(list) => {
            exo.rules.retain(|key, _| !key_on_symbols(key, list));
            exo
        }
        InheritKind::ImportList(list) => {
            exo.rules.retain(|key, _| key_on_symbols(key, list));
            exo
        }
        InheritKind::Import { symbol, alias } => {
            let wanted = std::slice::from_ref(symbol);
            exo.rules.retain(|key, _| key_on_symbols(key, wanted));
            if let Some(alias) = alias {
                exo.rules = exo
                    .rules
                    .into_iter()
                    .map(|(key, rules)| {
                        let key = match key {
                            PatternKey::Sim(_) => PatternKey::Sim(alias.clone()),
                            PatternKey::Solo(_) => PatternKey::Solo(alias.clone()),
                            PatternKey::DeixisKey(_) => PatternKey::DeixisKey(alias.clone()),
                            structural => structural,
                        };
                        (key, rules)
                    })
                    .collect();
            }
            exo
        }
    }
}

/// Overlay `add` onto `base`: same-pattern rules (key plus genos
/// set) are replaced, new ones appended; a non-empty escape table
/// replaces the base table wholesale (most derived wins), as does
/// the identity.
fn overlay(mut base: Exo, add: Exo) -> Exo {
    for (key, variants) in add.rules {
        let entry = base.rules.entry(key).or_default();
        for rule in variants {
            match entry.iter_mut().find(|r| r.genoses == rule.genoses) {
                Some(existing) => *existing = rule,
                None => entry.push(rule),
            }
        }
    }
    if !add.escape.is_empty() {
        base.escape = add.escape;
    }
    base.source = add.source;
    base.target = add.target;
    if !add.associates.is_empty() {
        base.associates = add.associates;
    }
    if !add.assets.is_empty() {
        base.assets = add.assets;
    }
    base
}

/// Parse an `.exo` source against the source dialektos (needed for
/// longest-defined-symbol pattern matching).
pub fn parse_exo_source(
    source: &str,
    path: &Path,
    dialect: &str,
    target: &str,
    dial: &Dialektos,
) -> Result<Exo> {
    let loc = |line: usize| Location {
        file: path.to_path_buf(),
        line,
        col: 1,
    };
    let invalid = |line: usize, msg: String| Error::at(ErrorKind::InvalidExo(msg), loc(line));

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    if i < lines.len() && lines[i].starts_with("#!") {
        i += 1;
    }
    let sigil = match lines.get(i).map(|l| l.trim()) {
        Some("@@@!atrep-exo") => Sigil::Canonical,
        Some("\\\\\\!atrep-exo") => Sigil::Alias,
        _ => {
            return Err(invalid(
                i + 1,
                "missing `@@@!atrep-exo` declaration".to_string(),
            ));
        }
    };
    let sig = sigil.active();
    i += 1;

    let mut exo = Exo {
        source: String::new(),
        target: String::new(),
        escape: HashMap::new(),
        rules: HashMap::new(),
        associates: Vec::new(),
        assets: Vec::new(),
    };

    let comment = format!("{sig}{sig}/");
    let decl_open = format!("{sig}=");
    let asset_open = format!("{sig}=* ");
    let assoc_open = format!("{sig}+");
    let escape_open = format!("{sig}%");
    let escape_close = format!("%{sig}");
    let rule_open = format!("{sig}-> ");
    let rule_close = format!(">-{sig}");

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || line.starts_with(&comment) {
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix(&asset_open) {
            let mut parts = rest.split_whitespace();
            let (Some(filename), Some(sentinel), None) = (parts.next(), parts.next(), parts.next())
            else {
                return Err(invalid(
                    i + 1,
                    format!(
                        "malformed asset section `{line}` (expected `@=* <filename> <sentinel>`)"
                    ),
                ));
            };
            if filename.contains('/') || filename.contains('\\') {
                return Err(invalid(
                    i + 1,
                    format!("asset filename `{filename}` must not contain path separators"),
                ));
            }
            if exo.assets.iter().any(|(f, _)| f == filename) {
                return Err(invalid(i + 1, format!("duplicate asset `{filename}`")));
            }
            let header_line = i + 1;
            i += 1;
            let mut body = String::new();
            loop {
                if i >= lines.len() {
                    return Err(invalid(
                        header_line,
                        format!("asset `{filename}`: missing sentinel line `{sentinel}`"),
                    ));
                }
                if lines[i] == sentinel {
                    i += 1;
                    break;
                }
                body.push_str(lines[i]);
                body.push('\n');
                i += 1;
            }
            exo.assets.push((filename.to_string(), body));
            continue;
        }
        if let Some(rest) = line.strip_prefix(&assoc_open) {
            let Some((a, b)) = rest.split_once("=>") else {
                return Err(invalid(
                    i + 1,
                    format!("malformed association `{line}` (expected `@+dialect=>target`)"),
                ));
            };
            exo.associates
                .push((a.trim().to_string(), b.trim().to_string()));
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix(&decl_open) {
            if !exo.source.is_empty() {
                return Err(invalid(i + 1, "duplicate metamorphosis declaration".into()));
            }
            let Some((src, tgt)) = rest.split_once("=>") else {
                return Err(invalid(
                    i + 1,
                    format!("malformed metamorphosis declaration `{line}`"),
                ));
            };
            exo.source = src.trim().to_string();
            exo.target = tgt.trim().to_string();
            if exo.source != dialect || exo.target != target {
                return Err(invalid(
                    i + 1,
                    format!(
                        "metamorphosis declaration `{}=>{}` does not match \
                         `{dialect}.{target}.exo`",
                        exo.source, exo.target
                    ),
                ));
            }
            i += 1;
            continue;
        }
        if line == escape_open {
            if !exo.escape.is_empty() {
                return Err(invalid(i + 1, "duplicate escape table".into()));
            }
            i += 1;
            while i < lines.len() && lines[i].trim() != escape_close {
                let entry = lines[i];
                if !entry.trim().is_empty() {
                    let mut chars = entry.chars();
                    let from = chars.next().unwrap();
                    let to = chars.as_str().trim_start();
                    if to.is_empty() {
                        return Err(invalid(
                            i + 1,
                            format!("escape entry for `{from}` has no replacement"),
                        ));
                    }
                    if exo.escape.insert(from, to.to_string()).is_some() {
                        return Err(invalid(i + 1, format!("duplicate escape for `{from}`")));
                    }
                }
                i += 1;
            }
            if i == lines.len() {
                return Err(invalid(i, "unterminated escape table".into()));
            }
            i += 1;
            continue;
        }
        if let Some(pattern_text) = line.strip_prefix(&rule_open) {
            let (key, genoses, separator) =
                parse_pattern(pattern_text.trim(), dial).map_err(|msg| invalid(i + 1, msg))?;
            let start = i + 1;
            let mut end = start;
            while end < lines.len() && lines[end].trim() != rule_close {
                end += 1;
            }
            if end == lines.len() {
                return Err(invalid(
                    i + 1,
                    format!("unterminated rule `{pattern_text}`"),
                ));
            }
            let body = lines[start..end].join("\n");
            let template = parse_template(&body, sig, dial).map_err(|msg| invalid(i + 1, msg))?;
            let variants = exo.rules.entry(key.clone()).or_default();
            if variants.iter().any(|r| r.genoses == genoses) {
                return Err(invalid(
                    i + 1,
                    format!("duplicate pattern `{pattern_text}`"),
                ));
            }
            variants.push(Rule {
                genoses,
                template,
                separator,
            });
            i = end + 1;
            continue;
        }
        return Err(invalid(i + 1, format!("unexpected line: `{line}`")));
    }
    if exo.source.is_empty() {
        return Err(invalid(1, "missing metamorphosis declaration".into()));
    }
    Ok(exo)
}

/// Parse a rule pattern: a `*`-prefixed structural name or a
/// dialektos sim symbol (longest defined symbol), each optionally
/// followed by `.genos` suffixes. A `*` followed by an alphanumeric
/// is the structural prefix; otherwise `*` is symbol material.
#[allow(clippy::type_complexity)]
fn parse_pattern(
    text: &str,
    dial: &Dialektos,
) -> std::result::Result<(PatternKey, Vec<String>, Option<char>), String> {
    let mut separator: Option<char> = None;
    let structural = text.strip_prefix('*').filter(|r| {
        r.chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    });
    let (key, rest) = if let Some(rest) = structural {
        let name: String = rest
            .chars()
            .take_while(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            .collect();
        let after = &rest[name.len()..];
        if name == "milestone" {
            let arg = after.trim();
            if arg.is_empty() {
                (PatternKey::MilestoneKey(None), String::new())
            } else {
                let scheme: String = arg
                    .chars()
                    .take_while(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                    .collect();
                if scheme.is_empty() {
                    return Err(format!("`*milestone` argument `{arg}` is not a scheme"));
                }
                let tail = arg[scheme.len()..].trim_start().to_string();
                (PatternKey::MilestoneKey(Some(scheme)), tail)
            }
        } else if name == "solo" || name == "deixis" || name == "row" || name == "cell" {
            // Argument-taking: `*solo <symbol>[.genos...]` /
            // `*deixis <symbol>[.genos...]` / `*cell <symbol>` /
            // `*row <symbol> <separator>`.
            let arg = after.trim_start();
            if arg.is_empty() {
                return Err(format!("`*{name}` requires a sim symbol argument"));
            }
            let Some(def) = dial.longest_match(arg) else {
                return Err(format!(
                    "`*{name}` argument `{arg}` does not start with a defined \
                     sim symbol of dialektos `{}`",
                    dial.id
                ));
            };
            let symbol = def.symbol.clone();
            let mut tail = arg[symbol.len()..].trim_start().to_string();
            let key = match name.as_str() {
                "solo" => PatternKey::Solo(symbol),
                "deixis" => PatternKey::DeixisKey(symbol),
                "cell" => PatternKey::Cell(symbol),
                _ => {
                    // The separator character follows the symbol.
                    let sep_part = tail.trim_start().to_string();
                    let Some(sep) = sep_part.chars().next().filter(|c| !c.is_whitespace()) else {
                        return Err("`*row` requires a separator character".into());
                    };
                    tail = sep_part[sep.len_utf8()..].trim_start().to_string();
                    separator = Some(sep);
                    PatternKey::Row(symbol)
                }
            };
            (key, tail)
        } else {
            if !STRUCTURAL_NAMES.contains(&name.as_str()) {
                return Err(format!("unknown structural pattern `*{name}`"));
            }
            (PatternKey::Structural(name), after.to_string())
        }
    } else {
        // Longest defined symbol of the source dialektos.
        let Some(def) = dial.longest_match(text) else {
            return Err(format!(
                "pattern `{text}` does not start with a defined sim symbol \
                 of dialektos `{}`",
                dial.id
            ));
        };
        let symbol = def.symbol.clone();
        let after = text[symbol.len()..].to_string();
        // A parenthesized canonical term keys a per-term variant
        // for vocabulary-lemma sims: `@-> :(auctor)`.
        if let Some(rest) = after.strip_prefix('(')
            && let Some(close) = rest.find(')')
        {
            let term = rest[..close].trim().to_string();
            if term.is_empty() {
                return Err("empty vocabulary term in pattern".into());
            }
            (
                PatternKey::TermKey(symbol, term),
                rest[close + 1..].to_string(),
            )
        } else {
            (PatternKey::Sim(symbol), after)
        }
    };
    let mut genoses = Vec::new();
    let mut rest = rest.as_str();
    while !rest.is_empty() {
        let Some(after_dot) = rest.strip_prefix('.') else {
            return Err(format!("unexpected pattern suffix `{rest}`"));
        };
        let genos: String = after_dot
            .chars()
            .take_while(|&c| sigil::is_genos_continue(c))
            .collect();
        if !sigil::is_valid_genos(&genos) {
            return Err(format!("invalid genos suffix `.{after_dot}`"));
        }
        rest = &after_dot[genos.len()..];
        genoses.push(genos);
    }
    Ok((key, genoses, separator))
}

/// Parse a template body into segments. `\@` yields a literal `@`;
/// any other active sigil must open a slot.
fn parse_template(
    body: &str,
    sig: char,
    dial: &Dialektos,
) -> std::result::Result<Vec<Seg>, String> {
    let chars: Vec<char> = body.chars().collect();
    let (segs, end) = parse_segs(&chars, 0, sig, false, dial)?;
    debug_assert_eq!(end, chars.len());
    Ok(segs)
}

/// Resolve a slot name that is not a built-in component: the name
/// of a monosim-form sim of the source dialektos (a sim-name
/// slot), resolved to its symbol.
fn named_slot(name: &str, dial: &Dialektos) -> std::result::Result<SlotName, String> {
    match dial.sim_named(name) {
        dialektos::NamedLookup::One(def) if matches!(def.form, SimForm::Mono { .. }) => {
            Ok(SlotName::Named(def.symbol.clone()))
        }
        dialektos::NamedLookup::One(_) => Err(format!(
            "slot `{name}` names a sim of dialektos `{}` that is not a monosim",
            dial.id
        )),
        dialektos::NamedLookup::Ambiguous => Err(format!(
            "slot `{name}` is ambiguous in dialektos `{}` (several sims share the name)",
            dial.id
        )),
        dialektos::NamedLookup::None => Err(format!("unknown slot `{name}`")),
    }
}

/// Parse template segments from `i`. Inside a conditional section
/// (`in_group`), parsing stops at the closing `]` + sigil.
fn parse_segs(
    chars: &[char],
    mut i: usize,
    sig: char,
    in_group: bool,
    dial: &Dialektos,
) -> std::result::Result<(Vec<Seg>, usize), String> {
    let inactive = if sig == sigil::CANONICAL {
        sigil::ALIAS
    } else {
        sigil::CANONICAL
    };
    let mut segs: Vec<Seg> = Vec::new();
    let mut lit = String::new();
    let flush = |lit: &mut String, segs: &mut Vec<Seg>| {
        if !lit.is_empty() {
            segs.push(Seg::Lit(std::mem::take(lit)));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if in_group && c == ']' && chars.get(i + 1) == Some(&sig) {
            flush(&mut lit, &mut segs);
            return Ok((segs, i + 2));
        }
        if c == inactive && chars.get(i + 1) == Some(&sig) {
            lit.push(sig);
            i += 2;
            continue;
        }
        if c == sig {
            match chars.get(i + 1) {
                Some(&'(') => {
                    let raw = chars.get(i + 2) == Some(&'(');
                    let mut j = i + if raw { 3 } else { 2 };
                    let gate = chars.get(j) == Some(&'?');
                    if gate {
                        if raw {
                            return Err("a gate slot has no raw form".to_string());
                        }
                        if !in_group {
                            return Err("gate slot outside a conditional section".to_string());
                        }
                        j += 1;
                    }
                    let name_start = j;
                    while j < chars.len() && (chars[j].is_ascii_lowercase() || chars[j] == '-') {
                        j += 1;
                    }
                    let name: String = chars[name_start..j].iter().collect();
                    let slot = match SlotName::parse(&name) {
                        Some(slot) => slot,
                        None => named_slot(&name, dial)?,
                    };
                    let prefix = if chars.get(j) == Some(&':') {
                        j += 1;
                        let p_start = j;
                        while j < chars.len() && chars[j] != ')' {
                            j += 1;
                        }
                        Some(chars[p_start..j].iter().collect::<String>())
                    } else {
                        None
                    };
                    let close_ok = if raw {
                        chars.get(j) == Some(&')') && chars.get(j + 1) == Some(&')')
                    } else {
                        chars.get(j) == Some(&')')
                    };
                    if !close_ok {
                        return Err(format!("malformed slot near `{name}`"));
                    }
                    if gate && prefix.is_some() {
                        return Err(format!("gate slot `{name}` takes no hanging prefix"));
                    }
                    j += if raw { 2 } else { 1 };
                    flush(&mut lit, &mut segs);
                    segs.push(Seg::Slot {
                        name: slot,
                        raw,
                        prefix,
                        gate,
                    });
                    i = j;
                }
                Some(&'[') => {
                    if in_group {
                        return Err("conditional sections do not nest".to_string());
                    }
                    let (inner, next) = parse_segs(chars, i + 2, sig, true, dial)?;
                    if !inner.iter().any(|seg| matches!(seg, Seg::Slot { .. })) {
                        return Err("conditional section contains no slot".to_string());
                    }
                    flush(&mut lit, &mut segs);
                    segs.push(Seg::Group(inner));
                    i = next;
                }
                _ => {
                    return Err("unescaped sigil in template (write it as an \
                                inactive-sigil escape)"
                        .to_string());
                }
            }
            continue;
        }
        lit.push(c);
        i += 1;
    }
    if in_group {
        return Err("unterminated conditional section".to_string());
    }
    flush(&mut lit, &mut segs);
    Ok((segs, i))
}

// ---------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------

/// Render a kanon to the exomorphosis's target format. `dir` is the
/// document's directory, used to resolve the `.exo` of any
/// dialektos entered via monad-englossis.
pub fn render(doc: &Document, exo: &Exo, dir: &Path) -> Result<String> {
    render_with_aux(doc, exo, dir).map(|(main, _)| main)
}

/// One auxiliary output: the embedded dialektos, the
/// associated target, and the rendered content. Asset sections
/// use the marker `*asset` in the dialektos position and their
/// filename in the target position.
pub type AuxOutput = (String, String, String);

/// Render the document plus the auxiliary outputs produced by
/// associated exos (`@+dialect=>target`), e.g. the companion
/// .bib of a LaTeX export.
pub fn render_with_aux(doc: &Document, exo: &Exo, dir: &Path) -> Result<(String, Vec<AuxOutput>)> {
    if doc.dialect_id != exo.source {
        return Err(Error::new(ErrorKind::InvalidExo(format!(
            "document declares `{}` but the exomorphosis is for `{}`",
            doc.dialect_id, exo.source
        ))));
    }
    let r = Renderer {
        exo,
        dir,
        aux: std::cell::RefCell::new(Vec::new()),
        raw_depth: std::cell::Cell::new(0),
    };
    let grammata = r.render_blocks(&doc.blocks)?;
    let main = match r.find_rule(&PatternKey::Structural("document".into()), &[])? {
        Some(rule) => r.fill(rule, &Slots::document(&grammata))?,
        None => return Err(unhandled("*document")),
    };
    let mut aux = r.aux.into_inner();
    for (filename, content) in &exo.assets {
        aux.push(("*asset".to_string(), filename.clone(), content.clone()));
    }
    Ok((main, aux))
}

fn unhandled(pattern: &str) -> Error {
    Error::new(ErrorKind::ExoUnhandled(pattern.to_string()))
}

/// Slot values available for one node.
#[derive(Default)]
struct Slots<'a> {
    lemma: Option<String>,
    grammata: Option<String>,
    hypograph: Option<String>,
    taxis: Option<String>,
    onym: Option<&'a str>,
    genoses: Option<String>,
    param: Option<&'a str>,
    dialect: Option<&'a str>,
    content: Option<&'a str>,
    cells: Option<String>,
    scheme: Option<&'a str>,
    value: Option<&'a str>,
    /// The node's own inline content, for sim-name slots: the
    /// grammata of an endo-simmere or paragraph, the lemma of a
    /// para-simmere.
    inlines: Option<&'a [Inline]>,
}

impl<'a> Slots<'a> {
    fn document(grammata: &str) -> Slots<'a> {
        Slots {
            grammata: Some(grammata.to_string()),
            ..Slots::default()
        }
    }
}

static HEADER_GENOS: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| "header".to_string());

/// Split a line's inlines on a separator character occurring in
/// text nodes; leading/trailing whitespace of each cell trims,
/// empty edge cells (from leading/trailing separators) drop.
fn split_cells(line: &[Inline], separator: char) -> Vec<Vec<Inline>> {
    let mut cells: Vec<Vec<Inline>> = vec![Vec::new()];
    for inline in line {
        match inline {
            Inline::Text(t) => {
                let mut first = true;
                for piece in t.split(separator) {
                    if !first {
                        cells.push(Vec::new());
                    }
                    first = false;
                    if !piece.is_empty() {
                        cells
                            .last_mut()
                            .unwrap()
                            .push(Inline::Text(piece.to_string()));
                    }
                }
            }
            other => cells.last_mut().unwrap().push(other.clone()),
        }
    }
    // Trim cell edges and drop empty edge cells.
    for cell in &mut cells {
        if let Some(Inline::Text(t)) = cell.first_mut() {
            *t = t.trim_start().to_string();
            if t.is_empty() {
                cell.remove(0);
            }
        }
        if let Some(Inline::Text(t)) = cell.last_mut() {
            *t = t.trim_end().to_string();
            if t.is_empty() {
                cell.pop();
            }
        }
    }
    while cells.first().is_some_and(Vec::is_empty) {
        cells.remove(0);
    }
    while cells.last().is_some_and(Vec::is_empty) {
        cells.pop();
    }
    cells
}

struct Renderer<'a> {
    exo: &'a Exo,
    dir: &'a Path,
    /// Auxiliary outputs from associated exos:
    /// (dialect, target, content).
    aux: std::cell::RefCell<Vec<(String, String, String)>>,
    /// While positive, the escape table is suspended: content
    /// destined for a raw slot (`@((grammata))` and friends)
    /// renders unescaped — math and verbatim-adjacent targets
    /// (LaTeX) need the source characters back.
    raw_depth: std::cell::Cell<u32>,
}

impl Renderer<'_> {
    /// Most specific rule for `key` among variants whose genoses are
    /// all present on the node; ties are an error.
    fn find_rule(&self, key: &PatternKey, node_genoses: &[String]) -> Result<Option<&Rule>> {
        let Some(variants) = self.exo.rules.get(key) else {
            return Ok(None);
        };
        let mut best: Option<&Rule> = None;
        let mut tied = false;
        for rule in variants {
            if !rule.genoses.iter().all(|g| node_genoses.contains(g)) {
                continue;
            }
            match best {
                Some(b) if rule.genoses.len() == b.genoses.len() => tied = true,
                Some(b) if rule.genoses.len() > b.genoses.len() => {
                    best = Some(rule);
                    tied = false;
                }
                None => {
                    best = Some(rule);
                    tied = false;
                }
                _ => {}
            }
        }
        if tied {
            return Err(Error::new(ErrorKind::ExoAmbiguous(key.to_string())));
        }
        Ok(best)
    }

    fn escape(&self, text: &str) -> String {
        if self.exo.escape.is_empty() || self.raw_depth.get() > 0 {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            match self.exo.escape.get(&c) {
                Some(rep) => out.push_str(rep),
                None => out.push(c),
            }
        }
        out
    }

    /// Fill a rule's template from the node's slot values.
    fn fill(&self, rule: &Rule, slots: &Slots) -> Result<String> {
        let mut out = String::new();
        self.fill_segs(&rule.template, slots, &mut out)?;
        Ok(out)
    }

    /// Whether the rule's template uses `@((name))` — the raw
    /// spelling — for this slot.
    fn wants_raw(segs: &[Seg], slot: &SlotName) -> bool {
        segs.iter().any(|seg| match seg {
            Seg::Slot { name, raw, .. } => *raw && name == slot,
            Seg::Group(inner) => Self::wants_raw(inner, slot),
            Seg::Lit(_) => false,
        })
    }

    /// A sim-name slot's value: the parameters of the node's own
    /// monosim children of the named sim, in document order,
    /// joined with single spaces. Empty when the node kind has no
    /// inline content of its own.
    fn named_value(inlines: Option<&[Inline]>, symbol: &str) -> String {
        let mut out = String::new();
        for inline in inlines.unwrap_or(&[]) {
            if let Inline::Monosim {
                symbol: s, param, ..
            } = inline
                && s == symbol
            {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(param);
            }
        }
        out
    }

    /// The monosim symbols a template names through sim-name
    /// slots (gate slots included).
    fn named_symbols<'s>(segs: &'s [Seg], out: &mut Vec<&'s str>) {
        for seg in segs {
            match seg {
                Seg::Slot {
                    name: SlotName::Named(symbol),
                    ..
                } => out.push(symbol),
                Seg::Group(inner) => Self::named_symbols(inner, out),
                _ => {}
            }
        }
    }

    /// Render the inline content a rule's node holds directly. A
    /// monosim the rule names through a sim-name slot is taken
    /// over by the rule: the host emits its parameter, so the
    /// monosim's own rule does not render it a second time.
    fn render_held(&self, rule: &Rule, inlines: &[Inline]) -> Result<String> {
        let mut named = Vec::new();
        Self::named_symbols(&rule.template, &mut named);
        if named.is_empty() {
            return self.render_inlines(inlines);
        }
        let mut out = String::new();
        for inline in inlines {
            if let Inline::Monosim { symbol, .. } = inline
                && named.contains(&symbol.as_str())
            {
                continue;
            }
            out.push_str(&self.render_inline(inline)?);
        }
        Ok(out)
    }

    /// Run `render` with the escape table suspended.
    fn render_raw<T>(&self, render: impl FnOnce() -> Result<T>) -> Result<T> {
        self.raw_depth.set(self.raw_depth.get() + 1);
        let result = render();
        self.raw_depth.set(self.raw_depth.get() - 1);
        result
    }

    /// Render inline content escaped or raw per the rule's slot
    /// spelling.
    fn render_inlines_for(
        &self,
        rule: &Rule,
        slot: SlotName,
        inlines: &[Inline],
    ) -> Result<String> {
        // Sim-name slots read the lemma of a para-simmere, never
        // its hypograph.
        let render = || match slot {
            SlotName::Hypograph => self.render_inlines(inlines),
            _ => self.render_held(rule, inlines),
        };
        if Self::wants_raw(&rule.template, &slot) {
            self.render_raw(render)
        } else {
            render()
        }
    }

    /// Fill segments into `out`. Returns false when any slot
    /// rendered empty - the gate for conditional sections.
    fn fill_segs(&self, segs: &[Seg], slots: &Slots, out: &mut String) -> Result<bool> {
        let mut all_nonempty = true;
        for seg in segs {
            match seg {
                Seg::Lit(text) => out.push_str(text),
                Seg::Slot {
                    name,
                    raw,
                    prefix,
                    gate,
                } => {
                    let (value, escapable) = match name {
                        SlotName::Lemma => (slots.lemma.clone(), false),
                        SlotName::Grammata => (slots.grammata.clone(), false),
                        SlotName::Hypograph => (slots.hypograph.clone(), false),
                        SlotName::TaxisSlot => (slots.taxis.clone(), true),
                        SlotName::Onym => (slots.onym.map(str::to_string), true),
                        SlotName::Genoses => (slots.genoses.clone(), true),
                        SlotName::Param => (slots.param.map(str::to_string), true),
                        SlotName::Dialect => (slots.dialect.map(str::to_string), true),
                        SlotName::Content => (slots.content.map(str::to_string), true),
                        SlotName::Cells => (slots.cells.clone(), false),
                        SlotName::Scheme => (slots.scheme.map(str::to_string), true),
                        SlotName::Value => (slots.value.map(str::to_string), true),
                        SlotName::Named(symbol) => {
                            (Some(Self::named_value(slots.inlines, symbol)), true)
                        }
                    };
                    let Some(value) = value else {
                        return Err(Error::new(ErrorKind::InvalidExo(format!(
                            "slot `{name:?}` is not available to this rule's node kind"
                        ))));
                    };
                    if value.is_empty() {
                        all_nonempty = false;
                    }
                    if *gate {
                        continue;
                    }
                    let value = if escapable && !*raw {
                        self.escape(&value)
                    } else {
                        value
                    };
                    match prefix {
                        Some(p) => out.push_str(&hanging_prefix(&value, p)),
                        None => out.push_str(&value),
                    }
                }
                Seg::Group(inner) => {
                    let mut buf = String::new();
                    if self.fill_segs(inner, slots, &mut buf)? {
                        out.push_str(&buf);
                    }
                }
            }
        }
        Ok(all_nonempty)
    }

    fn render_blocks(&self, blocks: &[Block]) -> Result<String> {
        let rendered: Vec<String> = blocks
            .iter()
            .map(|b| self.render_block(b))
            .collect::<Result<_>>()?;
        Ok(rendered.join("\n"))
    }

    /// Render a strophe's lines as table rows: split each line
    /// on the row rule's separator, render cells through the
    /// `*cell` rule, and fill the row template's @(cells) slot.
    /// A line of dashes/separators/spaces is the header rule:
    /// dropped, with prior rows rendered through the `.header`
    /// genos variants when defined.
    fn render_rows(
        &self,
        symbol: &str,
        row_key: &PatternKey,
        lines: &[Vec<Inline>],
    ) -> Result<Vec<String>> {
        let cell_key = PatternKey::Cell(symbol.to_string());
        // The separator lives on the (any) row rule variant.
        let separator = self
            .exo
            .rules
            .get(row_key)
            .and_then(|v| v.first())
            .and_then(|r| r.separator)
            .ok_or_else(|| {
                Error::new(ErrorKind::InvalidExo(format!(
                    "`*row {symbol}` rule has no separator"
                )))
            })?;
        let is_header_rule = |line: &[Inline]| -> bool {
            let mut saw_dash = false;
            for inline in line {
                let Inline::Text(t) = inline else {
                    return false;
                };
                for c in t.chars() {
                    match c {
                        '-' => saw_dash = true,
                        c if c == separator || c.is_whitespace() || c == '+' => {}
                        _ => return false,
                    }
                }
            }
            saw_dash
        };
        let header_rows = lines.iter().position(|l| is_header_rule(l));
        let mut out = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            if is_header_rule(line) {
                continue;
            }
            let header = header_rows.is_some_and(|h| idx < h);
            let genoses: &[String] = if header {
                std::slice::from_ref(&HEADER_GENOS)
            } else {
                &[]
            };
            let mut cells_out = String::new();
            for cell in split_cells(line, separator) {
                let Some(cell_rule) = self.find_rule(&cell_key, genoses)? else {
                    return Err(unhandled(&format!("*cell {symbol}")));
                };
                let slots = Slots {
                    grammata: Some(self.render_inlines(&cell)?),
                    ..Slots::default()
                };
                cells_out.push_str(&self.fill(cell_rule, &slots)?);
            }
            let Some(row_rule) = self.find_rule(row_key, genoses)? else {
                return Err(unhandled(&format!("*row {symbol}")));
            };
            let slots = Slots {
                cells: Some(cells_out),
                ..Slots::default()
            };
            out.push(self.fill(row_rule, &slots)?);
        }
        Ok(out)
    }

    fn render_inlines(&self, inlines: &[Inline]) -> Result<String> {
        let mut out = String::new();
        for inline in inlines {
            out.push_str(&self.render_inline(inline)?);
        }
        Ok(out)
    }

    fn render_block(&self, block: &Block) -> Result<String> {
        match block {
            Block::Paragraph(inlines) => {
                // An endo-simmere or monosim standing alone: a
                // matching `*solo` rule replaces the paragraph.
                if let [
                    Inline::Endo {
                        symbol,
                        content,
                        ann,
                        ..
                    },
                ] = inlines.as_slice()
                    && let Some(rule) =
                        self.find_rule(&PatternKey::Solo(symbol.clone()), &ann.genoses)?
                {
                    let slots = Slots {
                        grammata: Some(self.render_held(rule, content)?),
                        onym: Some(ann.onym.as_deref().unwrap_or("")),
                        genoses: Some(ann.genoses.join(" ")),
                        inlines: Some(content),
                        ..Slots::default()
                    };
                    return self.fill(rule, &slots);
                }
                if let [Inline::Monosim { symbol, param, ann }] = inlines.as_slice()
                    && let Some(rule) =
                        self.find_rule(&PatternKey::Solo(symbol.clone()), &ann.genoses)?
                {
                    let slots = Slots {
                        param: Some(param),
                        onym: Some(ann.onym.as_deref().unwrap_or("")),
                        genoses: Some(ann.genoses.join(" ")),
                        ..Slots::default()
                    };
                    return self.fill(rule, &slots);
                }
                let key = PatternKey::Structural("paragraph".into());
                let Some(rule) = self.find_rule(&key, &[])? else {
                    return Err(unhandled("*paragraph"));
                };
                let slots = Slots {
                    grammata: Some(self.render_held(rule, inlines)?),
                    inlines: Some(inlines),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Block::Para {
                symbol,
                taxis,
                lemma,
                children,
                hypograph,
                ann,
                ..
            } => {
                // A term-keyed variant (vocabulary-lemma sims)
                // wins over the plain sim rule.
                let term_rule = match lemma.as_slice() {
                    [Inline::Text(text)] => self.find_rule(
                        &PatternKey::TermKey(symbol.clone(), text.trim().to_string()),
                        &ann.genoses,
                    )?,
                    _ => None,
                };
                let rule = match term_rule {
                    Some(rule) => rule,
                    None => {
                        let key = PatternKey::Sim(symbol.clone());
                        let Some(rule) = self.find_rule(&key, &ann.genoses)? else {
                            return Err(unhandled(symbol));
                        };
                        rule
                    }
                };
                let slots = Slots {
                    lemma: Some(self.render_inlines_for(rule, SlotName::Lemma, lemma)?),
                    grammata: Some(if Self::wants_raw(&rule.template, &SlotName::Grammata) {
                        self.render_raw(|| self.render_blocks(children))?
                    } else {
                        self.render_blocks(children)?
                    }),
                    hypograph: Some(self.render_inlines_for(
                        rule,
                        SlotName::Hypograph,
                        hypograph,
                    )?),
                    taxis: Some(match taxis {
                        Some(Taxis::Explicit(n)) => n.to_string(),
                        _ => String::new(),
                    }),
                    inlines: Some(lemma),
                    onym: Some(ann.onym.as_deref().unwrap_or("")),
                    genoses: Some(ann.genoses.join(" ")),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Block::Stichoi {
                symbol,
                taxis,
                lemma,
                strophes,
                hypograph,
                ann,
                ..
            } => {
                // A dialektos-defined stichoi sim renders through
                // its symbol rule; the core form through *stichoi.
                let (key, desc) = match symbol {
                    Some(sym) => (PatternKey::Sim(sym.clone()), sym.as_str()),
                    None => (PatternKey::Structural("stichoi".into()), "*stichoi"),
                };
                let Some(rule) = self.find_rule(&key, &ann.genoses)? else {
                    return Err(unhandled(desc));
                };
                // Cell interpretation: a `*row` rule for this
                // sim splits each line on the declared separator
                // (spec: the kanon is untouched; this is a
                // rendering interpretation).
                let row_key = symbol
                    .as_ref()
                    .map(|s| PatternKey::Row(s.clone()))
                    .filter(|k| self.exo.rules.contains_key(k));
                let strophe_key = PatternKey::Structural("strophe".into());
                let stichos_key = PatternKey::Structural("stichos".into());
                let mut rendered_strophes = Vec::new();
                for strophe in strophes {
                    let mut rendered_lines = Vec::new();
                    if let Some(row_key) = &row_key {
                        // Rows bypass the strophe wrapper: they
                        // fill the sim rule's grammata directly.
                        rendered_lines =
                            self.render_rows(symbol.as_deref().unwrap(), row_key, &strophe.0)?;
                        rendered_strophes.push(rendered_lines.join("\n"));
                        continue;
                    } else {
                        for line in &strophe.0 {
                            let Some(line_rule) = self.find_rule(&stichos_key, &[])? else {
                                return Err(unhandled("*stichos"));
                            };
                            let slots = Slots {
                                grammata: Some(self.render_inlines(line)?),
                                ..Slots::default()
                            };
                            rendered_lines.push(self.fill(line_rule, &slots)?);
                        }
                    }
                    let Some(strophe_rule) = self.find_rule(&strophe_key, &[])? else {
                        return Err(unhandled("*strophe"));
                    };
                    let slots = Slots {
                        grammata: Some(rendered_lines.join("\n")),
                        ..Slots::default()
                    };
                    rendered_strophes.push(self.fill(strophe_rule, &slots)?);
                }
                let slots = Slots {
                    lemma: Some(self.render_inlines(lemma)?),
                    grammata: Some(rendered_strophes.join("\n")),
                    hypograph: Some(self.render_inlines(hypograph)?),
                    taxis: Some(match taxis {
                        Some(Taxis::Explicit(n)) => n.to_string(),
                        _ => String::new(),
                    }),
                    onym: Some(ann.onym.as_deref().unwrap_or("")),
                    genoses: Some(ann.genoses.join(" ")),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Block::ParaDiaphane { children, ann } => {
                let key = PatternKey::Structural("diaphane".into());
                match self.find_rule(&key, &ann.genoses)? {
                    Some(rule) => {
                        let slots = Slots {
                            grammata: Some(self.render_blocks(children)?),
                            onym: Some(ann.onym.as_deref().unwrap_or("")),
                            genoses: Some(ann.genoses.join(" ")),
                            ..Slots::default()
                        };
                        self.fill(rule, &slots)
                    }
                    // Default: transparent.
                    None => self.render_blocks(children),
                }
            }
            Block::VerbatimBlock { content, ann } => {
                let key = PatternKey::Structural("verbatim".into());
                let Some(rule) = self.find_rule(&key, &ann.genoses)? else {
                    return Err(unhandled("*verbatim"));
                };
                let slots = Slots {
                    content: Some(content),
                    onym: Some(ann.onym.as_deref().unwrap_or("")),
                    genoses: Some(ann.genoses.join(" ")),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Block::MonadEnglossis {
                dialect,
                children,
                ann,
            } => {
                let key = PatternKey::Structural("englossis".into());
                let Some(rule) = self.find_rule(&key, &ann.genoses)? else {
                    return Err(unhandled("*englossis"));
                };
                // The enclosed dialektos renders with its own
                // rules - inline for the same target, or into an
                // auxiliary output through an associated exo.
                let association = self
                    .exo
                    .associates
                    .iter()
                    .find(|(d, _)| d == dialect)
                    .cloned();
                let grammata = if let Some((_, aux_target)) = association {
                    let inner_exo = resolve_exo(self.dir, dialect, &aux_target)?;
                    let inner = Renderer {
                        exo: &inner_exo,
                        dir: self.dir,
                        aux: std::cell::RefCell::new(Vec::new()),
                        raw_depth: std::cell::Cell::new(0),
                    };
                    let content = inner.render_blocks(children)?;
                    self.aux
                        .borrow_mut()
                        .push((dialect.clone(), aux_target, content));
                    String::new()
                } else if *dialect == self.exo.source {
                    self.render_blocks(children)?
                } else {
                    let inner_exo = resolve_exo(self.dir, dialect, &self.exo.target)?;
                    let inner = Renderer {
                        exo: &inner_exo,
                        dir: self.dir,
                        aux: std::cell::RefCell::new(Vec::new()),
                        raw_depth: std::cell::Cell::new(self.raw_depth.get()),
                    };
                    inner.render_blocks(children)?
                };
                let slots = Slots {
                    grammata: Some(grammata),
                    dialect: Some(dialect),
                    onym: Some(ann.onym.as_deref().unwrap_or("")),
                    genoses: Some(ann.genoses.join(" ")),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Block::Enmedia { param } => {
                let key = PatternKey::Structural("media".into());
                let Some(rule) = self.find_rule(&key, &[])? else {
                    return Err(unhandled("*media"));
                };
                let slots = Slots {
                    param: Some(param),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Block::EnmediaHashed { .. }
            | Block::AnaphorEnglossis { .. }
            | Block::AnaphorEnlexis { .. }
            | Block::ParaAxioma { .. }
            | Block::AxiomaRefBlock { .. } => Err(Error::new(ErrorKind::Syntax(
                "exomorphosis input must be a kanon (unexpanded form found)".to_string(),
            ))),
        }
    }

    fn render_inline(&self, inline: &Inline) -> Result<String> {
        match inline {
            Inline::Text(text) => {
                let key = PatternKey::Structural("text".into());
                match self.find_rule(&key, &[])? {
                    Some(rule) => {
                        let slots = Slots {
                            content: Some(text),
                            ..Slots::default()
                        };
                        self.fill(rule, &slots)
                    }
                    // Default: identity through the escape table.
                    None => Ok(self.escape(text)),
                }
            }
            Inline::Endo {
                symbol,
                content,
                ann,
                ..
            } => {
                let key = PatternKey::Sim(symbol.clone());
                let Some(rule) = self.find_rule(&key, &ann.genoses)? else {
                    return Err(unhandled(symbol));
                };
                let slots = Slots {
                    grammata: Some(self.render_inlines_for(rule, SlotName::Grammata, content)?),
                    onym: Some(ann.onym.as_deref().unwrap_or("")),
                    genoses: Some(ann.genoses.join(" ")),
                    inlines: Some(content),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Inline::VerbatimInline { content, ann } => {
                let key = PatternKey::Structural("verbatim-inline".into());
                let Some(rule) = self.find_rule(&key, &ann.genoses)? else {
                    return Err(unhandled("*verbatim-inline"));
                };
                let slots = Slots {
                    content: Some(content),
                    onym: Some(ann.onym.as_deref().unwrap_or("")),
                    genoses: Some(ann.genoses.join(" ")),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Inline::EndoDiaphane { content, ann } => {
                let key = PatternKey::Structural("diaphane-inline".into());
                match self.find_rule(&key, &ann.genoses)? {
                    Some(rule) => {
                        let slots = Slots {
                            grammata: Some(self.render_held(rule, content)?),
                            onym: Some(ann.onym.as_deref().unwrap_or("")),
                            genoses: Some(ann.genoses.join(" ")),
                            inlines: Some(content),
                            ..Slots::default()
                        };
                        self.fill(rule, &slots)
                    }
                    None => self.render_inlines(content),
                }
            }
            Inline::Monosim { symbol, param, ann } => {
                let key = PatternKey::Sim(symbol.clone());
                let Some(rule) = self.find_rule(&key, &ann.genoses)? else {
                    return Err(unhandled(symbol));
                };
                let slots = Slots {
                    param: Some(param),
                    onym: Some(ann.onym.as_deref().unwrap_or("")),
                    genoses: Some(ann.genoses.join(" ")),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Inline::Milestone { scheme, value, ann } => {
                let rule = match self.find_rule(
                    &PatternKey::MilestoneKey(Some(scheme.clone())),
                    &ann.genoses,
                )? {
                    Some(rule) => Some(rule),
                    None => self.find_rule(&PatternKey::MilestoneKey(None), &ann.genoses)?,
                };
                let Some(rule) = rule else {
                    return Err(unhandled("*milestone"));
                };
                let slots = Slots {
                    scheme: Some(scheme),
                    value: Some(value),
                    genoses: Some(ann.genoses.join(" ")),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Inline::OnymAnchor(onym) => {
                let key = PatternKey::Structural("onym-anchor".into());
                let Some(rule) = self.find_rule(&key, &[])? else {
                    return Err(unhandled("*onym-anchor"));
                };
                let slots = Slots {
                    onym: Some(onym),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Inline::Deixis { symbol, onym, ann } => {
                let key = PatternKey::DeixisKey(symbol.clone());
                let Some(rule) = self.find_rule(&key, &ann.genoses)? else {
                    return Err(unhandled(&format!("*deixis {symbol}")));
                };
                let slots = Slots {
                    onym: Some(onym),
                    genoses: Some(ann.genoses.join(" ")),
                    ..Slots::default()
                };
                self.fill(rule, &slots)
            }
            Inline::EndoAxioma { .. } | Inline::AxiomaRef { .. } => {
                Err(Error::new(ErrorKind::Syntax(
                    "exomorphosis input must be a kanon (unexpanded form found)".to_string(),
                )))
            }
        }
    }
}

/// Apply a hanging prefix: every line but the first is prefixed; on
/// empty lines the prefix is applied with trailing whitespace
/// removed. The empty segment after a trailing line break is not a
/// line.
fn hanging_prefix(value: &str, prefix: &str) -> String {
    let trailing_newline = value.ends_with('\n');
    let body = if trailing_newline {
        &value[..value.len() - 1]
    } else {
        value
    };
    let mut out = String::with_capacity(value.len());
    for (i, line) in body.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
            if line.is_empty() {
                out.push_str(prefix.trim_end());
            } else {
                out.push_str(prefix);
            }
        }
        out.push_str(line);
    }
    if trailing_newline {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hanging_prefix_rules() {
        // Lines after the first are prefixed; empty lines take the
        // trimmed prefix; the trailing segment is not a line.
        assert_eq!(hanging_prefix("a\nb\n", "> "), "a\n> b\n");
        assert_eq!(hanging_prefix("a\n\nb\n", "> "), "a\n>\n> b\n");
        assert_eq!(hanging_prefix("a", "> "), "a");
        assert_eq!(hanging_prefix("a\n", "> "), "a\n");
    }

    fn empty_dial() -> Dialektos {
        Dialektos {
            id: "t".to_string(),
            sims: Default::default(),
            lineage: Vec::new(),
            vocabularies: Default::default(),
            glossae: Default::default(),
        }
    }

    #[test]
    fn template_parsing() {
        let dial = empty_dial();
        let segs = parse_template("x @(lemma) y @((content:  )) \\@(z)", '@', &dial).unwrap();
        assert_eq!(segs.len(), 5);
        assert!(matches!(
            &segs[1],
            Seg::Slot {
                name: SlotName::Lemma,
                raw: false,
                prefix: None,
                gate: false
            }
        ));
        assert!(matches!(
            &segs[3],
            Seg::Slot {
                name: SlotName::Content,
                raw: true,
                prefix: Some(p),
                gate: false
            } if p == "  "
        ));
        assert!(matches!(&segs[4], Seg::Lit(l) if l == " @(z)"));
        // A gate slot is tested, not printed, and lives only in a
        // conditional section, in the plain form.
        let segs = parse_template("@[a@(?lemma)b]@", '@', &dial).unwrap();
        assert!(matches!(&segs[0], Seg::Group(inner)
            if matches!(&inner[1], Seg::Slot { name: SlotName::Lemma, gate: true, .. })));
        assert!(parse_template("@(?lemma)", '@', &dial).is_err());
        assert!(parse_template("@[@((?lemma))]@", '@', &dial).is_err());
        assert!(parse_template("@[@(?lemma:  )]@", '@', &dial).is_err());
        assert!(parse_template("bare @ sigil", '@', &dial).is_err());
        assert!(parse_template("@(nope)", '@', &dial).is_err());
    }
}
