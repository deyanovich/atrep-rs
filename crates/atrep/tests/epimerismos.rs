//! at-epimerismos: token-level parsing from the Russian National
//! Corpus, OpenCorpora and PROIEL, as onymized sentence diaphanes
//! of token diaphanes. Import shapes, kanonizo fixed point, litos
//! neutrality against the bare text, and export round trips.

use std::path::{Path, PathBuf};

use atrep::{dendron, epimerismos, kanonizo, litosis};

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

const RNC: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html>
<head>
<meta content="А. С. Пушкин" name="author"/>
<title>Капитанская дочка</title>
</head>
<body>
<p><se><w><ana lex="отец" gr="S,m,anim=sg,nom" sem="r:concr t:hum"/>Отец</w> <w><ana lex="мой" gr="A-PRO=m,sg,nom"/>мой</w> <w><ana lex="служить" gr="V,ipf,intr,act=praet,sg,indic,m"/>служил</w>.</se> <se><w><ana lex="я" gr="S-PRO,sg,1p=nom"/>Я</w> <w><ana lex="жить" gr="V,ipf,intr,act=praet,sg,indic,m"/>жил</w> <w><ana lex="недоросль" gr="S,m,anim=sg,ins"/><ana lex="недоросль" gr="S,m,anim=pl,dat"/>недорослем</w>.</se></p>
<p><se><w><ana lex="матушка" gr="S,f,anim=sg,nom"/>Матушка</w> <w><ana lex="быть" gr="V,ipf,intr,act=praet,sg,indic,f"/>была</w> <w><ana lex="ещё" gr="ADV"/>ещё</w> <w><ana lex="я" gr="S-PRO,sg,1p=ins"/>мною</w> <w><ana lex="брюхатый" gr="A=f,sg,nom,plen"/>брюхата</w>.</se></p>
</body>
</html>
"#;

#[test]
fn rnc_imports_kanonizes_and_round_trips() {
    let doc = epimerismos::rnc_to_document(RNC).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.starts_with("@@@!litogramma\n"), "{atd}");
    assert!(atd.contains("@=Капитанская дочка=@"), "{atd}");
    assert!(atd.contains("@=:А. С. Пушкин:=@"), "{atd}");
    // The token: parsing first, then the form, in a diaphane;
    // the sentence: a diaphane opened by the periodos marker.
    assert!(
        atd.contains("@@.@!.(1)@@.@!=(отец)@!/(S)@!%(m,anim=sg,nom)@!&(r:concr_t:hum)Отец.@@ @@.@!=(мой)@!/(A-PRO)@!%(=m,sg,nom)мой.@@"),
        "{atd}"
    );
    // An ambiguous token repeats its parsing from lexema.
    assert!(
        atd.contains("@@.@!.(2)@@.@!=(я)@!/(S-PRO)@!%(sg,1p=nom)Я.@@ @@.@!=(жить)"),
        "{atd}"
    );
    assert!(
        atd.contains("@@.@!=(недоросль)@!/(S)@!%(m,anim=sg,ins)@!=(недоросль)@!/(S)@!%(m,anim=pl,dat)недорослем.@@..@@\n"),
        "{atd}"
    );

    let tmp = tmp_dir("epimerismos-rnc");
    let kanon = kanon_of(&tmp, "rnc", &atd);
    let atk = dendron::serialize(&kanon);
    // Kanonizo keeps the token and sentence diaphanes: they
    // scope their monosims.
    assert!(atk.contains("@@.@!.(1)@@.@!=(отец)"), "{atk}");
    // Fixed point.
    let kanon2 = kanon_of(&tmp, "rnc2", &atk);
    assert_eq!(dendron::serialize(&kanon2), atk);

    // Litos: the same as the bare text's.
    let bare = "@@@!litogramma\n\n@=Капитанская дочка=@\n\n@=:А. С. Пушкин:=@\n\n\
        Отец мой служил. Я жил недорослем.\n\nМатушка была ещё мною брюхата.\n";
    let bare = kanon_of(&tmp, "bare", bare);
    assert_eq!(litos_of(&kanon), litos_of(&bare));

    // Export: the canonical sample comes back byte for byte, and
    // the exported form re-imports to the same document.
    let out = epimerismos::export(&kanon, "rnc").unwrap().unwrap();
    assert_eq!(out, RNC);
    let again = epimerismos::rnc_to_document(&out).unwrap();
    assert_eq!(dendron::serialize(&again), atd);
}

const OPENCORPORA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<annotation version="2.0" revision="0">
<text id="1" parent="0" name="Школа злословия">
<paragraphs>
<paragraph id="1">
<sentence id="1">
<source>Школа злословия учит прикусить язык.</source>
<tokens>
<token id="1" text="Школа">
<tfr rev_id="1" t="Школа">
<v><l id="1" t="школа"><g v="NOUN"/><g v="inan"/><g v="femn"/><g v="sing"/><g v="nomn"/></l></v>
</tfr>
</token>
<token id="2" text="злословия">
<tfr rev_id="2" t="злословия">
<v><l id="2" t="злословие"><g v="NOUN"/><g v="inan"/><g v="neut"/><g v="sing"/><g v="gent"/></l></v>
</tfr>
</token>
<token id="3" text="учит">
<tfr rev_id="3" t="учит">
<v><l id="3" t="учить"><g v="VERB"/><g v="impf"/><g v="tran"/><g v="sing"/><g v="3per"/><g v="pres"/><g v="indc"/></l></v>
</tfr>
</token>
<token id="4" text="прикусить">
<tfr rev_id="4" t="прикусить">
<v><l id="4" t="прикусить"><g v="INFN"/><g v="perf"/><g v="tran"/></l></v>
</tfr>
</token>
<token id="5" text="язык">
<tfr rev_id="5" t="язык">
<v><l id="5" t="язык"><g v="NOUN"/><g v="inan"/><g v="masc"/><g v="sing"/><g v="accs"/></l></v>
<v><l id="6" t="язык"><g v="NOUN"/><g v="inan"/><g v="masc"/><g v="sing"/><g v="nomn"/></l></v>
</tfr>
</token>
<token id="6" text=".">
<tfr rev_id="6" t=".">
<v><l id="7" t="."><g v="PNCT"/></l></v>
</tfr>
</token>
</tokens>
</sentence>
</paragraph>
</paragraphs>
</text>
</annotation>
"#;

#[test]
fn opencorpora_imports_and_round_trips() {
    let doc = epimerismos::opencorpora_to_document(OPENCORPORA).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@=Школа злословия=@"), "{atd}");
    assert!(
        atd.contains(
            "@@.@!.(1)@@.@!=(школа)@!/(NOUN)@!%(inan,femn,sing,nomn)Школа.@@ @@.@!=(злословие)"
        ),
        "{atd}"
    );
    // Two analyses; the punctuation token keeps its lemma.
    assert!(
        atd.contains("@@.@!=(язык)@!/(NOUN)@!%(inan,masc,sing,accs)@!=(язык)@!/(NOUN)@!%(inan,masc,sing,nomn)язык.@@@@.@!=(.)@!/(PNCT)..@@.@@\n"),
        "{atd}"
    );
    let tmp = tmp_dir("epimerismos-oc");
    let kanon = kanon_of(&tmp, "oc", &atd);
    let bare = kanon_of(
        &tmp,
        "bare",
        "@@@!litogramma\n\n@=Школа злословия=@\n\nШкола злословия учит прикусить язык.\n",
    );
    assert_eq!(litos_of(&kanon), litos_of(&bare));
    let out = epimerismos::export(&kanon, "opencorpora").unwrap().unwrap();
    assert_eq!(out, OPENCORPORA);
    let again = epimerismos::opencorpora_to_document(&out).unwrap();
    assert_eq!(dendron::serialize(&again), atd);
}

const PROIEL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<proiel schema-version="2.0">
<source id="atrep" language="und">
<title>Greek New Testament</title>
<div>
<title>Matthew</title>
<sentence id="1">
<token id="1" form="Βίβλος" citation-part="MATT 1.1" lemma="βίβλος" part-of-speech="Nb" morphology="-s---fn--i" head-id="2" relation="sub" presentation-after=" "/>
<token id="2" form="γενέσεως" citation-part="MATT 1.1" lemma="γένεσις" part-of-speech="Nb" morphology="-s---fg--i" relation="pred" presentation-after=" "/>
<token id="3" form="Ἰησοῦ" citation-part="MATT 1.1" lemma="Ἰησοῦς" part-of-speech="Ne" morphology="-s---mg--i" head-id="2" relation="atr" presentation-after="."/>
</sentence>
<sentence id="2">
<token id="4" form="Ἀβραὰμ" citation-part="MATT 1.2" lemma="Ἀβραάμ" part-of-speech="Ne" morphology="-s---mn--i" head-id="5" relation="sub" presentation-after=" "/>
<token id="5" form="ἐγέννησεν" citation-part="MATT 1.2" lemma="γεννάω" part-of-speech="V-" morphology="3siia----i" relation="pred" presentation-after="."/>
</sentence>
</div>
</source>
</proiel>
"#;

#[test]
fn proiel_imports_and_round_trips() {
    let doc = epimerismos::proiel_to_document(PROIEL).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@=Greek New Testament=@"), "{atd}");
    assert!(atd.contains("@# Matthew\n"), "{atd}");
    // Tokens carry onyms so heads can point at them; the citation
    // part opens the sentence as a core milestone; the source's
    // sentence id is the periodos.
    assert!(
        atd.contains("@@.@!.(1)@(\"proiel:MATT_1.1\")@@.@!=(βίβλος)@!/(Nb)@!%(-s---fn--i)@!>(t2)@!-(sub)Βίβλος.@@(t1) @@.@!=(γένεσις)@!/(Nb)@!%(-s---fg--i)@!-(pred)γενέσεως.@@(t2) "),
        "{atd}"
    );
    let tmp = tmp_dir("epimerismos-proiel");
    let kanon = kanon_of(&tmp, "proiel", &atd);
    let atk = dendron::serialize(&kanon);
    // Kanonizo keeps the token onyms that heads point at,
    // renumbered together with the heads, and drops the rest.
    assert!(atk.contains("@!>(o1)@!-(sub)Βίβλος.@@ @@.@!=(γένεσις)@!/(Nb)@!%(-s---fg--i)@!-(pred)γενέσεως.@@(o1) "), "{atk}");
    // The milestone is identity and survives litosis; the parsing
    // does not.
    let litos = litos_of(&kanon);
    assert!(litos.contains("proiel:MATT_1.1"), "{litos}");
    assert!(
        !litos.contains("@!.") && !litos.contains("@!=") && !litos.contains("@@."),
        "{litos}"
    );
    let out = epimerismos::export(&kanon, "proiel").unwrap().unwrap();
    assert_eq!(out, PROIEL);
    let again = epimerismos::proiel_to_document(&out).unwrap();
    assert_eq!(dendron::serialize(&again), atd);
}

#[test]
fn proiel_lemma_with_parentheses_survives() {
    // Parentheses nest inside a monosim parameter, so a lemma such
    // as PROIEL's οὕτω(ς) is carried verbatim through the document,
    // the kanon and the export instead of being dropped.
    let src = PROIEL.replace(r#"lemma="βίβλος""#, r#"lemma="οὕτω(ς)""#);
    let doc = epimerismos::proiel_to_document(&src).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@!=(οὕτω(ς))@!/(Nb)"), "{atd}");
    let tmp = tmp_dir("epimerismos-proiel-parens");
    let kanon = kanon_of(&tmp, "proiel-parens", &atd);
    let atk = dendron::serialize(&kanon);
    assert!(atk.contains("@!=(οὕτω(ς))@!/(Nb)"), "{atk}");
    let out = epimerismos::export(&kanon, "proiel").unwrap().unwrap();
    assert_eq!(out, src);
    // A lemma of two words is carried with its space, written
    // escaped, instead of being dropped.
    let src = PROIEL.replace(r#"lemma="βίβλος""#, r#"lemma="res publica""#);
    let doc = epimerismos::proiel_to_document(&src).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains(r"@!=(res\ publica)@!/(Nb)"), "{atd}");
    let kanon = kanon_of(&tmp, "proiel-space", &atd);
    let out = epimerismos::export(&kanon, "proiel").unwrap().unwrap();
    assert_eq!(out, src);
    // An unbalanced value still cannot be spelled and is dropped.
    let src = PROIEL.replace(r#"lemma="βίβλος""#, r#"lemma="οὕτω)""#);
    let atd = dendron::serialize(&epimerismos::proiel_to_document(&src).unwrap());
    assert!(atd.contains("@@.@!/(Nb)@!%(-s---fn--i)"), "{atd}");
}

const CONLLU: &str = "# sent_id = 1\n# text = Школа злословия учит.\n\
1\tШкола\tшкола\tNOUN\t_\tCase=Nom|Gender=Fem|Number=Sing\t3\tnsubj\t_\t_\n\
2\tзлословия\tзлословие\tNOUN\t_\tCase=Gen|Gender=Neut|Number=Sing\t1\tnmod\t_\t_\n\
3\tучит\tучить\tVERB\tVBZ\tMood=Ind|Number=Sing|Person=3\t0\troot\t_\tSpaceAfter=No\n\
4\t.\t.\tPUNCT\t_\t_\t3\tpunct\t_\t_\n\n";

#[test]
fn conllu_imports_and_round_trips() {
    let doc = epimerismos::conllu_to_document(CONLLU).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@@.@!.(1)@@.@!=(школа)@!/(NOUN)@!%(Case=Nom,Gender=Fem,Number=Sing)@!>(t1-3)@!-(nsubj)Школа.@@(t1-1) "),
        "{atd}"
    );
    assert!(
        atd.contains(
            "@@.@!=(учить)@!/(VERB/VBZ)@!%(Mood=Ind,Number=Sing,Person=3)@!-(root)учит.@@(t1-3)@@."
        ),
        "{atd}"
    );
    let tmp = tmp_dir("epimerismos-conllu");
    let kanon = kanon_of(&tmp, "ud", &atd);
    let bare = kanon_of(&tmp, "bare", "@@@!litogramma\n\nШкола злословия учит.\n");
    assert_eq!(litos_of(&kanon), litos_of(&bare));
    let out = epimerismos::export(&kanon, "conllu").unwrap().unwrap();
    assert_eq!(out, CONLLU);
    let again = epimerismos::conllu_to_document(&out).unwrap();
    assert_eq!(dendron::serialize(&again), atd);
}
