//! LSP adaptation of the core scanner: the semantic-token legend
//! and the protocol's delta encoding. The scanner itself lives in
//! `atrep::scan`.

use atrep::scan::Tok;
use lsp_types::{SemanticToken, SemanticTokenType};

pub use atrep::scan::scan;

/// Token legend, in registration order. `TokKind as u32` indexes it
/// (the core enum's discriminants define this order).
pub const LEGEND: [SemanticTokenType; 7] = [
    SemanticTokenType::NAMESPACE, // 0: declared dialektos id
    SemanticTokenType::KEYWORD,   // 1: sim / episim
    SemanticTokenType::COMMENT,   // 2
    SemanticTokenType::NUMBER,    // 3: taxis params, versions
    SemanticTokenType::VARIABLE,  // 4: onym / general params
    SemanticTokenType::MACRO,     // 5: declaration marker
    SemanticTokenType::OPERATOR,  // 6: escapes, undefined sims
];

/// Delta-encode tokens for `textDocument/semanticTokens/full`.
pub fn encode(mut toks: Vec<Tok>) -> Vec<SemanticToken> {
    toks.sort_by_key(|t| (t.line, t.start));
    let mut out = Vec::with_capacity(toks.len());
    let (mut prev_line, mut prev_start) = (0u32, 0u32);
    for t in toks {
        let delta_line = t.line - prev_line;
        let delta_start = if delta_line == 0 {
            t.start - prev_start
        } else {
            t.start
        };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length: t.len,
            token_type: t.kind as u32,
            token_modifiers_bitset: 0,
        });
        prev_line = t.line;
        prev_start = t.start;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use atrep::scan::TokKind;

    #[test]
    fn delta_encoding() {
        let enc = encode(vec![
            Tok {
                line: 0,
                start: 0,
                len: 4,
                kind: TokKind::Macro,
            },
            Tok {
                line: 0,
                start: 4,
                len: 3,
                kind: TokKind::Namespace,
            },
            Tok {
                line: 2,
                start: 1,
                len: 2,
                kind: TokKind::Keyword,
            },
        ]);
        assert_eq!(
            enc.iter()
                .map(|t| (t.delta_line, t.delta_start, t.length, t.token_type))
                .collect::<Vec<_>>(),
            vec![(0, 0, 4, 5), (0, 4, 3, 0), (2, 1, 2, 1)]
        );
    }
}
