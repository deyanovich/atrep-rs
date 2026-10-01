//! # atrep
//!
//! Pilot implementation of the Atrep core: parser, Dendron (AST),
//! kanonizo (canonicalization), and litosis (content hashing).
//!
//! Implements the Atreptos Platform Specification **Draft v0.10**
//! (spec repository `atrep/spec`, tag `v0.10`, commit
//! `dea21f970e88d19e74fe410ba2628b566f503db2`). See the repository
//! README for the pilot's documented limitations and the spec-gap
//! notes discovered while implementing.

pub mod atramento;
pub mod dendron;
pub mod dialektos;
pub mod endo;
pub mod epimerismos;
pub mod error;
pub mod exo;
pub mod fb2;
pub mod fetch;
pub mod glossa;
pub mod kanonizo;
pub mod litosis;
pub mod morph;
pub mod outline;
pub mod parser;
pub mod scan;
pub mod sigil;
pub mod source;
pub mod zygosis;

pub use dendron::Document;
pub use error::{Error, ErrorKind, Result};

use std::path::Path;
use unicode_normalization::UnicodeNormalization;

/// Parse and validate a document (the `atrep check` operation).
pub fn check_file(path: &Path) -> Result<Document> {
    let bytes = std::fs::read(path).map_err(|e| {
        Error::new(ErrorKind::MissingResource(format!(
            "{}: {e}",
            path.display()
        )))
    })?;
    let source = String::from_utf8(bytes).map_err(|_| Error::new(ErrorKind::InvalidUtf8))?;
    let normalized: String = source.nfc().collect();
    parser::parse_document(&normalized, path)
}

/// Result of [`check_any`]: a document or a dialektos definition.
#[derive(Debug)]
pub enum Checked {
    Document(Document),
    Dialektos(dialektos::Dialektos),
}

/// True when the source's first content line (after an optional
/// shebang) declares the `atrep` meta-dialektos, i.e. the file is a
/// dialektos definition rather than a document.
pub fn is_definition_source(source: &str) -> bool {
    let mut lines = source.lines();
    let mut first = lines.next().unwrap_or("");
    if first.starts_with("#!") {
        first = lines.next().unwrap_or("");
    }
    let decl = first.trim();
    let rest = if let Some(r) = decl.strip_prefix("@@@!") {
        r
    } else if let Some(r) = decl.strip_prefix("\\\\\\!") {
        r
    } else {
        return false;
    };
    // Ignore a `@<version>` suffix on the declared identifier.
    let id = match rest.find('@') {
        Some(i) => &rest[..i],
        None => rest,
    };
    id == "atrep"
}

/// Parse and validate either a document or a dialektos definition
/// file, routing on the declaration.
pub fn check_any(path: &Path) -> Result<Checked> {
    let bytes = std::fs::read(path).map_err(|e| {
        Error::new(ErrorKind::MissingResource(format!(
            "{}: {e}",
            path.display()
        )))
    })?;
    let source = String::from_utf8(bytes).map_err(|_| Error::new(ErrorKind::InvalidUtf8))?;
    check_source(&source, path)
}

/// Parse and validate either a document or a dialektos definition
/// from in-memory source, routing on the declaration. `path` is
/// used for dialektos/import resolution (the parent directory) and
/// error locations; the file is not read. This is the entry point
/// for callers holding unsaved buffers (editors, the LSP).
pub fn check_source(source: &str, path: &Path) -> Result<Checked> {
    if is_definition_source(source) {
        dialektos::parse_source(source, path).map(Checked::Dialektos)
    } else {
        let normalized: String = source.nfc().collect();
        parser::parse_document(&normalized, path).map(Checked::Document)
    }
}

/// Export through a built-in exomorphosis, for targets whose
/// output the template language cannot produce (generated ids,
/// hoisted metadata, adjacency-derived attributes): FictionBook
/// (`fb2`), the Russian National Corpus (`rnc`), OpenCorpora
/// (`opencorpora`) and PROIEL (`proiel`). None for any other
/// target, which resolves an `.exo` as usual.
pub fn native_export(doc: &Document, target: &str) -> Option<Result<String>> {
    match target {
        "fb2" => Some(Ok(fb2::document_to_fb2(doc))),
        "rnc" | "opencorpora" | "proiel" | "conllu" => epimerismos::export(doc, target),
        _ => None,
    }
}

/// [`native_export`] with the document's directory, from which
/// the FB2 export reads the media its images refer to.
pub fn native_export_in(doc: &Document, target: &str, dir: &Path) -> Option<Result<String>> {
    match target {
        "fb2" => Some(Ok(fb2::document_to_fb2_with(doc, &|param| {
            std::fs::read(dir.join(param)).ok()
        }))),
        _ => native_export(doc, target),
    }
}
