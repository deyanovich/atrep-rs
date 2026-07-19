//! Error-condition tests (spec: Parsing chapter, "Error Handling").

use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::{kanonizo, parser};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Parse a source string as if it lived in the fixtures directory
/// (so the `exempli` dialektos resolves).
fn parse(source: &str) -> atrep::Result<atrep::Document> {
    parser::parse_document(source, &fixtures().join("virtual.atd"))
}

#[test]
fn missing_declaration() {
    let err = parse("just text\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::MissingDeclaration));
}

#[test]
fn unresolvable_dialektos() {
    let err = parse("@@@!no-such-dialect\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnresolvableDialektos(_)));
}

#[test]
fn atrep_declaration_in_document_is_error() {
    let err = parse("@@@!atrep\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::InvalidDeclaration(_)));
}

#[test]
fn undefined_sim() {
    let err = parse("@@@!exempli\n\ntext with @?bad?@ sim\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UndefinedSim(_)));
}

#[test]
fn sigil_run_too_long() {
    let err = parse("@@@!exempli\n\n@@@@@(five)\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::SigilRunTooLong(5)));
}

#[test]
fn monosim_whitespace() {
    let err = parse("@@@!exempli\n\nSee @^(a b) here.\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::MonosimWhitespace(_)));
}

#[test]
fn para_sim_in_endo_context() {
    let err = parse("@@@!exempli\n\ntext @# heading misuse\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::EndoMimicsPara));
}

#[test]
fn invalid_genos() {
    let err = parse("@@@!exempli\n\n@:x:@.bad- text\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::InvalidGenos(_)));
}

#[test]
fn unmatched_endo_sim() {
    let err = parse("@@@!exempli\n\n@:never closed\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnmatchedSim(_)));
}

#[test]
fn unmatched_para_sim() {
    let err = parse("@@@!exempli\n\n@# Title\nbody without episim\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnmatchedSim(_)));
}

#[test]
fn anaphor_in_endo_context() {
    let err = parse("@@@!exempli\n\ninline @@@(file.atd) include\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::AnaphorInEndoContext));
}

#[test]
fn escaped_sigil_is_text() {
    let doc = parse("@@@!exempli\n\nmail me at x\\@example.org today\n").unwrap();
    // One paragraph, pure text, with a literal @.
    let atrep::dendron::Block::Paragraph(inlines) = &doc.blocks[0] else {
        panic!("expected paragraph");
    };
    let atrep::dendron::Inline::Text(t) = &inlines[0] else {
        panic!("expected text");
    };
    assert!(t.contains("x@example.org"));
}

#[test]
fn axioma_reference_before_definition() {
    let tmp = tmp_dir("axioma-order");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\nUse @@(:sig:) here.\n\n@@:my signature:@@(sig)\n",
    )
    .unwrap();
    let err = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::AxiomaBeforeDefinition(_)));
}

#[test]
fn axioma_expansion() {
    let tmp = tmp_dir("axioma-expansion");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\n@@:The Atrep Project:@@(who)\n\nBy @@(:who:), for @@(:who:).\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(
        result.kanon,
        "@@@!exempli\n\nBy The Atrep Project, for The Atrep Project.\n"
    );
}

#[test]
fn enlexis_of_onymized_endo() {
    let tmp = tmp_dir("enlexis-endo");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\nA @:key term:@(kt) appears.\n\nCopy: @@+(:kt:)\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    // The copy is the simmere minus its own onym; the onym's only
    // reference was the expanded enlexis, so the live simmere drops
    // it as unreferenced.
    assert_eq!(
        result.kanon,
        "@@@!exempli\n\nA @:key term:@ appears.\n\nCopy: @@\"@:key term:@\"@@\n"
    );
}

#[test]
fn enlexis_of_onymized_para() {
    let tmp = tmp_dir("enlexis-para");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\n@=() Fig\nbody\n=@(fig)\n\nSee @^(fig).\n\n@@@+(:fig:)\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    // The live figure keeps its (canonicalized) onym; the verbatim
    // copy shows the pre-kanonizo source form without it.
    assert_eq!(
        result.kanon,
        "@@@!exempli\n\n@=(1) Fig\nbody\n=@(o1)\n\nSee @^(o1).\n\n\
         @@@\"\n@=() Fig\nbody\n=@\n\"@@@\n"
    );
}

#[test]
fn enlexis_forward_reference_to_onym_target() {
    let tmp = tmp_dir("enlexis-forward");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\nCopy: @@+(:kt:)\n\nA @:key term:@(kt) appears.\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(
        result.kanon,
        "@@@!exempli\n\nCopy: @@\"@:key term:@\"@@\n\nA @:key term:@ appears.\n"
    );
}

#[test]
fn enlexis_standalone_onym_is_error() {
    let tmp = tmp_dir("enlexis-standalone");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\nAnchor @(here) marks a spot.\n\n@@@+(:here:)\n",
    )
    .unwrap();
    let err = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::EnlexisStandaloneOnym(_)));
}

#[test]
fn enlexis_para_target_in_endo_context_is_error() {
    let tmp = tmp_dir("enlexis-para-in-endo");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\n@=() Fig\nbody\n=@(fig)\n\nInline @@+(:fig:) misuse.\n",
    )
    .unwrap();
    let err = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::EnlexisParaInEndoContext(_)));
}

#[test]
fn enlexis_undefined_ref_is_error() {
    let tmp = tmp_dir("enlexis-undefined");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\nNothing @@+(:nope:) here.\n",
    )
    .unwrap();
    let err = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::AxiomaBeforeDefinition(_)));
}

#[test]
fn taxis_autonumber_and_validation() {
    let tmp = tmp_dir("taxis");
    // Two autonumbered figures, then an explicit mismatch.
    std::fs::write(
        tmp.join("ok.atd"),
        "@@@!exempli\n\n@=() One\na\n=@\n\n@=() Two\nb\n=@\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("ok.atd")).unwrap();
    assert!(result.kanon.contains("@=(1) One"));
    assert!(result.kanon.contains("@=(2) Two"));

    std::fs::write(
        tmp.join("bad.atd"),
        "@@@!exempli\n\n@=(1) One\na\n=@\n\n@=(3) Two\nb\n=@\n",
    )
    .unwrap();
    let err = kanonizo::kanonizo_file(&tmp.join("bad.atd")).unwrap_err();
    assert!(matches!(
        err.kind,
        ErrorKind::TaxisInconsistent {
            expected: 2,
            found: 3
        }
    ));
}

#[test]
fn transclusion_cycle() {
    let tmp = tmp_dir("cycle");
    std::fs::write(tmp.join("a.atd"), "@@@!exempli\n\n@@@(b.atd)\n").unwrap();
    std::fs::write(tmp.join("b.atd"), "@@@!exempli\n\n@@@(a.atd)\n").unwrap();
    let err = kanonizo::kanonizo_file(&tmp.join("a.atd")).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::TransclusionCycle(_)));
}

#[test]
fn unreferenced_diaphane_unwrapped() {
    let tmp = tmp_dir("diaphane");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\n@@@.\nwrapped paragraph\n.@@@\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(result.kanon, "@@@!exempli\n\nwrapped paragraph\n");
}

#[test]
fn genos_bearing_diaphane_kept() {
    let tmp = tmp_dir("diaphane-genos");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\n@@@.\nwrapped paragraph\n.@@@.aside\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(
        result.kanon,
        "@@@!exempli\n\n@@@.\nwrapped paragraph\n.@@@.aside\n"
    );
}

/// Fresh per-test scratch directory with the `exempli` dialektos.
fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        fixtures().join("exempli.lektos"),
        dir.join("exempli.lektos"),
    )
    .unwrap();
    dir
}
