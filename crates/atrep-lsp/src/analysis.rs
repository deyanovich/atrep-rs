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
            let range = e
                .location
                .as_ref()
                .map(|loc| location_range(source, loc.line, loc.col))
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
fn location_range(source: &str, line: usize, col: usize) -> Range {
    let l = line.saturating_sub(1);
    let text = source.lines().nth(l).unwrap_or("");
    let mut start16 = 0u32;
    let mut end16 = 0u32;
    for (i, c) in text.chars().enumerate() {
        if i < col.saturating_sub(1) {
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
