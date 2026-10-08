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
Liberal
\t[m1][t]lĭb′ẽr·al[/t][/m]
\t[m1][p]a.[/p] [p]Etym:[/p] [com][i]F. libéral, L. liberalis, fr. liber free.[/i][/com][/m]
\t[m1]1) Free by birth; hence, befitting a freeman or gentleman; refined; noble. [ex]a liberal ancestry[/ex][/m]
\t[m1]2) Bestowing in a large and noble way, as a freeman; generous; bounteous. [ex]a liberal giver[/ex][/m]
\t[m2]a) Not strict or rigorous; free; as, a liberal translation of a classic.[/m]
\t[m2]b) Not narrow or contracted in mind; catholic.[/m]
\t[m1]3) Free to excess; licentious. [p]Obs.[/p][/m]
\t[m1]4) Not bound by orthodox tenets; independent in opinion. See <<Liberalism>>.[/m]
\t[m1][*][b]Syn.[/b] — Generous; bountiful; munificent; ample; profuse; free.[/*][/m]

Liberalism
\t[m1][t]lĭb′ẽr·al·ĭz’m[/t][/m]
\t[m1][p]n.[/p][/m]
\t[m1]Liberal principles; the principles and methods of the liberals in politics or religion; specifically, the principles of the ~ party. [s]liberalism.wav[/s][/m]

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
Obs.
\tobsolete
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
    // label makes the comment the etymology.
    assert!(
        s.contains(
            "@! Liberal\n@=%lĭb′ẽr·al%=@\n\n@=&a.&=@\n\n@=<@/F. libéral, L. liberalis, fr. liber free./@>=@\n"
        ),
        "{s}"
    );
    // Numbered senses, letters nested under the arabic sense before
    // them, taxis positional.
    assert!(s.contains("@:(1)\nFree by birth"), "{s}");
    assert!(
        s.contains("@:(2)\nBestowing in a large and noble way, as a freeman; generous; bounteous. @~a liberal giver~@\n\n@:(1)\nNot strict"),
        "{s}"
    );
    assert!(
        s.contains("@:(2)\nNot narrow or contracted in mind; catholic.\n:@\n:@\n\n@:(3)\n"),
        "{s}"
    );
    // A label in the text, untyped; a reference to another card.
    assert!(s.contains("licentious. @[Obs.]@\n"), "{s}");
    assert!(s.contains("See @>(Liberalism).\n"), "{s}");
    // The secondary text, a typed diaphane.
    assert!(
        s.contains(
            "@@.@*Syn.*@ — Generous; bountiful; munificent; ample; profuse; free..@@.secondary"
        ),
        "{s}"
    );
    // A card with no numbering is one sense; ~ is the headword;
    // the sound file an enmedia block.
    assert!(
        s.contains("@! Liberalism\n@=%lĭb′ẽr·al·ĭz’m%=@\n\n@=&n.&=@\n@@@@(liberalism.wav)\n@:(1)\nLiberal principles"),
        "{s}"
    );
    assert!(s.contains("the principles of the Liberalism party."), "{s}");
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
    std::fs::write(tmp.join("liberalism.wav"), b"RIFF").unwrap();
    let kanon = kanonizo::kanonizo_file(&path).unwrap().document;
    let atk = dendron::serialize(&kanon);
    assert!(atk.contains("@! Liberal\n"), "{atk}");
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
    assert!(tex.contains("lĭb′ẽr·al"), "{tex}");
}

/// The labels an abbreviations list does not resolve are reported
/// (not errors): a free-text label is legitimate, the report lets
/// the maintainer complete the list. Without a list, nothing.
#[test]
fn unresolved_labels_are_reported() {
    use atrep::{dialektos, report};
    let doc = dsl::dsl_to_document_with_abbreviations(&utf16le(WEBSTER), &utf16le(ABBREV)).unwrap();
    let dial = dialektos::resolve(Path::new("."), "lexigramma").unwrap();
    let found = report::unresolved_labels(&doc, &dial);
    assert!(found.is_empty(), "{found:?}");
    // A label the list lacks, in an entry.
    let dsl = WEBSTER.replace("[p]Obs.[/p]", "[p]Rare.[/p]");
    let doc = dsl::dsl_to_document_with_abbreviations(&utf16le(&dsl), &utf16le(ABBREV)).unwrap();
    let found = report::unresolved_labels(&doc, &dial);
    assert_eq!(
        found,
        vec![report::UnresolvedLabel {
            label: "Rare.".to_string(),
            entry: Some("Liberal".to_string()),
        }]
    );
    // No list: no report.
    let doc = dsl::dsl_to_document(&utf16le(&dsl)).unwrap();
    assert!(report::unresolved_labels(&doc, &dial).is_empty());
}
