//! Sigil characters, escaping, symbolic character class, and
//! episymbol derivation (spec: Core Syntax; Parsing / Tokenization).

/// The canonical sigil character.
pub const CANONICAL: char = '@';
/// The alias sigil character.
pub const ALIAS: char = '\\';

/// Which of the two sigil characters is active in a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sigil {
    Canonical,
    Alias,
}

impl Sigil {
    /// The active sigil character.
    pub fn active(self) -> char {
        match self {
            Sigil::Canonical => CANONICAL,
            Sigil::Alias => ALIAS,
        }
    }

    /// The inactive sigil character (the escape prefix).
    pub fn inactive(self) -> char {
        match self {
            Sigil::Canonical => ALIAS,
            Sigil::Alias => CANONICAL,
        }
    }
}

/// A symbolic character: not alphanumeric (any script), not
/// whitespace, and not a sigil character.
pub fn is_symbolic(c: char) -> bool {
    !c.is_alphanumeric() && !c.is_whitespace() && c != CANONICAL && c != ALIAS
}

/// Bracket flipped to its matching counterpart; other characters
/// unchanged.
pub fn flip_bracket(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        _ => c,
    }
}

/// The episymbol of a symbol: reversed characters, with brackets
/// flipped unless bracket matching is disabled.
pub fn episymbol(symbol: &str, bracket_matching: bool) -> String {
    symbol
        .chars()
        .rev()
        .map(|c| if bracket_matching { flip_bracket(c) } else { c })
        .collect()
}

/// Validate an onym identifier: alphanumeric characters (arbitrary
/// script), dashes, underscores, colons, and periods; must start and
/// end with an alphanumeric; no consecutive symbolic characters.
pub fn is_valid_onym(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut prev_symbolic = false;
    let mut chars = s.chars().peekable();
    let mut first = true;
    let mut last_alnum = false;
    while let Some(c) = chars.next() {
        let alnum = c.is_alphanumeric();
        let allowed_sym = matches!(c, '-' | '_' | ':' | '.');
        if !alnum && !allowed_sym {
            return false;
        }
        if first && !alnum {
            return false;
        }
        if !alnum && prev_symbolic {
            return false;
        }
        prev_symbolic = !alnum;
        last_alnum = alnum;
        first = false;
        let _ = chars.peek();
    }
    last_alnum
}

/// Validate a milestone value: one onym-valid piece, or several
/// joined by `|` — the quasi-coordinate notation (`17a|1`,
/// `17a|0.1`) that `quasialign` inserts at statistical midpoints.
/// The pipe marks a derived, non-citable coordinate; each piece on
/// its own obeys the onym rules.
pub fn is_valid_milestone_value(s: &str) -> bool {
    !s.is_empty() && s.split('|').all(is_valid_onym)
}

/// Validate a genos identifier: kebab case — lowercase alphanumeric
/// characters (arbitrary script, so caseless scripts qualify) and
/// hyphens; must start with a letter; no consecutive hyphens; must
/// not end with a hyphen.
pub fn is_valid_genos(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut prev_hyphen = false;
    for (i, c) in s.chars().enumerate() {
        if c == '-' {
            if i == 0 || prev_hyphen {
                return false;
            }
            prev_hyphen = true;
            continue;
        }
        if c.is_uppercase() || !c.is_alphanumeric() {
            return false;
        }
        if i == 0 && !c.is_alphabetic() {
            return false;
        }
        prev_hyphen = false;
    }
    !s.ends_with('-')
}

/// A character that may start a genos identifier.
pub fn is_genos_start(c: char) -> bool {
    c.is_alphabetic() && !c.is_uppercase()
}

/// A character that may continue a genos identifier.
pub fn is_genos_continue(c: char) -> bool {
    c == '-' || (c.is_alphanumeric() && !c.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episymbols() {
        assert_eq!(episymbol("#", true), "#");
        assert_eq!(episymbol("#=", true), "=#");
        assert_eq!(episymbol("{", true), "}");
        assert_eq!(episymbol(":[", true), "]:");
        assert_eq!(episymbol("=>", true), "<=");
        assert_eq!(episymbol("<", false), "<");
    }

    #[test]
    fn onym_grammar() {
        assert!(is_valid_onym("figure-one"));
        assert!(is_valid_onym("a"));
        assert!(is_valid_onym("o1"));
        assert!(is_valid_onym("sec:intro.2_a"));
        assert!(is_valid_onym("сноска-1"));
        assert!(!is_valid_onym(""));
        assert!(!is_valid_onym("-a"));
        assert!(!is_valid_onym("a-"));
        assert!(!is_valid_onym("a--b"));
        assert!(!is_valid_onym("a b"));
        assert!(!is_valid_onym("a/b"));
    }

    #[test]
    fn genos_grammar() {
        assert!(is_valid_genos("important"));
        assert!(is_valid_genos("foreign-word"));
        assert!(is_valid_genos("латинский"));
        assert!(is_valid_genos("a1"));
        assert!(!is_valid_genos("1a"));
        assert!(!is_valid_genos("Important"));
        assert!(!is_valid_genos("a--b"));
        assert!(!is_valid_genos("a-"));
        assert!(!is_valid_genos("-a"));
        assert!(!is_valid_genos(""));
    }
}
