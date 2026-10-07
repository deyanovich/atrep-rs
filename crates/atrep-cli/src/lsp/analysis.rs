//! Per-document analysis: diagnostics via atrep, dialektos
//! resolution for the scanner.

use std::path::{Path, PathBuf};

use atrep::dialektos::{self, Dialektos};
use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use unicode_normalization::UnicodeNormalization;

/// Parse the buffer (document or definition, routed on the
/// declaration) and map the error, if any, to LSP diagnostics.
pub fn diagnostics(source: &str, path: &Path) -> Vec<Diagnostic> {
    match atrep::check_source(source, path) {
        Ok(_) => Vec::new(),
        Err(e) => {
            // check_source parses the NFC form of a document, so
            // its column counts NFC characters; a definition is
            // parsed as is.
            let nfc = !atrep::is_definition_source(source);
            let range = e
                .location
                .as_ref()
                .map(|loc| location_range(source, loc.line, loc.col, nfc))
                .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
            vec![Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("atrep".into()),
                message: e.to_string(),
                ..Default::default()
            }]
        }
    }
}

/// Range for a 1-based (line, col) error location: from the column
/// to the end of that line (the core reports positions, not spans).
/// With `nfc`, `col` counts characters of the line's NFC form and
/// is mapped back onto the buffer's own characters.
fn location_range(source: &str, line: usize, col: usize, nfc: bool) -> Range {
    let l = line.saturating_sub(1);
    let text = source.lines().nth(l).unwrap_or("");
    let start = if nfc {
        buffer_index(text, col.saturating_sub(1))
    } else {
        col.saturating_sub(1)
    };
    let mut start16 = 0u32;
    let mut end16 = 0u32;
    for (i, c) in text.chars().enumerate() {
        if i < start {
            start16 += c.len_utf16() as u32;
        }
        end16 += c.len_utf16() as u32;
    }
    if end16 <= start16 {
        start16 = 0;
    }
    Range::new(
        Position::new(l as u32, start16),
        Position::new(l as u32, end16),
    )
}

/// The index of the buffer character holding NFC character
/// `nfc_index` of `text`'s NFC form. The normalization is replayed
/// segment by segment — a segment runs from one starter (combining
/// class 0) to the next, so each normalizes on its own — and the
/// segment that produces the target character is answered by its
/// first buffer character.
fn buffer_index(text: &str, nfc_index: usize) -> usize {
    use unicode_normalization::char::canonical_combining_class;
    let mut starts: Vec<(usize, usize)> = text
        .char_indices()
        .enumerate()
        .filter(|(i, (_, c))| *i == 0 || canonical_combining_class(*c) == 0)
        .map(|(i, (byte, _))| (byte, i))
        .collect();
    let total = text.chars().count();
    starts.push((text.len(), total));
    let mut produced = 0usize;
    for w in starts.windows(2) {
        let n = text[w[0].0..w[1].0].nfc().count();
        if produced + n > nfc_index {
            return w[0].1;
        }
        produced += n;
    }
    total
}

/// Resolve the buffer's declared dialektos (documents only;
/// definitions highlight structurally). NFC-normalized like the
/// parser input; resolution is local-to-the-file's-directory plus
/// the embedded std set, exactly as atrep resolves it.
pub fn resolve_dialektos(source: &str, path: &Path) -> Option<Dialektos> {
    if atrep::is_definition_source(source) {
        return None;
    }
    let id = atrep::scan::declared_id(source)?;
    let normalized: String = id.nfc().collect();
    let dir: PathBuf = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    dialektos::resolve(&dir, &normalized).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_litogramma_document() {
        let src = "@@@!litogramma\n\n@#(1) Title\ntext\n#@\n";
        assert!(diagnostics(src, Path::new("t.atd")).is_empty());
    }

    #[test]
    fn missing_declaration_is_flagged() {
        let d = diagnostics("just prose\n", Path::new("t.atd"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].range.start.line, 0);
    }

    #[test]
    fn unresolvable_dialektos_is_flagged() {
        let d = diagnostics("@@@!no-such-dialekt\n", Path::new("t.atd"));
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn std_dialektos_resolves() {
        let src = "@@@!litogramma\n\ntext\n";
        assert!(resolve_dialektos(src, Path::new("t.atd")).is_some());
    }

    #[test]
    fn definition_source_gets_no_dialektos() {
        let src = "@@@!atrep\n\n@=== note\n";
        assert!(resolve_dialektos(src, Path::new("x.dia")).is_none());
    }
}
