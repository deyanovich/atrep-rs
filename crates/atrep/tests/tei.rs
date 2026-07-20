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

/// Strictness: what the subset does not cover is an error
/// naming the construct, not silence.
#[test]
fn tei_endo_is_strict() {
    let tei = r#"<TEI><teiHeader><fileDesc><titleStmt><title>T</title>
    </titleStmt></fileDesc></teiHeader>
    <text><body><p>x<app><rdg>y</rdg></app></p></body></text></TEI>"#;
    let err = endo::tei_to_document(tei).unwrap_err();
    assert!(err.to_string().contains("app"), "was: {err}");
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
