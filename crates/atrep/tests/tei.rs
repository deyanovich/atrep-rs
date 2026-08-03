//! The TEI (P5 basic subset) endomorphosis into litogramma —
//! the stress test for litogramma's coverage of real literary
//! markup. Golden tests are definition-free (serialization
//! needs no dialektos); kanonizo-level verification lives with
//! the litogramma definition.

use atrep::{dendron, endo};

const SAMPLE_TEI: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<TEI xmlns="http://www.tei-c.org/ns/1.0">
  <teiHeader>
    <fileDesc>
      <titleStmt>
        <title>Odes and a Scene</title>
        <author>Alexander Pope</author>
        <editor>A. Careful Editor</editor>
      </titleStmt>
      <publicationStmt><p>Test specimen.</p></publicationStmt>
      <sourceDesc><p>Born digital.</p></sourceDesc>
    </fileDesc>
  </teiHeader>
  <text>
    <front>
      <div type="dedication"><p>Ignored front matter.</p></div>
    </front>
    <body>
      <div type="part" n="1">
        <head>Early Poems</head>
        <epigraph>
          <quote>Happy the man, whose wish and care...</quote>
          <bibl>Horace, Epode II</bibl>
        </epigraph>
        <div type="chapter" n="1">
          <head>Ode on Solitude</head>
          <p>Written when the poet was <emph>about twelve</emph>
          years old,<note place="foot">The dating is
          traditional.</note> in <foreign xml:lang="la">otium</foreign>.
          He later called it <q>a childish thing</q>, and
          <said>I meant every word</said>.</p>
          <lg>
            <head>The Ode</head>
            <lg>
              <l>Happy the man, whose wish and care</l>
              <l>A few paternal acres bound,</l>
            </lg>
            <lg>
              <l>Blest, who can unconcern'dly find</l>
              <l>Hours, days, and years slide soft away,</l>
            </lg>
          </lg>
          <div>
            <head>A Reading</head>
            <p>The <hi rend="italic">retirement ideal</hi> was a
            <hi rend="bold">commonplace</hi>.<lb/>It descends
            from Horace.</p>
            <cit>
              <quote>Beatus ille qui procul negotiis.</quote>
              <bibl>Horace, Epode II, 1</bibl>
            </cit>
          </div>
        </div>
      </div>
      <div type="part" n="2">
        <head>A Scene</head>
        <castList>
          <head>Persons</head>
          <castItem>Philosopher, a recluse</castItem>
        </castList>
        <p>See the <choice><abbr>fig.</abbr><expan>figure</expan></choice>
        of <persName>Pope</persName>'s lyre
        <ref target="#fig-lyre">above</ref>, the
        <ref target="https://example.org/pope">archive</ref>, and the
        <soCalled>retirement ideal</soCalled> as a
        <term>topos</term>.</p>
        <figure xml:id="fig-lyre">
          <graphic url="lyre.svg"/>
          <head>A lyre</head>
        </figure>
        <table>
          <head>Editions</head>
          <row><cell>Year</cell><cell>Printer</cell></row>
          <row><cell>1717</cell><cell>W. Bowyer</cell></row>
        </table>
        <list type="ordered">
          <item>the ode</item>
          <item>the essay</item>
        </list>
        <sp>
          <speaker>Philosopher</speaker>
          <p>What is solitude<note place="foot">A rhetorical
          question.</note> but society refined?</p>
          <stage>He gestures at the empty room.</stage>
          <p>I rest my case.</p>
        </sp>
        <stage>Curtain.</stage>
      </div>
    </body>
    <back>
      <div type="colophon"><p>Ignored back matter.</p></div>
    </back>
  </text>
</TEI>
"##;

#[test]
fn tei_endo_produces_litogramma() {
    let doc = endo::tei_to_document(SAMPLE_TEI).unwrap();
    assert_eq!(doc.dialect_id, "litogramma");
    let atd = dendron::serialize(&doc);

    // Front matter from the header.
    assert!(atd.starts_with("@@@!litogramma\n"));
    assert!(atd.contains("@=Odes and a Scene=@"));
    assert!(atd.contains("@=:Alexander Pope:=@"));
    assert!(atd.contains("@=;A. Careful Editor;=@"));

    // Structure: parts with taxis and lemma; nested chapter and
    // untyped section by depth.
    assert!(atd.contains("@== Early Poems\n"));
    assert!(atd.contains("@=== Ode on Solitude\n"));
    assert!(atd.contains("@# A Reading\n"));

    // Epigraph with attribution.
    assert!(atd.contains("@\"/\nHappy the man, whose wish and care...\n/\"@ Horace, Epode II"));

    // Verse: lemma, two strophes, exact lines.
    assert!(atd.contains(
        "@~ The Ode\nHappy the man, whose wish and care\nA few paternal acres bound,\n\n\
         Blest, who can unconcern'dly find\nHours, days, and years slide soft away,\n~@"
    ));

    // Footnotes: deixis callouts with bodies after the paragraph.
    assert!(atd.contains("years old,@^(n1) in"));
    assert!(atd.contains("@^\nThe dating is traditional.\n^@(n1)"));
    assert!(atd.contains("solitude@^(n2) but society refined?"));

    // Inline: emph and foreign (genos + language), hi variants,
    // lb as space, inline quotation and speech.
    assert!(atd.contains("@/about twelve/@"));
    assert!(atd.contains("@/otium/@.foreign.la"));
    assert!(atd.contains("@\"\"a childish thing\"\"@,"));
    assert!(atd.contains("@\"\"I meant every word\"\"@.said."));
    assert!(atd.contains("@/retirement ideal/@"));
    assert!(atd.contains("@*commonplace*@. It descends from Horace."));

    // Drama: speaker lemma, inline flow, stage blocks.
    assert!(atd.contains("@: Philosopher\n"));
    assert!(atd.contains("@:[\nHe gestures at the empty room.\n]:@"));
    assert!(atd.contains("@:[\nCurtain.\n]:@"));

    // Practical-coverage constructs.
    assert!(atd.contains("@:!Philosopher, a recluse!:@"));
    assert!(atd.contains("above@>(fig-lyre)"));
    assert!(atd.contains("archive (@><https://example.org/pope><@)"));
    assert!(atd.contains("@\"\"retirement ideal\"\"@ as a @/topos/@.term"));
    assert!(atd.contains("figure of Pope's lyre"));
    assert!(atd.contains("@@@@(lyre.svg)"));
    assert!(atd.contains("| Year | Printer |"));
    assert!(atd.contains("| 1717 | W. Bowyer |"));
    assert!(atd.contains("@.-(1)\nthe ode\n.-@") || atd.contains("@.-(1)\nthe ode\n"));

    // Cited quotation with attribution hypograph.
    assert!(atd.contains("@\"\nBeatus ille qui procul negotiis.\n\"@ Horace, Epode II, 1"));

    // Front and back matter skipped.
    assert!(!atd.contains("Ignored"));
}

/// Plutarch/epidoc corpus shapes: a self-closing <q/> marker
/// must not swallow the enclosing quotation's close; <author>
/// inside a <bibl> flattens as citation text; a body-level
/// <docAuthor> (translator attribution furniture) skips; and
/// single-quoted XML attribute values parse.
#[test]
fn epidoc_corpus_shapes() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Moralia</title></titleStmt></fileDesc></teiHeader>
<text><body>
<div type="edition">
<div n="6" subtype="section" type="textpart">
<p rend='indent'>Excuses accepted kindly: <q>I forgot <q type="unspecified" /> I did not know</q> and the flow continues.</p>
</div>
<div n="intro" type="textpart">
<p>Questioned without reason.<note anchored="true" resp="Loeb">cf. S. G. quoted by <bibl> <author>M. Adler</author>, <title rend="italic">Diss.</title> x (1910)</bibl>.</note></p>
</div>
<docAuthor>Francis George Fowler</docAuthor>
</div>
</body></text></TEI>"##;
    let atd = dendron::serialize(&endo::tei_to_document(tei).unwrap());
    // The outer q closes where it should; prose after it stays
    // in the paragraph.
    assert!(
        atd.contains(r#"@""I forgot  I did not know""@ and the flow continues."#)
            || atd.contains(r#"@""I forgot I did not know""@ and the flow continues."#),
        "in:\n{atd}"
    );
    // The single-quoted rend attribute parsed (no import error)
    // and the bibl author text carries.
    assert!(atd.contains("M. Adler"));
    // Translator furniture is skipped.
    assert!(!atd.contains("Fowler"));
}

/// A note inside a <speaker> pairs its callout with a landed
/// body block (a dropped body orphans the deixis at kanonizo).
#[test]
fn speaker_note_body_lands() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Play</title></titleStmt></fileDesc></teiHeader>
<text><body>
<div type="edition">
<sp> <speaker>PROLOGUE<note resp="editor">Probably not by Plautus.</note> </speaker>
<l n="1">ATTEND to me this day;</l>
<l n="2">good things I bring upon the stage.</l>
</sp>
<sp> <speaker>The COMPANY<note resp="editor">All the actors.</note> of COMEDIANS</speaker>
<p>Farewell, and applaud us.</p>
</sp>
</div>
</body></text></TEI>"##;
    let atd = dendron::serialize(&endo::tei_to_document(tei).unwrap());
    // Verse-speech path and prose-speech path both land the body.
    assert!(atd.contains("PROLOGUE@^(n1)"), "in:\n{atd}");
    assert!(atd.contains("@^\nProbably not by Plautus.\n^@(n1)"));
    assert!(atd.contains("The COMPANY@^(n2) of COMEDIANS"));
    assert!(atd.contains("@^\nAll the actors.\n^@(n2)"));
}

/// TEI P4 (Perseus): a DOCTYPE with an internal DTD subset
/// (parameter entities) strips wholesale; TEI.2 and numbered
/// div1/div2 normalize to the P5 shapes.
#[test]
fn p4_doctype_and_numbered_divs() {
    let tei = r##"<?xml version="1.0"?>
<!DOCTYPE TEI.2 PUBLIC "-//TEI P4//DTD Main DTD Driver File//EN" "http://www.tei-c.org/Guidelines/DTD/tei2.dtd" [
<!ENTITY % TEI.XML "INCLUDE">
<!ENTITY % PersProse PUBLIC "-//Perseus P4//DTD Perseus Prose//EN" "http://www.perseus.tufts.edu/DTD/1.0/PersProse.dtd">
%PersProse;
]>
<TEI.2>
<teiHeader><fileDesc><titleStmt><title>Germania</title></titleStmt></fileDesc></teiHeader>
<text><body>
<div1 type="chapter" n="1">
<head>Boundaries</head>
<p>Germany is separated from the Galli by the Rhine.</p>
<div2 type="section" n="1"><p>A nested section.</p></div2>
</div1>
</body></text></TEI.2>"##;
    let atd = dendron::serialize(&endo::tei_to_document(tei).unwrap());
    assert!(atd.contains("@=Germania=@"));
    assert!(atd.contains("Boundaries"));
    assert!(atd.contains("Germany is separated from the Galli by the Rhine."));
    assert!(atd.contains("A nested section."));
    assert!(!atd.contains("PersProse"));
}

/// Narration interleaved with <said> at block level (Xenophon
/// `<said>For,</said> said he, <said>…</said>`) joins the
/// paragraph flow as quoted phrases; a said spanning its whole
/// paragraph still becomes a dialogue block.
#[test]
fn said_narration_interleave() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Hellenica</title></titleStmt></fileDesc></teiHeader>
<text><body>
<div type="edition">
<div type="textpart" subtype="section" n="15">
<p><said direct="true">For,</said> said he, <said direct="true">you and I have done many things.</said></p>
</div>
<div type="textpart" subtype="section" n="16">
<p><said who="#Socrates"><label>Socrates.</label> Why have you come?</said></p>
</div>
</div>
</body></text></TEI>"##;
    let atd = dendron::serialize(&endo::tei_to_document(tei).unwrap());
    // Interleave: one paragraph, narration between quoted saids.
    assert!(
        atd.contains(r#"@""For,""@.said said he, @""you and I have done many things.""@.said"#),
        "in:\n{atd}"
    );
    // The whole-paragraph said keeps the dialogue-block form.
    assert!(atd.contains("@: Socrates\nWhy have you come?\n:@"));
}

/// Perseus latinLit tolerance: cast-list furniture, stray
/// closes, print furniture (<space/>, <desc>, empty <p/> and
/// <head/>, document-level <pb/>), block-level punctuation
/// debris, and milestone sanitizing (underscored units, values
/// that reduce to nothing).
#[test]
fn latin_corpus_tolerances() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Comoedia</title></titleStmt></fileDesc></teiHeader>
<text>
<pb n="469"/>
<body>
<div type="edition">
<div type="textpart" subtype="act" n="front">
<l n="cast"><foreign xml:lang="lat">Dramatis Personae</foreign>
<listPerson rend="bulleted">
<person> <persName>DEMAENETUS, <roleName>an aged Athenian.</roleName> </persName> </person>
<listPerson rend="castGroup">
<person> <persName>LIBANUS, <roleName>a servant.</roleName> </persName> </person>
</listPerson>
</listPerson></l>
</div>
<div type="textpart" subtype="poem" n="56">
<head>A HEADING</head><head/>
<l n="1">A line with a metrical <space/> gap</l>
<l n="2"><milestone unit="alt_poem_line" n="2a_0"/>An underscored unit</l>
<l n="3"><milestone unit="section" n="9."/>A trailing-dot value</l>
<l n="4"><milestone unit="section" n="1.-a."/>Debris drops the milestone</l>
</div>
<div type="textpart" n="15" subtype="section"><p><said who="#A" rend="merge">Spoken words</said>.</p></div>
<div type="textpart" n="16" subtype="section"> <p/> </div>
<div type="textpart" n="17" subtype="section"><p>A note citation<note><p><cit>
<quote xml:lang="lat">Datis vadibus.</quote>
<bibl n="Hor. S. 1.1.11"/>
</cit> explains.</p></note> continues <desc rend="align(center)">Desunt non pauca.</desc> here.</p></div>
</div>
</body></text></TEI>"##;
    let doc = endo::tei_to_document(tei).unwrap();
    let atd = dendron::serialize(&doc);
    // Cast furniture skipped, stray </person> tolerated.
    assert!(!atd.contains("DEMAENETUS"));
    assert!(atd.contains("Dramatis Personae"));
    // Empty <head/> does not swallow the following lines.
    assert!(atd.contains("A HEADING"));
    assert!(atd.contains("A line with a metrical"));
    // Underscored unit kebabs; trailing-dot value trims; debris
    // value drops its milestone.
    assert!(atd.contains("@(\"alt-poem-line:2a_0\")"));
    assert!(atd.contains("@(\"section:9\")"));
    assert!(!atd.contains("1.-a"));
    assert!(atd.contains("Debris drops the milestone"));
    // Punctuation debris at block level is dropped, the said
    // paragraph carries.
    assert!(atd.contains("Spoken words"));
    // Self-closing <bibl/> and <desc> do not derail the note.
    assert!(atd.contains("continues"));
    assert!(atd.contains("here."));
    assert!(!atd.contains("Desunt"));
}

/// Strictness: what the subset does not cover is an error
/// naming the construct, not silence.
#[test]
fn tei_endo_is_strict() {
    let tei = r#"<TEI><teiHeader><fileDesc><titleStmt><title>T</title>
    </titleStmt></fileDesc></teiHeader>
    <text><body><p>x<interp>y</interp></p></body></text></TEI>"#;
    let err = endo::tei_to_document(tei).unwrap_err();
    assert!(err.to_string().contains("interp"), "was: {err}");
}

/// Real-corpus TEI shapes: Perseus dialogue paragraphs
/// (<p><said><label>), Stephanus milestones, DraCor verse
/// speeches with interleaved stage directions and line notes,
/// epigraph cit wrappers, and headless grouping divs.
#[test]
fn corpus_tei_shapes() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt>
  <title>Crito</title>
  <title type="main">Crito</title>
  <author>Plato</author>
</titleStmt></fileDesc></teiHeader>
<standOff><listRelation/></standOff>
<text><body>
<div type="edition">
<div type="textpart">
<milestone unit="page" resp="Stephanus" n="43"/>
<p><said who="#Socrates"><label>Socrates.</label> Why have you come, Crito?</said></p>
<p><said who="#Crito"><label>Crito.</label> To bring news
<quote type="verse"><l>to Phthia shalt thou go</l><l>on the third day</l></quote>
<bibl n="Hom. Il. 9.363">Hom. Il.</bibl>.</said></p>
</div>
<sp><speaker>ALCESTE.</speaker>
  <l>Laissez-moi, je vous prie.</l>
  <stage>Il sort.</stage>
  <note type="L">Une note de ligne.</note>
  <l>Et je vous supplierai.</l>
</sp>
<epigraph><cit><quote><p>Vengeance is mine.</p></quote></cit></epigraph>
</div>
</body></text></TEI>"##;
    let doc = atrep::endo::tei_to_document(tei).unwrap();
    let atd = atrep::dendron::serialize(&doc);
    // The main title wins over the untyped duplicate.
    assert_eq!(atd.matches("@=Crito=@").count(), 1);
    // Stephanus milestone: the core coordinate form, the unit
    // riding as a presentation genos.
    assert!(atd.contains("@(\"stephanus:43\").page"));
    // Dialogue with the label as the speech prefix.
    assert!(atd.contains("@: Socrates\nWhy have you come, Crito?\n:@"));
    // The inline verse quote joins with the solidus.
    assert!(atd.contains("@\"\"to Phthia shalt thou go / on the third day\"\"@"));
    // The inline bibl citation text carries.
    assert!(atd.contains("Hom. Il."));
    // The verse speech: stage as an inline-stage line, the line
    // note as a deixis with its body after the block.
    assert!(atd.contains("@:~ ALCESTE.\nLaissez-moi, je vous prie.\n@:(Il sort.):@@^(n1)"));
    assert!(atd.contains("Et je vous supplierai."));
    assert!(atd.contains("@^\nUne note de ligne.\n^@(n1)"));
    // The epigraph unwraps its cit.
    assert!(atd.contains("@\"/\nVengeance is mine.\n/\"@"));
}

/// Core milestones and zygosis: coordinates survive kanonizo
/// and litosis, duplicates are rejected, and two witnesses
/// weave into a milestone-headed zygoma.
#[test]
fn milestones_and_zygosis() {
    use atrep::{dendron, kanonizo, litosis, zygosis};
    let tmp = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("zygosis");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    // litogramma resolves from the embedded standard library.
    let dia = "litogramma";

    // Kanonizo: milestones untouched, quoted phrase split at an
    // interior coordinate by zygosis.
    let a = format!(
        "@@@!{dia}\n\nProem A.\n\n@(\"s:1\") alpha one @,quoted @(\"s:2\") across,@ two.\n"
    );
    let b = format!("@@@!{dia}\n\n@(\"s:1\") beta one.\n\n@(\"s:2\") beta two.\n");
    std::fs::write(tmp.join("a.atd"), &a).unwrap();
    std::fs::write(tmp.join("b.atd"), &b).unwrap();
    let ka = kanonizo::kanonizo_file(&tmp.join("a.atd"))
        .unwrap()
        .document;
    let kb = kanonizo::kanonizo_file(&tmp.join("b.atd"))
        .unwrap()
        .document;
    assert!(dendron::serialize(&ka).contains("@(\"s:1\")"));

    // Litosis keeps coordinates (and strips their free genoses).
    let lit = litosis::litosis_with(&ka, &|_| Ok(Vec::new()), &|id| {
        atrep::dialektos::resolve(&tmp, id).ok()
    })
    .unwrap();
    assert!(
        lit.litos.contains("@(\"s:1\")"),
        "coordinates survive litosis"
    );

    // Duplicate coordinates are rejected.
    std::fs::write(
        tmp.join("dup.atd"),
        format!("@@@!{dia}\n\nx @(\"s:1\") y @(\"s:1\") z.\n"),
    )
    .unwrap();
    assert!(kanonizo::kanonizo_file(&tmp.join("dup.atd")).is_err());

    // The weave.
    let zyg =
        zygosis::zygosis(&[("alpha".to_string(), ka), ("beta".to_string(), kb)], "s").unwrap();
    let out = dendron::serialize(&zyg);
    // Proem rides its witness diaphane before any coordinate.
    assert!(out.contains("Proem A."));
    // Milestone-headed segments with witness-tagged diaphanes.
    assert!(out.contains("@(\"s:1\")"));
    assert!(out.contains(".zyg-alpha"));
    assert!(out.contains(".zyg-beta"));
    // The quoted phrase split across the cut: closed and
    // reopened, both halves phrases.
    let seg2 = &out[out.find("@(\"s:2\")").unwrap()..];
    assert!(seg2.contains("across"));
    assert!(!seg2.contains("quoted"));
    // Order consistency violation is an error.
    let c = format!("@@@!{dia}\n\n@(\"s:2\") first.\n\n@(\"s:1\") second.\n");
    std::fs::write(tmp.join("c.atd"), &c).unwrap();
    let kc = kanonizo::kanonizo_file(&tmp.join("c.atd"))
        .unwrap()
        .document;
    let kb2 = kanonizo::kanonizo_file(&tmp.join("b.atd"))
        .unwrap()
        .document;
    let err = zygosis::zygosis(&[("b".to_string(), kb2), ("c".to_string(), kc)], "s");
    assert!(err.is_err());
}

/// The opt-in line-milestone pre-pass: verse `<l n>` lines under
/// numbered book divs get a milestone at line 1 and every fifth
/// line, valued `{book}.{N}`; other lines get none, and the flag
/// left off is byte-identical to a plain import.
#[test]
fn tei_line_milestones_book_scheme() {
    let mut tei = String::from(
        "<TEI xmlns=\"http://www.tei-c.org/ns/1.0\">\n\
         <teiHeader><fileDesc><titleStmt><title>Epic</title></titleStmt>\
         </fileDesc></teiHeader>\n<text><body>\n",
    );
    for book in 1..=2 {
        tei.push_str(&format!("<div type=\"book\" n=\"{book}\"><lg>\n"));
        for n in 1..=12 {
            tei.push_str(&format!("<l n=\"{n}\">line {book} {n}</l>\n"));
        }
        tei.push_str("</lg></div>\n");
    }
    tei.push_str("</body></text></TEI>\n");

    let doc = endo::tei_to_document_lines(&tei, Some("grc:book-line")).unwrap();
    let atd = dendron::serialize(&doc);
    for v in ["1.1", "1.5", "1.10", "2.1", "2.5", "2.10"] {
        assert!(
            atd.contains(&format!("@(\"grc:book-line:{v}\")")),
            "missing milestone {v} in:\n{atd}"
        );
    }
    for v in [
        "1.2", "1.3", "1.4", "1.6", "1.11", "1.12", "2.2", "2.7", "2.11",
    ] {
        assert!(
            !atd.contains(&format!("book-line:{v}\")")),
            "unexpected milestone {v}"
        );
    }

    // Default behavior (flag absent) is byte-identical.
    let plain = dendron::serialize(&endo::tei_to_document(&tei).unwrap());
    assert!(!plain.contains("book-line"));
    assert_eq!(
        plain,
        dendron::serialize(&endo::tei_to_document_lines(&tei, None).unwrap())
    );
}

/// Without a book div in scope, line milestones carry the bare
/// `{N}` value.
#[test]
fn tei_line_milestones_bookless() {
    let mut tei = String::from(
        "<TEI xmlns=\"http://www.tei-c.org/ns/1.0\">\n\
         <teiHeader><fileDesc><titleStmt><title>Poem</title></titleStmt>\
         </fileDesc></teiHeader>\n<text><body><lg>\n",
    );
    for n in 1..=12 {
        tei.push_str(&format!("<l n=\"{n}\">verse {n}</l>\n"));
    }
    tei.push_str("</lg></body></text></TEI>\n");

    let atd = dendron::serialize(&endo::tei_to_document_lines(&tei, Some("grc:line")).unwrap());
    for v in ["1", "5", "10"] {
        assert!(
            atd.contains(&format!("@(\"grc:line:{v}\")")),
            "missing milestone {v} in:\n{atd}"
        );
    }
    for v in ["2", "3", "4", "6", "11", "12"] {
        assert!(
            !atd.contains(&format!("grc:line:{v}\")")),
            "unexpected milestone {v}"
        );
    }
}
