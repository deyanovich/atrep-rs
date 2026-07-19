//! Lexical scanner for syntax highlighting (semantic tokens).
//!
//! Consumed by `atrep-lsp` (LSP semantic tokens) and the Atramento
//! editor (CodeMirror decorations over Tauri IPC).
//!
//! Line-based, mirroring the shapes atrep's parser recognizes
//! (declaration, sigil runs + sim symbols, monosim parameters,
//! comments, escapes, episims) without building a Dendron. When the
//! document's dialektos resolves, sim symbols are checked against
//! it; otherwise every well-formed sim shape highlights the same.

use crate::dialektos::Dialektos;
use crate::sigil;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokKind {
    Namespace = 0,
    Keyword = 1,
    Comment = 2,
    Number = 3,
    Variable = 4,
    Macro = 5,
    Operator = 6,
}

impl TokKind {
    /// Stable lowercase name (CSS-class-friendly).
    pub fn name(self) -> &'static str {
        match self {
            TokKind::Namespace => "namespace",
            TokKind::Keyword => "keyword",
            TokKind::Comment => "comment",
            TokKind::Number => "number",
            TokKind::Variable => "variable",
            TokKind::Macro => "macro",
            TokKind::Operator => "operator",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Tok {
    pub line: u32,
    /// Start column in UTF-16 code units.
    pub start: u32,
    /// Length in UTF-16 code units.
    pub len: u32,
    pub kind: TokKind,
}

/// One source line as (char, utf16-column) pairs.
struct Cols {
    chars: Vec<char>,
    /// utf16 column at each char index; one extra entry = line end.
    cols: Vec<u32>,
}

impl Cols {
    fn new(line: &str) -> Self {
        let chars: Vec<char> = line.chars().collect();
        let mut cols = Vec::with_capacity(chars.len() + 1);
        let mut c16 = 0u32;
        for &c in &chars {
            cols.push(c16);
            c16 += c.len_utf16() as u32;
        }
        cols.push(c16);
        Cols { chars, cols }
    }
    fn len(&self) -> usize {
        self.chars.len()
    }
    fn end16(&self) -> u32 {
        *self.cols.last().unwrap()
    }
}

pub struct Scanner<'a> {
    dial: Option<&'a Dialektos>,
    active: char,
    /// Open multi-line comment: the opener's sigil-run length.
    in_comment: Option<usize>,
    toks: Vec<Tok>,
}

/// Scan a whole document (or definition) into semantic tokens.
pub fn scan(source: &str, dial: Option<&Dialektos>) -> Vec<Tok> {
    let mut sc = Scanner {
        dial,
        active: active_sigil(source),
        in_comment: None,
        toks: Vec::new(),
    };
    let mut decl_seen = false;
    for (ln, raw) in source.lines().enumerate() {
        let line = ln as u32;
        let cols = Cols::new(raw);
        if ln == 0 && raw.starts_with("#!") {
            sc.push(line, 0, cols.end16(), TokKind::Comment);
            continue;
        }
        if !decl_seen && !raw.trim().is_empty() {
            decl_seen = true;
            if sc.scan_declaration(line, &cols) {
                continue;
            }
        }
        sc.scan_line(line, &cols);
    }
    sc.toks
}

/// The dialektos identifier on the declaration line, without any
/// `@version` suffix; `None` when the source has no declaration.
pub fn declared_id(source: &str) -> Option<String> {
    let mut lines = source.lines();
    let mut first = lines.next().unwrap_or("");
    if first.starts_with("#!") {
        first = lines.next().unwrap_or("");
    }
    for l in std::iter::once(first).chain(lines) {
        let t = l.trim();
        if t.is_empty() {
            continue;
        }
        let rest = t
            .strip_prefix("@@@!")
            .or_else(|| t.strip_prefix("\\\\\\!"))?;
        let id = rest
            .split('@')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("");
        return (!id.is_empty()).then(|| id.to_string());
    }
    None
}

/// The active sigil declared on the first content line (canonical
/// `@` when absent or canonical-declared).
fn active_sigil(source: &str) -> char {
    let mut lines = source.lines();
    let mut first = lines.next().unwrap_or("");
    if first.starts_with("#!") {
        first = lines.next().unwrap_or("");
    }
    for l in std::iter::once(first).chain(lines) {
        let t = l.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("\\\\\\!") {
            return sigil::ALIAS;
        }
        break;
    }
    sigil::CANONICAL
}

impl Scanner<'_> {
    fn push(&mut self, line: u32, start: u32, len: u32, kind: TokKind) {
        if len > 0 {
            self.toks.push(Tok {
                line,
                start,
                len,
                kind,
            });
        }
    }

    /// Declaration line `@@@!id[@version]`. Returns true when the
    /// line is one (and has been tokenized).
    fn scan_declaration(&mut self, line: u32, cols: &Cols) -> bool {
        let n = cols.len();
        // skip indentation
        let mut i = 0;
        while i < n && cols.chars[i].is_whitespace() {
            i += 1;
        }
        if i + 4 > n {
            return false;
        }
        let marker: String = cols.chars[i..i + 4].iter().collect();
        let is_decl = marker == "@@@!" || marker == "\\\\\\!";
        if !is_decl {
            return false;
        }
        self.push(line, cols.cols[i], 4, TokKind::Macro);
        let mut j = i + 4;
        let id_start = j;
        while j < n && cols.chars[j] != '@' && !cols.chars[j].is_whitespace() {
            j += 1;
        }
        self.push(
            line,
            cols.cols[id_start],
            cols.cols[j] - cols.cols[id_start],
            TokKind::Namespace,
        );
        if j < n && cols.chars[j] == '@' {
            let v_start = j;
            while j < n && !cols.chars[j].is_whitespace() {
                j += 1;
            }
            self.push(
                line,
                cols.cols[v_start],
                cols.cols[j] - cols.cols[v_start],
                TokKind::Number,
            );
        }
        true
    }

    fn scan_line(&mut self, line: u32, cols: &Cols) {
        let n = cols.len();
        let mut i = 0;

        // Continue an open multi-line comment: look for `/` + run.
        if let Some(run) = self.in_comment {
            match self.find_comment_close(cols, 0, run) {
                Some(close_end) => {
                    self.push(line, 0, cols.cols[close_end], TokKind::Comment);
                    self.in_comment = None;
                    i = close_end;
                }
                None => {
                    self.push(line, 0, cols.end16(), TokKind::Comment);
                    return;
                }
            }
        }

        while i < n {
            let c = cols.chars[i];
            if c == self.active {
                i = self.scan_sigil_run(line, cols, i);
            } else if c == inactive(self.active) {
                // escape: inactive sigil + next char
                let end = (i + 2).min(n);
                self.push(
                    line,
                    cols.cols[i],
                    cols.cols[end] - cols.cols[i],
                    TokKind::Operator,
                );
                i = end;
            } else if sigil::is_symbolic(c) {
                // potential episim: symbolic run followed by sigils
                let sym_start = i;
                let mut j = i;
                while j < n && sigil::is_symbolic(cols.chars[j]) {
                    j += 1;
                }
                if j < n && cols.chars[j] == self.active {
                    let mut k = j;
                    while k < n && cols.chars[k] == self.active {
                        k += 1;
                    }
                    let symbol: String = cols.chars[sym_start..j].iter().collect();
                    let kind = self.classify_episim(&symbol);
                    self.push(
                        line,
                        cols.cols[sym_start],
                        cols.cols[k] - cols.cols[sym_start],
                        kind,
                    );
                    i = k;
                } else {
                    i = j;
                }
            } else {
                i += 1;
            }
        }
    }

    /// From a sigil at `i`: run, then comment / sim symbol / param.
    /// Returns the next scan position.
    fn scan_sigil_run(&mut self, line: u32, cols: &Cols, i: usize) -> usize {
        let n = cols.len();
        let start = i;
        let mut j = i;
        while j < n && cols.chars[j] == self.active {
            j += 1;
        }
        let run = j - i;

        // comment: run + '/'
        if j < n && cols.chars[j] == '/' {
            match self.find_comment_close(cols, j + 1, run) {
                Some(close_end) => {
                    self.push(
                        line,
                        cols.cols[start],
                        cols.cols[close_end] - cols.cols[start],
                        TokKind::Comment,
                    );
                    return close_end;
                }
                None => {
                    self.push(
                        line,
                        cols.cols[start],
                        cols.end16() - cols.cols[start],
                        TokKind::Comment,
                    );
                    if run >= 2 {
                        self.in_comment = Some(run);
                    }
                    return n;
                }
            }
        }

        // sim symbol: longest defined match when the dialektos is
        // known; otherwise a symbolic run stopping at `(` (the
        // parameter opener).
        let (sym_len, kind) = match self.dial {
            Some(d) => {
                let rest: String = cols.chars[j..].iter().collect();
                match d.longest_match(&rest) {
                    Some(def) => (def.symbol.chars().count(), TokKind::Keyword),
                    None => (heuristic_symbol_len(&cols.chars[j..]), TokKind::Operator),
                }
            }
            None => (heuristic_symbol_len(&cols.chars[j..]), TokKind::Keyword),
        };
        j += sym_len;
        let kind = if sym_len == 0 {
            TokKind::Operator
        } else {
            kind
        };
        self.push(
            line,
            cols.cols[start],
            cols.cols[j] - cols.cols[start],
            kind,
        );

        // optional parameter `(...)`
        if j < n && cols.chars[j] == '(' {
            let p_start = j + 1;
            let mut k = p_start;
            while k < n && cols.chars[k] != ')' {
                k += 1;
            }
            if k < n {
                let kind = if cols.chars[p_start..k]
                    .first()
                    .is_some_and(|c| c.is_ascii_digit())
                {
                    TokKind::Number
                } else {
                    TokKind::Variable
                };
                self.push(
                    line,
                    cols.cols[p_start],
                    cols.cols[k] - cols.cols[p_start],
                    kind,
                );
                return k + 1;
            }
        }
        j
    }

    /// Find `/` + sigil-run(len) starting at char index `from`;
    /// returns the char index just past the closer.
    fn find_comment_close(&self, cols: &Cols, from: usize, run: usize) -> Option<usize> {
        let n = cols.len();
        let mut i = from;
        while i < n {
            if cols.chars[i] == '/' {
                let mut k = i + 1;
                while k < n && cols.chars[k] == self.active && k - i - 1 < run {
                    k += 1;
                }
                if k - i - 1 == run {
                    return Some(k);
                }
            }
            i += 1;
        }
        None
    }

    fn classify_episim(&self, symbol: &str) -> TokKind {
        match self.dial {
            Some(d) => {
                let matches = d.sims.values().any(|s| s.episymbol() == symbol);
                if matches {
                    TokKind::Keyword
                } else {
                    TokKind::Operator
                }
            }
            None => TokKind::Keyword,
        }
    }
}

/// Without a dialektos: symbolic chars up to (not including) the
/// parameter opener `(`.
fn heuristic_symbol_len(chars: &[char]) -> usize {
    chars
        .iter()
        .take_while(|&&c| sigil::is_symbolic(c) && c != '(')
        .count()
}

fn inactive(active: char) -> char {
    if active == sigil::CANONICAL {
        sigil::ALIAS
    } else {
        sigil::CANONICAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<(u32, u32, u32, TokKind)> {
        scan(source, None)
            .into_iter()
            .map(|t| (t.line, t.start, t.len, t.kind))
            .collect()
    }

    #[test]
    fn declaration_tokens() {
        let toks = kinds("@@@!litogramma@0.3\n");
        assert_eq!(toks[0], (0, 0, 4, TokKind::Macro));
        assert_eq!(toks[1], (0, 4, 10, TokKind::Namespace));
        assert_eq!(toks[2], (0, 14, 4, TokKind::Number));
    }

    #[test]
    fn para_sim_with_taxis_and_close() {
        let src = "@@@!litogramma\n\n@#(1) Title\ntext\n#@\n";
        let toks = kinds(src);
        // line 2: sim `@#` + number param `1`; line 4: episim `#@`
        assert!(toks.contains(&(2, 0, 2, TokKind::Keyword)));
        assert!(toks.contains(&(2, 3, 1, TokKind::Number)));
        assert!(toks.contains(&(4, 0, 2, TokKind::Keyword)));
    }

    #[test]
    fn onym_param_is_variable() {
        let toks = kinds("@@@!x\n\n@=(sec:intro)\n");
        assert!(toks.contains(&(2, 3, 9, TokKind::Variable)));
    }

    #[test]
    fn escapes_are_operators() {
        let toks = kinds("@@@!x\n\nprice \\@ two\n");
        assert!(toks.contains(&(2, 6, 2, TokKind::Operator)));
    }

    #[test]
    fn single_line_comment() {
        let toks = kinds("@@@!x\n\n@@/ note /@@\n");
        assert!(toks.contains(&(2, 0, 12, TokKind::Comment)));
    }

    #[test]
    fn multi_line_comment() {
        let src = "@@@!x\n\n@@/ open\nstill inside\ndone /@@ tail\n";
        let toks = kinds(src);
        assert!(toks.contains(&(2, 0, 8, TokKind::Comment)));
        assert!(toks.contains(&(3, 0, 12, TokKind::Comment)));
        assert!(toks.contains(&(4, 0, 8, TokKind::Comment)));
    }

    #[test]
    fn alias_sigil_documents() {
        let src = "\\\\\\!x\n\n\\#(1) Title\n";
        let toks = kinds(src);
        assert!(toks.contains(&(2, 0, 2, TokKind::Keyword)));
        assert!(toks.contains(&(2, 3, 1, TokKind::Number)));
    }

    #[test]
    fn utf16_columns() {
        // '𝔸' is 2 UTF-16 units; the sim after it must account for that.
        let toks = kinds("@@@!x\n\n𝔸 @,(n)\n");
        assert!(toks.contains(&(2, 3, 2, TokKind::Keyword)));
    }
}
