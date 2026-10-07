//! Litosis: reduction of a kanon to its pure content and the litos
//! ID (spec: chapter "Litosis").
//!
//! Strips all genoses and all monosims except enmedia; enmedia
//! monosims are replaced by the SHA-256 of the referenced media
//! content. Onyms are stripped except those referenced by a
//! deixis: where content attaches is part of the document's
//! identity, so deixes and their targets' onyms survive,
//! renumbered afresh in declaration order so stripped metadata
//! cannot influence the hash. The litos ID is the SHA-256 of the
//! serialized result.

use sha2::{Digest, Sha256};

use crate::dendron::{self, Annotations, Block, Document, Inline};
use crate::dialektos;
use crate::error::{Error, ErrorKind, Result};
use crate::kanonizo::hex;

/// The litosis result: the serialized litos text and the litos ID
/// (lowercase-hex SHA-256 of that text).
#[derive(Debug)]
pub struct LitosResult {
    pub litos: String,
    pub litos_id: String,
}

/// Reduce a kanon document to its litos. `read_media` maps an
/// enmedia parameter (e.g. `media/m1.svg`) to the media bytes;
/// use [`media_from_dir`] for the common on-disk layout.
pub fn litosis(
    doc: &Document,
    read_media: &dyn Fn(&str) -> Result<Vec<u8>>,
) -> Result<LitosResult> {
    litosis_with(doc, read_media, &|_| None)
}

/// [`litosis`] with dialektos resolution: genoses drawn from a
/// sim's bound vocabulary are semantic by declaration and are
/// preserved (spec: "Controlled Vocabularies"); free genoses
/// strip as before. The resolver answers for the document's
/// dialektos and any englossis-embedded ones.
pub fn litosis_with(
    doc: &Document,
    read_media: &dyn Fn(&str) -> Result<Vec<u8>>,
    resolve: &dyn Fn(&str) -> Option<dialektos::Dialektos>,
) -> Result<LitosResult> {
    let mut doc = doc.clone();
    let mut kept: std::collections::HashSet<String> = std::collections::HashSet::new();
    collect_deixis_onyms(&doc.blocks, &mut kept);
    let dial = resolve(&doc.dialect_id);
    // Autonym onyms survive litosis even unreferenced and are
    // exempt from the o1.. renumbering: a computed headword is
    // citable identity, like a milestone (spec v0.12.1).
    let mut autonym: std::collections::HashSet<String> = std::collections::HashSet::new();
    strip_blocks(
        &mut doc.blocks,
        read_media,
        &kept,
        dial.as_ref(),
        resolve,
        &mut autonym,
    )?;
    renumber_kept_onyms(&mut doc.blocks, &autonym);
    let litos = dendron::serialize(&doc);
    let litos_id = hex(&Sha256::digest(litos.as_bytes()));
    Ok(LitosResult { litos, litos_id })
}

/// The vocabulary-bound genoses of a sim, if any.
fn vocab_genoses<'a>(
    dial: Option<&'a dialektos::Dialektos>,
    symbol: &str,
) -> Option<&'a dialektos::Vocabulary> {
    let dial = dial?;
    let def = dial.sims.get(symbol)?;
    let vocab = def.genos_vocabulary.as_ref()?;
    dial.vocabularies.get(vocab)
}

/// Media lookup resolving parameters relative to a directory
/// (typically the directory containing the `.atk` file).
pub fn media_from_dir(dir: &std::path::Path) -> impl Fn(&str) -> Result<Vec<u8>> + '_ {
    move |param: &str| {
        let path = dir.join(param);
        std::fs::read(&path).map_err(|e| {
            Error::new(ErrorKind::MissingResource(format!(
                "{}: {e}",
                path.display()
            )))
        })
    }
}

/// Collect the onyms referenced by deixes (which survive litosis).
fn collect_deixis_onyms(blocks: &[Block], kept: &mut std::collections::HashSet<String>) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => collect_deixis_onyms_inline(inlines, kept),
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                collect_deixis_onyms_inline(lemma, kept);
                collect_deixis_onyms(children, kept);
                collect_deixis_onyms_inline(hypograph, kept);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                collect_deixis_onyms_inline(lemma, kept);
                for strophe in strophes {
                    for line in &strophe.0 {
                        collect_deixis_onyms_inline(line, kept);
                    }
                }
                collect_deixis_onyms_inline(hypograph, kept);
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                collect_deixis_onyms(children, kept);
            }
            _ => {}
        }
    }
}

fn collect_deixis_onyms_inline(inlines: &[Inline], kept: &mut std::collections::HashSet<String>) {
    for inline in inlines {
        match inline {
            Inline::Deixis { onym, .. } => {
                kept.insert(onym.clone());
            }
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                collect_deixis_onyms_inline(content, kept);
            }
            _ => {}
        }
    }
}

fn strip_blocks(
    blocks: &mut Vec<Block>,
    read_media: &dyn Fn(&str) -> Result<Vec<u8>>,
    kept: &std::collections::HashSet<String>,
    dial: Option<&dialektos::Dialektos>,
    resolve: &dyn Fn(&str) -> Option<dialektos::Dialektos>,
    autonym: &mut std::collections::HashSet<String>,
) -> Result<()> {
    let is_autonym_sim = |dial: Option<&dialektos::Dialektos>, symbol: &str| -> bool {
        dial.and_then(|d| d.sims.get(symbol))
            .is_some_and(|def| matches!(def.form, dialektos::SimForm::Para { autonym: true, .. }))
    };
    let mut i = 0;
    while i < blocks.len() {
        match &mut blocks[i] {
            Block::Paragraph(inlines) => {
                strip_inlines(inlines, read_media)?;
                if inlines.is_empty() {
                    blocks.remove(i);
                    continue;
                }
            }
            Block::Para {
                symbol,
                lemma,
                children,
                hypograph,
                ann,
                ..
            } => {
                let keep_vocab = vocab_genoses(dial, symbol);
                if is_autonym_sim(dial, symbol)
                    && let Some(o) = &ann.onym
                {
                    autonym.insert(o.clone());
                }
                strip_ann_vocab(ann, kept, autonym, keep_vocab);
                strip_inlines(lemma, read_media)?;
                strip_blocks(children, read_media, kept, dial, resolve, autonym)?;
                strip_inlines(hypograph, read_media)?;
            }
            Block::Stichoi {
                symbol,
                lemma,
                strophes,
                hypograph,
                ann,
                ..
            } => {
                let keep_vocab = vocab_genoses(dial, symbol.as_deref().unwrap_or(""));
                if is_autonym_sim(dial, symbol.as_deref().unwrap_or(""))
                    && let Some(o) = &ann.onym
                {
                    autonym.insert(o.clone());
                }
                strip_ann_vocab(ann, kept, autonym, keep_vocab);
                strip_inlines(lemma, read_media)?;
                for strophe in strophes.iter_mut() {
                    for line in &mut strophe.0 {
                        // Stichos whitespace is content (spec:
                        // Whitespace Normalization) - no
                        // re-normalization after stripping.
                        strip_inlines_keep_ws(line, read_media)?;
                    }
                    // A stichos that held only metadata (an onym
                    // anchor, a monosim) has no content left and is
                    // no line: kept empty, it would serialize as a
                    // blank line, a strophe break the document
                    // does not have.
                    strophe.0.retain(|line| !line.is_empty());
                }
                strophes.retain(|strophe| !strophe.0.is_empty());
                strip_inlines(hypograph, read_media)?;
            }
            Block::ParaDiaphane { children, .. } => {
                // Annotations are metadata: the diaphane loses its
                // reason to exist and is unwrapped. (A diaphane is
                // not a dialektos sim and cannot be a deixis
                // target.)
                let mut children = std::mem::take(children);
                strip_blocks(&mut children, read_media, kept, dial, resolve, autonym)?;
                blocks.splice(i..=i, children);
                continue;
            }
            Block::VerbatimBlock { ann, .. } => *ann = Annotations::default(),
            Block::MonadEnglossis {
                dialect,
                children,
                ann,
            } => {
                *ann = Annotations::default();
                // The embedded dialektos's own vocabularies
                // govern its genoses.
                let inner = resolve(dialect);
                strip_blocks(children, read_media, kept, inner.as_ref(), resolve, autonym)?;
            }
            Block::Enmedia { param } => {
                let bytes = read_media(param)?;
                let sha256 = hex(&Sha256::digest(&bytes));
                blocks[i] = Block::EnmediaHashed { sha256 };
            }
            Block::EnmediaHashed { .. } => {}
            Block::AnaphorEnglossis { .. }
            | Block::AnaphorEnlexis { .. }
            | Block::ParaAxioma { .. }
            | Block::AxiomaRefBlock { .. } => {
                return Err(Error::new(ErrorKind::Syntax(
                    "litosis input must be a kanon (unexpanded transclusion found)".into(),
                )));
            }
        }
        i += 1;
    }
    Ok(())
}

/// Clear annotations, keeping the onym only when a deixis
/// references it or it is an autonym (citable identity);
/// genoses that are canonical terms of the sim's bound
/// vocabulary are semantic and survive.
fn strip_ann_vocab(
    ann: &mut Annotations,
    kept: &std::collections::HashSet<String>,
    autonym: &std::collections::HashSet<String>,
    vocab: Option<&dialektos::Vocabulary>,
) {
    let keep = ann
        .onym
        .as_ref()
        .is_some_and(|o| kept.contains(o.as_str()) || autonym.contains(o.as_str()));
    if !keep {
        ann.onym = None;
    }
    match vocab {
        Some(vocab) => ann.genoses.retain(|g| vocab.terms.contains_key(g)),
        None => ann.genoses.clear(),
    }
}

/// Renumber the surviving onyms afresh (`o1`.. in declaration
/// order), independent of the kanon numbering, and rewrite the
/// deixes; stripped metadata must not influence the litos ID.
/// Autonym onyms are exempt: computed from content, they are
/// already canonical (spec v0.12.1).
fn renumber_kept_onyms(blocks: &mut [Block], exempt: &std::collections::HashSet<String>) {
    let mut mapping: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    collect_decls(blocks, exempt, &mut mapping);
    rewrite_kept(blocks, &mapping);
}

fn collect_decls(
    blocks: &[Block],
    exempt: &std::collections::HashSet<String>,
    mapping: &mut std::collections::HashMap<String, String>,
) {
    for block in blocks {
        let (ann, children) = match block {
            Block::Para { children, ann, .. } => (Some(ann), Some(children)),
            Block::Stichoi { ann, .. } => (Some(ann), None),
            Block::MonadEnglossis { children, ann, .. } => (Some(ann), Some(children)),
            _ => (None, None),
        };
        if let Some(ann) = ann
            && let Some(onym) = &ann.onym
            && !exempt.contains(onym)
            && !mapping.contains_key(onym)
        {
            let next = format!("o{}", mapping.len() + 1);
            mapping.insert(onym.clone(), next);
        }
        if let Some(children) = children {
            collect_decls(children, exempt, mapping);
        }
    }
}

fn rewrite_kept(blocks: &mut [Block], mapping: &std::collections::HashMap<String, String>) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => rewrite_kept_inline(inlines, mapping),
            Block::Para {
                lemma,
                children,
                hypograph,
                ann,
                ..
            } => {
                rewrite_kept_ann(ann, mapping);
                rewrite_kept_inline(lemma, mapping);
                rewrite_kept(children, mapping);
                rewrite_kept_inline(hypograph, mapping);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ann,
                ..
            } => {
                rewrite_kept_ann(ann, mapping);
                rewrite_kept_inline(lemma, mapping);
                for strophe in strophes {
                    for line in &mut strophe.0 {
                        rewrite_kept_inline(line, mapping);
                    }
                }
                rewrite_kept_inline(hypograph, mapping);
            }
            Block::MonadEnglossis { children, ann, .. } => {
                rewrite_kept_ann(ann, mapping);
                rewrite_kept(children, mapping);
            }
            _ => {}
        }
    }
}

fn rewrite_kept_ann(ann: &mut Annotations, mapping: &std::collections::HashMap<String, String>) {
    if let Some(onym) = &ann.onym
        && let Some(new) = mapping.get(onym)
    {
        ann.onym = Some(new.clone());
    }
}

fn rewrite_kept_inline(
    inlines: &mut [Inline],
    mapping: &std::collections::HashMap<String, String>,
) {
    for inline in inlines {
        match inline {
            Inline::Deixis { onym, .. } => {
                if let Some(new) = mapping.get(onym) {
                    *onym = new.clone();
                }
            }
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                rewrite_kept_inline(content, mapping);
            }
            _ => {}
        }
    }
}

fn strip_inlines(
    inlines: &mut Vec<Inline>,
    read_media: &dyn Fn(&str) -> Result<Vec<u8>>,
) -> Result<()> {
    strip_inlines_inner(inlines, read_media)?;
    // Removing metadata nodes can leave doubled spaces;
    // re-normalize.
    crate::kanonizo::normalize_inline_seq(inlines);
    Ok(())
}

/// As [`strip_inlines`], but without whitespace re-normalization:
/// for stichos lines, where whitespace is authorial content.
fn strip_inlines_keep_ws(
    inlines: &mut Vec<Inline>,
    read_media: &dyn Fn(&str) -> Result<Vec<u8>>,
) -> Result<()> {
    strip_inlines_inner(inlines, read_media)
}

#[allow(clippy::only_used_in_recursion)]
fn strip_inlines_inner(
    inlines: &mut Vec<Inline>,
    read_media: &dyn Fn(&str) -> Result<Vec<u8>>,
) -> Result<()> {
    let mut i = 0;
    while i < inlines.len() {
        match &mut inlines[i] {
            Inline::Text(_) => {}
            Inline::Endo { content, ann, .. } => {
                *ann = Annotations::default();
                strip_inlines_inner(content, read_media)?;
            }
            Inline::VerbatimInline { ann, .. } => *ann = Annotations::default(),
            Inline::EndoDiaphane { content, .. } => {
                let mut content = std::mem::take(content);
                strip_inlines_inner(&mut content, read_media)?;
                inlines.splice(i..=i, content);
                continue;
            }
            Inline::Monosim { .. } | Inline::OnymAnchor(_) => {
                inlines.remove(i);
                continue;
            }
            Inline::Milestone { ann, .. } => {
                // Coordinates are identity: the milestone survives
                // with scheme and value; its free genoses are
                // presentation hints and strip.
                *ann = Annotations::default();
            }
            Inline::Deixis { ann, .. } => {
                // Deixes survive litosis: attachment is content.
                // Their own annotations are metadata.
                *ann = Annotations::default();
            }
            Inline::EndoAxioma { .. } | Inline::AxiomaRef { .. } => {
                return Err(Error::new(ErrorKind::Syntax(
                    "litosis input must be a kanon (unexpanded axioma found)".into(),
                )));
            }
        }
        i += 1;
    }
    Ok(())
}
