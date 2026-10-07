//! litogramma at core: the literary dialektos (and its
//! bibliogramma companion) resolve from the embedded standard
//! library, kanonize, and export through the bundled exos —
//! with no local definition files.

use std::path::{Path, PathBuf};

use atrep::{exo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SPECIMEN: &str = "\
@@@!litogramma

@=Solitude in the Early Ode=@

@=:A. Careful:=@

@#(1) The Claim
Pope's early ode already contains the @/whole program/@.@^(n1)

@^
Written at about twelve.
^@(n1)
#@
";

#[test]
fn litogramma_resolves_from_std() {
    let tmp = tmp_dir("litogramma-std");
    std::fs::write(tmp.join("doc.atd"), SPECIMEN).unwrap();
    // No litogramma.dia next to the document: resolution must
    // fall back to the embedded standard library.
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let atd = atrep::dendron::serialize(&kanon.document);
    assert!(atd.contains("@=Solitude in the Early Ode=@"));
    assert!(atd.contains("@/whole program/@"));
}

#[test]
fn litogramma_std_exos_render() {
    let tmp = tmp_dir("litogramma-exo");
    std::fs::write(tmp.join("doc.atd"), SPECIMEN).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    for target in ["html", "gemtext", "latex"] {
        let x = exo::resolve_exo(&tmp, "litogramma", target).unwrap();
        let out = exo::render(&kanon.document, &x, &tmp).unwrap();
        assert!(
            out.contains("whole program"),
            "{target} export lost content"
        );
        assert!(
            out.contains("Solitude in the Early Ode"),
            "{target} export lost the title"
        );
    }
}

#[test]
fn bibliogramma_resolves_from_std() {
    let tmp = tmp_dir("bibliogramma-std");
    let doc = "\
@@@!bibliogramma

@& pope1700
@: author
Pope, Alexander
:@

@: title
Ode on Solitude
:@

@: year
1700
:@
&@.article
";
    std::fs::write(tmp.join("refs.atd"), doc).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("refs.atd")).unwrap();
    for target in ["bib", "bibtex"] {
        let x = exo::resolve_exo(&tmp, "bibliogramma", target).unwrap();
        let out = exo::render(&kanon.document, &x, &tmp).unwrap();
        assert!(
            out.contains("pope1700"),
            "{target} export lost the entry key"
        );
    }
}

/// Drift guard: the embedded std is the home of the litogramma
/// files the engine ships; the litogramma repo is the workbench
/// where they are edited with local precedence. Every file present
/// in both places must be byte-identical (the vendored set is
/// derived, not listed: any std file the repo also has), which is
/// the same rule `scripts/vendor-litogramma.sh check` applies.
/// Absent the sibling checkout (standalone), the guard is skipped.
#[test]
fn std_copies_track_the_litogramma_repo() {
    std_copies_track("litogramma", 11);
}

/// The same guard for the lexigramma repo (the dictionary
/// dialektos and its exos ship in std the same way).
#[test]
fn std_copies_track_the_lexigramma_repo() {
    std_copies_track("lexigramma", 3);
}

fn std_copies_track(name: &str, at_least: usize) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("..")
        .join(name);
    if !repo.is_dir() {
        return;
    }
    let std_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("std");
    let mut shared = 0;
    for entry in std::fs::read_dir(&std_dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let theirs = repo.join(&name);
        if !theirs.is_file() {
            continue;
        }
        shared += 1;
        assert!(
            std::fs::read(&path).unwrap() == std::fs::read(&theirs).unwrap(),
            "std/{name} has drifted from the {} repo — run \
             scripts/vendor-litogramma.sh pull (or push) {}",
            repo.display(),
            repo.display()
        );
    }
    assert!(
        shared >= at_least,
        "the vendored set shrank: {shared} shared files"
    );
}

/// The three export homs are total over litogramma and carry
/// koine's inlines: the visible-URL link passes through to every
/// target (all three speak it); the inline quotation lands on
/// at-html's classed span, at-docbook's quote, and at-tei's q.
/// Guards the seam the vendoring check cannot see: a koine sim
/// added after a hom was written.
#[test]
fn litogramma_homs_are_total_and_carry_koine_inlines() {
    let tmp = tmp_dir("litogramma-homs");
    let atd = "@@@!litogramma\n\
               \n\
               @=Title=@\n\
               \n\
               @#(1) One\n\
               A @\"\"word\"\"@ and @><https://example.org/x><@ here.\n\
               #@\n";
    std::fs::write(tmp.join("doc.atd"), atd).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    for (target, quotation) in [
        ("at-html", "@,word,@.quotation"),
        ("at-docbook", "@\"\"word\"\"@"),
        ("at-tei", "@,word,@.q"),
    ] {
        let m = atrep::morph::resolve_morph(&tmp, "litogramma", target).unwrap();
        let out = atrep::morph::apply(&kanon.document, &m).unwrap();
        let s = atrep::dendron::serialize(&out);
        assert!(s.contains("@><https://example.org/x><@"), "{target}: {s}");
        assert!(s.contains(quotation), "{target}: {s}");
    }
}
