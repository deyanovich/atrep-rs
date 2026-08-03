//! Glossae: localized sim names (spec: "Glossae: Localized
//! Names"). A glossa is a per-language symbol-to-name mapping,
//! a separate small artifact (`<dialektos>.<lang>.glossa`) so a
//! new language never requires a new edition of the dialektos.
//! The symbol is the key, here as everywhere: names are
//! presentation and address surface, never identity.

use std::collections::BTreeMap;

use crate::dialektos::Dialektos;
use crate::error::{Error, ErrorKind, Result};

/// Parse and validate a `.glossa` source against its dialektos:
/// `@@@!atrep-glossa`, the `@=<dialektos>=><lang>` pair, then
/// one `@<symbol> <name>` line per named sim. Every symbol must
/// be defined in the dialektos; no symbol may appear twice; no
/// two symbols may share a localized name; a name follows the
/// primary-name rules (no braces).
pub fn parse_glossa_source(
    text: &str,
    dial: &Dialektos,
    lang: &str,
) -> Result<BTreeMap<String, String>> {
    let invalid = |msg: String| Error::new(ErrorKind::InvalidLektos(format!("glossa: {msg}")));
    let mut lines = text.lines();
    let mut line = lines.next().unwrap_or("");
    if line.starts_with("#!") {
        line = lines.next().unwrap_or("");
    }
    if line.trim() != "@@@!atrep-glossa" {
        return Err(invalid("missing `@@@!atrep-glossa` declaration".into()));
    }
    let mut declared = false;
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut seen_names: BTreeMap<String, String> = BTreeMap::new();
    for raw in lines {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("@@/") {
            continue;
        }
        // The first sigil line is the pair declaration; every
        // later line is a name line (a sim symbol may itself
        // begin with `=`, so the `@=` prefix cannot decide).
        if !declared {
            let Some(rest) = line.strip_prefix("@=") else {
                return Err(invalid(format!(
                    "expected the `@=<dialektos>=><lang>` pair, found `{line}`"
                )));
            };
            let Some((d, l)) = rest.split_once("=>") else {
                return Err(invalid(format!("malformed pair `{line}` (expected `=>`)")));
            };
            if d.trim() != dial.id || l.trim() != lang {
                return Err(invalid(format!(
                    "pair `{line}` does not match `{}.{lang}`",
                    dial.id
                )));
            }
            declared = true;
            continue;
        }
        let Some(rest) = line.strip_prefix('@') else {
            return Err(invalid(format!("unexpected line: `{line}`")));
        };
        let Some((symbol, name)) = rest.split_once(char::is_whitespace) else {
            return Err(invalid(format!("malformed name line: `{line}`")));
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(invalid(format!("empty name for `{symbol}`")));
        }
        if !crate::sigil::is_valid_name(name) {
            return Err(invalid(format!(
                "name `{name}` for `{symbol}`: letters, digits, and hyphens \
                 only, beginning and ending alphanumeric"
            )));
        }
        if !dial.sims.contains_key(symbol) {
            return Err(invalid(format!(
                "`{symbol}` is not defined in dialektos `{}`",
                dial.id
            )));
        }
        if names.contains_key(symbol) {
            return Err(invalid(format!("duplicate entry for `{symbol}`")));
        }
        if let Some(other) = seen_names.get(name) {
            return Err(invalid(format!(
                "name `{name}` is shared by `{other}` and `{symbol}`"
            )));
        }
        seen_names.insert(name.to_string(), symbol.to_string());
        names.insert(symbol.to_string(), name.to_string());
    }
    if !declared {
        return Err(invalid("missing pair declaration".into()));
    }
    Ok(names)
}
