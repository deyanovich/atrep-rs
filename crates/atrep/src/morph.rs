//! Morphisms: mapping the kanon of one dialektos to the kanon of
//! another via `.hom` / `.iso` rule files (spec: chapter
//! "Transformations", v0.11 draft).
//!
//! Homomorphisms are directional and may lose information (unwrap,
//! drop); isomorphisms are rename-only, bijective, and one file
//! serves both directions. Same-symbol form-compatible sims map
//! implicitly; dialektos lineage derives embedding morphisms with
//! no file at all.

use std::collections::HashMap;
use std::path::Path;

use crate::dendron::{Block, Document, Inline};
use crate::dialektos::{self, Dialektos, InheritKind, Optionality, SimForm};
use crate::error::{Error, ErrorKind, Result};
use crate::kanonizo;
use crate::source::{DirSource, Source, fetch_normalized};

/// Embedded standard-library morphisms: (a, b, extension, source).
const STD_MORPHS: &[(&str, &str, &str, &str)] = &[
    (
        "at-html",
        "at-markdown",
        "hom",
        include_str!("../std/at-html.at-markdown.hom"),
    ),
    (
        "at-djot",
        "at-markdown",
        "iso",
        include_str!("../std/at-djot.at-markdown.iso"),
    ),
    (
        "at-org",
        "at-html",
        "hom",
        include_str!("../std/at-org.at-html.hom"),
    ),
    (
        "at-html",
        "at-org",
        "hom",
        include_str!("../std/at-html.at-org.hom"),
    ),
    (
        "at-rst",
        "at-html",
        "hom",
        include_str!("../std/at-rst.at-html.hom"),
    ),
    (
        "at-html",
        "at-rst",
        "hom",
        include_str!("../std/at-html.at-rst.hom"),
    ),
    (
        "litogramma",
        "at-html",
        "hom",
        include_str!("../std/litogramma.at-html.hom"),
    ),
    (
        "litogramma",
        "at-docbook",
        "hom",
        include_str!("../std/litogramma.at-docbook.hom"),
    ),
    (
        "litogramma",
        "at-tei",
        "hom",
        include_str!("../std/litogramma.at-tei.hom"),
    ),
    (
        "at-docbook",
        "litogramma",
        "hom",
        include_str!("../std/at-docbook.litogramma.hom"),
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    Rename {
        to: String,
        /// Genoses appended to the mapped simmere (hom-only).
        genoses: Vec<String>,
    },
    Drop,
    /// The wrapper dissolves and its content splices into the
    /// enclosing sequence (hom-only), parameterized by the
    /// disposition of the wrapper's lemma. This action set is
    /// closed under composition (spec: "Composition").
    Dissolve(Lemma),
}

/// Lemma disposition of a dissolve.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Lemma {
    /// `@<< <sym>` — the lemma is discarded (unwrap).
    Discard,
    /// `@<# <sym>` — the lemma is emitted first as a plain
    /// paragraph.
    Plain,
    /// `@<# <sym> <endo-sym> [.genos...]` — the lemma is emitted
    /// first, wrapped in the target endo-simmere (the
    /// solo-heading convention; extract).
    Heading {
        symbol: String,
        genoses: Vec<String>,
    },
}

impl Action {
    fn rename(to: &str) -> Action {
        Action::Rename {
            to: to.to_string(),
            genoses: Vec::new(),
        }
    }
}

/// A group rule: the algebra's one sequence-level action, the
/// flat-to-nested converse of the extracting dissolve (hom-only).
/// A trigger — a paragraph holding exactly one endo-simmere of
/// the trigger symbol — opens a region spanning the following
/// siblings (spec: "Group Rules").
#[derive(Debug, Clone, PartialEq, Eq)]
struct GroupRule {
    /// Source endo-sim symbol whose solo occurrences trigger.
    trigger: String,
    /// Nesting rank: a trigger closes every open region of equal
    /// or greater rank. Authored files rank by file order, and
    /// composition preserves distinct ranks — a pullback that
    /// would produce peer triggers sharing a rank is refused
    /// (no normal form; spec v0.11.1), so ranks are unique
    /// within a morphism and serialization is faithful.
    rank: usize,
    disposition: GroupDisposition,
}

/// Disposition of the wrapper and the trigger's content.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GroupDisposition {
    /// `@># <endo> <sym> [.genos...]` (`lemma: true`) — wrap the
    /// region, trigger content becomes the lemma; or
    /// `@>< <endo> <sym> [.genos...]` (`lemma: false`) — wrap
    /// with the trigger content discarded (recorded loss). The
    /// trigger's onym and genoses transfer to the wrapper either
    /// way.
    Wrap {
        symbol: String,
        genoses: Vec<String>,
        lemma: bool,
    },
    /// `@># <endo>` (operand-less) — deplain: the trigger
    /// re-emits as a plain paragraph of its content, region
    /// untouched. Onym and genoses are lost with the markup
    /// (recorded loss); an empty trigger emits nothing.
    Deplain,
    /// `@>- <endo>` — the trigger and its entire region drop.
    DropRegion,
    /// `@>: <endo> <endo'> [.genos...]` — the degenerate wrap:
    /// no wrapper; the trigger re-emits as a solo endo of the
    /// target symbol (solo occurrences only, unlike a rename).
    Relabel {
        symbol: String,
        genoses: Vec<String>,
    },
    /// `@>: <endo>` — the trigger paragraph is deleted and the
    /// region's siblings keep their places.
    DeleteTrigger,
}

/// A resolved morphism: the effective symbol map (explicit rules
/// plus implicit identities) from source to target.
#[derive(Debug, Clone)]
pub struct Morph {
    pub source: String,
    pub target: String,
    map: HashMap<String, Action>,
    /// Group rules, ordered by rank (the grouping stage of the
    /// normal form).
    groups: Vec<GroupRule>,
    /// Provenance hops (`@=via a=>b`) of a serialized composite,
    /// in application order; empty for authored morphisms.
    via: Vec<String>,
    source_dial: Dialektos,
    target_dial: Dialektos,
}

impl Morph {
    /// Whether this morphism is the identity on its source: an
    /// endomorphism mapping every sim of the source dialektos
    /// to itself, adding nothing. The decidable core of the
    /// registry's embedding metric: `A` embeds losslessly in
    /// `H` iff the fused round trip `A => H => A` is the
    /// identity.
    pub fn is_identity(&self) -> bool {
        self.source == self.target
            && self.groups.is_empty()
            && self.source_dial.sims.keys().all(|symbol| {
                matches!(
                    self.map.get(symbol),
                    Some(Action::Rename { to, genoses })
                        if to == symbol && genoses.is_empty()
                )
            })
    }

    /// A pure rename table: no group rules, and every action a
    /// rename (implicit identities and component-dropping renames
    /// included). The only first factor a grouping second factor
    /// fuses across (spec: "Composition").
    fn is_rename_only(&self) -> bool {
        self.groups.is_empty()
            && self
                .map
                .values()
                .all(|a| matches!(a, Action::Rename { .. }))
    }

    /// The group rule triggered by `symbol`, if any.
    fn group_for(&self, symbol: &str) -> Option<&GroupRule> {
        self.groups.iter().find(|g| g.trigger == symbol)
    }
}

// ---------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------

/// Resolve the morphism `source => target` in `dir`: an explicit
/// `.hom` or `.iso` file (an `.iso` also serves the reverse
/// direction), the embedded standard library, or an embedding
/// derived from the source dialektos's lineage.
pub fn resolve_morph(dir: &Path, source: &str, target: &str) -> Result<Morph> {
    resolve_morph_from(&DirSource::new(dir), source, target)
}

/// [`resolve_morph`] over an arbitrary resolution [`Source`].
pub fn resolve_morph_from(ctx: &dyn Source, source: &str, target: &str) -> Result<Morph> {
    resolve_morph_variant_from(ctx, source, target, None)
}

/// Resolve a named morphism variant (`<a>.<b>.<variant>.hom` /
/// `.iso`): several morphisms may exist for one pair, each a
/// different representation; the unnamed default participates in
/// route discovery, named variants only when explicitly
/// requested.
pub fn resolve_morph_variant(
    dir: &Path,
    source: &str,
    target: &str,
    variant: Option<&str>,
) -> Result<Morph> {
    resolve_morph_variant_from(&DirSource::new(dir), source, target, variant)
}

/// [`resolve_morph_variant`] over an arbitrary resolution
/// [`Source`].
pub fn resolve_morph_variant_from(
    ctx: &dyn Source,
    source: &str,
    target: &str,
    variant: Option<&str>,
) -> Result<Morph> {
    let src_dial = dialektos::resolve_from(ctx, source)?;
    let tgt_dial = dialektos::resolve_from(ctx, target)?;
    let infix = variant.map(|v| format!(".{v}")).unwrap_or_default();

    let read = |name: &str| -> Result<Option<String>> { fetch_normalized(ctx, name) };

    // Forward .hom, forward .iso, reverse .iso.
    let candidates: [(String, bool, bool); 3] = [
        (format!("{source}.{target}{infix}.hom"), false, false),
        (format!("{source}.{target}{infix}.iso"), true, false),
        (format!("{target}.{source}{infix}.iso"), true, true),
    ];
    for (name, iso, reversed) in &candidates {
        let text = match read(name)? {
            Some(text) => Some(text),
            None => std_morph_source(name),
        };
        if let Some(text) = text {
            return parse_morph_source(
                &text,
                Path::new(name),
                *iso,
                *reversed,
                &src_dial,
                &tgt_dial,
            );
        }
    }

    // Derived embedding from the lineage (default only: a named
    // variant must exist as a file).
    if variant.is_none()
        && let Some(morph) = derive_embedding(&src_dial, &tgt_dial)
    {
        return Ok(morph);
    }
    Err(Error::new(ErrorKind::UnresolvableMorph(format!(
        "{source}=>{target}{}",
        variant
            .map(|v| format!(" (variant {v})"))
            .unwrap_or_default()
    ))))
}

/// Resolve a morphism route from `source` to `target`: the direct
/// morphism when one exists, otherwise the shortest path of
/// directly resolvable morphisms through the dialektoi known to
/// the resolution context (local definition files and the
/// standard library). Two or more distinct shortest paths are an
/// error - the platform never chooses silently.
pub fn resolve_route(dir: &Path, source: &str, target: &str) -> Result<Vec<Morph>> {
    resolve_route_from(&DirSource::new(dir), source, target)
}

/// [`resolve_route`] over an arbitrary resolution [`Source`].
pub fn resolve_route_from(ctx: &dyn Source, source: &str, target: &str) -> Result<Vec<Morph>> {
    match try_direct(ctx, source, target)? {
        Some(m) => Ok(vec![m]),
        None => resolve_via(ctx, source, target),
    }
}

/// Apply a route: fused into a single composite morphism when
/// composition succeeds (one traversal, identical result —
/// spec: "Composition"), falling back to staged application in
/// the one inexpressible corner (a dissolve heading unmapped by
/// the next hop).
pub fn apply_route(doc: &Document, route: &[Morph]) -> Result<Document> {
    let Some(first) = route.first() else {
        return Err(Error::new(ErrorKind::InvalidMorph(
            "empty morphism route".to_string(),
        )));
    };
    let mut fused = first.clone();
    for next in &route[1..] {
        match compose(&fused, next) {
            Ok(f) => fused = f,
            Err(_) => return apply_route_staged(doc, route),
        }
    }
    apply(doc, &fused)
}

/// Apply a route in stages; each intermediate result is a full
/// document of its dialektos. The reference semantics fusion
/// must agree with.
pub fn apply_route_staged(doc: &Document, route: &[Morph]) -> Result<Document> {
    let mut current = None;
    for morph in route {
        let input = current.as_ref().unwrap_or(doc);
        current = Some(apply(input, morph)?);
    }
    current.ok_or_else(|| Error::new(ErrorKind::InvalidMorph("empty morphism route".to_string())))
}

/// Compose two morphisms symbolically: chase each source symbol
/// through `f`'s map, then `g`'s. The action algebra is closed
/// under this composition, so the result is an ordinary
/// [`Morph`] (serializable as a `.hom` file via
/// [`serialize_hom`]). Applying the composite equals staged
/// application. The one inexpressible case is an error: a
/// heading emitted by an `f` dissolve that `g` leaves unmapped.
pub fn compose(f: &Morph, g: &Morph) -> Result<Morph> {
    if f.target != g.source {
        return Err(Error::new(ErrorKind::InvalidMorph(format!(
            "cannot compose {}=>{} with {}=>{}",
            f.source, f.target, g.source, g.target
        ))));
    }
    // The fusion bound: grouping parses sibling sequences, so it
    // fuses only across a factor that leaves them undisturbed.
    // Anything but a pure rename table in `f` can move, create,
    // or delete `g`'s triggers; the composite has no normal form
    // and staged application is the semantics (spec:
    // "Composition").
    if !g.groups.is_empty() && !f.is_rename_only() {
        return Err(Error::new(ErrorKind::InvalidMorph(format!(
            "cannot fuse {}=>{} with {}=>{}: the second factor \
             groups and the first is not a pure rename table; \
             apply the route staged",
            f.source, f.target, g.source, g.target
        ))));
    }
    let mut map: HashMap<String, Action> = HashMap::new();
    for (symbol, action) in &f.map {
        let composite = match action {
            Action::Drop => Some(Action::Drop),
            Action::Dissolve(Lemma::Discard) => Some(Action::Dissolve(Lemma::Discard)),
            // The plain paragraph is core; g passes it through.
            Action::Dissolve(Lemma::Plain) => Some(Action::Dissolve(Lemma::Plain)),
            Action::Dissolve(Lemma::Heading {
                symbol: heading,
                genoses,
            }) => match g.map.get(heading) {
                Some(Action::Rename { to, genoses: more }) => {
                    let mut genoses = genoses.clone();
                    genoses.extend(more.iter().cloned());
                    Some(Action::Dissolve(Lemma::Heading {
                        symbol: to.clone(),
                        genoses,
                    }))
                }
                // The heading endo unwraps: the lemma becomes a
                // plain paragraph.
                Some(Action::Dissolve(Lemma::Discard)) => Some(Action::Dissolve(Lemma::Plain)),
                // The heading drops with the lemma inside it.
                Some(Action::Drop) => Some(Action::Dissolve(Lemma::Discard)),
                Some(Action::Dissolve(_)) => {
                    unreachable!("a dissolve of an endo is validated to discard")
                }
                None => {
                    return Err(Error::new(ErrorKind::InvalidMorph(format!(
                        "cannot compose: heading `{heading}` has no mapping in {}=>{}",
                        g.source, g.target
                    ))));
                }
            },
            Action::Rename { to, genoses } => match g.map.get(to) {
                Some(Action::Rename {
                    to: onward,
                    genoses: more,
                }) => {
                    let mut genoses = genoses.clone();
                    genoses.extend(more.iter().cloned());
                    Some(Action::Rename {
                        to: onward.clone(),
                        genoses,
                    })
                }
                Some(Action::Drop) => Some(Action::Drop),
                Some(Action::Dissolve(d)) => Some(Action::Dissolve(d.clone())),
                // Unmapped in g: unmapped in the composite (the
                // same documents fail, naming the source symbol).
                None => None,
            },
        };
        if let Some(composite) = composite {
            map.insert(symbol.clone(), composite);
        }
    }

    // Group rules: at most one side carries them (the fusion
    // bound above rejects the mixed case). `f`'s chase their
    // wrapper through `g`'s table, demoting the disposition down
    // the family; `g`'s pull back through `f`'s renames.
    let mut groups: Vec<GroupRule> = Vec::new();
    let genos_concat = |a: &[String], b: &[String]| -> Vec<String> {
        let mut out = a.to_vec();
        out.extend(b.iter().cloned());
        out
    };
    let lemma_supported = |symbol: &String| -> bool {
        !matches!(
            g.target_dial.sims.get(symbol).map(|d| &d.form),
            Some(SimForm::Para {
                lemma: Optionality::Unsupported,
                ..
            })
        )
    };
    for rule in &f.groups {
        let disposition = match &rule.disposition {
            GroupDisposition::Wrap {
                symbol,
                genoses,
                lemma,
            } => match g.map.get(symbol) {
                Some(Action::Rename { to, genoses: more }) => GroupDisposition::Wrap {
                    symbol: to.clone(),
                    genoses: genos_concat(genoses, more),
                    // A component-dropping rename that loses the
                    // lemma demotes the wrap to lemma-discarding.
                    lemma: *lemma && lemma_supported(to),
                },
                Some(Action::Drop) => GroupDisposition::DropRegion,
                Some(Action::Dissolve(Lemma::Discard)) => GroupDisposition::DeleteTrigger,
                Some(Action::Dissolve(Lemma::Heading {
                    symbol: heading,
                    genoses: more,
                })) => {
                    if *lemma {
                        GroupDisposition::Relabel {
                            symbol: heading.clone(),
                            genoses: genos_concat(genoses, more),
                        }
                    } else {
                        // An absent lemma emits nothing.
                        GroupDisposition::DeleteTrigger
                    }
                }
                Some(Action::Dissolve(Lemma::Plain)) => {
                    if *lemma {
                        // The wrapper dissolves with the trigger
                        // content as a plain paragraph: deplain.
                        GroupDisposition::Deplain
                    } else {
                        GroupDisposition::DeleteTrigger
                    }
                }
                None => {
                    return Err(Error::new(ErrorKind::InvalidMorph(format!(
                        "cannot compose: group wrapper `{symbol}` has no mapping in {}=>{}",
                        g.source, g.target
                    ))));
                }
            },
            // Deplain emits core forms only; g passes them
            // through.
            GroupDisposition::Deplain => GroupDisposition::Deplain,
            GroupDisposition::DropRegion => GroupDisposition::DropRegion,
            GroupDisposition::Relabel { symbol, genoses } => match g.map.get(symbol) {
                Some(Action::Rename { to, genoses: more }) => GroupDisposition::Relabel {
                    symbol: to.clone(),
                    genoses: genos_concat(genoses, more),
                },
                // Dropping the solo endo empties its paragraph,
                // which vanishes with it.
                Some(Action::Drop) => GroupDisposition::DeleteTrigger,
                // Unwrapping it strips the markup and leaves the
                // content as a plain paragraph: deplain.
                Some(Action::Dissolve(Lemma::Discard)) => GroupDisposition::Deplain,
                Some(Action::Dissolve(_)) => {
                    unreachable!("a dissolve of an endo is validated to discard")
                }
                None => {
                    return Err(Error::new(ErrorKind::InvalidMorph(format!(
                        "cannot compose: relabel target `{symbol}` has no mapping in {}=>{}",
                        g.source, g.target
                    ))));
                }
            },
            GroupDisposition::DeleteTrigger => GroupDisposition::DeleteTrigger,
        };
        groups.push(GroupRule {
            trigger: rule.trigger.clone(),
            rank: rule.rank,
            disposition,
        });
    }
    for rule in &g.groups {
        let mut pulled = false;
        for (symbol, action) in &f.map {
            let Action::Rename { to, genoses: gen_f } = action else {
                unreachable!("bounded above to a pure rename table");
            };
            if to != &rule.trigger {
                continue;
            }
            // A non-injective rename onto a trigger pulls back
            // peer triggers sharing one rank — a shape the
            // serialized normal form cannot express. No normal
            // form; staged application covers the route.
            if pulled {
                return Err(Error::new(ErrorKind::InvalidMorph(format!(
                    "cannot fuse {}=>{} with {}=>{}: two source sims rename \
                     onto the group trigger `{}`; apply the route staged",
                    f.source, f.target, g.source, g.target, rule.trigger
                ))));
            }
            pulled = true;
            let disposition = match &rule.disposition {
                GroupDisposition::Wrap {
                    symbol: wrap,
                    genoses,
                    lemma,
                } => GroupDisposition::Wrap {
                    symbol: wrap.clone(),
                    genoses: genos_concat(gen_f, genoses),
                    lemma: *lemma,
                },
                GroupDisposition::Relabel {
                    symbol: relabel,
                    genoses,
                } => GroupDisposition::Relabel {
                    symbol: relabel.clone(),
                    genoses: genos_concat(gen_f, genoses),
                },
                other => other.clone(),
            };
            groups.push(GroupRule {
                trigger: symbol.clone(),
                rank: rule.rank,
                disposition,
            });
        }
    }
    groups.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.trigger.cmp(&b.trigger)));

    let hops = |m: &Morph| -> Vec<String> {
        if m.via.is_empty() {
            vec![format!("{}=>{}", m.source, m.target)]
        } else {
            m.via.clone()
        }
    };
    let mut via = hops(f);
    via.extend(hops(g));

    Ok(Morph {
        source: f.source.clone(),
        target: g.target.clone(),
        map,
        groups,
        via,
        source_dial: f.source_dial.clone(),
        target_dial: g.target_dial.clone(),
    })
}

/// Serialize a morphism as a `.hom` file in normal form: group
/// rules in rank order (order is nesting semantics), then table
/// rules sorted by source symbol, implicit identities omitted.
/// Two morphisms are equal iff their normal forms are byte-equal;
/// `@=via` provenance lines are metadata, excluded from semantic
/// comparison.
pub fn serialize_hom(m: &Morph) -> String {
    serialize_hom_inner(m, true)
}

/// The semantic normal form: [`serialize_hom`] without the
/// `@=via` provenance lines. This is the equality and coherence
/// surface — two morphisms are equal iff their normal forms are
/// byte-equal, regardless of how they were derived.
pub fn normal_form(m: &Morph) -> String {
    serialize_hom_inner(m, false)
}

fn serialize_hom_inner(m: &Morph, with_via: bool) -> String {
    let mut implicit: HashMap<String, Action> = HashMap::new();
    add_implicit_identities(&mut implicit, &m.source_dial, &m.target_dial, true);
    let mut out = format!("@@@!atrep-hom\n@={}=>{}\n", m.source, m.target);
    if with_via {
        for hop in &m.via {
            out.push_str(&format!("@=via {hop}\n"));
        }
    }
    let genos_suffix =
        |genoses: &[String]| genoses.iter().map(|g| format!(" .{g}")).collect::<String>();
    let mut rules = String::new();
    for rule in &m.groups {
        let trigger = &rule.trigger;
        match &rule.disposition {
            GroupDisposition::Wrap {
                symbol,
                genoses,
                lemma,
            } => {
                let spelling = if *lemma { "@>#" } else { "@><" };
                rules.push_str(&format!(
                    "{spelling} {trigger} {symbol}{}\n",
                    genos_suffix(genoses)
                ));
            }
            GroupDisposition::Deplain => rules.push_str(&format!("@># {trigger}\n")),
            GroupDisposition::DropRegion => rules.push_str(&format!("@>- {trigger}\n")),
            GroupDisposition::Relabel { symbol, genoses } => {
                rules.push_str(&format!(
                    "@>: {trigger} {symbol}{}\n",
                    genos_suffix(genoses)
                ));
            }
            GroupDisposition::DeleteTrigger => rules.push_str(&format!("@>: {trigger}\n")),
        }
    }
    let mut symbols: Vec<&String> = m.map.keys().collect();
    symbols.sort();
    for symbol in symbols {
        let action = &m.map[symbol];
        if implicit.get(symbol) == Some(action) {
            continue;
        }
        match action {
            Action::Rename { to, genoses } => {
                rules.push_str(&format!("@:: {symbol} {to}{}\n", genos_suffix(genoses)));
            }
            Action::Drop => rules.push_str(&format!("@-- {symbol}\n")),
            Action::Dissolve(Lemma::Discard) => rules.push_str(&format!("@<< {symbol}\n")),
            Action::Dissolve(Lemma::Plain) => rules.push_str(&format!("@<# {symbol}\n")),
            Action::Dissolve(Lemma::Heading {
                symbol: heading,
                genoses,
            }) => {
                rules.push_str(&format!(
                    "@<# {symbol} {heading}{}\n",
                    genos_suffix(genoses)
                ));
            }
        }
    }
    if !rules.is_empty() {
        out.push('\n');
        out.push_str(&rules);
    }
    out
}

/// A direct resolution attempt: `Ok(None)` when nothing resolves,
/// errors only for actually-malformed definitions.
fn try_direct(ctx: &dyn Source, source: &str, target: &str) -> Result<Option<Morph>> {
    match resolve_morph_from(ctx, source, target) {
        Ok(m) => Ok(Some(m)),
        Err(e) if matches!(e.kind, ErrorKind::UnresolvableMorph(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// The dialektoi known to the resolution context: local
/// definition files plus the standard library.
fn known_dialektoi(ctx: &dyn Source, source: &str, target: &str) -> Vec<String> {
    let mut ids: Vec<String> = vec![source.to_string(), target.to_string()];
    for name in ctx.names() {
        if let Some(stem) = name
            .strip_suffix(".lektos")
            .or_else(|| name.strip_suffix(".dia"))
            && !stem.is_empty()
            && !stem.contains('.')
        {
            ids.push(stem.to_string());
        }
    }
    for (id, _) in dialektos::std_dialektos_ids() {
        ids.push(id.to_string());
    }
    ids.sort();
    ids.dedup();
    ids
}

fn resolve_via(ctx: &dyn Source, source: &str, target: &str) -> Result<Vec<Morph>> {
    let nodes = known_dialektoi(ctx, source, target);
    // Breadth-first over directly resolvable morphisms, tracking
    // every shortest path (the graphs are small).
    let mut paths: Vec<Vec<String>> = vec![vec![source.to_string()]];
    let mut visited_depth: HashMap<String, usize> = HashMap::new();
    visited_depth.insert(source.to_string(), 0);
    for depth in 1..=nodes.len() {
        let mut next: Vec<Vec<String>> = Vec::new();
        let mut arrived: Vec<Vec<String>> = Vec::new();
        for path in &paths {
            let last = path.last().expect("non-empty path");
            for node in &nodes {
                if path.contains(node) {
                    continue;
                }
                // Do not pass beyond a node already reached
                // earlier (only equal-depth arrivals matter for
                // ambiguity).
                if visited_depth.get(node).is_some_and(|&d| d < depth) {
                    continue;
                }
                if try_direct(ctx, last, node)?.is_none() {
                    continue;
                }
                let mut extended = path.clone();
                extended.push(node.clone());
                visited_depth.entry(node.clone()).or_insert(depth);
                if node == target {
                    arrived.push(extended);
                } else {
                    next.push(extended);
                }
            }
        }
        if !arrived.is_empty() {
            if arrived.len() > 1 {
                let routes: Vec<String> = arrived.iter().map(|p| p.join(" => ")).collect();
                return Err(Error::new(ErrorKind::AmbiguousMorphRoute(
                    routes.join(" | "),
                )));
            }
            let route = &arrived[0];
            let mut morphs = Vec::new();
            for pair in route.windows(2) {
                morphs.push(
                    try_direct(ctx, &pair[0], &pair[1])?.expect("edge existed during discovery"),
                );
            }
            return Ok(morphs);
        }
        paths = next;
        if paths.is_empty() {
            break;
        }
    }
    Err(Error::new(ErrorKind::UnresolvableMorph(format!(
        "{source}=>{target}"
    ))))
}

fn std_morph_source(name: &str) -> Option<String> {
    STD_MORPHS
        .iter()
        .find_map(|(a, b, ext, src)| (format!("{a}.{b}.{ext}") == name).then(|| src.to_string()))
}

/// The embedding C => P derived from C's lineage: identity for
/// inherited symbols, inverted aliases for aliased imports.
/// Partial: C's sims that do not come from P stay unmapped.
fn derive_embedding(src: &Dialektos, tgt: &Dialektos) -> Option<Morph> {
    let ops: Vec<_> = src
        .lineage
        .iter()
        .filter(|op| op.source == tgt.id)
        .collect();
    if ops.is_empty() {
        return None;
    }
    let mut map: HashMap<String, Action> = HashMap::new();
    for op in ops {
        match &op.kind {
            InheritKind::Full => {
                for symbol in tgt.sims.keys() {
                    if src.sims.contains_key(symbol) {
                        map.insert(symbol.clone(), Action::rename(symbol));
                    }
                }
            }
            InheritKind::Exclude(excluded) => {
                for symbol in tgt.sims.keys() {
                    if !excluded.contains(symbol) && src.sims.contains_key(symbol) {
                        map.insert(symbol.clone(), Action::rename(symbol));
                    }
                }
            }
            InheritKind::ImportList(symbols) => {
                for symbol in symbols {
                    map.insert(symbol.clone(), Action::rename(symbol));
                }
            }
            InheritKind::Import { symbol, alias } => {
                let local = alias.clone().unwrap_or_else(|| symbol.clone());
                map.insert(local, Action::rename(symbol));
            }
        }
    }
    Some(Morph {
        source: src.id.clone(),
        target: tgt.id.clone(),
        map,
        groups: Vec::new(),
        via: Vec::new(),
        source_dial: src.clone(),
        target_dial: tgt.clone(),
    })
}

// ---------------------------------------------------------------
// Definition parsing
// ---------------------------------------------------------------

fn parse_morph_source(
    source_text: &str,
    path: &Path,
    iso: bool,
    reversed: bool,
    src_dial: &Dialektos,
    tgt_dial: &Dialektos,
) -> Result<Morph> {
    let invalid = |msg: String| {
        Error::new(ErrorKind::InvalidMorph(format!(
            "{}: {msg}",
            path.display()
        )))
    };

    let lines: Vec<&str> = source_text.lines().collect();
    let mut i = 0;
    if i < lines.len() && lines[i].starts_with("#!") {
        i += 1;
    }
    let expected_decl = if iso {
        "@@@!atrep-iso"
    } else {
        "@@@!atrep-hom"
    };
    match lines.get(i).map(|l| l.trim()) {
        Some(l) if l == expected_decl => {}
        _ => return Err(invalid(format!("missing `{expected_decl}` declaration"))),
    }
    i += 1;

    // The declared operands read in file order; a reversed .iso
    // swaps them relative to the requested direction.
    let (file_a, file_b) = if reversed {
        (&tgt_dial.id, &src_dial.id)
    } else {
        (&src_dial.id, &tgt_dial.id)
    };
    let arrow = if iso { "<=>" } else { "=>" };

    let mut declared = false;
    let mut explicit: Vec<(String, Action)> = Vec::new();
    let mut group_lines: Vec<(String, GroupDisposition)> = Vec::new();
    let mut via: Vec<String> = Vec::new();

    while i < lines.len() {
        let line = lines[i].trim();
        i += 1;
        if line.is_empty() || line.starts_with("@@/") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("@=via ") {
            let hop = rest.trim();
            if !hop.contains("=>") {
                return Err(invalid(format!(
                    "malformed provenance hop `{line}` (expected `a=>b`)"
                )));
            }
            via.push(hop.to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("@=") {
            if declared {
                return Err(invalid("duplicate morphism declaration".into()));
            }
            let Some((a, b)) = rest.split_once(arrow) else {
                return Err(invalid(format!(
                    "malformed morphism declaration `{line}` (expected `{arrow}`)"
                )));
            };
            if a.trim() != file_a.as_str() || b.trim() != file_b.as_str() {
                return Err(invalid(format!(
                    "declaration `{line}` does not match `{file_a}.{file_b}`"
                )));
            }
            declared = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("@::") {
            let mut parts = rest.split_whitespace();
            let (Some(a), Some(b)) = (parts.next(), parts.next()) else {
                return Err(invalid(format!("malformed rename `{line}`")));
            };
            let mut genoses: Vec<String> = Vec::new();
            for extra in parts {
                let Some(genos) = extra.strip_prefix('.') else {
                    return Err(invalid(format!("malformed rename `{line}`")));
                };
                if !crate::sigil::is_valid_genos(genos) {
                    return Err(invalid(format!("invalid genos `{extra}` in `{line}`")));
                }
                genoses.push(genos.to_string());
            }
            if iso && !genoses.is_empty() {
                return Err(invalid(
                    "genos-adding renames are not allowed in an .iso".into(),
                ));
            }
            let (from, to) = if reversed { (b, a) } else { (a, b) };
            explicit.push((
                from.to_string(),
                Action::Rename {
                    to: to.to_string(),
                    genoses,
                },
            ));
            continue;
        }
        if let Some(rest) = line.strip_prefix("@<#") {
            if iso {
                return Err(invalid("extract rules are not allowed in an .iso".into()));
            }
            let mut parts = rest.split_whitespace();
            let Some(sym) = parts.next() else {
                return Err(invalid(format!("malformed extract `{line}`")));
            };
            let heading = parts.next();
            let mut genoses: Vec<String> = Vec::new();
            for extra in parts {
                let Some(genos) = extra.strip_prefix('.') else {
                    return Err(invalid(format!("malformed extract `{line}`")));
                };
                if !crate::sigil::is_valid_genos(genos) {
                    return Err(invalid(format!("invalid genos `{extra}` in `{line}`")));
                }
                genoses.push(genos.to_string());
            }
            let lemma = match heading {
                Some(h) => Lemma::Heading {
                    symbol: h.to_string(),
                    genoses,
                },
                None if genoses.is_empty() => Lemma::Plain,
                None => unreachable!("genoses only parse after a heading operand"),
            };
            explicit.push((sym.to_string(), Action::Dissolve(lemma)));
            continue;
        }
        if let Some(rest) = line.strip_prefix("@<<") {
            if iso {
                return Err(invalid("unwrap rules are not allowed in an .iso".into()));
            }
            explicit.push((rest.trim().to_string(), Action::Dissolve(Lemma::Discard)));
            continue;
        }
        if let Some(rest) = line.strip_prefix("@--") {
            if iso {
                return Err(invalid("drop rules are not allowed in an .iso".into()));
            }
            explicit.push((rest.trim().to_string(), Action::Drop));
            continue;
        }
        if let Some((spelling, rest)) = ["@>#", "@><", "@>-", "@>:"]
            .iter()
            .find_map(|p| line.strip_prefix(p).map(|rest| (*p, rest)))
        {
            if iso {
                return Err(invalid("group rules are not allowed in an .iso".into()));
            }
            let mut parts = rest.split_whitespace();
            let Some(trigger) = parts.next() else {
                return Err(invalid(format!("malformed group rule `{line}`")));
            };
            let operand = parts.next();
            let mut genoses: Vec<String> = Vec::new();
            for extra in parts {
                let Some(genos) = extra.strip_prefix('.') else {
                    return Err(invalid(format!("malformed group rule `{line}`")));
                };
                if !crate::sigil::is_valid_genos(genos) {
                    return Err(invalid(format!("invalid genos `{extra}` in `{line}`")));
                }
                genoses.push(genos.to_string());
            }
            let disposition = match (spelling, operand) {
                ("@>#" | "@><", Some(symbol)) => GroupDisposition::Wrap {
                    symbol: symbol.to_string(),
                    genoses,
                    lemma: spelling == "@>#",
                },
                // Operand-less @># is deplain: "becomes a plain
                // paragraph" in both families (compare @<#).
                ("@>#", None) if genoses.is_empty() => GroupDisposition::Deplain,
                ("@>#", None) => {
                    return Err(invalid(format!(
                        "deplain `{line}` takes no genoses (a plain \
                         paragraph carries no annotations)"
                    )));
                }
                ("@><", None) => {
                    return Err(invalid(format!(
                        "group rule `{line}` needs a wrapper sim operand"
                    )));
                }
                ("@>-", None) if genoses.is_empty() => GroupDisposition::DropRegion,
                ("@>-", _) => {
                    return Err(invalid(format!(
                        "region drop `{line}` takes only a trigger operand"
                    )));
                }
                ("@>:", Some(symbol)) => GroupDisposition::Relabel {
                    symbol: symbol.to_string(),
                    genoses,
                },
                ("@>:", None) if genoses.is_empty() => GroupDisposition::DeleteTrigger,
                ("@>:", None) => {
                    return Err(invalid(format!(
                        "trigger deletion `{line}` takes no genoses"
                    )));
                }
                _ => unreachable!("spelling set is fixed"),
            };
            group_lines.push((trigger.to_string(), disposition));
            continue;
        }
        return Err(invalid(format!("unexpected line: `{line}`")));
    }
    if !declared {
        return Err(invalid("missing morphism declaration".into()));
    }

    // Validate explicit rules and build the effective map.
    let mut map: HashMap<String, Action> = HashMap::new();
    for (symbol, action) in explicit {
        let Some(src_def) = src_dial.sims.get(&symbol) else {
            return Err(invalid(format!(
                "`{symbol}` is not defined in dialektos `{}`",
                src_dial.id
            )));
        };
        match &action {
            Action::Rename { to, .. } => {
                let Some(tgt_def) = tgt_dial.sims.get(to) else {
                    return Err(invalid(format!(
                        "`{to}` is not defined in dialektos `{}`",
                        tgt_dial.id
                    )));
                };
                if let Err(msg) = forms_compatible(&src_def.form, &tgt_def.form, !iso) {
                    return Err(invalid(format!(
                        "rename `{symbol}` -> `{to}` is form-incompatible: {msg}"
                    )));
                }
            }
            Action::Dissolve(Lemma::Plain | Lemma::Heading { .. }) => {
                match &src_def.form {
                    SimForm::Para { stichoi: false, .. } => {}
                    _ => {
                        return Err(invalid(format!(
                            "extract of `{symbol}`: source must be an ordinary para-simmere"
                        )));
                    }
                }
                if let Action::Dissolve(Lemma::Heading {
                    symbol: heading, ..
                }) = &action
                {
                    match tgt_dial.sims.get(heading).map(|d| &d.form) {
                        Some(SimForm::Endo) => {}
                        _ => {
                            return Err(invalid(format!(
                                "extract target `{heading}` is not an endo-simmere of `{}`",
                                tgt_dial.id
                            )));
                        }
                    }
                }
            }
            Action::Dissolve(Lemma::Discard) => match &src_def.form {
                SimForm::Mono { .. } => {
                    return Err(invalid(format!("monosim `{symbol}` cannot be unwrapped")));
                }
                SimForm::Para { stichoi: true, .. } => {
                    return Err(invalid(format!(
                        "stichoi-form sim `{symbol}` cannot be unwrapped"
                    )));
                }
                _ => {}
            },
            Action::Drop => {}
        }
        if map.insert(symbol.clone(), action).is_some() {
            return Err(invalid(format!("duplicate rule for `{symbol}`")));
        }
    }
    add_implicit_identities(&mut map, src_dial, tgt_dial, !iso);

    // Validate group rules and assign ranks (file order is the
    // nesting order, outermost first).
    let mut groups: Vec<GroupRule> = Vec::new();
    for (rank, (trigger, disposition)) in group_lines.into_iter().enumerate() {
        match src_dial.sims.get(&trigger).map(|d| &d.form) {
            Some(SimForm::Endo) => {}
            Some(_) => {
                return Err(invalid(format!(
                    "group trigger `{trigger}` is not an endo-simmere of `{}`",
                    src_dial.id
                )));
            }
            None => {
                return Err(invalid(format!(
                    "`{trigger}` is not defined in dialektos `{}`",
                    src_dial.id
                )));
            }
        }
        if groups.iter().any(|g| g.trigger == trigger) {
            return Err(invalid(format!("duplicate group rule for `{trigger}`")));
        }
        match &disposition {
            GroupDisposition::Wrap {
                symbol,
                lemma: use_lemma,
                ..
            } => match tgt_dial.sims.get(symbol).map(|d| &d.form) {
                Some(SimForm::Para {
                    stichoi: false,
                    taxis,
                    lemma,
                    hypograph,
                    ..
                }) => {
                    if *hypograph == Optionality::Required {
                        return Err(invalid(format!(
                            "group target `{symbol}` requires a hypograph grouping cannot supply"
                        )));
                    }
                    if *taxis == Optionality::Required {
                        return Err(invalid(format!(
                            "group target `{symbol}` requires a taxis grouping cannot supply"
                        )));
                    }
                    if *use_lemma && *lemma == Optionality::Unsupported {
                        return Err(invalid(format!(
                            "group target `{symbol}` cannot carry a lemma"
                        )));
                    }
                    if !*use_lemma && *lemma == Optionality::Required {
                        return Err(invalid(format!(
                            "group target `{symbol}` requires a lemma the rule discards"
                        )));
                    }
                }
                Some(_) => {
                    return Err(invalid(format!(
                        "group target `{symbol}` is not an ordinary para-simmere of `{}`",
                        tgt_dial.id
                    )));
                }
                None => {
                    return Err(invalid(format!(
                        "`{symbol}` is not defined in dialektos `{}`",
                        tgt_dial.id
                    )));
                }
            },
            GroupDisposition::Relabel { symbol, .. } => {
                match tgt_dial.sims.get(symbol).map(|d| &d.form) {
                    Some(SimForm::Endo) => {}
                    _ => {
                        return Err(invalid(format!(
                            "relabel target `{symbol}` is not an endo-simmere of `{}`",
                            tgt_dial.id
                        )));
                    }
                }
            }
            GroupDisposition::Deplain
            | GroupDisposition::DropRegion
            | GroupDisposition::DeleteTrigger => {}
        }
        groups.push(GroupRule {
            trigger,
            rank,
            disposition,
        });
    }

    if iso {
        // Bijectivity over the effective map.
        let mut seen: HashMap<&String, &String> = HashMap::new();
        for (from, action) in &map {
            let Action::Rename { to, .. } = action else {
                unreachable!("iso rules are renames only");
            };
            if let Some(other) = seen.insert(to, from) {
                return Err(invalid(format!(
                    "not bijective: both `{other}` and `{from}` map to `{to}`"
                )));
            }
        }
    }

    // Totality: an explicit morphism accounts for every sim of
    // its source dialektos — table rule, implicit self-map, or
    // group trigger; `@--` marks deliberate drops. Checked here,
    // with no documents present (spec: "Static Validation").
    let mut missing: Vec<String> = src_dial
        .sims
        .keys()
        .filter(|s| !map.contains_key(*s) && !groups.iter().any(|g| &g.trigger == *s))
        .cloned()
        .collect();
    if !missing.is_empty() {
        missing.sort();
        return Err(Error::new(ErrorKind::MorphIncomplete(format!(
            "{}: {}",
            path.display(),
            missing.join(", ")
        ))));
    }

    Ok(Morph {
        source: src_dial.id.clone(),
        target: tgt_dial.id.clone(),
        map,
        groups,
        via,
        source_dial: src_dial.clone(),
        target_dial: tgt_dial.clone(),
    })
}

/// Same-symbol sims with form-compatible definitions map to
/// themselves unless an explicit rule says otherwise.
fn add_implicit_identities(
    map: &mut HashMap<String, Action>,
    src: &Dialektos,
    tgt: &Dialektos,
    lossy: bool,
) {
    for (symbol, src_def) in &src.sims {
        if map.contains_key(symbol) {
            continue;
        }
        if let Some(tgt_def) = tgt.sims.get(symbol)
            && forms_compatible(&src_def.form, &tgt_def.form, lossy).is_ok()
        {
            map.insert(symbol.clone(), Action::rename(symbol));
        }
    }
}

/// Every component the source may carry must be supported by the
/// target; every component the target requires must be guaranteed
/// by the source.
/// `lossy` (homomorphisms): a component the target does not
/// support is dropped at application rather than refused —
/// recorded loss, like the lemma of an unwrap. Isomorphisms
/// and derived embeddings stay strict.
fn forms_compatible(src: &SimForm, tgt: &SimForm, lossy: bool) -> std::result::Result<(), String> {
    let component = |name: &str,
                     s: Optionality,
                     t: Optionality|
     -> std::result::Result<(), String> {
        match (s, t) {
            (Optionality::Required | Optionality::Optional, Optionality::Unsupported) if !lossy => {
                Err(format!(
                    "source may carry a {name} the target does not support"
                ))
            }
            (Optionality::Optional | Optionality::Unsupported, Optionality::Required)
                if s != Optionality::Required =>
            {
                Err(format!("target requires a {name} the source may omit"))
            }
            _ => Ok(()),
        }
    };
    match (src, tgt) {
        (SimForm::Endo, SimForm::Endo) => Ok(()),
        (SimForm::Mono { .. }, SimForm::Mono { .. }) => Ok(()),
        (
            SimForm::Para {
                autonym: _,
                taxis: st,
                lemma: sl,
                hypograph: sh,
                stichoi: ss,
            },
            SimForm::Para {
                autonym: _,
                taxis: tt,
                lemma: tl,
                hypograph: th,
                stichoi: ts,
            },
        ) => {
            if ss != ts {
                return Err("stichoi-form flag differs".to_string());
            }
            component("taxis", *st, *tt)?;
            component("lemma", *sl, *tl)?;
            component("hypograph", *sh, *th)
        }
        _ => Err("different form kinds".to_string()),
    }
}

// ---------------------------------------------------------------
// Application
// ---------------------------------------------------------------

/// Apply the morphism to a kanon, producing a kanon of the target
/// dialektos. The output is re-validated (taxis runs, deixis
/// targets) against the target definitions.
pub fn apply(doc: &Document, morph: &Morph) -> Result<Document> {
    if doc.dialect_id != morph.source {
        return Err(Error::new(ErrorKind::InvalidMorph(format!(
            "document declares `{}` but the morphism is from `{}`",
            doc.dialect_id, morph.source
        ))));
    }
    // Pre-scan: every dialektos-defined symbol in the document
    // must have a mapping (or, for solo trigger paragraphs and
    // deixes to triggers, a group rule).
    let mut unmapped: Vec<String> = Vec::new();
    scan_unmapped_blocks(&doc.blocks, morph, &mut unmapped);
    if !unmapped.is_empty() {
        unmapped.sort();
        unmapped.dedup();
        return Err(Error::new(ErrorKind::MorphUnmapped(unmapped.join(", "))));
    }

    let mut blocks = doc.blocks.clone();
    transform_blocks(&mut blocks, morph);

    let mut out = Document {
        dialect_id: morph.target.clone(),
        dialect_version: None,
        blocks,
    };
    // Re-validate the result as a kanon of the target dialektos.
    kanonizo::validate_deixes(&out.blocks)?;
    kanonizo::evaluate_taxis(&mut out.blocks)?;
    Ok(out)
}

fn scan_unmapped_blocks(blocks: &[Block], morph: &Morph, out: &mut Vec<String>) {
    let check = |symbol: &String, out: &mut Vec<String>| {
        if !morph.map.contains_key(symbol) {
            out.push(symbol.clone());
        }
    };
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => {
                // A solo trigger paragraph is consumed by its
                // group rule; only its content needs mappings.
                if let [
                    Inline::Endo {
                        symbol, content, ..
                    },
                ] = inlines.as_slice()
                    && morph.group_for(symbol).is_some()
                {
                    scan_unmapped_inlines(content, morph, out);
                } else {
                    scan_unmapped_inlines(inlines, morph, out);
                }
            }
            Block::Para {
                symbol,
                lemma,
                children,
                hypograph,
                ..
            } => {
                check(symbol, out);
                scan_unmapped_inlines(lemma, morph, out);
                scan_unmapped_blocks(children, morph, out);
                scan_unmapped_inlines(hypograph, morph, out);
            }
            Block::Stichoi {
                symbol,
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                if let Some(symbol) = symbol {
                    check(symbol, out);
                }
                scan_unmapped_inlines(lemma, morph, out);
                for strophe in strophes {
                    for line in &strophe.0 {
                        scan_unmapped_inlines(line, morph, out);
                    }
                }
                scan_unmapped_inlines(hypograph, morph, out);
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                scan_unmapped_blocks(children, morph, out);
            }
            _ => {}
        }
    }
}

fn scan_unmapped_inlines(inlines: &[Inline], morph: &Morph, out: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::Endo {
                symbol, content, ..
            } => {
                if !morph.map.contains_key(symbol) {
                    out.push(symbol.clone());
                }
                scan_unmapped_inlines(content, morph, out);
            }
            // A deixis to a trigger follows the group rule.
            Inline::Deixis { symbol, .. }
                if !morph.map.contains_key(symbol) && morph.group_for(symbol).is_none() =>
            {
                out.push(symbol.clone());
            }
            Inline::Monosim { symbol, .. } if !morph.map.contains_key(symbol) => {
                out.push(symbol.clone());
            }
            Inline::EndoDiaphane { content, .. } => scan_unmapped_inlines(content, morph, out),
            _ => {}
        }
    }
}

/// Transform a block sequence: the grouping pass first (spec:
/// "Group Rules" — every sequence independently), then the rule
/// table. Wrappers and relabeled headings produced by grouping
/// are already target-dialektos nodes and are not subject to the
/// table.
fn transform_blocks(blocks: &mut Vec<Block>, morph: &Morph) {
    if morph.groups.is_empty() {
        transform_blocks_table(blocks, morph);
    } else {
        let taken = std::mem::take(blocks);
        *blocks = group_and_transform(taken, morph);
    }
}

/// The grouping stack machine, fused with the table: a trigger
/// closes every open region of equal or greater rank and opens
/// its own; every other sibling is table-transformed and routed
/// into the innermost open region.
fn group_and_transform(blocks: Vec<Block>, morph: &Morph) -> Vec<Block> {
    struct Open {
        rank: usize,
        kind: OpenKind,
        out: Vec<Block>,
    }
    enum OpenKind {
        Wrap {
            symbol: String,
            lemma: Vec<Inline>,
            ann: crate::dendron::Annotations,
        },
        DropRegion,
        /// Relabel / trigger deletion: the region delimits
        /// nesting but contributes no wrapper.
        Transparent,
    }
    fn close_one(stack: &mut Vec<Open>, result: &mut Vec<Block>, morph: &Morph) {
        let open = stack.pop().expect("close on an empty region stack");
        let parent = stack.last_mut().map(|o| &mut o.out).unwrap_or(result);
        match open.kind {
            OpenKind::Wrap { symbol, lemma, ann } => {
                let bracket_matching = morph
                    .target_dial
                    .sims
                    .get(&symbol)
                    .map(|d| d.bracket_matching)
                    .unwrap_or(true);
                parent.push(Block::Para {
                    symbol,
                    taxis: None,
                    lemma,
                    children: open.out,
                    hypograph: Vec::new(),
                    bracket_matching,
                    ann,
                });
            }
            OpenKind::DropRegion => {}
            OpenKind::Transparent => parent.extend(open.out),
        }
    }

    let mut result: Vec<Block> = Vec::new();
    let mut stack: Vec<Open> = Vec::new();
    for block in blocks {
        let rule = match &block {
            Block::Paragraph(inlines) => match inlines.as_slice() {
                [Inline::Endo { symbol, .. }] => morph.group_for(symbol).cloned(),
                _ => None,
            },
            _ => None,
        };
        let Some(rule) = rule else {
            let mut one = vec![block];
            transform_blocks_table(&mut one, morph);
            let parent = stack.last_mut().map(|o| &mut o.out).unwrap_or(&mut result);
            parent.extend(one);
            continue;
        };
        while stack.last().is_some_and(|o| o.rank >= rule.rank) {
            close_one(&mut stack, &mut result, morph);
        }
        let Block::Paragraph(mut inlines) = block else {
            unreachable!("trigger is a paragraph");
        };
        let Some(Inline::Endo {
            content, mut ann, ..
        }) = inlines.pop()
        else {
            unreachable!("trigger holds exactly one endo");
        };
        let mut content = content;
        transform_inlines(&mut content, morph);
        match rule.disposition {
            GroupDisposition::Wrap {
                symbol,
                genoses,
                lemma,
            } => {
                ann.genoses.extend(genoses);
                stack.push(Open {
                    rank: rule.rank,
                    kind: OpenKind::Wrap {
                        symbol,
                        lemma: if lemma { content } else { Vec::new() },
                        ann,
                    },
                    out: Vec::new(),
                });
            }
            GroupDisposition::Deplain => {
                if !content.is_empty() {
                    let parent = stack.last_mut().map(|o| &mut o.out).unwrap_or(&mut result);
                    parent.push(Block::Paragraph(content));
                }
                stack.push(Open {
                    rank: rule.rank,
                    kind: OpenKind::Transparent,
                    out: Vec::new(),
                });
            }
            GroupDisposition::DropRegion => {
                stack.push(Open {
                    rank: rule.rank,
                    kind: OpenKind::DropRegion,
                    out: Vec::new(),
                });
            }
            GroupDisposition::Relabel { symbol, genoses } => {
                ann.genoses.extend(genoses);
                let bracket_matching = morph
                    .target_dial
                    .sims
                    .get(&symbol)
                    .map(|d| d.bracket_matching)
                    .unwrap_or(true);
                let relabeled = Block::Paragraph(vec![Inline::Endo {
                    symbol,
                    content,
                    bracket_matching,
                    ann,
                }]);
                let parent = stack.last_mut().map(|o| &mut o.out).unwrap_or(&mut result);
                parent.push(relabeled);
                stack.push(Open {
                    rank: rule.rank,
                    kind: OpenKind::Transparent,
                    out: Vec::new(),
                });
            }
            GroupDisposition::DeleteTrigger => {
                stack.push(Open {
                    rank: rule.rank,
                    kind: OpenKind::Transparent,
                    out: Vec::new(),
                });
            }
        }
    }
    while !stack.is_empty() {
        close_one(&mut stack, &mut result, morph);
    }
    result
}

fn transform_blocks_table(blocks: &mut Vec<Block>, morph: &Morph) {
    let mut i = 0;
    while i < blocks.len() {
        // Determine the action for dialektos-defined blocks first.
        let action = match &blocks[i] {
            Block::Para { symbol, .. } => morph.map.get(symbol).cloned(),
            Block::Stichoi {
                symbol: Some(symbol),
                ..
            } => morph.map.get(symbol).cloned(),
            _ => None,
        };
        match action {
            Some(Action::Drop) => {
                blocks.remove(i);
                continue;
            }
            Some(Action::Dissolve(Lemma::Discard)) => {
                let Block::Para { children, .. } = &mut blocks[i] else {
                    unreachable!("unwrap is validated to para form");
                };
                let mut inner = std::mem::take(children);
                transform_blocks(&mut inner, morph);
                let n = inner.len();
                blocks.splice(i..=i, inner);
                i += n;
                continue;
            }
            Some(Action::Dissolve(disposition)) => {
                let Block::Para {
                    lemma, children, ..
                } = &mut blocks[i]
                else {
                    unreachable!("extract is validated to para form");
                };
                let mut lemma = std::mem::take(lemma);
                transform_inlines(&mut lemma, morph);
                let mut inner = std::mem::take(children);
                transform_blocks(&mut inner, morph);
                let mut spliced: Vec<Block> = Vec::new();
                if !lemma.is_empty() {
                    match &disposition {
                        Lemma::Plain => spliced.push(Block::Paragraph(lemma)),
                        Lemma::Heading { symbol, genoses } => {
                            spliced.push(Block::Paragraph(vec![Inline::Endo {
                                symbol: symbol.clone(),
                                content: lemma,
                                bracket_matching: morph
                                    .target_dial
                                    .sims
                                    .get(symbol)
                                    .map(|d| d.bracket_matching)
                                    .unwrap_or(true),
                                ann: crate::dendron::Annotations {
                                    onym: None,
                                    genoses: genoses.clone(),
                                },
                            }]));
                        }
                        Lemma::Discard => unreachable!("handled above"),
                    }
                }
                spliced.extend(inner);
                let n = spliced.len();
                blocks.splice(i..=i, spliced);
                i += n;
                continue;
            }
            Some(Action::Rename { to, genoses }) => match &mut blocks[i] {
                Block::Para {
                    symbol,
                    taxis,
                    lemma,
                    children,
                    hypograph,
                    bracket_matching,
                    ann,
                } => {
                    *symbol = to.clone();
                    let tgt_def = morph.target_dial.sims.get(&to);
                    *bracket_matching = tgt_def
                        .map(|d| d.bracket_matching)
                        .unwrap_or(*bracket_matching);
                    // A lossy rename drops the components the
                    // target does not support.
                    if let Some(SimForm::Para {
                        taxis: tt,
                        lemma: tl,
                        hypograph: th,
                        ..
                    }) = tgt_def.map(|d| &d.form)
                    {
                        if *tt == Optionality::Unsupported {
                            *taxis = None;
                        }
                        if *tl == Optionality::Unsupported {
                            lemma.clear();
                        }
                        if *th == Optionality::Unsupported {
                            hypograph.clear();
                        }
                    }
                    ann.genoses.extend(genoses.iter().cloned());
                    transform_inlines(lemma, morph);
                    transform_blocks(children, morph);
                    transform_inlines(hypograph, morph);
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
                    *symbol = Some(to.clone());
                    let tgt_def = morph.target_dial.sims.get(&to);
                    *bracket_matching = tgt_def
                        .map(|d| d.bracket_matching)
                        .unwrap_or(*bracket_matching);
                    if let Some(SimForm::Para {
                        taxis: tt,
                        lemma: tl,
                        hypograph: th,
                        ..
                    }) = tgt_def.map(|d| &d.form)
                    {
                        if *tt == Optionality::Unsupported {
                            *taxis = None;
                        }
                        if *tl == Optionality::Unsupported {
                            lemma.clear();
                        }
                        if *th == Optionality::Unsupported {
                            hypograph.clear();
                        }
                    }
                    ann.genoses.extend(genoses.iter().cloned());
                    transform_inlines(lemma, morph);
                    for strophe in strophes {
                        for line in &mut strophe.0 {
                            transform_inlines(line, morph);
                        }
                    }
                    transform_inlines(hypograph, morph);
                }
                _ => unreachable!(),
            },
            None => match &mut blocks[i] {
                Block::Paragraph(inlines) => {
                    transform_inlines(inlines, morph);
                    // A paragraph whose only content was dropped
                    // (e.g. a standalone monosim) vanishes with it.
                    if inlines.is_empty() {
                        blocks.remove(i);
                        continue;
                    }
                }
                Block::Stichoi {
                    lemma,
                    strophes,
                    hypograph,
                    ..
                } => {
                    transform_inlines(lemma, morph);
                    for strophe in strophes {
                        for line in &mut strophe.0 {
                            transform_inlines(line, morph);
                        }
                    }
                    transform_inlines(hypograph, morph);
                }
                Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                    transform_blocks(children, morph);
                }
                _ => {}
            },
        }
        i += 1;
    }
}

fn transform_inlines(inlines: &mut Vec<Inline>, morph: &Morph) {
    let mut i = 0;
    while i < inlines.len() {
        let action = match &inlines[i] {
            Inline::Endo { symbol, .. } | Inline::Monosim { symbol, .. } => {
                morph.map.get(symbol).cloned()
            }
            // A deixis to a trigger follows the group rule: to
            // the wrapper or relabeled heading it moved to, or
            // out of the document with a dropped region or
            // deleted trigger.
            Inline::Deixis { symbol, .. } => morph.map.get(symbol).cloned().or_else(|| {
                morph.group_for(symbol).map(|rule| match &rule.disposition {
                    GroupDisposition::Wrap { symbol, .. }
                    | GroupDisposition::Relabel { symbol, .. } => Action::rename(symbol),
                    // A deplained trigger loses its onym with
                    // the markup.
                    GroupDisposition::Deplain
                    | GroupDisposition::DropRegion
                    | GroupDisposition::DeleteTrigger => Action::Drop,
                })
            }),
            _ => None,
        };
        match action {
            Some(Action::Drop) => {
                inlines.remove(i);
                continue;
            }
            Some(Action::Dissolve(Lemma::Discard)) => match &mut inlines[i] {
                Inline::Endo { content, .. } => {
                    let mut inner = std::mem::take(content);
                    transform_inlines(&mut inner, morph);
                    let n = inner.len();
                    inlines.splice(i..=i, inner);
                    i += n;
                    continue;
                }
                // A deixis to an unwrapped sim loses its target.
                Inline::Deixis { .. } => {
                    inlines.remove(i);
                    continue;
                }
                _ => unreachable!("unwrap is validated to endo form"),
            },
            Some(Action::Dissolve(_)) => {
                // Lemma-emitting dissolves are validated to para
                // sims; an inline deixis pointing at a dissolved
                // sim loses its target (the wrapper's onym is
                // gone).
                if matches!(&inlines[i], Inline::Deixis { .. }) {
                    inlines.remove(i);
                    continue;
                }
                unreachable!("extract is validated to para form");
            }
            Some(Action::Rename { to, genoses }) => match &mut inlines[i] {
                Inline::Endo {
                    symbol,
                    content,
                    bracket_matching,
                    ann,
                } => {
                    *symbol = to.clone();
                    *bracket_matching = morph
                        .target_dial
                        .sims
                        .get(&to)
                        .map(|d| d.bracket_matching)
                        .unwrap_or(*bracket_matching);
                    ann.genoses.extend(genoses.iter().cloned());
                    transform_inlines(content, morph);
                }
                Inline::Monosim { symbol, ann, .. } => {
                    *symbol = to.clone();
                    ann.genoses.extend(genoses.iter().cloned());
                }
                Inline::Deixis { symbol, .. } => {
                    *symbol = to.clone();
                }
                _ => unreachable!(),
            },
            None => match &mut inlines[i] {
                Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                    transform_inlines(content, morph);
                }
                _ => {}
            },
        }
        i += 1;
    }
}
