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

/// Drift guard: the std copies are vendored from the normative
/// litogramma repo. When the sibling checkout is present, the
/// copies must be byte-identical; absent (standalone checkout),
/// the guard is skipped.
#[test]
fn std_copies_track_the_litogramma_repo() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("../litogramma");
    if !repo.is_dir() {
        return;
    }
    let std_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("std");
    for f in [
        "litogramma.dia",
        "bibliogramma.dia",
        "litogramma.html.exo",
        "litogramma.gemtext.exo",
        "litogramma.latex.exo",
        "bibliogramma.bib.exo",
        "bibliogramma.bibtex.exo",
        "litogramma.at-html.hom",
        "litogramma.at-docbook.hom",
        "litogramma.at-tei.hom",
        "at-docbook.litogramma.hom",
    ] {
        let ours = std::fs::read(std_dir.join(f)).unwrap();
        let theirs = std::fs::read(repo.join(f)).unwrap();
        assert!(
            ours == theirs,
            "std/{f} has drifted from the litogramma repo — re-vendor it"
        );
    }
}
