//! Core milestones (spec v0.12/v0.12.1): syntax validation,
//! litosis survival, derived quasi-coordinate values, and the
//! scheme-filtered `*milestone` exo pattern.

use std::path::{Path, PathBuf};

use atrep::{dialektos, exo, kanonizo, litosis, parser};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn milestone_syntax_is_validated() {
    let parse =
        |body: &str| parser::parse_document(&format!("@@@!koine\n\n{body}\n"), Path::new("t.atd"));
    // Well-formed, including colons/periods in the value and
    // the derived quasi-coordinate bar (spec v0.12.1).
    assert!(parse(r#"Text @("steph:43a") more."#).is_ok());
    assert!(parse(r#"Text @("bcv:jhn.3.16") more."#).is_ok());
    assert!(parse(r#"Text @("steph:1a|0.1") more."#).is_ok());
    // Genos annotations are allowed.
    assert!(parse(r#"Text @("steph:43a").page more."#).is_ok());
    // No colon: not a coordinate.
    assert!(parse(r#"Text @("nocolon") more."#).is_err());
    // Scheme must satisfy the genos grammar.
    assert!(parse(r#"Text @("Steph:43a") more."#).is_err());
    // Value must satisfy the onym grammar.
    assert!(parse(r#"Text @("steph:4#3") more."#).is_err());
    // A milestone cannot carry an onym suffix.
    assert!(parse(r#"Text @("steph:43a")(name) more."#).is_err());
}

#[test]
fn milestone_free_genoses_strip_at_litosis() {
    let dir = tmp_dir("milestone-litos");
    let atd = "@@@!koine\n\nOpening @(\"steph:43a\").page text.\n";
    std::fs::write(dir.join("doc.atd"), atd).unwrap();
    let result = kanonizo::kanonizo_file(&dir.join("doc.atd")).unwrap();
    let lookup = litosis::media_from_dir(&dir);
    let litos = litosis::litosis(&result.document, &lookup).unwrap();
    // Scheme and value survive verbatim; the free genos strips.
    assert!(litos.litos.contains(r#"@("steph:43a")"#), "{}", litos.litos);
    assert!(!litos.litos.contains(".page"), "{}", litos.litos);
}

#[test]
fn milestone_duplicate_coordinates_error() {
    let dir = tmp_dir("milestone-dup");
    let atd = "@@@!koine\n\nOne @(\"steph:43a\") two @(\"steph:43a\") three.\n";
    std::fs::write(dir.join("doc.atd"), atd).unwrap();
    let err = kanonizo::kanonizo_file(&dir.join("doc.atd"));
    assert!(err.is_err());
    assert!(format!("{}", err.unwrap_err()).contains("duplicate milestone"));
}

#[test]
fn scheme_filtered_milestone_rule_beats_the_generic() {
    let dir = tmp_dir("milestone-exo");
    std::fs::write(
        dir.join("mstest.lektos"),
        "@@@!atrep\n\n@=== emphasis\n@/ grammata /@\n===@\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("mstest.txt.exo"),
        "@@@!atrep-exo\n@=mstest=>txt\n\n\
         @-> *document\n@(grammata)\n>-@\n\n\
         @-> *paragraph\n@(grammata)\n>-@\n\n\
         @-> *milestone steph\nSTEPH[@(value)]\n>-@\n\n\
         @-> *milestone\nGEN[@(scheme):@(value)]\n>-@\n\n\
         @-> /\n@(grammata)\n>-@\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("doc.atd"),
        "@@@!mstest\n\nAt @(\"steph:43a\") and @(\"bekker:1094a\") both.\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&dir.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&dir, "mstest", "txt").unwrap();
    let out = exo::render(&result.document, &x, &dir).unwrap();
    // The steph milestone takes the scheme-scoped rule; the
    // bekker one falls back to the generic rule.
    assert!(out.contains("STEPH[43a]"), "{out}");
    assert!(out.contains("GEN[bekker:1094a]"), "{out}");
    let _ = dialektos::resolve(&dir, "mstest").unwrap();
}
