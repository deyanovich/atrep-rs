//! Kanonizo: canonicalization of a deltos (`.atd`) into a kanon
//! (`.atk`) (spec: chapter "Kanonizo").
//!
//! Steps, in spec order: shebang removal and sigil canonicalization
//! (parser/serializer), Unicode NFC, transclusion expansion, onym
//! canonicalization, taxis evaluation, comment and unreferenced-
//! diaphane removal, paragraph collapsing and whitespace
//! normalization, media processing (local and remote).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::dendron::{self, Annotations, Block, Document, Inline, Taxis};
use crate::dialektos;
use crate::error::{Error, ErrorKind, Result};
use crate::fetch::{self, Fetcher};
use crate::parser;

/// One media resource bundled by kanonizo.
#[derive(Debug, Clone)]
pub struct MediaEntry {
    /// Canonical file name (`m1.svg`, ...).
    pub name: String,
    /// The parameter as written in the source document.
    pub source: String,
    /// SHA-256 of the file content (lowercase hex).
    pub sha256: String,
    /// The resource content (read locally or fetched remotely).
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct KanonResult {
    pub document: Document,
    /// Serialized canonical text.
    pub kanon: String,
    pub media: Vec<MediaEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct KanonizoOptions {
    /// Timeout/retry policy for remote resources.
    pub fetch: fetch::FetchConfig,
}

/// Kanonizo state threaded through expansion: the remote fetcher
/// and the paths/URLs on the current inclusion chain (for cycle
/// detection).
struct Ctx<'a> {
    fetcher: &'a dyn Fetcher,
    cfg: &'a fetch::FetchConfig,
}

impl Ctx<'_> {
    fn fetch(&self, url: &str) -> Result<fetch::Fetched> {
        fetch::fetch_with_retry(self.fetcher, self.cfg, url)
    }
}

#[derive(Default)]
struct Seen {
    paths: Vec<PathBuf>,
    urls: Vec<String>,
}

/// Run the full kanonizo pipeline on a deltos file, fetching remote
/// resources over HTTP(S) with the default policy.
pub fn kanonizo_file(path: &Path) -> Result<KanonResult> {
    let opts = KanonizoOptions::default();
    #[cfg(feature = "net")]
    let fetcher = fetch::HttpFetcher::new(&opts.fetch);
    #[cfg(not(feature = "net"))]
    let fetcher = fetch::DeniedFetcher;
    kanonizo_file_with(path, &fetcher, &opts)
}

/// Run the full kanonizo pipeline with an explicit fetcher and
/// options.
pub fn kanonizo_file_with(
    path: &Path,
    fetcher: &dyn Fetcher,
    opts: &KanonizoOptions,
) -> Result<KanonResult> {
    let ctx = Ctx {
        fetcher,
        cfg: &opts.fetch,
    };
    let mut seen = Seen::default();
    let mut doc = load_and_expand(path, &mut seen, &ctx)?;
    expand_axiomata(&mut doc)?;
    validate_deixes(&doc.blocks)?;
    validate_milestones(&doc.blocks)?;
    // Autonym sims (spec: "Auto-Onymization") pin their onyms from
    // their lemmas before canonical renumbering, which exempts them.
    let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let dial = dialektos::resolve(&base, &doc.dialect_id)?;
    let autonyms = assign_autonyms(&mut doc.blocks, &dial)?;
    canonicalize_onyms(&mut doc, &autonyms);
    evaluate_taxis_except(&mut doc.blocks, &autonym_symbols(&dial))?;
    // Vocabulary normalization (spec: "Vocabulary
    // Normalization"): aliases in any language canonicalize, so
    // authoring language cannot fork the kanon.
    normalize_vocabularies(&mut doc.blocks, &dial, &base);
    unwrap_empty_diaphanes_blocks(&mut doc.blocks);
    normalize_document(&mut doc);
    let base_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let media = process_media(&mut doc.blocks, &base_dir, &ctx)?;
    let kanon = dendron::serialize(&doc);
    Ok(KanonResult {
        document: doc,
        kanon,
        media,
    })
}

/// The sim symbols exempt from sibling-run taxis sequencing:
/// autonym sims number per lemma (homographs) instead.
fn autonym_symbols(dial: &dialektos::Dialektos) -> std::collections::HashSet<String> {
    dial.sims
        .values()
        .filter(|d| matches!(d.form, dialektos::SimForm::Para { autonym: true, .. }))
        .map(|d| d.symbol.clone())
        .collect()
}

/// Settle a parsed document in place without canonicalizing it
/// (Greek "tasso": to arrange — the verb behind "taxis"). Runs
/// the ordering subset of kanonizo: autonym pinning, sibling-run
/// taxis sequencing, and vocabulary normalization. Onyms stay as
/// authored, deixes are not validated, and no transclusion,
/// axioma expansion, or media processing takes place — anaphors
/// and axiomata remain visible as such. `base` resolves the
/// embedded dialektos of englossis blocks (the document's
/// directory, or `.` for in-memory sources).
pub fn tasso(doc: &mut Document, dial: &dialektos::Dialektos, base: &Path) -> Result<()> {
    assign_autonyms(&mut doc.blocks, dial)?;
    evaluate_taxis_except(&mut doc.blocks, &autonym_symbols(dial))?;
    normalize_vocabularies(&mut doc.blocks, dial, base);
    Ok(())
}

/// Result of kanonizing a dialektos definition file
/// (`.dia` → `.lektos`).
#[derive(Debug)]
pub struct LektosResult {
    pub dialektos: dialektos::Dialektos,
    /// Serialized canonical `.lektos` text.
    pub kanon: String,
}

/// Run definition-file kanonizo (spec: "Defining Dialektoi"): parse
/// the definition — which removes any shebang and comments, expands
/// inheritance and imports, and NFC-normalizes — and re-emit it in
/// canonical form (canonical sigil, sim definitions sorted by name).
/// Spec "Milestone Preservation": a coordinate names one point;
/// declaring the same scheme--value twice is an error.
/// Milestones are otherwise untouched by kanonizo.
fn validate_milestones(blocks: &[Block]) -> Result<()> {
    use std::collections::HashSet;
    fn walk_inlines(inlines: &[Inline], seen: &mut HashSet<(String, String)>) -> Result<()> {
        for inline in inlines {
            match inline {
                Inline::Milestone { scheme, value, .. }
                    if !seen.insert((scheme.clone(), value.clone())) =>
                {
                    return Err(Error::new(ErrorKind::Syntax(format!(
                        "duplicate milestone `{scheme}:{value}`"
                    ))));
                }
                Inline::Milestone { .. } => {}
                Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                    walk_inlines(content, seen)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn walk(blocks: &[Block], seen: &mut HashSet<(String, String)>) -> Result<()> {
        for block in blocks {
            match block {
                Block::Paragraph(inlines) => walk_inlines(inlines, seen)?,
                Block::Para {
                    lemma,
                    children,
                    hypograph,
                    ..
                } => {
                    walk_inlines(lemma, seen)?;
                    walk(children, seen)?;
                    walk_inlines(hypograph, seen)?;
                }
                Block::ParaDiaphane { children, .. } => walk(children, seen)?,
                Block::Stichoi {
                    lemma,
                    strophes,
                    hypograph,
                    ..
                } => {
                    walk_inlines(lemma, seen)?;
                    for strophe in strophes {
                        for line in &strophe.0 {
                            walk_inlines(line, seen)?;
                        }
                    }
                    walk_inlines(hypograph, seen)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut seen = HashSet::new();
    walk(blocks, &mut seen)
}

pub fn kanonizo_definition_file(path: &Path) -> Result<LektosResult> {
    let dialektos = dialektos::parse_file(path)?;
    let kanon = dialektos::serialize(&dialektos);
    Ok(LektosResult { dialektos, kanon })
}

/// Write kanonizo outputs: the `.atk` file and, when media is
/// present, the `.atk.tar.gz` archive with `media/` and the
/// manifest.
#[cfg(feature = "bundle")]
pub fn write_outputs(result: &KanonResult, atk_path: &Path) -> Result<()> {
    std::fs::write(atk_path, &result.kanon)?;
    if result.media.is_empty() {
        return Ok(());
    }
    let archive_path = atk_path.with_extension("atk.tar.gz");
    let file = std::fs::File::create(&archive_path)?;
    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);

    let atk_name = atk_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "document.atk".to_string());
    append_bytes(&mut tar, &atk_name, result.kanon.as_bytes())?;

    #[derive(serde::Serialize)]
    struct ManifestEntry<'a> {
        name: &'a str,
        source: &'a str,
        sha256: &'a str,
    }
    let manifest: Vec<ManifestEntry> = result
        .media
        .iter()
        .map(|m| ManifestEntry {
            name: &m.name,
            source: &m.source,
            sha256: &m.sha256,
        })
        .collect();
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Error::new(ErrorKind::Syntax(format!("manifest serialization: {e}"))))?;
    append_bytes(&mut tar, "media-manifest.json", manifest_json.as_bytes())?;

    for m in &result.media {
        append_bytes(&mut tar, &format!("media/{}", m.name), &m.bytes)?;
    }
    tar.into_inner()
        .and_then(|enc| enc.finish())
        .map_err(|e| Error::new(ErrorKind::Io(e)))?;
    Ok(())
}

#[cfg(feature = "bundle")]
fn append_bytes<W: std::io::Write>(
    tar: &mut tar::Builder<W>,
    name: &str,
    bytes: &[u8],
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0); // deterministic archive
    header.set_cksum();
    tar.append_data(&mut header, name, bytes)
        .map_err(|e| Error::new(ErrorKind::Io(e)))
}

// ---------------------------------------------------------------
// Loading and transclusion expansion
// ---------------------------------------------------------------

/// Read a source file, NFC-normalize, parse, and recursively expand
/// anaphor transclusions.
fn load_and_expand(path: &Path, seen: &mut Seen, ctx: &Ctx) -> Result<Document> {
    let canonical = path.canonicalize().map_err(|e| {
        Error::new(ErrorKind::MissingResource(format!(
            "{}: {e}",
            path.display()
        )))
    })?;
    if seen.paths.contains(&canonical) {
        return Err(Error::new(ErrorKind::TransclusionCycle(
            canonical.display().to_string(),
        )));
    }
    seen.paths.push(canonical);

    let bytes = std::fs::read(path).map_err(|e| {
        Error::new(ErrorKind::MissingResource(format!(
            "{}: {e}",
            path.display()
        )))
    })?;
    let source = String::from_utf8(bytes).map_err(|_| Error::new(ErrorKind::InvalidUtf8))?;
    let normalized: String = source.nfc().collect();
    let mut doc = parser::parse_document(&normalized, path)?;

    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    expand_transclusions(&mut doc.blocks, &dir, seen, ctx).map_err(|e| e.via(path))?;

    seen.paths.pop();
    Ok(doc)
}

/// Fetch and expand a remote document. Remote files have no local
/// directory: the dialektos declaration and any relative
/// transclusions resolve against `dir`, the including document's
/// directory (pilot decision; see README spec gaps).
fn load_and_expand_remote(url: &str, dir: &Path, seen: &mut Seen, ctx: &Ctx) -> Result<Document> {
    if seen.urls.iter().any(|u| u == url) {
        return Err(Error::new(ErrorKind::TransclusionCycle(url.to_string())));
    }
    seen.urls.push(url.to_string());

    let via = |e: Error| e.via(Path::new(url));
    let fetched = ctx.fetch(url)?;
    let source =
        String::from_utf8(fetched.bytes).map_err(|_| via(Error::new(ErrorKind::InvalidUtf8)))?;
    let normalized: String = source.nfc().collect();
    // Synthesize a parse path in the including directory so error
    // locations and dialektos resolution point somewhere sensible.
    let name = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("remote.atd");
    let parse_path = dir.join(name);
    let mut doc = parser::parse_document(&normalized, &parse_path).map_err(via)?;
    expand_transclusions(&mut doc.blocks, dir, seen, ctx).map_err(via)?;

    seen.urls.pop();
    Ok(doc)
}

fn is_url(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://")
}

fn expand_transclusions(
    blocks: &mut [Block],
    dir: &Path,
    seen: &mut Seen,
    ctx: &Ctx,
) -> Result<()> {
    let mut i = 0;
    while i < blocks.len() {
        match &mut blocks[i] {
            Block::AnaphorEnglossis { target } => {
                let inner = if is_url(target) {
                    load_and_expand_remote(&target.clone(), dir, seen, ctx)?
                } else {
                    load_and_expand(&dir.join(target.as_str()), seen, ctx)?
                };
                blocks[i] = Block::MonadEnglossis {
                    dialect: inner.dialect_id,
                    children: inner.blocks,
                    ann: Annotations::default(),
                };
            }
            Block::AnaphorEnlexis { target } => {
                let content = if is_url(target) {
                    let fetched = ctx.fetch(target)?;
                    String::from_utf8(fetched.bytes)
                        .map_err(|_| Error::new(ErrorKind::InvalidUtf8).via(Path::new(target)))?
                } else {
                    let target_path = dir.join(target.as_str());
                    std::fs::read_to_string(&target_path).map_err(|e| {
                        Error::new(ErrorKind::MissingResource(format!(
                            "{}: {e}",
                            target_path.display()
                        )))
                    })?
                };
                let content = if content.is_empty() || content.ends_with('\n') {
                    content
                } else {
                    content + "\n"
                };
                blocks[i] = Block::VerbatimBlock {
                    content,
                    ann: Annotations::default(),
                };
            }
            Block::Para { children, .. }
            | Block::ParaDiaphane { children, .. }
            | Block::MonadEnglossis { children, .. }
            | Block::ParaAxioma { children, .. } => {
                expand_transclusions(children, dir, seen, ctx)?;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

// ---------------------------------------------------------------
// Axioma expansion
// ---------------------------------------------------------------

enum AxiomaContent {
    Inline(Vec<Inline>),
    Block(Vec<Block>),
}

/// What an onym declared in the source document is attached to,
/// for axioma-enlexis resolution. The `Endo`/`Para` payload is the
/// simmere's canonical serialization with its own onym stripped
/// (the copy must not depend on the author-chosen anchor name).
enum OnymTarget {
    Standalone,
    Endo(String),
    Para(String),
}

/// Expand axioma references in document order. Axioma definitions
/// must precede their first reference and are removed from the
/// kanon. Enlexis references additionally resolve against any
/// onymized simmere (spec: Axioma Enlexis), collected in a pre-pass
/// so forward references are allowed (the before-first-reference
/// rule is stated for axiomata only).
fn expand_axiomata(doc: &mut Document) -> Result<()> {
    let mut targets: HashMap<String, OnymTarget> = HashMap::new();
    collect_onym_targets(&doc.blocks, &mut targets);
    let mut registry: HashMap<String, AxiomaContent> = HashMap::new();
    expand_axiomata_blocks(&mut doc.blocks, &mut registry, &targets)
}

fn collect_onym_targets(blocks: &[Block], targets: &mut HashMap<String, OnymTarget>) {
    let declare =
        |targets: &mut HashMap<String, OnymTarget>, onym: &Option<String>, block: &Block| {
            if let Some(name) = onym
                && !targets.contains_key(name)
            {
                targets.insert(name.clone(), OnymTarget::Para(block_copy_text(block)));
            }
        };
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => collect_onym_targets_inline(inlines, targets),
            Block::Para {
                lemma,
                children,
                hypograph,
                ann,
                ..
            } => {
                declare(targets, &ann.onym, block);
                collect_onym_targets_inline(lemma, targets);
                collect_onym_targets(children, targets);
                collect_onym_targets_inline(hypograph, targets);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ann,
                ..
            } => {
                declare(targets, &ann.onym, block);
                collect_onym_targets_inline(lemma, targets);
                for strophe in strophes {
                    for line in &strophe.0 {
                        collect_onym_targets_inline(line, targets);
                    }
                }
                collect_onym_targets_inline(hypograph, targets);
            }
            Block::ParaDiaphane { children, ann } | Block::MonadEnglossis { children, ann, .. } => {
                declare(targets, &ann.onym, block);
                collect_onym_targets(children, targets);
            }
            Block::VerbatimBlock { ann, .. } => declare(targets, &ann.onym, block),
            Block::ParaAxioma { children, .. } => collect_onym_targets(children, targets),
            _ => {}
        }
    }
}

fn collect_onym_targets_inline(inlines: &[Inline], targets: &mut HashMap<String, OnymTarget>) {
    let declare =
        |targets: &mut HashMap<String, OnymTarget>, onym: &Option<String>, inline: &Inline| {
            if let Some(name) = onym
                && !targets.contains_key(name)
            {
                targets.insert(name.clone(), OnymTarget::Endo(inline_copy_text(inline)));
            }
        };
    for inline in inlines {
        match inline {
            Inline::OnymAnchor(name) if !targets.contains_key(name) => {
                targets.insert(name.clone(), OnymTarget::Standalone);
            }
            Inline::Endo { content, ann, .. } => {
                declare(targets, &ann.onym, inline);
                collect_onym_targets_inline(content, targets);
            }
            Inline::EndoDiaphane { content, ann } => {
                declare(targets, &ann.onym, inline);
                collect_onym_targets_inline(content, targets);
            }
            Inline::VerbatimInline { ann, .. }
            | Inline::Monosim { ann, .. }
            | Inline::Deixis { ann, .. } => {
                declare(targets, &ann.onym, inline);
            }
            Inline::EndoAxioma { content, .. } => collect_onym_targets_inline(content, targets),
            _ => {}
        }
    }
}

/// Canonical source form of a block-level enlexis target: the block
/// with its own onym stripped, whitespace-normalized, serialized.
fn block_copy_text(block: &Block) -> String {
    let mut clone = block.clone();
    match &mut clone {
        Block::Para { ann, .. }
        | Block::Stichoi { ann, .. }
        | Block::ParaDiaphane { ann, .. }
        | Block::MonadEnglossis { ann, .. }
        | Block::VerbatimBlock { ann, .. } => ann.onym = None,
        _ => {}
    }
    let mut wrapped = vec![clone];
    normalize_blocks(&mut wrapped);
    dendron::serialize_block_fragment(&wrapped)
}

/// Canonical source form of an inline-level enlexis target.
fn inline_copy_text(inline: &Inline) -> String {
    let mut clone = inline.clone();
    match &mut clone {
        Inline::Endo { ann, .. }
        | Inline::EndoDiaphane { ann, .. }
        | Inline::VerbatimInline { ann, .. }
        | Inline::Monosim { ann, .. }
        | Inline::Deixis { ann, .. } => ann.onym = None,
        _ => {}
    }
    let mut wrapped = vec![clone];
    normalize_inline_seq(&mut wrapped);
    dendron::serialize_inline_fragment(&wrapped)
}

fn expand_axiomata_blocks(
    blocks: &mut Vec<Block>,
    registry: &mut HashMap<String, AxiomaContent>,
    targets: &HashMap<String, OnymTarget>,
) -> Result<()> {
    let mut i = 0;
    while i < blocks.len() {
        // Definitions and references are handled by replacement so
        // that splicing keeps document order intact.
        match &mut blocks[i] {
            Block::ParaAxioma { onym, children } => {
                let onym = onym.clone();
                let mut content = std::mem::take(children);
                expand_axiomata_blocks(&mut content, registry, targets)?;
                registry.insert(onym, AxiomaContent::Block(content));
                blocks.remove(i);
                continue;
            }
            Block::AxiomaRefBlock { onym, enlexis } => {
                let onym = onym.clone();
                if *enlexis {
                    blocks[i] = resolve_enlexis_block(&onym, registry, targets)?;
                    i += 1;
                } else {
                    let Some(AxiomaContent::Block(content)) = registry.get(onym.as_str()) else {
                        return Err(Error::new(ErrorKind::AxiomaBeforeDefinition(onym)));
                    };
                    let content = content.clone();
                    let n = content.len();
                    blocks.splice(i..=i, content);
                    i += n;
                }
                continue;
            }
            Block::Paragraph(inlines) => expand_axiomata_inlines(inlines, registry, targets)?,
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                expand_axiomata_inlines(lemma, registry, targets)?;
                expand_axiomata_blocks(children, registry, targets)?;
                expand_axiomata_inlines(hypograph, registry, targets)?;
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                expand_axiomata_inlines(lemma, registry, targets)?;
                for strophe in strophes {
                    for line in &mut strophe.0 {
                        expand_axiomata_inlines(line, registry, targets)?;
                    }
                }
                expand_axiomata_inlines(hypograph, registry, targets)?;
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                expand_axiomata_blocks(children, registry, targets)?;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

/// Resolve a block-context enlexis reference: axioma definitions
/// take precedence, then onymized simmeres. Inline content in block
/// context wraps as a paragraph-hosted lexema-enlexis.
fn resolve_enlexis_block(
    onym: &str,
    registry: &HashMap<String, AxiomaContent>,
    targets: &HashMap<String, OnymTarget>,
) -> Result<Block> {
    let verbatim_block = |content: String| Block::VerbatimBlock {
        content,
        ann: Annotations::default(),
    };
    let verbatim_paragraph = |content: String| {
        Block::Paragraph(vec![Inline::VerbatimInline {
            content,
            ann: Annotations::default(),
        }])
    };
    match registry.get(onym) {
        Some(AxiomaContent::Block(content)) => {
            Ok(verbatim_block(dendron::serialize_block_fragment(content)))
        }
        Some(AxiomaContent::Inline(content)) => Ok(verbatim_paragraph(
            dendron::serialize_inline_fragment(content),
        )),
        None => match targets.get(onym) {
            Some(OnymTarget::Para(s)) => Ok(verbatim_block(s.clone())),
            Some(OnymTarget::Endo(s)) => Ok(verbatim_paragraph(s.clone())),
            Some(OnymTarget::Standalone) => Err(Error::new(ErrorKind::EnlexisStandaloneOnym(
                onym.to_string(),
            ))),
            None => Err(Error::new(ErrorKind::AxiomaBeforeDefinition(
                onym.to_string(),
            ))),
        },
    }
}

/// Resolve an inline-context enlexis reference. Block content is an
/// error (a para-simmere cannot be included in endo-context).
fn resolve_enlexis_inline(
    onym: &str,
    registry: &HashMap<String, AxiomaContent>,
    targets: &HashMap<String, OnymTarget>,
) -> Result<Inline> {
    let content = match registry.get(onym) {
        Some(AxiomaContent::Inline(content)) => dendron::serialize_inline_fragment(content),
        Some(AxiomaContent::Block(_)) => {
            return Err(Error::new(ErrorKind::EnlexisParaInEndoContext(
                onym.to_string(),
            )));
        }
        None => match targets.get(onym) {
            Some(OnymTarget::Endo(s)) => s.clone(),
            Some(OnymTarget::Para(_)) => {
                return Err(Error::new(ErrorKind::EnlexisParaInEndoContext(
                    onym.to_string(),
                )));
            }
            Some(OnymTarget::Standalone) => {
                return Err(Error::new(ErrorKind::EnlexisStandaloneOnym(
                    onym.to_string(),
                )));
            }
            None => {
                return Err(Error::new(ErrorKind::AxiomaBeforeDefinition(
                    onym.to_string(),
                )));
            }
        },
    };
    Ok(Inline::VerbatimInline {
        content,
        ann: Annotations::default(),
    })
}

fn expand_axiomata_inlines(
    inlines: &mut Vec<Inline>,
    registry: &mut HashMap<String, AxiomaContent>,
    targets: &HashMap<String, OnymTarget>,
) -> Result<()> {
    let mut i = 0;
    while i < inlines.len() {
        match &mut inlines[i] {
            Inline::EndoAxioma { onym, content } => {
                let onym = onym.clone();
                let mut content = std::mem::take(content);
                expand_axiomata_inlines(&mut content, registry, targets)?;
                registry.insert(onym, AxiomaContent::Inline(content));
                inlines.remove(i);
                continue;
            }
            Inline::AxiomaRef { onym, enlexis } => {
                let onym = onym.clone();
                if *enlexis {
                    inlines[i] = resolve_enlexis_inline(&onym, registry, targets)?;
                    i += 1;
                } else {
                    let Some(AxiomaContent::Inline(content)) = registry.get(onym.as_str()) else {
                        return Err(Error::new(ErrorKind::AxiomaBeforeDefinition(onym)));
                    };
                    let content = content.clone();
                    let n = content.len();
                    inlines.splice(i..=i, content);
                    i += n;
                }
                continue;
            }
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                expand_axiomata_inlines(content, registry, targets)?;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

// ---------------------------------------------------------------
// Deixis validation
// ---------------------------------------------------------------

/// Every deixis must point at a declared onym attached to a
/// para-simmere (or stichoi-form simmere) of the deixis's sim
/// (spec: Deixis).
pub(crate) fn validate_deixes(blocks: &[Block]) -> Result<()> {
    // Onym -> symbol of the para/stichoi block it is attached to.
    let mut targets: HashMap<String, Option<String>> = HashMap::new();
    collect_block_onym_symbols(blocks, &mut targets);
    check_deixes_blocks(blocks, &targets)
}

fn collect_block_onym_symbols(blocks: &[Block], targets: &mut HashMap<String, Option<String>>) {
    for block in blocks {
        let (symbol, ann, children) = match block {
            Block::Para {
                symbol,
                children,
                ann,
                ..
            } => (Some(symbol.clone()), Some(ann), Some(children)),
            Block::Stichoi { symbol, ann, .. } => (symbol.clone(), Some(ann), None),
            Block::ParaDiaphane { children, ann } => (None, Some(ann), Some(children)),
            Block::MonadEnglossis { children, ann, .. } => (None, Some(ann), Some(children)),
            Block::VerbatimBlock { ann, .. } => (None, Some(ann), None),
            _ => (None, None, None),
        };
        if let Some(ann) = ann
            && let Some(onym) = &ann.onym
        {
            targets.entry(onym.clone()).or_insert(symbol);
        }
        match block {
            Block::Paragraph(inlines) => collect_inline_onym_decls(inlines, targets),
            Block::Para {
                lemma, hypograph, ..
            } => {
                collect_inline_onym_decls(lemma, targets);
                collect_inline_onym_decls(hypograph, targets);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                collect_inline_onym_decls(lemma, targets);
                for strophe in strophes {
                    for line in &strophe.0 {
                        collect_inline_onym_decls(line, targets);
                    }
                }
                collect_inline_onym_decls(hypograph, targets);
            }
            _ => {}
        }
        if let Some(children) = children {
            collect_block_onym_symbols(children, targets);
        }
    }
}

/// Inline onym declarations enter the target map with no symbol: a
/// deixis pointing at one is a target mismatch, not an undefined
/// reference.
fn collect_inline_onym_decls(inlines: &[Inline], targets: &mut HashMap<String, Option<String>>) {
    for inline in inlines {
        match inline {
            Inline::OnymAnchor(name) => {
                targets.entry(name.clone()).or_insert(None);
            }
            Inline::Endo { content, ann, .. } => {
                if let Some(o) = &ann.onym {
                    targets.entry(o.clone()).or_insert(None);
                }
                collect_inline_onym_decls(content, targets);
            }
            Inline::EndoDiaphane { content, ann } => {
                if let Some(o) = &ann.onym {
                    targets.entry(o.clone()).or_insert(None);
                }
                collect_inline_onym_decls(content, targets);
            }
            Inline::VerbatimInline { ann, .. }
            | Inline::Monosim { ann, .. }
            | Inline::Deixis { ann, .. } => {
                if let Some(o) = &ann.onym {
                    targets.entry(o.clone()).or_insert(None);
                }
            }
            _ => {}
        }
    }
}

fn check_deixes_blocks(blocks: &[Block], targets: &HashMap<String, Option<String>>) -> Result<()> {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => check_deixes_inlines(inlines, targets)?,
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                check_deixes_inlines(lemma, targets)?;
                check_deixes_blocks(children, targets)?;
                check_deixes_inlines(hypograph, targets)?;
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                check_deixes_inlines(lemma, targets)?;
                for strophe in strophes {
                    for line in &strophe.0 {
                        check_deixes_inlines(line, targets)?;
                    }
                }
                check_deixes_inlines(hypograph, targets)?;
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                check_deixes_blocks(children, targets)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_deixes_inlines(
    inlines: &[Inline],
    targets: &HashMap<String, Option<String>>,
) -> Result<()> {
    for inline in inlines {
        match inline {
            Inline::Deixis { symbol, onym, .. } => match targets.get(onym) {
                None => {
                    return Err(Error::new(ErrorKind::DeixisUndefinedOnym(onym.clone())));
                }
                Some(target_symbol) if target_symbol.as_deref() != Some(symbol) => {
                    return Err(Error::new(ErrorKind::DeixisTargetMismatch {
                        symbol: symbol.clone(),
                        onym: onym.clone(),
                    }));
                }
                _ => {}
            },
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                check_deixes_inlines(content, targets)?;
            }
            _ => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------
// Onym canonicalization
// ---------------------------------------------------------------

/// Rewrite onyms to canonical `o<n>` identifiers: collect
/// declarations in document order, drop unreferenced ones, renumber
/// referenced ones, and rewrite references (monosim parameters that
/// match a declared onym).
fn canonicalize_onyms(doc: &mut Document, exempt: &std::collections::HashSet<String>) {
    let mut declared: Vec<String> = Vec::new();
    walk_annotations(&mut doc.blocks, &mut |what| {
        if let OnymSite::Declaration(name) = what
            && !exempt.contains(name.as_str())
            && !declared.contains(name)
        {
            declared.push(name.clone());
        }
    });
    let mut referenced: Vec<String> = Vec::new();
    walk_annotations(&mut doc.blocks, &mut |what| {
        if let OnymSite::Reference(param) = what
            && declared.contains(param)
            && !referenced.contains(param)
        {
            referenced.push(param.clone());
        }
    });
    // Canonical numbering follows declaration order.
    let mut mapping: HashMap<String, String> = HashMap::new();
    let mut counter = 0u64;
    for name in &declared {
        if referenced.contains(name) {
            counter += 1;
            mapping.insert(name.clone(), format!("o{counter}"));
        }
    }
    // Autonym onyms are citable identity: they survive even
    // unreferenced, under their computed names.
    for name in exempt {
        mapping.insert(name.clone(), name.clone());
    }
    rewrite_onyms(&mut doc.blocks, &mapping);
}

/// Assign autonym onyms (spec: "Auto-Onymization"): a para-sim
/// declared `autonym` registers its lemma, sim-stripped and
/// normalized, as its onym; an explicit taxis suffixes it, which
/// is how homographs stay distinct. Kanonizo pins the value: an
/// absent onym is filled, a matching one is kept (idempotence),
/// a contradicting one is an error, and duplicates are errors.
/// The returned set is exempt from canonical renumbering.
fn assign_autonyms(
    blocks: &mut [Block],
    dial: &dialektos::Dialektos,
) -> Result<std::collections::HashSet<String>> {
    let mut assigned = std::collections::HashSet::new();
    let mut groups: std::collections::HashMap<String, (u64, Vec<u64>)> =
        std::collections::HashMap::new();
    assign_autonyms_walk(blocks, dial, &mut assigned, &mut groups)?;
    // Homograph numbering is per lemma: every homograph carries a
    // taxis and the numbers run 1..n in document order.
    for (lemma, (untaxed, mut numbers)) in groups {
        if numbers.is_empty() {
            continue;
        }
        if untaxed > 0 {
            return Err(Error::new(ErrorKind::Syntax(format!(
                "autonym homographs of `{lemma}` must all carry a taxis"
            ))));
        }
        numbers.sort_unstable();
        for (i, n) in numbers.iter().enumerate() {
            if *n != (i as u64) + 1 {
                return Err(Error::new(ErrorKind::Syntax(format!(
                    "autonym homographs of `{lemma}`: taxis must run 1..n \
                     (found {n} where {} was expected)",
                    i + 1
                ))));
            }
        }
    }
    Ok(assigned)
}

fn autonym_of(lemma: &[Inline], taxis: &Option<Taxis>) -> Option<String> {
    fn text_of(inlines: &[Inline], out: &mut String) {
        for inline in inlines {
            match inline {
                Inline::Text(t) => out.push_str(t),
                Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                    text_of(content, out)
                }
                _ => {}
            }
        }
    }
    let mut text = String::new();
    text_of(lemma, &mut text);
    let mut onym = String::new();
    let mut pending_sep = false;
    for c in text.trim().chars() {
        if c.is_alphanumeric() {
            if pending_sep && !onym.is_empty() {
                onym.push('-');
            }
            pending_sep = false;
            onym.push(c);
        } else {
            pending_sep = true;
        }
    }
    if let Some(Taxis::Explicit(n)) = taxis {
        if !onym.is_empty() {
            onym.push('-');
        }
        onym.push_str(&n.to_string());
    }
    if crate::sigil::is_valid_onym(&onym) {
        Some(onym)
    } else {
        None
    }
}

fn assign_autonyms_walk(
    blocks: &mut [Block],
    dial: &dialektos::Dialektos,
    assigned: &mut std::collections::HashSet<String>,
    groups: &mut std::collections::HashMap<String, (u64, Vec<u64>)>,
) -> Result<()> {
    for block in blocks {
        match block {
            Block::Para {
                symbol,
                taxis,
                lemma,
                children,
                ann,
                ..
            } => {
                let is_autonym = dial.sims.values().any(|def| {
                    def.symbol == *symbol
                        && matches!(def.form, dialektos::SimForm::Para { autonym: true, .. })
                });
                if is_autonym {
                    let base = autonym_of(lemma, &None);
                    if let Some(base) = base {
                        let entry = groups.entry(base).or_default();
                        match taxis {
                            Some(Taxis::Explicit(n)) => entry.1.push(*n),
                            _ => entry.0 += 1,
                        }
                    }
                    let Some(computed) = autonym_of(lemma, taxis) else {
                        return Err(Error::new(ErrorKind::Syntax(format!(
                            "autonym sim `{symbol}`: the lemma yields no valid onym"
                        ))));
                    };
                    match &ann.onym {
                        None => ann.onym = Some(computed.clone()),
                        Some(existing) if *existing == computed => {}
                        Some(existing) => {
                            return Err(Error::new(ErrorKind::Syntax(format!(
                                "autonym sim `{symbol}`: explicit onym `{existing}` \
                                 contradicts the computed `{computed}`"
                            ))));
                        }
                    }
                    if !assigned.insert(computed.clone()) {
                        return Err(Error::new(ErrorKind::Syntax(format!(
                            "autonym sim `{symbol}`: duplicate onym `{computed}` \
                             (disambiguate homographs with a taxis)"
                        ))));
                    }
                }
                assign_autonyms_walk(children, dial, assigned, groups)?;
            }
            Block::ParaDiaphane { children, .. } => {
                assign_autonyms_walk(children, dial, assigned, groups)?;
            }
            Block::MonadEnglossis { children, .. } => {
                assign_autonyms_walk(children, dial, assigned, groups)?;
            }
            _ => {}
        }
    }
    Ok(())
}

enum OnymSite<'a> {
    Declaration(&'a String),
    Reference(&'a String),
}

fn walk_annotations(blocks: &mut [Block], f: &mut dyn FnMut(OnymSite<'_>)) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => walk_annotations_inline(inlines, f),
            Block::Para {
                lemma,
                children,
                hypograph,
                ann,
                ..
            } => {
                if let Some(o) = &ann.onym {
                    f(OnymSite::Declaration(o));
                }
                walk_annotations_inline(lemma, f);
                walk_annotations(children, f);
                walk_annotations_inline(hypograph, f);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ann,
                ..
            } => {
                if let Some(o) = &ann.onym {
                    f(OnymSite::Declaration(o));
                }
                walk_annotations_inline(lemma, f);
                for strophe in strophes {
                    for line in &mut strophe.0 {
                        walk_annotations_inline(line, f);
                    }
                }
                walk_annotations_inline(hypograph, f);
            }
            Block::ParaDiaphane { children, ann } => {
                if let Some(o) = &ann.onym {
                    f(OnymSite::Declaration(o));
                }
                walk_annotations(children, f);
            }
            Block::MonadEnglossis { children, ann, .. } => {
                if let Some(o) = &ann.onym {
                    f(OnymSite::Declaration(o));
                }
                walk_annotations(children, f);
            }
            Block::VerbatimBlock { ann, .. } => {
                if let Some(o) = &ann.onym {
                    f(OnymSite::Declaration(o));
                }
            }
            _ => {}
        }
    }
}

fn walk_annotations_inline(inlines: &mut [Inline], f: &mut dyn FnMut(OnymSite<'_>)) {
    for inline in inlines {
        match inline {
            Inline::OnymAnchor(name) => f(OnymSite::Declaration(name)),
            Inline::Endo { content, ann, .. } => {
                if let Some(o) = &ann.onym {
                    f(OnymSite::Declaration(o));
                }
                walk_annotations_inline(content, f);
            }
            Inline::EndoDiaphane { content, ann } => {
                if let Some(o) = &ann.onym {
                    f(OnymSite::Declaration(o));
                }
                walk_annotations_inline(content, f);
            }
            Inline::VerbatimInline { ann, .. } => {
                if let Some(o) = &ann.onym {
                    f(OnymSite::Declaration(o));
                }
            }
            Inline::Monosim { param, .. } => f(OnymSite::Reference(param)),
            Inline::Deixis { onym, ann, .. } => {
                if let Some(o) = &ann.onym {
                    f(OnymSite::Declaration(o));
                }
                f(OnymSite::Reference(onym));
            }
            _ => {}
        }
    }
}

fn rewrite_onyms(blocks: &mut [Block], mapping: &HashMap<String, String>) {
    let rewrite_ann = |ann: &mut Annotations| {
        ann.onym = ann.onym.take().and_then(|o| mapping.get(&o).cloned());
    };
    for block in blocks.iter_mut() {
        match block {
            Block::Paragraph(inlines) => rewrite_onyms_inline(inlines, mapping),
            Block::Para {
                lemma,
                children,
                hypograph,
                ann,
                ..
            } => {
                rewrite_ann(ann);
                rewrite_onyms_inline(lemma, mapping);
                rewrite_onyms(children, mapping);
                rewrite_onyms_inline(hypograph, mapping);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ann,
                ..
            } => {
                rewrite_ann(ann);
                rewrite_onyms_inline(lemma, mapping);
                for strophe in strophes {
                    for line in &mut strophe.0 {
                        rewrite_onyms_inline(line, mapping);
                    }
                }
                rewrite_onyms_inline(hypograph, mapping);
            }
            Block::ParaDiaphane { children, ann } | Block::MonadEnglossis { children, ann, .. } => {
                rewrite_ann(ann);
                rewrite_onyms(children, mapping);
            }
            Block::VerbatimBlock { ann, .. } => rewrite_ann(ann),
            _ => {}
        }
    }
}

fn rewrite_onyms_inline(inlines: &mut Vec<Inline>, mapping: &HashMap<String, String>) {
    let rewrite_ann = |ann: &mut Annotations| {
        ann.onym = ann.onym.take().and_then(|o| mapping.get(&o).cloned());
    };
    let mut i = 0;
    while i < inlines.len() {
        match &mut inlines[i] {
            Inline::OnymAnchor(name) => match mapping.get(name.as_str()) {
                Some(new) => *name = new.clone(),
                None => {
                    inlines.remove(i);
                    continue;
                }
            },
            Inline::Endo { content, ann, .. } => {
                rewrite_ann(ann);
                rewrite_onyms_inline(content, mapping);
            }
            Inline::EndoDiaphane { content, ann } => {
                rewrite_ann(ann);
                rewrite_onyms_inline(content, mapping);
            }
            Inline::VerbatimInline { ann, .. } => rewrite_ann(ann),
            Inline::Monosim { param, .. } => {
                if let Some(new) = mapping.get(param.as_str()) {
                    *param = new.clone();
                }
            }
            Inline::Deixis { onym, ann, .. } => {
                rewrite_ann(ann);
                if let Some(new) = mapping.get(onym.as_str()) {
                    *onym = new.clone();
                }
            }
            _ => {}
        }
        i += 1;
    }
}

// ---------------------------------------------------------------
// Taxis evaluation
// ---------------------------------------------------------------

pub(crate) fn evaluate_taxis(blocks: &mut [Block]) -> Result<()> {
    evaluate_taxis_except(blocks, &std::collections::HashSet::new())
}

/// Sibling-run taxis sequencing, skipping the given sim symbols:
/// autonym sims number per lemma (homographs), not per sibling
/// run, and are validated by `assign_autonyms` instead.
pub(crate) fn evaluate_taxis_except(
    blocks: &mut [Block],
    exempt_symbols: &std::collections::HashSet<String>,
) -> Result<()> {
    let mut run: Option<(String, u64)> = None;
    for block in blocks.iter_mut() {
        match block {
            Block::Para {
                symbol,
                taxis: Some(taxis),
                children,
                ..
            } if !exempt_symbols.contains(symbol.as_str()) => {
                let counter = match &mut run {
                    Some((sym, counter)) if sym == symbol => {
                        *counter += 1;
                        *counter
                    }
                    _ => {
                        run = Some((symbol.clone(), 1));
                        1
                    }
                };
                match taxis {
                    Taxis::Auto => *taxis = Taxis::Explicit(counter),
                    Taxis::Explicit(n) => {
                        if *n != counter {
                            return Err(Error::new(ErrorKind::TaxisInconsistent {
                                expected: counter,
                                found: *n,
                            }));
                        }
                    }
                }
                evaluate_taxis_except(children, exempt_symbols)?;
            }
            Block::Stichoi {
                symbol: Some(symbol),
                taxis: Some(taxis),
                ..
            } => {
                let counter = match &mut run {
                    Some((sym, counter)) if sym == symbol => {
                        *counter += 1;
                        *counter
                    }
                    _ => {
                        run = Some((symbol.clone(), 1));
                        1
                    }
                };
                match taxis {
                    Taxis::Auto => *taxis = Taxis::Explicit(counter),
                    Taxis::Explicit(n) => {
                        if *n != counter {
                            return Err(Error::new(ErrorKind::TaxisInconsistent {
                                expected: counter,
                                found: *n,
                            }));
                        }
                    }
                }
            }
            Block::Para { children, .. }
            | Block::ParaDiaphane { children, .. }
            | Block::MonadEnglossis { children, .. } => {
                run = None;
                evaluate_taxis_except(children, exempt_symbols)?;
            }
            _ => run = None,
        }
    }
    Ok(())
}

// ---------------------------------------------------------------
// Diaphane unwrapping and whitespace normalization
// ---------------------------------------------------------------

/// Unwrap diaphanes that carry no annotations (after onym
/// canonicalization, so a diaphane whose onym went unreferenced is
/// also unwrapped).
fn unwrap_empty_diaphanes_blocks(blocks: &mut Vec<Block>) {
    let mut i = 0;
    while i < blocks.len() {
        match &mut blocks[i] {
            Block::ParaDiaphane { children, ann } if ann.is_empty() => {
                let children = std::mem::take(children);
                let n = children.len();
                blocks.splice(i..=i, children);
                // Re-visit the spliced range (may nest).
                let _ = n;
                continue;
            }
            Block::Paragraph(inlines) => unwrap_empty_diaphanes_inline(inlines),
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                unwrap_empty_diaphanes_inline(lemma);
                unwrap_empty_diaphanes_blocks(children);
                unwrap_empty_diaphanes_inline(hypograph);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                unwrap_empty_diaphanes_inline(lemma);
                for strophe in strophes {
                    for line in &mut strophe.0 {
                        unwrap_empty_diaphanes_inline(line);
                    }
                }
                unwrap_empty_diaphanes_inline(hypograph);
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                unwrap_empty_diaphanes_blocks(children);
            }
            _ => {}
        }
        i += 1;
    }
}

fn unwrap_empty_diaphanes_inline(inlines: &mut Vec<Inline>) {
    let mut i = 0;
    while i < inlines.len() {
        match &mut inlines[i] {
            Inline::EndoDiaphane { content, ann } if ann.is_empty() => {
                let content = std::mem::take(content);
                inlines.splice(i..=i, content);
                continue;
            }
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                unwrap_empty_diaphanes_inline(content);
            }
            _ => {}
        }
        i += 1;
    }
}

/// Whitespace normalization and paragraph collapsing: in every
/// inline sequence (outside verbatim content), whitespace runs
/// become single spaces and boundary whitespace is trimmed.
/// Canonicalize vocabulary-bound lemmas and genoses (spec:
/// "Controlled Vocabularies"). Only single-text lemmas are
/// candidates - vocabulary terms are plain tokens.
fn normalize_vocabularies(blocks: &mut [Block], dial: &dialektos::Dialektos, base: &Path) {
    let canonicalize_genoses = |symbol: &str, ann: &mut Annotations| {
        if let Some(def) = dial.sims.get(symbol)
            && let Some(vocab_name) = &def.genos_vocabulary
            && let Some(vocab) = dial.vocabularies.get(vocab_name)
        {
            for genos in &mut ann.genoses {
                let canonical = vocab.canonicalize(genos).to_string();
                *genos = canonical;
            }
        }
    };
    fn walk_inlines(inlines: &mut [Inline], canon: &impl Fn(&str, &mut Annotations)) {
        for inline in inlines {
            match inline {
                Inline::Endo {
                    symbol,
                    content,
                    ann,
                    ..
                } => {
                    canon(symbol, ann);
                    walk_inlines(content, canon);
                }
                Inline::Monosim { symbol, ann, .. } | Inline::Deixis { symbol, ann, .. } => {
                    canon(symbol, ann);
                }
                Inline::EndoDiaphane { content, .. } => walk_inlines(content, canon),
                _ => {}
            }
        }
    }
    fn walk(
        blocks: &mut [Block],
        canon: &impl Fn(&str, &mut Annotations),
        dial: &dialektos::Dialektos,
        base: &Path,
    ) {
        for block in blocks {
            match block {
                Block::Para {
                    symbol,
                    lemma,
                    children,
                    hypograph,
                    ann,
                    ..
                } => {
                    canon(symbol, ann);
                    if let Some(def) = dial.sims.get(symbol.as_str())
                        && let Some(vocab_name) = &def.lemma_vocabulary
                        && let Some(vocab) = dial.vocabularies.get(vocab_name)
                        && let [Inline::Text(text)] = lemma.as_mut_slice()
                    {
                        *text = vocab.canonicalize(text.trim()).to_string();
                    }
                    walk_inlines(lemma, canon);
                    walk(children, canon, dial, base);
                    walk_inlines(hypograph, canon);
                }
                Block::Stichoi {
                    symbol,
                    lemma,
                    strophes,
                    hypograph,
                    ann,
                    ..
                } => {
                    if let Some(symbol) = symbol {
                        canon(symbol, ann);
                        if let Some(def) = dial.sims.get(symbol.as_str())
                            && let Some(vocab_name) = &def.lemma_vocabulary
                            && let Some(vocab) = dial.vocabularies.get(vocab_name)
                            && let [Inline::Text(text)] = lemma.as_mut_slice()
                        {
                            *text = vocab.canonicalize(text.trim()).to_string();
                        }
                    }
                    walk_inlines(lemma, canon);
                    for strophe in strophes {
                        for line in &mut strophe.0 {
                            walk_inlines(line, canon);
                        }
                    }
                    walk_inlines(hypograph, canon);
                }
                Block::Paragraph(inlines) => walk_inlines(inlines, canon),
                Block::ParaDiaphane { children, .. } => {
                    walk(children, canon, dial, base);
                }
                // An englossis block normalizes against the
                // embedded dialektos's own vocabularies.
                Block::MonadEnglossis {
                    dialect, children, ..
                } => {
                    if dialect != &dial.id
                        && let Ok(inner) = dialektos::resolve(base, dialect)
                    {
                        normalize_vocabularies(children, &inner, base);
                    } else {
                        walk(children, canon, dial, base);
                    }
                }
                _ => {}
            }
        }
    }
    walk(blocks, &canonicalize_genoses, dial, base);
}

fn normalize_document(doc: &mut Document) {
    normalize_blocks(&mut doc.blocks);
}

fn normalize_blocks(blocks: &mut Vec<Block>) {
    for block in blocks.iter_mut() {
        match block {
            Block::Paragraph(inlines) => normalize_inline_seq(inlines),
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                normalize_inline_seq(lemma);
                normalize_blocks(children);
                normalize_inline_seq(hypograph);
            }
            Block::Stichoi {
                lemma, hypograph, ..
            } => {
                // Stichos lines are exempt from whitespace
                // normalization: leading and internal whitespace
                // is authorial content (spec: Whitespace
                // Normalization). The lemma and hypograph are
                // ordinary inline components.
                normalize_inline_seq(lemma);
                normalize_inline_seq(hypograph);
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                normalize_blocks(children);
            }
            _ => {}
        }
    }
    // Drop paragraphs that normalized to nothing.
    blocks.retain(|b| !matches!(b, Block::Paragraph(inlines) if inlines.is_empty()));
}

/// Normalize one inline sequence: merge adjacent text nodes,
/// collapse whitespace runs, trim the sequence boundaries, recurse
/// into nested inline content.
pub(crate) fn normalize_inline_seq(inlines: &mut Vec<Inline>) {
    // Recurse first.
    for inline in inlines.iter_mut() {
        match inline {
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                normalize_inline_seq(content);
            }
            _ => {}
        }
    }
    // Merge adjacent Text nodes.
    let mut merged: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in inlines.drain(..) {
        if let (Some(Inline::Text(prev)), Inline::Text(cur)) = (merged.last_mut(), &inline) {
            prev.push_str(cur);
            continue;
        }
        merged.push(inline);
    }
    // Collapse whitespace runs within each text node. A run
    // spanning a text-node boundary is already merged above; runs
    // adjacent to non-text inlines keep a single space.
    for inline in merged.iter_mut() {
        if let Inline::Text(t) = inline {
            let mut out = String::with_capacity(t.len());
            let mut in_ws = false;
            for c in t.chars() {
                if c.is_whitespace() {
                    if !in_ws {
                        out.push(' ');
                    }
                    in_ws = true;
                } else {
                    out.push(c);
                    in_ws = false;
                }
            }
            *t = out;
        }
    }
    // Trim sequence boundaries.
    if let Some(Inline::Text(t)) = merged.first_mut() {
        *t = t.trim_start().to_string();
    }
    if let Some(Inline::Text(t)) = merged.last_mut() {
        *t = t.trim_end().to_string();
    }
    merged.retain(|i| !matches!(i, Inline::Text(t) if t.is_empty()));
    *inlines = merged;
}

// ---------------------------------------------------------------
// Media processing
// ---------------------------------------------------------------

fn process_media(blocks: &mut [Block], base_dir: &Path, ctx: &Ctx) -> Result<Vec<MediaEntry>> {
    let mut entries = Vec::new();
    process_media_blocks(blocks, base_dir, ctx, &mut entries)?;
    Ok(entries)
}

fn process_media_blocks(
    blocks: &mut [Block],
    base_dir: &Path,
    ctx: &Ctx,
    entries: &mut Vec<MediaEntry>,
) -> Result<()> {
    for block in blocks.iter_mut() {
        match block {
            Block::Enmedia { param } => {
                let (bytes, content_type) = if is_url(param) {
                    let fetched = ctx.fetch(param)?;
                    (fetched.bytes, fetched.content_type)
                } else {
                    let resolved = base_dir.join(param.as_str());
                    let bytes = std::fs::read(&resolved).map_err(|e| {
                        Error::new(ErrorKind::MissingResource(format!(
                            "{}: {e}",
                            resolved.display()
                        )))
                    })?;
                    (bytes, None)
                };
                let sha256 = hex(&Sha256::digest(&bytes));
                let n = entries.len() + 1;
                let ext = fetch::infer_extension(param, content_type.as_deref(), &bytes);
                let name = format!("m{n}.{ext}");
                let source = std::mem::take(param);
                *param = format!("media/{name}");
                entries.push(MediaEntry {
                    name,
                    source,
                    sha256,
                    bytes,
                });
            }
            Block::Para { children, .. }
            | Block::ParaDiaphane { children, .. }
            | Block::MonadEnglossis { children, .. } => {
                process_media_blocks(children, base_dir, ctx, entries)?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
