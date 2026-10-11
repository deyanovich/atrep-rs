//! ABBYY Lingvo DSL import: a dictionary becomes lexigramma.
//! Fixtures after Webster's Revised Unabridged (1913) and Dahl's
//! Explanatory Dictionary (1863–66), both public domain, in the
//! DSL shape.

use std::path::{Path, PathBuf};

use atrep::{dendron, dsl, exo, kanonizo};

const WEBSTER: &str = "\
#NAME \"Webster's Revised Unabridged Dictionary (1913)\"
#INDEX_LANGUAGE \"English\"
#CONTENTS_LANGUAGE \"English\"
#INCLUDE \"abbrev.dsl\"

{{ Three entries after Webster 1913, in the DSL shape. }}
Culture
\t[m1][t]kŭl′tụr[/t] [s]culture.wav[/s][/m]
\t[m1][p]n.[/p] [p]Etym:[/p] [com][i]F. culture, L. cultura, fr. colere to till, cultivate; of uncertain origin. Cf. <<Cultivation>>.[/i][/com][/m]
\t[m1]1) The act or practice of cultivating, or of preparing the earth for seed and raising crops by tillage. [ex]the culture of the soil[/ex][/m]
\t[m1]2) The act of, or any labor or means employed for, training, disciplining, or refining the moral and intellectual nature of man. [ex]the culture of the mind[/ex][/m]
\t[m1]3) The state of being cultivated; result of cultivation; physical improvement; enlightenment and discipline acquired by mental and moral training; civilization; refinement in manners and taste.[/m]
\t[m1]4) [p]Biol.[/p][/m]
\t[m2]a) The cultivation of bacteria or other organisms in artificial media or under artificial conditions.[/m]
\t[m2]b) The collection of organisms resulting from such a cultivation.[/m]
\t[m1]5) [p]Cartography[/p] Those details of a map, collectively, which do not represent natural features of the area delineated, as names and the symbols for towns, roads, houses, bridges, meridians, and parallels.[/m]
\t[m1][*][b]~ fluid[/b], a fluid in which the germs of microscopic organisms are made to develop, either for purposes of study or as a means of modifying their virulence.[/*][/m]

Cultivation
\t[m1][t]kŭl′tĭ·vā′shŭn[/t][/m]
\t[m1][p]n.[/p] [p]Etym:[/p] [com][i]Cf. F. cultivation. Cf. <<Culture>>.[/i][/com][/m]
\t[m1]1) The art or act of cultivating; improvement for agricultural purposes or by agricultural processes; tillage; production by tillage.[/m]
\t[m1]2) Bestowal of time or attention for self-improvement or for the benefit of others; fostering care.[/m]
\t[m1]3) The state of being cultivated; advancement in physical, intellectual, or moral condition; refinement; culture. [ex]Italy . . . was but imperfectly reduced to cultivation before the irruption of the barbarians. — Hallam[/ex][/m]

{to} bank
bank
\t[m1][p]v. i.[/p][/m]
\t[m1]1) To keep a bank; to carry on the business of a banker.[/m]
\t[m1]2) To deposit money in a bank; to have an account with a banker. [ref]bank[/ref][/m]
\t@ bank up
\t[m1]To heap up; as, to bank up the fire.[/m]
\t@

bank
\t[m1][p]n.[/p] [p]Etym:[/p] [com][i]OE. banke; akin to E. bench.[/i][/com][/m]
\t[m1]1) A mound, pile, or ridge of earth, raised above the surrounding level. [ex]We walked along the river bank.[/ex] [url]https://www.gutenberg.org/ebooks/74[/url][/m]
\t[m1]2) A bench, as for rowers in a galley.[/m]
";

const ABBREV: &str = "\
a.
\tadjective
n.
\tnoun
v. i.
\tverb intransitive
Biol.
\tbiology
Etym:
\tetymology
";

const DAHL: &str = "\
#NAME \"Толковый словарь живого великорусского языка (Даль)\"
#INDEX_LANGUAGE \"Russian\"
#CONTENTS_LANGUAGE \"Russian\"

КН[']И[/']ГА
\t[m1][p]ж.[/p][/m]
\t[m1]сшитые в один переплет листы бумаги, или пергамента; писание, все что в книге содержится. [ex]Священная книга. Книга Бытия.[/ex][/m]
\t[m1]Книжка [p]умалит.[/p] [ex]Записная книжка.[/ex][/m]

П[']Е[/']РО
\t[m1][p]ср.[/p] [lang name=\"Latin\"]penna[/lang][/m]
\t[m1]1) птичье перо, роговой трубчатый стержень с опахалом.[/m]
\t[m1]2) орудие письма. [ex]Что написано пером, того не вырубишь топором.[/ex] См. <<КНИГА>>.[/m]
";

fn utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xff, 0xfe];
    for u in text.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn webster_imports_as_lexigramma() {
    let doc = dsl::dsl_to_document_with_abbreviations(&utf16le(WEBSTER), &utf16le(ABBREV)).unwrap();
    assert_eq!(doc.dialect_id, "lexigramma");
    let s = dendron::serialize(&doc);
    // Title and languages.
    assert!(
        s.contains("@=Webster's Revised Unabridged Dictionary (1913)=@"),
        "{s}"
    );
    assert!(s.contains("@=/en en/=@"), "{s}");
    // The abbreviations dictionary as the abbreviations sim over a
    // definition list.
    assert!(
        s.contains("@[[\n@::;\n@:: a.\n@;\nadjective\n;@\n::@"),
        "{s}"
    );
    assert!(
        s.contains("@:: v. i.\n@;\nverb intransitive\n;@\n::@"),
        "{s}"
    );
    assert!(
        s.contains("@:: Etym:\n@;\netymology\n;@\n::@\n;::@\n]]@"),
        "{s}"
    );
    // Labels open the first line: the grammar line; the etymology
    // label makes the comment the etymology, a reference inside it
    // still a reference.
    assert!(
        s.contains(
            "@! Culture\n@=%kŭl′tụr%=@\n\n@=&n.&=@\n\n@=<@/F. culture, L. cultura, fr. colere to till, cultivate; of uncertain origin. Cf. @>(Cultivation)./@>=@\n"
        ),
        "{s}"
    );
    // Numbered senses, letters nested under the arabic sense before
    // them, taxis positional; a sense that is only a label.
    assert!(
        s.contains("@:(1)\nThe act or practice of cultivating"),
        "{s}"
    );
    assert!(
        s.contains("@:(2)\nThe act of, or any labor or means employed for, training, disciplining, or refining the moral and intellectual nature of man. @~the culture of the mind~@\n:@\n\n@:(3)\n"),
        "{s}"
    );
    assert!(
        s.contains("@:(4)\n@[Biol.]@\n\n@:(1)\nThe cultivation of bacteria"),
        "{s}"
    );
    assert!(
        s.contains("@:(2)\nThe collection of organisms resulting from such a cultivation.\n:@\n:@\n\n@:(5)\n"),
        "{s}"
    );
    // A label in the text, untyped; a reference to another card,
    // in the running text of an etymology.
    assert!(
        s.contains("@:(5)\n@[Cartography]@ Those details of a map"),
        "{s}"
    );
    assert!(s.contains("Cf. @>(Culture)./@>=@\n"), "{s}");
    // The secondary text, a typed diaphane; ~ is the headword.
    assert!(
        s.contains(
            "@@.@*Culture fluid*@, a fluid in which the germs of microscopic organisms are made to develop, either for purposes of study or as a means of modifying their virulence..@@.secondary"
        ),
        "{s}"
    );
    // The sound file an enmedia block, before the senses.
    assert!(
        s.contains("@! Culture\n@=%kŭl′tụr%=@\n\n@=&n.&=@\n\n@=<@/F. culture, L. cultura, fr. colere to till, cultivate; of uncertain origin. Cf. @>(Cultivation)./@>=@\n@@@@(culture.wav)\n@:(1)\n"),
        "{s}"
    );
    assert!(
        s.contains("@! Cultivation\n@=%kŭl′tĭ·vā′shŭn%=@\n\n@=&n.&=@\n\n@=<@/Cf. F. cultivation. Cf. @>(Culture)./@>=@\n\n@:(1)\nThe art or act of cultivating"),
        "{s}"
    );
    // A braced headword: displayed whole, indexed by the rest (the
    // sort key); the second headword a spelling variant.
    assert!(
        s.contains("@! to bank\n@=*bank*=@.sort\n\n@=~bank~=@.spelling\n\n@=&v. i.&=@\n"),
        "{s}"
    );
    // A ref names the card that leads with that headword, not the
    // one that carries it as a sort key.
    assert!(s.contains("with a banker. @>(bank)\n"), "{s}");
    // The sub-card nests as an entry.
    assert!(
        s.contains("@! bank up\n@:(1)\nTo heap up; as, to bank up the fire.\n:@\n!@\n!@"),
        "{s}"
    );
    // Citation and link.
    assert!(
        s.contains("@~We walked along the river bank.~@ @><https://www.gutenberg.org/ebooks/74><@"),
        "{s}"
    );
}

#[test]
fn dahl_imports_with_stress_and_language() {
    let doc = dsl::dsl_to_document(&utf16le(DAHL)).unwrap();
    let s = dendron::serialize(&doc);
    assert!(s.contains("@=/ru ru/=@"), "{s}");
    // The stressed letter carries a combining acute.
    assert!(s.contains("@! КНИ\u{301}ГА\n@=&ж.&=@\n"), "{s}");
    // No numbering: one sense holds the body.
    assert!(s.contains("@:(1)\nсшитые в один переплет"), "{s}");
    assert!(
        s.contains("Книжка @[умалит.]@ @~Записная книжка.~@\n:@\n!@"),
        "{s}"
    );
    // A run in a named language is an equivalent in that language.
    assert!(s.contains("@=&ср.&=@\n\n@=>penna<=@.la\n"), "{s}");
    // A reference without the stress resolves to the stressed
    // headword's onym; so does one written with the stress tag.
    assert!(s.contains("См. @>(КНИ-ГА)."), "{s}");
    let tagged = DAHL.replace("<<КНИГА>>", "<<КНИ[']ГА>>");
    let s2 = dendron::serialize(&dsl::dsl_to_document(&utf16le(&tagged)).unwrap());
    assert!(s2.contains("См. @>(КНИ-ГА)."), "{s2}");
}

/// The decodings: UTF-16 by its mark, cp1251 by the header,
/// gzip (dictzip) unwrapped, all to the same document.
#[test]
fn dsl_decodings_agree() {
    let reference = dendron::serialize(&dsl::dsl_to_document(&utf16le(DAHL)).unwrap());
    let be: Vec<u8> = [0xfe, 0xff]
        .into_iter()
        .chain(DAHL.encode_utf16().flat_map(|u| u.to_be_bytes()))
        .collect();
    assert_eq!(
        dendron::serialize(&dsl::dsl_to_document(&be).unwrap()),
        reference
    );
    assert_eq!(
        dendron::serialize(&dsl::dsl_to_document(DAHL.as_bytes()).unwrap()),
        reference
    );
    // cp1251: every character of the fixture is in the code page.
    let cyr = format!("#SOURCE_CODE_PAGE \"Cyrillic\"\n{DAHL}");
    let bytes: Vec<u8> = cyr
        .chars()
        .map(|c| match c {
            '\u{0}'..='\u{7f}' => c as u8,
            'А'..='я' => 0xc0 + (c as u32 - 'А' as u32) as u8,
            'Ё' => 0xa8,
            'ё' => 0xb8,
            '—' => 0x97,
            other => panic!("not cp1251: {other:?}"),
        })
        .collect();
    assert_eq!(
        dendron::serialize(&dsl::dsl_to_document(&bytes).unwrap()),
        reference
    );
    #[cfg(feature = "bundle")]
    {
        use std::io::Write;
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&utf16le(DAHL)).unwrap();
        let dz = enc.finish().unwrap();
        assert_eq!(
            dendron::serialize(&dsl::dsl_to_document(&dz).unwrap()),
            reference
        );
    }
}

/// The import is a valid lexigramma document: it kanonizes
/// (senses validated, autonyms assigned) and exports through the
/// std exos.
#[test]
fn dsl_import_kanonizes_and_exports() {
    let tmp = tmp_dir("dsl-kanon");
    let doc = dsl::dsl_to_document_with_abbreviations(&utf16le(WEBSTER), &utf16le(ABBREV)).unwrap();
    let path = tmp.join("webster.atd");
    std::fs::write(&path, dendron::serialize(&doc)).unwrap();
    std::fs::write(tmp.join("culture.wav"), b"RIFF").unwrap();
    let kanon = kanonizo::kanonizo_file(&path).unwrap().document;
    let atk = dendron::serialize(&kanon);
    assert!(atk.contains("@! Culture\n"), "{atk}");
    let x = exo::resolve_exo(&tmp, "lexigramma", "kindle").unwrap();
    let html = exo::render(&kanon, &x, &tmp).unwrap();
    assert!(html.contains(r#"<idx:orth value="to bank">"#), "{html}");
    assert!(
        html.contains(r#"<idx:orth value="bank"></idx:orth>"#),
        "{html}"
    );
    assert!(html.contains("<span class=\"secondary\">"), "{html}");
    let x = exo::resolve_exo(&tmp, "lexigramma", "latex").unwrap();
    let tex = exo::render(&kanon, &x, &tmp).unwrap();
    assert!(tex.contains("kŭl′tụr"), "{tex}");
}

/// The labels an abbreviations list does not resolve are reported
/// (not errors): a free-text label is legitimate, the report lets
/// the maintainer complete the list. Without a list, nothing.
#[test]
fn unresolved_labels_are_reported() {
    use atrep::{dialektos, report};
    let doc = dsl::dsl_to_document_with_abbreviations(&utf16le(WEBSTER), &utf16le(ABBREV)).unwrap();
    let dial = dialektos::resolve(Path::new("."), "lexigramma").unwrap();
    // The fixture's list lacks one label, in an entry.
    let found = report::unresolved_labels(&doc, &dial);
    let cartography = report::UnresolvedLabel {
        label: "Cartography".to_string(),
        entry: Some("Culture".to_string()),
    };
    assert_eq!(found, vec![cartography.clone()]);
    // A label the list resolves, swapped for one it lacks: both
    // reported, in document order.
    let dsl = WEBSTER.replace("[p]Biol.[/p]", "[p]Rare.[/p]");
    let doc = dsl::dsl_to_document_with_abbreviations(&utf16le(&dsl), &utf16le(ABBREV)).unwrap();
    let found = report::unresolved_labels(&doc, &dial);
    assert_eq!(
        found,
        vec![
            report::UnresolvedLabel {
                label: "Rare.".to_string(),
                entry: Some("Culture".to_string()),
            },
            cartography,
        ]
    );
    // No list: no report.
    let doc = dsl::dsl_to_document(&utf16le(&dsl)).unwrap();
    assert!(report::unresolved_labels(&doc, &dial).is_empty());
}
