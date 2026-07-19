//! The at-rst syntax mapper (both directions), the extract and
//! genos-rename morphism primitives, and transitive route
//! resolution across the four-node graph.

use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::{dendron, endo, exo, kanonizo, morph};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SAMPLE_RST: &str = "\
Solitude
========

Text with *emphasis*, **strength**, and ``x < y`` beside a
claim [#fn]_ of note.

.. note::

   Mind the gap.

term
   the definition of the term

:Author: Alexander Pope

- first point
- second point

1. one
2. two

::

   if a && b { run() }

.. [#fn] The supporting footnote.

.. image:: lyre.svg
";

fn kanon_from_rst(tmp: &Path, rst: &str) -> kanonizo::KanonResult {
    let doc = endo::rst_to_document(rst).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    // The image needs to exist for kanonizo media processing.
    std::fs::write(tmp.join("lyre.svg"), "<svg/>").unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap()
}

/// Import, serialize, check the exact at-rst .atd source.
#[test]
fn rst_endo_produces_canonical_atd() {
    let doc = endo::rst_to_document(SAMPLE_RST).unwrap();
    assert_eq!(
        dendron::serialize(&doc),
        "@@@!at-rst\n\
         \n\
         @#Solitude#@\n\
         \n\
         Text with @*emphasis*@, @**strength**@, and @@\"x < y\"@@ beside a claim @^(fn) of note.\n\
         \n\
         @!\n\
         Mind the gap.\n\
         !@\n\
         \n\
         @:: term\n\
         the definition of the term\n\
         ::@\n\
         \n\
         @: Author\n\
         Alexander Pope\n\
         :@\n\
         \n\
         @-\n\
         first point\n\
         -@\n\
         \n\
         @-\n\
         second point\n\
         -@\n\
         \n\
         @.(1)\n\
         one\n\
         .@\n\
         \n\
         @.(2)\n\
         two\n\
         .@\n\
         \n\
         @@@\"\n\
         if a && b { run() }\n\
         \"@@@\n\
         \n\
         @^\n\
         The supporting footnote.\n\
         ^@(fn)\n\
         @@@@(lyre.svg)\n"
    );
}

/// RST -> at-rst -> kanon -> RST is a fixed point on the
/// canonical subset.
#[test]
fn rst_roundtrip_is_idempotent() {
    let tmp = tmp_dir("rst-roundtrip");
    let cycle = |rst: &str| -> String {
        let kanon = kanon_from_rst(&tmp, rst);
        let x = exo::resolve_exo(&tmp, "at-rst", "rst").unwrap();
        exo::render(&kanon.document, &x, &tmp).unwrap()
    };
    let rst1 = cycle(SAMPLE_RST);
    // The exported image path is the canonical media/m1.svg; make
    // it resolvable for the second cycle's kanonizo.
    std::fs::create_dir_all(tmp.join("media")).unwrap();
    std::fs::write(tmp.join("media/m1.svg"), "<svg/>").unwrap();
    let rst2 = cycle(&rst1);
    assert_eq!(rst1, rst2);
    // Footnote machinery survives: canonical onym both ways.
    assert!(rst1.contains("[#o1]_"));
    assert!(rst1.contains(".. [#o1] The supporting footnote."));
}

/// at-rst => at-html: extract turns the definition term into a
/// strong heading-line, admonitions become divisions with their
/// kind as a genos, fields drop, footnote bodies dissolve and
/// their deixes go with them.
#[test]
fn rst_to_html_hom() {
    let tmp = tmp_dir("rst-to-html");
    let kanon = kanon_from_rst(&tmp, SAMPLE_RST);
    let m = morph::resolve_morph(&tmp, "at-rst", "at-html").unwrap();
    let out = morph::apply(&kanon.document, &m).unwrap();
    let s = dendron::serialize(&out);
    assert!(s.starts_with("@@@!at-html\n"));
    // Renames along the hom.
    assert!(s.contains("@/emphasis/@") && s.contains("@!strength!@"));
    // Admonition -> division with kind genos.
    assert!(s.contains("@_\nMind the gap.\n_@.note"));
    // Extract: term becomes a strong solo line, definition follows.
    assert!(s.contains("@!term!@\n\nthe definition of the term\n"));
    // Field dropped; footnote body dissolved; deixis gone.
    assert!(!s.contains("Author"));
    assert!(!s.contains("@^("));
    assert!(s.contains("The supporting footnote."));
    // The result renders with at-html's exo.
    let x = exo::resolve_exo(&tmp, "at-html", "html").unwrap();
    let html = exo::render(&out, &x, &tmp).unwrap();
    assert!(html.contains("<div class=\"note\">"));
}

/// The transitive gate: at-rst => at-markdown has no direct
/// morphism; the route composes through at-html.
#[test]
fn transitive_route_rst_to_markdown() {
    let tmp = tmp_dir("rst-transitive");
    let rst = "Title\n=====\n\nWith *em* and **strong**.\n\n- point\n";
    let kanon = kanon_from_rst(&tmp, rst);

    // No direct morphism exists.
    let err = morph::resolve_morph(&tmp, "at-rst", "at-markdown").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnresolvableMorph(_)));

    // The route resolves with two hops via at-html.
    let route = morph::resolve_route(&tmp, "at-rst", "at-markdown").unwrap();
    let hops: Vec<(&str, &str)> = route
        .iter()
        .map(|m| (m.source.as_str(), m.target.as_str()))
        .collect();
    assert_eq!(hops, [("at-rst", "at-html"), ("at-html", "at-markdown")]);
    let out = morph::apply_route(&kanon.document, &route).unwrap();
    assert_eq!(
        dendron::serialize(&out),
        "@@@!at-markdown\n\
         \n\
         @#Title#@\n\
         \n\
         With @*em*@ and @**strong**@.\n\
         \n\
         @-\n\
         point\n\
         -@\n"
    );
    // And onward to Markdown text via at-markdown's exo.
    let x = exo::resolve_exo(&tmp, "at-markdown", "md").unwrap();
    let md = exo::render(&out, &x, &tmp).unwrap();
    assert!(md.contains("# Title"));
    assert!(md.contains("*em* and **strong**"));
}

/// The reverse composes too: at-markdown => at-rst via the
/// derived embedding into at-html, then the explicit hom.
#[test]
fn transitive_route_markdown_to_rst() {
    let tmp = tmp_dir("md-to-rst");
    let doc = endo::markdown_to_document("# Title\n\nJust *style*.\n").unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();

    let route = morph::resolve_route(&tmp, "at-markdown", "at-rst").unwrap();
    assert_eq!(route.len(), 2);
    let out = morph::apply_route(&kanon.document, &route).unwrap();
    let x = exo::resolve_exo(&tmp, "at-rst", "rst").unwrap();
    let rst = exo::render(&out, &x, &tmp).unwrap();
    assert!(rst.starts_with("Title\n========"));
    assert!(rst.contains("Just *style*."));
}

/// Two distinct shortest routes are an error, never a silent
/// choice.
#[test]
fn ambiguous_route_is_error() {
    let tmp = tmp_dir("route-ambiguous");
    for id in ["aa", "bb", "cc", "dd"] {
        std::fs::write(
            tmp.join(format!("{id}.lektos")),
            "@@@!atrep\n\n@=== emphasis\n@/ grammata /@\n===@\n",
        )
        .unwrap();
    }
    for (a, b) in [("aa", "bb"), ("bb", "dd"), ("aa", "cc"), ("cc", "dd")] {
        std::fs::write(
            tmp.join(format!("{a}.{b}.hom")),
            format!("@@@!atrep-hom\n@={a}=>{b}\n"),
        )
        .unwrap();
    }
    let err = morph::resolve_route(&tmp, "aa", "dd").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::AmbiguousMorphRoute(_)));
}

/// The xhtml exo is a second target for the same dialektos.
#[test]
fn xhtml_is_a_second_export_target() {
    let tmp = tmp_dir("xhtml-target");
    std::fs::write(tmp.join("doc.atd"), "@@@!at-html\n\nJust @/text/@.\n").unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "at-html", "xhtml").unwrap();
    let xhtml = exo::render(&kanon.document, &x, &tmp).unwrap();
    assert!(xhtml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xhtml.contains("<html xmlns=\"http://www.w3.org/1999/xhtml\">"));
    assert!(xhtml.contains("<em>text</em>"));
}
