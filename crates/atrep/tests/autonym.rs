//! Auto-onymization: a para-sim declared `autonym` registers its
//! lemma, sim-stripped and normalized, as its onym; taxis
//! disambiguates homographs; kanonizo pins the value.

use std::fs;
use std::path::PathBuf;

fn workspace(name: &str, dia: &str, atd: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("atrep-autonym-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("testlex.dia"), dia).unwrap();
    let doc = dir.join("doc.atd");
    fs::write(&doc, atd).unwrap();
    (dir, doc)
}

const DIA: &str =
    "@@@!atrep\n\n@=== entry\n@![(taxis)] lemma\nautonym\ngrammata\n!@\n@\"test entry\"@\n===@\n";

#[test]
fn autonym_assigns_normalizes_and_disambiguates() {
    // Homograph taxis runs per lemma, not per sibling run:
    // bank 1-2 then crane 1-2 in one document.
    let atd = "@@@!testlex\n\n@! βαίνω\nsense one.\n!@\n\n@!(1) bank note\nmoney.\n!@\n\n@!(2) bank note\nriver.\n!@\n\n@!(1) crane\nbird.\n!@\n\n@!(2) crane\nmachine.\n!@\n";
    let (_dir, doc) = workspace("basic", DIA, atd);
    let result = atrep::kanonizo::kanonizo_file(&doc).unwrap();
    // Greek headword becomes the onym verbatim; spaces become
    // hyphens; the taxis suffixes.
    assert!(result.kanon.contains("(βαίνω)"), "{}", result.kanon);
    assert!(result.kanon.contains("(bank-note-2)"), "{}", result.kanon);
    assert!(result.kanon.contains("(crane-1)"), "{}", result.kanon);
    // Idempotence: re-kanonizing the kanon leaves it unchanged.
    let atk = doc.with_extension("atk");
    fs::write(&atk, &result.kanon).unwrap();
    let again = atrep::kanonizo::kanonizo_file(&atk).unwrap();
    assert_eq!(result.kanon, again.kanon);
}

#[test]
fn autonym_duplicate_headwords_error() {
    let atd = "@@@!testlex\n\n@! bank\none.\n!@\n\n@! bank\ntwo.\n!@\n";
    let (_dir, doc) = workspace("dup", DIA, atd);
    let err = atrep::kanonizo::kanonizo_file(&doc);
    assert!(err.is_err());
    assert!(format!("{}", err.unwrap_err()).contains("duplicate onym"));
}

#[test]
fn autonym_contradicting_explicit_onym_errors() {
    let atd = "@@@!testlex\n\n@! bank\none.\n!@(vault)\n";
    let (_dir, doc) = workspace("contradict", DIA, atd);
    let err = atrep::kanonizo::kanonizo_file(&doc);
    assert!(err.is_err());
    assert!(format!("{}", err.unwrap_err()).contains("contradicts"));
}

#[test]
fn autonym_homographs_must_all_carry_taxis() {
    let atd = "@@@!testlex\n\n@! bank\none.\n!@\n\n@!(2) bank\ntwo.\n!@\n";
    let (_dir, doc) = workspace("mixed", DIA, atd);
    let err = atrep::kanonizo::kanonizo_file(&doc);
    assert!(err.is_err());
    assert!(format!("{}", err.unwrap_err()).contains("must all carry a taxis"));
}
