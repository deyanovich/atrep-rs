//! Definition-file kanonizo tests (`.dia` -> `.lektos`).

use std::path::{Path, PathBuf};

use atrep::kanonizo;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Fresh per-test scratch directory.
fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_fixture(dir: &Path, name: &str) {
    std::fs::copy(fixtures().join(name), dir.join(name)).unwrap();
}

/// Canonical form of `child.dia`: alias sigil rewritten, comments
/// gone, inheritance and imports expanded, blocks sorted by name
/// with the duplicate `zeta` tie-broken by symbol (`!` < `?`).
const CHILD_LEKTOS: &str = "@@@!atrep\n\
    \n\
    @=== alpha\n\
    @~ grammata ~@\n\
    @\"\"\n\
    Multi-line long description.\n\
    Second line.\n\
    \"\"@\n\
    ===@\n\
    \n\
    @=== aside\n\
    @< grammata <@\n\
    @\"an aside (bracket matching disabled)\"@\n\
    ===@\n\
    \n\
    @=== link\n\
    @&(param)\n\
    ===@\n\
    \n\
    @=== mid\n\
    @- grammata -@\n\
    @\"a middle sim\"@\n\
    ===@\n\
    \n\
    @=== sect\n\
    @#(taxis) [lemma]\n\
    grammata\n\
    #@ [hypograph]\n\
    ===@\n\
    \n\
    @=== zeta\n\
    @! grammata !@\n\
    @\"an exclamation\"@\n\
    ===@\n\
    \n\
    @=== zeta\n\
    @? grammata ?@\n\
    @\"an exclamation\"@\n\
    ===@\n";

#[test]
fn lektos_kanon_expands_and_sorts() {
    let tmp = tmp_dir("lektos-expand");
    copy_fixture(&tmp, "base.dia");
    copy_fixture(&tmp, "child.dia");
    let result = kanonizo::kanonizo_definition_file(&tmp.join("child.dia")).unwrap();
    assert_eq!(result.kanon, CHILD_LEKTOS);
    assert_eq!(result.dialektos.sims.len(), 7);
}

#[test]
fn lektos_kanon_idempotent() {
    let tmp = tmp_dir("lektos-idempotent");
    copy_fixture(&tmp, "base.dia");
    copy_fixture(&tmp, "child.dia");
    let first = kanonizo::kanonizo_definition_file(&tmp.join("child.dia")).unwrap();
    std::fs::write(tmp.join("child.lektos"), &first.kanon).unwrap();
    let second = kanonizo::kanonizo_definition_file(&tmp.join("child.lektos")).unwrap();
    assert_eq!(second.kanon, first.kanon);
}

#[test]
fn lektos_kanon_reorders_existing_fixture() {
    // The fixture is deliberately unsorted (section, figure,
    // caption, ref) to keep parser-tolerance coverage; its kanon
    // sorts by name.
    let result = kanonizo::kanonizo_definition_file(&fixtures().join("exempli.lektos")).unwrap();
    assert_eq!(
        result.kanon,
        "@@@!atrep\n\
         \n\
         @=== caption\n\
         @: grammata :@\n\
         @\"a figure caption\"@\n\
         ===@\n\
         \n\
         @=== figure\n\
         @=[(taxis)] lemma\n\
         grammata\n\
         =@\n\
         @\"a numbered figure\"@\n\
         ===@\n\
         \n\
         @=== ref\n\
         @^(param)\n\
         @\"reference to an anchor, resolving to its taxis\"@\n\
         ===@\n\
         \n\
         @=== section\n\
         @# lemma\n\
         grammata\n\
         #@\n\
         @\"a titled section\"@\n\
         ===@\n"
    );
}

#[test]
fn document_parses_against_emitted_lektos() {
    // A document kanonized against the canonically re-emitted
    // dialektos must produce the same kanon as against the
    // original definition file.
    let reference = kanonizo::kanonizo_file(&fixtures().join("intro.atd")).unwrap();

    let tmp = tmp_dir("lektos-document");
    let emitted = kanonizo::kanonizo_definition_file(&fixtures().join("exempli.lektos")).unwrap();
    std::fs::write(tmp.join("exempli.lektos"), &emitted.kanon).unwrap();
    for name in ["intro.atd", "pipeline.svg", "steps.txt"] {
        copy_fixture(&tmp, name);
    }
    let result = kanonizo::kanonizo_file(&tmp.join("intro.atd")).unwrap();
    assert_eq!(result.kanon, reference.kanon);
}

#[test]
fn check_any_routes_definitions_and_documents() {
    let checked = atrep::check_any(&fixtures().join("exempli.lektos")).unwrap();
    assert!(matches!(checked, atrep::Checked::Dialektos(_)));
    let checked = atrep::check_any(&fixtures().join("intro.atd")).unwrap();
    assert!(matches!(checked, atrep::Checked::Document(_)));
}

/// Resolution runs storage-free through a MemorySource: a
/// dialektos with lineage, a morphism, and a transitive route
/// all resolve with no filesystem.
#[test]
fn memory_source_resolution() {
    use atrep::source::MemorySource;

    let mut ctx = MemorySource::new();
    ctx.insert(
        "notula.lektos",
        "@@@!atrep\n\n@=== emphasis\n@% grammata %@\n===@\n",
    );
    ctx.insert(
        "notula.at-html.hom",
        "@@@!atrep-hom\n@=notula=>at-html\n\n@:: % /\n",
    );
    // A child defined by lineage from the in-memory parent.
    ctx.insert("childa.lektos", "@@@!atrep\n\n@@:: notula\n");

    let dial = atrep::dialektos::resolve_from(&ctx, "childa").unwrap();
    assert!(dial.sims.contains_key("%"));

    let m = atrep::morph::resolve_morph_from(&ctx, "notula", "at-html").unwrap();
    assert_eq!(
        (m.source.as_str(), m.target.as_str()),
        ("notula", "at-html")
    );

    // Transitive: notula => at-html => at-markdown, composing
    // the in-memory edge with the embedded std library.
    let route = atrep::morph::resolve_route_from(&ctx, "notula", "at-markdown").unwrap();
    let hops: Vec<(&str, &str)> = route
        .iter()
        .map(|m| (m.source.as_str(), m.target.as_str()))
        .collect();
    assert_eq!(hops, [("notula", "at-html"), ("at-html", "at-markdown")]);

    // The exo resolves through lineage from memory too.
    let x = atrep::exo::resolve_exo_from(&ctx, "at-markdown", "md").unwrap();
    assert_eq!(
        (x.source.as_str(), x.target.as_str()),
        ("at-markdown", "md")
    );
}
