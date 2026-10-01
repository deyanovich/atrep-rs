//! The at-usfm scripture mapper: import golden, canonical
//! fixed point on the USFM surface, and the USX and OSIS
//! export surfaces of the same dialektos.

use std::path::{Path, PathBuf};

use atrep::{dendron, endo, exo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SAMPLE_USFM: &str = r#"\id JHN 43-JHN-web.sfm World English Bible
\ide UTF-8
\h John
\mt1 The Good News According to John
\c 3
\s1 Jesus and Nicodemus
\p
\v 1 Now there was a man of the Pharisees named Nicodemus, a ruler of the Jews.
\v 2 He came to Jesus by night.
\p
\v 16 \wj For God so loved the world, that he gave his only born\f + \fr 3:16 \ft "Only born" renders the Greek "monogenes".\f* Son.\wj*
\v 17 \wj God did not send his Son to judge the world.\wj*\x - \xo 3:17 \xt John 12:47\x*
\s1 From the Psalms
\q1 The earth is Yahweh's, with its fullness;
\q2 the world, and those who dwell in it.
\b
\q1 For he has founded it on the seas,
\q2 and established it on the floods.
"#;

fn kanon_of(usfm: &str, tmp: &Path) -> atrep::dendron::Document {
    let doc = endo::usfm_to_document(usfm).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document
}

#[test]
fn usfm_endo_produces_canonical_atd() {
    let doc = endo::usfm_to_document(SAMPLE_USFM).unwrap();
    assert_eq!(doc.dialect_id, "at-usfm");
    let atd = dendron::serialize(&doc);
    // The book code is the vocabulary lemma, lowercased.
    assert!(atd.starts_with("@@@!at-usfm\n\n@# jhn\n"));
    // Header lines are typed solo blocks; chapters and verses
    // are milestones.
    assert!(atd.contains("@_John_@.h"));
    assert!(atd.contains("@_The Good News According to John_@.mt1"));
    assert!(atd.contains("@##(3)"));
    assert!(atd.contains("@|(1) Now there was a man"));
    // Words of Jesus wrap the verse text, with the footnote
    // inline at its anchor and its reference as an .fr phrase.
    // The red letters carry their constant speaker as an unseen
    // prosopon (at-aphanes).
    assert!(atd.contains("@,@?:(Jesus)For God so loved the world"));
    assert!(atd.contains("@^@,3:16,@.fr \"Only born\" renders the Greek \"monogenes\".^@.f"));
    assert!(atd.contains("@^@,3:17,@.xo John 12:47^@.x"));
    // Poetry: per-line q-level phrases, \b splits strophes.
    assert!(atd.contains("@~\n@,The earth is Yahweh's, with its fullness;,@.q1"));
    assert!(atd.contains(",@.q2\n\n@,For he has founded it on the seas,,@.q1"));
    // Metadata markers are skipped.
    assert!(!atd.contains("UTF-8"));
    assert!(!atd.contains("43-JHN"));
}

/// USFM -> at-usfm -> kanon -> USFM is a fixed point from the
/// first canonical output onward.
#[test]
fn usfm_roundtrip_is_idempotent() {
    let tmp = tmp_dir("usfm-roundtrip");
    let cycle = |usfm: &str| -> String {
        let kanon = kanon_of(usfm, &tmp);
        let x = exo::resolve_exo(&tmp, "at-usfm", "usfm").unwrap();
        exo::render(&kanon, &x, &tmp).unwrap()
    };
    let usfm1 = cycle(SAMPLE_USFM);
    let usfm2 = cycle(&usfm1);
    assert_eq!(usfm1, usfm2);
    // Canonical shape: verses share the paragraph line, the
    // note keeps its \fr ... \ft structure, references theirs.
    assert!(usfm1.contains("\\p \\v 1 Now there was a man"));
    assert!(usfm1.contains("\\f + \\fr 3:16 \\ft \"Only born\""));
    assert!(usfm1.contains("\\x - \\xo 3:17 \\xt John 12:47\\x*"));
    assert!(usfm1.contains("\\q1 The earth is Yahweh's"));
}

#[test]
fn usx_export_renders_milestones_and_notes() {
    let tmp = tmp_dir("usfm-usx");
    let kanon = kanon_of(SAMPLE_USFM, &tmp);
    let x = exo::resolve_exo(&tmp, "at-usfm", "usx").unwrap();
    let usx = exo::render(&kanon, &x, &tmp).unwrap();
    assert!(usx.contains(r#"<book code="jhn" style="id"/>"#));
    assert!(usx.contains(r#"<chapter number="3" style="c"/>"#));
    assert!(usx.contains(r#"<verse number="16" style="v"/>"#));
    assert!(usx.contains(r#"<char style="wj">For God so loved"#));
    assert!(usx.contains(r#"<note style="f" caller="+"><char style="fr">3:16</char>"#));
    assert!(usx.contains(r#"<note style="x" caller="-"><char style="xo">3:17</char>"#));
    assert!(usx.contains(r#"<para style="q2">the world, and those who dwell in it.</para>"#));
}

#[test]
fn osis_export_renders_scripture_semantics() {
    let tmp = tmp_dir("usfm-osis");
    let kanon = kanon_of(SAMPLE_USFM, &tmp);
    let x = exo::resolve_exo(&tmp, "at-usfm", "osis").unwrap();
    let osis = exo::render(&kanon, &x, &tmp).unwrap();
    assert!(osis.contains(r#"<div type="book" osisID="jhn">"#));
    assert!(osis.contains(r#"<chapter n="3"/>"#));
    // Words of Jesus and the divine name get semantic OSIS
    // elements, not styling.
    assert!(osis.contains(r#"<q who="Jesus" marker="">For God so loved"#));
    assert!(osis.contains(r#"<note type="crossReference"><reference>3:17</reference>"#));
    assert!(osis.contains(r#"<l level="1">The earth is Yahweh's"#));
    // Running head and toc lines have no OSIS counterpart.
    assert!(!osis.contains(">John</"));
}

/// Book-name aliases canonicalize to the USFM code, so a
/// hand-written deltos can say `@# john` and hash identically.
#[test]
fn book_vocabulary_canonicalizes_aliases() {
    let tmp = tmp_dir("usfm-vocab");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!at-usfm\n\n@# John\n@##(3) @|(16) For God so loved the world.\n#@\n",
    )
    .unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let atk = dendron::serialize(&kanon.document);
    assert!(atk.contains("@# jhn\n"));
}

#[test]
fn usfm_endo_is_strict_about_unknown_markers() {
    let err = endo::usfm_to_document("\\id GEN\n\\zz custom\n").unwrap_err();
    assert!(err.to_string().contains("\\zz"));
    let err = endo::usfm_to_document("\\p no book\n").unwrap_err();
    assert!(err.to_string().contains("missing \\id"));
    // Unterminated character markers auto-close at line end (WEB
    // corpus round), so \nd without \nd* is tolerated, not an error.
    endo::usfm_to_document("\\id GEN\n\\p \\nd Yahweh\n").unwrap();
}

/// Real-corpus patterns (World English Bible): wordlist
/// wrappers unwrap, nested \+ markers, footnote labels, verse
/// lines under a bare \q marker, and text ending in a phrase
/// episymbol directly before a note.
const SAMPLE_PSALM: &str = r#"\id PSA
\c 23
\d A Psalm by \w David|strong="H1732"\w*.
\q1
\v 1 \w Yahweh|strong="H3068"\w* is my shepherd,\f + \fr 23:1 \fl Hebrew \ft \+wh Sheol\+wh* appears later.\f* I lack nothing.
\q2 He leads me beside still waters,\x - \xo 23:2 \xt John 10:11\x*
\m and restores my soul.
"#;

#[test]
fn corpus_patterns_roundtrip() {
    let tmp = tmp_dir("usfm-corpus-patterns");
    let doc = endo::usfm_to_document(SAMPLE_PSALM).unwrap();
    let atd = dendron::serialize(&doc);
    // Wordlist wrappers and their attributes are gone; the text
    // stays plain (no .w phrases).
    assert!(atd.contains("A Psalm by David."));
    assert!(atd.contains("@|(1) Yahweh is my shepherd,"));
    assert!(!atd.contains(".w"));
    assert!(!atd.contains("strong="));
    // The bare \q1 line owns the verse that follows it.
    assert!(atd.contains("@,@|(1) Yahweh"));
    // \fl flattens into note text; nested \+wh unwraps.
    assert!(atd.contains("Hebrew Sheol appears later."));
    // Text ending in the phrase episymbol directly before the
    // note gets the ambiguity separator, and the whole document
    // reparses to the same kanon.
    assert!(atd.contains("waters,|@^"));
    let kanon = kanon_of(SAMPLE_PSALM, &tmp);
    let x = exo::resolve_exo(&tmp, "at-usfm", "usfm").unwrap();
    let out1 = exo::render(&kanon, &x, &tmp).unwrap();
    let doc2 = endo::usfm_to_document(&out1).unwrap();
    std::fs::write(tmp.join("doc2.atd"), dendron::serialize(&doc2)).unwrap();
    let kanon2 = kanonizo::kanonizo_file(&tmp.join("doc2.atd"))
        .unwrap()
        .document;
    let out2 = exo::render(&kanon2, &x, &tmp).unwrap();
    assert_eq!(out1, out2);
    // The continuation paragraph is a typed \m paragraph.
    assert!(out1.contains("\\m and restores my soul."));
    assert!(out1.contains("\\q1 \\v 1 Yahweh is my shepherd,"));
}

/// Deuterocanonical codes are in the books vocabulary.
#[test]
fn deuterocanon_codes_resolve() {
    let doc =
        endo::usfm_to_document("\\id TOB\n\\c 1\n\\p\n\\v 1 The book of the words of Tobit.\n")
            .unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@# tob\n"));
}

/// USX 3 import: the same scripture through the XML surface
/// lands on the identical kanon (and litos) as the USFM
/// surface.
#[test]
fn usx_import_matches_usfm() {
    let tmp = tmp_dir("usx-import");
    let usfm_kanon = kanon_of(SAMPLE_USFM, &tmp);
    let x = exo::resolve_exo(&tmp, "at-usfm", "usx").unwrap();
    let usx = exo::render(&usfm_kanon, &x, &tmp).unwrap();
    let doc = endo::usx_to_document(&usx).unwrap();
    assert_eq!(doc.dialect_id, "at-usfm");
    std::fs::write(tmp.join("u.atd"), dendron::serialize(&doc)).unwrap();
    let usx_kanon = kanonizo::kanonizo_file(&tmp.join("u.atd"))
        .unwrap()
        .document;
    assert_eq!(
        dendron::serialize(&usfm_kanon),
        dendron::serialize(&usx_kanon)
    );
}

/// Paratext-style USX attributes (sid/eid pairs, vid) and
/// unknown-to-us end milestones are tolerated.
#[test]
fn usx_import_accepts_milestone_attributes() {
    let usx = r#"<?xml version="1.0" encoding="UTF-8"?>
<usx version="3.0">
<book code="GEN" style="id">World English Bible</book>
<chapter number="1" style="c" sid="GEN 1"/>
<para style="p"><verse number="1" style="v" sid="GEN 1:1"/>In the beginning.<verse eid="GEN 1:1"/></para>
<chapter eid="GEN 1"/>
</usx>"#;
    let doc = endo::usx_to_document(usx).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@# gen\n"));
    assert!(atd.contains("@##(1)"));
    // The butted milestone gets its separator.
    assert!(atd.contains("@|(1) In the beginning."));
}

/// OSIS import in the wild (KJV shape): bookGroup wrappers,
/// milestone chapters/verses with sID/eID, transChange, per-
/// verse lg groups that merge into one stichoi, self-closing
/// l/lg artifacts, and red-letter q milestones (skipped).
#[test]
fn osis_import_kjv_patterns() {
    let osis = r#"<?xml version="1.0" encoding="utf-8"?>
<osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace">
  <osisText osisIDWork="Bible.en.kjv">
    <header><work osisWork="kjv"><title>KJV</title></work></header>
    <div type="bookGroup" canonical="true">
      <title>Old Testament</title>
      <div type="book" osisID="Ps" canonical="true">
        <title type="main" short="Psalms">The Book of Psalms</title>
        <chapter osisRef="Ps.23" sID="Ps.23.s1" n="23" />
        <title type="psalm" canonical="true">A Psalm of David.</title>
        <lg>
          <l level="1"><verse osisID="Ps.23.1" sID="Ps.23.1.s2" n="1" />The LORD
<transChange type="added">is
</transChange> my shepherd; I shall not want.<verse eID="Ps.23.1.s2" /></l>
        </lg>
        <lg>
          <l level="1"><verse osisID="Ps.23.2" sID="Ps.23.2.s3" n="2" />He maketh me to lie down.<verse eID="Ps.23.2.s3" /></l>
          <l level="1" />
        </lg>
        <lg />
        <q who="Jesus" sID="q1" marker="" />
        <chapter eID="Ps.23.s1" />
      </div>
    </div>
  </osisText>
</osis>"#;
    let doc = endo::osis_to_document(osis).unwrap();
    let atd = dendron::serialize(&doc);
    // The osisID canonicalizes through the vocabulary alias.
    let tmp = tmp_dir("osis-kjv");
    std::fs::write(tmp.join("k.atd"), &atd).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("k.atd"))
        .unwrap()
        .document;
    let atk = dendron::serialize(&kanon);
    assert!(atk.contains("@# psa\n"));
    assert!(atk.contains("@##(23)"));
    assert!(atk.contains("@_A Psalm of David._@.d"));
    // Newline-wrapped transChange collapses and trims; the
    // butted verse milestone gets its separator.
    assert!(atk.contains("@,@|(1) The LORD @,is,@.add my shepherd; I shall not want.,@.q1"));
    // The two per-verse lg groups merged into one stichoi as
    // two strophes.
    assert_eq!(atk.matches("@~").count(), 1);
    assert!(atk.contains(",@.q1\n\n@,@|(2)"));
    // The empty l, the empty lg, and the red-letter milestone
    // left nothing behind.
    assert!(!atk.contains("@,,@"));
}

/// Whole-Bible files: multiple books in one USFM or USX file
/// become multiple book blocks.
#[test]
fn multi_book_files() {
    let usfm = "\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning.\n\\id EXO\n\\c 1\n\\p\n\\v 1 Now these are the names.\n";
    let doc = endo::usfm_to_document(usfm).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@# gen\n"));
    assert!(atd.contains("@# exo\n"));

    let usx = r#"<usx version="3.0">
<book code="GEN" style="id"/>
<para style="p"><verse number="1" style="v"/>In the beginning.</para>
<book code="EXO" style="id"/>
<para style="p"><verse number="1" style="v"/>Now these are the names.</para>
</usx>"#;
    let doc = endo::usx_to_document(usx).unwrap();
    assert_eq!(doc.blocks.len(), 2);
}

/// The latex exo: semantic macro layer plus the atrep-bible
/// class delivered as an asset.
#[test]
fn latex_export_emits_semantic_layer_and_class_asset() {
    let tmp = tmp_dir("usfm-latex");
    let kanon = kanon_of(SAMPLE_USFM, &tmp);
    let x = exo::resolve_exo(&tmp, "at-usfm", "latex").unwrap();
    let (tex, aux) = exo::render_with_aux(&kanon, &x, &tmp).unwrap();
    assert!(tex.contains("\\documentclass{atrep-bible}"));
    assert!(tex.contains("\\atbook{jhn}"));
    assert!(tex.contains("\\atchapter{3}"));
    assert!(tex.contains("\\atmarker{s1}{Jesus and Nicodemus}"));
    assert!(tex.contains("\\atverse{16} \\atchar{wj}{For God so loved"));
    assert!(tex.contains("\\atnote{f}{\\atref{3:16} \"Only born\" renders"));
    assert!(tex.contains("\\atq{1}{The earth is Yahweh's, with its fullness;}"));
    // The class rides along as an unconditional asset.
    let (marker, filename, cls) = &aux[0];
    assert_eq!(marker, "*asset");
    assert_eq!(filename, "atrep-bible.cls");
    assert!(cls.contains("\\ProvidesClass{atrep-bible}"));
    assert!(cls.contains("\\LoadClass[10pt,twoside,twocolumn]{book}"));
    // Byte-verbatim: no sigil processing touched the class.
    assert!(cls.contains("\\csname at@m@#1\\endcsname"));
}

/// Asset sections are strict about their framing.
#[test]
fn asset_sections_are_strict() {
    let tmp = tmp_dir("asset-errors");
    let write = |name: &str, content: &str| {
        std::fs::write(tmp.join(name), content).unwrap();
    };
    write(
        "d.dia",
        "@@@!atrep\n\n@=== para\n@_\ngrammata\n_@\n@\"p\"@\n===@\n",
    );
    // Missing sentinel.
    write(
        "d.x.exo",
        "@@@!atrep-exo\n@=d=>x\n\n@=* a.cls END\nbody\n\n@-> *document\n@(grammata)\n>-@\n",
    );
    let err = exo::resolve_exo(&tmp, "d", "x").unwrap_err();
    assert!(err.to_string().contains("missing sentinel"));
    // Duplicate filename.
    write(
        "d.y.exo",
        "@@@!atrep-exo\n@=d=>y\n\n@=* a.cls E1\nbody\nE1\n@=* a.cls E2\nbody\nE2\n\n@-> *document\n@(grammata)\n>-@\n",
    );
    let err = exo::resolve_exo(&tmp, "d", "y").unwrap_err();
    assert!(err.to_string().contains("duplicate asset"));
    // Path separators rejected.
    write(
        "d.z.exo",
        "@@@!atrep-exo\n@=d=>z\n\n@=* ../a.cls E\nbody\nE\n\n@-> *document\n@(grammata)\n>-@\n",
    );
    let err = exo::resolve_exo(&tmp, "d", "z").unwrap_err();
    assert!(err.to_string().contains("path separators"));
}

/// A named exo variant overlays the base: the redletter
/// edition changes only the document rule and inherits every
/// other rule and the class asset.
#[test]
fn redletter_exo_variant_overlays_the_base() {
    let tmp = tmp_dir("usfm-redletter");
    let kanon = kanon_of(SAMPLE_USFM, &tmp);
    let x = exo::resolve_exo_variant(&tmp, "at-usfm", "latex", Some("redletter")).unwrap();
    let (tex, aux) = exo::render_with_aux(&kanon, &x, &tmp).unwrap();
    assert!(tex.contains("\\documentclass[redletter]{atrep-bible}"));
    // Inherited rules still render.
    assert!(tex.contains("\\atchar{wj}{For God so loved"));
    // The class asset inherits from the base exo.
    assert!(
        aux.iter()
            .any(|(m, f, _)| m == "*asset" && f == "atrep-bible.cls")
    );
    // An unknown variant is an error.
    let err = exo::resolve_exo_variant(&tmp, "at-usfm", "latex", Some("nope")).unwrap_err();
    assert!(err.to_string().contains("variant"));
}

/// OSIS verses in container form (no sID/eID) at block level
/// gather into a paragraph, each opening with its milestone.
#[test]
fn osis_container_verses_import() {
    let osis = r#"<?xml version="1.0"?>
<osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace">
  <osisText osisIDWork="test">
    <div type="book" osisID="John">
      <chapter osisID="John.18">
        <verse osisID="John.18.37">Pilate therefore said unto him, Art thou a king then?</verse>
        <verse osisID="John.18.38">Pilate saith unto him, <q who="Pilate">What is truth?</q></verse>
      </chapter>
    </div>
  </osisText>
</osis>"#;
    let doc = endo::osis_to_document(osis).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@|(37) Pilate therefore said unto him, Art thou a king then? @|(38) Pilate saith unto him, @,@?:(Pilate)What is truth?,@.said"),
        "{atd}"
    );
}
