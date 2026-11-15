//! kolophon: the publication statement of a work as an embedded
//! block. The TEI and DocBook importers write it from the header,
//! at the document's end; every std exo a litogramma document can
//! reach renders it, and the TEI and DocBook exos place it by its
//! dialektos name where the format keeps such a statement.

use std::path::{Path, PathBuf};

use atrep::dendron::{self, Block, Document, Inline};
use atrep::{dialektos, endo, exo, kanonizo, morph};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn kanon_of(tmp: &Path, name: &str, source: &str) -> Document {
    let path = tmp.join(name);
    std::fs::write(&path, source).unwrap();
    kanonizo::kanonizo_file(&path).unwrap().document
}

/// What `atrep exo FILE.atd TARGET` does: resolve the export,
/// apply its route, render.
fn export(tmp: &Path, doc: &Document, target: &str) -> String {
    let (route, x) = exo::resolve_export(tmp, &doc.dialect_id, target, None).unwrap();
    if route.is_empty() {
        return exo::render(doc, &x, tmp).unwrap();
    }
    let mapped = morph::apply_route(doc, &route).unwrap();
    exo::render(&mapped, &x, tmp).unwrap()
}

/// The fields of the document's kolophon block, in order; `None`
/// when it has none. The block must be the document's last.
fn kolophon_of(doc: &Document) -> Option<Vec<(String, String)>> {
    let at = doc.blocks.iter().position(
        |b| matches!(b, Block::MonadEnglossis { dialect, .. } if dialect == "kolophon"),
    )?;
    assert_eq!(at, doc.blocks.len() - 1, "the colophon closes the document");
    let Block::MonadEnglossis { children, .. } = &doc.blocks[at] else {
        unreachable!()
    };
    let text = |inlines: &[Inline]| -> String {
        inlines
            .iter()
            .map(|i| match i {
                Inline::Text(t) => t.as_str(),
                _ => panic!("a colophon field is plain text"),
            })
            .collect()
    };
    Some(
        children
            .iter()
            .map(|b| match b {
                Block::Para {
                    symbol,
                    lemma,
                    children,
                    ..
                } => {
                    assert_eq!(symbol, ":");
                    let [Block::Paragraph(value)] = children.as_slice() else {
                        panic!("a colophon field holds one paragraph")
                    };
                    (text(lemma), text(value))
                }
                other => panic!("not a field: {other:?}"),
            })
            .collect(),
    )
}

fn fields(pairs: &[(&str, &str)]) -> Option<Vec<(String, String)>> {
    Some(
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
    )
}

const HISTORIES: &str = "\
@@@!litogramma

@=The Histories=@

@=:Herodotus:=@

@=== Clio
These are the researches of Herodotus of Halicarnassus.
===@

@@@!(kolophon)
@: officina
Clarendon Press
:@

@: locus
Oxford
:@

@: datum
1858
:@
!@@@
";

fn tei(publication: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <TEI xmlns=\"http://www.tei-c.org/ns/1.0\">\n\
         <teiHeader><fileDesc>\n\
         <titleStmt><title>The Histories</title><author>Herodotus</author></titleStmt>\n\
         <publicationStmt>{publication}</publicationStmt>\n\
         <sourceDesc><p>From the Rawlinson translation.</p></sourceDesc>\n\
         </fileDesc></teiHeader>\n\
         <text><body><div><head>Clio</head>\
         <p>These are the researches of Herodotus of Halicarnassus.</p></div></body></text>\n\
         </TEI>\n"
    )
}

/// The dialektos is in the standard library, and its vocabulary
/// is bibliogramma's: a field named in any covered language
/// kanonizes to the Latin term, and a lemma outside the
/// vocabulary stands as written.
#[test]
fn field_names_kanonize_to_the_latin_terms() {
    let tmp = tmp_dir("kolophon-vocabulary");
    dialektos::resolve(&tmp, "kolophon").unwrap();
    let doc = kanon_of(
        &tmp,
        "doc.atd",
        "@@@!litogramma\n\n@=The Histories=@\n\n\
         @@@!(kolophon)\n\
         @: publisher\nClarendon Press\n:@\n\n\
         @: место\nOxford\n:@\n\n\
         @: date\n1858\n:@\n\n\
         @: isbn\n0-19-814562-5\n:@\n\
         !@@@\n",
    );
    assert_eq!(
        kolophon_of(&doc),
        fields(&[
            ("officina", "Clarendon Press"),
            ("locus", "Oxford"),
            ("datum", "1858"),
            ("isbn", "0-19-814562-5"),
        ])
    );
}

/// A TEI publication statement's publisher, place and date are
/// the colophon, at the document's end; the rest of the header
/// reads as before.
#[test]
fn tei_publication_statement_is_the_colophon() {
    let doc = endo::tei_to_document(&tei(
        "<publisher>Clarendon Press</publisher><pubPlace>Oxford</pubPlace>\
         <date when=\"1858\">1858</date>",
    ))
    .unwrap();
    assert_eq!(
        kolophon_of(&doc),
        fields(&[
            ("officina", "Clarendon Press"),
            ("locus", "Oxford"),
            ("datum", "1858"),
        ])
    );
    let text = dendron::serialize(&doc);
    assert!(
        text.starts_with("@@@!litogramma\n\n@=The Histories=@\n\n@=:Herodotus:=@\n\n"),
        "{text}"
    );
    assert!(!text.contains("Rawlinson"), "{text}");
    // The imported document is a valid deltos: it kanonizes.
    let tmp = tmp_dir("kolophon-tei-import");
    let kanon = kanon_of(&tmp, "doc.atd", &text);
    assert_eq!(kolophon_of(&kanon), kolophon_of(&doc));
}

/// A publisher that wraps a name gives its text; a date with no
/// printed text gives its `when`, or its `from` and `to` as a
/// range; the availability and the identifiers are skipped.
#[test]
fn tei_statement_children_read_as_text() {
    let doc = endo::tei_to_document(&tei(
        "<publisher>\n <orgName>\n  Clarendon Press\n </orgName>\n</publisher>\
         <availability><p>Free to use.</p></availability>\
         <idno type=\"ISBN\">0-19-814562-5</idno>\
         <date from=\"1858\" to=\"1860\"/>",
    ))
    .unwrap();
    assert_eq!(
        kolophon_of(&doc),
        fields(&[
            ("officina", "Clarendon Press"),
            ("datum", "1858\u{2013}1860"),
        ])
    );
    let doc = endo::tei_to_document(&tei("<date when=\"1858-03\"/>")).unwrap();
    assert_eq!(kolophon_of(&doc), fields(&[("datum", "1858-03")]));
}

/// A statement in prose, as `atrep exo` itself writes it for a
/// document with no colophon, gives no block: such a file reads
/// as it did before the colophon was imported.
#[test]
fn tei_prose_statement_gives_no_colophon() {
    let doc = endo::tei_to_document(&tei("<p>Exported from an atrep document.</p>")).unwrap();
    assert_eq!(kolophon_of(&doc), None);
    assert!(!dendron::serialize(&doc).contains("kolophon"));
}

/// A DocBook root's info gives the colophon: the publisher's name,
/// its address and the date, in the colophon's order whatever the
/// info's. An info below the root gives none.
#[test]
fn docbook_info_is_the_colophon() {
    let doc = endo::docbook_to_document(
        "<article xmlns=\"http://docbook.org/ns/docbook\" version=\"5.0\">\n\
         <info><title>On the Sublime</title>\
         <author><personname>Longinus</personname>\
         <address><city>Athens</city></address></author>\
         <pubdate>1890</pubdate>\
         <publisher><publishername>Macmillan</publishername>\
         <address><city>London</city></address></publisher></info>\n\
         <section><info><title>First</title><pubdate>1891</pubdate></info>\
         <para>Sublimity is a certain distinction in expression.</para></section>\n\
         </article>\n",
    )
    .unwrap();
    assert_eq!(doc.dialect_id, "at-docbook");
    assert_eq!(
        kolophon_of(&doc),
        fields(&[
            ("officina", "Macmillan"),
            ("locus", "London"),
            ("datum", "1890"),
        ])
    );
    let text = dendron::serialize(&doc);
    assert!(
        text.starts_with("@@@!at-docbook\n\n@=On the Sublime=@\n\n"),
        "{text}"
    );
    assert!(!text.contains("1891") && !text.contains("Athens"), "{text}");
}

/// The TEI export draws the colophon into the header's publication
/// statement, wherever the block stands, and leaves nothing in the
/// body; the export note follows as the statement's availability.
/// The importer reads the statement back as the same colophon.
#[test]
fn tei_export_places_the_colophon_in_the_header() {
    let tmp = tmp_dir("kolophon-tei-export");
    let doc = kanon_of(&tmp, "doc.atd", HISTORIES);
    let out = export(&tmp, &doc, "tei");
    assert!(
        out.contains(
            "<publicationStmt>\n\
             <publisher>Clarendon Press</publisher>\n\
             <pubPlace>Oxford</pubPlace>\n\
             <date>1858</date>\n\
             <availability>\n\
             <p>Exported from an atrep document.</p>\n\
             </availability>\n\
             </publicationStmt>\n"
        ),
        "{out}"
    );
    let body = &out[out.find("<body>").unwrap()..];
    assert!(!body.contains("Clarendon"), "{out}");
    let back = endo::tei_to_document(&out).unwrap();
    assert_eq!(kolophon_of(&back), kolophon_of(&doc));
}

/// The same in the frame of a document with no title, and a field
/// outside the statement's elements is an idno typed with its
/// name.
#[test]
fn tei_export_of_a_titleless_document_and_a_free_field() {
    let tmp = tmp_dir("kolophon-tei-titleless");
    let doc = kanon_of(
        &tmp,
        "doc.atd",
        "@@@!litogramma\n\n\
         @# Clio\nThese are the researches of Herodotus.\n#@\n\n\
         @@@!(kolophon)\n\
         @: officina\nClarendon Press\n:@\n\n\
         @: isbn\n0-19-814562-5\n:@\n\
         !@@@\n",
    );
    let out = export(&tmp, &doc, "tei");
    assert!(
        out.contains(
            "<title/>\n</titleStmt>\n<publicationStmt>\n\
             <publisher>Clarendon Press</publisher>\n\
             <idno type=\"isbn\">0-19-814562-5</idno>\n\
             <availability>\n"
        ),
        "{out}"
    );
}

/// With no colophon the statement is the export note alone, as it
/// was: the header's bytes do not move.
#[test]
fn tei_export_without_a_colophon_is_unchanged() {
    let tmp = tmp_dir("kolophon-tei-none");
    let doc = kanon_of(
        &tmp,
        "doc.atd",
        "@@@!litogramma\n\n@=The Histories=@\n\nThese are the researches.\n",
    );
    let out = export(&tmp, &doc, "tei");
    assert!(
        out.contains(
            "<titleStmt>\n<title>The Histories</title>\n</titleStmt>\n\
             <publicationStmt>\n\
             <p>Exported from an atrep document.</p>\n\
             </publicationStmt>\n\
             <sourceDesc>\n"
        ),
        "{out}"
    );
}

/// DocBook keeps the statement in an info after the title.
#[test]
fn docbook_export_places_the_colophon_after_the_title() {
    let tmp = tmp_dir("kolophon-docbook-export");
    let doc = kanon_of(&tmp, "doc.atd", HISTORIES);
    let out = export(&tmp, &doc, "docbook");
    assert!(
        out.contains(
            "<title>The Histories</title>\n\
             <info>\n\
             <publishername>Clarendon Press</publishername>\n\
             <address><city>Oxford</city></address>\n\
             <pubdate>1858</pubdate>\n\
             </info>\n"
        ),
        "{out}"
    );
    assert_eq!(out.matches("Clarendon").count(), 1, "{out}");
    let back = endo::docbook_to_document(&out).unwrap();
    assert_eq!(kolophon_of(&back), kolophon_of(&doc));
}

/// Every other format a litogramma document exports to renders
/// the colophon where it stands, a field a line or a paragraph:
/// an imported document that gained a colophon still exports
/// everywhere it did.
#[test]
fn every_export_renders_the_colophon() {
    let tmp = tmp_dir("kolophon-exports");
    let doc = kanon_of(&tmp, "doc.atd", HISTORIES);
    for (target, expected) in [
        (
            "html",
            "<div class=\"englossis\" data-dialect=\"kolophon\">\n\
             <p class=\"officina\">Clarendon Press</p>\n\
             <p class=\"locus\">Oxford</p>\n\
             <p class=\"datum\">1858</p>\n\
             </div>",
        ),
        (
            "xhtml",
            "<div class=\"kolophon\">\n\
             <p class=\"officina\">Clarendon Press</p>\n\
             <p class=\"locus\">Oxford</p>\n\
             <p class=\"datum\">1858</p>\n\
             </div>",
        ),
        (
            "latex",
            "\\begin{center}\nClarendon Press\\par\nOxford\\par\n1858\\par\n\\end{center}",
        ),
        ("md", "\n\nClarendon Press\n\nOxford\n\n1858\n"),
        ("dj", "\n\nClarendon Press\n\nOxford\n\n1858\n"),
        ("org", "\n\nClarendon Press\n\nOxford\n\n1858\n"),
        ("rst", "\n\nClarendon Press\n\nOxford\n\n1858\n"),
        ("gemtext", "\n\nClarendon Press\n\nOxford\n\n1858\n"),
    ] {
        let out = export(&tmp, &doc, target);
        assert!(out.contains(expected), "{target}:\n{out}");
    }
}

/// A dictionary (the TEI Lex-0 path) takes no colophon: its
/// exos have no rule for one.
#[test]
fn tei_dictionary_takes_no_colophon() {
    let doc = endo::tei_to_document(
        "<TEI xmlns=\"http://www.tei-c.org/ns/1.0\">\n\
         <teiHeader><fileDesc><titleStmt><title>A Lexicon</title></titleStmt>\
         <publicationStmt><publisher>Clarendon Press</publisher></publicationStmt>\
         <sourceDesc><p>Print.</p></sourceDesc></fileDesc></teiHeader>\n\
         <text><body><entry xml:id=\"logos\"><form type=\"lemma\"><orth>logos</orth></form>\
         <sense><def>word, account</def></sense></entry></body></text>\n\
         </TEI>\n",
    )
    .unwrap();
    assert_eq!(doc.dialect_id, "lexigramma");
    assert!(!dendron::serialize(&doc).contains("kolophon"));
}
