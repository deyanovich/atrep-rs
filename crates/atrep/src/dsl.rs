//! ABBYY Lingvo DSL -> lexigramma.
//!
//! A DSL dictionary is a text file of cards: one or more headword
//! lines at the left margin, then the body, every line indented.
//! The body is marked with square-bracket tags; the headwords are
//! indexed, a `{…}` run in a headword displayed but not indexed.
//! The file is UTF-16 with a byte-order mark, or a code page the
//! `#SOURCE_CODE_PAGE` header names; `.dsl.dz` is gzip (dictzip).
//!
//! What the import keeps and where it goes:
//!
//! - `#NAME` is the title; `#INDEX_LANGUAGE` / `#CONTENTS_LANGUAGE`
//!   the `languages` line, as language codes.
//! - A card is an `entry`, its first headword the lemma (the
//!   onym), further headwords `variant` forms (.spelling). A
//!   headword with a `{…}` run keeps the whole as the displayed
//!   lemma and registers the indexed part as an `inflection` key
//!   typed .sort. `@` sub-cards nest as entries.
//! - Senses are derived: a body line that opens with a number
//!   ("1)", "2.", "а)", "b)") opens a sense; arabic numbers are the
//!   first depth, letters subdivide the arabic sense before them,
//!   and a deeper margin (`[m2]` under `[m1]`) nests. Taxis is
//!   positional, so a gap in the source numbering does not fail
//!   the kanon. A card whose body carries no numbering is one
//!   sense, never a guess at more. Lines before the first sense
//!   that hold only labels or a transcription are the entry's
//!   grammar line and pronunciation.
//! - `[trn]` is an `equivalent` in the contents language, `[ex]`
//!   a `citation`, `[p]` a `usage-label` (untyped: DSL does not
//!   say what kind of label it is; an etymology label such as
//!   Webster's "Etym:" makes the rest of the line the `etymology`),
//!   `[t]` a `pronunciation`, `[lang name="…"]` an `equivalent` in
//!   that language, `[b]` / `[i]` koine strong / emphasis, `[url]` a koine
//!   link, `<<…>>` and `[ref]` the koine ref resolved to the target
//!   card's onym, `['] … [/']` a combining acute on the stressed
//!   letter, `[s]` an enmedia block in the entry, `[*] … [/*]` an
//!   inline diaphane typed .secondary.
//! - The `#INCLUDE`d abbreviations dictionary, when supplied,
//!   becomes the `abbreviations` sim after the title — a koine
//!   definition list, one item per card: the label stays the
//!   abbreviation in the text, the list carries its expansion.
//! - Dropped, text kept: `[c]` colour, `[u]`, `[sub]`, `[sup]`,
//!   `[com]`, `[!trs]`; `{{…}}` comments are dropped whole.

use std::collections::HashMap;

use crate::dendron::{Annotations, Block, Document, Inline, Taxis};
use crate::error::{Error, ErrorKind, Result};

fn dsl_err(msg: String) -> Error {
    Error::new(ErrorKind::MissingResource(format!("dsl import: {msg}")))
}

// ---------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------

/// Decode a DSL file's bytes: gzip (dictzip) unwrapped, then
/// UTF-16 by its byte-order mark, UTF-8 by its mark or by being
/// valid, else the code page the `#SOURCE_CODE_PAGE` header names
/// (Cyrillic as cp1251, anything else as Latin-1).
pub fn decode(bytes: &[u8]) -> Result<String> {
    let bytes: std::borrow::Cow<[u8]> = if bytes.starts_with(&[0x1f, 0x8b]) {
        std::borrow::Cow::Owned(gunzip(bytes)?)
    } else {
        std::borrow::Cow::Borrowed(bytes)
    };
    let text = if bytes.starts_with(&[0xff, 0xfe]) {
        utf16(&bytes[2..], true)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        utf16(&bytes[2..], false)
    } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else if let Ok(s) = std::str::from_utf8(&bytes) {
        s.to_string()
    } else {
        let latin: String = bytes.iter().map(|&b| b as char).collect();
        let page = latin
            .lines()
            .take(8)
            .find_map(|l| l.trim().strip_prefix("#SOURCE_CODE_PAGE"))
            .map(|v| v.trim().trim_matches('"').to_string())
            .unwrap_or_default();
        if page.eq_ignore_ascii_case("Cyrillic") {
            bytes.iter().map(|&b| cp1251(b)).collect()
        } else {
            latin
        }
    };
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

#[cfg(feature = "bundle")]
fn gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::MultiGzDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| dsl_err(format!("gzip: {e}")))?;
    Ok(out)
}

#[cfg(not(feature = "bundle"))]
fn gunzip(_bytes: &[u8]) -> Result<Vec<u8>> {
    Err(dsl_err(
        "a .dsl.dz needs the `bundle` feature (gzip)".to_string(),
    ))
}

fn utf16(bytes: &[u8], le: bool) -> String {
    let units = bytes.chunks_exact(2).map(|p| {
        if le {
            u16::from_le_bytes([p[0], p[1]])
        } else {
            u16::from_be_bytes([p[0], p[1]])
        }
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// Windows-1251.
fn cp1251(b: u8) -> char {
    const HIGH: [char; 128] = [
        'Ђ', 'Ѓ', '‚', 'ѓ', '„', '…', '†', '‡', '€', '‰', 'Љ', '‹', 'Њ', 'Ќ', 'Ћ', 'Џ', 'ђ', '‘',
        '’', '“', '”', '•', '–', '—', '\u{fffd}', '™', 'љ', '›', 'њ', 'ќ', 'ћ', 'џ', '\u{a0}', 'Ў',
        'ў', 'Ј', '¤', 'Ґ', '¦', '§', 'Ё', '©', 'Є', '«', '¬', '\u{ad}', '®', 'Ї', '°', '±', 'І',
        'і', 'ґ', 'µ', '¶', '·', 'ё', '№', 'є', '»', 'ј', 'Ѕ', 'ѕ', 'ї', 'А', 'Б', 'В', 'Г', 'Д',
        'Е', 'Ж', 'З', 'И', 'Й', 'К', 'Л', 'М', 'Н', 'О', 'П', 'Р', 'С', 'Т', 'У', 'Ф', 'Х', 'Ц',
        'Ч', 'Ш', 'Щ', 'Ъ', 'Ы', 'Ь', 'Э', 'Ю', 'Я', 'а', 'б', 'в', 'г', 'д', 'е', 'ж', 'з', 'и',
        'й', 'к', 'л', 'м', 'н', 'о', 'п', 'р', 'с', 'т', 'у', 'ф', 'х', 'ц', 'ч', 'ш', 'щ', 'ъ',
        'ы', 'ь', 'э', 'ю', 'я',
    ];
    if b < 0x80 {
        b as char
    } else {
        HIGH[(b - 0x80) as usize]
    }
}

/// A DSL language name as a language code (ISO 639-1 where there
/// is one); an unknown name passes through lowercased.
fn language_code(name: &str) -> String {
    let n = name.trim().trim_matches('"');
    let code = match n.to_ascii_lowercase().as_str() {
        "english" => "en",
        "russian" => "ru",
        "german" => "de",
        "french" => "fr",
        "spanish" => "es",
        "italian" => "it",
        "portuguese" => "pt",
        "latin" => "la",
        "greek" => "el",
        "ancientgreek" | "ancient greek" | "greekancient" => "grc",
        "ukrainian" => "uk",
        "belarusian" => "be",
        "polish" => "pl",
        "czech" => "cs",
        "bulgarian" => "bg",
        "serbian" => "sr",
        "croatian" => "hr",
        "dutch" => "nl",
        "swedish" => "sv",
        "danish" => "da",
        "norwegian" => "no",
        "finnish" => "fi",
        "hungarian" => "hu",
        "turkish" => "tr",
        "arabic" => "ar",
        "hebrew" => "he",
        "chinese" => "zh",
        "japanese" => "ja",
        "korean" => "ko",
        "kazakh" => "kk",
        "tatar" => "tt",
        "armenian" => "hy",
        "georgian" => "ka",
        other => return other.replace(' ', "-"),
    };
    code.to_string()
}

// ---------------------------------------------------------------
// Lines and cards
// ---------------------------------------------------------------

#[derive(Default)]
struct Header {
    name: Option<String>,
    index_language: Option<String>,
    contents_language: Option<String>,
    include: Option<String>,
}

/// One card: its headword lines and its body lines (indentation
/// removed), `{{…}}` comments already dropped.
struct Card {
    headwords: Vec<String>,
    body: Vec<String>,
}

/// Strip `{{…}}` comments, which may span lines.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next();
            depth += 1;
            continue;
        }
        if c == '}' && chars.peek() == Some(&'}') && depth > 0 {
            chars.next();
            depth -= 1;
            continue;
        }
        if depth == 0 {
            out.push(c);
        }
    }
    out
}

fn split(text: &str) -> (Header, Vec<Card>) {
    let text = strip_comments(text);
    let mut header = Header::default();
    let mut cards: Vec<Card> = Vec::new();
    let mut current: Option<Card> = None;
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            if let Some(c) = current.take() {
                cards.push(c);
            }
            continue;
        }
        if current.is_none() && cards.is_empty() && line.starts_with('#') {
            let (key, value) = line[1..]
                .split_once(char::is_whitespace)
                .map_or((line[1..].trim(), ""), |(k, v)| (k.trim(), v.trim()));
            let value = value.trim_matches('"').to_string();
            match key {
                "NAME" => header.name = Some(value),
                "INDEX_LANGUAGE" => header.index_language = Some(value),
                "CONTENTS_LANGUAGE" => header.contents_language = Some(value),
                "INCLUDE" => header.include = Some(value),
                _ => {}
            }
            continue;
        }
        let indented = line.starts_with(' ') || line.starts_with('\t');
        match (&mut current, indented) {
            (Some(card), true) => card.body.push(line.trim_start().to_string()),
            (Some(card), false) if card.body.is_empty() => card.headwords.push(line.to_string()),
            (Some(_), false) => {
                // A headword directly after a body: a new card
                // without the blank line between.
                cards.push(current.take().unwrap());
                current = Some(Card {
                    headwords: vec![line.to_string()],
                    body: Vec::new(),
                });
            }
            (None, false) => {
                current = Some(Card {
                    headwords: vec![line.to_string()],
                    body: Vec::new(),
                });
            }
            (None, true) => {
                // Body text with no headword above it: a stray
                // continuation, read as a card without headword.
                current = Some(Card {
                    headwords: Vec::new(),
                    body: vec![line.trim_start().to_string()],
                });
            }
        }
    }
    if let Some(c) = current {
        cards.push(c);
    }
    (header, cards)
}

/// A headword's displayed text and its indexed text: the tags
/// lowered to text (a stress mark, an escape), the `{…}` runs
/// displayed and not indexed.
fn headword_forms(raw: &str) -> (String, String) {
    let empty = HashMap::new();
    let lower = Lower {
        headword: "",
        contents_lang: "",
        refs: &empty,
    };
    let raw = text_of(&lower.line(raw).inlines);
    let mut display = String::new();
    let mut indexed = String::new();
    let mut in_brace = false;
    for c in raw.chars() {
        match c {
            '{' => in_brace = true,
            '}' => in_brace = false,
            _ => {
                display.push(c);
                if !in_brace {
                    indexed.push(c);
                }
            }
        }
    }
    (collapse(&display), collapse(&indexed))
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------
// Tags -> inlines
// ---------------------------------------------------------------

/// What a body line yields: its margin level, its inline content,
/// and the media files it attaches.
struct Line {
    margin: u8,
    inlines: Vec<Inline>,
    media: Vec<String>,
}

struct Frame {
    tag: String,
    arg: Option<String>,
    content: Vec<Inline>,
}

struct Lower<'a> {
    headword: &'a str,
    contents_lang: &'a str,
    refs: &'a HashMap<String, String>,
}

impl Lower<'_> {
    fn line(&self, text: &str) -> Line {
        let mut margin = 0u8;
        let mut media = Vec::new();
        let mut stack: Vec<Frame> = vec![Frame {
            tag: String::new(),
            arg: None,
            content: Vec::new(),
        }];
        let mut buf = String::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let flush = |buf: &mut String, stack: &mut Vec<Frame>| {
            if !buf.is_empty() {
                stack
                    .last_mut()
                    .unwrap()
                    .content
                    .push(Inline::Text(std::mem::take(buf)));
            }
        };
        while i < chars.len() {
            let c = chars[i];
            match c {
                '\\' if i + 1 < chars.len() => {
                    buf.push(chars[i + 1]);
                    i += 2;
                }
                '~' => {
                    buf.push_str(self.headword);
                    i += 1;
                }
                '<' if chars.get(i + 1) == Some(&'<') => {
                    let end = find(&chars, i + 2, &['>', '>']).unwrap_or(chars.len());
                    let raw: String = chars[i + 2..end].iter().collect();
                    // The target is written like any text (a stress
                    // mark as a tag), so it is lowered before the
                    // lookup.
                    let target = text_of(&self.line(&raw).inlines);
                    flush(&mut buf, &mut stack);
                    stack
                        .last_mut()
                        .unwrap()
                        .content
                        .push(self.reference(&target));
                    i = (end + 2).min(chars.len());
                }
                '[' => {
                    let Some(end) = chars[i + 1..].iter().position(|&c| c == ']') else {
                        buf.push(c);
                        i += 1;
                        continue;
                    };
                    let end = i + 1 + end;
                    let tag: String = chars[i + 1..end].iter().collect();
                    i = end + 1;
                    if let Some(closing) = tag.strip_prefix('/') {
                        flush(&mut buf, &mut stack);
                        let closing = closing.trim();
                        // Close the innermost frame of that tag;
                        // a stray closer is ignored.
                        if let Some(pos) = stack
                            .iter()
                            .rposition(|f| f.tag == closing && !f.tag.is_empty())
                        {
                            while stack.len() > pos + 1 {
                                let frame = stack.pop().unwrap();
                                let lowered = self.close(frame, &mut media);
                                stack.last_mut().unwrap().content.extend(lowered);
                            }
                            let frame = stack.pop().unwrap();
                            let lowered = self.close(frame, &mut media);
                            stack.last_mut().unwrap().content.extend(lowered);
                        }
                        continue;
                    }
                    let (name, arg) = tag
                        .split_once(char::is_whitespace)
                        .map_or((tag.trim(), None), |(n, a)| {
                            (n.trim(), Some(a.trim().to_string()))
                        });
                    if let Some(level) = name.strip_prefix('m')
                        && let Ok(level) = level.parse::<u8>()
                        && name.len() <= 2
                    {
                        margin = level;
                        continue;
                    }
                    if name == "/" {
                        continue;
                    }
                    flush(&mut buf, &mut stack);
                    stack.push(Frame {
                        tag: name.to_string(),
                        arg,
                        content: Vec::new(),
                    });
                }
                _ => {
                    buf.push(c);
                    i += 1;
                }
            }
        }
        flush(&mut buf, &mut stack);
        while stack.len() > 1 {
            let frame = stack.pop().unwrap();
            let lowered = self.close(frame, &mut media);
            stack.last_mut().unwrap().content.extend(lowered);
        }
        let mut inlines = stack.pop().unwrap().content;
        trim_ends(&mut inlines);
        Line {
            margin,
            inlines,
            media,
        }
    }

    fn reference(&self, target: &str) -> Inline {
        let key = collapse(target);
        let onym = spellings(&key)
            .iter()
            .find_map(|k| self.refs.get(k).cloned())
            .unwrap_or_else(|| onym_of(&key, None));
        Inline::Monosim {
            symbol: ">".to_string(),
            param: onym,
            ann: Annotations::default(),
        }
    }

    fn close(&self, frame: Frame, media: &mut Vec<String>) -> Vec<Inline> {
        let Frame { tag, arg, content } = frame;
        let endo = |symbol: &str, content: Vec<Inline>, genoses: Vec<String>| {
            vec![Inline::Endo {
                symbol: symbol.to_string(),
                content,
                bracket_matching: true,
                ann: Annotations {
                    onym: None,
                    genoses,
                },
            }]
        };
        match tag.as_str() {
            "b" => endo("*", content, vec![]),
            "i" => endo("/", content, vec![]),
            "p" => endo("[", content, vec![]),
            "trn" => endo("=>", content, vec![self.contents_lang.to_string()]),
            "ex" => endo("~", content, vec![]),
            "t" => endo("=%", content, vec![]),
            "*" => vec![Inline::EndoDiaphane {
                content,
                ann: Annotations {
                    onym: None,
                    genoses: vec!["secondary".to_string()],
                },
            }],
            "lang" => {
                let name = arg
                    .as_deref()
                    .and_then(|a| a.split_once('=').map(|(_, v)| v))
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"');
                // A run in a named language: an equivalent in that
                // language (Dahl's Latin names, a bilingual's
                // glosses in a third language).
                let code = language_code(name);
                if code.is_empty() {
                    content
                } else {
                    endo("=>", content, vec![code])
                }
            }
            "ref" => {
                let target = text_of(&content);
                vec![self.reference(&target)]
            }
            "url" => endo("><", content, vec![]),
            "s" => {
                let file = text_of(&content);
                if !file.is_empty() {
                    media.push(file);
                }
                Vec::new()
            }
            "'" => {
                // The stressed letter: a combining acute follows it.
                let mut text = text_of(&content);
                text.push('\u{301}');
                vec![Inline::Text(text)]
            }
            _ => content,
        }
    }
}

fn find(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    (from..chars.len().saturating_sub(pat.len() - 1)).find(|&i| chars[i..i + pat.len()] == *pat)
}

fn text_of(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Endo { content, .. } | Inline::EndoDiaphane { content, .. } => {
                out.push_str(&text_of(content))
            }
            _ => {}
        }
    }
    out
}

fn trim_ends(inlines: &mut Vec<Inline>) {
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

/// The onym an autonym entry of this headword gets.
fn onym_of(headword: &str, taxis: Option<u64>) -> String {
    let lemma = vec![Inline::Text(headword.to_string())];
    crate::kanonizo::autonym_of(&lemma, &taxis.map(Taxis::Explicit)).unwrap_or_default()
}

// ---------------------------------------------------------------
// Senses
// ---------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Numbering {
    Arabic,
    Letter,
}

/// A line's leading sense number, removed from the line.
fn take_numbering(inlines: &mut [Inline]) -> Option<Numbering> {
    let Some(Inline::Text(t)) = inlines.first_mut() else {
        return None;
    };
    let s = t.trim_start();
    let mut chars = s.char_indices();
    let kind;
    let rest;
    match chars.next() {
        Some((_, c)) if c.is_ascii_digit() => {
            let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
            let after = &s[digits..];
            if !(after.starts_with(')') || after.starts_with('.')) {
                return None;
            }
            kind = Numbering::Arabic;
            rest = &after[1..];
        }
        Some((_, c)) if c.is_alphabetic() && c.is_lowercase() => {
            let (next_at, _) = chars.next()?;
            if !s[next_at..].starts_with(')') {
                return None;
            }
            kind = Numbering::Letter;
            rest = &s[next_at + 1..];
        }
        _ => return None,
    }
    if !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return None;
    }
    *t = rest.trim_start().to_string();
    Some(kind)
}

struct Sense {
    margin: u8,
    kind: Numbering,
    blocks: Vec<Block>,
    children: Vec<Sense>,
}

fn sense_block(sense: Sense, taxis: u64) -> Block {
    let mut children = sense.blocks;
    children.extend(
        sense
            .children
            .into_iter()
            .enumerate()
            .map(|(i, s)| sense_block(s, i as u64 + 1)),
    );
    Block::Para {
        symbol: ":".to_string(),
        taxis: Some(Taxis::Explicit(taxis)),
        lemma: Vec::new(),
        children,
        hypograph: Vec::new(),
        bracket_matching: false,
        ann: Annotations::default(),
    }
}

fn push_sense(forest: &mut Vec<Sense>, path: &mut Vec<usize>, sense: Sense) {
    // Pop to the parent this sense belongs under.
    while let Some(&last) = path.last() {
        let top = {
            let mut node: &Sense = &forest[path[0]];
            for &i in &path[1..] {
                node = &node.children[i];
            }
            let _ = last;
            node
        };
        let nest = match (top.kind, sense.kind) {
            (Numbering::Arabic, Numbering::Letter) => true,
            (Numbering::Letter, Numbering::Arabic) => false,
            _ => sense.margin > top.margin,
        };
        if nest {
            break;
        }
        path.pop();
    }
    let siblings: &mut Vec<Sense> = if path.is_empty() {
        forest
    } else {
        let mut node: &mut Sense = &mut forest[path[0]];
        for &i in &path[1..] {
            node = &mut node.children[i];
        }
        &mut node.children
    };
    siblings.push(sense);
    path.push(siblings.len() - 1);
}

fn current_sense<'a>(forest: &'a mut [Sense], path: &[usize]) -> Option<&'a mut Sense> {
    let mut node: &mut Sense = forest.get_mut(*path.first()?)?;
    for &i in &path[1..] {
        node = &mut node.children[i];
    }
    Some(node)
}

/// Whether a preamble line is entry furniture: only labels (the
/// grammar line) or only a transcription.
fn furniture(inlines: &[Inline]) -> Option<&'static str> {
    let mut kind: Option<&'static str> = None;
    for inline in inlines {
        match inline {
            Inline::Text(t) if t.trim().is_empty() || t.trim() == "," => {}
            Inline::Endo { symbol, .. } if symbol == "[" => {
                if kind.is_some_and(|k| k != "=&") {
                    return None;
                }
                kind = Some("=&");
            }
            Inline::Endo { symbol, .. } if symbol == "=%" => {
                if kind.is_some_and(|k| k != "=%") {
                    return None;
                }
                kind = Some("=%");
            }
            _ => return None,
        }
    }
    kind
}

/// A pronunciation solo (the transcription line may precede the
/// grammar line).
fn is_pronunciation(block: &Block) -> bool {
    matches!(block, Block::Paragraph(p) if matches!(p.as_slice(), [Inline::Endo { symbol, .. }] if symbol == "=%"))
}

/// A line that opens with usage labels followed by other
/// content: the labels' text, and the rest of the line.
fn leading_labels(inlines: &[Inline]) -> Option<(Vec<Inline>, Vec<Inline>)> {
    let mut labels: Vec<Inline> = Vec::new();
    let mut n = 0;
    for inline in inlines {
        match inline {
            Inline::Endo {
                symbol, content, ..
            } if symbol == "[" => {
                if !labels.is_empty() {
                    labels.push(Inline::Text(" ".to_string()));
                }
                labels.extend(content.iter().cloned());
            }
            Inline::Text(t) if t.trim().is_empty() && !labels.is_empty() => {}
            _ => break,
        }
        n += 1;
    }
    if labels.is_empty() || n == inlines.len() {
        return None;
    }
    let mut rest: Vec<Inline> = inlines[n..].to_vec();
    trim_ends(&mut rest);
    Some((labels, rest))
}

/// Whether the labels end in an etymology label ("Etym:", "Etym.",
/// "Etymology", "Этим."); the label itself is dropped.
fn split_etymology_label(mut labels: Vec<Inline>) -> (Vec<Inline>, bool) {
    let is_etym = |t: &str| {
        let t = t.trim().trim_end_matches([':', '.']).to_lowercase();
        matches!(t.as_str(), "etym" | "etymology" | "этим" | "этимология")
    };
    let Some(Inline::Text(last)) = labels.last() else {
        return (labels, false);
    };
    if !is_etym(last) {
        return (labels, false);
    }
    labels.pop();
    while matches!(labels.last(), Some(Inline::Text(t)) if t.trim().is_empty()) {
        labels.pop();
    }
    (labels, true)
}

fn solo(symbol: &str, content: Vec<Inline>, genoses: Vec<String>) -> Block {
    Block::Paragraph(vec![Inline::Endo {
        symbol: symbol.to_string(),
        content,
        bracket_matching: true,
        ann: Annotations {
            onym: None,
            genoses,
        },
    }])
}

// ---------------------------------------------------------------
// Entries
// ---------------------------------------------------------------

struct Registry {
    /// Indexed headword text -> onym of the card that owns it.
    refs: HashMap<String, String>,
    /// Displayed lemma -> homograph count (for the taxis).
    homographs: HashMap<String, u64>,
}

/// A headword as written and without its combining marks (the
/// stress a Russian dictionary prints, which a reference omits).
fn spellings(key: &str) -> Vec<String> {
    use unicode_normalization::UnicodeNormalization;
    let bare: String = key
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .nfc()
        .collect();
    if bare == key {
        vec![key.to_string()]
    } else {
        vec![key.to_string(), bare]
    }
}

fn registry(cards: &[Card]) -> Registry {
    let mut counts: HashMap<String, u64> = HashMap::new();
    for card in cards {
        if let Some(first) = card.headwords.first() {
            let (display, _) = headword_forms(first);
            *counts.entry(display).or_default() += 1;
        }
    }
    let mut seen: HashMap<String, u64> = HashMap::new();
    let mut refs = HashMap::new();
    let mut onyms = Vec::new();
    for card in cards {
        let Some(first) = card.headwords.first() else {
            continue;
        };
        let (display, indexed) = headword_forms(first);
        let taxis = if counts[&display] > 1 {
            let n = seen.entry(display.clone()).or_default();
            *n += 1;
            Some(*n)
        } else {
            None
        };
        let onym = onym_of(&display, taxis);
        onyms.push((display, indexed, onym));
    }
    // A reference lands on the card whose first headword it names
    // as displayed; else as indexed; else on the card that carries
    // it as a further headword — in that order over the whole
    // file, so a later card's headword outranks an earlier card's
    // sort key. Stress marks are not part of a reference's
    // spelling.
    for (display, _, onym) in &onyms {
        for key in spellings(display) {
            refs.entry(key).or_insert_with(|| onym.clone());
        }
    }
    for (_, indexed, onym) in &onyms {
        for key in spellings(indexed) {
            refs.entry(key).or_insert_with(|| onym.clone());
        }
    }
    for (card, (_, _, onym)) in cards.iter().filter(|c| !c.headwords.is_empty()).zip(&onyms) {
        for hw in card.headwords.iter().skip(1) {
            let (d, indexed) = headword_forms(hw);
            for key in [d, indexed] {
                for key in spellings(&key) {
                    refs.entry(key).or_insert_with(|| onym.clone());
                }
            }
        }
    }
    Registry {
        refs,
        homographs: counts,
    }
}

fn entry(headwords: &[String], body: &[String], taxis: Option<u64>, lower: &Lower) -> Block {
    let (display, indexed) = headwords
        .first()
        .map(|h| headword_forms(h))
        .unwrap_or_default();
    let lower = Lower {
        headword: &display,
        ..*lower
    };
    let mut children: Vec<Block> = Vec::new();
    if indexed != display && !indexed.is_empty() {
        children.push(solo(
            "=*",
            vec![Inline::Text(indexed)],
            vec!["sort".to_string()],
        ));
    }
    for hw in headwords.iter().skip(1) {
        let (d, _) = headword_forms(hw);
        children.push(solo(
            "=~",
            vec![Inline::Text(d)],
            vec!["spelling".to_string()],
        ));
    }
    // Sub-cards: a body line `@ headword` opens one, `@` alone
    // closes it; the lines between are its body.
    let mut forest: Vec<Sense> = Vec::new();
    let mut path: Vec<usize> = Vec::new();
    let mut preamble: Vec<Block> = Vec::new();
    let mut media: Vec<String> = Vec::new();
    let mut unnumbered: Vec<Block> = Vec::new();
    let mut subs: Vec<Block> = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let raw = &body[i];
        if let Some(sub) = raw.strip_prefix('@') {
            let sub = sub.trim();
            if sub.is_empty() {
                i += 1;
                continue;
            }
            let mut sub_body = Vec::new();
            let mut j = i + 1;
            while j < body.len() && !body[j].starts_with('@') {
                sub_body.push(body[j].clone());
                j += 1;
            }
            subs.push(entry(&[sub.to_string()], &sub_body, None, &lower));
            i = j;
            continue;
        }
        let mut line = lower.line(raw);
        media.append(&mut line.media);
        if line.inlines.is_empty() {
            i += 1;
            continue;
        }
        if let Some(kind) = take_numbering(&mut line.inlines) {
            trim_ends(&mut line.inlines);
            let blocks = if line.inlines.is_empty() {
                Vec::new()
            } else {
                vec![Block::Paragraph(line.inlines)]
            };
            push_sense(
                &mut forest,
                &mut path,
                Sense {
                    margin: line.margin,
                    kind,
                    blocks,
                    children: Vec::new(),
                },
            );
        } else if let Some(sense) = current_sense(&mut forest, &path) {
            sense.blocks.push(Block::Paragraph(line.inlines));
        } else if forest.is_empty()
            && unnumbered.is_empty()
            && preamble.iter().all(is_pronunciation)
            && let Some((labels, rest)) = leading_labels(&line.inlines)
        {
            // A first line that opens with labels: the labels are
            // the grammar line, what follows them stays a
            // paragraph of the entry — unless the last label is an
            // etymology label (Webster's "Etym:"), which makes the
            // rest the etymology.
            let (labels, etymology) = split_etymology_label(labels);
            if !labels.is_empty() {
                preamble.push(solo("=&", labels, vec![]));
            }
            if !rest.is_empty() {
                if etymology {
                    preamble.push(solo("=<", rest, vec![]));
                } else {
                    preamble.push(Block::Paragraph(rest));
                }
            }
        } else if forest.is_empty()
            && unnumbered.is_empty()
            && let Some(symbol) = furniture(&line.inlines)
        {
            let content = if symbol == "=&" {
                // The grammar line holds the labels' text.
                let mut out: Vec<Inline> = Vec::new();
                for inline in line.inlines {
                    match inline {
                        Inline::Endo { content, .. } => {
                            if !out.is_empty() {
                                out.push(Inline::Text(" ".to_string()));
                            }
                            out.extend(content);
                        }
                        Inline::Text(t) if !t.trim().is_empty() => out.push(Inline::Text(t)),
                        _ => {}
                    }
                }
                out
            } else {
                line.inlines
                    .into_iter()
                    .flat_map(|inline| match inline {
                        Inline::Endo { content, .. } => content,
                        other => vec![other],
                    })
                    .collect()
            };
            preamble.push(solo(symbol, content, vec![]));
        } else {
            unnumbered.push(Block::Paragraph(line.inlines));
        }
        i += 1;
    }
    children.extend(preamble);
    children.extend(media.into_iter().map(|param| Block::Enmedia { param }));
    if forest.is_empty() {
        // No numbering: one sense, the whole body.
        if !unnumbered.is_empty() {
            children.push(sense_block(
                Sense {
                    margin: 0,
                    kind: Numbering::Arabic,
                    blocks: unnumbered,
                    children: Vec::new(),
                },
                1,
            ));
        }
    } else {
        // Unnumbered lines before the first sense that are not
        // furniture belong to the entry, before its senses.
        children.extend(unnumbered);
        children.extend(
            forest
                .into_iter()
                .enumerate()
                .map(|(i, s)| sense_block(s, i as u64 + 1)),
        );
    }
    children.extend(subs);
    Block::Para {
        symbol: "!".to_string(),
        taxis: taxis.map(Taxis::Explicit),
        lemma: vec![Inline::Text(display)],
        children,
        hypograph: Vec::new(),
        bracket_matching: false,
        ann: Annotations::default(),
    }
}

// ---------------------------------------------------------------
// Documents
// ---------------------------------------------------------------

/// Import a DSL dictionary from its bytes (see `decode`).
pub fn dsl_to_document(bytes: &[u8]) -> Result<Document> {
    dsl_text_to_document(&decode(bytes)?, None)
}

/// Import a DSL dictionary from its bytes with the `#INCLUDE`d
/// abbreviations dictionary's bytes beside it.
pub fn dsl_to_document_with_abbreviations(bytes: &[u8], abbreviations: &[u8]) -> Result<Document> {
    dsl_text_to_document(&decode(bytes)?, Some(&decode(abbreviations)?))
}

/// The `#INCLUDE` header's file name, when the dictionary names
/// an abbreviations dictionary beside it.
pub fn dsl_include(bytes: &[u8]) -> Result<Option<String>> {
    let text = decode(bytes)?;
    Ok(split(&text).0.include)
}

/// Import decoded DSL text; `abbreviations` is the decoded text of
/// the abbreviations dictionary, when supplied.
pub fn dsl_text_to_document(text: &str, abbreviations: Option<&str>) -> Result<Document> {
    let (header, cards) = split(text);
    if cards.is_empty() && header.name.is_none() {
        return Err(dsl_err("no cards and no #NAME header".to_string()));
    }
    let contents_lang = header
        .contents_language
        .as_deref()
        .map(language_code)
        .unwrap_or_default();
    let reg = registry(&cards);
    let mut blocks: Vec<Block> = Vec::new();
    if let Some(name) = &header.name {
        blocks.push(solo("=", vec![Inline::Text(name.clone())], vec![]));
    }
    if header.index_language.is_some() || header.contents_language.is_some() {
        let mut codes = Vec::new();
        if let Some(l) = &header.index_language {
            codes.push(language_code(l));
        }
        if let Some(l) = &header.contents_language {
            codes.push(language_code(l));
        }
        blocks.push(solo("=/", vec![Inline::Text(codes.join(" "))], vec![]));
    }
    if let Some(abbr) = abbreviations {
        let (_, abbr_cards) = split(abbr);
        let items: Vec<Block> = abbr_cards
            .iter()
            .filter(|c| !c.headwords.is_empty())
            .map(|c| {
                let lower = Lower {
                    headword: "",
                    contents_lang: &contents_lang,
                    refs: &reg.refs,
                };
                let definitions: Vec<Block> = c
                    .body
                    .iter()
                    .map(|l| lower.line(l))
                    .filter(|l| !l.inlines.is_empty())
                    .map(|l| Block::Para {
                        symbol: ";".to_string(),
                        taxis: None,
                        lemma: Vec::new(),
                        children: vec![Block::Paragraph(l.inlines)],
                        hypograph: Vec::new(),
                        bracket_matching: false,
                        ann: Annotations::default(),
                    })
                    .collect();
                let mut lemma: Vec<Inline> = Vec::new();
                for (n, hw) in c.headwords.iter().enumerate() {
                    if n > 0 {
                        lemma.push(Inline::Text(", ".to_string()));
                    }
                    lemma.push(Inline::Text(headword_forms(hw).0));
                }
                Block::Para {
                    symbol: "::".to_string(),
                    taxis: None,
                    lemma,
                    children: definitions,
                    hypograph: Vec::new(),
                    bracket_matching: false,
                    ann: Annotations::default(),
                }
            })
            .collect();
        if !items.is_empty() {
            let list = Block::Para {
                symbol: "::;".to_string(),
                taxis: None,
                lemma: Vec::new(),
                children: items,
                hypograph: Vec::new(),
                bracket_matching: false,
                ann: Annotations::default(),
            };
            blocks.push(Block::Para {
                symbol: "[[".to_string(),
                taxis: None,
                lemma: Vec::new(),
                children: vec![list],
                hypograph: Vec::new(),
                bracket_matching: true,
                ann: Annotations::default(),
            });
        }
    }
    let mut seen: HashMap<String, u64> = HashMap::new();
    for card in &cards {
        if card.headwords.is_empty() {
            continue;
        }
        let (display, _) = headword_forms(&card.headwords[0]);
        let taxis = if reg.homographs.get(&display).copied().unwrap_or(0) > 1 {
            let n = seen.entry(display.clone()).or_default();
            *n += 1;
            Some(*n)
        } else {
            None
        };
        let lower = Lower {
            headword: &display,
            contents_lang: &contents_lang,
            refs: &reg.refs,
        };
        blocks.push(entry(&card.headwords, &card.body, taxis, &lower));
    }
    Ok(Document {
        dialect_id: "lexigramma".to_string(),
        dialect_version: None,
        blocks,
    })
}
