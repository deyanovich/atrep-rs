//! Review follow-up regressions for the scripture and JATS
//! importers (endo.rs: JATS, USFM, usfm_apply_scheme, USX,
//! OSIS).

use std::path::{Path, PathBuf};

use atrep::dendron::Document;
use atrep::{dendron, endo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn kanon_of(doc: &Document, tmp: &Path) -> String {
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(doc)).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document;
    dendron::serialize(&kanon)
}

const SCHEME_USFM: &str = "\\id PSA\n\\c 23\n\\p\n\\v 1 The LORD is my shepherd; I shall not want.\n\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved the world.\n";

const SCHEME_OSIS: &str = r#"<?xml version="1.0"?>
<osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace">
  <osisText osisIDWork="kjv">
    <div type="book" osisID="Ps">
      <chapter osisID="Ps.23" n="23"/>
      <p><verse osisID="Ps.23.1" n="1"/>The LORD is my shepherd; I shall not want.</p>
    </div>
    <div type="book" osisID="John">
      <chapter osisID="John.3" n="3"/>
      <p><verse osisID="John.3.16" n="16"/>For God so loved the world.</p>
    </div>
  </osisText>
</osis>"#;

/// The scheme coordinate's book segment is the canonical USFM
/// code whatever the source spelled: OSIS `Ps`/`John` and USFM
/// `PSA`/`JHN` land on the same kanon.
#[test]
fn scheme_coordinates_agree_across_osis_and_usfm() {
    let tmp = tmp_dir("review-endo-c-scheme");
    let mut from_usfm = endo::usfm_to_document(SCHEME_USFM).unwrap();
    endo::usfm_apply_scheme(&mut from_usfm, "protestant");
    let mut from_osis = endo::osis_to_document(SCHEME_OSIS).unwrap();
    endo::usfm_apply_scheme(&mut from_osis, "protestant");
    let usfm_atd = dendron::serialize(&from_usfm);
    let osis_atd = dendron::serialize(&from_osis);
    for atd in [&usfm_atd, &osis_atd] {
        assert!(atd.contains("@(\"protestant:psa.23\")"), "{atd}");
        assert!(atd.contains("@(\"protestant:psa.23.1\")"), "{atd}");
        assert!(atd.contains("@(\"protestant:jhn.3.16\")"), "{atd}");
        assert!(!atd.contains("protestant:ps."), "{atd}");
        assert!(!atd.contains("protestant:john."), "{atd}");
    }
    let usfm_kanon = kanon_of(&from_usfm, &tmp);
    let osis_kanon = kanon_of(&from_osis, &tmp);
    assert_eq!(usfm_kanon, osis_kanon);
    assert!(usfm_kanon.contains("@# psa\n@(\"protestant:psa.23\")"));
}

/// A verse before any chapter, or a `\c` with no number, never
/// yields a coordinate with empty segments: the bare `\c` is an
/// import error, the chapterless verse keeps its plain monosim.
#[test]
fn scheme_never_builds_malformed_coordinates() {
    let err = endo::usfm_to_document(
        "\\id OBA\n\\p\n\\v 1 The vision of Obadiah.\n\\c\n\\p\n\\v 2 Behold.\n",
    )
    .unwrap_err();
    assert!(err.to_string().contains("\\c"), "{err}");

    let mut doc = endo::usfm_to_document(
        "\\id OBA\n\\p\n\\v 1 The vision of Obadiah.\n\\c 1\n\\p\n\\v 2 Behold.\n",
    )
    .unwrap();
    endo::usfm_apply_scheme(&mut doc, "protestant");
    let atd = dendron::serialize(&doc);
    assert!(!atd.contains(".."), "{atd}");
    assert!(atd.contains("@|(1) The vision of Obadiah."), "{atd}");
    assert!(atd.contains("@(\"protestant:oba.1\")"), "{atd}");
    assert!(atd.contains("@(\"protestant:oba.1.2\") Behold."), "{atd}");
}

const SECTIONS_OSIS: &str = r#"<?xml version="1.0"?>
<osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace">
  <osisText osisIDWork="test">
    <div type="book" osisID="Gen">
      <chapter osisID="Gen.1" n="1"/>
      <div type="section">
        <title>The Creation</title>
        <p><verse osisID="Gen.1.1" n="1"/>In the beginning God created the heaven and the earth.</p>
      </div>
      <div type="section">
        <title>The First Day</title>
        <p><verse osisID="Gen.1.3" n="3"/>And God said, Let there be light.</p>
      </div>
    </div>
  </osisText>
</osis>"#;

const SECTIONS2_OSIS: &str = r#"<?xml version="1.0"?>
<osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace">
  <osisText osisIDWork="test">
    <div type="book" osisID="Gen">
      <chapter osisID="Gen.1" n="1"/>
      <div type="section">
        <p><verse osisID="Gen.1.1" n="1"/>In the beginning God created the heaven and the earth.</p>
      </div>
      <div type="section">
        <title>The First Day</title>
      </div>
    </div>
  </osisText>
</osis>"#;

/// A section div's closer does not close the book: content
/// after the first section stays inside it.
#[test]
fn osis_section_divs_stay_inside_the_book() {
    let doc = endo::osis_to_document(SECTIONS_OSIS).unwrap();
    assert_eq!(doc.blocks.len(), 1);
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@_The Creation_@.s1"), "{atd}");
    assert!(atd.contains("@_The First Day_@.s1"), "{atd}");
    assert!(
        atd.contains("@|(3) And God said, Let there be light."),
        "{atd}"
    );

    let doc = endo::osis_to_document(SECTIONS2_OSIS).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@_The First Day_@.s1"), "{atd}");
}

const ABSTRACT_JATS: &str = r#"<article xmlns:xlink="http://www.w3.org/1999/xlink">
<front><article-meta>
<title-group><article-title>Solitude</article-title></title-group>
<abstract><title>Abstract</title><p>The program.<fn><p>Abstract note.</p></fn></p></abstract>
</article-meta></front>
<body>
<sec><title>Claim</title>
<p>The ode holds.<fn><p>Body note.</p></fn> And more.<fn><p>Second body note.</p></fn></p>
</sec>
</body>
</article>"#;

/// An abstract's own title label does not leak the sec-title
/// sentinel, and notes in the abstract and body share one
/// counter, so every callout binds its own body.
#[test]
fn jats_abstract_title_and_notes() {
    let tmp = tmp_dir("review-endo-c-jats");
    let doc = endo::jats_to_document(ABSTRACT_JATS).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(!atd.contains('\u{0}'), "{atd}");
    assert!(!atd.contains("eltit"), "{atd}");
    assert!(atd.contains("The program.@^(n1)"), "{atd}");
    assert!(
        atd.contains("The ode holds.@^(n2) And more.@^(n3)"),
        "{atd}"
    );
    let kanon = kanon_of(&doc, &tmp);
    assert!(kanon.contains("The program.@^(o1)"), "{kanon}");
    assert!(
        kanon.contains("The ode holds.@^(o2) And more.@^(o3)"),
        "{kanon}"
    );
    assert!(kanon.contains("^@(o3)"), "{kanon}");

    // A title anywhere else outside a sec head is a clear error.
    let err = endo::jats_to_document(
        r#"<article><body><sec><title>Claim</title><disp-quote><title>Motto</title><p>Thus.</p></disp-quote></sec></body></article>"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("<title>"), "{err}");
}

const MIXED_JATS: &str = r#"<article xmlns:xlink="http://www.w3.org/1999/xlink">
<front><article-meta>
<title-group><article-title>Solitude</article-title></title-group>
</article-meta></front>
<body>
<sec><title>Claim</title>
<p>The ode <xref ref-type="bibr" rid="pope1700">[1]</xref> holds.</p>
</sec>
</body>
<back><ref-list>
<ref id="pope1700"><mixed-citation publication-type="journal"><string-name>Pope, Alexander</string-name>. Ode on Solitude. <source>Juvenilia</source> 1700.</mixed-citation></ref>
</ref-list></back>
</article>"#;

/// A mixed-citation flattens its child elements into the note
/// field instead of rejecting them.
#[test]
fn jats_mixed_citation_flattens_to_text() {
    let doc = endo::jats_to_document(MIXED_JATS).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@& pope1700"), "{atd}");
    assert!(
        atd.contains("@: note\nPope, Alexander. Ode on Solitude. Juvenilia 1700.\n:@"),
        "{atd}"
    );
}

/// USX poetry styles the USFM importer accepts (qr, qc, qm*)
/// import as poetry lines, and an open/close `<para style="b">`
/// pair is the strophe break.
#[test]
fn usx_poetry_styles_and_strophe_break() {
    let usx = r#"<usx version="3.0">
<book code="PSA" style="id"/>
<chapter number="3" style="c"/>
<para style="q1"><verse number="1" style="v"/>The earth is Yahweh's.</para>
<para style="qr">Selah</para>
<para style="b"></para>
<para style="qc">Centered.</para>
<para style="qm2">Embedded.</para>
<para style="qm">Embedded first.</para>
</usx>"#;
    let doc = endo::usx_to_document(usx).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@,@|(1) The earth is Yahweh's.,@.q1\n@,Selah,@.qr\n\n@,Centered.,@.qc\n@,Embedded.,@.qm2\n@,Embedded first.,@.qm1\n~@"), "{atd}");
    assert_eq!(atd.matches("@~").count(), 1);

    let err = endo::usx_to_document(
        r#"<usx version="3.0"><book code="PSA" style="id"/><para style="b">text</para></usx>"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("style=\"b\""), "{err}");
}
