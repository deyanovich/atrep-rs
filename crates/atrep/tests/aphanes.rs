//! at-aphanes: the unseen pack. Editorial metadata (who speaks
//! a prose line, whom a name denotes, a reference's target, a
//! date's value, an analytic category) rides monosims first
//! inside the annotated span; litosis strips them, exos read them
//! through sim-name slots, importers emit them from the source's
//! attributes.

use std::path::{Path, PathBuf};

use atrep::{dendron, dialektos, endo, exo, kanonizo, litosis, morph};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn kanon_of(tmp: &Path, name: &str, atd: &str) -> dendron::Document {
    let path = tmp.join(format!("{name}.atd"));
    std::fs::write(&path, atd).unwrap();
    kanonizo::kanonizo_file(&path).unwrap().document
}

fn litos_of(doc: &dendron::Document) -> String {
    litosis::litosis(doc, &|_| Ok(Vec::new())).unwrap().litos
}

const ANNOTATED: &str = "\
@@@!litogramma

@=Tom Sawyer=@

@# Chapter I

@:-@?:(polly)\u{201c}Tom!\u{201d}-:@

No answer.

Aunt @,@?:(polly)Polly,@.persname went to the door and looked out
among the tomato vines toward @,@?.(st-petersburg)the village,@.placename.

@\"\"@?:(tom)@?%(dialect)I reckon\"\"@.said, said he, on @,@?-(1876-06-01)the first of June,@.date.
#@
";

const BARE: &str = "\
@@@!litogramma

@=Tom Sawyer=@

@# Chapter I

@:-\u{201c}Tom!\u{201d}-:@

No answer.

Aunt @,Polly,@.persname went to the door and looked out
among the tomato vines toward @,the village,@.placename.

@\"\"I reckon\"\"@.said, said he, on @,the first of June,@.date.
#@
";

/// The pack parses inside litogramma, kanonizes in place, and
/// leaves the litos untouched: the annotated and the bare text
/// have the same identity.
#[test]
fn aphanes_is_hash_neutral() {
    let tmp = tmp_dir("aphanes-litos");
    let annotated = kanon_of(&tmp, "annotated", ANNOTATED);
    let bare = kanon_of(&tmp, "bare", BARE);
    let atk = dendron::serialize(&annotated);
    assert!(
        atk.contains("@:-@?:(polly)\u{201c}Tom!\u{201d}-:@"),
        "{atk}"
    );
    assert!(atk.contains("@,@?:(polly)Polly,@.persname"), "{atk}");
    assert!(
        atk.contains("@\"\"@?:(tom)@?%(dialect)I reckon\"\"@.said"),
        "{atk}"
    );
    assert_eq!(litos_of(&annotated), litos_of(&bare));
    assert!(!litos_of(&annotated).contains("?:"));
}

/// Every litogramma export accepts the pack: the paper targets
/// render nothing for it, the TEI target turns it into attributes.
#[test]
fn aphanes_renders_across_litogramma_targets() {
    let tmp = tmp_dir("aphanes-exo");
    let kanon = kanon_of(&tmp, "doc", ANNOTATED);
    for target in ["html", "gemtext", "latex"] {
        let x = exo::resolve_exo(&tmp, "litogramma", target).unwrap();
        let out = exo::render(&kanon, &x, &tmp).unwrap();
        assert!(out.contains("Polly"), "{target} lost the name");
        assert!(!out.contains("polly"), "{target} leaked a key:\n{out}");
        assert!(!out.contains("?:"), "{target} leaked a symbol:\n{out}");
    }
    // Homs into the paper-side dialektoi drop the pack.
    for target in ["at-html", "at-docbook"] {
        let route = morph::resolve_route(&tmp, "litogramma", target).unwrap();
        let out = morph::apply_route(&kanon, &route).unwrap();
        assert!(
            !dendron::serialize(&out).contains("@?"),
            "{target} kept the pack"
        );
    }
    // The TEI hom carries the pack implicitly; the tei exo reads
    // it through sim-name slots.
    let route = morph::resolve_route(&tmp, "litogramma", "at-tei").unwrap();
    let at_tei = morph::apply_route(&kanon, &route).unwrap();
    let x = exo::resolve_exo(&tmp, "at-tei", "tei").unwrap();
    let tei = exo::render(&at_tei, &x, &tmp).unwrap();
    assert!(tei.contains(r##"<said who="#polly">"##), "{tei}");
    assert!(
        tei.contains(r##"<persName ref="#polly">Polly</persName>"##),
        "{tei}"
    );
    assert!(
        tei.contains(r##"<placeName ref="#st-petersburg">the village</placeName>"##),
        "{tei}"
    );
    // Litogramma marks speech with the said genos on the
    // quotation, so TEI gets said, not q.
    assert!(
        tei.contains(r##"<said who="#tom" ana="#dialect">I reckon</said>"##),
        "{tei}"
    );
    assert!(
        tei.contains(r##"<date when="1876-06-01">the first of June</date>"##),
        "{tei}"
    );
    assert!(!tei.contains("?:"), "{tei}");
}

/// The TEI importer: a speech paragraph attributed by pointer
/// alone is a prose dialogue line with the prosopon; inline said
/// and q carry who and ana; names with ref keep their span; a
/// date's when and a paragraph's ana ride along; speech mode is
/// a genos. A drama speech with a printed speaker is untouched.
#[test]
fn tei_import_emits_aphanes() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Tom Sawyer</title></titleStmt></fileDesc></teiHeader>
<text><body>
<div type="chapter" n="1">
<p><said who="#polly">“What’s gone with that boy, I wonder?”</said></p>
<p><said who="#polly">“Tom!”</said> No answer. <q who="#tom" ana="#dialect">“I reckon.”</q></p>
<p>Aunt <persName ref="#polly">Polly</persName> went to <placeName ref="#st-petersburg">the village</placeName>
with the <orgName ref="#temperance">Cadets of Temperance</orgName> on
<date when="1876-06-01">the first of June</date>; <rs type="person" ref="#tom">the boy</rs> stayed.</p>
<p ana="#narration">She said <said direct="false" aloud="false">she would never go back</said>.</p>
<p><said who="#Socrates"><label>Socrates.</label> Why have you come?</said></p>
<p>Plain <persName>Huck</persName> stays transparent.</p>
</div>
</body></text></TEI>"##;
    let doc = endo::tei_to_document(tei).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@:-@?:(polly)\u{201c}What\u{2019}s gone with that boy, I wonder?\u{201d}-:@"),
        "{atd}"
    );
    assert!(
        atd.contains("@\"\"@?:(polly)\u{201c}Tom!\u{201d}\"\"@.said No answer."),
        "{atd}"
    );
    assert!(
        atd.contains("@\"\"@?:(tom)@?%(dialect)\u{201c}I reckon.\u{201d}\"\"@.said"),
        "{atd}"
    );
    assert!(
        atd.contains("Aunt @,@?:(polly)Polly,@.persname went to"),
        "{atd}"
    );
    assert!(
        atd.contains("@,@?.(st-petersburg)the village,@.placename"),
        "{atd}"
    );
    assert!(
        atd.contains("@,@?&(temperance)Cadets of Temperance,@.orgname"),
        "{atd}"
    );
    assert!(
        atd.contains("@,@?-(1876-06-01)the first of June,@.date"),
        "{atd}"
    );
    assert!(atd.contains("@,@?:(tom)the boy,@.rs stayed."), "{atd}");
    assert!(
        atd.contains(
            "@?%(narration)She said @\"\"she would never go back\"\"@.said.indirect.thought."
        ),
        "{atd}"
    );
    // Drama keeps its printed speaker, and the pointer rides first
    // in the lemma.
    assert!(
        atd.contains("@: @?:(Socrates)Socrates\nWhy have you come?\n:@"),
        "{atd}"
    );
    // A name without a pointer stays transparent (reading text).
    assert!(atd.contains("Plain Huck stays transparent."), "{atd}");

    // Kanon and litos: the pointers survive kanonizo untouched
    // (free keys are not numbered onyms), the litos drops them.
    let tmp = tmp_dir("aphanes-tei-roundtrip");
    let kanon = kanon_of(&tmp, "doc", &atd);
    let atk = dendron::serialize(&kanon);
    assert!(atk.contains("@?:(polly)"), "{atk}");
    assert!(!litos_of(&kanon).contains("@?"));

    // And back out to TEI with the attributes restored.
    let route = morph::resolve_route(&tmp, "litogramma", "at-tei").unwrap();
    let at_tei = morph::apply_route(&kanon, &route).unwrap();
    let x = exo::resolve_exo(&tmp, "at-tei", "tei").unwrap();
    let out = exo::render(&at_tei, &x, &tmp).unwrap();
    assert!(out.contains(r##"<said who="#polly">"##), "{out}");
    assert!(
        out.contains("<said who=\"#tom\" ana=\"#dialect\">\u{201c}I reckon.\u{201d}</said>"),
        "{out}"
    );
    assert!(
        out.contains(r##"<persName ref="#polly">Polly</persName>"##),
        "{out}"
    );
    assert!(
        out.contains(r##"<orgName ref="#temperance">Cadets of Temperance</orgName>"##),
        "{out}"
    );
    assert!(out.contains(r##"<rs ref="#tom">the boy</rs>"##), "{out}");
    assert!(out.contains(r##"<p ana="#narration">"##), "{out}");
    assert!(
        out.contains(r##"<said direct="false" aloud="false">she would never go back</said>"##),
        "{out}"
    );
}

/// OSIS: q/@who and reference/@osisRef become prosopon and
/// skopos; the OSIS export restores the attributes.
#[test]
fn osis_import_and_export_carry_aphanes() {
    let osis = r#"<?xml version="1.0"?>
<osis xmlns="http://www.bibletechnologies.net/2003/OSIS/namespace">
  <osisText osisIDWork="kjv">
    <div type="book" osisID="Matt">
      <chapter osisID="Matt.4" sID="c4" n="4"/>
      <p><verse osisID="Matt.4.19" sID="v19" n="19"/>And he saith unto them, <q who="Jesus">Follow me.</q><verse eID="v19"/>
      <verse osisID="Matt.4.20" sID="v20" n="20"/><q who="Pilate">What is truth?</q> as it is written in <reference osisRef="Gen.1.1-Gen.1.3">Genesis 1:1-3</reference>.<note type="crossReference" osisRef="Matt.4.20"><reference osisRef="Isa.9.1">Isa 9:1</reference></note><verse eID="v20"/></p>
      <chapter eID="c4"/>
    </div>
  </osisText>
</osis>"#;
    let doc = endo::osis_to_document(osis).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@,@?:(Jesus)Follow me.,@.wj"), "{atd}");
    assert!(atd.contains("@,@?:(Pilate)What is truth?,@.said"), "{atd}");
    assert!(
        atd.contains("@,@?>(Gen.1.1-Gen.1.3)Genesis 1:1-3,@.ref"),
        "{atd}"
    );
    assert!(
        atd.contains("@^@?>(Matt.4.20)@,@?>(Isa.9.1)Isa 9:1,@.xo^@.x"),
        "{atd}"
    );

    let tmp = tmp_dir("aphanes-osis");
    let kanon = kanon_of(&tmp, "doc", &atd);
    let x = exo::resolve_exo(&tmp, "at-usfm", "osis").unwrap();
    let out = exo::render(&kanon, &x, &tmp).unwrap();
    assert!(
        out.contains(r#"<q who="Jesus" marker="">Follow me.</q>"#),
        "{out}"
    );
    assert!(
        out.contains(r#"<q who="Pilate" marker="">What is truth?</q>"#),
        "{out}"
    );
    assert!(
        out.contains(r#"<reference osisRef="Gen.1.1-Gen.1.3">Genesis 1:1-3</reference>"#),
        "{out}"
    );
    assert!(out.contains(r#"<note type="crossReference" osisRef="Matt.4.20"><reference osisRef="Isa.9.1">Isa 9:1</reference></note>"#), "{out}");
    let x = exo::resolve_exo(&tmp, "at-usfm", "usx").unwrap();
    let usx = exo::render(&kanon, &x, &tmp).unwrap();
    assert!(
        usx.contains(r#"<ms style="qt-s" who="Pilate"/>What is truth?<ms style="qt-e"/>"#),
        "{usx}"
    );
    assert!(
        usx.contains(r#"<ref loc="Gen.1.1-Gen.1.3">Genesis 1:1-3</ref>"#),
        "{usx}"
    );
    assert!(!litos_of(&kanon).contains("@?"));
}

/// USX: ref/@loc normalizes to the OSIS spelling, qt-s/qt-e
/// milestones fold into a said span with the speaker, the red
/// letters carry their constant speaker.
#[test]
fn usx_import_carries_aphanes() {
    let usx = r#"<?xml version="1.0" encoding="utf-8"?>
<usx version="3.0">
<book code="MAT" style="id"/>
<chapter number="4" style="c" sid="MAT 4"/>
<para style="p"><verse number="19" style="v" sid="MAT 4:19"/>And he saith unto them, <char style="wj">Follow me.</char> <ms style="qt-s" sid="q1" who="Pilate"/>What is truth?<ms style="qt-e" eid="q1"/> as in <ref loc="GEN 1:1-3">Genesis 1:1–3</ref> and <ref loc="JHN 3:16-4:2">John 3:16–4:2</ref>.<verse eid="MAT 4:19"/></para>
<chapter eid="MAT 4"/>
</usx>"#;
    let doc = endo::usx_to_document(usx).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@,@?:(Jesus)Follow me.,@.wj"), "{atd}");
    assert!(atd.contains("@,@?:(Pilate)What is truth?,@.said"), "{atd}");
    assert!(
        atd.contains("@,@?>(Gen.1.1-Gen.1.3)Genesis 1:1\u{2013}3,@.ref"),
        "{atd}"
    );
    assert!(
        atd.contains("@,@?>(John.3.16-John.4.2)John 3:16\u{2013}4:2,@.ref"),
        "{atd}"
    );
    assert!(!atd.contains("qt-"), "{atd}");
}

/// A sim-name slot must name a monosim of the source dialektos.
#[test]
fn sim_name_slot_is_validated() {
    let tmp = tmp_dir("aphanes-slot");
    std::fs::write(
        tmp.join("litogramma.check.exo"),
        "@@@!atrep-exo\n@=litogramma=>check\n\n@-> *document\n@(grammata)\n>-@\n\n@-> *paragraph\n<p @(emphasis)>@(grammata)</p>\n>-@\n",
    )
    .unwrap();
    let err = exo::resolve_exo(&tmp, "litogramma", "check").unwrap_err();
    assert!(err.to_string().contains("not a monosim"), "{err}");
    std::fs::write(
        tmp.join("litogramma.check.exo"),
        "@@@!atrep-exo\n@=litogramma=>check\n\n@-> *document\n@(grammata)\n>-@\n\n@-> *paragraph\n<p @(nonesuch)>@(grammata)</p>\n>-@\n",
    )
    .unwrap();
    let err = exo::resolve_exo(&tmp, "litogramma", "check").unwrap_err();
    assert!(err.to_string().contains("unknown slot"), "{err}");
}

/// The packs ship English and Russian glossae, and litogramma
/// inherits them with the sims: a document may spell the unseen
/// marks and the parsing in any of the three, to one kanon.
#[test]
fn std_glossae_travel_with_the_packs() {
    let tmp = tmp_dir("aphanes-glossae");
    let greek = "@@@!litogramma\n\n@:-@?:(polly)\u{201c}Tom!\u{201d}-:@\n\n\
        @@.@!.(1)@@.@!=(школа)@!/(S)Школа.@@ учит.@@\n";
    let english = "@@@!litogramma\n\n@:-@{person}(polly)\u{201c}Tom!\u{201d}-:@\n\n\
        @@.@{sentence}(1)@@.@{lexeme}(школа)@{part-of-speech}(S)Школа.@@ учит.@@\n";
    let russian = "@@@!litogramma\n\n@:-@{лицо}(polly)\u{201c}Tom!\u{201d}-:@\n\n\
        @@.@{предложение}(1)@@.@{лексема}(школа)@{часть-речи}(S)Школа.@@ учит.@@\n";
    let k1 = kanon_of(&tmp, "greek", greek);
    let k2 = kanon_of(&tmp, "english", english);
    let k3 = kanon_of(&tmp, "russian", russian);
    assert_eq!(dendron::serialize(&k1), dendron::serialize(&k2));
    assert_eq!(dendron::serialize(&k1), dendron::serialize(&k3));
    let dial = dialektos::resolve(&tmp, "litogramma").unwrap();
    assert!(dial.glossae.contains_key("en") && dial.glossae.contains_key("ru"));
    let ru = dendron::serialize_plerographic_in(&k1, &dial, Some("ru")).unwrap();
    assert!(
        ru.contains("@{лицо}(polly)") && ru.contains("@{лексема}(школа)"),
        "{ru}"
    );
    let en = dendron::serialize_plerographic_in(&k1, &dial, Some("en")).unwrap();
    assert!(
        en.contains("@{person}(polly)") && en.contains("@{sentence}(1)"),
        "{en}"
    );
}

/// TEI tokens and sentences with a parsing become the parsing
/// pack's diaphanes; bare ones stay transparent.
#[test]
fn tei_tokens_import_into_the_parsing_pack() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>T</title></titleStmt></fileDesc></teiHeader>
<text><body>
<p><s xml:id="s1"><w lemma="школа" pos="NOUN" msd="Case=Nom|Number=Sing">Школа</w> <w lemma="учить" pos="VERB">учит</w><pc pos="PUNCT">.</pc></s> <s>Plain <w>words</w> stay.</s></p>
</body></text></TEI>"##;
    let doc = endo::tei_to_document(tei).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@@.@!.(s1)@@.@!=(школа)@!/(NOUN)@!%(Case=Nom,Number=Sing)Школа.@@ @@.@!=(учить)@!/(VERB)учит.@@@@.@!/(PUNCT)..@@.@@ @@.@!.(1)Plain words stay..@@"),
        "{atd}"
    );
    let tmp = tmp_dir("aphanes-tei-tokens");
    let kanon = kanon_of(&tmp, "doc", &atd);
    let bare = kanon_of(
        &tmp,
        "bare",
        "@@@!litogramma\n\n@=T=@\n\nШкола учит. Plain words stay.\n",
    );
    assert_eq!(litos_of(&kanon), litos_of(&bare));
}

/// TEI choice: the reading text is the editor's form; the page's
/// form rides along as the paradosis aphanes on a diaphane around
/// it, so the litos is the reading and the kanon still answers
/// what the page printed. One side alone is plain text.
#[test]
fn tei_choice_carries_the_transmitted_reading() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Tom Sawyer</title></titleStmt></fileDesc></teiHeader>
<text><body>
<p>See the <choice><abbr>fig.</abbr><expan>figure</expan></choice> of <choice><orig>ye olde</orig><reg>the old</reg></choice> lyre; <choice><sic>teh</sic><corr>the</corr></choice> end. A <choice><reg>lone</reg></choice> side and a <choice><orig>bare</orig></choice> one.</p>
</body></text></TEI>"##;
    let doc = endo::tei_to_document(tei).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains(
            "See the @@.@?~(fig.)figure.@@ of @@.@?~(ye\\ olde)the old.@@ lyre; @@.@?~(teh)the.@@ end. A lone side and a bare one."
        ),
        "{atd}"
    );
    let tmp = tmp_dir("aphanes-choice");
    let kanon = kanon_of(&tmp, "choice", &atd);
    let atk = dendron::serialize(&kanon);
    assert!(atk.contains("@@.@?~(ye\\ olde)the old.@@"), "{atk}");
    let litos = litos_of(&kanon);
    assert!(
        litos.contains("See the figure of the old lyre; the end."),
        "{litos}"
    );
    assert!(!litos.contains("olde"), "{litos}");
    // The plerographic spelling reads the glossa.
    let dial = dialektos::resolve(&tmp, "litogramma").unwrap();
    let plero = dendron::serialize_plerographic_in(&kanon, &dial, Some("en")).unwrap();
    assert!(plero.contains("@{original}(ye\\ olde)"), "{plero}");
    // TEI export writes the pair back as a choice (a gate slot
    // closes the element only where a paradosis is present); a
    // one-sided choice stays plain text.
    let route = morph::resolve_route(&tmp, "litogramma", "at-tei").unwrap();
    let at_tei = morph::apply_route(&kanon, &route).unwrap();
    let x = exo::resolve_exo(&tmp, "at-tei", "tei").unwrap();
    let out = exo::render(&at_tei, &x, &tmp).unwrap();
    assert!(
        out.contains(
            "of <choice><orig>ye olde</orig><reg>the old</reg></choice> lyre; <choice><orig>teh</orig><reg>the</reg></choice> end. A lone side and a bare one."
        ),
        "{out}"
    );
}

/// TEI listPerson: each person is a dramatis-persona line with
/// the xml:id as its onym, so a prosopon key in the text resolves
/// against a declared character without a cast table; a person
/// with a note is a character entry with the note as description.
#[test]
fn tei_list_person_declares_the_cast() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Tom Sawyer</title></titleStmt></fileDesc></teiHeader>
<text><body>
<listPerson>
<head>Persons</head>
<person xml:id="polly"><persName>Aunt Polly</persName><note>Tom's aunt.</note><sex>F</sex></person>
<person xml:id="tom"><persName>Tom Sawyer</persName><birth>1835</birth></person>
<listPerson><person><persName>A nameless boy</persName></person></listPerson>
</listPerson>
<p><said who="#polly">“Tom!”</said></p>
</body></text></TEI>"##;
    let doc = endo::tei_to_document(tei).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@#_Persons_#@"), "{atd}");
    assert!(
        atd.contains("@:!! Aunt Polly\nTom's aunt.\n!!:@(polly)"),
        "{atd}"
    );
    assert!(atd.contains("@:!Tom Sawyer!:@(tom)"), "{atd}");
    assert!(atd.contains("@:!A nameless boy!:@\n"), "{atd}");
    let tmp = tmp_dir("aphanes-list-person");
    let kanon = kanon_of(&tmp, "cast", &atd);
    // The ids are numbered onyms: kanonizo renumbers the one a
    // prosopon key names, together with the key, and drops the
    // one nothing points at.
    let atk = dendron::serialize(&kanon);
    assert!(atk.contains("!!:@(o1)") && atk.contains("@?:(o1)"), "{atk}");
    assert!(atk.contains("@:!Tom Sawyer!:@\n"), "{atk}");
}

/// The cast declared where TEI editions declare it, in the
/// header's particDesc, lowers to the same lines as a body-level
/// list, after the front matter.
#[test]
fn tei_header_list_person_declares_the_cast() {
    let tei = r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader>
<fileDesc><titleStmt><title>Tom Sawyer</title><author>Mark Twain</author></titleStmt></fileDesc>
<profileDesc>
<langUsage><language ident="en"/></langUsage>
<particDesc>
<listPerson>
<person xml:id="polly"><persName>Aunt Polly</persName></person>
<person xml:id="tom"><persName>Tom Sawyer</persName></person>
</listPerson>
</particDesc>
</profileDesc>
</teiHeader>
<text><body>
<p><said who="#tom">“I reckon.”</said></p>
</body></text></TEI>"##;
    let doc = endo::tei_to_document(tei).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@=:Mark Twain:=@\n\n@:!Aunt Polly!:@(polly)\n\n@:!Tom Sawyer!:@(tom)\n"),
        "{atd}"
    );
    let tmp = tmp_dir("aphanes-header-cast");
    let kanon = kanon_of(&tmp, "cast", &atd);
    let atk = dendron::serialize(&kanon);
    assert!(
        atk.contains("@:!Tom Sawyer!:@(o1)") && atk.contains("@?:(o1)"),
        "{atk}"
    );
}

/// TEI sp/@who: the printed prefix stays the dialogue lemma, and
/// the pointer rides as a prosopon first in it, so a speech
/// reaches its character by key even where the prefix is
/// abbreviated or punctuated. A speech without a pointer is
/// attributed by its prefix alone, as before, and the two forms
/// share one litos.
#[test]
fn tei_speech_carries_its_speaker_pointer() {
    let tei = |who: &str| {
        format!(
            r##"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Hamlet</title></titleStmt></fileDesc></teiHeader>
<text><body><div type="scene">
<sp{who}><speaker>BARNARDO.</speaker><p>Who’s there?</p></sp>
<sp><speaker>FRANCISCO.</speaker><p>Nay, answer me.</p></sp>
<sp{who}><speaker>BARNARDO.</speaker><l>Long live the King!</l></sp>
<sp{who}><p>A speakerless continuation.</p></sp>
</div></body></text></TEI>"##
        )
    };
    let doc = endo::tei_to_document(&tei(r##" who="#barnardo""##)).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@: @?:(barnardo)BARNARDO.\nWho\u{2019}s there?\n:@"),
        "{atd}"
    );
    // No pointer in the source: the prefix alone, as before.
    assert!(atd.contains("@: FRANCISCO.\nNay, answer me.\n:@"), "{atd}");
    // The verse speech carries it in its lemma too.
    assert!(
        atd.contains("@:~ @?:(barnardo)BARNARDO.\nLong live the King!"),
        "{atd}"
    );
    // A speakerless speech stays speakerless: the pointer annotates
    // a printed prefix, it does not stand in for one.
    assert!(atd.contains("\nA speakerless continuation.\n"), "{atd}");
    assert_eq!(atd.matches("@?:(barnardo)").count(), 2, "{atd}");

    // Same litos with and without the pointers.
    let tmp = tmp_dir("aphanes-speech");
    let kanon = kanon_of(&tmp, "who", &atd);
    let bare = dendron::serialize(&endo::tei_to_document(&tei("")).unwrap());
    assert!(!bare.contains("@?:"), "{bare}");
    let bare_kanon = kanon_of(&tmp, "bare", &bare);
    assert_eq!(litos_of(&kanon), litos_of(&bare_kanon));

    // TEI export restores who on the speech; a speech without a
    // pointer exports without the attribute.
    let route = morph::resolve_route(&tmp, "litogramma", "at-tei").unwrap();
    let at_tei = morph::apply_route(&kanon, &route).unwrap();
    let x = exo::resolve_exo(&tmp, "at-tei", "tei").unwrap();
    let out = exo::render(&at_tei, &x, &tmp).unwrap();
    assert!(
        out.contains("<sp who=\"#barnardo\">\n<speaker>BARNARDO.</speaker>"),
        "{out}"
    );
    assert!(out.contains("<sp>\n<speaker>FRANCISCO.</speaker>"), "{out}");
}
