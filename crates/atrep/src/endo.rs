//! Endomorphosis (pilot): hardcoded importers into the
//! standard-library dialektoi.
//!
//! The `.endo` file format is not specified by the spec; per the
//! spec, endomorphosis engines are implementation-side. Two
//! importers live here:
//!
//! - Markdown into `at-markdown`: the CommonMark subset that
//!   at-markdown models - ATX headings, paragraphs, blockquotes,
//!   flat list items, fenced code (info string becomes a genos),
//!   standalone images, emphasis/strong, code spans, backslash
//!   escapes, and the footnote extension (`[^name]` callouts as
//!   deixes, `[^name]: body` definitions as footnote bodies).
//! - HTML into `at-html`: the canonical subset the at-html syntax
//!   mapper models - h1-h6, p, em, strong, span/div (class maps
//!   to genoses), blockquote, ul/ol/li, pre>code, inline code,
//!   img. Strict: unknown elements are errors. The inverse of the
//!   at-html exomorphosis over this subset.
//! - reStructuredText into `at-rst`: underlined titles (fixed
//!   adornment table = - ~ ^ " ' for levels 1-6, a canonical-
//!   subset deviation from docutils's order-of-first-use), note
//!   and warning admonitions, image directives, footnotes
//!   (`.. [#name]` bodies and `[#name]_` callouts as deixes),
//!   field lists, bullet and enumerated items, definition items,
//!   literal blocks, indented blockquotes, em/strong, and
//!   double-backtick literals.

use crate::dendron::{Annotations, Block, Document, Inline, Strophe, Taxis};
use crate::error::Result;

/// The deepest nesting an importer follows before refusing the
/// document. Block containers (quotes, items, divisions,
/// sections) and inline spans each recurse once per level; an
/// unbounded ladder would overflow the stack instead of failing.
/// The bound counts frames, so it is sized to the largest
/// importer frame of a debug build (Org's, near 19 KiB) on a
/// 2 MiB thread (the test default), with room to spare.
const MAX_NESTING: usize = 64;

thread_local! {
    /// The current importer nesting depth; every recursive
    /// importer function holds a [`Nesting`] for its frame.
    static NESTING: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One level of importer nesting, released when dropped.
struct Nesting;

impl Drop for Nesting {
    fn drop(&mut self) {
        NESTING.with(|c| c.set(c.get() - 1));
    }
}

/// Enter one nesting level, or fail (through the importer's own
/// error constructor) past [`MAX_NESTING`].
fn descend(err: fn(String) -> Error) -> Result<Nesting> {
    let depth = NESTING.with(|c| c.get());
    if depth >= MAX_NESTING {
        return Err(err(format!("nesting deeper than {MAX_NESTING} levels")));
    }
    NESTING.with(|c| c.set(depth + 1));
    Ok(Nesting)
}

fn md_err(msg: String) -> Error {
    Error::new(ErrorKind::Syntax(format!("markdown import: {msg}")))
}

/// Import Markdown text as an `at-markdown` document.
pub fn markdown_to_document(md: &str) -> Result<Document> {
    let lines: Vec<&str> = md.lines().collect();
    let blocks = parse_blocks(&lines)?;
    Ok(Document {
        dialect_id: "at-markdown".to_string(),
        dialect_version: None,
        blocks,
    })
}

fn parse_blocks(lines: &[&str]) -> Result<Vec<Block>> {
    let _depth = descend(md_err)?;
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Fenced code block; the info string becomes a genos. A
        // longer fence holds shorter ones as content.
        if line.starts_with("```") {
            let ticks = line.chars().take_while(|&c| c == '`').count();
            let info = line[ticks..].trim().to_string();
            let start = i + 1;
            let mut end = start;
            while end < lines.len() && !closes_fence(lines[end], ticks) {
                end += 1;
            }
            let mut content = lines[start..end].join("\n");
            content.push('\n');
            let genoses = if info.is_empty() { vec![] } else { vec![info] };
            blocks.push(Block::VerbatimBlock {
                content,
                ann: Annotations {
                    onym: None,
                    genoses,
                },
            });
            i = if end < lines.len() { end + 1 } else { end };
            continue;
        }

        // ATX heading.
        if let Some((level, rest)) = atx_heading(line) {
            blocks.push(Block::Paragraph(vec![Inline::Endo {
                symbol: "#".repeat(level),
                content: parse_inline(rest.trim()),
                bracket_matching: true,
                ann: Annotations::default(),
            }]));
            i += 1;
            continue;
        }

        // Blockquote: consecutive `>`-prefixed lines.
        if line.starts_with('>') {
            let start = i;
            while i < lines.len() && lines[i].starts_with('>') {
                i += 1;
            }
            let inner: Vec<&str> = lines[start..i]
                .iter()
                .map(|l| {
                    l.strip_prefix("> ")
                        .or_else(|| l.strip_prefix('>'))
                        .unwrap_or(l)
                })
                .collect();
            blocks.push(Block::Para {
                symbol: ">".to_string(),
                taxis: None,
                lemma: vec![],
                children: parse_blocks(&inner)?,
                hypograph: vec![],
                bracket_matching: false,
                ann: Annotations::default(),
            });
            continue;
        }

        // Unordered list item: `- ` line plus two-space-indented
        // continuation lines.
        if let Some(rest) = line.strip_prefix("- ") {
            let (children, next) = item_content(lines, i, rest, "  ")?;
            blocks.push(Block::Para {
                symbol: "-".to_string(),
                taxis: None,
                lemma: vec![],
                children,
                hypograph: vec![],
                bracket_matching: true,
                ann: Annotations::default(),
            });
            i = next;
            continue;
        }

        // Ordered list item: `N. ` line plus three-space-indented
        // continuation lines.
        if let Some((n, rest)) = ordered_marker(line) {
            let (children, next) = item_content(lines, i, rest, "   ")?;
            blocks.push(Block::Para {
                symbol: ".".to_string(),
                taxis: Some(Taxis::Explicit(n)),
                lemma: vec![],
                children,
                hypograph: vec![],
                bracket_matching: true,
                ann: Annotations::default(),
            });
            i = next;
            continue;
        }

        // Footnote definition `[^name]: body` (the footnote
        // extension); continuation lines indent four spaces.
        if let Some(after) = line.strip_prefix("[^")
            && let Some((name, body)) = after.split_once("]: ")
            && !name.is_empty()
        {
            let (children, next) = item_content(lines, i, body, "    ")?;
            blocks.push(Block::Para {
                symbol: "^".to_string(),
                taxis: None,
                lemma: vec![],
                children,
                hypograph: vec![],
                bracket_matching: true,
                ann: Annotations {
                    onym: Some(name.to_string()),
                    genoses: vec![],
                },
            });
            i = next;
            continue;
        }

        // Standalone image: enmedia.
        if let Some(target) = line
            .trim_end()
            .strip_prefix("![](")
            .and_then(|r| r.strip_suffix(')'))
        {
            blocks.push(Block::Enmedia {
                param: target.to_string(),
            });
            i += 1;
            continue;
        }

        // Paragraph: consecutive plain lines, soft breaks joined
        // with spaces.
        let start = i;
        while i < lines.len() && !lines[i].trim().is_empty() && !is_block_start(lines[i]) {
            i += 1;
        }
        let text = lines[start..i].join(" ");
        blocks.push(Block::Paragraph(parse_inline(&text)));
    }
    Ok(blocks)
}

/// Collect a list item's content: the marker line's remainder plus
/// continuation lines carrying the given indent.
fn item_content(
    lines: &[&str],
    i: usize,
    first: &str,
    indent: &str,
) -> Result<(Vec<Block>, usize)> {
    let mut inner: Vec<String> = vec![first.to_string()];
    let mut j = i + 1;
    while j < lines.len() {
        let line = lines[j];
        if let Some(rest) = line.strip_prefix(indent) {
            inner.push(rest.to_string());
        } else if line.trim().is_empty()
            && lines
                .get(j + 1)
                .is_some_and(|next| next.starts_with(indent))
        {
            inner.push(String::new());
        } else {
            break;
        }
        j += 1;
    }
    let refs: Vec<&str> = inner.iter().map(String::as_str).collect();
    Ok((parse_blocks(&refs)?, j))
}

/// Whether `line` closes a code fence opened with `ticks`
/// backticks: a run of at least as many, alone on its line.
fn closes_fence(line: &str, ticks: usize) -> bool {
    let run = line.trim_end();
    run.len() >= ticks && run.chars().all(|c| c == '`')
}

fn is_block_start(line: &str) -> bool {
    line.starts_with("```")
        || atx_heading(line).is_some()
        || line.starts_with('>')
        || line.starts_with("- ")
        || ordered_marker(line).is_some()
}

fn atx_heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&level)
        && let Some(rest) = line[level..].strip_prefix(' ')
    {
        return Some((level, rest));
    }
    None
}

fn ordered_marker(line: &str) -> Option<(u64, &str)> {
    let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = line[digits.len()..].strip_prefix(". ")?;
    Some((digits.parse().ok()?, rest))
}

/// Parse inline Markdown: code spans, strong, emphasis, backslash
/// escapes. Unclosed delimiters fall back to literal text.
/// The visible-URL link endo: the grammata IS the displayed
/// target (the link sim's own doctrine). Hidden-href sources
/// project to prose text with this beside it, pending F9.
fn link_endo(url: String) -> Inline {
    Inline::Endo {
        symbol: "><".to_string(),
        content: vec![Inline::Text(url)],
        bracket_matching: true,
        ann: Annotations::default(),
    }
}

/// A CommonMark-shaped autolink at `chars[open] == '<'`: an
/// absolute URI (ASCII-alphabetic scheme, then `:`), no
/// whitespace or `<` inside, closed by `>`. Returns the URI and
/// the index past the closing `>`.
fn autolink_target(finder: &mut Finder, open: usize) -> Option<(String, usize)> {
    let chars = finder.chars;
    let close = finder.find(open + 1, &['>'])?;
    let inner: String = chars[open + 1..close].iter().collect();
    if inner.is_empty() || inner.chars().any(|c| c.is_whitespace() || c == '<') {
        return None;
    }
    let (scheme, rest) = inner.split_once(':')?;
    let mut sc = scheme.chars();
    if !sc.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !sc.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        || rest.is_empty()
    {
        return None;
    }
    Some((inner, close + 1))
}

fn parse_inline(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut finder = Finder::new(&chars);
    let mut closers: ScanMemo<usize> = ScanMemo::new();
    let mut inlines: Vec<Inline> = Vec::new();
    let mut lit = String::new();
    let mut i = 0;

    let flush = |lit: &mut String, inlines: &mut Vec<Inline>| {
        if !lit.is_empty() {
            inlines.push(Inline::Text(std::mem::take(lit)));
        }
    };

    while i < chars.len() {
        let c = chars[i];
        // Backslash escape of ASCII punctuation.
        if c == '\\'
            && let Some(&next) = chars.get(i + 1)
            && next.is_ascii_punctuation()
        {
            lit.push(next);
            i += 2;
            continue;
        }
        // Code span.
        if c == '`'
            && let Some(close) = finder.find(i + 1, &['`'])
        {
            flush(&mut lit, &mut inlines);
            inlines.push(Inline::VerbatimInline {
                content: chars[i + 1..close].iter().collect(),
                ann: Annotations::default(),
            });
            i = close + 1;
            continue;
        }
        // Footnote callout `[^name]` (the footnote extension).
        if c == '['
            && chars.get(i + 1) == Some(&'^')
            && let Some(close) = finder.find(i + 2, &[']'])
        {
            flush(&mut lit, &mut inlines);
            inlines.push(Inline::Deixis {
                symbol: "^".to_string(),
                onym: chars[i + 2..close].iter().collect(),
                ann: Annotations::default(),
            });
            i = close + 1;
            continue;
        }
        // Autolink `<URL>` — the visible-URL link: the target IS
        // the display, exactly the link sim's shape.
        if c == '<'
            && let Some((url, next)) = autolink_target(&mut finder, i)
        {
            flush(&mut lit, &mut inlines);
            inlines.push(link_endo(url));
            i = next;
            continue;
        }
        // Inline link `[text](url)`: a hidden href. The faithful
        // form waits on F9; meanwhile the org projection applies —
        // the text stays prose with the visible URL beside it,
        // `text (url)`, and nothing is lost. (An image's `![` is
        // not a link; nested brackets are outside the subset.)
        if c == '['
            && (i == 0 || chars[i - 1] != '!')
            && let Some(close) = finder.find(i + 1, &[']'])
            && chars.get(close + 1) == Some(&'(')
            && let Some(end) = finder.find(close + 2, &[')'])
        {
            let text: String = chars[i + 1..close].iter().collect();
            let url: String = chars[close + 2..end]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            flush(&mut lit, &mut inlines);
            if url.is_empty() {
                inlines.extend(parse_inline(&text));
            } else if text.trim().is_empty() || text.trim() == url {
                inlines.push(link_endo(url));
            } else {
                inlines.extend(parse_inline(&text));
                inlines.push(Inline::Text(" (".to_string()));
                inlines.push(link_endo(url));
                inlines.push(Inline::Text(")".to_string()));
            }
            i = end + 1;
            continue;
        }
        // Strong, then emphasis; a triple run is emphasis around
        // strong (and falls back to strong when it has no triple
        // closer).
        if c == '*' {
            let run = chars[i..]
                .iter()
                .take(3)
                .take_while(|&&ch| ch == '*')
                .count();
            let tried = if run == 3 { 0..2 } else { 3 - run..4 - run };
            if let Some((delim, close)) = STAR_RUNS[tried]
                .iter()
                .find_map(|&d| Some((d, star_close(&chars, &mut closers, i, d)?)))
            {
                flush(&mut lit, &mut inlines);
                let inner: String = chars[i + delim.len()..close].iter().collect();
                let symbols: &[&str] = match delim.len() {
                    3 => &["**", "*"],
                    2 => &["**"],
                    _ => &["*"],
                };
                let mut content = parse_inline(&inner);
                for symbol in symbols {
                    content = vec![Inline::Endo {
                        symbol: symbol.to_string(),
                        content,
                        bracket_matching: true,
                        ann: Annotations::default(),
                    }];
                }
                inlines.extend(content);
                i = close + delim.len();
                continue;
            }
        }
        lit.push(c);
        i += 1;
    }
    flush(&mut lit, &mut inlines);
    inlines
}

/// Position of the next occurrence of `delim` at or after `from`.
fn find(chars: &[char], from: usize, delim: &[char]) -> Option<usize> {
    (from..chars.len().saturating_sub(delim.len() - 1))
        .find(|&j| &chars[j..j + delim.len()] == delim)
}

/// Memory for the forward scans of one inline run, keyed by what
/// is sought. A scan from `from` that ended at `pos` (or nowhere)
/// answers every later query starting at or before `pos` without
/// rescanning, so a run of unclosed openers costs linear rather
/// than quadratic time. Sound whenever the scan's answer is the
/// first position at or after `from` meeting a condition that
/// does not itself depend on `from`.
struct ScanMemo<K>(Vec<(K, usize, Option<usize>)>);

impl<K: PartialEq + Copy> ScanMemo<K> {
    fn new() -> Self {
        ScanMemo(Vec::new())
    }

    fn find(&mut self, key: K, from: usize, scan: impl FnOnce() -> Option<usize>) -> Option<usize> {
        if let Some((_, f, r)) = self.0.iter().find(|(k, ..)| *k == key)
            && *f <= from
            && r.is_none_or(|p| from <= p)
        {
            return *r;
        }
        let r = scan();
        match self.0.iter_mut().find(|(k, ..)| *k == key) {
            Some(e) => (e.1, e.2) = (from, r),
            None => self.0.push((key, from, r)),
        }
        r
    }
}

/// The `*` delimiter runs, longest first.
const STAR_RUNS: [&[char]; 3] = [&['*', '*', '*'], &['*', '*'], &['*']];

/// The closer for the `*` run `delim` opening at `open`. A run
/// opens only before non-whitespace and closes only after it
/// (CommonMark's flanking, reduced; reStructuredText's start-
/// and end-string rule), and a single closing star is not part
/// of a longer run, so `**a * b**` is one strong span and
/// `*a **b** c*` one emphasis around a strong.
fn star_close(
    chars: &[char],
    closers: &mut ScanMemo<usize>,
    open: usize,
    delim: &'static [char],
) -> Option<usize> {
    let len = delim.len();
    let from = open + len;
    if chars.get(from).is_none_or(|c| c.is_whitespace()) {
        return None;
    }
    closers.find(len, from, || {
        (from..chars.len().saturating_sub(len - 1)).find(|&p| {
            chars[p..p + len] == *delim
                && !chars[p - 1].is_whitespace()
                && (len > 1 || (chars[p - 1] != '*' && chars.get(p + 1) != Some(&'*')))
        })
    })
}

/// [`find`] over one inline run, with memory.
struct Finder<'a> {
    chars: &'a [char],
    memo: ScanMemo<&'static [char]>,
}

impl<'a> Finder<'a> {
    fn new(chars: &'a [char]) -> Self {
        Finder {
            chars,
            memo: ScanMemo::new(),
        }
    }

    /// Position of the next occurrence of `delim` at or after
    /// `from`.
    fn find(&mut self, from: usize, delim: &'static [char]) -> Option<usize> {
        let chars = self.chars;
        self.memo.find(delim, from, || find(chars, from, delim))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_parsing() {
        let inlines = parse_inline(r"a *em* **st** `c*d` \*lit\*");
        assert_eq!(inlines.len(), 7);
        assert!(matches!(&inlines[1], Inline::Endo { symbol, .. } if symbol == "*"));
        assert!(matches!(&inlines[3], Inline::Endo { symbol, .. } if symbol == "**"));
        assert!(matches!(&inlines[5], Inline::VerbatimInline { content, .. } if content == "c*d"));
        assert!(matches!(&inlines[6], Inline::Text(t) if t == " *lit*"));
    }

    #[test]
    fn unclosed_delimiters_are_literal() {
        let inlines = parse_inline("a * b ` c");
        assert_eq!(inlines.len(), 1);
        assert!(matches!(&inlines[0], Inline::Text(t) if t == "a * b ` c"));
    }
}

// ---------------------------------------------------------------
// HTML importer (the at-html syntax mapper, inbound)
// ---------------------------------------------------------------

use crate::error::{Error, ErrorKind};

#[derive(Debug)]
pub(crate) enum Tok {
    Open {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    Close(String),
    Text(String),
}

/// Import a useful subset of HTML as an `at-html` document.
pub fn html_to_document(html: &str) -> Result<Document> {
    let toks = tokenize_html(html)?;
    // Unwrap the document frame: drop html/body tags and the head
    // section wholesale.
    let mut body: Vec<Tok> = Vec::new();
    let mut in_head = false;
    for tok in toks {
        match &tok {
            Tok::Open { name, .. } if name == "head" => in_head = true,
            Tok::Close(name) if name == "head" => in_head = false,
            Tok::Open { name, .. } | Tok::Close(name) if name == "html" || name == "body" => {}
            _ if in_head => {}
            _ => body.push(tok),
        }
    }
    let (blocks, end) = parse_html_blocks(&body, 0, None)?;
    debug_assert_eq!(end, body.len());
    Ok(Document {
        dialect_id: "at-html".to_string(),
        dialect_version: None,
        blocks,
    })
}

fn html_err(msg: String) -> Error {
    Error::new(ErrorKind::Syntax(format!("html import: {msg}")))
}

fn tokenize_html(html: &str) -> Result<Vec<Tok>> {
    let mut toks = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(lt) = rest.find('<') {
            if lt > 0 {
                toks.push(Tok::Text(rest[..lt].to_string()));
            }
            rest = &rest[lt..];
            if rest.starts_with("<!--") {
                let end = rest
                    .find("-->")
                    .ok_or_else(|| html_err("unterminated comment".into()))?;
                rest = &rest[end + 3..];
                continue;
            }
            if let Some(cdata) = rest.strip_prefix("<![CDATA[") {
                let end = cdata
                    .find("]]>")
                    .ok_or_else(|| html_err("unterminated CDATA section".into()))?;
                toks.push(Tok::Text(cdata_text(&cdata[..end])));
                rest = &cdata[end + 3..];
                continue;
            }
            if rest.starts_with("<!") {
                // Doctype and other declarations are skipped.
                let end = rest
                    .find('>')
                    .ok_or_else(|| html_err("unterminated declaration".into()))?;
                rest = &rest[end + 1..];
                continue;
            }
            let end = tag_end(rest).ok_or_else(|| html_err("unterminated tag".into()))?;
            let inner = &rest[1..end];
            rest = &rest[end + 1..];
            if let Some(name) = inner.strip_prefix('/') {
                toks.push(Tok::Close(name.trim().to_ascii_lowercase()));
                continue;
            }
            let (inner, self_closing) = match inner.strip_suffix('/') {
                Some(i) => (i, true),
                None => (inner, false),
            };
            let mut parts = inner.trim().splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or("").to_ascii_lowercase();
            if name.is_empty() {
                return Err(html_err("empty tag".into()));
            }
            let attrs = parse_html_attrs(parts.next().unwrap_or(""))?;
            toks.push(Tok::Open {
                name,
                attrs,
                self_closing,
            });
        } else {
            toks.push(Tok::Text(rest.to_string()));
            break;
        }
    }
    Ok(toks)
}

/// Minimal attribute parsing: `name="value"` pairs (and bare
/// names, which get empty values).
fn parse_html_attrs(text: &str) -> Result<Vec<(String, String)>> {
    let mut attrs = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let name_end = rest
            .find(|c: char| c == '=' || c.is_whitespace())
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_ascii_lowercase();
        rest = rest[name_end..].trim_start();
        let value = if let Some(after_eq) = rest.strip_prefix('=') {
            let after_eq = after_eq.trim_start();
            // XML allows either quote character (rend='indent'
            // in Perseus epidoc files).
            let quote = match after_eq.chars().next() {
                Some(q @ ('"' | '\'')) => q,
                _ => {
                    return Err(html_err(format!("unquoted attribute value after `{name}`")));
                }
            };
            let quoted = &after_eq[1..];
            let end = quoted
                .find(quote)
                .ok_or_else(|| html_err(format!("unterminated attribute `{name}`")))?;
            rest = quoted[end + 1..].trim_start();
            decode_entities(&quoted[..end])
        } else {
            String::new()
        };
        if !name.is_empty() {
            attrs.push((name, value));
        }
    }
    Ok(attrs)
}

pub(crate) fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

fn class_genoses(attrs: &[(String, String)]) -> Vec<String> {
    attr(attrs, "class")
        .map(|c| c.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Decode character references: the predefined XML entities,
/// `&nbsp;`, and decimal or hexadecimal numeric references, in
/// one pass (so `&amp;lt;` is `&lt;`). An unknown or malformed
/// reference stays literal.
pub(crate) fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        // A reference is short: its `;` is looked for nearby only.
        let decoded = rest[1..]
            .char_indices()
            .take(32)
            .find(|&(_, c)| c == ';')
            .and_then(|(semi, _)| Some((entity_char(&rest[1..1 + semi])?, semi + 2)));
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The character a reference names (the text between `&` and
/// `;`). NUL is refused: it is the importers' sentinel prefix.
fn entity_char(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            let number = name.strip_prefix('#')?;
            let (digits, radix) = match number.strip_prefix(['x', 'X']) {
                Some(hex) => (hex, 16),
                None => (number, 10),
            };
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return None;
            }
            let code = u32::from_str_radix(digits, radix).ok()?;
            char::from_u32(code).filter(|&c| c != '\0')
        }
    }
}

/// The index of the `>` that ends the tag opening at the head of
/// `tag`; a `>` inside a quoted attribute value is content.
fn tag_end(tag: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in tag.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '>' => return Some(i),
                _ => {}
            },
        }
    }
    None
}

/// The text of a CDATA section as a text token's content: the
/// consumers decode references in text tokens, so `&` is written
/// as its reference and the section's text comes out verbatim.
fn cdata_text(raw: &str) -> String {
    raw.replace('&', "&amp;")
}

fn heading_level(name: &str) -> Option<usize> {
    let n = name.strip_prefix('h')?;
    let level: usize = n.parse().ok()?;
    (1..=6).contains(&level).then_some(level)
}

/// Parse block content until the closing tag `until` (or end of
/// input). Whitespace-only text between blocks is skipped.
fn parse_html_blocks(
    toks: &[Tok],
    mut i: usize,
    until: Option<&str>,
) -> Result<(Vec<Block>, usize)> {
    let _depth = descend(html_err)?;
    let mut blocks = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if Some(name.as_str()) == until => return Ok((blocks, i + 1)),
            Tok::Close(name) => return Err(html_err(format!("unexpected `</{name}>`"))),
            Tok::Text(text) => {
                if !text.trim().is_empty() {
                    return Err(html_err(format!(
                        "bare text at block level: `{}`",
                        text.trim()
                    )));
                }
                i += 1;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } => {
                if let Some(level) = heading_level(name) {
                    let (content, next) = parse_html_inlines(toks, i + 1, name, false)?;
                    blocks.push(Block::Paragraph(vec![Inline::Endo {
                        symbol: "#".repeat(level),
                        content,
                        bracket_matching: true,
                        ann: Annotations::default(),
                    }]));
                    i = next;
                    continue;
                }
                match name.as_str() {
                    "p" => {
                        let (content, next) = parse_html_inlines(toks, i + 1, "p", false)?;
                        blocks.push(Block::Paragraph(content));
                        i = next;
                    }
                    "blockquote" => {
                        let (children, next) = parse_html_blocks(toks, i + 1, Some("blockquote"))?;
                        blocks.push(para_block(">", None, children, false, vec![]));
                        i = next;
                    }
                    "div" => {
                        let mut genoses = class_genoses(attrs);
                        // The verse/table divisions are the exo
                        // rendering of the verse stichoi sim;
                        // invert them back to it.
                        if genoses.first().map(String::as_str) == Some("verse")
                            || genoses.first().map(String::as_str) == Some("table")
                        {
                            let kind = genoses.remove(0);
                            let taxis = attr(attrs, "data-taxis")
                                .and_then(|v| v.parse().ok())
                                .map(Taxis::Explicit);
                            let (block, next) =
                                parse_html_verse(toks, i + 1, kind == "table", taxis, genoses)?;
                            blocks.push(block);
                            i = next;
                            continue;
                        }
                        let (children, next) = parse_html_blocks(toks, i + 1, Some("div"))?;
                        let mut block = para_block("_", None, children, true, genoses);
                        // The division's id is its onym (the exo
                        // writes it so; pointers refer to it).
                        if let Block::Para { ann, .. } = &mut block {
                            ann.onym = attr(attrs, "id")
                                .filter(|id| !id.is_empty())
                                .map(str::to_string);
                        }
                        blocks.push(block);
                        i = next;
                    }
                    "ul" => {
                        let (items, next) = parse_html_items(toks, i + 1, "ul", |_, _| None)?;
                        blocks.push(para_block("--", None, items, true, vec![]));
                        i = next;
                    }
                    "ol" => {
                        let (items, next) = parse_html_items(toks, i + 1, "ol", |attrs, idx| {
                            let value = attr(attrs, "value")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(idx as u64 + 1);
                            Some(Taxis::Explicit(value))
                        })?;
                        blocks.push(para_block("..", None, items, true, vec![]));
                        i = next;
                    }
                    "pre" => {
                        let (content, next) = parse_html_pre(toks, i + 1)?;
                        blocks.push(Block::VerbatimBlock {
                            content,
                            ann: Annotations::default(),
                        });
                        i = next;
                    }
                    "img" => {
                        let src =
                            attr(attrs, "src").ok_or_else(|| html_err("img without src".into()))?;
                        blocks.push(Block::Enmedia {
                            param: src.to_string(),
                        });
                        i += 1;
                        if !self_closing && matches!(toks.get(i), Some(Tok::Close(n)) if n == "img")
                        {
                            i += 1;
                        }
                    }
                    other => {
                        return Err(html_err(format!(
                            "unsupported element at block level: `<{other}>`"
                        )));
                    }
                }
            }
        }
    }
    match until {
        Some(name) => Err(html_err(format!("unclosed `<{name}>`"))),
        None => Ok((blocks, i)),
    }
}

/// Parse the inside of a `div class="verse"` / `div
/// class="table"` back into the verse stichoi sim: `p
/// class="verse-title"` is the lemma, `p class="strophe"` holds
/// `<br/>`-separated stichoi, `p class="verse-attribution"` is
/// the hypograph.
fn parse_html_verse(
    toks: &[Tok],
    mut i: usize,
    table: bool,
    taxis: Option<Taxis>,
    mut genoses: Vec<String>,
) -> Result<(Block, usize)> {
    let mut lemma = Vec::new();
    let mut strophes = Vec::new();
    let mut hypograph = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "div" => {
                i += 1;
                if table {
                    genoses.insert(0, "table".to_string());
                }
                return Ok((
                    Block::Stichoi {
                        symbol: Some("~".to_string()),
                        taxis,
                        lemma,
                        strophes,
                        hypograph,
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses,
                        },
                    },
                    i,
                ));
            }
            Tok::Text(text) if text.trim().is_empty() => i += 1,
            Tok::Open { name, attrs, .. } if name == "p" => {
                let classes = class_genoses(attrs);
                let class = classes.first().map(String::as_str);
                match class {
                    Some("verse-title") | Some("table-caption") => {
                        let (content, next) = parse_html_inlines(toks, i + 1, "p", false)?;
                        lemma = content;
                        i = next;
                    }
                    Some("verse-attribution") => {
                        let (content, next) = parse_html_inlines(toks, i + 1, "p", false)?;
                        hypograph = content;
                        i = next;
                    }
                    Some("strophe") => {
                        let (content, next) = parse_html_inlines(toks, i + 1, "p", true)?;
                        strophes.push(Strophe(split_on_breaks(content)));
                        i = next;
                    }
                    _ => return Err(html_err("unsupported paragraph inside a verse".into())),
                }
            }
            _ => return Err(html_err("unsupported content inside a verse".into())),
        }
    }
    Err(html_err("unterminated verse division".into()))
}

/// Marker for a `br` inside a strophe; never appears in real
/// text (NUL is rejected by the tokenizer's input).
const BR_SENTINEL: &str = "\u{0}br";

/// Split a strophe's inline run on the sentinels its `br`
/// elements produced.
fn split_on_breaks(content: Vec<Inline>) -> Vec<Vec<Inline>> {
    let mut lines = vec![Vec::new()];
    for inline in content {
        if matches!(&inline, Inline::Text(t) if t == BR_SENTINEL) {
            lines.push(Vec::new());
        } else {
            lines.last_mut().unwrap().push(inline);
        }
    }
    // Each line starts with the source newline that followed the
    // previous `br` (or the `p` open tag): renderer formatting,
    // not content. Authorial leading spaces stay.
    for line in &mut lines {
        if let Some(Inline::Text(t)) = line.first_mut() {
            let trimmed = t.trim_start_matches('\n').to_string();
            if trimmed.is_empty() {
                line.remove(0);
            } else {
                *t = trimmed;
            }
        }
    }
    if lines.last().is_some_and(|l| {
        l.iter()
            .all(|i| matches!(i, Inline::Text(t) if t.trim().is_empty()))
    }) {
        lines.pop();
    }
    lines
}

fn para_block(
    symbol: &str,
    taxis: Option<Taxis>,
    children: Vec<Block>,
    bracket_matching: bool,
    genoses: Vec<String>,
) -> Block {
    Block::Para {
        symbol: symbol.to_string(),
        taxis,
        lemma: vec![],
        children,
        hypograph: vec![],
        bracket_matching,
        ann: Annotations {
            onym: None,
            genoses,
        },
    }
}

/// Parse `li` children of a list. Item content may be blocks or
/// bare inline content (wrapped as one paragraph).
fn parse_html_items(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    taxis: impl Fn(&[(String, String)], usize) -> Option<Taxis>,
) -> Result<(Vec<Block>, usize)> {
    let mut items = Vec::new();
    loop {
        match toks.get(i) {
            Some(Tok::Close(name)) if name == until => return Ok((items, i + 1)),
            Some(Tok::Text(text)) if text.trim().is_empty() => i += 1,
            Some(Tok::Open { name, attrs, .. }) if name == "li" => {
                let t = taxis(attrs, items.len());
                let symbol = if t.is_some() { "." } else { "-" };
                let (children, next) = parse_html_item_content(toks, i + 1)?;
                items.push(para_block(symbol, t, children, true, vec![]));
                i = next;
            }
            _ => return Err(html_err(format!("expected `<li>` or `</{until}>`"))),
        }
    }
}

fn parse_html_item_content(toks: &[Tok], i: usize) -> Result<(Vec<Block>, usize)> {
    // Bare inline content (text or inline tag first) wraps as one
    // paragraph; otherwise the item holds blocks.
    let inline_first = match toks.get(i) {
        Some(Tok::Text(t)) if !t.trim().is_empty() => true,
        Some(Tok::Open { name, .. }) => {
            matches!(name.as_str(), "em" | "strong" | "span" | "code")
        }
        _ => false,
    };
    if inline_first {
        let (content, next) = parse_html_inlines(toks, i, "li", false)?;
        Ok((vec![Block::Paragraph(content)], next))
    } else {
        parse_html_blocks(toks, i, Some("li"))
    }
}

fn parse_html_pre(toks: &[Tok], mut i: usize) -> Result<(String, usize)> {
    // Canonical form is <pre><code>raw</code></pre>; a bare <pre>
    // is accepted too.
    let coded = matches!(toks.get(i), Some(Tok::Open { name, .. }) if name == "code");
    if coded {
        i += 1;
    }
    let mut content = String::new();
    while let Some(Tok::Text(text)) = toks.get(i) {
        content.push_str(&decode_entities(text));
        i += 1;
    }
    if coded {
        match toks.get(i) {
            Some(Tok::Close(name)) if name == "code" => i += 1,
            _ => return Err(html_err("unclosed `<code>` in `<pre>`".into())),
        }
    }
    match toks.get(i) {
        Some(Tok::Close(name)) if name == "pre" => i += 1,
        _ => return Err(html_err("unclosed `<pre>`".into())),
    }
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    Ok((content, i))
}

/// Parse inline content until the closing tag `until`.
fn parse_html_inlines(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    breaks: bool,
) -> Result<(Vec<Inline>, usize)> {
    let _depth = descend(html_err)?;
    let mut inlines = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => return Ok((inlines, i + 1)),
            Tok::Close(name) => return Err(html_err(format!("unexpected `</{name}>`"))),
            Tok::Text(text) => {
                inlines.push(Inline::Text(decode_entities(text)));
                i += 1;
            }
            Tok::Open {
                name, self_closing, ..
            } if breaks && name == "br" => {
                inlines.push(Inline::Text(BR_SENTINEL.to_string()));
                i += 1;
                if !self_closing && matches!(toks.get(i), Some(Tok::Close(n)) if n == "br") {
                    i += 1;
                }
            }
            Tok::Open { name, attrs, .. } => match name.as_str() {
                "em" | "strong" | "span" => {
                    let symbol = match name.as_str() {
                        "em" => "/",
                        "strong" => "!",
                        _ => ",",
                    };
                    let genoses = if name == "span" {
                        class_genoses(attrs)
                    } else {
                        vec![]
                    };
                    let (content, next) = parse_html_inlines(toks, i + 1, name, breaks)?;
                    // An empty, classless span with an id is the
                    // exo's onym anchor.
                    if name == "span"
                        && content.is_empty()
                        && genoses.is_empty()
                        && let Some(id) = attr(attrs, "id").filter(|id| !id.is_empty())
                    {
                        inlines.push(Inline::OnymAnchor(id.to_string()));
                        i = next;
                        continue;
                    }
                    inlines.push(Inline::Endo {
                        symbol: symbol.to_string(),
                        content,
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses,
                        },
                    });
                    i = next;
                }
                "code" => {
                    let mut content = String::new();
                    i += 1;
                    while let Some(Tok::Text(text)) = toks.get(i) {
                        content.push_str(&decode_entities(text));
                        i += 1;
                    }
                    match toks.get(i) {
                        Some(Tok::Close(name)) if name == "code" => i += 1,
                        _ => return Err(html_err("unclosed `<code>`".into())),
                    }
                    inlines.push(Inline::VerbatimInline {
                        content,
                        ann: Annotations::default(),
                    });
                }
                "a" => {
                    // The visible-URL link: an anchor whose text
                    // is its target (or none) is the link sim
                    // exactly; a hidden href projects as prose
                    // text with the visible URL beside it — the
                    // org projection, pending F9's faithful form.
                    let href = attr(attrs, "href")
                        .map(|h| h.trim().to_string())
                        .unwrap_or_default();
                    let (content, next) = parse_html_inlines(toks, i + 1, name, breaks)?;
                    // The exo's deixis: a pointer anchor to an
                    // onymized division. Its dagger is rendering,
                    // not content.
                    if let Some(onym) = href.strip_prefix('#')
                        && !onym.is_empty()
                        && class_genoses(attrs).iter().any(|c| c == "pointer")
                    {
                        inlines.push(Inline::Deixis {
                            symbol: "_".to_string(),
                            onym: onym.to_string(),
                            ann: Annotations::default(),
                        });
                        i = next;
                        continue;
                    }
                    if href.is_empty() {
                        inlines.extend(content);
                    } else {
                        let visible = match content.as_slice() {
                            [] => true,
                            [Inline::Text(t)] => t.trim() == href,
                            _ => false,
                        };
                        if visible {
                            inlines.push(link_endo(href));
                        } else {
                            inlines.extend(content);
                            inlines.push(Inline::Text(" (".to_string()));
                            inlines.push(link_endo(href));
                            inlines.push(Inline::Text(")".to_string()));
                        }
                    }
                    i = next;
                }
                other => {
                    return Err(html_err(format!("unsupported inline element: `<{other}>`")));
                }
            },
        }
    }
    Err(html_err(format!("unclosed `<{until}>`")))
}

// ---------------------------------------------------------------
// reStructuredText importer (the at-rst syntax mapper, inbound)
// ---------------------------------------------------------------

const RST_ADORNMENTS: &[(char, usize)] =
    &[('=', 1), ('-', 2), ('~', 3), ('^', 4), ('"', 5), ('\'', 6)];

/// Import a useful subset of reStructuredText as an `at-rst`
/// document.
pub fn rst_to_document(rst: &str) -> Result<Document> {
    let lines: Vec<&str> = rst.lines().collect();
    let blocks = parse_rst_blocks(&lines)?;
    Ok(Document {
        dialect_id: "at-rst".to_string(),
        dialect_version: None,
        blocks,
    })
}

fn rst_err(msg: String) -> crate::error::Error {
    crate::error::Error::new(ErrorKind::Syntax(format!("rst import: {msg}")))
}

fn adornment_level(line: &str) -> Option<usize> {
    let mut chars = line.chars();
    let first = chars.next()?;
    let (_, level) = RST_ADORNMENTS.iter().find(|(c, _)| *c == first)?;
    (line.len() >= 3 && chars.all(|c| c == first)).then_some(*level)
}

fn rst_indented(line: &str) -> Option<&str> {
    line.strip_prefix("   ")
}

/// Collect an indented block starting at `i`: indented lines and
/// interior blank lines, dedented.
fn rst_indented_block<'a>(lines: &[&'a str], mut i: usize) -> (Vec<&'a str>, usize) {
    let mut inner: Vec<&str> = Vec::new();
    while i < lines.len() {
        if let Some(rest) = rst_indented(lines[i]) {
            inner.push(rest);
        } else if lines[i].trim().is_empty()
            && lines
                .get(i + 1)
                .is_some_and(|next| rst_indented(next).is_some())
        {
            inner.push("");
        } else {
            break;
        }
        i += 1;
    }
    (inner, i)
}

fn parse_rst_blocks(lines: &[&str]) -> Result<Vec<Block>> {
    let _depth = descend(rst_err)?;
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Directives and footnote bodies.
        if let Some(rest) = line.strip_prefix(".. ") {
            if let Some(kind) = ["note", "warning"]
                .iter()
                .find(|k| rest == format!("{k}::"))
            {
                let mut j = i + 1;
                if lines.get(j).is_some_and(|l| l.trim().is_empty()) {
                    j += 1;
                }
                let (inner, next) = rst_indented_block(lines, j);
                let children = parse_rst_blocks(&inner)?;
                let symbol = if *kind == "note" { "!" } else { "!!" };
                blocks.push(rst_para(symbol, vec![], children, None));
                i = next;
                continue;
            }
            if let Some(path) = rest.strip_prefix("image:: ") {
                blocks.push(Block::Enmedia {
                    param: path.trim().to_string(),
                });
                i += 1;
                continue;
            }
            if let Some(after) = rest.strip_prefix("[#") {
                let Some((name, body)) = after.split_once("] ") else {
                    return Err(rst_err(format!("malformed footnote `{line}`")));
                };
                let mut inner: Vec<&str> = vec![body];
                let (cont, next) = rst_indented_block(lines, i + 1);
                inner.extend(cont);
                let children = parse_rst_blocks(&inner)?;
                blocks.push(rst_para("^", vec![], children, Some(name.to_string())));
                i = next.max(i + 1);
                continue;
            }
            return Err(rst_err(format!("unsupported directive `{line}`")));
        }

        // Field list entry `:Name: value`.
        if let Some(rest) = line.strip_prefix(':')
            && let Some((name, value)) = rest.split_once(": ")
            && !name.is_empty()
            && !name.contains(char::is_whitespace)
        {
            // The body continues on indented lines (the exo's
            // shape for a field of several lines or paragraphs).
            let mut inner: Vec<&str> = vec![value.trim()];
            let (cont, next) = rst_indented_block(lines, i + 1);
            inner.extend(cont);
            let children = parse_rst_blocks(&inner)?;
            blocks.push(rst_para(":", parse_rst_inline(name), children, None));
            i = next.max(i + 1);
            continue;
        }

        // Underlined title.
        if !line.starts_with(' ')
            && let Some(under) = lines.get(i + 1)
            && let Some(level) = adornment_level(under)
            // docutils takes an underline shorter than its title
            // as a title still (with a warning) once it is four
            // characters long; a shorter one is ordinary text.
            && (under.len() >= line.trim_end().chars().count() || under.len() >= 4)
        {
            blocks.push(Block::Paragraph(vec![Inline::Endo {
                symbol: "#".repeat(level),
                content: parse_rst_inline(line.trim_end()),
                bracket_matching: true,
                ann: Annotations::default(),
            }]));
            i += 2;
            continue;
        }

        // Bullet and enumerated items (flat, like the md importer).
        if let Some(rest) = line.strip_prefix("- ") {
            let (children, next) = rst_item_content(lines, i, rest, "  ")?;
            blocks.push(rst_para("-", vec![], children, None));
            i = next;
            continue;
        }
        if let Some((n, rest)) = rst_enum_marker(line) {
            let (children, next) = rst_item_content(lines, i, rest, "   ")?;
            let mut block = rst_para(".", vec![], children, None);
            if let Block::Para { taxis, .. } = &mut block {
                *taxis = Some(Taxis::Explicit(n));
            }
            blocks.push(block);
            i = next;
            continue;
        }

        // Literal block: a standalone `::` line.
        if line.trim_end() == "::" {
            let mut j = i + 1;
            if lines.get(j).is_some_and(|l| l.trim().is_empty()) {
                j += 1;
            }
            let (inner, next) = rst_indented_block(lines, j);
            let mut content = inner.join("\n");
            if !content.is_empty() {
                content.push('\n');
            }
            blocks.push(Block::VerbatimBlock {
                content,
                ann: Annotations::default(),
            });
            i = next;
            continue;
        }

        // Indented block at this level: a blockquote.
        if rst_indented(line).is_some() {
            let (inner, next) = rst_indented_block(lines, i);
            let children = parse_rst_blocks(&inner)?;
            blocks.push(rst_para(">", vec![], children, None));
            i = next;
            continue;
        }

        // Definition item: a term line immediately followed by an
        // indented block (no blank line between).
        if lines.get(i + 1).is_some_and(|l| rst_indented(l).is_some()) {
            let (inner, next) = rst_indented_block(lines, i + 1);
            let children = parse_rst_blocks(&inner)?;
            blocks.push(rst_para(
                "::",
                parse_rst_inline(line.trim_end()),
                children,
                None,
            ));
            i = next;
            continue;
        }

        // Paragraph: soft-joined plain lines.
        let start = i;
        while i < lines.len()
            && !lines[i].trim().is_empty()
            && !lines[i].starts_with(' ')
            && !lines[i].starts_with(".. ")
            && !lines[i].starts_with("- ")
            && rst_enum_marker(lines[i]).is_none()
            && lines
                .get(i + 1)
                .is_none_or(|l| adornment_level(l).is_none())
            && lines.get(i + 1).is_none_or(|l| rst_indented(l).is_none())
        {
            i += 1;
        }
        if i == start {
            i += 1;
        }
        let text = lines[start..i.max(start + 1).min(lines.len())].join(" ");
        blocks.push(Block::Paragraph(parse_rst_inline(text.trim_end())));
    }
    Ok(blocks)
}

fn rst_para(symbol: &str, lemma: Vec<Inline>, children: Vec<Block>, onym: Option<String>) -> Block {
    Block::Para {
        symbol: symbol.to_string(),
        taxis: None,
        lemma,
        children,
        hypograph: vec![],
        bracket_matching: symbol != ">",
        ann: Annotations {
            onym,
            genoses: vec![],
        },
    }
}

fn rst_enum_marker(line: &str) -> Option<(u64, &str)> {
    let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = line[digits.len()..].strip_prefix(". ")?;
    Some((digits.parse().ok()?, rest))
}

fn rst_item_content(
    lines: &[&str],
    i: usize,
    first: &str,
    indent: &str,
) -> Result<(Vec<Block>, usize)> {
    let mut inner: Vec<String> = vec![first.to_string()];
    let mut j = i + 1;
    while j < lines.len() {
        if let Some(rest) = lines[j].strip_prefix(indent) {
            inner.push(rest.to_string());
        } else if lines[j].trim().is_empty()
            && lines
                .get(j + 1)
                .is_some_and(|next| next.starts_with(indent))
        {
            inner.push(String::new());
        } else {
            break;
        }
        j += 1;
    }
    let refs: Vec<&str> = inner.iter().map(String::as_str).collect();
    Ok((parse_rst_blocks(&refs)?, j))
}

/// Inline reStructuredText: strong, emphasis, double-backtick
/// literals, footnote callouts (deixes), backslash escapes.
fn parse_rst_inline(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut finder = Finder::new(&chars);
    let mut closers: ScanMemo<usize> = ScanMemo::new();
    let mut inlines: Vec<Inline> = Vec::new();
    let mut lit = String::new();
    let mut i = 0;

    let flush = |lit: &mut String, inlines: &mut Vec<Inline>| {
        if !lit.is_empty() {
            inlines.push(Inline::Text(std::mem::take(lit)));
        }
    };

    while i < chars.len() {
        let c = chars[i];
        if c == '\\'
            && let Some(&next) = chars.get(i + 1)
            && next.is_ascii_punctuation()
        {
            lit.push(next);
            i += 2;
            continue;
        }
        // Double-backtick literal.
        if c == '`'
            && chars.get(i + 1) == Some(&'`')
            && let Some(close) = finder.find(i + 2, &['`', '`'])
        {
            flush(&mut lit, &mut inlines);
            inlines.push(Inline::VerbatimInline {
                content: chars[i + 2..close].iter().collect(),
                ann: Annotations::default(),
            });
            i = close + 2;
            continue;
        }
        // Footnote callout `[#name]_`.
        if c == '['
            && chars.get(i + 1) == Some(&'#')
            && let Some(close) = finder.find(i + 2, &[']', '_'])
        {
            flush(&mut lit, &mut inlines);
            inlines.push(Inline::Deixis {
                symbol: "^".to_string(),
                onym: chars[i + 2..close].iter().collect(),
                ann: Annotations::default(),
            });
            i = close + 2;
            continue;
        }
        // Hyperlink reference with embedded URI, `text <url>`_ (or
        // anonymous, `__). Text equal to the URL, or absent, is the
        // visible-URL link sim exactly; distinct text projects as
        // prose with the visible URL beside it, pending F9. Named
        // references without a URI stay literal, as do bare URLs.
        if c == '`'
            && chars.get(i + 1) != Some(&'`')
            && let Some(close) = finder.find(i + 1, &['`', '_'])
            && let Some((text, url)) = rst_embedded_uri(&chars[i + 1..close])
        {
            flush(&mut lit, &mut inlines);
            let text = text.trim();
            if text.is_empty() || text == url {
                inlines.push(link_endo(url));
            } else {
                inlines.extend(parse_rst_inline(text));
                inlines.push(Inline::Text(" (".to_string()));
                inlines.push(link_endo(url));
                inlines.push(Inline::Text(")".to_string()));
            }
            i = close + 2;
            if chars.get(i) == Some(&'_') {
                i += 1;
            }
            continue;
        }
        // Strong, then emphasis.
        if c == '*' {
            let strong = chars.get(i + 1) == Some(&'*');
            let delim = STAR_RUNS[if strong { 1 } else { 2 }];
            let open_len = delim.len();
            if let Some(close) = star_close(&chars, &mut closers, i, delim) {
                flush(&mut lit, &mut inlines);
                let inner: String = chars[i + open_len..close].iter().collect();
                inlines.push(Inline::Endo {
                    symbol: if strong { "**".into() } else { "*".into() },
                    content: parse_rst_inline(&inner),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = close + open_len;
                continue;
            }
        }
        lit.push(c);
        i += 1;
    }
    flush(&mut lit, &mut inlines);
    inlines
}

// ---------------------------------------------------------------
// TEI (P5 subset) -> litogramma
// ---------------------------------------------------------------
//
// A deliberately basic subset, imported as a litogramma
// document — the stress test for litogramma's coverage of real
// literary markup. Handled: teiHeader titleStmt (title, author,
// editor); body div nesting (type part/chapter, else
// depth-mapped) with head as the section lemma and numeric n as
// taxis; p; lg/l verse (head as lemma, nested lg as strophes);
// sp/speaker dialogue with stage directions (block and inline);
// quote; epigraph with bibl attribution; note[place=foot] as a
// deixis callout plus a footnote body; hi/emph/foreign inline;
// lb as a space. Everything else is a strict error — the
// point is to learn what litogramma cannot say.

/// A choice element: prefer the regularized reading
/// (expan/corr/reg) over the source form (abbr/sic/orig).
/// Split the inside of a `` `...`_ `` reference into (text, URI)
/// when it carries an embedded URI: `text <uri>` with the angle
/// group last and preceded by whitespace (or standing alone). A
/// `<name_>` alias, whitespace inside the target, or a stray
/// backtick disqualifies it.
fn rst_embedded_uri(inner: &[char]) -> Option<(String, String)> {
    if inner.last() != Some(&'>') || inner.contains(&'`') {
        return None;
    }
    let open = inner.iter().rposition(|&c| c == '<')?;
    if open > 0 && !inner[open - 1].is_whitespace() {
        return None;
    }
    let url: String = inner[open + 1..inner.len() - 1].iter().collect();
    if url.is_empty() || url.chars().any(char::is_whitespace) || url.ends_with('_') {
        return None;
    }
    let text: String = inner[..open].iter().collect();
    Some((text, url))
}

// ---------------------------------------------------------------
// at-aphanes: the unseen annotations
// ---------------------------------------------------------------
//
// Editorial metadata that never reaches the page rides the
// at-aphanes monosims (std/at-aphanes.dia), written first inside
// the span they annotate: the speaker of a prose dialogue line
// (prosopon), the referent of a name (prosopon / chora /
// syllogos), the target of a reference (skopos), the normalized
// value of a date (chronos), an analytic category (eidos). The
// importers below emit them where the source carries the
// attribute; litosis strips them, so the litos is unchanged.

const PROSOPON: &str = "?:";
const CHORA: &str = "?.";
const SYLLOGOS: &str = "?&";
const SKOPOS: &str = "?>";
const CHRONOS: &str = "?-";
const EIDOS: &str = "?%";
const PARADOSIS: &str = "?~";

/// A key as a monosim parameter: a leading `#` (a TEI pointer)
/// sheds, interior whitespace runs become hyphens (a parameter
/// carries no whitespace), case is preserved so ids round-trip.
fn aphanes_key(value: &str) -> String {
    let mut out = String::new();
    let mut pending = false;
    for c in value.trim().trim_start_matches('#').chars() {
        if c.is_whitespace() {
            pending = true;
        } else {
            if pending && !out.is_empty() {
                out.push('-');
            }
            pending = false;
            out.push(c);
        }
    }
    out
}

/// One aphanes monosim.
fn aphanes(symbol: &str, value: &str) -> Inline {
    Inline::Monosim {
        symbol: symbol.to_string(),
        param: aphanes_key(value),
        ann: Annotations::default(),
    }
}

/// TEI `ana`: whitespace-separated pointers, one eidos each.
fn aphanes_eidos(ana: &str) -> Vec<Inline> {
    ana.split_whitespace().map(|a| aphanes(EIDOS, a)).collect()
}

/// Annotations first inside the span they annotate.
fn with_aphanes(mut marks: Vec<Inline>, content: Vec<Inline>) -> Vec<Inline> {
    marks.extend(content);
    marks
}

/// The printed text of an inline run, monosims excluded.
fn plain_text(inlines: &[Inline]) -> String {
    fn walk(inlines: &[Inline], out: &mut String) {
        for inline in inlines {
            match inline {
                Inline::Text(t) => out.push_str(t),
                Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                    walk(content, out)
                }
                _ => {}
            }
        }
    }
    let mut out = String::new();
    walk(inlines, &mut out);
    out
}

/// The transmitted reading of a TEI `choice` as a paradosis value:
/// whitespace runs collapse to single spaces (written escaped in
/// the parameter), and a value that is empty or unbalanced in
/// its parentheses has no spelling.
fn paradosis_value(inlines: &[Inline]) -> Option<String> {
    let value: String = plain_text(inlines)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!value.is_empty() && crate::parser::balanced_parens(&value)).then_some(value)
}

/// The reading text of a `choice` is the editor's form (expan,
/// corr, reg); the page's form (abbr, sic, orig) rides along as
/// the paradosis aphanes on a diaphane around the reading, so the
/// litos stays the reading text and the kanon still answers what
/// the page printed. One side alone is plain text.
fn tei_choice_inlines(preferred: Vec<Inline>, fallback: Vec<Inline>) -> Vec<Inline> {
    if preferred.is_empty() {
        return fallback;
    }
    let Some(value) = paradosis_value(&fallback) else {
        return preferred;
    };
    let mut content = vec![Inline::Monosim {
        symbol: PARADOSIS.to_string(),
        param: value,
        ann: Annotations::default(),
    }];
    content.extend(preferred);
    vec![Inline::EndoDiaphane {
        content,
        ann: Annotations::default(),
    }]
}

/// The unseen marks of a TEI `said`/`q`: the speaker (prosopon)
/// and the analytic categories (eidos).
fn tei_said_marks(attrs: &[(String, String)]) -> Vec<Inline> {
    let mut marks = Vec::new();
    if let Some(who) = attr(attrs, "who") {
        marks.push(aphanes(PROSOPON, who));
    }
    if let Some(ana) = attr(attrs, "ana") {
        marks.extend(aphanes_eidos(ana));
    }
    marks
}

/// Speech mode is a class, not a value: `direct="false"` is the
/// `indirect` genos, `aloud="false"` the `thought` genos, on the
/// quotation span (TEI's defaults, direct and aloud, are silent).
fn tei_said_mode(attrs: &[(String, String)]) -> Vec<String> {
    let mut genoses = Vec::new();
    if attr(attrs, "direct") == Some("false") {
        genoses.push("indirect".to_string());
    }
    if attr(attrs, "aloud") == Some("false") {
        genoses.push("thought".to_string());
    }
    genoses
}

/// A TEI name, date, or seg that carries an unseen attribute
/// becomes an annotation span (genos = the element name) with
/// the marks first inside; one that carries none stays
/// transparent (the reading-text policy). Returns the genos and
/// the marks, or None when there is nothing unseen to keep.
fn tei_aphanes_span(name: &str, attrs: &[(String, String)]) -> Option<(String, Vec<Inline>)> {
    let pointer = attr(attrs, "ref").or_else(|| attr(attrs, "key"));
    let mut marks: Vec<Inline> = Vec::new();
    match name {
        "persName" => marks.extend(pointer.map(|p| aphanes(PROSOPON, p))),
        "placeName" => marks.extend(pointer.map(|p| aphanes(CHORA, p))),
        "orgName" => marks.extend(pointer.map(|p| aphanes(SYLLOGOS, p))),
        "name" | "rs" => {
            let symbol = match attr(attrs, "type") {
                Some("person") | Some("persName") | Some("pers") => Some(PROSOPON),
                Some("place") | Some("placeName") => Some(CHORA),
                Some("org") | Some("orgName") | Some("organisation") | Some("organization") => {
                    Some(SYLLOGOS)
                }
                _ => None,
            };
            if let (Some(sym), Some(p)) = (symbol, pointer) {
                marks.push(aphanes(sym, p));
            }
        }
        "date" | "dateRange" => {
            if let Some(when) = attr(attrs, "when") {
                marks.push(aphanes(CHRONOS, when));
            } else if let (Some(from), Some(to)) = (attr(attrs, "from"), attr(attrs, "to")) {
                marks.push(aphanes(CHRONOS, &format!("{from}/{to}")));
            } else if let Some(from) = attr(attrs, "from") {
                marks.push(aphanes(CHRONOS, &format!("{from}/")));
            } else if let Some(to) = attr(attrs, "to") {
                marks.push(aphanes(CHRONOS, &format!("/{to}")));
            }
        }
        _ => {}
    }
    if let Some(ana) = attr(attrs, "ana") {
        marks.extend(aphanes_eidos(ana));
    }
    if marks.is_empty() {
        return None;
    }
    let genos = match name {
        "dateRange" => "date".to_string(),
        other => other.to_ascii_lowercase(),
    };
    Some((genos, marks))
}

thread_local! {
    /// Running number for TEI sentences without an id, reset per
    /// import.
    static TEI_SENTENCES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether a TEI `w`, `pc` or `s` carries a parsing (or is a
/// sentence): then it becomes an at-epimerismos diaphane rather
/// than a transparent wrapper.
fn tei_parsing_span(name: &str, attrs: &[(String, String)]) -> bool {
    match name {
        "s" => true,
        _ => ["lemma", "pos", "type", "msd", "ana"]
            .iter()
            .any(|a| attr(attrs, a).is_some()),
    }
}

/// The diaphane for a TEI `w`/`pc` (a token: lemma, pos, msd as
/// lexema, meros, parepomena; ana as eidos) or `s` (a sentence:
/// the periodos marker with xml:id, n, or a running number).
fn tei_parsing_inline(name: &str, attrs: &[(String, String)], content: Vec<Inline>) -> Inline {
    use crate::epimerismos::{LEXEMA, MEROS, PAREPOMENA, PERIODOS};
    let mut marks: Vec<Inline> = Vec::new();
    if name == "s" {
        let id = attr(attrs, "xml:id")
            .or_else(|| attr(attrs, "n"))
            .map(aphanes_key)
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| {
                TEI_SENTENCES.with(|c| {
                    c.set(c.get() + 1);
                    c.get().to_string()
                })
            });
        marks.push(aphanes(PERIODOS, &id));
    } else {
        if let Some(l) = attr(attrs, "lemma") {
            marks.push(aphanes(LEXEMA, l));
        }
        if let Some(p) = attr(attrs, "pos").or_else(|| attr(attrs, "type")) {
            marks.push(aphanes(MEROS, p));
        }
        if let Some(m) = attr(attrs, "msd") {
            marks.push(aphanes(PAREPOMENA, &m.replace('|', ",")));
        }
    }
    if let Some(ana) = attr(attrs, "ana") {
        marks.extend(aphanes_eidos(ana));
    }
    Inline::EndoDiaphane {
        content: with_aphanes(marks, content),
        ann: Annotations::default(),
    }
}

/// USFM book codes to OSIS book ids, for the OSIS spelling of a
/// reference target (USX `loc="GEN 1:1-3"` -> `Gen.1.1-Gen.1.3`).
const OSIS_BOOKS: &[(&str, &str)] = &[
    ("GEN", "Gen"),
    ("EXO", "Exod"),
    ("LEV", "Lev"),
    ("NUM", "Num"),
    ("DEU", "Deut"),
    ("JOS", "Josh"),
    ("JDG", "Judg"),
    ("RUT", "Ruth"),
    ("1SA", "1Sam"),
    ("2SA", "2Sam"),
    ("1KI", "1Kgs"),
    ("2KI", "2Kgs"),
    ("1CH", "1Chr"),
    ("2CH", "2Chr"),
    ("EZR", "Ezra"),
    ("NEH", "Neh"),
    ("EST", "Esth"),
    ("JOB", "Job"),
    ("PSA", "Ps"),
    ("PRO", "Prov"),
    ("ECC", "Eccl"),
    ("SNG", "Song"),
    ("ISA", "Isa"),
    ("JER", "Jer"),
    ("LAM", "Lam"),
    ("EZK", "Ezek"),
    ("DAN", "Dan"),
    ("HOS", "Hos"),
    ("JOL", "Joel"),
    ("AMO", "Amos"),
    ("OBA", "Obad"),
    ("JON", "Jonah"),
    ("MIC", "Mic"),
    ("NAM", "Nah"),
    ("HAB", "Hab"),
    ("ZEP", "Zeph"),
    ("HAG", "Hag"),
    ("ZEC", "Zech"),
    ("MAL", "Mal"),
    ("MAT", "Matt"),
    ("MRK", "Mark"),
    ("LUK", "Luke"),
    ("JHN", "John"),
    ("ACT", "Acts"),
    ("ROM", "Rom"),
    ("1CO", "1Cor"),
    ("2CO", "2Cor"),
    ("GAL", "Gal"),
    ("EPH", "Eph"),
    ("PHP", "Phil"),
    ("COL", "Col"),
    ("1TH", "1Thess"),
    ("2TH", "2Thess"),
    ("1TI", "1Tim"),
    ("2TI", "2Tim"),
    ("TIT", "Titus"),
    ("PHM", "Phlm"),
    ("HEB", "Heb"),
    ("JAS", "Jas"),
    ("1PE", "1Pet"),
    ("2PE", "2Pet"),
    ("1JN", "1John"),
    ("2JN", "2John"),
    ("3JN", "3John"),
    ("JUD", "Jude"),
    ("REV", "Rev"),
    ("TOB", "Tob"),
    ("JDT", "Jdt"),
    ("ESG", "EsthGr"),
    ("WIS", "Wis"),
    ("SIR", "Sir"),
    ("BAR", "Bar"),
    ("LJE", "EpJer"),
    ("S3Y", "PrAzar"),
    ("SUS", "Sus"),
    ("BEL", "Bel"),
    ("1MA", "1Macc"),
    ("2MA", "2Macc"),
    ("3MA", "3Macc"),
    ("4MA", "4Macc"),
    ("1ES", "1Esd"),
    ("2ES", "2Esd"),
    ("MAN", "PrMan"),
    ("PS2", "AddPs"),
    ("ODA", "Odes"),
    ("PSS", "PssSol"),
    ("DAG", "DanGr"),
];

/// A USX `loc` in OSIS spelling: `GEN 1:1-3` -> `Gen.1.1-Gen.1.3`,
/// `GEN 1:1-2:3` -> `Gen.1.1-Gen.2.3`, `GEN 1` -> `Gen.1`; several
/// references (`;`-separated) join with commas, which the OSIS
/// grammar does not read but which keep the parameter whole. An
/// unrecognized shape falls back to the raw text as a key.
fn usx_loc_to_osis(loc: &str) -> String {
    fn one(part: &str) -> Option<String> {
        let part = part.trim();
        let (code, rest) = match part.split_once(' ') {
            Some((c, r)) => (c.trim(), r.trim()),
            None => (part, ""),
        };
        let book = OSIS_BOOKS
            .iter()
            .find(|(usfm, _)| usfm.eq_ignore_ascii_case(code))
            .map(|(_, osis)| *osis)?;
        if rest.is_empty() {
            return Some(book.to_string());
        }
        let (start, end) = match rest.split_once('-') {
            Some((a, b)) => (a.trim(), Some(b.trim())),
            None => (rest, None),
        };
        let (chapter, verse) = match start.split_once(':') {
            Some((c, v)) => (c.trim(), Some(v.trim())),
            None => (start, None),
        };
        if chapter.is_empty() || !chapter.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let mut out = format!("{book}.{chapter}");
        if let Some(v) = verse {
            out.push('.');
            out.push_str(v);
        }
        if let Some(end) = end {
            out.push('-');
            out.push_str(book);
            out.push('.');
            match end.split_once(':') {
                Some((c, v)) => {
                    out.push_str(c.trim());
                    out.push('.');
                    out.push_str(v.trim());
                }
                None => {
                    // `1:1-3`: the end is a verse of the same chapter;
                    // `1-3` with no verse: a chapter range.
                    if verse.is_some() {
                        out.push_str(chapter);
                        out.push('.');
                    }
                    out.push_str(end);
                }
            }
        }
        Some(out)
    }
    let parts: Option<Vec<String>> = loc.split(';').map(one).collect();
    match parts {
        Some(parts) if !parts.is_empty() => parts.join(","),
        _ => aphanes_key(loc),
    }
}

fn tei_choice(
    toks: &[Tok],
    mut i: usize,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Inline>, Vec<Inline>, usize)> {
    let mut preferred: Vec<Inline> = Vec::new();
    let mut fallback: Vec<Inline> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "choice" => return Ok((preferred, fallback, i + 1)),
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            // An empty reading (`<expan/>`, `<sic/>`) has no close
            // tag to run to.
            Tok::Open {
                name,
                self_closing: true,
                ..
            } if matches!(
                name.as_str(),
                "expan" | "corr" | "reg" | "abbr" | "sic" | "orig"
            ) =>
            {
                i += 1;
            }
            Tok::Open { name, .. } if matches!(name.as_str(), "expan" | "corr" | "reg") => {
                let n = name.clone();
                let ((content, _), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                preferred = content;
                i = next;
            }
            Tok::Open { name, .. } if matches!(name.as_str(), "abbr" | "sic" | "orig") => {
                let n = name.clone();
                let ((content, _), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                fallback = content;
                i = next;
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <choice>"))),
        }
    }
    Err(tei_err("unterminated <choice>".into()))
}

/// Trim the leading and trailing whitespace of an inline run
/// (element boundaries carry no significant whitespace).
fn trim_run(inlines: &mut Vec<Inline>) {
    if let Some(Inline::Text(t)) = inlines.first_mut() {
        *t = t.trim_start().to_string();
        if t.is_empty() {
            inlines.remove(0);
        }
    }
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        *t = t.trim_end().to_string();
        if t.is_empty() {
            inlines.pop();
        }
    }
}

pub(crate) fn tei_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("tei import: {msg}")))
}

/// Tokenize XML: like the HTML tokenizer, but case-preserving
/// (TEI is case-sensitive) and skipping the XML declaration.
pub(crate) fn tokenize_xml(xml: &str) -> Result<Vec<Tok>> {
    let mut toks = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        if let Some(lt) = rest.find('<') {
            if lt > 0 {
                toks.push(Tok::Text(rest[..lt].to_string()));
            }
            rest = &rest[lt..];
            if rest.starts_with("<!--") {
                let end = rest
                    .find("-->")
                    .ok_or_else(|| tei_err("unterminated comment".into()))?;
                rest = &rest[end + 3..];
                continue;
            }
            if let Some(cdata) = rest.strip_prefix("<![CDATA[") {
                let end = cdata
                    .find("]]>")
                    .ok_or_else(|| tei_err("unterminated CDATA section".into()))?;
                toks.push(Tok::Text(cdata_text(&cdata[..end])));
                rest = &cdata[end + 3..];
                continue;
            }
            if rest.starts_with("<?") || rest.starts_with("<!") {
                // A DOCTYPE may carry an internal DTD subset
                // (`[ … ]>`, Perseus P4 parameter entities): the
                // declaration then ends at `]>`, not the first `>`.
                let end = rest
                    .find('>')
                    .ok_or_else(|| tei_err("unterminated declaration".into()))?;
                if let Some(bracket) = rest.find('[')
                    && bracket < end
                {
                    let close = rest[bracket..]
                        .find("]>")
                        .ok_or_else(|| tei_err("unterminated declaration".into()))?;
                    rest = &rest[bracket + close + 2..];
                } else {
                    rest = &rest[end + 1..];
                }
                continue;
            }
            let end = tag_end(rest).ok_or_else(|| tei_err("unterminated tag".into()))?;
            let inner = &rest[1..end];
            rest = &rest[end + 1..];
            if let Some(name) = inner.strip_prefix('/') {
                toks.push(Tok::Close(name.trim().to_string()));
                continue;
            }
            let (inner, self_closing) = match inner.strip_suffix('/') {
                Some(i) => (i, true),
                None => (inner, false),
            };
            let mut parts = inner.trim().splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or("").to_string();
            if name.is_empty() {
                return Err(tei_err("empty tag".into()));
            }
            let attrs = parse_html_attrs(parts.next().unwrap_or(""))?;
            toks.push(Tok::Open {
                name,
                attrs,
                self_closing,
            });
        } else {
            toks.push(Tok::Text(rest.to_string()));
            break;
        }
    }
    Ok(toks)
}

/// Inject a synthetic `<milestone unit="{scheme}" n="{value}"/>`
/// immediately after every `<l n="N">` open whose N is a positive
/// integer and (N == 1 or N % 5 == 0). The book prefix is the `n`
/// of the innermost enclosing `<div type|subtype="book">` with a
/// numeric n (value `{book}.{N}`), else the bare `{N}`.
fn tei_inject_line_milestones(toks: &mut Vec<Tok>, scheme: &str) {
    let mut books: Vec<Option<u32>> = Vec::new();
    let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
    for tok in std::mem::take(toks) {
        let mut milestone: Option<Tok> = None;
        match &tok {
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "div" && !self_closing => {
                let is_book = [attr(attrs, "type"), attr(attrs, "subtype")]
                    .iter()
                    .flatten()
                    .any(|v| v.eq_ignore_ascii_case("book"));
                let book = is_book
                    .then(|| attr(attrs, "n").and_then(|n| n.trim().parse::<u32>().ok()))
                    .flatten();
                books.push(book);
            }
            Tok::Close(name) if name == "div" => {
                books.pop();
            }
            Tok::Open { name, attrs, .. } if name == "l" => {
                if let Some(n) = attr(attrs, "n").and_then(|n| n.trim().parse::<u32>().ok())
                    && (n == 1 || n % 5 == 0)
                {
                    let value = match books.iter().rev().find_map(|b| *b) {
                        Some(b) => format!("{b}.{n}"),
                        None => n.to_string(),
                    };
                    milestone = Some(Tok::Open {
                        name: "milestone".to_string(),
                        attrs: vec![
                            ("unit".to_string(), scheme.to_string()),
                            ("n".to_string(), value),
                        ],
                        self_closing: true,
                    });
                }
            }
            _ => {}
        }
        out.push(tok);
        if let Some(ms) = milestone {
            out.push(ms);
        }
    }
    *toks = out;
}

/// Import a TEI P5 document (basic subset) as litogramma.
pub fn tei_to_document(xml: &str) -> Result<Document> {
    tei_to_document_lines(xml, None)
}

/// As `tei_to_document`, but with an opt-in line-milestone
/// pre-pass: when `line_milestones` is set, every `<l n="N">`
/// verse line whose N is a positive integer with N == 1 or
/// N % 5 == 0 gets a synthetic milestone under that scheme as its
/// first inline (value `{book}.{N}` under the innermost numeric
/// book div, else `{N}`).
pub fn tei_to_document_lines(xml: &str, line_milestones: Option<&str>) -> Result<Document> {
    TEI_SENTENCES.with(|c| c.set(0));
    let mut toks = tokenize_xml(xml)?;
    // TEI P4 numbered divisions (div1..div7, Perseus P4 files)
    // normalize to the P5 nested div: depth is recomputed from
    // type/nesting either way.
    for tok in &mut toks {
        match tok {
            Tok::Open { name, .. } | Tok::Close(name)
                if name.len() == 4
                    && name.starts_with("div")
                    && name.as_bytes()[3].is_ascii_digit() =>
            {
                *name = "div".to_string();
            }
            _ => {}
        }
    }
    if let Some(scheme) = line_milestones {
        tei_inject_line_milestones(&mut toks, scheme);
    }
    // A body of <entry> elements is a dictionary: the Lex-0
    // path imports it as lexigramma.
    if tei_is_dictionary(&toks) {
        return tei_lex0_document(&toks);
    }
    let mut blocks: Vec<Block> = Vec::new();
    // The bibliography is read ahead of the text, so a ref that
    // points at one of its entries imports as a cite.
    let entries = tei_bibliography(&toks)?;
    let mut ctx = TeiCtx {
        notes: 0,
        bib_keys: entries
            .iter()
            .filter_map(|e| match e {
                Block::Para { lemma, .. } => Some(plain_text(lemma)),
                _ => None,
            })
            .collect(),
    };
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            // A byte-order mark is whitespace at document level.
            Tok::Text(t) if t.trim_start_matches('\u{feff}').trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "TEI" || name == "TEI.2" => i += 1,
            Tok::Close(name) if name == "TEI" || name == "TEI.2" => i += 1,
            Tok::Open { name, .. } if name == "teiHeader" => {
                let next = tei_header(&toks, i + 1, &mut blocks, &mut ctx)?;
                i = next;
            }
            Tok::Open { name, .. } if name == "standOff" => {
                i = skip_element(&toks, i + 1, "standOff".to_string())?;
            }
            Tok::Open { name, .. } if name == "text" => i += 1,
            Tok::Close(name) if name == "text" => i += 1,
            Tok::Open { name, .. } if name == "back" => {
                i = skip_element(&toks, i + 1, name.clone())?;
            }
            Tok::Open { name, .. } if name == "front" => {
                i = tei_front(&toks, i + 1, &mut blocks, &mut ctx)?;
            }
            Tok::Open { name, .. } if name == "body" => {
                let (inner, next) = tei_blocks(&toks, i + 1, "body", 0, &mut ctx)?;
                blocks.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "interpGrp" => {
                // metrical interpretation apparatus: skip whole
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(&toks, i + 1, "interpGrp".to_string())?;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "pb" || name == "gap" => {
                // a page break / gap between text-level elements
                // (Perseus emits <pb/> before <front>) — tolerated
                let sc = *self_closing;
                let n2 = name.clone();
                i += 1;
                if !sc && matches!(toks.get(i), Some(Tok::Close(n)) if *n == n2) {
                    i += 1;
                }
            }
            other => return Err(tei_err(format!("unexpected {other:?} at document level"))),
        }
    }
    let mut ms_state = MsPass {
        page: String::new(),
        page_scheme: String::new(),
        seen: std::collections::HashSet::new(),
    };
    tei_compose_bekker(&mut blocks, &mut ms_state);
    tei_settle_heads(&mut blocks);
    if !entries.is_empty() {
        blocks.push(Block::MonadEnglossis {
            dialect: "bibliogramma".to_string(),
            children: entries,
            ann: Annotations::default(),
        });
    }
    Ok(Document {
        dialect_id: "litogramma".to_string(),
        dialect_version: None,
        blocks,
    })
}

/// The state one TEI import threads through its readers: the
/// footnote counter (every note body is `n{notes}`) and the keys
/// of the bibliography being imported (the xml:id of every
/// listBibl entry), which make a ref at one of them a cite.
#[derive(Default)]
struct TeiCtx {
    notes: usize,
    bib_keys: std::collections::HashSet<String>,
}

/// The bibliography of a TEI text: every bibl or biblStruct
/// inside a listBibl, wherever the list sits in the text (a
/// bibliography div, the back matter), as bibliogramma entries in
/// document order. The key is the xml:id; an entry without one
/// cannot be cited but keeps its place under a synthesized key
/// (bibl-N, its ordinal). The header's listBibl describes the
/// sources of the edition and is not read.
fn tei_bibliography(toks: &[Tok]) -> Result<Vec<Block>> {
    let mut entries: Vec<Block> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // The ids the source spells, so a synthesized key takes none.
    let taken: std::collections::HashSet<&str> = toks
        .iter()
        .filter_map(|t| match t {
            Tok::Open { name, attrs, .. } if name == "bibl" || name == "biblStruct" => {
                attr(attrs, "xml:id").or_else(|| attr(attrs, "id"))
            }
            _ => None,
        })
        .collect();
    let mut in_header = false;
    let mut lists = 0usize;
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "teiHeader" => in_header = true,
            Tok::Close(name) if name == "teiHeader" => in_header = false,
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "listBibl" => lists += 1,
            Tok::Close(name) if name == "listBibl" => lists = lists.saturating_sub(1),
            Tok::Open {
                name,
                attrs,
                self_closing: false,
            } if (name == "bibl" || name == "biblStruct") && lists > 0 && !in_header => {
                let key = match attr(attrs, "xml:id").or_else(|| attr(attrs, "id")) {
                    Some(key) => key.to_string(),
                    None => {
                        let mut n = entries.len() + 1;
                        while taken.contains(format!("bibl-{n}").as_str()) {
                            n += 1;
                        }
                        format!("bibl-{n}")
                    }
                };
                if seen.insert(key.clone()) {
                    let (entry, next) = tei_bibl_entry(toks, i + 1, name, attrs, &key)?;
                    entries.push(entry);
                    i = next;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    Ok(entries)
}

/// One listBibl entry: author, editor, title, date, publisher,
/// pubPlace and biblScope map onto bibliogramma fields, at any
/// depth (biblStruct's analytic, monogr and imprint are read
/// through). An entry with no such children keeps its printed
/// text as a note field.
fn tei_bibl_entry(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    attrs: &[(String, String)],
    key: &str,
) -> Result<(Block, usize)> {
    let start = i;
    let mut authors: Vec<String> = Vec::new();
    let mut editors: Vec<String> = Vec::new();
    let mut titles: Vec<(String, String)> = Vec::new();
    let mut rest: Vec<(String, String)> = Vec::new();
    let mut depth = 0usize;
    let end = loop {
        match toks.get(i) {
            None => return Err(tei_err(format!("unterminated <{until}>"))),
            Some(Tok::Close(name)) if name == until => {
                if depth == 0 {
                    break i + 1;
                }
                depth -= 1;
                i += 1;
            }
            Some(Tok::Open {
                name,
                self_closing: false,
                ..
            }) if name == until => {
                depth += 1;
                i += 1;
            }
            Some(Tok::Open {
                name,
                attrs: a,
                self_closing: false,
            }) if matches!(
                name.as_str(),
                "author"
                    | "editor"
                    | "title"
                    | "date"
                    | "publisher"
                    | "pubPlace"
                    | "biblScope"
                    | "note"
            ) =>
            {
                let (text, next) = tei_text_of(toks, i + 1, name)?;
                i = next;
                match name.as_str() {
                    "author" => authors.push(text),
                    "editor" => editors.push(text),
                    "title" => {
                        titles.push((attr(a, "level").unwrap_or("").to_string(), text));
                    }
                    "date" => {
                        let when = attr(a, "when").unwrap_or(&text).to_string();
                        let year = when.strip_prefix('-').unwrap_or(&when);
                        let field = if !year.is_empty() && year.chars().all(|c| c.is_ascii_digit())
                        {
                            "year"
                        } else {
                            "date"
                        };
                        rest.push((field.to_string(), when));
                    }
                    "publisher" => rest.push(("publisher".to_string(), text)),
                    // A typed note is the field its type names
                    // (the bibliogramma tei exo writes the
                    // fields TEI has no element for this way).
                    "note" => rest.push((attr(a, "type").unwrap_or("note").to_string(), text)),
                    "pubPlace" => rest.push(("location".to_string(), text)),
                    _ => match attr(a, "unit").or_else(|| attr(a, "type")) {
                        Some("page" | "pp" | "pages") => rest.push(("pages".to_string(), text)),
                        Some("volume" | "vol") => rest.push(("volume".to_string(), text)),
                        _ => {}
                    },
                }
            }
            Some(_) => i += 1,
        }
    };
    // The levels name the genus: an analytic title in a journal is
    // an article, in a monograph a contribution; a lone title is
    // the entry's title whatever its level.
    // Beside a journal or monograph title, an unlevelled first
    // title is the analytic one.
    let analytic = titles.iter().position(|(l, _)| l == "a").or_else(|| {
        (titles.len() > 1
            && titles[0].0.is_empty()
            && titles[1..].iter().any(|(l, _)| !l.is_empty()))
        .then_some(0)
    });
    let mut kind = "misc";
    let mut fields: Vec<(String, String)> = Vec::new();
    if !authors.is_empty() {
        fields.push(("author".to_string(), authors.join(" and ")));
    }
    for (n, (level, text)) in titles.into_iter().enumerate() {
        let field = match (analytic, level.as_str()) {
            (Some(a), _) if a == n => "title",
            (Some(_), "j") => {
                kind = "article";
                "journal"
            }
            // Beside an analytic title, a monograph title — or
            // one that names no level — is the containing work.
            (Some(_), "m" | "") => {
                kind = "incollection";
                "booktitle"
            }
            (None, level) if !fields.iter().any(|(f, _)| f == "title") => {
                match level {
                    "m" => kind = "book",
                    "j" => kind = "periodical",
                    _ => {}
                }
                "title"
            }
            _ => continue,
        };
        if !fields.iter().any(|(f, _)| f == field) {
            fields.push((field.to_string(), text));
        }
    }
    if !editors.is_empty() {
        fields.push(("editor".to_string(), editors.join(" and ")));
    }
    // A repeated element (a second note, a second page range)
    // joins the first, as authors do.
    for (field, text) in rest {
        match fields.iter_mut().find(|(f, _)| *f == field) {
            Some((_, value)) if text.is_empty() || *value == text => {}
            Some((_, value)) if value.is_empty() => *value = text,
            Some((_, value)) => {
                value.push_str("; ");
                value.push_str(&text);
            }
            None => fields.push((field, text)),
        }
    }
    if fields.is_empty() {
        let (printed, _) = tei_text_of(toks, start, until)?;
        fields.push(("note".to_string(), printed));
    }
    // A type names the genus when it can be spelled as a genos
    // ("Journal Article" is journal-article); else the genus the
    // titles gave stands.
    let kind = attr(attrs, "type")
        .and_then(tei_kebab)
        .unwrap_or_else(|| kind.to_string());
    let children = fields
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(name, value)| Block::Para {
            symbol: ":".to_string(),
            taxis: None,
            lemma: vec![Inline::Text(name)],
            children: vec![Block::Paragraph(vec![Inline::Text(value)])],
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations::default(),
        })
        .collect();
    Ok((
        Block::Para {
            symbol: "&".to_string(),
            taxis: None,
            lemma: vec![Inline::Text(key.to_string())],
            children,
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations {
                onym: None,
                genoses: vec![kind],
            },
        },
        end,
    ))
}

/// Skip an element wholesale (front and back matter).
pub(crate) fn skip_element(toks: &[Tok], mut i: usize, name: String) -> Result<usize> {
    let mut depth = 1;
    while i < toks.len() {
        match &toks[i] {
            Tok::Open {
                name: n,
                self_closing: false,
                ..
            } if *n == name => depth += 1,
            Tok::Close(n) if *n == name => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(tei_err(format!("unterminated <{name}>")))
}

/// teiHeader: titleStmt children become litogramma front
/// matter and a particDesc listPerson the cast; the rest of the
/// header is skipped.
fn tei_header(
    toks: &[Tok],
    mut i: usize,
    blocks: &mut Vec<Block>,
    ctx: &mut TeiCtx,
) -> Result<usize> {
    // When any title carries type="main", untyped and
    // bibliographic siblings are catalogue noise: main becomes
    // the title, type="sub" the subtitle, the rest drops.
    // Only the titles the loop below reads count (the ones
    // directly under the descended wrappers): a type="main" in
    // the skipped sourceDesc describes the source, not this
    // edition.
    let has_main = {
        let mut probe = i;
        let mut found = false;
        while probe < toks.len() {
            match &toks[probe] {
                Tok::Close(name) if name == "teiHeader" => break,
                Tok::Open {
                    name,
                    attrs,
                    self_closing,
                } if name == "title" => {
                    if attr(attrs, "type") == Some("main") {
                        found = true;
                    }
                    probe = if *self_closing {
                        probe + 1
                    } else {
                        skip_element(toks, probe + 1, "title".to_string())?
                    };
                }
                Tok::Open {
                    name,
                    self_closing: false,
                    ..
                } if !matches!(
                    name.as_str(),
                    "fileDesc" | "titleStmt" | "profileDesc" | "particDesc"
                ) =>
                {
                    probe = skip_element(toks, probe + 1, name.clone())?;
                }
                _ => probe += 1,
            }
        }
        found
    };
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "teiHeader" => return Ok(i + 1),
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "title" => {
                if *self_closing {
                    // <title/>: an empty title — nothing to emit
                    i += 1;
                    continue;
                }
                let ttype = attr(attrs, "type").map(str::to_string);
                let save = ctx.notes;
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, "title", ctx)?;
                let symbol = match (has_main, ttype.as_deref()) {
                    (true, Some("main")) => Some("="),
                    (true, Some("sub")) => Some("=_"),
                    (true, _) => None,
                    (false, Some("sub")) => Some("=_"),
                    (false, _) => Some("="),
                };
                if let Some(symbol) = symbol {
                    blocks.push(solo_endo(symbol, content));
                    blocks.extend(bodies);
                } else {
                    // a dropped catalogue title takes its notes
                    // with it: release their numbers
                    ctx.notes = save;
                }
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "author" || name == "editor" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let n = name.clone();
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                blocks.push(solo_endo(if n == "author" { "=:" } else { "=;" }, content));
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if matches!(
                name.as_str(),
                "fileDesc" | "titleStmt" | "profileDesc" | "particDesc"
            ) =>
            {
                i += 1
            }
            Tok::Close(name)
                if matches!(
                    name.as_str(),
                    "fileDesc" | "titleStmt" | "profileDesc" | "particDesc"
                ) =>
            {
                i += 1
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "listPerson" => {
                // The edition's cast (profileDesc/particDesc):
                // the same dramatis-persona lines as a body-level
                // list, after the front matter.
                let (items, next) = tei_list_person(toks, i + 1, ctx)?;
                blocks.extend(items);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } => {
                // Everything else in the header is skipped.
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(toks, i + 1, name.clone())?;
                }
            }
            _ => i += 1,
        }
    }
    Err(tei_err("unterminated <teiHeader>".into()))
}

/// The core milestone for a TEI milestone (spec v0.12): the
/// scheme comes from resp (the reference system's name),
/// falling back to unit; the unit rides as a presentation
/// genos when both are present (a Stephanus page renders
/// bolder than a section in a Loeb margin).
/// A milestone n that reads as a prose TITLE, not a reference
/// coordinate (Ovid n="Quattuor aetates. Gigantes."): callers
/// treat it as furniture.
fn tei_milestone_is_title(n: &str) -> bool {
    n.trim().chars().count() > 24
}

/// A TEI name as a genos-shaped identifier (a milestone scheme, an
/// entry genus): lowercased, every run of other characters
/// one hyphen, none at the edges. None when nothing valid is left.
fn tei_kebab(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    crate::sigil::is_valid_genos(&out).then_some(out)
}

fn tei_milestone_mono(n: &str, attrs: &[(String, String)]) -> Option<Inline> {
    // Reference-system names ride as kebab-case schemes/genoses:
    // underscores in a unit (alt_poem_line) become hyphens, and
    // a pointer or an abbreviation (resp="#Bekker", "St.") sheds
    // what the scheme grammar has no spelling for.
    let resp = attr(attrs, "resp")
        .or_else(|| attr(attrs, "ed").filter(|e| e.len() > 1))
        .and_then(tei_kebab);
    let unit = attr(attrs, "unit").map(|u| u.to_ascii_lowercase().replace('_', "-"));
    let scheme = resp
        .clone()
        .or_else(|| unit.clone())
        .unwrap_or_else(|| "milestone".to_string());
    let genoses = match (&resp, unit) {
        (Some(_), Some(u)) => vec![u],
        _ => Vec::new(),
    };
    let value: String = n
        .trim()
        .chars()
        .map(|c| {
            if c.is_whitespace() {
                '-'
            } else if c.is_alphanumeric() || matches!(c, '.' | ':' | '-' | '_') {
                c
            } else {
                // uncertain-numbering junk (Livy n="5??"):
                // sanitize to a valid milestone value
                '-'
            }
        })
        .collect();
    let value = value
        .trim_matches(|c| c == '-' || c == '.')
        .replace(".-", "-")
        .replace("-.", "-");
    let value = {
        let mut v = String::new();
        let mut dash = false;
        for c in value.chars() {
            if c == '-' {
                if !dash {
                    v.push(c);
                }
                dash = true;
            } else {
                v.push(c);
                dash = false;
            }
        }
        v
    };
    // Perseus's ~5-line "card" is an arbitrary witness division,
    // not a canonical reference: it lives in the perseus:
    // namespace (registry ruling 2026-07-15, perseus:card:N).
    let (scheme, value, genoses) = if scheme == "card" {
        (
            "perseus".to_string(),
            format!("card:{value}"),
            vec!["card".to_string()],
        )
    } else {
        (scheme, value, genoses)
    };
    // A value that sanitizes to nothing citable (empty, or stray
    // punctuation debris) is dropped rather than emitted as an
    // invalid coordinate.
    if !crate::sigil::is_valid_milestone_value(&value) {
        return None;
    }
    Some(Inline::Milestone {
        scheme,
        value,
        ann: Annotations {
            onym: None,
            genoses,
        },
    })
}

/// A run of consecutive verse lines (and interleaved stage
/// directions, which become lines of their own) outside an lg.
/// Bekker citations are page+line; Perseus emits separate page
/// (n="1094a") and line (n="5") milestones under resp=Bekker.
/// Compose: a digits-only bekker value appends to the last seen
/// page value.
/// A head not promoted to a division lemma (a subtitle: the
/// second <head> of a div) would serialize as the NUL-sentinel
/// pseudo-sim; settle any leftovers into .head phrases.
fn tei_settle_heads(blocks: &mut [Block]) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => {
                for inl in inlines {
                    if let Inline::Endo { symbol, ann, .. } = inl
                        && symbol == "\u{0}head"
                    {
                        *symbol = ",".to_string();
                        ann.genoses = vec!["head".to_string()];
                    }
                }
            }
            Block::Para { children, .. }
            | Block::ParaDiaphane { children, .. }
            | Block::MonadEnglossis { children, .. } => tei_settle_heads(children),
            _ => {}
        }
    }
}

struct MsPass {
    page: String,
    page_scheme: String,
    seen: std::collections::HashSet<String>,
}

fn tei_compose_bekker(blocks: &mut [Block], st: &mut MsPass) {
    fn inlines_pass(inlines: &mut [Inline], st: &mut MsPass) {
        for inl in inlines {
            match inl {
                Inline::Milestone { scheme, value, ann } => {
                    if scheme == "bekker" {
                        if value.chars().all(|c| c.is_ascii_digit()) {
                            if !st.page.is_empty() {
                                *value = format!("{}{}", st.page, value);
                            }
                        } else {
                            st.page = value.clone();
                            st.page_scheme = scheme.clone();
                        }
                    } else if ann.genoses.iter().any(|g| g == "page") {
                        st.page = value.clone();
                        st.page_scheme = scheme.clone();
                    } else if scheme == "section"
                        && !st.page_scheme.is_empty()
                        && value.starts_with(&st.page)
                    {
                        // a resp-less section under a resp'd page
                        // (Perseus tags only pages with the
                        // reference system): inherit the scheme
                        *scheme = st.page_scheme.clone();
                    }
                    // re-announced anchors (page repeated at book
                    // heads, line grids restarting) drop: kanonizo
                    // forbids duplicate milestone values
                    let key = format!("{scheme}:{value}");
                    if !st.seen.insert(key) {
                        *inl = Inline::Text(" ".to_string());
                    }
                }
                Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                    inlines_pass(content, st);
                }
                _ => {}
            }
        }
    }
    for block in blocks {
        match block {
            Block::Paragraph(inlines) => inlines_pass(inlines, st),
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                inlines_pass(lemma, st);
                tei_compose_bekker(children, st);
                inlines_pass(hypograph, st);
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                inlines_pass(lemma, st);
                for strophe in strophes {
                    for line in &mut strophe.0 {
                        inlines_pass(line, st);
                    }
                }
                inlines_pass(hypograph, st);
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                tei_compose_bekker(children, st);
            }
            _ => {}
        }
    }
}

fn tei_line_run(
    toks: &[Tok],
    mut i: usize,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Vec<Inline>>, Vec<Block>, usize)> {
    let mut lines: Vec<Vec<Inline>> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open {
                name, self_closing, ..
            } if name == "l" => {
                // <l .../> is an empty placeholder (Perseus
                // style="hidden" n="0" anchors): skip, else the
                // run would swallow everything to a phantom </l>
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((mut content, inner), next) = tei_inline_run(toks, i + 1, "l", ctx)?;
                trim_inline_edges(&mut content);
                lines.push(content);
                bodies.extend(inner);
                i = next;
            }
            _ => break,
        }
    }
    Ok((lines, bodies, i))
}

/// Whether an inline <quote> holds verse lines.
fn tei_is_verse_quote(toks: &[Tok], i: usize) -> bool {
    if let Tok::Open { attrs, .. } = &toks[i]
        && attr(attrs, "type") == Some("verse")
    {
        return true;
    }
    let mut probe = i + 1;
    while matches!(&toks.get(probe), Some(Tok::Text(t)) if t.trim().is_empty()) {
        probe += 1;
    }
    // A quoted drama excerpt (<sp> speeches) reads as verse too.
    matches!(&toks.get(probe), Some(Tok::Open { name, .. })
        if name == "l" || name == "lg" || name == "sp")
}

/// The lines of an inline verse quotation, joined " / ".
fn tei_inline_verse_quote(
    toks: &[Tok],
    mut i: usize,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Inline>, Vec<Block>, usize)> {
    let mut out: Vec<Inline> = Vec::new();
    let mut vq_bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "quote" => return Ok((out, vq_bodies, i + 1)),
            Tok::Close(name) if name == "lg" => i += 1,
            Tok::Open { name, .. } if name == "lg" => i += 1,
            Tok::Open { name, .. } if name == "sp" => i += 1,
            Tok::Close(name) if name == "sp" => i += 1,
            Tok::Open { name, .. } if name == "speaker" => {
                // a drama excerpt: the speaker label leads its line
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "speaker", ctx)?;
                if !out.is_empty() {
                    out.push(Inline::Text(" / ".to_string()));
                }
                out.extend(content);
                vq_bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "quote" => {
                // a quotation nested within the quoted verse
                if *self_closing {
                    i += 1;
                    continue;
                }
                let (mut content, inner, next) = tei_inline_verse_quote(toks, i + 1, ctx)?;
                trim_inline_edges(&mut content);
                if !out.is_empty() {
                    out.push(Inline::Text(" / ".to_string()));
                }
                out.push(Inline::Endo {
                    symbol: "\"\"".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                vq_bodies.extend(inner);
                i = next;
            }
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open {
                name, self_closing, ..
            } if name == "l" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((mut content, inner), next) = tei_inline_run(toks, i + 1, "l", ctx)?;
                trim_inline_edges(&mut content);
                if !out.is_empty() {
                    out.push(Inline::Text(" / ".to_string()));
                }
                out.extend(content);
                vq_bodies.extend(inner);
                i = next;
            }
            Tok::Text(t) => {
                // prose interleaved among the quoted lines
                if !out.is_empty() {
                    out.push(Inline::Text(" ".to_string()));
                }
                out.push(Inline::Text(
                    collapse_ws(&decode_entities(t)).trim().to_string(),
                ));
                i += 1;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "milestone" => {
                if let Some(ms) = attr(attrs, "n")
                    .filter(|n| !tei_milestone_is_title(n))
                    .and_then(|n| tei_milestone_mono(n, attrs))
                {
                    out.push(ms);
                } else {
                    // an n-less milestone (unit=para print-
                    // paragraph anchor) still separates words
                    out.push(Inline::Text(" ".to_string()));
                }
                let sc = *self_closing;
                i += 1;
                if !sc && matches!(toks.get(i), Some(Tok::Close(n)) if n == "milestone") {
                    i += 1;
                }
            }
            Tok::Open { name, .. } if name == "placeName" => {
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "placeName", ctx)?;
                out.extend(content);
                vq_bodies.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "q" => {
                let ((mut content, inner), next) = tei_inline_run(toks, i + 1, "q", ctx)?;
                vq_bodies.extend(inner);
                trim_inline_edges(&mut content);
                out.push(Inline::Endo {
                    symbol: "\"\"".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "note" => {
                if *self_closing {
                    // <note/>: an empty note — nothing to emit
                    i += 1;
                    continue;
                }
                // apparatus notes inside a quoted verse run drop
                i = skip_element(toks, i + 1, "note".to_string())?;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "bibl" => {
                if *self_closing {
                    // <bibl/>: citation data in attributes only
                    i += 1;
                    continue;
                }
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "bibl", ctx)?;
                if !out.is_empty() {
                    out.push(Inline::Text(" ".to_string()));
                }
                out.extend(content);
                out.push(Inline::Text(" ".to_string()));
                vq_bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "gap" => {
                out.push(Inline::Text("\u{2026}".to_string()));
                let sc = *self_closing;
                i += 1;
                if !sc && matches!(toks.get(i), Some(Tok::Close(n)) if n == "gap") {
                    i += 1;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "pb" => {
                // a print page turn mid-quote — no content
                let sc = *self_closing;
                i += 1;
                if !sc && matches!(toks.get(i), Some(Tok::Close(n)) if n == "pb") {
                    i += 1;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "app" => {
                // an apparatus entry among the lines: its lem
                // reading continues the quoted text
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((mut content, inner), next) = tei_app_lem(toks, i + 1, ctx)?;
                trim_inline_edges(&mut content);
                if !out.is_empty() && !content.is_empty() {
                    out.push(Inline::Text(" ".to_string()));
                }
                out.extend(content);
                vq_bodies.extend(inner);
                i = next;
            }
            other => {
                return Err(tei_err(format!("unsupported {other:?} in a verse quote")));
            }
        }
    }
    Err(tei_err("unterminated verse <quote>".into()))
}

/// A parallel-segmentation apparatus entry (app) in an inline
/// context: the lem reading is the text and runs as inlines; the
/// variant readings (rdg), witness detail and apparatus notes are
/// not text and drop — at-tei has no apparatus construct to keep
/// them in. A rdgGrp is transparent: the lem it groups still
/// counts. Returns the reading and the index past </app>.
#[allow(clippy::type_complexity)]
fn tei_app_lem(
    toks: &[Tok],
    mut i: usize,
    ctx: &mut TeiCtx,
) -> Result<((Vec<Inline>, Vec<Block>), usize)> {
    let mut inlines: Vec<Inline> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "app" => return Ok(((inlines, bodies), i + 1)),
            Tok::Open {
                name, self_closing, ..
            } if name == "lem" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "lem", ctx)?;
                inlines.extend(content);
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "rdgGrp" => i += 1,
            Tok::Open {
                name, self_closing, ..
            } => {
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(toks, i + 1, name.clone())?;
                }
            }
            _ => i += 1,
        }
    }
    Err(tei_err("unterminated <app>".into()))
}

/// A block-level apparatus entry: the lem's blocks when it holds
/// block children, else its inline run as one paragraph.
fn tei_app_lem_blocks(
    toks: &[Tok],
    mut i: usize,
    depth: usize,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Block>, usize)> {
    let mut blocks: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "app" => return Ok((blocks, i + 1)),
            Tok::Open {
                name, self_closing, ..
            } if name == "lem" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let mut probe = i + 1;
                while matches!(&toks.get(probe), Some(Tok::Text(t)) if t.trim().is_empty()) {
                    probe += 1;
                }
                let structural = matches!(&toks.get(probe), Some(Tok::Open { name, .. })
                    if matches!(name.as_str(), "p" | "l" | "lg" | "sp" | "div" | "head" | "quote" | "ab"));
                if structural {
                    let (inner, next) = tei_blocks(toks, i + 1, "lem", depth, ctx)?;
                    blocks.extend(inner);
                    i = next;
                } else {
                    let ((mut content, inner), next) = tei_inline_run(toks, i + 1, "lem", ctx)?;
                    trim_inline_edges(&mut content);
                    if !content.is_empty() {
                        blocks.push(Block::Paragraph(content));
                    }
                    blocks.extend(inner);
                    i = next;
                }
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "rdgGrp" => i += 1,
            Tok::Open {
                name, self_closing, ..
            } => {
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(toks, i + 1, name.clone())?;
                }
            }
            _ => i += 1,
        }
    }
    Err(tei_err("unterminated <app>".into()))
}

/// A note body: paragraphs when present, else one inline run.
fn tei_note_body(toks: &[Tok], mut i: usize, ctx: &mut TeiCtx) -> Result<(Vec<Block>, usize)> {
    let mut children: Vec<Block> = Vec::new();
    let mut inline: Vec<Inline> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "note" => {
                if !inline.is_empty() {
                    let mut content = std::mem::take(&mut inline);
                    trim_inline_edges(&mut content);
                    if !content.is_empty() {
                        children.push(Block::Paragraph(content));
                    }
                }
                if children.is_empty() {
                    children.push(Block::Paragraph(Vec::new()));
                }
                return Ok((children, i + 1));
            }
            Tok::Open { name, .. } if name == "p" || name == "span" => {
                let n = name.clone();
                if !inline.is_empty() {
                    let mut content = std::mem::take(&mut inline);
                    trim_inline_edges(&mut content);
                    if !content.is_empty() {
                        children.push(Block::Paragraph(content));
                    }
                }
                let ((mut content, bodies), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                trim_inline_edges(&mut content);
                children.push(Block::Paragraph(content));
                children.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "lg" => {
                let (block, bodies, next) = tei_verse(toks, i + 1, ctx)?;
                children.push(block);
                children.extend(bodies);
                i = next;
            }
            _ => {
                // A plain inline body: one run to the close.
                let ((content, bodies), next) = tei_inline_run(toks, i, "note", ctx)?;
                inline.extend(content);
                children.extend(bodies);
                if !inline.is_empty() {
                    let mut content = std::mem::take(&mut inline);
                    trim_inline_edges(&mut content);
                    if !content.is_empty() {
                        children.push(Block::Paragraph(content));
                    }
                }
                if children.is_empty() {
                    children.push(Block::Paragraph(Vec::new()));
                }
                return Ok((children, next));
            }
        }
    }
    Err(tei_err("unterminated <note>".into()))
}

/// Verse lines inside an ab: lg groups are strophes; notes
/// between lines ride the previous line's end.
fn tei_ab_lines(
    toks: &[Tok],
    mut i: usize,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Strophe>, Vec<Block>, usize)> {
    let mut strophes: Vec<Strophe> = Vec::new();
    let mut current: Vec<Vec<Inline>> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "ab" => {
                if !current.is_empty() {
                    strophes.push(Strophe(current));
                }
                return Ok((strophes, bodies, i + 1));
            }
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "lg" => i += 1,
            Tok::Close(name) if name == "lg" => {
                if !current.is_empty() {
                    strophes.push(Strophe(std::mem::take(&mut current)));
                }
                i += 1;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "l" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((mut content, inner), next) = tei_inline_run(toks, i + 1, "l", ctx)?;
                trim_inline_edges(&mut content);
                current.push(content);
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "note" => {
                if *self_closing {
                    // <note/>: an empty note — nothing to emit
                    i += 1;
                    continue;
                }
                ctx.notes += 1;
                let onym = format!("n{}", ctx.notes);
                let (children, next) = tei_note_body(toks, i + 1, ctx)?;
                let deixis = Inline::Deixis {
                    symbol: "^".to_string(),
                    onym: onym.clone(),
                    ann: Annotations::default(),
                };
                match current
                    .last_mut()
                    .or_else(|| strophes.last_mut().and_then(|s| s.0.last_mut()))
                {
                    Some(line) => line.push(deixis),
                    None => current.push(vec![deixis]),
                }
                bodies.push(Block::Para {
                    symbol: "^".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: Some(onym),
                        genoses: Vec::new(),
                    },
                });
                i = next;
            }
            other => {
                return Err(tei_err(format!("unsupported {other:?} in an <ab> verse")));
            }
        }
    }
    Err(tei_err("unterminated <ab>".into()))
}

fn solo_endo(symbol: &str, content: Vec<Inline>) -> Block {
    solo_endo_onym(symbol, content, None)
}

fn solo_endo_onym(symbol: &str, content: Vec<Inline>, onym: Option<String>) -> Block {
    Block::Paragraph(vec![Inline::Endo {
        symbol: symbol.to_string(),
        content,
        bracket_matching: true,
        ann: Annotations {
            onym,
            genoses: Vec::new(),
        },
    }])
}

/// An xml:id as an onym, when it is one.
fn xml_id_onym(attrs: &[(String, String)]) -> Option<String> {
    attr(attrs, "xml:id")
        .filter(|id| crate::sigil::is_valid_onym(id))
        .map(str::to_string)
}

/// The litogramma sectioning ladder. A div's level is set by
/// its type when recognized, otherwise one deeper than its
/// parent; body-level untyped divs start as chapters.
const SECTION_LADDER: [&str; 6] = ["==", "===", "#", "##", "###", "####"];

fn div_level(div_type: Option<&str>, parent_level: usize) -> usize {
    match div_type {
        Some("part") | Some("book") => 0,
        Some("chapter") | Some("canto") => 1,
        _ => (parent_level + 1).min(SECTION_LADDER.len() - 1),
    }
}

/// Parse block content until the closing tag `until`.
fn tei_blocks(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    depth: usize,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Block>, usize)> {
    let mut blocks: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => return Ok((blocks, i + 1)),
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Text(t)
                if t.trim().chars().count() <= 3
                    && t.trim()
                        .chars()
                        .all(|c| c.is_ascii_punctuation() || c.is_whitespace()) =>
            {
                // stray punctuation orphaned outside a block (a
                // trailing `.` or `?` left by overlapping markup):
                // dropped, not an error.
                i += 1;
            }
            Tok::Text(t) => {
                return Err(tei_err(format!("bare text at block level: `{}`", t.trim())));
            }
            Tok::Open {
                name,
                self_closing: true,
                ..
            } if name == "div" => {
                // <div/>: an empty division — nothing to emit
                i += 1;
            }
            Tok::Open { name, attrs, .. } if name == "div" => {
                let level = div_level(attr(attrs, "type"), depth);
                let symbol = SECTION_LADDER[level];
                // div @n carries a REFERENCE value (a Stephanus
                // page, a letter number), not a sequence ordinal:
                // explicit taxis from it breaks kanonizo's
                // consistency check (Symposium opens at 172) —
                // let kanonizo number sequentially
                let taxis: Option<Taxis> = None;
                let _ = attr(attrs, "n");
                let (mut children, next) = tei_blocks(toks, i + 1, "div", level, ctx)?;
                // A leading head becomes the section lemma;
                // milestones may stand before it (a book
                // milestone opening its div).
                let head_at = children
                    .iter()
                    .position(|b| {
                        !matches!(b, Block::Paragraph(v)
                            if matches!(v.as_slice(), [Inline::Milestone { .. }]))
                    })
                    .unwrap_or(0);
                let lemma = match children.get(head_at) {
                    Some(Block::Paragraph(inlines))
                        if matches!(inlines.first(),
                            Some(Inline::Endo { symbol, .. }) if symbol == "\u{0}head") =>
                    {
                        let Some(Block::Paragraph(mut inlines)) = Some(children.remove(head_at))
                        else {
                            unreachable!()
                        };
                        let Some(Inline::Endo { content, .. }) = inlines.pop() else {
                            unreachable!()
                        };
                        content
                    }
                    _ => Vec::new(),
                };
                // A div that held nothing but its head and the
                // bibliography (read ahead, closing the document)
                // would leave a hollow section behind.
                let only_bibl = children.is_empty()
                    && toks[i + 1..next]
                        .iter()
                        .any(|t| matches!(t, Tok::Open { name, .. } if name == "listBibl"));
                if lemma.is_empty() {
                    // A headless div is transparent: its children
                    // splice here (litogramma headings require a
                    // lemma, and an edition number on a headless
                    // div already rides the milestones).
                    blocks.extend(children);
                } else if only_bibl {
                    // nothing to emit
                } else {
                    blocks.push(Block::Para {
                        symbol: symbol.to_string(),
                        taxis,
                        lemma,
                        children,
                        hypograph: Vec::new(),
                        bracket_matching: true,
                        ann: Annotations::default(),
                    });
                }
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "head" => {
                if *self_closing {
                    // <head/>: an empty heading — nothing to emit
                    i += 1;
                    continue;
                }
                // Wrapped in a sentinel endo; the enclosing div
                // promotes it to the lemma.
                let ((content, bodies), next) = {
                    let (run, next) = tei_inline_run(toks, i + 1, "head", ctx)?;
                    (run, next)
                };
                blocks.push(Block::Paragraph(vec![Inline::Endo {
                    symbol: "\u{0}head".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                }]));
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "said" => {
                let (block, next) = tei_said_paragraph(toks, i, ctx)?;
                blocks.push(block);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "app" => {
                // A block-level apparatus entry: the lem reading
                // is the text (block content when it holds
                // blocks, else one paragraph); the variants drop.
                if *self_closing {
                    i += 1;
                    continue;
                }
                let (inner, next) = tei_app_lem_blocks(toks, i + 1, depth, ctx)?;
                blocks.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "listPerson" => {
                if *self_closing {
                    i += 1;
                } else {
                    let (items, next) = tei_list_person(toks, i + 1, ctx)?;
                    blocks.extend(items);
                    i = next;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "listBibl" => {
                // The bibliography was read ahead of the text
                // (tei_bibliography) and closes the document.
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(toks, i + 1, "listBibl".to_string())?;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "pb" || name == "gap" => {
                let sc = *self_closing;
                let n2 = name.clone();
                i += 1;
                if !sc && matches!(toks.get(i), Some(Tok::Close(n)) if *n == n2) {
                    i += 1;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if matches!(name.as_str(), "delSpan" | "addSpan" | "anchor") => {
                let sc = *self_closing;
                let n2 = name.clone();
                i += 1;
                if !sc && matches!(toks.get(i), Some(Tok::Close(n)) if *n == n2) {
                    i += 1;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "interpGrp" => {
                // metrical interpretation apparatus: skip whole
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(toks, i + 1, "interpGrp".to_string())?;
                }
            }
            Tok::Open { name, .. } if name == "label" => {
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "label", ctx)?;
                blocks.push(Block::Paragraph(vec![Inline::Endo {
                    symbol: ",".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec!["label".to_string()],
                    },
                }]));
                blocks.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "speaker" => {
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "speaker", ctx)?;
                blocks.push(Block::Paragraph(vec![Inline::Endo {
                    symbol: ",".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec!["speaker".to_string()],
                    },
                }]));
                blocks.extend(inner);
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "p" => {
                if *self_closing {
                    // <p/>: an empty paragraph — nothing to emit
                    i += 1;
                    continue;
                }
                let ana = attr(attrs, "ana").map(aphanes_eidos);
                let mut probe = i + 1;
                while matches!(&toks.get(probe), Some(Tok::Text(t)) if t.trim().is_empty()) {
                    probe += 1;
                }
                // A said paragraph only when the said spans the
                // whole p: narration interleaved around saids
                // (Xenophon `<said>For,</said> said he, <said>`)
                // stays a plain paragraph flow with quoted
                // phrases.
                if matches!(&toks.get(probe), Some(Tok::Open { name, .. }) if name == "said")
                    && tei_said_spans_paragraph(toks, probe)
                {
                    let (block, next) = tei_said_paragraph(toks, probe, ctx)?;
                    blocks.push(block);
                    i = next;
                    continue;
                }
                let ((content, bodies), next) = {
                    let (run, next) = tei_inline_run(toks, i + 1, "p", ctx)?;
                    (run, next)
                };
                // A paragraph-level ana annotates the paragraph:
                // its eidos marks come first inside it.
                let content = match ana {
                    Some(marks) => with_aphanes(marks, content),
                    None => content,
                };
                blocks.push(Block::Paragraph(content));
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "lg" => {
                let (block, bodies, next) = tei_verse(toks, i + 1, ctx)?;
                blocks.push(block);
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "sp" => {
                let who = attr(attrs, "who").map(str::to_string);
                let (speech, next) = tei_speech(toks, i + 1, who.as_deref(), depth, ctx)?;
                blocks.extend(speech);
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "milestone" => {
                if let Some(ms) = attr(attrs, "n")
                    .filter(|n| !tei_milestone_is_title(n))
                    .and_then(|n| tei_milestone_mono(n, attrs))
                {
                    blocks.push(Block::Paragraph(vec![ms]));
                }
                i += 1;
                if !self_closing {
                    i = skip_element(toks, i, "milestone".to_string())?;
                }
            }
            Tok::Open { name, .. } if name == "l" => {
                // A loose run of verse lines outside an lg.
                let (strophe, line_bodies, next) = tei_line_run(toks, i, ctx)?;
                blocks.push(Block::Stichoi {
                    symbol: Some("~".to_string()),
                    taxis: None,
                    lemma: Vec::new(),
                    strophes: vec![Strophe(strophe)],
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                blocks.extend(line_bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "stage" => {
                let ((content, bodies), next) = {
                    let (run, next) = tei_inline_run(toks, i + 1, "stage", ctx)?;
                    (run, next)
                };
                blocks.push(Block::Para {
                    symbol: ":[".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children: vec![Block::Paragraph(content)],
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "quote" || name == "q" => {
                let (children, next) = tei_quote_content(toks, i + 1, name.clone(), ctx)?;
                blocks.push(Block::Para {
                    symbol: "\"".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open { name, .. } if name == "cit" => {
                // A cited quotation: the bibl becomes the
                // blockquote's attribution hypograph.
                let (block, next) = tei_cit(toks, i + 1, ctx)?;
                blocks.push(block);
                i = next;
            }
            Tok::Open { name, .. } if name == "epigraph" => {
                let (block, next) = tei_epigraph(toks, i + 1, ctx)?;
                blocks.push(block);
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "figure" => {
                let onym = attr(attrs, "xml:id").map(str::to_string);
                let (block, bodies, next) = tei_figure(toks, i + 1, ctx, onym)?;
                blocks.push(block);
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "table" => {
                let (block, bodies, next) = tei_table(toks, i + 1, ctx)?;
                blocks.push(block);
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "list" => {
                let ordered = matches!(attr(attrs, "type"), Some("ordered"));
                let (items, next) = tei_list(toks, i + 1, ordered, ctx)?;
                // label/item pairs make it a definition list
                let gloss = items
                    .iter()
                    .any(|b| matches!(b, Block::Para { symbol, .. } if symbol == "::"));
                blocks.push(Block::Para {
                    symbol: if gloss {
                        "::;"
                    } else if ordered {
                        ".."
                    } else {
                        "--"
                    }
                    .to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children: items,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "lb" || name == "pb" => {
                i += 1;
                if !self_closing {
                    i = skip_element(toks, i, name.clone())?;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "note" => {
                if *self_closing {
                    // <note/>: an empty note — nothing to emit
                    i += 1;
                    continue;
                }
                let (children, next) = tei_note_body(toks, i + 1, ctx)?;
                blocks.push(Block::Para {
                    symbol: "^!".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if matches!(name.as_str(), "pb" | "space" | "desc" | "docAuthor") => {
                let sc = *self_closing;
                let n2 = name.clone();
                i += 1;
                if !sc {
                    i = skip_element(toks, i, n2)?;
                }
            }
            Tok::Open { name, .. } if name == "castList" => {
                let (items, next) = tei_cast_list(toks, i + 1, "castList", ctx)?;
                blocks.extend(items);
                i = next;
            }
            Tok::Open { name, .. } if name == "dateline" || name == "trailer" => {
                let n = name.clone();
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                blocks.push(solo_endo("-/", content));
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, attrs, .. }
                if name == "ab"
                    && {
                        let mut probe = i + 1;
                        while matches!(&toks.get(probe), Some(Tok::Text(t)) if t.trim().is_empty())
                        {
                            probe += 1;
                        }
                        matches!(&toks.get(probe), Some(Tok::Open { name: n2, .. }) if n2 == "lg" || n2 == "l")
                    } =>
            {
                let genoses = attr(attrs, "type")
                    .map(|t| vec![t.to_ascii_lowercase()])
                    .unwrap_or_default();
                let (lines, bodies, next) = tei_ab_lines(toks, i + 1, ctx)?;
                blocks.push(Block::Stichoi {
                    symbol: Some("~".to_string()),
                    taxis: None,
                    lemma: Vec::new(),
                    strophes: lines,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses,
                    },
                });
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "salute" || name == "signed" || name == "ab" => {
                let n = name.clone();
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                blocks.push(Block::Paragraph(content));
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "closer" || name == "opener" => {
                let n = name.clone();
                let (inner, next) = tei_blocks(toks, i + 1, &n, depth, ctx)?;
                blocks.extend(inner);
                i = next;
            }
            Tok::Close(name) if until != name.as_str() && name == "p" => {
                // overlapping markup (a said paragraph closed at
                // its </said>, orphaning the outer </p>): the
                // stray close is structure noise — skip
                i += 1;
            }
            other => {
                return Err(tei_err(format!("unsupported {other:?} at block level")));
            }
        }
    }
    Err(tei_err(format!("unterminated <{until}>")))
}

/// Quote content: paragraphs, or a bare inline run.
fn tei_quote_content(
    toks: &[Tok],
    i: usize,
    until: String,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Block>, usize)> {
    // Peek past insignificant whitespace and leading self-closing
    // milestones (a card/para milestone can head a verse quote)
    // to the first structural child.
    let mut probe = i;
    loop {
        match &toks.get(probe) {
            Some(Tok::Text(t)) if t.trim().is_empty() => probe += 1,
            Some(Tok::Open {
                name,
                self_closing: true,
                ..
            }) if name == "milestone" => probe += 1,
            _ => break,
        }
    }
    // Block children — paragraphs OR verse lines/groups (a
    // quotation of verse, e.g. a speech in Homer set as bare
    // <l> lines inside <q>) — parse as blocks; the bare-<l> and
    // <lg> arms of tei_blocks wrap them in stichoi.
    if matches!(&toks.get(probe),
        Some(Tok::Open { name, .. }) if name == "p" || name == "l" || name == "lg")
    {
        return tei_blocks(toks, i, &until, 0, ctx);
    }
    let ((content, bodies), next) = tei_inline_run(toks, i, &until, ctx)?;
    let mut blocks = vec![Block::Paragraph(content)];
    blocks.extend(bodies);
    Ok((blocks, next))
}

/// An lg: head becomes the verse lemma; l children are lines;
/// nested lg groups are strophes.
fn tei_verse(toks: &[Tok], mut i: usize, ctx: &mut TeiCtx) -> Result<(Block, Vec<Block>, usize)> {
    let mut lemma: Vec<Inline> = Vec::new();
    let mut strophes: Vec<Strophe> = Vec::new();
    let mut current: Vec<Vec<Inline>> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "lg" => {
                if !current.is_empty() {
                    strophes.push(Strophe(std::mem::take(&mut current)));
                }
                return Ok((
                    Block::Stichoi {
                        symbol: Some("~".to_string()),
                        taxis: None,
                        lemma,
                        strophes,
                        hypograph: Vec::new(),
                        bracket_matching: true,
                        ann: Annotations::default(),
                    },
                    bodies,
                    i + 1,
                ));
            }
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "head" => {
                let ((content, inner), next) = {
                    let (run, next) = tei_inline_run(toks, i + 1, "head", ctx)?;
                    (run, next)
                };
                lemma = content;
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "l" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((content, inner), next) = {
                    let (run, next) = tei_inline_run(toks, i + 1, "l", ctx)?;
                    (run, next)
                };
                current.push(content);
                bodies.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "lg" => {
                // A nested stanza: flush and recurse one level,
                // taking its lines as one strophe.
                if !current.is_empty() {
                    strophes.push(Strophe(std::mem::take(&mut current)));
                }
                let (inner, inner_bodies, next) = tei_verse(toks, i + 1, ctx)?;
                let Block::Stichoi {
                    lemma: inner_lemma,
                    strophes: mut inner_strophes,
                    ..
                } = inner
                else {
                    unreachable!()
                };
                if !inner_lemma.is_empty() {
                    // A stanza title: a strophe carries no lemma,
                    // so the head leads its strophe as a .head
                    // phrase on a line of its own.
                    let head = vec![Inline::Endo {
                        symbol: ",".to_string(),
                        content: inner_lemma,
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses: vec!["head".to_string()],
                        },
                    }];
                    match inner_strophes.first_mut() {
                        Some(strophe) => strophe.0.insert(0, head),
                        None => inner_strophes.push(Strophe(vec![head])),
                    }
                }
                strophes.extend(inner_strophes);
                bodies.extend(inner_bodies);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "note" => {
                if *self_closing {
                    // <note/>: an empty note — nothing to emit
                    i += 1;
                    continue;
                }
                // A note between lines: the callout rides the end
                // of the previous line.
                ctx.notes += 1;
                let onym = format!("n{}", ctx.notes);
                let (children, next) = tei_note_body(toks, i + 1, ctx)?;
                let deixis = Inline::Deixis {
                    symbol: "^".to_string(),
                    onym: onym.clone(),
                    ann: Annotations::default(),
                };
                match current
                    .last_mut()
                    .or_else(|| strophes.last_mut().and_then(|s| s.0.last_mut()))
                {
                    Some(line) => line.push(deixis),
                    None => current.push(vec![deixis]),
                }
                bodies.push(Block::Para {
                    symbol: "^".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: Some(onym),
                        genoses: Vec::new(),
                    },
                });
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "milestone" || name == "lb" || name == "pb" || name == "gap" => {
                // Furniture between lines, as in a verse speech:
                // a milestone is a line of its own, a gap a
                // lacuna, a line or page break nothing.
                if name == "milestone"
                    && let Some(ms) = attr(attrs, "n")
                        .filter(|n| !tei_milestone_is_title(n))
                        .and_then(|n| tei_milestone_mono(n, attrs))
                {
                    current.push(vec![ms]);
                } else if name == "gap" {
                    current.push(vec![Inline::Text("[\u{2026}]".to_string())]);
                }
                i += 1;
                if !self_closing {
                    i = skip_element(toks, i, name.clone())?;
                }
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <lg>"))),
        }
    }
    Err(tei_err("unterminated <lg>".into()))
}

/// An sp: speaker becomes the dialogue lemma; the speech body
/// is block content. The printed prefix is the speaker; when
/// the source also points at a character (sp/@who), the pointer
/// rides as a prosopon first in the lemma, so the speech reaches
/// its character by key. A speech without a pointer is attributed
/// by its prefix alone.
fn tei_speech(
    toks: &[Tok],
    mut i: usize,
    who: Option<&str>,
    depth: usize,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Block>, usize)> {
    let mut lemma: Vec<Inline> = Vec::new();
    // Note bodies from the speaker run (an editor's note on the
    // dramatis persona) must land with the speech: a dropped
    // body orphans its deixis callout.
    let mut lemma_bodies: Vec<Block> = Vec::new();
    // The speaker leads; everything after is block content.
    loop {
        match toks.get(i) {
            Some(Tok::Text(t)) if t.trim().is_empty() => i += 1,
            Some(Tok::Open { name, .. }) if name == "speaker" => {
                let ((content, inner), next) = {
                    let (run, next) = tei_inline_run(toks, i + 1, "speaker", ctx)?;
                    (run, next)
                };
                lemma = content;
                lemma_bodies = inner;
                i = next;
                break;
            }
            _ => break,
        }
    }
    // A speakerless <sp> stays speakerless (it splices bare
    // below): the pointer annotates a printed prefix, it does
    // not stand in for one.
    if let Some(who) = who.filter(|_| !lemma.is_empty()) {
        lemma.insert(0, aphanes(PROSOPON, who));
    }
    // Verse speeches (l children, the DraCor shape) become the
    // verse-dialogue form; prose and mixed speeches stay
    // dialogue blocks (verse runs inside them become verse
    // blocks at the block level).
    let pure_verse;
    {
        let mut probe = i;
        let mut has_l = false;
        let mut prose = false;
        while probe < toks.len() {
            match &toks[probe] {
                Tok::Close(name) if name == "sp" => break,
                Tok::Open { name, .. } if name == "l" => has_l = true,
                Tok::Open { name, .. } if name == "p" || name == "ab" || name == "quote" => {
                    prose = true;
                }
                _ => {}
            }
            probe += 1;
        }
        pure_verse = has_l && !prose;
    }
    let mut probe = i;
    while matches!(&toks.get(probe), Some(Tok::Text(t)) if t.trim().is_empty()) {
        probe += 1;
    }
    if pure_verse
        && matches!(&toks.get(probe), Some(Tok::Open { name, .. }) if name == "l" || name == "lg")
    {
        let (lines, bodies, next) = tei_speech_lines(toks, probe, ctx)?;
        // a speakerless <sp> (continuation after an interjection)
        // must not open a lemma-less verse-dialogue: its lines
        // splice as a plain stichoi block in the flow
        let symbol = if lemma.is_empty() {
            "~".to_string()
        } else {
            ":~".to_string()
        };
        let mut out = vec![Block::Stichoi {
            symbol: Some(symbol),
            taxis: None,
            lemma,
            strophes: vec![Strophe(lines)],
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations::default(),
        }];
        out.extend(lemma_bodies);
        out.extend(bodies);
        return Ok((out, next));
    }
    let (children, next) = tei_blocks(toks, i, "sp", depth, ctx)?;
    // a speakerless <sp> (continuation) splices bare: dialogue
    // sims require a lemma
    if lemma.is_empty() {
        let mut out = children;
        out.extend(lemma_bodies);
        return Ok((out, next));
    }
    let mut out = vec![Block::Para {
        symbol: ":".to_string(),
        taxis: None,
        lemma,
        children,
        hypograph: Vec::new(),
        bracket_matching: true,
        ann: Annotations::default(),
    }];
    out.extend(lemma_bodies);
    Ok((out, next))
}

/// The lines of a verse speech: l elements are lines, lg
/// wrappers are transparent, stage directions and milestones
/// become lines of their own carrying the inline forms.
fn tei_speech_lines(
    toks: &[Tok],
    mut i: usize,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Vec<Inline>>, Vec<Block>, usize)> {
    let mut lines: Vec<Vec<Inline>> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "sp" => return Ok((lines, bodies, i + 1)),
            Tok::Close(name) if name == "lg" => i += 1,
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "lg" => i += 1,
            Tok::Open {
                name, self_closing, ..
            } if name == "l" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((mut content, inner), next) = tei_inline_run(toks, i + 1, "l", ctx)?;
                trim_inline_edges(&mut content);
                lines.push(content);
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "note" => {
                if *self_closing {
                    // <note/>: an empty note — nothing to emit
                    i += 1;
                    continue;
                }
                // A note between lines: the callout rides the end
                // of the previous line.
                ctx.notes += 1;
                let onym = format!("n{}", ctx.notes);
                let (children, next) = tei_note_body(toks, i + 1, ctx)?;
                let deixis = Inline::Deixis {
                    symbol: "^".to_string(),
                    onym: onym.clone(),
                    ann: Annotations::default(),
                };
                match lines.last_mut() {
                    Some(line) => line.push(deixis),
                    None => lines.push(vec![deixis]),
                }
                bodies.push(Block::Para {
                    symbol: "^".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: Some(onym),
                        genoses: Vec::new(),
                    },
                });
                i = next;
            }
            Tok::Open { name, .. } if name == "stage" => {
                let ((mut content, inner), next) = tei_inline_run(toks, i + 1, "stage", ctx)?;
                trim_inline_edges(&mut content);
                lines.push(vec![Inline::Endo {
                    symbol: ":(".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                }]);
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "milestone" || name == "lb" || name == "pb" || name == "gap" => {
                if name == "milestone"
                    && let Some(ms) = attr(attrs, "n")
                        .filter(|n| !tei_milestone_is_title(n))
                        .and_then(|n| tei_milestone_mono(n, attrs))
                {
                    lines.push(vec![ms]);
                } else if name == "gap" {
                    // lost lines between verses render as a lacuna
                    lines.push(vec![Inline::Text("[\u{2026}]".to_string())]);
                }
                i += 1;
                if !self_closing {
                    i = skip_element(toks, i, name.clone())?;
                }
            }
            other => {
                return Err(tei_err(format!("unsupported {other:?} in a verse speech")));
            }
        }
    }
    Err(tei_err("unterminated <sp>".into()))
}

/// A Perseus dialogue paragraph: <p><said who><label>Speaker.
/// </label> speech</said></p> becomes a dialogue block whose
/// lemma is the label (sans trailing period).
/// Whether the <said> opening at `i` runs to the end of its
/// paragraph (only whitespace between its close and the </p>).
fn tei_said_spans_paragraph(toks: &[Tok], i: usize) -> bool {
    if matches!(
        &toks[i],
        Tok::Open {
            self_closing: true,
            ..
        }
    ) {
        return false;
    }
    let mut depth = 0usize;
    let mut j = i;
    while j < toks.len() {
        match &toks[j] {
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "said" => depth += 1,
            Tok::Close(name) if name == "said" => {
                let Some(d) = depth.checked_sub(1) else {
                    return false;
                };
                depth = d;
                if depth == 0 {
                    j += 1;
                    while matches!(&toks.get(j), Some(Tok::Text(t)) if t.trim().is_empty()) {
                        j += 1;
                    }
                    return matches!(&toks.get(j), Some(Tok::Close(name)) if name == "p");
                }
            }
            _ => {}
        }
        j += 1;
    }
    false
}

fn tei_said_paragraph(toks: &[Tok], mut i: usize, ctx: &mut TeiCtx) -> Result<(Block, usize)> {
    // i points at the <said> open.
    let said_attrs: Vec<(String, String)> = match &toks[i] {
        Tok::Open { attrs, .. } => attrs.clone(),
        _ => Vec::new(),
    };
    let who = attr(&said_attrs, "who").map(|w| w.trim_start_matches('#').to_string());
    i += 1;
    let mut lemma: Vec<Inline> = Vec::new();
    // Milestones may precede the label inside <said> (the
    // Perseus Greek convention); they join the speech content.
    let mut prefix: Vec<Inline> = Vec::new();
    loop {
        let mut probe = i;
        while matches!(&toks.get(probe), Some(Tok::Text(t)) if t.trim().is_empty()) {
            probe += 1;
        }
        match &toks.get(probe) {
            Some(Tok::Open {
                name,
                attrs,
                self_closing,
            }) if name == "milestone" => {
                if let Some(ms) = attr(attrs, "n")
                    .filter(|n| !tei_milestone_is_title(n))
                    .and_then(|n| tei_milestone_mono(n, attrs))
                {
                    prefix.push(ms);
                }
                i = probe + 1;
                if !self_closing {
                    i = skip_element(toks, i, "milestone".to_string())?;
                }
            }
            _ => break,
        }
    }
    let mut probe = i;
    while matches!(&toks.get(probe), Some(Tok::Text(t)) if t.trim().is_empty()) {
        probe += 1;
    }
    // A note in the label spawns a body that lands with the
    // speech, ahead of the speech's own.
    let mut bodies: Vec<Block> = Vec::new();
    if matches!(&toks.get(probe), Some(Tok::Open { name, .. }) if name == "label") {
        let ((mut content, inner), next) = tei_inline_run(toks, probe + 1, "label", ctx)?;
        trim_inline_edges(&mut content);
        // the label's period is print furniture, even before a
        // callout
        if let Some(Inline::Text(t)) = content
            .iter_mut()
            .rev()
            .find(|x| !matches!(x, Inline::Deixis { .. }))
        {
            *t = t.trim_end_matches('.').to_string();
        }
        lemma = content;
        bodies = inner;
        i = next;
    }
    let ((mut content, said_bodies), next) = tei_inline_run(toks, i, "said", ctx)?;
    bodies.extend(said_bodies);
    trim_inline_edges(&mut content);
    if !prefix.is_empty() {
        prefix.extend(content);
        content = prefix;
    }
    // Consume the closing </p>.
    let mut j = next;
    while matches!(&toks.get(j), Some(Tok::Text(t)) if t.trim().is_empty()) {
        j += 1;
    }
    if matches!(&toks.get(j), Some(Tok::Close(name)) if name == "p") {
        j += 1;
    }
    if lemma.is_empty() && (who.is_some() || attr(&said_attrs, "ana").is_some()) {
        // Prose fiction: a speech paragraph attributed by pointer
        // alone (no printed speaker) is a dialogue line, the
        // speaker riding as an unseen prosopon first inside it.
        let line = Inline::Endo {
            symbol: ":-".to_string(),
            content: with_aphanes(tei_said_marks(&said_attrs), content),
            bracket_matching: true,
            ann: Annotations {
                onym: None,
                genoses: tei_said_mode(&said_attrs),
            },
        };
        let mut children = vec![Block::Paragraph(vec![line])];
        children.extend(bodies);
        return Ok((
            Block::ParaDiaphane {
                children,
                ann: Annotations::default(),
            },
            j,
        ));
    }
    let mut children = vec![Block::Paragraph(content)];
    children.extend(bodies);
    if lemma.is_empty() {
        // said/speech without an attribution: a plain paragraph
        return Ok((
            Block::ParaDiaphane {
                children,
                ann: Annotations::default(),
            },
            j,
        ));
    }
    // A printed label with a pointer: the drama block keeps its
    // prefix and carries the pointer first in the lemma.
    if let Some(who) = &who {
        lemma.insert(0, aphanes(PROSOPON, who));
    }
    Ok((
        Block::Para {
            symbol: ":".to_string(),
            taxis: None,
            lemma,
            children,
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations::default(),
        },
        j,
    ))
}

/// A cit: a block quotation whose bibl becomes the
/// attribution hypograph.
fn tei_cit(toks: &[Tok], mut i: usize, ctx: &mut TeiCtx) -> Result<(Block, usize)> {
    let mut children: Vec<Block> = Vec::new();
    let mut hypograph: Vec<Inline> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "cit" => {
                return Ok((
                    Block::Para {
                        symbol: "\"".to_string(),
                        taxis: None,
                        lemma: Vec::new(),
                        children,
                        hypograph,
                        bracket_matching: true,
                        ann: Annotations::default(),
                    },
                    i + 1,
                ));
            }
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "quote" || name == "q" => {
                let (inner, next) = tei_quote_content(toks, i + 1, name.clone(), ctx)?;
                children.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "bibl" => {
                if *self_closing {
                    // <bibl/>: citation data in attributes only
                    i += 1;
                    continue;
                }
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, "bibl", ctx)?;
                hypograph = content;
                children.extend(bodies);
                i = next;
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <cit>"))),
        }
    }
    Err(tei_err("unterminated <cit>".into()))
}

/// An epigraph: quote content plus a bibl attribution as the
/// hypograph.
fn tei_epigraph(toks: &[Tok], mut i: usize, ctx: &mut TeiCtx) -> Result<(Block, usize)> {
    let mut children: Vec<Block> = Vec::new();
    let mut hypograph: Vec<Inline> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "epigraph" => {
                return Ok((
                    Block::Para {
                        symbol: "\"/".to_string(),
                        taxis: None,
                        lemma: Vec::new(),
                        children,
                        hypograph,
                        bracket_matching: true,
                        ann: Annotations::default(),
                    },
                    i + 1,
                ));
            }
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "quote" || name == "q" => {
                let (inner, next) = tei_quote_content(toks, i + 1, name.clone(), ctx)?;
                children.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "p" => {
                let ((content, bodies), next) = {
                    let (run, next) = tei_inline_run(toks, i + 1, "p", ctx)?;
                    (run, next)
                };
                children.push(Block::Paragraph(content));
                children.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "cit" => i += 1,
            Tok::Close(name) if name == "cit" => i += 1,
            Tok::Open {
                name, self_closing, ..
            } if name == "pb" || name == "gap" => {
                let sc = *self_closing;
                let n2 = name.clone();
                i += 1;
                if !sc && matches!(toks.get(i), Some(Tok::Close(n)) if *n == n2) {
                    i += 1;
                }
            }
            Tok::Open { name, .. } if name == "lg" => {
                let (block, bodies, next) = tei_verse(toks, i + 1, ctx)?;
                children.push(block);
                children.extend(bodies);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "bibl" => {
                if *self_closing {
                    // <bibl/>: citation data in attributes only
                    i += 1;
                    continue;
                }
                let ((content, bodies), next) = {
                    let (run, next) = tei_inline_run(toks, i + 1, "bibl", ctx)?;
                    (run, next)
                };
                hypograph = content;
                children.extend(bodies);
                i = next;
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <epigraph>"))),
        }
    }
    Err(tei_err("unterminated <epigraph>".into()))
}

/// The front matter, tolerantly: docTitle/titlePart become the
/// title, byline/docAuthor the author, epigraph and argument
/// carry over; everything else in front is skipped (TEI front
/// matter is a grab bag; the body stays strict).
fn tei_front(
    toks: &[Tok],
    mut i: usize,
    blocks: &mut Vec<Block>,
    ctx: &mut TeiCtx,
) -> Result<usize> {
    let mut depth = 1;
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "front" => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i + 1);
                }
                i += 1;
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "front" => {
                depth += 1;
                i += 1;
            }
            Tok::Open { name, .. } if name == "titlePart" => {
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, "titlePart", ctx)?;
                blocks.push(solo_endo("=", content));
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "byline" || name == "docAuthor" => {
                let n = name.clone();
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                blocks.push(solo_endo("=:", content));
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "epigraph" => {
                let (block, next) = tei_epigraph(toks, i + 1, ctx)?;
                blocks.push(block);
                i = next;
            }
            Tok::Open { name, .. } if name == "argument" => {
                let (children, next) = tei_blocks(toks, i + 1, "argument", 0, ctx)?;
                blocks.push(Block::Para {
                    symbol: "=\"".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name != "docTitle" => {
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(toks, i + 1, name.clone())?;
                }
            }
            _ => i += 1,
        }
    }
    Err(tei_err("unterminated <front>".into()))
}

/// A figure: graphic becomes the enmedia, head the caption,
/// xml:id the onym (internal refs link to it).
fn tei_figure(
    toks: &[Tok],
    mut i: usize,
    ctx: &mut TeiCtx,
    onym: Option<String>,
) -> Result<(Block, Vec<Block>, usize)> {
    let mut lemma: Vec<Inline> = Vec::new();
    let mut children: Vec<Block> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "figure" => {
                return Ok((
                    Block::Para {
                        symbol: "<".to_string(),
                        taxis: None,
                        lemma,
                        children,
                        hypograph: Vec::new(),
                        bracket_matching: false,
                        ann: Annotations {
                            onym,
                            genoses: Vec::new(),
                        },
                    },
                    bodies,
                    i + 1,
                ));
            }
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "graphic" => {
                if let Some(url) = attr(attrs, "url") {
                    children.push(Block::Enmedia {
                        param: url.to_string(),
                    });
                }
                i += 1;
                if !self_closing && matches!(toks.get(i), Some(Tok::Close(n)) if n == "graphic") {
                    i += 1;
                }
            }
            Tok::Open { name, .. } if name == "head" => {
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "head", ctx)?;
                lemma = content;
                bodies.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "figDesc" => {
                i = skip_element(toks, i + 1, "figDesc".to_string())?;
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <figure>"))),
        }
    }
    Err(tei_err("unterminated <figure>".into()))
}

/// A table: rows become stichoi lines, cells pipe-separated;
/// head becomes the caption lemma.
fn tei_table(toks: &[Tok], mut i: usize, ctx: &mut TeiCtx) -> Result<(Block, Vec<Block>, usize)> {
    let mut lemma: Vec<Inline> = Vec::new();
    let mut rows: Vec<Vec<Inline>> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "table" => {
                return Ok((
                    Block::Stichoi {
                        symbol: Some("+".to_string()),
                        taxis: None,
                        lemma,
                        strophes: vec![Strophe(rows)],
                        hypograph: Vec::new(),
                        bracket_matching: true,
                        ann: Annotations::default(),
                    },
                    bodies,
                    i + 1,
                ));
            }
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "head" => {
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "head", ctx)?;
                lemma = content;
                bodies.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "row" => {
                // Cells keep their inline forms (a phrase, a
                // name, a callout), pipe-separated in one line.
                let mut row: Vec<Inline> = vec![Inline::Text("| ".to_string())];
                let mut first = true;
                i += 1;
                loop {
                    match toks.get(i) {
                        None => return Err(tei_err("unterminated <row>".into())),
                        Some(Tok::Close(name)) if name == "row" => {
                            i += 1;
                            break;
                        }
                        Some(Tok::Text(t)) if t.trim().is_empty() => i += 1,
                        Some(Tok::Open { name, .. }) if name == "cell" => {
                            let ((mut content, inner), next) =
                                tei_inline_run(toks, i + 1, "cell", ctx)?;
                            trim_inline_edges(&mut content);
                            if !first {
                                row.push(Inline::Text(" | ".to_string()));
                            }
                            first = false;
                            row.extend(content);
                            bodies.extend(inner);
                            i = next;
                        }
                        other => {
                            return Err(tei_err(format!("unsupported {other:?} in <row>")));
                        }
                    }
                }
                row.push(Inline::Text(" |".to_string()));
                rows.push(row);
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <table>"))),
        }
    }
    Err(tei_err("unterminated <table>".into()))
}

/// A list's items (flat).
fn tei_list(
    toks: &[Tok],
    mut i: usize,
    ordered: bool,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Block>, usize)> {
    let mut items: Vec<Block> = Vec::new();
    let mut n = 0u64;
    // A gloss list pairs each label with the item after it: the
    // pair is a definition item, the label its term.
    let mut label: Option<Vec<Inline>> = None;
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "list" => return Ok((items, i + 1)),
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open {
                name, self_closing, ..
            } if name == "label" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((mut content, bodies), next) = tei_inline_run(toks, i + 1, "label", ctx)?;
                trim_inline_edges(&mut content);
                label = (!content.is_empty()).then_some(content);
                items.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "item" && label.is_some() => {
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, "item", ctx)?;
                items.push(Block::Para {
                    symbol: "::".to_string(),
                    taxis: None,
                    lemma: label.take().unwrap_or_default(),
                    children: vec![Block::Para {
                        symbol: ";".to_string(),
                        taxis: None,
                        lemma: Vec::new(),
                        children: vec![Block::Paragraph(content)],
                        hypograph: Vec::new(),
                        bracket_matching: true,
                        ann: Annotations::default(),
                    }],
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                items.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "head" => {
                // A list head reads as a preceding run-in.
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, "head", ctx)?;
                items.push(Block::Paragraph(vec![Inline::Endo {
                    symbol: "#_".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                }]));
                items.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "item" => {
                n += 1;
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, "item", ctx)?;
                items.push(Block::Para {
                    symbol: if ordered { ".-" } else { "-" }.to_string(),
                    taxis: ordered.then_some(Taxis::Explicit(n)),
                    lemma: Vec::new(),
                    children: vec![Block::Paragraph(content)],
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                items.extend(bodies);
                i = next;
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <list>"))),
        }
    }
    Err(tei_err("unterminated <list>".into()))
}

/// A castList: castItems become dramatis-persona lines.
fn tei_cast_list(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Block>, usize)> {
    let mut items: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => return Ok((items, i + 1)),
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open {
                name, self_closing, ..
            } if name == "castGroup" => {
                // A group of characters: its head is a run-in
                // heading, its items join the cast in order.
                if *self_closing {
                    i += 1;
                    continue;
                }
                let (inner, next) = tei_cast_list(toks, i + 1, "castGroup", ctx)?;
                items.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "roleDesc" => {
                // The description a group shares.
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, "roleDesc", ctx)?;
                if !content.is_empty() {
                    items.push(Block::Paragraph(content));
                }
                items.extend(bodies);
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "castItem" || name == "head" => {
                let n = name.clone();
                let onym = if n == "castItem" {
                    xml_id_onym(attrs)
                } else {
                    None
                };
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                items.push(solo_endo_onym(
                    if n == "head" { "#_" } else { ":!" },
                    content,
                    onym,
                ));
                items.extend(bodies);
                i = next;
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <{until}>"))),
        }
    }
    Err(tei_err(format!("unterminated <{until}>")))
}

/// A listPerson: each person becomes a dramatis-persona line -
/// the persName as the name, the xml:id as the onym every
/// prosopon key resolves against - or, when it carries a note, a
/// character entry whose description is the note. A head is a
/// run-in heading; nested lists flatten. The prosopographic
/// detail (birth, death, sex, occupation, ...) is recorded loss.
fn tei_list_person(toks: &[Tok], mut i: usize, ctx: &mut TeiCtx) -> Result<(Vec<Block>, usize)> {
    let mut items: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "listPerson" => return Ok((items, i + 1)),
            Tok::Text(_) => i += 1,
            Tok::Open {
                name, self_closing, ..
            } if name == "listPerson" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let (inner, next) = tei_list_person(toks, i + 1, ctx)?;
                items.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "head" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((mut content, bodies), next) = tei_inline_run(toks, i + 1, "head", ctx)?;
                trim_run(&mut content);
                items.push(solo_endo("#_", content));
                items.extend(bodies);
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
                ..
            } if name == "person" || name == "personGrp" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let n2 = name.clone();
                let onym = xml_id_onym(attrs);
                let (block, next) = tei_person(toks, i + 1, &n2, onym, ctx)?;
                items.extend(block);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } => {
                let sc = *self_closing;
                let n2 = name.clone();
                i += 1;
                if !sc {
                    i = skip_element(toks, i, n2)?;
                }
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <listPerson>"))),
        }
    }
    Err(tei_err("unterminated <listPerson>".into()))
}

/// One person (or personGrp) of a listPerson: the first persName
/// is the name, the first note the description; the rest is
/// skipped. Without a name there is nothing to declare.
fn tei_person(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    onym: Option<String>,
    ctx: &mut TeiCtx,
) -> Result<(Vec<Block>, usize)> {
    let mut name: Option<Vec<Inline>> = None;
    let mut name_bodies: Vec<Block> = Vec::new();
    let mut description: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(n) if n == until => {
                let Some(mut name) = name else {
                    return Ok((name_bodies, i + 1));
                };
                trim_run(&mut name);
                if name.is_empty() {
                    return Ok((name_bodies, i + 1));
                }
                let block = if description.is_empty() {
                    solo_endo_onym(":!", name, onym)
                } else {
                    Block::Para {
                        symbol: ":!!".to_string(),
                        taxis: None,
                        lemma: name,
                        children: description,
                        hypograph: Vec::new(),
                        bracket_matching: true,
                        ann: Annotations {
                            onym,
                            genoses: Vec::new(),
                        },
                    }
                };
                let mut out = vec![block];
                out.extend(name_bodies);
                return Ok((out, i + 1));
            }
            Tok::Text(_) => i += 1,
            Tok::Open {
                name: n,
                self_closing,
                ..
            } if n == "persName" && name.is_none() && !*self_closing => {
                let ((content, bodies), next) = tei_inline_run(toks, i + 1, "persName", ctx)?;
                name = Some(content);
                name_bodies = bodies;
                i = next;
            }
            Tok::Open {
                name: n,
                self_closing,
                ..
            } if n == "note" && description.is_empty() && !*self_closing => {
                let (children, next) = tei_note_body(toks, i + 1, ctx)?;
                description = children;
                i = next;
            }
            Tok::Open {
                name: n,
                self_closing,
                ..
            } => {
                let sc = *self_closing;
                let n2 = n.clone();
                i += 1;
                if !sc {
                    i = skip_element(toks, i, n2)?;
                }
            }
            other => return Err(tei_err(format!("unsupported {other:?} in <{until}>"))),
        }
    }
    Err(tei_err(format!("unterminated <{until}>")))
}

/// Parse an inline run until the closing tag `until`. Returns
/// the inlines plus any footnote bodies spawned by notes.
#[allow(clippy::type_complexity)]
fn tei_inline_run(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    ctx: &mut TeiCtx,
) -> Result<((Vec<Inline>, Vec<Block>), usize)> {
    let mut inlines: Vec<Inline> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => {
                trim_run(&mut inlines);
                return Ok(((inlines, bodies), i + 1));
            }
            Tok::Text(t) => {
                // XML prose whitespace is insignificant:
                // collapse runs (including newlines) to single
                // spaces.
                let decoded = decode_entities(t);
                let mut collapsed = String::with_capacity(decoded.len());
                let mut in_ws = false;
                for c in decoded.chars() {
                    if c.is_whitespace() {
                        if !in_ws {
                            collapsed.push(' ');
                        }
                        in_ws = true;
                    } else {
                        collapsed.push(c);
                        in_ws = false;
                    }
                }
                if !collapsed.is_empty() {
                    inlines.push(Inline::Text(collapsed));
                }
                i += 1;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "lb" || name == "pb" => {
                inlines.push(Inline::Text(" ".to_string()));
                i += 1;
                if !self_closing && matches!(toks.get(i), Some(Tok::Close(n)) if n == name) {
                    i += 1;
                }
            }
            Tok::Open { name, attrs, .. }
                if name == "hi" || name == "emph" || name == "foreign" || name == "title" =>
            {
                if matches!(
                    &toks[i],
                    Tok::Open {
                        self_closing: true,
                        ..
                    }
                ) {
                    i += 1;
                    continue;
                }
                let (symbol, genoses): (&str, Vec<String>) = match name.as_str() {
                    "emph" => ("/", vec![]),
                    "foreign" => {
                        let mut genoses = vec!["foreign".to_string()];
                        if let Some(lang) = attr(attrs, "xml:lang") {
                            genoses.push(lang.to_lowercase());
                        }
                        ("/", genoses)
                    }
                    "title" => ("/", vec!["title".to_string()]),
                    _ => match attr(attrs, "rend") {
                        Some("bold") | Some("b") => ("*", vec![]),
                        _ => ("/", vec![]),
                    },
                };
                let n = name.clone();
                let ((content, inner_bodies), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                inlines.push(Inline::Endo {
                    symbol: symbol.to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses,
                    },
                });
                bodies.extend(inner_bodies);
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "q" || name == "said" => {
                if *self_closing {
                    // <q/> (Perseus type="unspecified"): an empty
                    // quotation-boundary marker — nothing to emit;
                    // consuming it as an open would swallow the
                    // enclosing element's close
                    i += 1;
                    continue;
                }
                let mut genoses = if name == "said" || attr(attrs, "who").is_some() {
                    vec!["said".to_string()]
                } else {
                    Vec::new()
                };
                genoses.extend(tei_said_mode(attrs));
                let marks = tei_said_marks(attrs);
                let n = name.clone();
                let ((content, inner_bodies), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                inlines.push(Inline::Endo {
                    symbol: "\"\"".to_string(),
                    content: with_aphanes(marks, content),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses,
                    },
                });
                bodies.extend(inner_bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "stage" => {
                let ((content, inner_bodies), next) = tei_inline_run(toks, i + 1, "stage", ctx)?;
                inlines.push(Inline::Endo {
                    symbol: ":(".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                bodies.extend(inner_bodies);
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "ref" || name == "ptr" => {
                let target = attr(attrs, "target").unwrap_or("").to_string();
                let self_closing = matches!(
                    &toks[i],
                    Tok::Open {
                        self_closing: true,
                        ..
                    }
                );
                // A target lists one or more pointers; when every
                // one is internal (#id), each id is resolved on
                // its own.
                let ids: Vec<&str> = if target.split_whitespace().all(|t| t.starts_with('#')) {
                    target
                        .split_whitespace()
                        .map(|t| &t[1..])
                        .filter(|id| !id.is_empty())
                        .collect()
                } else {
                    Vec::new()
                };
                if !ids.is_empty() {
                    // A paired ref whose matching note follows
                    // directly is a printed callout duplicate:
                    // drop it, the deixis takes over. A
                    // self-closing <ptr/> ends where it opens.
                    let mut probe = i;
                    if !self_closing {
                        probe += 1;
                        while probe < toks.len()
                            && !matches!(&toks[probe], Tok::Close(n2) if n2 == name)
                        {
                            probe += 1;
                        }
                    }
                    let mut after = probe + 1;
                    while matches!(&toks.get(after), Some(Tok::Text(t)) if t.trim().is_empty()) {
                        after += 1;
                    }
                    if let Some(Tok::Open {
                        name: n2,
                        attrs: a2,
                        ..
                    }) = &toks.get(after)
                        && n2 == "note"
                        && attr(a2, "xml:id").is_some_and(|id| ids.contains(&id))
                    {
                        i = after;
                        continue;
                    }
                }
                let n = name.clone();
                let (content, next) = if self_closing {
                    (Vec::new(), i + 1)
                } else {
                    let ((c, inner), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                    bodies.extend(inner);
                    (c, next)
                };
                if !ids.is_empty() {
                    // A pointer at a bibliography entry is a cite,
                    // and so is one typed bibr whose entry is
                    // missing. The cite annotates its printed
                    // reference: it opens a diaphane over the
                    // ref's text; an empty pointer is the bare
                    // mark. Several targets are several marks.
                    let bibr = attr(attrs, "type") == Some("bibr");
                    let mono = |id: &str| Inline::Monosim {
                        symbol: if bibr || ctx.bib_keys.contains(id) {
                            ">["
                        } else {
                            ">"
                        }
                        .to_string(),
                        param: id.to_string(),
                        ann: Annotations::default(),
                    };
                    let cited = bibr || ids.iter().any(|id| ctx.bib_keys.contains(*id));
                    if cited {
                        let mut span: Vec<Inline> = ids.iter().map(|id| mono(id)).collect();
                        if content.is_empty() {
                            inlines.extend(span);
                        } else {
                            span.extend(content);
                            inlines.push(Inline::EndoDiaphane {
                                content: span,
                                ann: Annotations::default(),
                            });
                        }
                    } else {
                        // Internal reference: the text stays, the
                        // ref mono follows it.
                        inlines.extend(content);
                        inlines.extend(ids.iter().map(|id| mono(id)));
                    }
                } else if !target.is_empty() {
                    // External link.
                    if !content.is_empty() {
                        inlines.extend(content);
                        inlines.push(Inline::Text(" (".to_string()));
                        inlines.push(Inline::Endo {
                            symbol: "><".to_string(),
                            content: vec![Inline::Text(target)],
                            bracket_matching: true,
                            ann: Annotations::default(),
                        });
                        inlines.push(Inline::Text(")".to_string()));
                    } else {
                        inlines.push(Inline::Endo {
                            symbol: "><".to_string(),
                            content: vec![Inline::Text(target)],
                            bracket_matching: true,
                            ann: Annotations::default(),
                        });
                    }
                } else {
                    inlines.extend(content);
                }
                i = next;
            }
            Tok::Open { name, .. } if name == "choice" => {
                // Reading text: expan/corr/reg, with abbr/sic/orig
                // as the paradosis aphanes.
                let (preferred, fallback, next) = tei_choice(toks, i + 1, ctx)?;
                inlines.extend(tei_choice_inlines(preferred, fallback));
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "gap" || name == "milestone" => {
                if name == "gap" {
                    inlines.push(Inline::Text("[\u{2026}]".to_string()));
                } else if let Some(ms) = attr(attrs, "n")
                    .filter(|n| !tei_milestone_is_title(n))
                    .and_then(|n| tei_milestone_mono(n, attrs))
                {
                    inlines.push(ms);
                } else {
                    // an n-less milestone (unit=para print-
                    // paragraph anchor) still separates words
                    inlines.push(Inline::Text(" ".to_string()));
                }
                i += 1;
                if !self_closing {
                    i = skip_element(toks, i, name.clone())?;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "del" => {
                // editorially-deleted text: the printed edition
                // still shows it (struck/bracketed) — keep it as
                // a .del phrase rather than dropping words
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "del", ctx)?;
                inlines.push(Inline::Endo {
                    symbol: ",".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec!["del".to_string()],
                    },
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if matches!(name.as_str(), "w" | "pc" | "s") && tei_parsing_span(name, attrs) => {
                // Tokens and sentences with a parsing: the parsing
                // pack's diaphanes (at-epimerismos); litosis
                // unwraps them.
                if *self_closing {
                    i += 1;
                    continue;
                }
                let n = name.clone();
                let ((content, inner), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                inlines.push(tei_parsing_inline(&n, attrs, content));
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if matches!(
                name.as_str(),
                "name" | "persName" | "placeName" | "orgName" | "rs" | "date" | "dateRange" | "seg"
            ) && tei_aphanes_span(name, attrs).is_some() =>
            {
                // A name, date, or seg carrying an unseen
                // attribute (ref, key, when, ana): the span stays,
                // the attribute rides first inside it as an
                // at-aphanes monosim.
                let (genos, marks) = tei_aphanes_span(name, attrs).unwrap();
                if *self_closing {
                    i += 1;
                    continue;
                }
                let n = name.clone();
                let ((content, inner), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                inlines.push(Inline::Endo {
                    symbol: ",".to_string(),
                    content: with_aphanes(marks, content),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![genos],
                    },
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if matches!(
                name.as_str(),
                "add"
                    | "name"
                    | "persName"
                    | "placeName"
                    | "orgName"
                    | "rs"
                    | "date"
                    | "dateRange"
                    | "author"
                    | "time"
                    | "num"
                    | "measure"
                    | "mentioned"
                    | "seg"
                    | "forename"
                    | "surname"
                    | "roleName"
                    | "genName"
                    | "nameLink"
                    | "span"
                    | "s"
                    | "addName"
                    | "abbr"
                    | "ex"
                    | "expan"
                    | "w"
                    | "role"
                    | "roleDesc"
                    | "actor"
            ) =>
            {
                // Transparent wrappers: the text carries, the
                // markup does not (reading-text policy). A
                // self-closing form carries nothing.
                if *self_closing {
                    i += 1;
                    continue;
                }
                let n = name.clone();
                let ((content, inner), next) = tei_inline_run(toks, i + 1, &n, ctx)?;
                inlines.extend(content);
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "bibl" => {
                // an inline citation (after a cit quote): the
                // text carries, space-separated from neighbours
                if *self_closing {
                    // <bibl/>: citation data in attributes only
                    i += 1;
                    continue;
                }
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "bibl", ctx)?;
                if !matches!(inlines.last(), Some(Inline::Text(t)) if t.ends_with(char::is_whitespace))
                    && !inlines.is_empty()
                {
                    inlines.push(Inline::Text(" ".to_string()));
                }
                inlines.extend(content);
                inlines.push(Inline::Text(" ".to_string()));
                bodies.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "soCalled" => {
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "soCalled", ctx)?;
                inlines.push(Inline::Endo {
                    symbol: "\"\"".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "term" => {
                let ((content, inner), next) = tei_inline_run(toks, i + 1, "term", ctx)?;
                inlines.push(Inline::Endo {
                    symbol: "/".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec!["term".to_string()],
                    },
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "note" => {
                if *self_closing {
                    // <note/>: an empty note — nothing to emit
                    i += 1;
                    continue;
                }
                // A footnote: the callout is a deixis, the body a
                // litogramma footnote block emitted after the
                // enclosing paragraph. Bodies may hold their own
                // paragraphs; an empty note vanishes.
                let save = ctx.notes;
                ctx.notes += 1;
                let onym = format!("n{}", ctx.notes);
                let (children, next) = tei_note_body(toks, i + 1, ctx)?;
                if children
                    .iter()
                    .all(|b| matches!(b, Block::Paragraph(v) if v.is_empty()))
                {
                    ctx.notes = save;
                    i = next;
                    continue;
                }
                inlines.push(Inline::Deixis {
                    symbol: "^".to_string(),
                    onym: onym.clone(),
                    ann: Annotations::default(),
                });
                bodies.push(Block::Para {
                    symbol: "^".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: Some(onym),
                        genoses: Vec::new(),
                    },
                });
                i = next;
            }
            Tok::Open { name, .. } if name == "idno" => {
                // Catalogue identifiers are annotation, not text.
                i = skip_element(toks, i + 1, "idno".to_string())?;
            }
            Tok::Open { name, .. } if name == "cit" => {
                // An inline cited quotation: the quote renders as
                // a quotation phrase (verse lines solidus-joined),
                // the bibl citation follows as text.
                i += 1;
                continue;
            }
            Tok::Close(name) if name == "cit" => {
                i += 1;
                continue;
            }
            Tok::Open { name, attrs, .. } if name == "quote" && tei_is_verse_quote(toks, i) => {
                // An inline verse quotation: lines join with the
                // classic solidus convention inside a quotation
                // phrase.
                let _ = attrs;
                let (content, vq_bodies, next) = tei_inline_verse_quote(toks, i + 1, ctx)?;
                inlines.push(Inline::Endo {
                    symbol: "\"\"".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                bodies.extend(vq_bodies);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if matches!(
                name.as_str(),
                "p" | "div"
                    | "ab"
                    | "text"
                    | "body"
                    | "head"
                    | "opener"
                    | "closer"
                    | "salute"
                    | "dateline"
                    | "signed"
            ) =>
            {
                // a paragraph (or a whole div section, an ab
                // block, an embedded quoted document with its
                // letter furniture) inside an inline context
                // (long Perseus notes): flatten
                if *self_closing {
                    i += 1;
                    continue;
                }
                if !inlines.is_empty() {
                    inlines.push(Inline::Text(" ".to_string()));
                }
                let n2 = name.clone();
                let ((content, inner), next) = tei_inline_run(toks, i + 1, &n2, ctx)?;
                inlines.extend(content);
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "quote" => {
                // prose quotation inline: a quotation phrase
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((mut content, inner), next) = tei_inline_run(toks, i + 1, "quote", ctx)?;
                trim_inline_edges(&mut content);
                inlines.push(Inline::Endo {
                    symbol: "\"\"".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "app" => {
                // apparatus criticus: the lem reading is the
                // text, the variants are not
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((content, inner), next) = tei_app_lem(toks, i + 1, ctx)?;
                inlines.extend(content);
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "delSpan" || name == "addSpan" || name == "anchor" => {
                // editorial span anchors (deletion/addition to a
                // #target) and their #xml:id targets: apparatus,
                // not text — skip
                let sc = *self_closing;
                let n2 = name.clone();
                i += 1;
                if !sc && matches!(toks.get(i), Some(Tok::Close(n)) if *n == n2) {
                    i += 1;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "reg" => {
                // standalone <reg> = a regularized/gazetteer form
                // (Herodotus place annotations): apparatus, drop
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(toks, i + 1, "reg".to_string())?;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if matches!(
                name.as_str(),
                "gloss" | "sic" | "corr" | "unclear" | "label" | "supplied"
            ) =>
            {
                // editorial wrappers: the text stays, typed as a
                // phrase genos
                if *self_closing {
                    i += 1;
                    continue;
                }
                let n2 = name.clone();
                let ((content, inner), next) = tei_inline_run(toks, i + 1, &n2, ctx)?;
                inlines.push(Inline::Endo {
                    symbol: ",".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![n2],
                    },
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "l" => {
                // a loose verse line in an inline context: the
                // solidus convention
                if *self_closing {
                    i += 1;
                    continue;
                }
                let ((mut content, inner), next) = tei_inline_run(toks, i + 1, "l", ctx)?;
                trim_inline_edges(&mut content);
                if !inlines.is_empty() {
                    inlines.push(Inline::Text(" / ".to_string()));
                }
                inlines.extend(content);
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "listBibl" => {
                // The bibliography was read ahead of the text
                // (tei_bibliography), wherever the list sits —
                // in a note as well.
                if *self_closing {
                    i += 1;
                } else {
                    i = skip_element(toks, i + 1, "listBibl".to_string())?;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if matches!(name.as_str(), "listPerson" | "castList" | "castGroup") => {
                // dramatis-personae furniture inside a cast line —
                // skipped wholesale like the block-level cast list.
                if *self_closing {
                    i += 1;
                } else {
                    let n2 = name.clone();
                    i = skip_element(toks, i + 1, n2)?;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if matches!(name.as_str(), "space" | "desc" | "figure") => {
                // <space/> (metrical gap), <desc> (editorial
                // description), and an inline <figure> (a print
                // mark anchor) are print furniture — skipped.
                if *self_closing {
                    i += 1;
                } else {
                    let n2 = name.clone();
                    i = skip_element(toks, i + 1, n2)?;
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "list" => {
                // an inline list flattens, items joined "; "
                if *self_closing {
                    i += 1;
                    continue;
                }
                let close = name.clone();
                i += 1;
                let mut first = true;
                loop {
                    let tok = toks
                        .get(i)
                        .ok_or_else(|| tei_err(format!("unterminated <{close}>")))?;
                    match tok {
                        Tok::Close(n) if *n == close => {
                            i += 1;
                            break;
                        }
                        Tok::Text(t) if t.trim().is_empty() => i += 1,
                        Tok::Open {
                            name, self_closing, ..
                        } if matches!(name.as_str(), "milestone" | "pb" | "lb") => {
                            let sc = *self_closing;
                            let n2 = name.clone();
                            i += 1;
                            if !sc && matches!(toks.get(i), Some(Tok::Close(n)) if *n == n2) {
                                i += 1;
                            }
                        }
                        Tok::Open {
                            name, self_closing, ..
                        } if matches!(name.as_str(), "item" | "person" | "head" | "label") => {
                            if *self_closing {
                                i += 1;
                                continue;
                            }
                            let n2 = name.clone();
                            let ((content, inner), next) = tei_inline_run(toks, i + 1, &n2, ctx)?;
                            if !first {
                                inlines.push(Inline::Text("; ".to_string()));
                            } else if !inlines.is_empty() {
                                inlines.push(Inline::Text(" ".to_string()));
                            }
                            inlines.extend(content);
                            bodies.extend(inner);
                            first = false;
                            i = next;
                        }
                        other => {
                            return Err(tei_err(format!(
                                "unsupported {other:?} in an inline list"
                            )));
                        }
                    }
                }
            }
            Tok::Close(name)
                if until != name.as_str()
                    && matches!(
                        name.as_str(),
                        "p" | "l" | "quote" | "sp" | "persName" | "note" | "q" | "person"
                    ) =>
            {
                // overlapping markup (a block closed across the
                // run's opener, e.g. <p>…<quote>…</p>…</quote>):
                // the stray close is structure noise — skip
                i += 1;
            }
            other => {
                let near = toks[..i]
                    .iter()
                    .rev()
                    .find_map(|t| match t {
                        Tok::Text(x) if !x.trim().is_empty() => Some(x.trim()),
                        _ => None,
                    })
                    .unwrap_or("");
                let near: String = near
                    .chars()
                    .rev()
                    .take(60)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                return Err(tei_err(format!(
                    "unsupported {other:?} inline (near: \"...{near}\")"
                )));
            }
        }
    }
    Err(tei_err(format!("unterminated <{until}>")))
}

// ---------------------------------------------------------------
// Org-mode (subset) -> at-org
// ---------------------------------------------------------------
//
// A useful Org subset: star headings (levels 1-5; #+TITLE as a
// level-1 heading), paragraphs, quote/verse/example/src blocks
// (src language as an enlexis genos), pipe tables as stichoi
// rows, flat bullet/numbered/description items, footnotes as
// deixis callouts with definition bodies, org's emphasis family
// (*bold* /italic/ _underline_ +strike+ =verbatim= ~code~), and
// [[links]]. Unknown #+KEYWORD lines are skipped (org metadata
// is open-ended); unknown structure is a strict error.

fn org_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("org import: {msg}")))
}

pub fn org_to_document(org: &str) -> Result<Document> {
    let _depth = descend(org_err)?;
    let lines: Vec<&str> = org.lines().collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut i = 0;

    fn flush(paragraph: &mut Vec<String>, blocks: &mut Vec<Block>) -> Result<()> {
        if paragraph.is_empty() {
            return Ok(());
        }
        let text = paragraph.join(" ");
        paragraph.clear();
        let inlines = org_inlines(&text)?;
        if !inlines.is_empty() {
            blocks.push(Block::Paragraph(inlines));
        }
        Ok(())
    }

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_end();

        if trimmed.trim().is_empty() {
            flush(&mut paragraph, &mut blocks)?;
            i += 1;
            continue;
        }

        // Headings: one to five stars.
        if let Some(rest) = star_heading(trimmed) {
            flush(&mut paragraph, &mut blocks)?;
            let (level, text) = rest;
            blocks.push(Block::Paragraph(vec![Inline::Endo {
                symbol: "#".repeat(level),
                content: org_inlines(text)?,
                bracket_matching: true,
                ann: Annotations::default(),
            }]));
            i += 1;
            continue;
        }

        // #+ keyword lines and blocks.
        if let Some(rest) = trimmed.strip_prefix("#+") {
            flush(&mut paragraph, &mut blocks)?;
            let upper = rest.to_ascii_uppercase();
            if let Some(title) = rest
                .strip_prefix("TITLE:")
                .or_else(|| rest.strip_prefix("title:"))
            {
                blocks.push(Block::Paragraph(vec![Inline::Endo {
                    symbol: "#".to_string(),
                    content: org_inlines(title.trim())?,
                    bracket_matching: true,
                    ann: Annotations::default(),
                }]));
                i += 1;
                continue;
            }
            if upper.starts_with("BEGIN_SRC") || upper.starts_with("BEGIN_EXAMPLE") {
                let lang = rest
                    .split_whitespace()
                    .nth(1)
                    .map(|l| l.to_lowercase())
                    .filter(|_| upper.starts_with("BEGIN_SRC"));
                let (content, next) = org_block_body(&lines, i + 1, &upper[6..])?;
                blocks.push(Block::VerbatimBlock {
                    content,
                    ann: Annotations {
                        onym: None,
                        genoses: lang.into_iter().collect(),
                    },
                });
                i = next;
                continue;
            }
            if upper.starts_with("BEGIN_QUOTE") {
                let (content, next) = org_block_body(&lines, i + 1, "QUOTE")?;
                let inner = org_to_document(&content)?;
                blocks.push(Block::Para {
                    symbol: ">".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children: inner.blocks,
                    hypograph: Vec::new(),
                    bracket_matching: false,
                    ann: Annotations::default(),
                });
                i = next;
                continue;
            }
            if upper.starts_with("BEGIN_VERSE") {
                let (content, next) = org_block_body(&lines, i + 1, "VERSE")?;
                let mut strophes: Vec<Strophe> = Vec::new();
                let mut current: Vec<Vec<Inline>> = Vec::new();
                for vline in content.lines() {
                    if vline.trim().is_empty() {
                        if !current.is_empty() {
                            strophes.push(Strophe(std::mem::take(&mut current)));
                        }
                    } else {
                        current.push(org_inlines(vline)?);
                    }
                }
                if !current.is_empty() {
                    strophes.push(Strophe(current));
                }
                blocks.push(Block::Stichoi {
                    symbol: Some("~".to_string()),
                    taxis: None,
                    lemma: Vec::new(),
                    strophes,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
                continue;
            }
            if upper.starts_with("BEGIN_") || upper.starts_with("END_") {
                return Err(org_err(format!("unsupported block `#+{rest}`")));
            }
            // Other keywords (#+AUTHOR:, #+OPTIONS:, ...) skip.
            i += 1;
            continue;
        }

        // A file link standing alone on its line is media (the
        // exo's enmedia shape); inside prose it stays a link.
        if let Some(path) = trimmed
            .trim_start()
            .strip_prefix("[[file:")
            .and_then(|r| r.strip_suffix("]]"))
            && !path.is_empty()
            && !path.contains(']')
        {
            flush(&mut paragraph, &mut blocks)?;
            blocks.push(Block::Enmedia {
                param: path.to_string(),
            });
            i += 1;
            continue;
        }

        // Pipe tables: a run of |-rows becomes one stichoi block.
        if trimmed.trim_start().starts_with('|') {
            flush(&mut paragraph, &mut blocks)?;
            let mut rows: Vec<Vec<Inline>> = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                rows.push(vec![Inline::Text(lines[i].trim().to_string())]);
                i += 1;
            }
            blocks.push(Block::Stichoi {
                symbol: Some("|".to_string()),
                taxis: None,
                lemma: Vec::new(),
                strophes: vec![Strophe(rows)],
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations::default(),
            });
            continue;
        }

        // Footnote definitions.
        if let Some(rest) = trimmed.strip_prefix("[fn:")
            && let Some(end) = rest.find(']')
        {
            let name = &rest[..end];
            let text = rest[end + 1..].trim();
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()) {
                flush(&mut paragraph, &mut blocks)?;
                let mut body = vec![text.to_string()];
                i += 1;
                while i < lines.len()
                    && !lines[i].trim().is_empty()
                    && lines[i].starts_with(char::is_whitespace)
                {
                    body.push(lines[i].trim().to_string());
                    i += 1;
                }
                blocks.push(Block::Para {
                    symbol: "^".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children: vec![Block::Paragraph(org_inlines(&body.join(" "))?)],
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: Some(name.to_string()),
                        genoses: Vec::new(),
                    },
                });
                continue;
            }
        }

        // List items (flat).
        let indent_trimmed = trimmed.trim_start();
        if let Some(rest) = indent_trimmed.strip_prefix("- ") {
            flush(&mut paragraph, &mut blocks)?;
            let (text, next) = org_item_body(&lines, i, rest)?;
            i = next;
            // Description items: `- term :: text`.
            if let Some(idx) = text.find(" :: ") {
                let (term, def) = (text[..idx].to_string(), text[idx + 4..].to_string());
                blocks.push(Block::Para {
                    symbol: "::".to_string(),
                    taxis: None,
                    lemma: org_inlines(&term)?,
                    children: vec![Block::Paragraph(org_inlines(&def)?)],
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
            } else {
                blocks.push(Block::Para {
                    symbol: "-".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children: vec![Block::Paragraph(org_inlines(&text)?)],
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
            }
            continue;
        }
        if let Some((number, rest)) = org_ordered_marker(indent_trimmed) {
            flush(&mut paragraph, &mut blocks)?;
            let (text, next) = org_item_body(&lines, i, rest)?;
            i = next;
            blocks.push(Block::Para {
                symbol: ".".to_string(),
                taxis: Some(Taxis::Explicit(number)),
                lemma: Vec::new(),
                children: vec![Block::Paragraph(org_inlines(&text)?)],
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations::default(),
            });
            continue;
        }

        paragraph.push(trimmed.trim().to_string());
        i += 1;
    }
    flush(&mut paragraph, &mut blocks)?;
    Ok(Document {
        dialect_id: "at-org".to_string(),
        dialect_version: None,
        blocks,
    })
}

/// `* Heading` through `***** Heading`.
fn star_heading(line: &str) -> Option<(usize, &str)> {
    let stars = line.chars().take_while(|&c| c == '*').count();
    if (1..=5).contains(&stars) && line[stars..].starts_with(' ') {
        Some((stars, line[stars + 1..].trim()))
    } else {
        None
    }
}

/// `N. text` or `N) text`.
fn org_ordered_marker(line: &str) -> Option<(u64, &str)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = &line[digits..];
    if let Some(text) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
        Some((line[..digits].parse().ok()?, text))
    } else {
        None
    }
}

/// The body of a `#+BEGIN_X` block, up to its `#+END_X`.
fn org_block_body(lines: &[&str], mut i: usize, kind: &str) -> Result<(String, usize)> {
    let word = kind.split_whitespace().next().unwrap_or(kind);
    let end = format!("END_{word}");
    // A quote's body is Org again, so quotes nest: an inner
    // BEGIN_QUOTE claims the next END_QUOTE. The other bodies
    // are verbatim or lines and end at the first END.
    let begin = (word == "QUOTE").then(|| format!("BEGIN_{word}"));
    let mut depth = 0usize;
    let mut body = Vec::new();
    while i < lines.len() {
        let upper = lines[i].trim().to_ascii_uppercase();
        if let Some(rest) = upper.strip_prefix("#+") {
            if rest.starts_with(&end) {
                if depth == 0 {
                    return Ok((body.join("\n"), i + 1));
                }
                depth -= 1;
            } else if begin.as_deref().is_some_and(|b| rest.starts_with(b)) {
                depth += 1;
            }
        }
        body.push(lines[i].to_string());
        i += 1;
    }
    Err(org_err(format!("unterminated #+BEGIN_{kind}")))
}

/// A list item's text plus indented continuation lines.
fn org_item_body(lines: &[&str], i: usize, first: &str) -> Result<(String, usize)> {
    let mut parts = vec![first.trim().to_string()];
    let mut j = i + 1;
    while j < lines.len()
        && !lines[j].trim().is_empty()
        && lines[j].starts_with("  ")
        && !lines[j].trim_start().starts_with("- ")
        && org_ordered_marker(lines[j].trim_start()).is_none()
    {
        parts.push(lines[j].trim().to_string());
        j += 1;
    }
    Ok((parts.join(" "), j))
}

/// Org inline markup over one logical line of text.
fn org_inlines(text: &str) -> Result<Vec<Inline>> {
    let _depth = descend(org_err)?;
    let chars: Vec<char> = text.chars().collect();
    let mut finder = Finder::new(&chars);
    let mut closers: ScanMemo<char> = ScanMemo::new();
    let mut inlines: Vec<Inline> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    let flush_plain = |plain: &mut String, inlines: &mut Vec<Inline>| {
        if !plain.is_empty() {
            inlines.push(Inline::Text(std::mem::take(plain)));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        // Links: [[target]] / [[target][description]].
        if c == '['
            && chars.get(i + 1) == Some(&'[')
            && let Some((target, desc, next)) = org_link(&mut finder, i + 2)
        {
            flush_plain(&mut plain, &mut inlines);
            if let Some(desc) = desc {
                inlines.push(Inline::Text(format!("{desc} (")));
                inlines.push(org_link_inline(&target));
                inlines.push(Inline::Text(")".to_string()));
            } else {
                inlines.push(org_link_inline(&target));
            }
            i = next;
            continue;
        }
        // Footnote callouts: [fn:name], the name alphanumeric.
        if c == '[' && chars[i..].starts_with(&['[', 'f', 'n', ':']) {
            let end = i
                + 4
                + chars[i + 4..]
                    .iter()
                    .take_while(|ch| ch.is_ascii_alphanumeric())
                    .count();
            if end > i + 4 && chars.get(end) == Some(&']') {
                flush_plain(&mut plain, &mut inlines);
                inlines.push(Inline::Deixis {
                    symbol: "^".to_string(),
                    onym: chars[i + 4..end].iter().collect(),
                    ann: Annotations::default(),
                });
                i = end + 1;
                continue;
            }
        }
        // Dedicated target `<<name>>`: an onym anchor (a radio
        // target's triple angles stay prose).
        if c == '<'
            && chars.get(i + 1) == Some(&'<')
            && (i == 0 || chars[i - 1] != '<')
            && let Some(close) = finder.find(i + 2, &['>', '>'])
            && close > i + 2
            && chars[i + 2..close]
                .iter()
                .all(|ch| !ch.is_whitespace() && !matches!(ch, '<' | '>'))
            && chars.get(close + 2) != Some(&'>')
        {
            flush_plain(&mut plain, &mut inlines);
            inlines.push(Inline::OnymAnchor(chars[i + 2..close].iter().collect()));
            i = close + 2;
            continue;
        }
        // Emphasis family and inline verbatim.
        if matches!(c, '*' | '/' | '_' | '+' | '=' | '~')
            && org_open_ok(&chars, i)
            && let Some(end) = closers.find(c, i + 1, || org_close(&chars, i))
        {
            flush_plain(&mut plain, &mut inlines);
            let content: String = chars[i + 1..end].iter().collect();
            match c {
                '=' | '~' => inlines.push(Inline::VerbatimInline {
                    content,
                    ann: Annotations::default(),
                }),
                _ => inlines.push(Inline::Endo {
                    symbol: c.to_string(),
                    content: org_inlines(&content)?,
                    bracket_matching: true,
                    ann: Annotations::default(),
                }),
            }
            i = end + 1;
            continue;
        }
        plain.push(c);
        i += 1;
    }
    flush_plain(&mut plain, &mut inlines);
    Ok(inlines)
}

fn org_link_inline(target: &str) -> Inline {
    Inline::Endo {
        symbol: "><".to_string(),
        content: vec![Inline::Text(target.to_string())],
        bracket_matching: true,
        ann: Annotations::default(),
    }
}

/// Parse from just past `[[` (a char index): target, optional
/// description, and the index past the closing `]]`.
fn org_link(finder: &mut Finder, start: usize) -> Option<(String, Option<String>, usize)> {
    let chars = finder.chars;
    let close = finder.find(start, &[']', ']'])?;
    let next = close + 2;
    match finder.find(start, &[']', '[']) {
        Some(sep) if sep < close => Some((
            chars[start..sep].iter().collect(),
            Some(chars[sep + 2..close].iter().collect()),
            next,
        )),
        _ => {
            if finder.find(start, &[']']).is_some_and(|p| p < close) {
                return None;
            }
            Some((chars[start..close].iter().collect(), None, next))
        }
    }
}

/// An emphasis marker opens when at start-of-text or after
/// whitespace/opening punctuation, with content following.
fn org_open_ok(chars: &[char], i: usize) -> bool {
    let before_ok = i == 0
        || chars
            .get(i - 1)
            .is_some_and(|c| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '"' | '\''));
    let after_ok = chars.get(i + 1).is_some_and(|c| !c.is_whitespace());
    before_ok && after_ok && chars.get(i + 1) != Some(&chars[i])
}

/// Find the closing marker: same char, preceded by non-space,
/// followed by whitespace/punctuation/end, on the same line.
fn org_close(chars: &[char], open: usize) -> Option<usize> {
    let marker = chars[open];
    let mut j = open + 1;
    while j < chars.len() {
        if chars[j] == marker
            && chars.get(j - 1).is_some_and(|c| !c.is_whitespace())
            && chars
                .get(j + 1)
                .is_none_or(|c| c.is_whitespace() || c.is_ascii_punctuation())
        {
            return Some(j);
        }
        j += 1;
    }
    None
}

// ---------------------------------------------------------------
// Djot (subset) -> at-djot
// ---------------------------------------------------------------
//
// The at-markdown-mirroring subset: ATX-style headings,
// >-prefixed block quotes, flat bullet and numbered items,
// fenced code with a language genos, and djot's inline family
// (_emphasis_, *strong*, `verbatim`), with backslash escapes.
// Everything else is a strict error.

fn djot_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("djot import: {msg}")))
}

pub fn djot_to_document(dj: &str) -> Result<Document> {
    let _depth = descend(djot_err)?;
    let lines: Vec<&str> = dj.lines().collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut i = 0;

    fn flush(paragraph: &mut Vec<String>, blocks: &mut Vec<Block>) -> Result<()> {
        if paragraph.is_empty() {
            return Ok(());
        }
        let text = paragraph.join(" ");
        paragraph.clear();
        let inlines = djot_inlines(&text)?;
        if !inlines.is_empty() {
            blocks.push(Block::Paragraph(inlines));
        }
        Ok(())
    }

    while i < lines.len() {
        let trimmed = lines[i].trim_end();
        if trimmed.trim().is_empty() {
            flush(&mut paragraph, &mut blocks)?;
            i += 1;
            continue;
        }
        // Headings.
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush(&mut paragraph, &mut blocks)?;
            blocks.push(Block::Paragraph(vec![Inline::Endo {
                symbol: "#".repeat(hashes),
                content: djot_inlines(trimmed[hashes + 1..].trim())?,
                bracket_matching: true,
                ann: Annotations::default(),
            }]));
            i += 1;
            continue;
        }
        // Fenced code.
        if trimmed.starts_with("```") {
            flush(&mut paragraph, &mut blocks)?;
            let ticks = trimmed.chars().take_while(|&c| c == '`').count();
            let lang = trimmed[ticks..].trim().to_lowercase();
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !closes_fence(lines[i], ticks) {
                body.push(lines[i].to_string());
                i += 1;
            }
            if i >= lines.len() {
                return Err(djot_err("unterminated code fence".into()));
            }
            i += 1;
            blocks.push(Block::VerbatimBlock {
                content: body.join("\n"),
                ann: Annotations {
                    onym: None,
                    genoses: if lang.is_empty() { vec![] } else { vec![lang] },
                },
            });
            continue;
        }
        // Block quote: a run of >-prefixed lines.
        if trimmed.starts_with('>') {
            flush(&mut paragraph, &mut blocks)?;
            let mut inner = Vec::new();
            while i < lines.len() {
                let l = lines[i].trim_end();
                // `> text`, a bare `>`, or `>text` (the space is
                // optional); the first line always matches, so
                // the run advances.
                if let Some(rest) = l.strip_prefix("> ").or_else(|| l.strip_prefix('>')) {
                    inner.push(rest.to_string());
                } else {
                    break;
                }
                i += 1;
            }
            let doc = djot_to_document(&inner.join("\n"))?;
            blocks.push(Block::Para {
                symbol: ">".to_string(),
                taxis: None,
                lemma: Vec::new(),
                children: doc.blocks,
                hypograph: Vec::new(),
                bracket_matching: false,
                ann: Annotations::default(),
            });
            continue;
        }
        // Footnote definition `[^label]: body`; indented lines
        // (and blank lines before one) continue the body.
        if let Some(after) = trimmed.strip_prefix("[^")
            && let Some((label, body)) = after.split_once("]:")
            && !label.is_empty()
            && !label.contains(|c: char| c.is_whitespace() || c == '[' || c == ']')
        {
            flush(&mut paragraph, &mut blocks)?;
            let mut inner = vec![body.trim().to_string()];
            i += 1;
            while i < lines.len() {
                let l = lines[i];
                let indented = |l: &str| l.starts_with([' ', '\t']) && !l.trim().is_empty();
                if indented(l) {
                    inner.push(l.trim().to_string());
                } else if l.trim().is_empty() && lines.get(i + 1).is_some_and(|n| indented(n)) {
                    inner.push(String::new());
                } else {
                    break;
                }
                i += 1;
            }
            let doc = djot_to_document(&inner.join("\n"))?;
            blocks.push(Block::Para {
                symbol: "^".to_string(),
                taxis: None,
                lemma: Vec::new(),
                children: doc.blocks,
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations {
                    onym: Some(label.to_string()),
                    genoses: Vec::new(),
                },
            });
            continue;
        }
        // Items.
        if let Some(rest) = trimmed.strip_prefix("- ") {
            flush(&mut paragraph, &mut blocks)?;
            blocks.push(Block::Para {
                symbol: "-".to_string(),
                taxis: None,
                lemma: Vec::new(),
                children: vec![Block::Paragraph(djot_inlines(rest.trim())?)],
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations::default(),
            });
            i += 1;
            continue;
        }
        if let Some((number, rest)) = org_ordered_marker(trimmed) {
            flush(&mut paragraph, &mut blocks)?;
            blocks.push(Block::Para {
                symbol: ".".to_string(),
                taxis: Some(Taxis::Explicit(number)),
                lemma: Vec::new(),
                children: vec![Block::Paragraph(djot_inlines(rest.trim())?)],
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations::default(),
            });
            i += 1;
            continue;
        }
        paragraph.push(trimmed.trim().to_string());
        i += 1;
    }
    flush(&mut paragraph, &mut blocks)?;
    Ok(Document {
        dialect_id: "at-djot".to_string(),
        dialect_version: None,
        blocks,
    })
}

/// Djot inline markup: `_` emphasis, `*` strong, backtick
/// verbatim, backslash escapes.
fn djot_inlines(text: &str) -> Result<Vec<Inline>> {
    let _depth = descend(djot_err)?;
    let chars: Vec<char> = text.chars().collect();
    let mut finder = Finder::new(&chars);
    let mut closers: ScanMemo<char> = ScanMemo::new();
    let mut inlines: Vec<Inline> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    let flush_plain = |plain: &mut String, inlines: &mut Vec<Inline>| {
        if !plain.is_empty() {
            inlines.push(Inline::Text(std::mem::take(plain)));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\\'
            && let Some(&next) = chars.get(i + 1)
            && next.is_ascii_punctuation()
        {
            plain.push(next);
            i += 2;
            continue;
        }
        if c == '`'
            && let Some(close) = finder.find(i + 1, &['`'])
        {
            flush_plain(&mut plain, &mut inlines);
            inlines.push(Inline::VerbatimInline {
                content: chars[i + 1..close].iter().collect(),
                ann: Annotations::default(),
            });
            i = close + 1;
            continue;
        }
        if matches!(c, '_' | '*')
            && org_open_ok(&chars, i)
            && let Some(end) = closers.find(c, i + 1, || org_close(&chars, i))
        {
            flush_plain(&mut plain, &mut inlines);
            let content: String = chars[i + 1..end].iter().collect();
            inlines.push(Inline::Endo {
                symbol: c.to_string(),
                content: djot_inlines(&content)?,
                bracket_matching: true,
                ann: Annotations::default(),
            });
            i = end + 1;
            continue;
        }
        // Autolink `<URL>` — the visible-URL link.
        if c == '<'
            && let Some((url, next)) = autolink_target(&mut finder, i)
        {
            flush_plain(&mut plain, &mut inlines);
            inlines.push(link_endo(url));
            i = next;
            continue;
        }
        // Footnote callout `[^label]`: a deixis on the footnote
        // symbol, as in at-markdown.
        if c == '['
            && chars.get(i + 1) == Some(&'^')
            && let Some(close) = finder.find(i + 2, &[']'])
            && close > i + 2
            && chars[i + 2..close]
                .iter()
                .all(|ch| !ch.is_whitespace() && *ch != '[')
        {
            flush_plain(&mut plain, &mut inlines);
            inlines.push(Inline::Deixis {
                symbol: "^".to_string(),
                onym: chars[i + 2..close].iter().collect(),
                ann: Annotations::default(),
            });
            i = close + 1;
            continue;
        }
        // Inline link `[text](url)`: the org projection, as in
        // at-markdown (the iso pair reads links identically).
        if c == '['
            && (i == 0 || chars[i - 1] != '!')
            && chars.get(i + 1) != Some(&'^')
            && let Some(close) = finder.find(i + 1, &[']'])
            && chars.get(close + 1) == Some(&'(')
            && let Some(end) = finder.find(close + 2, &[')'])
        {
            let text: String = chars[i + 1..close].iter().collect();
            let url: String = chars[close + 2..end]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            flush_plain(&mut plain, &mut inlines);
            if url.is_empty() {
                inlines.extend(djot_inlines(&text)?);
            } else if text.trim().is_empty() || text.trim() == url {
                inlines.push(link_endo(url));
            } else {
                inlines.extend(djot_inlines(&text)?);
                inlines.push(Inline::Text(" (".to_string()));
                inlines.push(link_endo(url));
                inlines.push(Inline::Text(")".to_string()));
            }
            i = end + 1;
            continue;
        }
        plain.push(c);
        i += 1;
    }
    flush_plain(&mut plain, &mut inlines);
    Ok(inlines)
}

// ---------------------------------------------------------------
// DocBook (subset) -> at-docbook
// ---------------------------------------------------------------
//
// The nested subset: article/book/chapter roots, recursively
// nested section (title as the lemma, xml:id as the onym),
// para, itemized/ordered lists, blockquote, note/warning,
// emphasis (role strong/bold as strong), literal and
// programlisting as core enlexis, and inline footnotes as
// deixis callouts plus footnote bodies emitted after the
// enclosing paragraph. Everything else is a strict error.

fn docbook_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("docbook import: {msg}")))
}

pub fn docbook_to_document(xml: &str) -> Result<Document> {
    let toks = tokenize_xml(xml)?;
    let mut i = 0;
    let mut blocks = Vec::new();
    let mut notes = 0usize;
    while i < toks.len() {
        match &toks[i] {
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "article" || name == "book" || name == "chapter" => {
                let (mut inner, next) = docbook_blocks(&toks, i + 1, name.clone(), &mut notes)?;
                // The root's leading title (bare, or from <info>)
                // is the document title, standing alone first.
                if let Some(content) = docbook_take_title(&mut inner) {
                    blocks.push(Block::Paragraph(vec![Inline::Endo {
                        symbol: "=".to_string(),
                        content,
                        bracket_matching: true,
                        ann: Annotations::default(),
                    }]));
                }
                blocks.extend(inner);
                i = next;
            }
            other => {
                return Err(docbook_err(format!(
                    "unexpected {other:?} at document level"
                )));
            }
        }
    }
    docbook_settle_titles(&mut blocks);
    Ok(Document {
        dialect_id: "at-docbook".to_string(),
        dialect_version: None,
        blocks,
    })
}

/// The sentinel symbol a `<title>` carries until its container
/// claims it (section lemma, document title).
const DOCBOOK_TITLE: &str = "\u{0}title";

/// Take a leading sentinel title paragraph off a block run.
fn docbook_take_title(blocks: &mut Vec<Block>) -> Option<Vec<Inline>> {
    match blocks.first() {
        Some(Block::Paragraph(inlines))
            if matches!(inlines.first(),
                Some(Inline::Endo { symbol, .. }) if symbol == DOCBOOK_TITLE) => {}
        _ => return None,
    }
    let Block::Paragraph(mut inlines) = blocks.remove(0) else {
        unreachable!()
    };
    let Some(Inline::Endo { content, .. }) = inlines.pop() else {
        unreachable!()
    };
    Some(content)
}

/// A `<title>` no container claimed (inside a blockquote, an
/// admonition, a list) is a caption: it settles as a plain
/// paragraph of its text rather than leaking the sentinel.
fn docbook_settle_titles(blocks: &mut [Block]) {
    for block in blocks.iter_mut() {
        match block {
            Block::Paragraph(inlines) => {
                if let [
                    Inline::Endo {
                        symbol, content, ..
                    },
                ] = inlines.as_mut_slice()
                    && symbol == DOCBOOK_TITLE
                {
                    *inlines = std::mem::take(content);
                }
            }
            Block::Para { children, .. } => docbook_settle_titles(children),
            _ => {}
        }
    }
}

fn docbook_blocks(
    toks: &[Tok],
    mut i: usize,
    until: String,
    notes: &mut usize,
) -> Result<(Vec<Block>, usize)> {
    let _depth = descend(docbook_err)?;
    let mut blocks: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if *name == until => return Ok((blocks, i + 1)),
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Text(t) => {
                return Err(docbook_err(format!(
                    "bare text at block level: `{}`",
                    t.trim()
                )));
            }
            Tok::Open { name, .. } if name == "info" => {
                // Metadata is skipped, except the title, which
                // rides the sentinel like a bare <title>.
                i += 1;
                loop {
                    match toks.get(i) {
                        None => return Err(docbook_err("unterminated <info>".into())),
                        Some(Tok::Close(name)) if name == "info" => {
                            i += 1;
                            break;
                        }
                        Some(Tok::Open {
                            name, self_closing, ..
                        }) if name == "title" => {
                            if *self_closing {
                                i += 1;
                                continue;
                            }
                            let ((content, inner), next) =
                                docbook_inline_run(toks, i + 1, "title", notes)?;
                            blocks.push(Block::Paragraph(vec![Inline::Endo {
                                symbol: DOCBOOK_TITLE.to_string(),
                                content,
                                bracket_matching: true,
                                ann: Annotations::default(),
                            }]));
                            blocks.extend(inner);
                            i = next;
                        }
                        Some(Tok::Open {
                            name, self_closing, ..
                        }) => {
                            i = if *self_closing {
                                i + 1
                            } else {
                                skip_element(toks, i + 1, name.clone())?
                            };
                        }
                        Some(_) => i += 1,
                    }
                }
            }
            Tok::Open { name, .. } if name == "title" => {
                // Sentinel-wrapped; the enclosing section (or the
                // root) promotes it to the lemma (or the title).
                let ((content, inner), next) = docbook_inline_run(toks, i + 1, "title", notes)?;
                blocks.push(Block::Paragraph(vec![Inline::Endo {
                    symbol: DOCBOOK_TITLE.to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                }]));
                blocks.extend(inner);
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "section" || name == "sect1" => {
                let onym = attr(attrs, "xml:id").map(str::to_string);
                let n = name.clone();
                let (mut children, next) = docbook_blocks(toks, i + 1, n, notes)?;
                // The leading title paragraph becomes the lemma.
                let Some(lemma) = docbook_take_title(&mut children) else {
                    return Err(docbook_err("section without a title".into()));
                };
                blocks.push(Block::Para {
                    symbol: "#".to_string(),
                    taxis: None,
                    lemma,
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym,
                        genoses: Vec::new(),
                    },
                });
                i = next;
            }
            Tok::Open { name, .. } if name == "para" => {
                let ((content, bodies), next) = docbook_inline_run(toks, i + 1, "para", notes)?;
                blocks.push(Block::Paragraph(content));
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. }
                if name == "blockquote" || name == "note" || name == "warning" =>
            {
                let symbol = match name.as_str() {
                    "blockquote" => ">",
                    "note" => "!",
                    _ => "!!",
                };
                let n = name.clone();
                let (children, next) = docbook_blocks(toks, i + 1, n, notes)?;
                blocks.push(Block::Para {
                    symbol: symbol.to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: symbol != ">",
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open { name, .. } if name == "itemizedlist" || name == "orderedlist" => {
                let symbol = if name == "itemizedlist" { "--" } else { ".." };
                let n = name.clone();
                let (children, next) = docbook_blocks(toks, i + 1, n, notes)?;
                blocks.push(Block::Para {
                    symbol: symbol.to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "footnote" => {
                let onym = attr(attrs, "xml:id").map(str::to_string);
                let (children, next) = docbook_blocks(toks, i + 1, "footnote".into(), notes)?;
                blocks.push(Block::Para {
                    symbol: "^".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym,
                        genoses: Vec::new(),
                    },
                });
                i = next;
            }
            Tok::Open { name, .. } if name == "listitem" => {
                let (children, next) = docbook_blocks(toks, i + 1, "listitem".into(), notes)?;
                blocks.push(Block::Para {
                    symbol: "-".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "programlisting" => {
                let lang = attr(attrs, "language").map(|l| l.to_lowercase());
                let (content, next) = docbook_raw_text(toks, i + 1, "programlisting")?;
                blocks.push(Block::VerbatimBlock {
                    content,
                    ann: Annotations {
                        onym: None,
                        genoses: lang.into_iter().collect(),
                    },
                });
                i = next;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "mediaobject" => {
                // The exo's media shape: mediaobject > imageobject
                // > imagedata, whose fileref is the enmedia param.
                let mut fileref: Option<String> = None;
                let mut open = !*self_closing;
                i += 1;
                while open {
                    match toks.get(i) {
                        None => return Err(docbook_err("unterminated <mediaobject>".into())),
                        Some(Tok::Close(n)) if n == "mediaobject" => open = false,
                        Some(Tok::Open { name: n, attrs, .. }) if n == "imagedata" => {
                            if fileref.is_none() {
                                fileref = attr(attrs, "fileref").map(str::to_string);
                            }
                        }
                        Some(_) => {}
                    }
                    i += 1;
                }
                let Some(param) = fileref else {
                    return Err(docbook_err(
                        "mediaobject without an imagedata fileref".into(),
                    ));
                };
                blocks.push(Block::Enmedia { param });
            }
            Tok::Open { name, .. } if name == "anchor" => {
                // A bare anchor between blocks: a paragraph of its
                // own holding the onym anchor.
                let (anchor, next) = docbook_anchor(toks, i)?;
                blocks.push(Block::Paragraph(vec![anchor]));
                i = next;
            }
            other => {
                return Err(docbook_err(format!("unsupported {other:?} at block level")));
            }
        }
    }
    Err(docbook_err(format!("unterminated <{until}>")))
}

/// `<anchor xml:id="..."/>` at `toks[i]`: the onym anchor (the
/// exo's shape) and the index past it.
fn docbook_anchor(toks: &[Tok], i: usize) -> Result<(Inline, usize)> {
    let Some(Tok::Open {
        attrs,
        self_closing,
        ..
    }) = toks.get(i)
    else {
        unreachable!("docbook_anchor is called at an anchor open tag")
    };
    let Some(id) = attr(attrs, "xml:id").or_else(|| attr(attrs, "id")) else {
        return Err(docbook_err("anchor without xml:id".into()));
    };
    let mut next = i + 1;
    if !*self_closing && matches!(toks.get(next), Some(Tok::Close(n)) if n == "anchor") {
        next += 1;
    }
    Ok((Inline::OnymAnchor(id.to_string()), next))
}

/// Raw text content (programlisting).
fn docbook_raw_text(toks: &[Tok], mut i: usize, until: &str) -> Result<(String, usize)> {
    let mut out = String::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => {
                return Ok((out.trim_matches('\n').to_string(), i + 1));
            }
            Tok::Text(t) => {
                out.push_str(&decode_entities(t));
                i += 1;
            }
            other => return Err(docbook_err(format!("unsupported {other:?} in <{until}>"))),
        }
    }
    Err(docbook_err(format!("unterminated <{until}>")))
}

/// Inline run until the closing tag; footnote bodies collect.
#[allow(clippy::type_complexity)]
fn docbook_inline_run(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    notes: &mut usize,
) -> Result<((Vec<Inline>, Vec<Block>), usize)> {
    let _depth = descend(docbook_err)?;
    let mut inlines: Vec<Inline> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => {
                trim_run(&mut inlines);
                return Ok(((inlines, bodies), i + 1));
            }
            Tok::Text(t) => {
                let decoded = decode_entities(t);
                let mut collapsed = String::with_capacity(decoded.len());
                let mut in_ws = false;
                for c in decoded.chars() {
                    if c.is_whitespace() {
                        if !in_ws {
                            collapsed.push(' ');
                        }
                        in_ws = true;
                    } else {
                        collapsed.push(c);
                        in_ws = false;
                    }
                }
                if !collapsed.is_empty() {
                    inlines.push(Inline::Text(collapsed));
                }
                i += 1;
            }
            Tok::Open { name, attrs, .. } if name == "emphasis" => {
                let strong = matches!(attr(attrs, "role"), Some("strong") | Some("bold"));
                let ((content, inner), next) = docbook_inline_run(toks, i + 1, "emphasis", notes)?;
                inlines.push(Inline::Endo {
                    symbol: if strong { "*" } else { "/" }.to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "literal" || name == "code" => {
                let n = name.clone();
                let (content, next) = docbook_raw_text(toks, i + 1, &n)?;
                inlines.push(Inline::VerbatimInline {
                    content,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "footnoteref" => {
                let Some(linkend) = attr(attrs, "linkend") else {
                    return Err(docbook_err("footnoteref without linkend".into()));
                };
                inlines.push(Inline::Deixis {
                    symbol: "^".to_string(),
                    onym: linkend.to_string(),
                    ann: Annotations::default(),
                });
                i += 1;
            }
            Tok::Open { name, .. } if name == "footnote" => {
                *notes += 1;
                let onym = format!("n{notes}");
                // A footnote holds block content (paras).
                let (children, next) = docbook_blocks(toks, i + 1, "footnote".into(), notes)?;
                inlines.push(Inline::Deixis {
                    symbol: "^".to_string(),
                    onym: onym.clone(),
                    ann: Annotations::default(),
                });
                bodies.push(Block::Para {
                    symbol: "^".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: Some(onym),
                        genoses: Vec::new(),
                    },
                });
                i = next;
            }
            Tok::Open { name, .. } if name == "anchor" => {
                let (anchor, next) = docbook_anchor(toks, i)?;
                inlines.push(anchor);
                i = next;
            }
            Tok::Open { name, .. } if name == "quote" => {
                let ((content, inner), next) = docbook_inline_run(toks, i + 1, "quote", notes)?;
                inlines.push(Inline::Endo {
                    symbol: "\"\"".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
                ..
            } if name == "link" || name == "ulink" => {
                // The visible-URL link: a DocBook 5 xlink:href (or
                // DocBook 4 ulink url) whose text is its target, or
                // none, is the link sim exactly; a hidden href
                // projects as prose text with the visible URL beside
                // it — the html projection, pending F9's faithful
                // form. An internal linkend has no URL: prose only.
                let url = attr(attrs, "xlink:href")
                    .or_else(|| attr(attrs, "url"))
                    .map(|u| u.trim().to_string())
                    .unwrap_or_default();
                let (content, next) = if *self_closing {
                    (Vec::new(), i + 1)
                } else {
                    let n = name.clone();
                    let ((content, inner), next) = docbook_inline_run(toks, i + 1, &n, notes)?;
                    bodies.extend(inner);
                    (content, next)
                };
                if url.is_empty() {
                    inlines.extend(content);
                } else {
                    let visible = match content.as_slice() {
                        [] => true,
                        [Inline::Text(t)] => t.trim() == url,
                        _ => false,
                    };
                    if visible {
                        inlines.push(link_endo(url));
                    } else {
                        inlines.extend(content);
                        inlines.push(Inline::Text(" (".to_string()));
                        inlines.push(link_endo(url));
                        inlines.push(Inline::Text(")".to_string()));
                    }
                }
                i = next;
            }
            other => return Err(docbook_err(format!("unsupported {other:?} inline"))),
        }
    }
    Err(docbook_err(format!("unterminated <{until}>")))
}

// ---------------------------------------------------------------
// BibTeX -> bibliogramma
// ---------------------------------------------------------------
//
// The standard entry syntax: `@type{key, field = value, ...}`
// with braced values (nesting preserved verbatim - BibTeX case
// protection is content), quoted values, and bare numbers.
// @comment blocks are skipped; @string and @preamble are strict
// errors (macro expansion is out of the canonical subset).

fn bib_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("bibtex import: {msg}")))
}

pub fn bibtex_to_document(bib: &str) -> Result<Document> {
    let chars: Vec<char> = bib.chars().collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        if chars[i] != '@' {
            return Err(bib_err(format!(
                "expected `@` at an entry, found `{}`",
                chars[i]
            )));
        }
        i += 1;
        let start = i;
        while i < chars.len() && chars[i].is_ascii_alphabetic() {
            i += 1;
        }
        let kind: String = chars[start..i].iter().collect::<String>().to_lowercase();
        if kind == "comment" {
            i = bib_skip_group(&chars, i)?;
            continue;
        }
        if kind == "string" || kind == "preamble" {
            return Err(bib_err(format!("@{kind} is not supported")));
        }
        if kind.is_empty() {
            return Err(bib_err("entry without a type".into()));
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if chars.get(i) != Some(&'{') {
            return Err(bib_err(format!("expected `{{` after @{kind}")));
        }
        i += 1;
        // Citation key up to the first comma.
        let key_start = i;
        while i < chars.len() && chars[i] != ',' && chars[i] != '}' {
            i += 1;
        }
        let key: String = chars[key_start..i]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        if key.is_empty() {
            return Err(bib_err(format!("@{kind} entry without a citation key")));
        }
        let mut fields: Vec<Block> = Vec::new();
        while i < chars.len() && chars[i] != '}' {
            if chars[i] == ',' || chars[i].is_whitespace() {
                i += 1;
                continue;
            }
            // Field name.
            let name_start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
            {
                i += 1;
            }
            let name: String = chars[name_start..i]
                .iter()
                .collect::<String>()
                .to_lowercase();
            if name.is_empty() {
                return Err(bib_err(format!("malformed field in @{kind}{{{key}}}")));
            }
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if chars.get(i) != Some(&'=') {
                return Err(bib_err(format!("field `{name}` without `=`")));
            }
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            let (value, next) = bib_value(&chars, i, &name)?;
            i = next;
            fields.push(Block::Para {
                symbol: ":".to_string(),
                taxis: None,
                lemma: vec![Inline::Text(name)],
                children: vec![Block::Paragraph(vec![Inline::Text(value)])],
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations::default(),
            });
        }
        if chars.get(i) != Some(&'}') {
            return Err(bib_err(format!("unterminated @{kind}{{{key}}}")));
        }
        i += 1;
        blocks.push(Block::Para {
            symbol: "&".to_string(),
            taxis: None,
            lemma: vec![Inline::Text(key)],
            children: fields,
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations {
                onym: None,
                genoses: vec![kind],
            },
        });
    }
    Ok(Document {
        dialect_id: "bibliogramma".to_string(),
        dialect_version: None,
        blocks,
    })
}

/// Skip an @comment: its braced group when one follows, else
/// (BibTeX needs no braces there) the free text up to the next
/// entry.
fn bib_skip_group(chars: &[char], mut i: usize) -> Result<usize> {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if chars.get(i) != Some(&'{') {
        while i < chars.len() && chars[i] != '@' {
            i += 1;
        }
        return Ok(i);
    }
    let mut depth = 0;
    while i < chars.len() {
        match chars[i] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(bib_err("unterminated @comment".into()))
}

/// A field value: `{...}` (nesting preserved), `"..."`, or a
/// bare number. Internal whitespace runs collapse to spaces.
fn bib_value(chars: &[char], mut i: usize, name: &str) -> Result<(String, usize)> {
    let collect = |raw: &str| -> String {
        let mut out = String::with_capacity(raw.len());
        let mut in_ws = false;
        for c in raw.trim().chars() {
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
        out
    };
    match chars.get(i) {
        Some('{') => {
            let mut depth = 0;
            let start = i + 1;
            while i < chars.len() {
                match chars[i] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            let raw: String = chars[start..i].iter().collect();
                            return Ok((collect(&raw), i + 1));
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            Err(bib_err(format!("unterminated braced value of `{name}`")))
        }
        Some('"') => {
            // Braces protect a quote inside a quoted value.
            let start = i + 1;
            let mut j = start;
            let mut depth = 0usize;
            while j < chars.len() && (chars[j] != '"' || depth > 0) {
                match chars[j] {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
                j += 1;
            }
            if j >= chars.len() {
                return Err(bib_err(format!("unterminated quoted value of `{name}`")));
            }
            let raw: String = chars[start..j].iter().collect();
            Ok((collect(&raw), j + 1))
        }
        Some(c) if c.is_ascii_digit() => {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            Ok((chars[start..i].iter().collect(), i))
        }
        _ => Err(bib_err(format!(
            "unsupported value form for `{name}` (macros/concatenation are out of subset)"
        ))),
    }
}

// ---------------------------------------------------------------
// JATS (subset) -> litogramma + bibliogramma
// ---------------------------------------------------------------
//
// Scholarly articles: article-meta (title, contrib authors,
// abstract), body sec nesting mapped onto litogramma's
// sectioning ladder, xref[bibr] citations as cite monos, inline
// fn as deixis callouts with footnote bodies, disp-quote,
// italic/bold/monospace - and the back-matter ref-list as
// bibliogramma entries embedded in an englossis block, closing
// the citation loop (export pairs .tex with .bib through the
// associated exo).

fn jats_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("jats import: {msg}")))
}

pub fn jats_to_document(xml: &str) -> Result<Document> {
    let toks = tokenize_xml(xml)?;
    let mut blocks: Vec<Block> = Vec::new();
    let mut refs: Vec<Block> = Vec::new();
    let mut notes = 0usize;
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "article" => i += 1,
            Tok::Close(name) if name == "article" => i += 1,
            Tok::Open { name, .. } if name == "front" => {
                i = jats_front(&toks, i + 1, &mut blocks, &mut notes)?;
            }
            Tok::Open { name, .. } if name == "body" => {
                let (inner, next) = jats_blocks(&toks, i + 1, "body", 0, &mut notes)?;
                blocks.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "back" => {
                i = jats_back(&toks, i + 1, &mut refs)?;
            }
            other => return Err(jats_err(format!("unexpected {other:?} at document level"))),
        }
    }
    if !refs.is_empty() {
        blocks.push(Block::MonadEnglossis {
            dialect: "bibliogramma".to_string(),
            children: refs,
            ann: Annotations::default(),
        });
    }
    Ok(Document {
        dialect_id: "litogramma".to_string(),
        dialect_version: None,
        blocks,
    })
}

/// front: article-title, contrib names, abstract. Notes share
/// the document's counter, so an abstract footnote and the
/// first body footnote get distinct onyms.
fn jats_front(
    toks: &[Tok],
    mut i: usize,
    blocks: &mut Vec<Block>,
    notes: &mut usize,
) -> Result<usize> {
    let mut depth = 1;
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "front" => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i + 1);
                }
                i += 1;
            }
            Tok::Open {
                name,
                self_closing: false,
                ..
            } if name == "front" => {
                depth += 1;
                i += 1;
            }
            Tok::Open { name, .. } if name == "article-title" => {
                let ((content, _), next) = jats_inline_run(toks, i + 1, "article-title", notes)?;
                blocks.push(solo_endo("=", content));
                i = next;
            }
            Tok::Open { name, .. } if name == "contrib" => {
                let (text, next) = jats_name(toks, i + 1, "contrib")?;
                if !text.is_empty() {
                    blocks.push(solo_endo("=:", vec![Inline::Text(text)]));
                }
                i = next;
            }
            Tok::Open { name, .. } if name == "abstract" => {
                let (children, next) = jats_blocks(toks, i + 1, "abstract", 0, notes)?;
                blocks.push(Block::Para {
                    symbol: "=\"".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            _ => i += 1,
        }
    }
    Err(jats_err("unterminated <front>".into()))
}

/// A contrib/name: "Given Surname"; string-name passes through.
fn jats_name(toks: &[Tok], mut i: usize, until: &str) -> Result<(String, usize)> {
    let mut surname = String::new();
    let mut given = String::new();
    let mut string_name = String::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => {
                let text = if !string_name.is_empty() {
                    string_name
                } else if given.is_empty() {
                    surname.clone()
                } else {
                    format!("{given} {surname}")
                };
                return Ok((text.trim().to_string(), i + 1));
            }
            Tok::Open { name, .. } if name == "surname" => {
                let (t, next) = docbook_raw_text(toks, i + 1, "surname")?;
                surname = t;
                i = next;
            }
            Tok::Open { name, .. } if name == "given-names" => {
                let (t, next) = docbook_raw_text(toks, i + 1, "given-names")?;
                given = t;
                i = next;
            }
            Tok::Open { name, .. } if name == "string-name" => {
                let (t, next) = docbook_raw_text(toks, i + 1, "string-name")?;
                string_name = t;
                i = next;
            }
            _ => i += 1,
        }
    }
    Err(jats_err(format!("unterminated <{until}>")))
}

/// The litogramma sectioning ladder for JATS sec depth.
fn jats_section_symbol(depth: usize) -> &'static str {
    ["#", "##", "###", "####"][depth.min(3)]
}

fn jats_blocks(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    depth: usize,
    notes: &mut usize,
) -> Result<(Vec<Block>, usize)> {
    let mut blocks: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => return Ok((blocks, i + 1)),
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Text(t) => {
                return Err(jats_err(format!(
                    "bare text at block level: `{}`",
                    t.trim()
                )));
            }
            Tok::Open { name, .. } if name == "sec" => {
                let (mut children, next) = jats_blocks(toks, i + 1, "sec", depth + 1, notes)?;
                let lemma = match children.first() {
                    Some(Block::Paragraph(inlines))
                        if matches!(inlines.first(),
                            Some(Inline::Endo { symbol, .. }) if symbol == "\u{0}title") =>
                    {
                        let Some(Block::Paragraph(mut inlines)) = Some(children.remove(0)) else {
                            unreachable!()
                        };
                        let Some(Inline::Endo { content, .. }) = inlines.pop() else {
                            unreachable!()
                        };
                        content
                    }
                    _ => return Err(jats_err("sec without a title".into())),
                };
                blocks.push(Block::Para {
                    symbol: jats_section_symbol(depth).to_string(),
                    taxis: None,
                    lemma,
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open { name, .. } if name == "title" => {
                let ((content, inner), next) = jats_inline_run(toks, i + 1, "title", notes)?;
                if until == "sec" && blocks.is_empty() {
                    // The sec's heading: a sentinel the sec arm
                    // lifts into its lemma; it never reaches the
                    // document.
                    blocks.push(Block::Paragraph(vec![Inline::Endo {
                        symbol: "\u{0}title".to_string(),
                        content,
                        bracket_matching: true,
                        ann: Annotations::default(),
                    }]));
                } else if until == "abstract" && blocks.is_empty() && inner.is_empty() {
                    // The abstract's own label ("Abstract"): the
                    // `="` block names itself and takes no lemma.
                } else {
                    return Err(jats_err(format!(
                        "<title> outside the head of a <sec> (in <{until}>)"
                    )));
                }
                blocks.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "p" => {
                let ((content, bodies), next) = jats_inline_run(toks, i + 1, "p", notes)?;
                blocks.push(Block::Paragraph(content));
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "disp-quote" => {
                let (children, next) = jats_blocks(toks, i + 1, "disp-quote", depth, notes)?;
                blocks.push(Block::Para {
                    symbol: "\"".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                i = next;
            }
            other => return Err(jats_err(format!("unsupported {other:?} at block level"))),
        }
    }
    Err(jats_err(format!("unterminated <{until}>")))
}

#[allow(clippy::type_complexity)]
fn jats_inline_run(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    notes: &mut usize,
) -> Result<((Vec<Inline>, Vec<Block>), usize)> {
    let mut inlines: Vec<Inline> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => {
                trim_run(&mut inlines);
                return Ok(((inlines, bodies), i + 1));
            }
            Tok::Text(t) => {
                let decoded = decode_entities(t);
                let mut collapsed = String::with_capacity(decoded.len());
                let mut in_ws = false;
                for c in decoded.chars() {
                    if c.is_whitespace() {
                        if !in_ws {
                            collapsed.push(' ');
                        }
                        in_ws = true;
                    } else {
                        collapsed.push(c);
                        in_ws = false;
                    }
                }
                if !collapsed.is_empty() {
                    inlines.push(Inline::Text(collapsed));
                }
                i += 1;
            }
            Tok::Open { name, .. } if name == "italic" || name == "bold" => {
                let symbol = if name == "italic" { "/" } else { "*" };
                let n = name.clone();
                let ((content, inner), next) = jats_inline_run(toks, i + 1, &n, notes)?;
                inlines.push(Inline::Endo {
                    symbol: symbol.to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "monospace" => {
                let (content, next) = docbook_raw_text(toks, i + 1, "monospace")?;
                inlines.push(Inline::VerbatimInline {
                    content,
                    ann: Annotations::default(),
                });
                i = next;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "xref" => {
                if attr(attrs, "ref-type") != Some("bibr") {
                    return Err(jats_err("only xref[ref-type=bibr] is supported".into()));
                }
                let Some(rid) = attr(attrs, "rid") else {
                    return Err(jats_err("xref without rid".into()));
                };
                inlines.push(Inline::Monosim {
                    symbol: ">[".to_string(),
                    param: rid.to_string(),
                    ann: Annotations::default(),
                });
                if *self_closing {
                    i += 1;
                } else {
                    // The visible callout text is dropped; the
                    // cite renders its own.
                    i = skip_element(toks, i + 1, "xref".to_string())?;
                }
            }
            Tok::Open { name, attrs, .. } if name == "ext-link" => {
                let target = attr(attrs, "xlink:href").map(str::to_string);
                let ((content, inner), next) = jats_inline_run(toks, i + 1, "ext-link", notes)?;
                let url = target.unwrap_or_else(|| {
                    content
                        .iter()
                        .filter_map(|x| match x {
                            Inline::Text(t) => Some(t.clone()),
                            _ => None,
                        })
                        .collect()
                });
                inlines.push(Inline::Endo {
                    symbol: "><".to_string(),
                    content: vec![Inline::Text(url)],
                    bracket_matching: true,
                    ann: Annotations::default(),
                });
                bodies.extend(inner);
                i = next;
            }
            Tok::Open { name, .. } if name == "fn" => {
                *notes += 1;
                let onym = format!("n{notes}");
                let (children, next) = jats_blocks(toks, i + 1, "fn", 0, notes)?;
                inlines.push(Inline::Deixis {
                    symbol: "^".to_string(),
                    onym: onym.clone(),
                    ann: Annotations::default(),
                });
                bodies.push(Block::Para {
                    symbol: "^".to_string(),
                    taxis: None,
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: true,
                    ann: Annotations {
                        onym: Some(onym),
                        genoses: Vec::new(),
                    },
                });
                i = next;
            }
            other => return Err(jats_err(format!("unsupported {other:?} inline"))),
        }
    }
    Err(jats_err(format!("unterminated <{until}>")))
}

/// back/ref-list: refs become bibliogramma entries.
fn jats_back(toks: &[Tok], mut i: usize, refs: &mut Vec<Block>) -> Result<usize> {
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "back" => return Ok(i + 1),
            Tok::Open { name, attrs, .. } if name == "ref" => {
                let Some(id) = attr(attrs, "id").map(str::to_string) else {
                    return Err(jats_err("ref without id".into()));
                };
                let (entry, next) = jats_ref(toks, i + 1, id)?;
                refs.push(entry);
                i = next;
            }
            _ => i += 1,
        }
    }
    Err(jats_err("unterminated <back>".into()))
}

/// One ref: element-citation fields map onto bibliogramma
/// fields; mixed-citation flattens into a note field.
fn jats_ref(toks: &[Tok], mut i: usize, key: String) -> Result<(Block, usize)> {
    let mut fields: Vec<Block> = Vec::new();
    let mut kind = "misc".to_string();
    let mut authors: Vec<String> = Vec::new();
    let push_field = |fields: &mut Vec<Block>, name: &str, value: String| {
        if value.is_empty() {
            return;
        }
        fields.push(Block::Para {
            symbol: ":".to_string(),
            taxis: None,
            lemma: vec![Inline::Text(name.to_string())],
            children: vec![Block::Paragraph(vec![Inline::Text(value)])],
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations::default(),
        });
    };
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "ref" => {
                if !authors.is_empty() {
                    let joined = authors.join(" and ");
                    fields.insert(
                        0,
                        Block::Para {
                            symbol: ":".to_string(),
                            taxis: None,
                            lemma: vec![Inline::Text("author".to_string())],
                            children: vec![Block::Paragraph(vec![Inline::Text(joined)])],
                            hypograph: Vec::new(),
                            bracket_matching: true,
                            ann: Annotations::default(),
                        },
                    );
                }
                return Ok((
                    Block::Para {
                        symbol: "&".to_string(),
                        taxis: None,
                        lemma: vec![Inline::Text(key)],
                        children: fields,
                        hypograph: Vec::new(),
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses: vec![kind],
                        },
                    },
                    i + 1,
                ));
            }
            Tok::Open { name, attrs, .. }
                if name == "element-citation" || name == "mixed-citation" =>
            {
                if let Some(pt) = attr(attrs, "publication-type") {
                    kind = match pt {
                        "journal" => "article".to_string(),
                        "book" => "book".to_string(),
                        other => other.to_string(),
                    };
                }
                if name == "mixed-citation" {
                    let (text, next) = jats_flat_text(toks, i + 1, "mixed-citation")?;
                    push_field(
                        &mut fields,
                        "note",
                        text.split_whitespace().collect::<Vec<_>>().join(" "),
                    );
                    i = next;
                } else {
                    i += 1;
                }
            }
            Tok::Close(name) if name == "element-citation" => i += 1,
            Tok::Open { name, .. } if name == "name" || name == "string-name" => {
                let n = name.clone();
                let (text, next) = jats_name_bib(toks, i + 1, &n)?;
                authors.push(text);
                i = next;
            }
            Tok::Open { name, .. } if name == "person-group" => i += 1,
            Tok::Close(name) if name == "person-group" => i += 1,
            Tok::Open { name, .. } if name == "article-title" => {
                let (t, next) = docbook_raw_text(toks, i + 1, "article-title")?;
                push_field(&mut fields, "title", t);
                i = next;
            }
            Tok::Open { name, .. }
                if matches!(
                    name.as_str(),
                    "source" | "year" | "volume" | "fpage" | "lpage" | "publisher-name"
                ) =>
            {
                let n = name.clone();
                let (t, next) = docbook_raw_text(toks, i + 1, &n)?;
                let field = match n.as_str() {
                    "source" => "journal",
                    "fpage" => "pages",
                    "lpage" => "lpages",
                    "publisher-name" => "publisher",
                    other => other,
                };
                push_field(&mut fields, field, t);
                i = next;
            }
            _ => i += 1,
        }
    }
    Err(jats_err("unterminated <ref>".into()))
}

/// The text of an element with its child elements flattened:
/// a mixed-citation reads as one string (names, titles and
/// punctuation in source order).
fn jats_flat_text(toks: &[Tok], mut i: usize, until: &str) -> Result<(String, usize)> {
    let mut out = String::new();
    let mut depth = 0usize;
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if depth == 0 && name == until => return Ok((out, i + 1)),
            Tok::Close(_) => depth = depth.saturating_sub(1),
            Tok::Open {
                self_closing: false,
                ..
            } => depth += 1,
            Tok::Open { .. } => {}
            Tok::Text(t) => out.push_str(&decode_entities(t)),
        }
        i += 1;
    }
    Err(jats_err(format!("unterminated <{until}>")))
}

/// A citation name: "Surname, Given".
fn jats_name_bib(toks: &[Tok], mut i: usize, until: &str) -> Result<(String, usize)> {
    let mut surname = String::new();
    let mut given = String::new();
    let mut whole = String::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => {
                let text = if !whole.is_empty() {
                    whole
                } else if given.is_empty() {
                    surname.clone()
                } else {
                    format!("{surname}, {given}")
                };
                return Ok((text.trim().to_string(), i + 1));
            }
            Tok::Open { name, .. } if name == "surname" => {
                let (t, next) = docbook_raw_text(toks, i + 1, "surname")?;
                surname = t;
                i = next;
            }
            Tok::Open { name, .. } if name == "given-names" => {
                let (t, next) = docbook_raw_text(toks, i + 1, "given-names")?;
                given = t;
                i = next;
            }
            Tok::Text(t) if until == "string-name" => {
                whole.push_str(t.trim());
                i += 1;
            }
            _ => i += 1,
        }
    }
    Err(jats_err(format!("unterminated <{until}>")))
}

// ---------------------------------------------------------------
// USFM -> at-usfm
// ---------------------------------------------------------------
//
// The practical marker core: \id opens the book (code as the
// vocabulary lemma); \c and \v are milestones in the flow (a
// chapter as a solo paragraph, verses inline); \p and its
// family start paragraphs (plain \p as core paragraphs, typed
// ones as genos blocks); \q1-\q3 poetry lines wrap in per-line
// q-level phrases inside a stichoi block, with \b as the
// strophe break; headings and titles are genos blocks;
// character markers (\wj \nd \add ...) become phrases; \f/\fe/
// \x notes become inline note endos at their anchor, their
// reference part as an .fr / .xo phrase. \ide/\usfm/
// \sts/\rem metadata is skipped; unknown markers are strict
// errors.

fn usfm_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("usfm import: {msg}")))
}

/// Heading-class markers: the whole entry is on the marker's
/// own line (titles, headings, superscriptions, speakers,
/// labels).
const USFM_HEADINGS: &[&str] = &[
    "h", "toc1", "toc2", "toc3", "mt1", "mt2", "mt3", "ms1", "ms2", "mr", "s1", "s2", "s3", "r",
    "d", "sp", "qa", "is1", "lit", "cl", "cp", "imt1", "imt2", "imt3", "io1", "io2",
];

/// Paragraph-class markers: typed paragraphs that accumulate
/// verse flow and continuation lines exactly like \p.
const USFM_PARAGRAPHS: &[&str] = &[
    "m", "mi", "pi1", "pi2", "pi3", "pc", "nb", "po", "ip", "li1", "tr", "th1", "th2", "th3",
    "tc1", "tc2", "tc3", "thr1", "thr2", "thr3", "tcr1", "tcr2", "tcr3", "li2", "li3", "li4",
    "ili1", "ili2", "lh", "lf", // embedded-text paragraph class (USFM 3.0)
    "pm", "pmo", "pmc", "pmr", "pr", "cls", "ph1", "ph2", "ipi",
];

/// Unnumbered spellings normalize to their numbered canonical.
fn usfm_canonical(marker: &str) -> &str {
    match marker {
        "mt" => "mt1",
        "ms" => "ms1",
        "s" => "s1",
        "s4" => "s3",
        "is" => "is1",
        "imt" => "imt1",
        "io" => "io1",
        "pi" => "pi1",
        "li" => "li1",
        "ili" => "ili1",
        other => other,
    }
}

pub fn usfm_to_document(usfm: &str) -> Result<Document> {
    let mut book_code: Option<String> = None;
    let mut books: Vec<Block> = Vec::new();
    let mut content: Vec<Block> = Vec::new();
    let mut paragraph: Vec<Inline> = Vec::new();
    let mut in_paragraph = false;
    let mut para_genos: Option<String> = None;
    let mut poetry: Vec<Vec<Inline>> = Vec::new();
    let mut strophes: Vec<Strophe> = Vec::new();

    fn flush_paragraph(
        paragraph: &mut Vec<Inline>,
        in_p: &mut bool,
        genos: &mut Option<String>,
        content: &mut Vec<Block>,
    ) {
        let mut inlines = std::mem::take(paragraph);
        while matches!(inlines.last(), Some(Inline::Text(t)) if t.trim().is_empty()) {
            inlines.pop();
        }
        let genos = genos.take();
        if !inlines.is_empty() {
            content.push(match genos {
                None => Block::Paragraph(inlines),
                Some(g) => Block::Paragraph(vec![Inline::Endo {
                    symbol: "_".to_string(),
                    content: inlines,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![g],
                    },
                }]),
            });
        }
        *in_p = false;
    }
    // The single q-level phrase of a poetry line, when the line
    // is still empty: \q1 alone opens the line, its verse and
    // text arrive on following lines and belong inside it.
    fn open_poetry_phrase(poetry: &mut [Vec<Inline>]) -> Option<&mut Vec<Inline>> {
        match poetry.last_mut()?.as_mut_slice() {
            [
                Inline::Endo {
                    symbol, content, ..
                },
            ] if symbol == "," && content.is_empty() => Some(content),
            _ => None,
        }
    }

    fn flush_poetry(
        poetry: &mut Vec<Vec<Inline>>,
        strophes: &mut Vec<Strophe>,
        content: &mut Vec<Block>,
    ) {
        if !poetry.is_empty() {
            strophes.push(Strophe(std::mem::take(poetry)));
        }
        if !strophes.is_empty() {
            content.push(Block::Stichoi {
                symbol: Some("~".to_string()),
                taxis: None,
                lemma: Vec::new(),
                strophes: std::mem::take(strophes),
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations::default(),
            });
        }
    }

    // USFM notes are free-flow and may wrap across source lines
    // (latVUC's patristic glosses); rejoin lines whose \f/\fe/\x
    // note is still open with the following line(s).
    fn note_delta(line: &str) -> i32 {
        let mut delta = 0;
        let mut rest = line;
        while let Some(bs) = rest.find('\\') {
            rest = &rest[bs + 1..];
            let marker: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            rest = &rest[marker.len()..];
            if matches!(marker.as_str(), "f" | "fe" | "x") {
                if rest.starts_with('*') {
                    delta -= 1;
                } else {
                    delta += 1;
                }
            }
        }
        delta
    }
    let mut joined: Vec<String> = Vec::new();
    let mut open = 0i32;
    for line in usfm.lines() {
        let d = note_delta(line);
        if open > 0 {
            let prev = joined.last_mut().unwrap();
            prev.push(' ');
            prev.push_str(line.trim());
        } else {
            joined.push(line.to_string());
        }
        open = (open + d).max(0);
    }

    for line in joined.iter() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let Some(rest) = line.strip_prefix('\\') else {
            // Continuation text of the current paragraph or
            // poetry line.
            if !poetry.is_empty() {
                let inner = usfm_inlines(line.trim())?;
                if let Some(content) = open_poetry_phrase(&mut poetry) {
                    content.extend(inner);
                } else {
                    let last = poetry.last_mut().unwrap();
                    last.push(Inline::Text(" ".to_string()));
                    last.extend(inner);
                }
            } else if in_paragraph {
                if !paragraph.is_empty() {
                    paragraph.push(Inline::Text(" ".to_string()));
                }
                paragraph.extend(usfm_inlines(line.trim())?);
            } else {
                return Err(usfm_err(format!("stray text outside a paragraph: {line}")));
            }
            continue;
        };
        let (marker, arg) = match rest.split_once(char::is_whitespace) {
            Some((m, a)) => (m, a.trim()),
            None => (rest, ""),
        };
        match marker {
            "id" => {
                let code = arg
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| usfm_err("\\id without a book code".into()))?;
                if let Some(prev) = book_code.replace(code.to_lowercase()) {
                    flush_poetry(&mut poetry, &mut strophes, &mut content);
                    flush_paragraph(
                        &mut paragraph,
                        &mut in_paragraph,
                        &mut para_genos,
                        &mut content,
                    );
                    books.push(usfm_book(prev, std::mem::take(&mut content)));
                }
            }
            "ide" | "usfm" | "sts" | "rem" | "iex" | "im" | "ie" | "ib" => {}
            "c" => {
                flush_poetry(&mut poetry, &mut strophes, &mut content);
                flush_paragraph(
                    &mut paragraph,
                    &mut in_paragraph,
                    &mut para_genos,
                    &mut content,
                );
                // The chapter number is the first token; the
                // rest of the \c line (e.g. a dangling \ca
                // alternate-number fragment) drops.
                let n = arg
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| usfm_err("\\c without a chapter number".into()))?;
                content.push(Block::Paragraph(vec![Inline::Monosim {
                    symbol: "##".to_string(),
                    param: n.to_string(),
                    ann: Annotations::default(),
                }]));
            }
            "v" => {
                let (n, text) = match arg.split_once(char::is_whitespace) {
                    Some((n, t)) => (n, t.trim()),
                    None => (arg, ""),
                };
                let verse = Inline::Monosim {
                    symbol: "|".to_string(),
                    param: n.to_string(),
                    ann: Annotations::default(),
                };
                if !poetry.is_empty() || !strophes.is_empty() {
                    let mut vline = vec![verse];
                    if !text.is_empty() {
                        vline.push(Inline::Text(" ".to_string()));
                        vline.extend(usfm_inlines(text)?);
                    }
                    if let Some(content) = open_poetry_phrase(&mut poetry) {
                        content.extend(vline);
                    } else {
                        // A verse between poetry lines starts a
                        // new line at the previous line's level.
                        let prev_level = poetry
                            .last()
                            .or_else(|| strophes.last().and_then(|s| s.0.last()))
                            .and_then(|line| match line.as_slice() {
                                [Inline::Endo { symbol, ann, .. }]
                                    if symbol == "," && ann.genoses.len() == 1 =>
                                {
                                    Some(ann.genoses[0].clone())
                                }
                                _ => None,
                            })
                            .filter(|g| g.starts_with('q'))
                            .unwrap_or_else(|| "q1".to_string());
                        poetry.push(vec![Inline::Endo {
                            symbol: ",".to_string(),
                            content: vline,
                            bracket_matching: true,
                            ann: Annotations {
                                onym: None,
                                genoses: vec![prev_level],
                            },
                        }]);
                    }
                } else {
                    if !in_paragraph {
                        // Lax USFM: \c (or a heading) directly
                        // followed by \v with no opening \p —
                        // open an implicit plain paragraph.
                        in_paragraph = true;
                        para_genos = None;
                    }
                    if !paragraph.is_empty() {
                        paragraph.push(Inline::Text(" ".to_string()));
                    }
                    paragraph.push(verse);
                    if !text.is_empty() {
                        paragraph.push(Inline::Text(" ".to_string()));
                        paragraph.extend(usfm_inlines(text)?);
                    }
                }
            }
            "p" => {
                flush_poetry(&mut poetry, &mut strophes, &mut content);
                flush_paragraph(
                    &mut paragraph,
                    &mut in_paragraph,
                    &mut para_genos,
                    &mut content,
                );
                in_paragraph = true;
                paragraph.extend(usfm_inlines(arg)?);
            }
            m if USFM_PARAGRAPHS.contains(&usfm_canonical(m)) => {
                flush_poetry(&mut poetry, &mut strophes, &mut content);
                flush_paragraph(
                    &mut paragraph,
                    &mut in_paragraph,
                    &mut para_genos,
                    &mut content,
                );
                in_paragraph = true;
                para_genos = Some(usfm_canonical(m).to_string());
                paragraph.extend(usfm_inlines(arg)?);
            }
            "b" => {
                if !poetry.is_empty() {
                    strophes.push(Strophe(std::mem::take(&mut poetry)));
                } else if strophes.is_empty() {
                    flush_paragraph(
                        &mut paragraph,
                        &mut in_paragraph,
                        &mut para_genos,
                        &mut content,
                    );
                }
            }
            q if (q.len() <= 2
                && q.starts_with('q')
                && q[1..].chars().all(|c| c.is_ascii_digit()))
                // right-aligned (\qr: Selah, doxologies),
                // centered (\qc), and embedded (\qm1..) poetry
                // lines: same q-level phrase treatment, the
                // marker itself as the genos level.
                || matches!(q, "qr" | "qc" | "qm1" | "qm2" | "qm3" | "qm") =>
            {
                flush_paragraph(
                    &mut paragraph,
                    &mut in_paragraph,
                    &mut para_genos,
                    &mut content,
                );
                let level = match q {
                    "q" => "q1",
                    "qm" => "qm1",
                    other => other,
                };
                let mut inner = usfm_inlines(arg)?;
                while matches!(inner.last(), Some(Inline::Text(t)) if t.trim().is_empty()) {
                    inner.pop();
                }
                poetry.push(vec![Inline::Endo {
                    symbol: ",".to_string(),
                    content: inner,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![level.to_string()],
                    },
                }]);
            }
            m if USFM_HEADINGS.contains(&usfm_canonical(m)) => {
                flush_poetry(&mut poetry, &mut strophes, &mut content);
                flush_paragraph(
                    &mut paragraph,
                    &mut in_paragraph,
                    &mut para_genos,
                    &mut content,
                );
                let canonical = usfm_canonical(m);
                let mut inner = usfm_inlines(arg)?;
                while matches!(inner.last(), Some(Inline::Text(t)) if t.trim().is_empty()) {
                    inner.pop();
                }
                content.push(Block::Paragraph(vec![Inline::Endo {
                    symbol: "_".to_string(),
                    content: inner,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![canonical.to_string()],
                    },
                }]));
            }
            other => return Err(usfm_err(format!("unsupported marker \\{other}"))),
        }
    }
    flush_poetry(&mut poetry, &mut strophes, &mut content);
    flush_paragraph(
        &mut paragraph,
        &mut in_paragraph,
        &mut para_genos,
        &mut content,
    );

    let Some(code) = book_code else {
        return Err(usfm_err("missing \\id".into()));
    };
    books.push(usfm_book(code, content));
    Ok(Document {
        dialect_id: "at-usfm".to_string(),
        dialect_version: None,
        blocks: books,
    })
}

fn usfm_book(code: String, content: Vec<Block>) -> Block {
    Block::Para {
        symbol: "#".to_string(),
        taxis: None,
        lemma: vec![Inline::Text(code)],
        children: content,
        hypograph: Vec::new(),
        bracket_matching: true,
        ann: Annotations::default(),
    }
}

/// Inline USFM: paired character markers, footnotes, and
/// cross-references.
fn usfm_inlines(text: &str) -> Result<Vec<Inline>> {
    let mut inlines: Vec<Inline> = Vec::new();
    let mut plain = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let Some(bs) = rest.find('\\') else {
            plain.push_str(rest);
            break;
        };
        plain.push_str(&rest[..bs]);
        rest = &rest[bs + 1..];
        let nested = rest.starts_with('+');
        if nested {
            rest = &rest[1..];
        }
        let marker: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        rest = &rest[marker.len()..];
        rest = rest.strip_prefix(' ').unwrap_or(rest);
        if marker.is_empty() || marker.ends_with('*') {
            return Err(usfm_err(format!("unexpected inline marker \\{marker}")));
        }
        if marker == "v" {
            // A verse milestone mid-line (the canonical export
            // keeps a paragraph's verses on one line).
            let n: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
            rest = rest[n.len()..].trim_start();
            let mut text = std::mem::take(&mut plain);
            text.truncate(text.trim_end().len());
            if !text.is_empty() {
                inlines.push(Inline::Text(text));
            }
            if !inlines.is_empty() {
                inlines.push(Inline::Text(" ".to_string()));
            }
            inlines.push(Inline::Monosim {
                symbol: "|".to_string(),
                param: n,
                ann: Annotations::default(),
            });
            inlines.push(Inline::Text(" ".to_string()));
            continue;
        }
        let closer_nested = format!("\\+{marker}*");
        let closer_plain = format!("\\{marker}*");
        // malformed sources open nested (\+add) but close plain
        // (\add*): accept either closer for a nested opener
        let (end, clen) = match (nested, rest.find(&closer_nested), rest.find(&closer_plain)) {
            (true, Some(e), _) => (e, closer_nested.len()),
            (_, _, Some(e)) => (e, closer_plain.len()),
            // a block-spanning character style (swef carries \qt
            // across poetry lines): close implicitly at line end;
            // notes must still close explicitly
            _ if !matches!(marker.as_str(), "f" | "fe" | "x") => (rest.len(), 0),
            _ => return Err(usfm_err(format!("unterminated \\{marker}"))),
        };
        let body = &rest[..end];
        rest = &rest[end + clen..];
        if !plain.is_empty() {
            inlines.push(Inline::Text(std::mem::take(&mut plain)));
        }
        match marker.as_str() {
            "f" | "fe" | "x" => {
                // The leading caller (+ / - / ?) drops; \fr and
                // \xo become .fr reference phrases; the text
                // markers (\ft \xt \fk \fq \fqa) flatten.
                let body = body.trim();
                // the caller token (+ - ? * or a literal letter,
                // e.g. fraLSG's `\x k`) drops; a body opening
                // directly with a marker has no caller
                let body = match (body.starts_with('\\'), body.char_indices().nth(1)) {
                    (false, Some((i, c))) if c.is_whitespace() => body[i..].trim_start(),
                    _ => body,
                };
                let note = usfm_note_inlines(body)?;
                inlines.push(Inline::Endo {
                    symbol: "^".to_string(),
                    content: note,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![marker.to_string()],
                    },
                });
            }
            "va" | "ca" | "vp" => {
                // Alternate verse/chapter numbering: the whole
                // span (marker + number) drops — the canonical
                // numbering is the milestone truth.
            }
            "w" | "wh" | "wg" | "wa" | "rb" => {
                // Wordlist/gloss wrappers mark up individual words
                // for concordance linking; the wrapper and its
                // |attributes drop, the text stays.
                let text = body.split('|').next().unwrap_or(body);
                if text.contains('\\') {
                    inlines.extend(usfm_inlines(text)?);
                } else {
                    plain.push_str(text);
                }
            }
            m => {
                // A paired character marker. The red letters (\wj)
                // carry their constant speaker as prosopon, as the
                // USX and OSIS importers do.
                let content = usfm_inlines(body.trim())?;
                let content = if m == "wj" {
                    with_aphanes(vec![aphanes(PROSOPON, "Jesus")], content)
                } else {
                    content
                };
                inlines.push(Inline::Endo {
                    symbol: ",".to_string(),
                    content,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![m.to_string()],
                    },
                });
            }
        }
    }
    if !plain.is_empty() {
        inlines.push(Inline::Text(plain));
    }
    Ok(inlines)
}

/// Note-body USFM: reference markers (\fr / \xo) become typed
/// phrases; text markers (\ft \xt \fk \fq \fqa \fl) flatten
/// into the note text. Paired character spans inside any
/// segment pass through `usfm_inlines` (which unwraps wordlist
/// markers and keeps the rest as phrases).
fn usfm_note_inlines(body: &str) -> Result<Vec<Inline>> {
    const TEXTUAL: &[&str] = &[
        "ft", "xt", "fk", "fq", "fqa", "fl", "xq", "xo", "fr", "xk", "xta",
    ];
    let mut segments: Vec<(Option<String>, String)> = Vec::new();
    let mut current: Option<String> = None;
    let mut text = String::new();
    let mut rest = body;
    while let Some(bs) = rest.find('\\') {
        let after = &rest[bs + 1..];
        let nested = after.starts_with('+');
        let mstart = if nested { &after[1..] } else { after };
        let marker: String = mstart
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if marker == "fr" || marker == "xo" || TEXTUAL.contains(&marker.as_str()) {
            text.push_str(&rest[..bs]);
            segments.push((current.take(), std::mem::take(&mut text)));
            current = Some(marker.clone());
            rest = &mstart[marker.len()..];
            rest = rest.strip_prefix(' ').unwrap_or(rest);
        } else {
            // A paired character span: copy it through its closer
            // untouched; the segment parse handles it.
            let closer_nested = format!("\\+{marker}*");
            let closer_plain = format!("\\{marker}*");
            let span_start = &rest[bs..];
            let content_start = span_start.len() - mstart[marker.len()..].len();
            let tail = &span_start[content_start..];
            let (end, clen) = match (nested, tail.find(&closer_nested), tail.find(&closer_plain)) {
                (true, Some(e), _) => (e, closer_nested.len()),
                (_, _, Some(e)) => (e, closer_plain.len()),
                _ => return Err(usfm_err(format!("unterminated \\{marker} in a note"))),
            };
            let span_end = content_start + end + clen;
            text.push_str(&rest[..bs + span_end]);
            rest = &rest[bs + span_end..];
        }
    }
    text.push_str(rest);
    segments.push((current.take(), text));

    let mut note: Vec<Inline> = Vec::new();
    for (marker, segment) in segments {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        match marker.as_deref() {
            Some("fr") | Some("xo") => note.push(Inline::Endo {
                symbol: ",".to_string(),
                content: usfm_inlines(segment)?,
                bracket_matching: true,
                ann: Annotations {
                    onym: None,
                    genoses: vec![marker.unwrap()],
                },
            }),
            _ => {
                if !note.is_empty() {
                    note.push(Inline::Text(" ".to_string()));
                }
                note.extend(usfm_inlines(segment)?);
            }
        }
    }
    Ok(note)
}

// ---------------------------------------------------------------
// USX -> at-usfm
// ---------------------------------------------------------------
//
// USX is USFM serialized as XML (one book per document):
// <para style> carries the marker, <chapter>/<verse> are
// milestones (USX 3 eid-only end milestones are skipped),
// <char style> spans are phrases with wordlist styles
// unwrapped, <note> is inline at its anchor. The same marker
// classification as the USFM importer applies.

/// Rewrite at-usfm chapter/verse markers as CORE milestones with
/// full-path values under a versification scheme (litokanon
/// milestone model): the chapter monosim `@##(3)` becomes
/// `@("scheme:jhn.3")` and the verse monosim `@|(16)` becomes
/// `@("scheme:jhn.3.16")`. Opt-in (the default at-usfm surface
/// and its round-trips are unchanged); book divisions and all
/// other markup stay as-is.
pub fn usfm_apply_scheme(doc: &mut Document, scheme: &str) {
    // the bare page/section rename applies only when the doc has
    // no milestones already carrying the target scheme (else it
    // would duplicate the resp'd anchors)
    fn has_scheme_blocks(blocks: &[Block], scheme: &str) -> bool {
        fn inl(inlines: &[Inline], scheme: &str) -> bool {
            inlines.iter().any(|i| match i {
                Inline::Milestone { scheme: ms, .. } => ms == scheme,
                Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                    inl(content, scheme)
                }
                _ => false,
            })
        }
        blocks.iter().any(|b| match b {
            Block::Paragraph(inlines) => inl(inlines, scheme),
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                inl(lemma, scheme) || has_scheme_blocks(children, scheme) || inl(hypograph, scheme)
            }
            Block::Stichoi {
                lemma,
                strophes,
                hypograph,
                ..
            } => {
                inl(lemma, scheme)
                    || strophes
                        .iter()
                        .any(|st| st.0.iter().any(|l| inl(l, scheme)))
                    || inl(hypograph, scheme)
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                has_scheme_blocks(children, scheme)
            }
            _ => false,
        })
    }
    let rename_bare = !has_scheme_blocks(&doc.blocks, scheme);
    // The coordinate's book segment is the canonical USFM code:
    // kanonizo canonicalizes the book lemma through the
    // dialektos's `books` vocabulary (OSIS `Ps` and USFM `PSA`
    // both land on `psa`), and the value must agree with it or
    // the same verse gets a different coordinate per source
    // format.
    let dial = crate::dialektos::resolve_from(&crate::source::MemorySource::new(), "at-usfm").ok();
    let books = dial.as_ref().and_then(|d| d.vocabularies.get("books"));
    fn inline_text(inlines: &[Inline]) -> String {
        let mut s = String::new();
        for i in inlines {
            if let Inline::Text(t) = i {
                s.push_str(t);
            }
        }
        s.trim().to_string()
    }
    fn walk_inlines(
        inlines: &mut [Inline],
        scheme: &str,
        book: &str,
        chapter: &mut String,
        rename_bare: bool,
    ) {
        for inl in inlines {
            match inl {
                Inline::Milestone {
                    scheme: ms, ann, ..
                } if (ms == "page" || ms == "section") && rename_bare => {
                    // a bare unit-fallback scheme (TEI with no
                    // resp'd reference system): adopt the
                    // requested scheme, the unit rides as genos
                    ann.genoses = vec![ms.clone()];
                    *ms = scheme.to_string();
                }
                // A verse before any chapter, or a chapter with
                // no number, has no well-formed coordinate: the
                // monosim stays as it is.
                Inline::Monosim { symbol, param, .. }
                    if symbol == "|" && !chapter.is_empty() && !param.is_empty() =>
                {
                    let value = format!("{book}.{chapter}.{param}");
                    *inl = Inline::Milestone {
                        scheme: scheme.to_string(),
                        value,
                        ann: Annotations::default(),
                    };
                }
                Inline::Monosim { symbol, param, .. } if symbol == "##" && !param.is_empty() => {
                    *chapter = param.clone();
                    let value = format!("{book}.{chapter}");
                    *inl = Inline::Milestone {
                        scheme: scheme.to_string(),
                        value,
                        ann: Annotations::default(),
                    };
                }
                Inline::Endo { content, .. } => {
                    walk_inlines(content, scheme, book, chapter, rename_bare);
                }
                _ => {}
            }
        }
    }
    fn walk_blocks(
        blocks: &mut [Block],
        scheme: &str,
        book: &str,
        chapter: &mut String,
        rename_bare: bool,
        books: Option<&crate::dialektos::Vocabulary>,
    ) {
        for block in blocks {
            match block {
                Block::Para {
                    symbol,
                    lemma,
                    children,
                    ..
                } if symbol == "#" => {
                    let code = inline_text(lemma);
                    let code = books
                        .map_or(code.as_str(), |books| books.canonicalize(&code))
                        .to_string();
                    let mut ch = String::new();
                    walk_blocks(children, scheme, &code, &mut ch, rename_bare, books);
                }
                Block::Para { children, .. } | Block::ParaDiaphane { children, .. } => {
                    walk_blocks(children, scheme, book, chapter, rename_bare, books);
                }
                Block::Paragraph(inlines) => {
                    walk_inlines(inlines, scheme, book, chapter, rename_bare);
                }
                Block::Stichoi { strophes, .. } => {
                    for strophe in strophes {
                        for line in &mut strophe.0 {
                            walk_inlines(line, scheme, book, chapter, rename_bare);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut ch = String::new();
    walk_blocks(&mut doc.blocks, scheme, "", &mut ch, rename_bare, books);
}

fn tanzil_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("tanzil import: {msg}")))
}

/// Tanzil txt-2 (one `sura|aya|text` line per ayah, `#` comment
/// lines) into an at-usfm-shaped document: one `#` book (code
/// "quran"); each surah opens with a standalone milestone
/// `<scheme>:N` followed by a stichoi block, one line per ayah,
/// each line opening with `<scheme>:N.M` (the recitation grain;
/// quasialign keeps stichoi lines atomic).
pub fn tanzil_to_document(src: &str, scheme: &str) -> Result<Document> {
    let mut content: Vec<Block> = Vec::new();
    let mut lines: Vec<Vec<Inline>> = Vec::new();
    let mut cur_sura: Option<u32> = None;
    fn flush(lines: &mut Vec<Vec<Inline>>, content: &mut Vec<Block>) {
        if lines.is_empty() {
            return;
        }
        content.push(Block::Stichoi {
            symbol: Some("~".to_string()),
            taxis: None,
            lemma: Vec::new(),
            strophes: vec![Strophe(std::mem::take(lines))],
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations::default(),
        });
    }
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, '|');
        let (s, a, text) = match (parts.next(), parts.next(), parts.next()) {
            (Some(s), Some(a), Some(t)) => (s, a, t.trim()),
            _ => return Err(tanzil_err(format!("malformed line: {line}"))),
        };
        let s: u32 = s
            .parse()
            .map_err(|_| tanzil_err(format!("bad sura number: {line}")))?;
        let a: u32 = a
            .parse()
            .map_err(|_| tanzil_err(format!("bad aya number: {line}")))?;
        if cur_sura != Some(s) {
            if let Some(prev) = cur_sura {
                if s != prev + 1 {
                    return Err(tanzil_err(format!("sura {s} after {prev}")));
                }
            } else if s != 1 {
                return Err(tanzil_err("text does not start at sura 1".to_string()));
            }
            flush(&mut lines, &mut content);
            cur_sura = Some(s);
            content.push(Block::Paragraph(vec![Inline::Milestone {
                scheme: scheme.to_string(),
                value: s.to_string(),
                ann: Annotations::default(),
            }]));
        }
        lines.push(vec![
            Inline::Milestone {
                scheme: scheme.to_string(),
                value: format!("{s}.{a}"),
                ann: Annotations::default(),
            },
            Inline::Text(format!(" {text}")),
        ]);
    }
    flush(&mut lines, &mut content);
    if cur_sura != Some(114) {
        return Err(tanzil_err(format!("expected 114 suras, got {cur_sura:?}")));
    }
    Ok(Document {
        dialect_id: "at-usfm".to_string(),
        dialect_version: None,
        blocks: vec![usfm_book("quran".to_string(), content)],
    })
}

fn usx_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("usx import: {msg}")))
}

pub fn usx_to_document(xml: &str) -> Result<Document> {
    let toks = tokenize_xml(xml)?;
    let mut i = 0;
    let mut book_code: Option<String> = None;
    let mut books: Vec<Block> = Vec::new();
    let mut content: Vec<Block> = Vec::new();
    let mut poetry: Vec<Vec<Inline>> = Vec::new();
    let mut strophes: Vec<Strophe> = Vec::new();

    fn flush_poetry(
        poetry: &mut Vec<Vec<Inline>>,
        strophes: &mut Vec<Strophe>,
        content: &mut Vec<Block>,
    ) {
        if !poetry.is_empty() {
            strophes.push(Strophe(std::mem::take(poetry)));
        }
        if !strophes.is_empty() {
            content.push(Block::Stichoi {
                symbol: Some("~".to_string()),
                taxis: None,
                lemma: Vec::new(),
                strophes: std::mem::take(strophes),
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations::default(),
            });
        }
    }

    while i < toks.len() {
        match &toks[i] {
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "usx" => i += 1,
            Tok::Close(name) if name == "usx" => i += 1,
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "book" => {
                let code =
                    attr(attrs, "code").ok_or_else(|| usx_err("<book> without a code".into()))?;
                if let Some(prev) = book_code.replace(code.to_lowercase()) {
                    flush_poetry(&mut poetry, &mut strophes, &mut content);
                    books.push(usfm_book(prev, std::mem::take(&mut content)));
                }
                i += 1;
                if !self_closing {
                    i = skip_element(&toks, i, "book".to_string())?;
                }
            }
            Tok::Open { name, attrs, .. } if name == "chapter" => {
                if let Some(n) = attr(attrs, "number") {
                    flush_poetry(&mut poetry, &mut strophes, &mut content);
                    content.push(Block::Paragraph(vec![Inline::Monosim {
                        symbol: "##".to_string(),
                        param: n.to_string(),
                        ann: Annotations::default(),
                    }]));
                }
                // An eid-only end milestone carries no number.
                i += 1;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "para" => {
                let style = attr(attrs, "style")
                    .ok_or_else(|| usx_err("<para> without a style".into()))?
                    .to_string();
                if *self_closing || style == "b" {
                    // The strophe break is empty either way;
                    // some serializers spell it as an open/close
                    // pair.
                    if *self_closing {
                        i += 1;
                    } else {
                        let (inner, next) = usx_inlines(&toks, i + 1, "para")?;
                        if inner
                            .iter()
                            .any(|x| !matches!(x, Inline::Text(t) if t.trim().is_empty()))
                        {
                            return Err(usx_err("<para style=\"b\"> with content".into()));
                        }
                        i = next;
                    }
                    if style == "b" && !poetry.is_empty() {
                        strophes.push(Strophe(std::mem::take(&mut poetry)));
                    }
                    continue;
                }
                let (inner, next) = usx_inlines(&toks, i + 1, "para")?;
                i = next;
                let canonical = usfm_canonical(&style).to_string();
                // The same poetry-line styles the USFM importer
                // accepts: indent levels, right-aligned, centered,
                // and embedded lines.
                let q_level = style == "q"
                    || (style.len() == 2
                        && style.starts_with('q')
                        && style[1..].chars().all(|c| c.is_ascii_digit()))
                    || matches!(style.as_str(), "qr" | "qc" | "qm" | "qm1" | "qm2" | "qm3");
                if q_level {
                    let level = match style.as_str() {
                        "q" => "q1",
                        "qm" => "qm1",
                        other => other,
                    };
                    poetry.push(vec![Inline::Endo {
                        symbol: ",".to_string(),
                        content: inner,
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses: vec![level.to_string()],
                        },
                    }]);
                    continue;
                }
                flush_poetry(&mut poetry, &mut strophes, &mut content);
                match canonical.as_str() {
                    "ide" | "rem" | "sts" | "usfm" => {}
                    "p" => content.push(Block::Paragraph(inner)),
                    m if USFM_PARAGRAPHS.contains(&m) || USFM_HEADINGS.contains(&m) => {
                        content.push(Block::Paragraph(vec![Inline::Endo {
                            symbol: "_".to_string(),
                            content: inner,
                            bracket_matching: true,
                            ann: Annotations {
                                onym: None,
                                genoses: vec![canonical.clone()],
                            },
                        }]));
                    }
                    other => {
                        return Err(usx_err(format!("unsupported para style `{other}`")));
                    }
                }
            }
            other => {
                return Err(usx_err(format!("unexpected {other:?} at book level")));
            }
        }
    }
    flush_poetry(&mut poetry, &mut strophes, &mut content);
    let Some(code) = book_code else {
        return Err(usx_err("missing <book code>".into()));
    };
    books.push(usfm_book(code, content));
    Ok(Document {
        dialect_id: "at-usfm".to_string(),
        dialect_version: None,
        blocks: books,
    })
}

/// Inline USX content up to the named closing tag.
/// Sentinels for USX quotation milestones inside one inline run;
/// `usx_fold_quotes` replaces them before the run is returned, so
/// they never reach a document.
const USX_QT_START: &str = "\u{0}qt-s";
const USX_QT_END: &str = "\u{0}qt-e";

/// Fold qt-s ... qt-e sentinels into said spans carrying the
/// speaker as prosopon. A start without an end in this run closes
/// at the run's end (the attribution does not carry into the next
/// block: a recorded limitation of the span model); an end without
/// a start drops.
fn usx_fold_quotes(inlines: Vec<Inline>) -> Vec<Inline> {
    if !inlines
        .iter()
        .any(|x| matches!(x, Inline::Monosim { symbol, .. } if symbol == USX_QT_START || symbol == USX_QT_END))
    {
        return inlines;
    }
    let mut out: Vec<Inline> = Vec::new();
    let mut open: Option<(String, Vec<Inline>)> = None;
    let close = |out: &mut Vec<Inline>, who: String, mut content: Vec<Inline>| {
        trim_inline_edges(&mut content);
        if content.is_empty() {
            return;
        }
        let marks = if who.is_empty() {
            Vec::new()
        } else {
            vec![Inline::Monosim {
                symbol: PROSOPON.to_string(),
                param: who,
                ann: Annotations::default(),
            }]
        };
        out.push(Inline::Endo {
            symbol: ",".to_string(),
            content: with_aphanes(marks, content),
            bracket_matching: true,
            ann: Annotations {
                onym: None,
                genoses: vec!["said".to_string()],
            },
        });
    };
    for inline in inlines {
        match inline {
            Inline::Monosim { symbol, param, .. } if symbol == USX_QT_START => {
                if let Some((who, content)) = open.take() {
                    close(&mut out, who, content);
                }
                open = Some((param, Vec::new()));
            }
            Inline::Monosim { symbol, .. } if symbol == USX_QT_END => {
                if let Some((who, content)) = open.take() {
                    close(&mut out, who, content);
                }
            }
            other => match &mut open {
                Some((_, content)) => content.push(other),
                None => out.push(other),
            },
        }
    }
    if let Some((who, content)) = open.take() {
        close(&mut out, who, content);
    }
    out
}

/// A USX `<ref>`: with a target, a ref span carrying it as
/// skopos; without, its text.
fn usx_push_ref(inlines: &mut Vec<Inline>, loc: Option<String>, mut inner: Vec<Inline>) {
    match loc {
        Some(target) => {
            trim_inline_edges(&mut inner);
            inlines.push(Inline::Endo {
                symbol: ",".to_string(),
                content: with_aphanes(
                    vec![Inline::Monosim {
                        symbol: SKOPOS.to_string(),
                        param: target,
                        ann: Annotations::default(),
                    }],
                    inner,
                ),
                bracket_matching: true,
                ann: Annotations {
                    onym: None,
                    genoses: vec!["ref".to_string()],
                },
            });
        }
        None => inlines.extend(inner),
    }
}

fn usx_inlines(toks: &[Tok], mut i: usize, until: &str) -> Result<(Vec<Inline>, usize)> {
    let mut inlines: Vec<Inline> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => return Ok((usx_fold_quotes(inlines), i + 1)),
            Tok::Text(t) => {
                let text = collapse_ws(&decode_entities(t));
                if after_milestone(&inlines) && !text.starts_with(' ') {
                    inlines.push(Inline::Text(" ".to_string()));
                }
                inlines.push(Inline::Text(text));
                i += 1;
            }
            Tok::Open { name, attrs, .. } if name == "verse" => {
                if let Some(n) = attr(attrs, "number") {
                    push_verse_milestone(&mut inlines, n.to_string());
                }
                i += 1;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "char" => {
                let style = attr(attrs, "style")
                    .ok_or_else(|| usx_err("<char> without a style".into()))?
                    .to_string();
                if *self_closing {
                    i += 1;
                    continue;
                }
                if after_milestone(&inlines) {
                    inlines.push(Inline::Text(" ".to_string()));
                }
                let (mut inner, next) = usx_inlines(toks, i + 1, "char")?;
                i = next;
                if !matches!(style.as_str(), "w" | "wh" | "wg" | "wa" | "rb") {
                    trim_inline_edges(&mut inner);
                }
                match style.as_str() {
                    // Wordlist/gloss wrappers unwrap to their text.
                    "w" | "wh" | "wg" | "wa" | "rb" => inlines.extend(inner),
                    // The red letters carry their constant speaker.
                    "wj" => inlines.push(Inline::Endo {
                        symbol: ",".to_string(),
                        content: with_aphanes(vec![aphanes(PROSOPON, "Jesus")], inner),
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses: vec![style],
                        },
                    }),
                    _ => inlines.push(Inline::Endo {
                        symbol: ",".to_string(),
                        content: inner,
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses: vec![style],
                        },
                    }),
                }
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "note" => {
                let style = attr(attrs, "style").unwrap_or("f").to_string();
                if *self_closing {
                    i += 1;
                    continue;
                }
                let (note, next) = usx_note_inlines(toks, i + 1)?;
                i = next;
                inlines.push(Inline::Endo {
                    symbol: "^".to_string(),
                    content: note,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![match style.as_str() {
                            "x" | "ex" => "x".to_string(),
                            "fe" => "fe".to_string(),
                            _ => "f".to_string(),
                        }],
                    },
                });
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "ref" || name == "optbreak" => {
                // <ref loc> keeps its span with the target riding as
                // skopos (OSIS spelling); a bare <ref> unwraps to its
                // text; <optbreak/> is a discretionary break.
                if name == "optbreak" {
                    inlines.push(Inline::Text(" ".to_string()));
                }
                if !*self_closing {
                    let loc = attr(attrs, "loc").map(usx_loc_to_osis);
                    let (inner, next) = usx_inlines(toks, i + 1, name)?;
                    usx_push_ref(&mut inlines, loc, inner);
                    i = next;
                } else {
                    i += 1;
                }
            }
            Tok::Open { name, attrs, .. } if name == "ms" => {
                // Milestones: a quotation start (qt-s, with its
                // speaker) and end (qt-e) become sentinels that fold
                // into a said span at the end of this run; other
                // milestones (ts, zaln-s, ...) carry nothing here.
                let style = attr(attrs, "style").unwrap_or("");
                let self_closing = matches!(
                    &toks[i],
                    Tok::Open {
                        self_closing: true,
                        ..
                    }
                );
                if style.starts_with("qt") && style.ends_with("-s") {
                    inlines.push(Inline::Monosim {
                        symbol: USX_QT_START.to_string(),
                        param: attr(attrs, "who").map(aphanes_key).unwrap_or_default(),
                        ann: Annotations::default(),
                    });
                } else if style.starts_with("qt") && style.ends_with("-e") {
                    inlines.push(Inline::Monosim {
                        symbol: USX_QT_END.to_string(),
                        param: String::new(),
                        ann: Annotations::default(),
                    });
                }
                i += 1;
                if !self_closing {
                    i = skip_element(toks, i, "ms".to_string())?;
                }
            }
            Tok::Close(name) if name == "ref" => i += 1,
            other => {
                return Err(usx_err(format!("unexpected {other:?} in inline content")));
            }
        }
    }
    Err(usx_err(format!("unterminated <{until}>")))
}

/// Note content: reference styles become typed phrases, text
/// styles flatten, anything else keeps its phrase.
fn usx_note_inlines(toks: &[Tok], mut i: usize) -> Result<(Vec<Inline>, usize)> {
    let mut note: Vec<Inline> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "note" => return Ok((note, i + 1)),
            Tok::Text(t) => {
                note.push(Inline::Text(collapse_ws(&decode_entities(t))));
                i += 1;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "char" => {
                let style = attr(attrs, "style").unwrap_or("").to_string();
                if *self_closing {
                    i += 1;
                    continue;
                }
                let (mut inner, next) = usx_inlines(toks, i + 1, "char")?;
                i = next;
                if matches!(style.as_str(), "fr" | "xo") {
                    trim_inline_edges(&mut inner);
                }
                match style.as_str() {
                    "fr" | "xo" => note.push(Inline::Endo {
                        symbol: ",".to_string(),
                        content: inner,
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses: vec![style],
                        },
                    }),
                    "ft" | "xt" | "fk" | "fq" | "fqa" | "fl" | "xq" | "w" | "wh" => {
                        note.extend(inner);
                    }
                    _ => note.push(Inline::Endo {
                        symbol: ",".to_string(),
                        content: inner,
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses: vec![style],
                        },
                    }),
                }
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "ref" => {
                if !*self_closing {
                    let loc = attr(attrs, "loc").map(usx_loc_to_osis);
                    let (inner, next) = usx_inlines(toks, i + 1, "ref")?;
                    usx_push_ref(&mut note, loc, inner);
                    i = next;
                } else {
                    i += 1;
                }
            }
            other => {
                return Err(usx_err(format!("unexpected {other:?} in a note")));
            }
        }
    }
    Err(usx_err("unterminated <note>".into()))
}

// ---------------------------------------------------------------
// OSIS -> at-usfm
// ---------------------------------------------------------------
//
// OSIS models the same scripture structure with semantic
// elements. Book divs contain chapters (milestone or container
// form), titles typed back to their marker, paragraphs with
// verse milestones, lg/l poetry, and inline semantics reversed
// from the osis exo (<q who="Jesus"> -> wj, <divineName> -> nd,
// <transChange> -> add, <hi type> -> em/bd/it/sc/no, <seg
// type> -> that genos). Generic <q> and <w> unwrap; the OSIS
// <header> is skipped. A whole-Bible file with several book
// divs becomes several book blocks.

fn osis_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("osis import: {msg}")))
}

pub fn osis_to_document(xml: &str) -> Result<Document> {
    let toks = tokenize_xml(xml)?;
    let mut i = 0;
    let mut books: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "osis" || name == "osisText" => i += 1,
            Tok::Close(name) if name == "osis" || name == "osisText" => i += 1,
            Tok::Open { name, .. } if name == "header" => {
                i = skip_element(&toks, i + 1, "header".to_string())?;
            }
            Tok::Open { name, .. } if name == "title" => {
                // A bookGroup (testament) title; book blocks are
                // self-contained, so it has no anchor.
                i = skip_element(&toks, i + 1, "title".to_string())?;
            }
            Tok::Open { name, attrs, .. } if name == "div" => {
                let div_type = attr(attrs, "type").unwrap_or("");
                if div_type == "book" {
                    let code = attr(attrs, "osisid")
                        .ok_or_else(|| osis_err("book div without an osisID".into()))?
                        .to_lowercase();
                    let (content, next) = osis_blocks(&toks, i + 1, "div")?;
                    i = next;
                    books.push(Block::Para {
                        symbol: "#".to_string(),
                        taxis: None,
                        lemma: vec![Inline::Text(code)],
                        children: content,
                        hypograph: Vec::new(),
                        bracket_matching: true,
                        ann: Annotations::default(),
                    });
                } else {
                    // Testament and front-matter groupings unwrap.
                    i += 1;
                }
            }
            Tok::Close(name) if name == "div" => i += 1,
            other => {
                return Err(osis_err(format!("unexpected {other:?} at document level")));
            }
        }
    }
    if books.is_empty() {
        return Err(osis_err("no book div found".into()));
    }
    Ok(Document {
        dialect_id: "at-usfm".to_string(),
        dialect_version: None,
        blocks: books,
    })
}

/// The verse or chapter number of an OSIS milestone: `n`, else
/// the last dotted segment of the osisID/sID.
fn osis_number(attrs: &[(String, String)]) -> Option<String> {
    if let Some(n) = attr(attrs, "n") {
        return Some(n.to_string());
    }
    let id = attr(attrs, "osisid").or_else(|| attr(attrs, "sid"))?;
    Some(id.rsplit('.').next().unwrap_or(id).to_string())
}

fn osis_title_genos(title_type: &str) -> &'static str {
    match title_type {
        "main" => "mt1",
        "psalm" => "d",
        "scope" => "mr",
        "parallel" => "r",
        "acrostic" => "qa",
        "chapterLabel" => "cl",
        "runningHead" => "h",
        _ => "s1",
    }
}

fn osis_blocks(toks: &[Tok], mut i: usize, until: &str) -> Result<(Vec<Block>, usize)> {
    let mut content: Vec<Block> = Vec::new();
    // Section divs unwrap into the flow; their closers must not
    // be taken for the book's.
    let mut divs = 0usize;
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until && divs == 0 => return Ok((content, i + 1)),
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Text(t) => {
                return Err(osis_err(format!(
                    "bare text at block level: `{}`",
                    t.trim()
                )));
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "chapter" => {
                if attr(attrs, "eid").is_none()
                    && let Some(n) = osis_number(attrs)
                {
                    content.push(Block::Paragraph(vec![Inline::Monosim {
                        symbol: "##".to_string(),
                        param: n,
                        ann: Annotations::default(),
                    }]));
                }
                // Container-form children continue in this
                // stream; the close is consumed below.
                let _ = self_closing;
                i += 1;
            }
            Tok::Close(name) if name == "chapter" => i += 1,
            Tok::Open { name, attrs, .. } if name == "title" => {
                let genos = osis_title_genos(attr(attrs, "type").unwrap_or(""));
                // A main title's short form is the running head
                // (what USFM's \h carries).
                let short = if genos == "mt1" {
                    attr(attrs, "short").map(str::to_string)
                } else {
                    None
                };
                let (mut inner, next) = osis_inlines(toks, i + 1, "title")?;
                trim_inline_edges(&mut inner);
                i = next;
                if let Some(short) = short {
                    content.push(Block::Paragraph(vec![Inline::Endo {
                        symbol: "_".to_string(),
                        content: vec![Inline::Text(short)],
                        bracket_matching: true,
                        ann: Annotations {
                            onym: None,
                            genoses: vec!["h".to_string()],
                        },
                    }]));
                }
                content.push(Block::Paragraph(vec![Inline::Endo {
                    symbol: "_".to_string(),
                    content: inner,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![genos.to_string()],
                    },
                }]));
            }
            Tok::Open { name, .. } if name == "speaker" => {
                let (mut inner, next) = osis_inlines(toks, i + 1, "speaker")?;
                trim_inline_edges(&mut inner);
                i = next;
                content.push(Block::Paragraph(vec![Inline::Endo {
                    symbol: "_".to_string(),
                    content: inner,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec!["sp".to_string()],
                    },
                }]));
            }
            Tok::Open { name, .. } if name == "p" => {
                let (inner, next) = osis_inlines(toks, i + 1, "p")?;
                i = next;
                let mut inner = inner;
                trim_inline_edges(&mut inner);
                if !inner.is_empty() {
                    content.push(Block::Paragraph(inner));
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "lg" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let (new_strophes, next) = osis_verse_lines(toks, i + 1)?;
                i = next;
                if new_strophes.is_empty() {
                    continue;
                }
                // Consecutive lg groups merge into one stichoi
                // block, each group a strophe (the USFM-canonical
                // reading of \q runs broken by \b).
                if let Some(Block::Stichoi { strophes, .. }) = content.last_mut() {
                    strophes.extend(new_strophes);
                } else {
                    content.push(Block::Stichoi {
                        symbol: Some("~".to_string()),
                        taxis: None,
                        lemma: Vec::new(),
                        strophes: new_strophes,
                        hypograph: Vec::new(),
                        bracket_matching: true,
                        ann: Annotations::default(),
                    });
                }
            }
            Tok::Open { name, .. } if name == "verse" => {
                // Verses at block level (the container form
                // `<verse osisID>text</verse>`, or milestones
                // followed by bare text): consecutive ones gather
                // into one paragraph, each opening with its
                // milestone.
                let mut inlines: Vec<Inline> = Vec::new();
                while i < toks.len() {
                    match &toks[i] {
                        Tok::Open {
                            name,
                            attrs,
                            self_closing,
                        } if name == "verse" => {
                            if attr(attrs, "eid").is_none()
                                && let Some(n) = osis_number(attrs)
                            {
                                push_verse_milestone(&mut inlines, n);
                            }
                            let container = !*self_closing;
                            i += 1;
                            if container {
                                let (inner, next) = osis_inlines(toks, i, "verse")?;
                                // The milestone and the verse text
                                // are separated by a space, as in
                                // the milestone form.
                                if after_milestone(&inlines)
                                    && matches!(inner.first(), Some(Inline::Text(t)) if !t.starts_with(' '))
                                {
                                    inlines.push(Inline::Text(" ".to_string()));
                                }
                                inlines.extend(inner);
                                i = next;
                            }
                        }
                        Tok::Text(t) if t.trim().is_empty() => i += 1,
                        Tok::Text(t) => {
                            let text = collapse_ws(&decode_entities(t));
                            if after_milestone(&inlines) && !text.starts_with(' ') {
                                inlines.push(Inline::Text(" ".to_string()));
                            }
                            inlines.push(Inline::Text(text));
                            i += 1;
                        }
                        _ => break,
                    }
                }
                trim_inline_edges(&mut inlines);
                if !inlines.is_empty() {
                    content.push(Block::Paragraph(inlines));
                }
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "div" => {
                // Section groupings unwrap into the flow.
                if !self_closing {
                    divs += 1;
                }
                i += 1;
            }
            Tok::Close(name) if name == "div" => {
                divs = divs.saturating_sub(1);
                i += 1;
            }
            Tok::Open {
                name, self_closing, ..
            } if name == "milestone" || name == "lb" || (name == "q" && *self_closing) => {
                if !self_closing {
                    i = skip_element(toks, i + 1, name.clone())?;
                } else {
                    i += 1;
                }
            }
            other => {
                return Err(osis_err(format!("unexpected {other:?} at block level")));
            }
        }
    }
    Err(osis_err(format!("unterminated <{until}>")))
}

/// An lg: l children become q-level lines; nested lg groups
/// split strophes.
fn osis_verse_lines(toks: &[Tok], mut i: usize) -> Result<(Vec<Strophe>, usize)> {
    let mut strophes: Vec<Strophe> = Vec::new();
    let mut lines: Vec<Vec<Inline>> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "lg" => {
                if !lines.is_empty() {
                    strophes.push(Strophe(lines));
                }
                return Ok((strophes, i + 1));
            }
            Tok::Text(t) if t.trim().is_empty() => i += 1,
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "l" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let level = attr(attrs, "level").unwrap_or("1");
                let (mut inner, next) = osis_inlines(toks, i + 1, "l")?;
                i = next;
                trim_inline_edges(&mut inner);
                lines.push(vec![Inline::Endo {
                    symbol: ",".to_string(),
                    content: inner,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![format!("q{level}")],
                    },
                }]);
            }
            Tok::Open { name, .. } if name == "lg" => {
                if !lines.is_empty() {
                    strophes.push(Strophe(std::mem::take(&mut lines)));
                }
                let (inner, next) = osis_verse_lines(toks, i + 1)?;
                strophes.extend(inner);
                i = next;
            }
            other => {
                return Err(osis_err(format!("unexpected {other:?} in an lg")));
            }
        }
    }
    Err(osis_err("unterminated <lg>".into()))
}

fn osis_inlines(toks: &[Tok], mut i: usize, until: &str) -> Result<(Vec<Inline>, usize)> {
    let mut inlines: Vec<Inline> = Vec::new();
    let phrase = |inlines: &mut Vec<Inline>, genos: String, mut content: Vec<Inline>| {
        trim_inline_edges(&mut content);
        inlines.push(Inline::Endo {
            symbol: ",".to_string(),
            content,
            bracket_matching: true,
            ann: Annotations {
                onym: None,
                genoses: vec![genos],
            },
        });
    };
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => return Ok((inlines, i + 1)),
            Tok::Text(t) => {
                let text = collapse_ws(&decode_entities(t));
                if after_milestone(&inlines) && !text.starts_with(' ') {
                    inlines.push(Inline::Text(" ".to_string()));
                }
                inlines.push(Inline::Text(text));
                i += 1;
            }
            Tok::Open { name, attrs, .. } if name == "verse" => {
                if attr(attrs, "eid").is_none()
                    && let Some(n) = osis_number(attrs)
                {
                    push_verse_milestone(&mut inlines, n);
                }
                i += 1;
            }
            Tok::Close(name) if name == "verse" => i += 1,
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "note" => {
                let is_x = attr(attrs, "type") == Some("crossReference");
                if *self_closing {
                    i += 1;
                    continue;
                }
                let (mut note, next) = osis_note_inlines(toks, i + 1, is_x)?;
                i = next;
                trim_inline_edges(&mut note);
                // What the note is about (note/@osisRef) rides as
                // skopos first inside the note.
                if let Some(target) = attr(attrs, "osisref") {
                    note = with_aphanes(vec![aphanes(SKOPOS, target)], note);
                }
                inlines.push(Inline::Endo {
                    symbol: "^".to_string(),
                    content: note,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![if is_x { "x" } else { "f" }.to_string()],
                    },
                });
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } => {
                let name = name.clone();
                if *self_closing {
                    if name == "lb" || name == "milestone" || name == "q" {
                        i += 1;
                        continue;
                    }
                    return Err(osis_err(format!("unexpected <{name}/> in inline content")));
                }
                let attrs = attrs.clone();
                if after_milestone(&inlines) {
                    inlines.push(Inline::Text(" ".to_string()));
                }
                let (inner, next) = osis_inlines(toks, i + 1, &name)?;
                i = next;
                match name.as_str() {
                    "q" => match attr(&attrs, "who") {
                        // The red letters: a constant speaker, who
                        // also rides as prosopon for the readers
                        // that do not know the wj convention.
                        Some("Jesus") => {
                            let mut inner = inner;
                            trim_inline_edges(&mut inner);
                            phrase(
                                &mut inlines,
                                "wj".to_string(),
                                with_aphanes(vec![aphanes(PROSOPON, "Jesus")], inner),
                            );
                        }
                        Some(who) => {
                            let mut inner = inner;
                            trim_inline_edges(&mut inner);
                            phrase(
                                &mut inlines,
                                "said".to_string(),
                                with_aphanes(vec![aphanes(PROSOPON, who)], inner),
                            );
                        }
                        // A generic quotation container.
                        None => inlines.extend(inner),
                    },
                    "divineName" => phrase(&mut inlines, "nd".to_string(), inner),
                    "transChange" => phrase(&mut inlines, "add".to_string(), inner),
                    "foreign" => phrase(&mut inlines, "tl".to_string(), inner),
                    "signed" => phrase(&mut inlines, "sig".to_string(), inner),
                    "hi" => {
                        let genos = match attr(&attrs, "type").unwrap_or("emphasis") {
                            "bold" => "bd",
                            "italic" => "it",
                            "small-caps" => "sc",
                            "normal" => "no",
                            _ => "em",
                        };
                        phrase(&mut inlines, genos.to_string(), inner);
                    }
                    "seg" => {
                        let genos = match attr(&attrs, "type").unwrap_or("") {
                            "selah" => "qs".to_string(),
                            "otPassage" => "qt".to_string(),
                            "keyword" => "k".to_string(),
                            "" => {
                                inlines.extend(inner);
                                continue;
                            }
                            other => other.to_string(),
                        };
                        phrase(&mut inlines, genos, inner);
                    }
                    "name" => {
                        if attr(&attrs, "type") == Some("x-workTitle") {
                            phrase(&mut inlines, "bk".to_string(), inner);
                        } else {
                            inlines.extend(inner);
                        }
                    }
                    // A reference with a target keeps its span, the
                    // target riding as skopos; wordlist wrappers and
                    // bare references unwrap in running text.
                    "reference" => match attr(&attrs, "osisref") {
                        Some(target) => {
                            let mut inner = inner;
                            trim_inline_edges(&mut inner);
                            phrase(
                                &mut inlines,
                                "ref".to_string(),
                                with_aphanes(vec![aphanes(SKOPOS, target)], inner),
                            );
                        }
                        None => inlines.extend(inner),
                    },
                    "w" | "a" => inlines.extend(inner),
                    "catchWord" | "rdg" => inlines.extend(inner),
                    other => {
                        return Err(osis_err(format!("unsupported inline element <{other}>")));
                    }
                }
            }
            Tok::Close(name) => {
                return Err(osis_err(format!("unmatched </{name}>")));
            }
        }
    }
    Err(osis_err(format!("unterminated <{until}>")))
}

/// XML text with insignificant line breaks: collapse every
/// whitespace run to a single space.
pub(crate) fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_ws = false;
    for c in text.chars() {
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
    out
}

/// A verse milestone must be followed by a separator on the
/// USFM surface; sources like the KJV OSIS butt the text
/// directly against the milestone.
fn after_milestone(inlines: &[Inline]) -> bool {
    matches!(inlines.last(), Some(Inline::Monosim { .. }))
}

/// The USFM surface separates a mid-line verse milestone from
/// what precedes it; sources may butt them together.
fn push_verse_milestone(inlines: &mut Vec<Inline>, n: String) {
    match inlines.last() {
        None => {}
        Some(Inline::Text(t)) if t.ends_with(char::is_whitespace) => {}
        _ => inlines.push(Inline::Text(" ".to_string())),
    }
    inlines.push(Inline::Monosim {
        symbol: "|".to_string(),
        param: n,
        ann: Annotations::default(),
    });
}

/// Trim leading/trailing whitespace-only text runs.
pub(crate) fn trim_inline_edges(inlines: &mut Vec<Inline>) {
    while matches!(inlines.first(), Some(Inline::Text(t)) if t.trim().is_empty()) {
        inlines.remove(0);
    }
    while matches!(inlines.last(), Some(Inline::Text(t)) if t.trim().is_empty()) {
        inlines.pop();
    }
    if let Some(Inline::Text(t)) = inlines.first_mut() {
        *t = t.trim_start().to_string();
    }
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        *t = t.trim_end().to_string();
    }
}

/// OSIS note content: <reference> becomes the fr/xo reference
/// phrase, catchWord/rdg/hi content flattens, nested markup
/// unwraps to text.
fn osis_note_inlines(toks: &[Tok], mut i: usize, is_x: bool) -> Result<(Vec<Inline>, usize)> {
    let mut note: Vec<Inline> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "note" => return Ok((note, i + 1)),
            Tok::Text(t) => {
                note.push(Inline::Text(collapse_ws(&decode_entities(t))));
                i += 1;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } if name == "reference" => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let target = attr(attrs, "osisref").map(|t| aphanes(SKOPOS, t));
                let (mut inner, next) = osis_inlines(toks, i + 1, "reference")?;
                i = next;
                trim_inline_edges(&mut inner);
                let inner = with_aphanes(target.into_iter().collect(), inner);
                note.push(Inline::Endo {
                    symbol: ",".to_string(),
                    content: inner,
                    bracket_matching: true,
                    ann: Annotations {
                        onym: None,
                        genoses: vec![if is_x { "xo" } else { "fr" }.to_string()],
                    },
                });
            }
            Tok::Open {
                name, self_closing, ..
            } => {
                if *self_closing {
                    i += 1;
                    continue;
                }
                let name = name.clone();
                let (inner, next) = osis_inlines(toks, i + 1, &name)?;
                note.extend(inner);
                i = next;
            }
            Tok::Close(name) => {
                return Err(osis_err(format!("unmatched </{name}> in a note")));
            }
        }
    }
    Err(osis_err("unterminated <note>".into()))
}

// -------------------------------------------------------------------
// TEI Lex-0: the dictionary path. A TEI document whose body
// carries <entry> elements imports as lexigramma - entries with
// autonym headwords, homograph taxis assigned per lemma, senses
// with positional taxis at every depth, the form block, usage
// labels, translation equivalents, citations and typed
// cross-references. Where the strict profile is exceeded the
// importer stays lenient: unknown wrappers recurse
// transparently, unknown scraps become plain text.
// -------------------------------------------------------------------

/// True when the tokenized TEI carries dictionary entries.
fn tei_is_dictionary(toks: &[Tok]) -> bool {
    toks.iter()
        .any(|t| matches!(t, Tok::Open { name, .. } if name == "entry"))
}

fn lex0_usg_genos(t: &str) -> Option<String> {
    let g = match t {
        "dom" | "domain" | "hint" => "dom",
        "reg" | "register" | "style" | "plev" => "reg",
        "geo" | "geographic" => "geo",
        "time" | "temporal" => "time",
        "lang" | "language" => "lang",
        _ => return None,
    };
    Some(g.to_string())
}

fn lex0_xr_genos(t: &str) -> Option<String> {
    let g = match t {
        "synonymy" | "syn" => "syn",
        "antonymy" | "ant" => "ant",
        "cf" | "comparison" => "cf",
        "see" | "seeAlso" | "related" => "see",
        _ => return None,
    };
    Some(g.to_string())
}

fn endo_inline(symbol: &str, content: Vec<Inline>, genoses: Vec<String>) -> Inline {
    Inline::Endo {
        symbol: symbol.to_string(),
        content,
        bracket_matching: true,
        ann: Annotations {
            onym: None,
            genoses,
        },
    }
}

fn solo_block(symbol: &str, content: Vec<Inline>, genoses: Vec<String>) -> Block {
    Block::Paragraph(vec![endo_inline(symbol, content, genoses)])
}

/// Collect the flattened text of an element (for headwords).
fn tei_text_of(toks: &[Tok], mut i: usize, until: &str) -> Result<(String, usize)> {
    let mut out = String::new();
    let mut depth = 0usize;
    while i < toks.len() {
        match &toks[i] {
            Tok::Text(t) => out.push_str(&decode_entities(t)),
            Tok::Open {
                self_closing: false,
                ..
            } => depth += 1,
            Tok::Close(name) => {
                if depth == 0 && name == until {
                    return Ok((collapse_ws(&out), i + 1));
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        i += 1;
    }
    Err(tei_err(format!("unterminated <{until}>")))
}

/// The Lex-0 document: header front matter as in the literary
/// path, then the body's entries.
fn tei_lex0_document(toks: &[Tok]) -> Result<Document> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut entry_ids: Vec<(usize, String)> = Vec::new();
    let ctx = &mut TeiCtx::default();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Text(t) if t.trim_start_matches('\u{feff}').trim().is_empty() => i += 1,
            Tok::Open { name, .. } if name == "TEI" || name == "text" || name == "body" => i += 1,
            Tok::Close(name) if name == "TEI" || name == "text" || name == "body" => i += 1,
            Tok::Open { name, .. } if name == "teiHeader" => {
                i = tei_header(toks, i + 1, &mut blocks, ctx)?;
            }
            Tok::Open { name, .. } if name == "standOff" || name == "front" || name == "back" => {
                i = skip_element(toks, i + 1, name.clone())?;
            }
            Tok::Open { name, .. } if name == "div" => i += 1,
            Tok::Close(name) if name == "div" => i += 1,
            Tok::Open { name, attrs, .. } if name == "entry" => {
                let n = attr(attrs, "n").and_then(|v| v.parse::<u64>().ok());
                let id = attr(attrs, "xml:id")
                    .or_else(|| attr(attrs, "id"))
                    .map(|s| s.to_string());
                let (entry, next) = tei_lex0_entry(toks, i + 1, "entry", n)?;
                if let Some(id) = id {
                    entry_ids.push((blocks.len(), id));
                }
                blocks.push(entry);
                i = next;
            }
            Tok::Open { name, .. } if name == "head" => {
                let ((inlines, bodies), next) = tei_inline_run(toks, i + 1, "head", ctx)?;
                blocks.push(Block::Para {
                    symbol: "#".to_string(),
                    taxis: None,
                    lemma: inlines,
                    children: Vec::new(),
                    hypograph: Vec::new(),
                    bracket_matching: false,
                    ann: Annotations::default(),
                });
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open { name, .. } if name == "p" => {
                let ((inlines, bodies), next) = tei_inline_run(toks, i + 1, "p", ctx)?;
                if !inlines.is_empty() {
                    blocks.push(Block::Paragraph(inlines));
                }
                blocks.extend(bodies);
                i = next;
            }
            Tok::Open {
                name,
                self_closing: true,
                ..
            } => {
                let _ = name;
                i += 1;
            }
            Tok::Open { name, .. } => {
                // Lenient: unknown wrappers recurse transparently.
                let _ = name;
                i += 1;
            }
            Tok::Close(_) => i += 1,
            Tok::Text(t) => {
                let s = collapse_ws(&decode_entities(t));
                if !s.trim().is_empty() {
                    blocks.push(Block::Paragraph(vec![Inline::Text(s)]));
                }
                i += 1;
            }
        }
    }
    assign_homograph_taxis(&mut blocks);
    // Cross-references address entries by their autonyms, not by
    // the source's xml:ids: rewrite ref params through the map.
    let mut idmap: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (idx, id) in &entry_ids {
        if let Some(Block::Para { lemma, taxis, .. }) = blocks.get(*idx) {
            let mut hw = String::new();
            for inl in lemma {
                if let Inline::Text(t) = inl {
                    hw.push_str(t);
                }
            }
            let mut onym = String::new();
            let mut sep = false;
            for c in hw.trim().chars() {
                if c.is_alphanumeric() {
                    if sep && !onym.is_empty() {
                        onym.push('-');
                    }
                    sep = false;
                    onym.push(c);
                } else {
                    sep = true;
                }
            }
            if let Some(Taxis::Explicit(n)) = taxis {
                onym.push('-');
                onym.push_str(&n.to_string());
            }
            if !onym.is_empty() {
                idmap.insert(id.clone(), onym);
            }
        }
    }
    if !idmap.is_empty() {
        rewrite_ref_params(&mut blocks, &idmap);
    }
    Ok(Document {
        dialect_id: "lexigramma".to_string(),
        dialect_version: None,
        blocks,
    })
}

fn rewrite_ref_params(blocks: &mut [Block], map: &std::collections::HashMap<String, String>) {
    fn walk_inlines(inlines: &mut [Inline], map: &std::collections::HashMap<String, String>) {
        for inl in inlines {
            match inl {
                Inline::Monosim { symbol, param, .. } if symbol == ">" => {
                    if let Some(target) = map.get(param.as_str()) {
                        *param = target.clone();
                    }
                }
                Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                    walk_inlines(content, map)
                }
                _ => {}
            }
        }
    }
    for b in blocks {
        match b {
            Block::Paragraph(inlines) => walk_inlines(inlines, map),
            Block::Para {
                lemma,
                children,
                hypograph,
                ..
            } => {
                walk_inlines(lemma, map);
                rewrite_ref_params(children, map);
                walk_inlines(hypograph, map);
            }
            Block::ParaDiaphane { children, .. } | Block::MonadEnglossis { children, .. } => {
                rewrite_ref_params(children, map)
            }
            _ => {}
        }
    }
}

/// Homograph taxis per lemma: entries sharing a headword get
/// 1..n in document order; unique headwords carry none (unless
/// the source gave one).
fn assign_homograph_taxis(blocks: &mut [Block]) {
    use std::collections::HashMap;
    let mut counts: HashMap<String, u64> = HashMap::new();
    for b in blocks.iter() {
        if let Block::Para { symbol, lemma, .. } = b
            && symbol == "!"
        {
            let mut t = String::new();
            for inl in lemma {
                if let Inline::Text(s) = inl {
                    t.push_str(s);
                }
            }
            *counts.entry(t).or_insert(0) += 1;
        }
    }
    let mut seen: HashMap<String, u64> = HashMap::new();
    for b in blocks.iter_mut() {
        if let Block::Para {
            symbol,
            lemma,
            taxis,
            ..
        } = b
            && symbol == "!"
            && taxis.is_none()
        {
            let mut t = String::new();
            for inl in lemma.iter() {
                if let Inline::Text(s) = inl {
                    t.push_str(s);
                }
            }
            if counts.get(&t).copied().unwrap_or(0) > 1 {
                let n = seen.entry(t).or_insert(0);
                *n += 1;
                *taxis = Some(Taxis::Explicit(*n));
            }
        }
    }
}

/// One <entry>: the form block, grammar, etymology, senses and
/// related entries, in source order.
fn tei_lex0_entry(
    toks: &[Tok],
    mut i: usize,
    until: &str,
    taxis_n: Option<u64>,
) -> Result<(Block, usize)> {
    let mut headword = String::new();
    let mut children: Vec<Block> = Vec::new();
    let mut sense_no = 0u64;
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => {
                let block = Block::Para {
                    symbol: "!".to_string(),
                    taxis: taxis_n.map(Taxis::Explicit),
                    lemma: vec![Inline::Text(headword.clone())],
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: false,
                    ann: Annotations::default(),
                };
                return Ok((block, i + 1));
            }
            Tok::Open { name, attrs, .. } if name == "form" => {
                let ftype = attr(attrs, "type").unwrap_or("lemma").to_string();
                i = tei_lex0_form(toks, i + 1, &ftype, &mut headword, &mut children)?;
            }
            Tok::Open { name, .. } if name == "gramGrp" => {
                let (text, next) = tei_text_of(toks, i + 1, "gramGrp")?;
                if !text.is_empty() {
                    children.push(solo_block("=&", vec![Inline::Text(text)], vec![]));
                }
                i = next;
            }
            Tok::Open { name, .. } if name == "etym" => {
                let ((inlines, _), next) = tei_lex0_inlines(toks, i + 1, "etym")?;
                if !inlines.is_empty() {
                    children.push(solo_block("=<", inlines, vec![]));
                }
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "sense" => {
                sense_no += 1;
                let _ = attrs;
                let (sense, next) = tei_lex0_sense(toks, i + 1, sense_no)?;
                children.push(sense);
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "re" => {
                let n = attr(attrs, "n").and_then(|v| v.parse::<u64>().ok());
                let (entry, next) = tei_lex0_entry(toks, i + 1, "re", n)?;
                children.push(entry);
                i = next;
            }
            Tok::Open { name, .. } if name == "usg" || name == "xr" || name == "note" => {
                let ((inlines, _), next) = tei_lex0_inline_element(toks, i)?;
                if !inlines.is_empty() {
                    children.push(Block::Paragraph(inlines));
                }
                i = next;
            }
            Tok::Open {
                self_closing: true, ..
            } => i += 1,
            Tok::Open { .. } => i += 1,
            Tok::Close(_) => i += 1,
            Tok::Text(_) => i += 1,
        }
    }
    Err(tei_err(format!("unterminated <{until}>")))
}

/// A <form>: the lemma orth feeds the headword; variant orths,
/// pronunciations and hyphenations become their solos.
fn tei_lex0_form(
    toks: &[Tok],
    mut i: usize,
    ftype: &str,
    headword: &mut String,
    children: &mut Vec<Block>,
) -> Result<usize> {
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "form" => return Ok(i + 1),
            Tok::Open { name, .. } if name == "orth" => {
                let (text, next) = tei_text_of(toks, i + 1, "orth")?;
                if ftype == "lemma" && headword.is_empty() {
                    *headword = text;
                } else if !text.is_empty() {
                    // Inflected forms are lookup keys (=*), not
                    // display variants (=~).
                    let sim = if ftype == "inflected" { "=*" } else { "=~" };
                    children.push(solo_block(sim, vec![Inline::Text(text)], vec![]));
                }
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "pron" => {
                let genos = match attr(attrs, "notation") {
                    Some("respell") => "respell",
                    _ => "ipa",
                }
                .to_string();
                let (text, next) = tei_text_of(toks, i + 1, "pron")?;
                if !text.is_empty() {
                    children.push(solo_block("=%", vec![Inline::Text(text)], vec![genos]));
                }
                i = next;
            }
            Tok::Open { name, .. } if name == "hyph" || name == "syll" => {
                let close = name.clone();
                let (text, next) = tei_text_of(toks, i + 1, &close)?;
                if !text.is_empty() {
                    children.push(solo_block("=-", vec![Inline::Text(text)], vec![]));
                }
                i = next;
            }
            Tok::Open { name, attrs, .. } if name == "form" => {
                // nested form: recurse under its own type so
                // <form type="inflected"> orths land as =* keys
                let nested = attr(attrs, "type").unwrap_or(ftype).to_string();
                i = tei_lex0_form(toks, i + 1, &nested, headword, children)?;
            }
            _ => i += 1,
        }
    }
    Err(tei_err("unterminated <form>".into()))
}

/// A <sense>: definitions, examples, equivalents, labels and
/// cross-references flow as one paragraph; nested senses follow.
fn tei_lex0_sense(toks: &[Tok], mut i: usize, taxis: u64) -> Result<(Block, usize)> {
    let mut flow: Vec<Inline> = Vec::new();
    let mut children: Vec<Block> = Vec::new();
    let mut sub_no = 0u64;
    let flush = |flow: &mut Vec<Inline>, children: &mut Vec<Block>| {
        trim_run(flow);
        if !flow.is_empty() {
            children.push(Block::Paragraph(std::mem::take(flow)));
        }
    };
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "sense" => {
                flush(&mut flow, &mut children);
                let block = Block::Para {
                    symbol: ":".to_string(),
                    taxis: Some(Taxis::Explicit(taxis)),
                    lemma: Vec::new(),
                    children,
                    hypograph: Vec::new(),
                    bracket_matching: false,
                    ann: Annotations::default(),
                };
                return Ok((block, i + 1));
            }
            Tok::Open { name, .. } if name == "sense" => {
                flush(&mut flow, &mut children);
                sub_no += 1;
                let (sense, next) = tei_lex0_sense(toks, i + 1, sub_no)?;
                children.push(sense);
                i = next;
            }
            Tok::Open { name, .. } if name == "def" => {
                let ((inlines, _), next) = tei_lex0_inlines(toks, i + 1, "def")?;
                if !flow.is_empty() {
                    flow.push(Inline::Text(" ".into()));
                }
                flow.extend(inlines);
                i = next;
            }
            Tok::Open { .. } | Tok::Text(_) => {
                let ((inlines, _), next) = tei_lex0_flow_item(toks, i)?;
                if !inlines.is_empty() {
                    if !flow.is_empty() {
                        flow.push(Inline::Text(" ".into()));
                    }
                    flow.extend(inlines);
                }
                i = next;
            }
            Tok::Close(_) => i += 1,
        }
    }
    Err(tei_err("unterminated <sense>".into()))
}

/// One flow item inside a sense: cit, usg, xr, gram, or text.
#[allow(clippy::type_complexity)]
fn tei_lex0_flow_item(toks: &[Tok], i: usize) -> Result<((Vec<Inline>, Vec<Block>), usize)> {
    match &toks[i] {
        Tok::Text(t) => {
            let s = collapse_ws(&decode_entities(t));
            if s.trim().is_empty() {
                Ok(((Vec::new(), Vec::new()), i + 1))
            } else {
                Ok(((vec![Inline::Text(s)], Vec::new()), i + 1))
            }
        }
        Tok::Open { .. } => tei_lex0_inline_element(toks, i),
        Tok::Close(_) => Ok(((Vec::new(), Vec::new()), i + 1)),
    }
}

/// An inline dictionary element at position i (an opener).
#[allow(clippy::type_complexity)]
fn tei_lex0_inline_element(toks: &[Tok], i: usize) -> Result<((Vec<Inline>, Vec<Block>), usize)> {
    let Tok::Open {
        name,
        attrs,
        self_closing,
    } = &toks[i]
    else {
        return Ok(((Vec::new(), Vec::new()), i + 1));
    };
    if *self_closing {
        return Ok(((Vec::new(), Vec::new()), i + 1));
    }
    match name.as_str() {
        "cit" => {
            let ctype = attr(attrs, "type").unwrap_or("example").to_string();
            tei_lex0_cit(toks, i + 1, &ctype)
        }
        "usg" => {
            let genoses = attr(attrs, "type")
                .and_then(lex0_usg_genos)
                .map(|g| vec![g])
                .unwrap_or_default();
            let ((inlines, bodies), next) = tei_lex0_inlines(toks, i + 1, "usg")?;
            Ok(((vec![endo_inline("[", inlines, genoses)], bodies), next))
        }
        "gram" | "pos" | "gen" | "number" | "tns" | "mood" => {
            let close = name.clone();
            let (text, next) = tei_text_of(toks, i + 1, &close)?;
            Ok((
                (
                    vec![endo_inline("&", vec![Inline::Text(text)], vec![])],
                    Vec::new(),
                ),
                next,
            ))
        }
        "xr" => tei_lex0_xr(toks, i + 1, attr(attrs, "type").map(|s| s.to_string())),
        "quote" | "mentioned" => {
            let close = name.clone();
            let ((inlines, bodies), next) = tei_lex0_inlines(toks, i + 1, &close)?;
            Ok(((vec![endo_inline("~", inlines, vec![])], bodies), next))
        }
        "note" => {
            let ((inlines, bodies), next) = tei_lex0_inlines(toks, i + 1, "note")?;
            Ok(((inlines, bodies), next))
        }
        _ => {
            // Lenient: unknown inline wrappers dissolve.
            let close = name.clone();
            tei_lex0_inlines(toks, i + 1, &close)
        }
    }
}

/// A <cit>: examples become citations, translationEquivalents
/// become equivalents with their language genos; a bibl author
/// follows as an .author annotation.
#[allow(clippy::type_complexity)]
fn tei_lex0_cit(
    toks: &[Tok],
    mut i: usize,
    ctype: &str,
) -> Result<((Vec<Inline>, Vec<Block>), usize)> {
    let mut out: Vec<Inline> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "cit" => return Ok(((out, Vec::new()), i + 1)),
            Tok::Open { name, attrs, .. } if name == "quote" => {
                let lang = attr(attrs, "xml:lang")
                    .or_else(|| attr(attrs, "lang"))
                    .map(|s| s.to_string());
                let ((inlines, _), next) = tei_lex0_inlines(toks, i + 1, "quote")?;
                if ctype == "translationEquivalent" || ctype == "translation" {
                    let genoses = lang.map(|l| vec![l]).unwrap_or_default();
                    out.push(endo_inline("=>", inlines, genoses));
                } else {
                    out.push(endo_inline("~", inlines, vec![]));
                }
                i = next;
            }
            Tok::Open { name, .. } if name == "bibl" || name == "author" => {
                let close = name.clone();
                let (text, next) = tei_text_of(toks, i + 1, &close)?;
                if !text.is_empty() {
                    out.push(Inline::Text(" ".into()));
                    out.push(endo_inline(
                        ",",
                        vec![Inline::Text(text)],
                        vec!["author".into()],
                    ));
                }
                i = next;
            }
            _ => i += 1,
        }
    }
    Err(tei_err("unterminated <cit>".into()))
}

/// An <xr>: typed cross-references to other entries by their
/// target ids (the autonym onyms).
#[allow(clippy::type_complexity)]
fn tei_lex0_xr(
    toks: &[Tok],
    mut i: usize,
    xtype: Option<String>,
) -> Result<((Vec<Inline>, Vec<Block>), usize)> {
    let genoses: Vec<String> = xtype
        .as_deref()
        .and_then(lex0_xr_genos)
        .map(|g| vec![g])
        .unwrap_or_default();
    let mut out: Vec<Inline> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == "xr" => return Ok(((out, Vec::new()), i + 1)),
            Tok::Open { name, attrs, .. } if name == "ref" => {
                let target = attr(attrs, "target").map(|t| t.trim_start_matches('#').to_string());
                let close = name.clone();
                let (text, next) = tei_text_of(toks, i + 1, &close)?;
                let param = target.unwrap_or(text);
                if !param.is_empty() {
                    out.push(Inline::Monosim {
                        symbol: ">".to_string(),
                        param,
                        ann: Annotations {
                            onym: None,
                            genoses: genoses.clone(),
                        },
                    });
                }
                i = next;
            }
            Tok::Open { name, .. } if name == "lbl" => {
                let (text, next) = tei_text_of(toks, i + 1, "lbl")?;
                if !text.is_empty() {
                    out.push(Inline::Text(format!("{text} ")));
                }
                i = next;
            }
            _ => i += 1,
        }
    }
    Err(tei_err("unterminated <xr>".into()))
}

/// Inline content of a dictionary element: text and nested
/// dictionary inlines.
#[allow(clippy::type_complexity)]
fn tei_lex0_inlines(
    toks: &[Tok],
    mut i: usize,
    until: &str,
) -> Result<((Vec<Inline>, Vec<Block>), usize)> {
    let mut inlines: Vec<Inline> = Vec::new();
    let mut bodies: Vec<Block> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Close(name) if name == until => {
                trim_run(&mut inlines);
                return Ok(((inlines, bodies), i + 1));
            }
            _ => {
                let ((inl, bod), next) = tei_lex0_flow_item(toks, i)?;
                inlines.extend(inl);
                bodies.extend(bod);
                i = next;
            }
        }
    }
    Err(tei_err(format!("unterminated <{until}>")))
}
