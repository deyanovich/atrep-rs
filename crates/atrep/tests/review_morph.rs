//! Regression tests for the morph review group: composition over
//! lossy renames, inherited vocabularies, englossis relabeling,
//! aliased imports, iso symmetry, and bare-filename resolution.

use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::{dendron, dialektos, kanonizo, morph};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn kanon_of(tmp: &Path, source: &str) -> atrep::Document {
    std::fs::write(tmp.join("doc.atd"), source).unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document
}

const FULL_BOX: &str =
    "@@@!atrep\n\n@=== box\n@-[(taxis)] [lemma]\ngrammata\n-@ [hypograph]\n===@\n";
const BARE_BOX: &str = "@@@!atrep\n\n@=== box\n@-\ngrammata\n-@\n===@\n";

/// Components a lossy rename drops at the intermediate hop stay
/// dropped in the composite: an extract of the bare box demotes
/// to an unwrap, and a rename back into a form that carries them
/// again has no normal form (the route applies staged).
#[test]
fn compose_honors_components_lost_at_the_intermediate_hop() {
    let tmp = tmp_dir("review-morph-lossy-compose");
    std::fs::write(tmp.join("aa.dia"), FULL_BOX).unwrap();
    std::fs::write(tmp.join("bb.dia"), BARE_BOX).unwrap();
    std::fs::write(tmp.join("cc.dia"), FULL_BOX).unwrap();
    std::fs::write(tmp.join("aa.bb.hom"), "@@@!atrep-hom\n@=aa=>bb\n").unwrap();
    std::fs::write(tmp.join("bb.cc.hom"), "@@@!atrep-hom\n@=bb=>cc\n\n@<# -\n").unwrap();
    let doc = kanon_of(
        &tmp,
        "@@@!aa\n\n@-(1) The Iliad, Book I\nSing, goddess, the wrath of Achilles.\n-@ Homer\n",
    );
    let f = morph::resolve_morph(&tmp, "aa", "bb").unwrap();
    let g = morph::resolve_morph(&tmp, "bb", "cc").unwrap();

    // Rename-then-extract: the lemma is gone before the extract.
    let staged = morph::apply_route_staged(&doc, &[f.clone(), g.clone()]).unwrap();
    assert_eq!(
        dendron::serialize(&staged),
        "@@@!cc\n\nSing, goddess, the wrath of Achilles.\n"
    );
    let fg = morph::compose(&f, &g).unwrap();
    assert_eq!(
        dendron::serialize(&morph::apply(&doc, &fg).unwrap()),
        dendron::serialize(&staged)
    );
    assert!(
        morph::serialize_hom(&fg).contains("@<< -\n"),
        "{}",
        morph::serialize_hom(&fg)
    );
    assert_eq!(
        dendron::serialize(&morph::apply_route(&doc, &[f.clone(), g]).unwrap()),
        dendron::serialize(&staged)
    );

    // Rename-then-rename back into the richer form: refused, and
    // the route falls back to staged application.
    std::fs::write(tmp.join("bb.cc.hom"), "@@@!atrep-hom\n@=bb=>cc\n").unwrap();
    let g = morph::resolve_morph(&tmp, "bb", "cc").unwrap();
    let err = morph::compose(&f, &g).unwrap_err();
    assert!(
        matches!(&err.kind, ErrorKind::InvalidMorph(s) if s.contains("apply the route staged")),
        "{err}"
    );
    let staged = morph::apply_route_staged(&doc, &[f.clone(), g.clone()]).unwrap();
    assert_eq!(
        dendron::serialize(&staged),
        "@@@!cc\n\n@-\nSing, goddess, the wrath of Achilles.\n-@\n"
    );
    assert_eq!(
        dendron::serialize(&morph::apply_route(&doc, &[f, g]).unwrap()),
        dendron::serialize(&staged)
    );
    let route = morph::resolve_route(&tmp, "aa", "cc").unwrap();
    assert_eq!(route.len(), 2);
    assert_eq!(
        dendron::serialize(&morph::apply_route(&doc, &route).unwrap()),
        dendron::serialize(&staged)
    );
}

const BIBLIO: &str = "@@@!atrep\n\n\
    @=== entry\n@& lemma\ngrammata\n&@\n@% genera\n===@\n\n\
    @=== field\n@: lemma=campi\ngrammata\n:@\n===@\n\n\
    @==% genera\nliber: book buch livre\n%==@\n\n\
    @==% campi\nauctor: author autor auteur\ntitulus: title titel titre\n%==@\n";

/// An heir inherits the vocabularies its sims bind, so alias
/// canonicalization works in the heir and its canonical .lektos
/// carries the `@==%` blocks its `lemma=` / `@%` bindings name.
#[test]
fn inheritance_brings_the_parent_vocabularies() {
    let tmp = tmp_dir("review-morph-inherited-vocab");
    std::fs::write(tmp.join("biblio.dia"), BIBLIO).unwrap();
    std::fs::write(tmp.join("kid.dia"), "@@@!atrep\n\n@@::biblio\n").unwrap();
    let kid = dialektos::resolve(&tmp, "kid").unwrap();
    assert_eq!(kid.vocabularies.len(), 2);
    let lektos = dialektos::serialize(&kid);
    assert!(lektos.contains("@==% campi\n"), "{lektos}");
    assert!(lektos.contains("@==% genera\n"), "{lektos}");

    let doc = kanon_of(&tmp, "@@@!kid\n\n@& k1\n@: auteur\nHomer\n:@\n&@.livre\n");
    assert_eq!(
        dendron::serialize(&doc),
        "@@@!kid\n\n@& k1\n@: auctor\nHomer\n:@\n&@.liber\n"
    );

    // A list import brings only the vocabularies its sims bind.
    std::fs::write(tmp.join("kid.dia"), "@@@!atrep\n\n@@::biblio::[:]\n").unwrap();
    let kid = dialektos::resolve(&tmp, "kid").unwrap();
    let names: Vec<&String> = kid.vocabularies.keys().collect();
    assert_eq!(names, vec!["campi"]);
}

/// An englossis of the source dialektos (what a transcluded
/// sibling document becomes) is mapped with the rest and
/// declares the target dialektos afterwards, so the result is a
/// kanon of the target throughout and renders with the target's
/// exomorphosis.
#[test]
fn englossis_of_the_source_declares_the_target() {
    let tmp = tmp_dir("review-morph-englossis");
    let doc = kanon_of(
        &tmp,
        "@@@!litogramma\n\n\
         @=The Iliad=@\n\n\
         Sing, goddess, the wrath of Achilles.\n\n\
         @@@!(litogramma)\n\
         @#(1) Book I\n\
         The wrath of @/Peleus/@' son.\n\
         #@\n\
         !@@@\n",
    );
    let m = morph::resolve_morph(&tmp, "litogramma", "at-tei").unwrap();
    let out = morph::apply(&doc, &m).unwrap();
    let atk = dendron::serialize(&out);
    assert!(atk.contains("@@@!(at-tei)\n"), "{atk}");
    assert!(!atk.contains("(litogramma)"), "{atk}");
    let x = atrep::exo::resolve_exo(&tmp, "at-tei", "tei").unwrap();
    let tei = atrep::exo::render(&out, &x, &tmp).unwrap();
    assert!(tei.contains("son."), "{tei}");
}

/// An .iso serves both directions, so a rename that is strictly
/// compatible one way only (a required lemma against an optional
/// one) is refused in both directions, explicitly or implicitly.
#[test]
fn iso_is_validated_symmetrically() {
    let tmp = tmp_dir("review-morph-iso-symmetry");
    std::fs::write(
        tmp.join("req.dia"),
        "@@@!atrep\n\n@=== box\n@- lemma\ngrammata\n-@\n===@\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("opt.dia"),
        "@@@!atrep\n\n@=== box\n@- [lemma]\ngrammata\n-@\n===@\n",
    )
    .unwrap();
    // Implicit identity: refused both ways (the file is then not
    // total over `-`).
    std::fs::write(tmp.join("req.opt.iso"), "@@@!atrep-iso\n@=req<=>opt\n").unwrap();
    for (a, b) in [("req", "opt"), ("opt", "req")] {
        let err = morph::resolve_morph(&tmp, a, b).unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::MorphIncomplete(s) if s.ends_with("-")),
            "{a}=>{b}: {err}"
        );
    }
    // Explicit rename: the form error names the direction.
    std::fs::write(
        tmp.join("req.opt.iso"),
        "@@@!atrep-iso\n@=req<=>opt\n\n@:: - -\n",
    )
    .unwrap();
    for (a, b) in [("req", "opt"), ("opt", "req")] {
        let err = morph::resolve_morph(&tmp, a, b).unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::InvalidMorph(s) if s.contains("form-incompatible")),
            "{a}=>{b}: {err}"
        );
    }
    // The same pair as a .hom is fine in the lossless direction.
    std::fs::remove_file(tmp.join("req.opt.iso")).unwrap();
    std::fs::write(tmp.join("req.opt.hom"), "@@@!atrep-hom\n@=req=>opt\n").unwrap();
    morph::resolve_morph(&tmp, "req", "opt").unwrap();
}

/// An aliased single import stores the alias as the sim's
/// symbol, so it must be a symbol: a run of symbolic characters
/// with the braces reserved. Otherwise check accepts a .dia
/// whose canonical .lektos cannot be parsed back.
#[test]
fn import_alias_must_be_a_symbol() {
    let tmp = tmp_dir("review-morph-import-alias");
    std::fs::write(tmp.join("biblio.dia"), BIBLIO).unwrap();
    for (alias, needle) in [("ab", "symbolic"), ("{", "reserved"), ("a-", "symbolic")] {
        std::fs::write(
            tmp.join("kid.dia"),
            format!("@@@!atrep\n\n@@::biblio::& {alias}\n"),
        )
        .unwrap();
        let err = dialektos::resolve(&tmp, "kid").unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::InvalidLektos(s) if s.contains(needle)),
            "alias `{alias}`: {err}"
        );
    }
    // A symbolic alias round-trips through the canonical form.
    std::fs::write(tmp.join("kid.dia"), "@@@!atrep\n\n@@::biblio::& $$\n").unwrap();
    let kid = dialektos::resolve(&tmp, "kid").unwrap();
    assert!(kid.sims.contains_key("$$"));
    let lektos = dialektos::serialize(&kid);
    std::fs::write(tmp.join("kid.lektos"), &lektos).unwrap();
    let again = dialektos::resolve(&tmp, "kid").unwrap();
    assert_eq!(again.sims, kid.sims);
    // An import naming no sim is malformed.
    std::fs::remove_file(tmp.join("kid.lektos")).unwrap();
    std::fs::write(tmp.join("kid.dia"), "@@@!atrep\n\n@@::biblio::\n").unwrap();
    let err = dialektos::resolve(&tmp, "kid").unwrap_err();
    assert!(
        matches!(&err.kind, ErrorKind::InvalidLektos(s) if s.contains("malformed import")),
        "{err}"
    );
}

/// Vocabulary bindings and definitions are validated: a
/// `lemma=` or `@%` naming nothing, a vocabulary line with no
/// canonical term, and a binding to an undefined vocabulary are
/// all errors rather than silent no-ops.
#[test]
fn vocabulary_bindings_are_validated() {
    let tmp = tmp_dir("review-morph-vocab-validation");
    let cases = [
        (
            "@@@!atrep\n\n@=== y\n@-- lemma=\ngrammata\n--@\n===@\n",
            "names no vocabulary",
        ),
        ("@@@!atrep\n\n@=== y\n@--\ngrammata\n--@\n@% \n===@\n", ""),
        ("@@@!atrep\n\n@==% v\n: b\n%==@\n", "no canonical term"),
        (
            "@@@!atrep\n\n@=== y\n@-- lemma=nowhere\ngrammata\n--@\n===@\n",
            "`nowhere` is not defined",
        ),
        (
            "@@@!atrep\n\n@=== y\n@-- grammata --@\n@% nowhere\n===@\n",
            "`nowhere` is not defined",
        ),
    ];
    for (src, needle) in cases {
        std::fs::write(tmp.join("v.dia"), src).unwrap();
        let err = dialektos::resolve(&tmp, "v").unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::InvalidLektos(s) if s.contains(needle)),
            "{src}\n{err}"
        );
    }
    // Defined (here or inherited) bindings pass.
    std::fs::write(tmp.join("biblio.dia"), BIBLIO).unwrap();
    std::fs::write(
        tmp.join("v.dia"),
        "@@@!atrep\n\n@@::biblio\n\n@=== extra\n@-- lemma=campi\ngrammata\n--@\n@% genera\n===@\n",
    )
    .unwrap();
    dialektos::resolve(&tmp, "v").unwrap();
}

/// A partial morphism (a derived embedding, or a composite over
/// one) reports the sims it leaves unmapped: these have no
/// faithful `.hom` spelling, since an explicit file must be
/// total and reloading one closes the gaps.
#[test]
fn partial_morphisms_report_their_unmapped_sims() {
    let tmp = tmp_dir("review-morph-unmapped");
    // at-markdown's footnote does not come from at-html.
    let derived = morph::resolve_morph(&tmp, "at-markdown", "at-html").unwrap();
    assert_eq!(derived.unmapped(), vec!["^".to_string()]);
    let onward = morph::resolve_morph(&tmp, "at-html", "at-rst").unwrap();
    assert_eq!(onward.unmapped(), Vec::<String>::new());
    let fused = morph::compose(&derived, &onward).unwrap();
    assert_eq!(fused.unmapped(), vec!["^".to_string()]);

    // An heir excluding a parent sim and redefining its symbol:
    // the derived embedding deliberately leaves it unmapped.
    std::fs::write(
        tmp.join("kid.dia"),
        "@@@!atrep\n\n@@::at-html:-:[/]\n\n@=== slash-own\n@/ grammata /@\n===@\n",
    )
    .unwrap();
    let kid = morph::resolve_morph(&tmp, "kid", "at-html").unwrap();
    assert_eq!(kid.unmapped(), vec!["/".to_string()]);
    let doc = kanon_of(&tmp, "@@@!kid\n\nA @/b/@ path.\n");
    let err = morph::apply(&doc, &kid).unwrap_err();
    assert!(
        matches!(&err.kind, ErrorKind::MorphUnmapped(s) if s == "/"),
        "{err}"
    );
}

/// A document given as a bare file name resolves in the working
/// directory: the empty parent path reads as `.`, so route
/// discovery sees the local definition files.
#[test]
fn bare_file_name_resolves_in_the_working_directory() {
    let tmp = tmp_dir("review-morph-bare-name");
    std::fs::write(tmp.join("aa.dia"), FULL_BOX).unwrap();
    std::fs::write(tmp.join("bb.dia"), BARE_BOX).unwrap();
    std::fs::write(tmp.join("cc.dia"), BARE_BOX).unwrap();
    std::fs::write(tmp.join("aa.bb.hom"), "@@@!atrep-hom\n@=aa=>bb\n").unwrap();
    std::fs::write(tmp.join("bb.cc.hom"), "@@@!atrep-hom\n@=bb=>cc\n").unwrap();
    std::env::set_current_dir(&tmp).unwrap();
    let empty = Path::new("doc.atd").parent().unwrap();
    assert_eq!(empty, Path::new(""));
    let route = morph::resolve_route(empty, "aa", "cc").unwrap();
    assert_eq!(route.len(), 2);
    assert_eq!(route[1].target, "cc");
    let kid = dialektos::parse_file(Path::new("aa.dia")).unwrap();
    assert!(kid.sims.contains_key("-"));
}
