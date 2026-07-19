//! Resolution sources: where dialektos definitions, morphisms,
//! and exomorphoses come from.
//!
//! Resolution semantics (spec: local-only, name-keyed) are
//! independent of storage; this trait is the seam. [`DirSource`]
//! is the classical directory context; [`MemorySource`] serves
//! resolution from an in-memory map — embedded engines, tests,
//! and registry services (including wasm targets with no
//! filesystem) resolve through it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use unicode_normalization::UnicodeNormalization;

use crate::error::{Error, ErrorKind, Result};

/// A named-artifact resolution context. Names are bare file
/// names (`at-html.lektos`, `a.b.hom`, `a.html.exo`).
pub trait Source {
    /// The artifact's text, or `None` when absent. Absence is
    /// not an error; unreadable/undecodable content is.
    fn fetch(&self, name: &str) -> Result<Option<String>>;

    /// The names of every artifact in the context (used for
    /// discovery, e.g. the known-dialektoi scan of transitive
    /// route resolution).
    fn names(&self) -> Vec<String>;
}

/// Fetch and NFC-normalize (all atrep artifact text is compared
/// and parsed in NFC).
pub(crate) fn fetch_normalized(source: &dyn Source, name: &str) -> Result<Option<String>> {
    Ok(source.fetch(name)?.map(|text| text.nfc().collect()))
}

/// A directory on disk — the classical resolution context.
pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        DirSource { root: root.into() }
    }
}

impl Source for DirSource {
    fn fetch(&self, name: &str) -> Result<Option<String>> {
        let path = self.root.join(name);
        if !path.is_file() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(|e| {
            Error::new(ErrorKind::MissingResource(format!(
                "{}: {e}",
                path.display()
            )))
        })?;
        let text = String::from_utf8(bytes).map_err(|_| Error::new(ErrorKind::InvalidUtf8))?;
        Ok(Some(text))
    }

    fn names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                if entry.path().is_file()
                    && let Some(name) = entry.file_name().to_str()
                {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        names
    }
}

/// An in-memory resolution context.
#[derive(Default)]
pub struct MemorySource {
    map: BTreeMap<String, String>,
}

impl MemorySource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert (or replace) an artifact under its file name.
    pub fn insert(&mut self, name: impl Into<String>, text: impl Into<String>) {
        self.map.insert(name.into(), text.into());
    }

    /// Remove an artifact by name.
    pub fn remove(&mut self, name: &str) {
        self.map.remove(name);
    }
}

impl Source for MemorySource {
    fn fetch(&self, name: &str) -> Result<Option<String>> {
        Ok(self.map.get(name).cloned())
    }

    fn names(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }
}
