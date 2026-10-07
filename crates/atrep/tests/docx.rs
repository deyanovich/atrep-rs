//! Word import: a .docx becomes litogramma by its declared
//! structure. `tom-sawyer.docx` is pandoc's rendering of a
//! Markdown chapter (headings, lists, a quote, a footnote, a
//! table, an image, a link); `fields.docx` is hand-built OOXML
//! for what pandoc never writes: CITATION fields with a b:Sources
//! part, a BIBLIOGRAPHY field, an XE mark, a comment, gridSpan and
//! vMerge, tracked changes, a bookmark.

use std::path::{Path, PathBuf};

use atrep::{dendron, docx, kanonizo};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn pandoc_document_imports_as_litogramma() {
    let (doc, media) = docx::docx_to_document_with_media(&fixture("tom-sawyer.docx")).unwrap();
    assert_eq!(doc.dialect_id, "litogramma");
    let atd = dendron::serialize(&doc);
    // Front matter from the Title and Author styles.
    assert!(
        atd.contains("@=The Adventures of Tom Sawyer=@\n\n@=:Mark Twain:=@"),
        "{atd}"
    );
    // Headings by outline level, nested.
    assert!(atd.contains("@# Chapter I\n"), "{atd}");
    assert!(atd.contains("@## The fence\n"), "{atd}");
    assert!(atd.contains("##@(the-fence)\n#@(chapter-i)"), "{atd}");
    // Emphasis, strong, a footnote callout with its body at the end.
    assert!(atd.contains("No answer.@^(f1) The old lady"), "{atd}");
    assert!(
        atd.contains("@/She/@ seldom or @*never*@ looked @/through/@ them"),
        "{atd}"
    );
    assert!(
        atd.contains("@^\nA footnote on the answer.\n^@(f1)"),
        "{atd}"
    );
    // The quote style, lists nested by level, the declared header
    // row, the image and the link.
    assert!(
        atd.contains(
            "@\"\n\u{201c}Well, I lay if I get hold of you I\u{2019}ll\u{2014}\u{201d}\n\"@"
        ),
        "{atd}"
    );
    assert!(atd.contains("@..\n@.-(1)\nwhitewash\n-.@\n\n@.-(2)\nthe fence\n\n@--\n@-\nthirty yards\n-@\n\n@-\nnine feet high\n-@\n--@\n-.@\n..@"), "{atd}");
    assert!(
        atd.contains("@+\n@+=\n@+:\nBoy\n:+@\n\n@+:\nPrice\n:+@\n=+@"),
        "{atd}"
    );
    assert!(
        atd.contains("@+-\n@+:\nBen\n:+@\n\n@+:\nan apple\n:+@\n-+@"),
        "{atd}"
    );
    assert!(
        atd.contains("@<\n@@@@(media/rId10.png)\nThe fence\n<@"),
        "{atd}"
    );
    assert!(
        atd.contains("Gutenberg (@><https://www.gutenberg.org/ebooks/74><@)"),
        "{atd}"
    );
    assert_eq!(media.len(), 1);
    assert_eq!(media[0].name, "rId10.png");
    assert!(media[0].bytes.starts_with(b"\x89PNG"));
    // A valid document that kanonizes.
    let tmp = tmp_dir("docx-pandoc");
    std::fs::create_dir_all(tmp.join("media")).unwrap();
    std::fs::write(tmp.join("media/rId10.png"), &media[0].bytes).unwrap();
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
}

#[test]
fn word_fields_and_sources_import() {
    let doc = docx::docx_to_document(&fixture("fields.docx")).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@=On Fences and Empires=@\n\n@=:Mark Twain:=@"),
        "{atd}"
    );
    // A CITATION field is a cite, its cached result the span; a
    // locator-less one too; an unlisted tag stays a cite.
    assert!(
        atd.contains("Rome fell slowly @@.@>[(gibbon1776)|(Gibbon, 1776, p. 15).@@ and Persia was vast @@.@>[(herodotus)|(Herodotus).@@. Nobody found the lost volume @@.@>[(aristotle)|(Aristotle).@@."),
        "{atd}"
    );
    // The XE mark is a hidden index entry.
    assert!(atd.contains("@%%(Rome)"), "{atd}");
    // The comment is a manuscript note; the insertion is text,
    // the deletion gone; the bookmark an anchor the link refers to.
    assert!(
        atd.contains("thirty yards long@^!(c1) and nine feet high.@(fence)"),
        "{atd}"
    );
    assert!(!atd.contains("ten feet"), "{atd}");
    assert!(atd.contains("See the fence@>(fence) above."), "{atd}");
    assert!(
        atd.contains("@^!\nCheck the length against the first edition.\n!^@(c1)"),
        "{atd}"
    );
    // The table: the declared header row, a two-row vertical merge
    // and a two-column span, the merged-away cell omitted.
    assert!(
        atd.contains("@+=\n@+:\nBoy\n:+@\n\n@+:\nPrice\n:+@\n\n@+:\nNote\n:+@\n=+@"),
        "{atd}"
    );
    assert!(
        atd.contains("@+-\n@+:\n@+_(2) Ben\n:+@\n\n@+:\n@+>(2) an apple\n:+@\n-+@"),
        "{atd}"
    );
    assert!(
        atd.contains("@+-\n@+:\na kite\n:+@\n\n@+:\nlater\n:+@\n-+@"),
        "{atd}"
    );
    // The BIBLIOGRAPHY field's listing is dropped, its heading and
    // what follows stay.
    assert!(
        atd.contains("@# Bibliography\nAfter the listing.\n#@"),
        "{atd}"
    );
    assert!(!atd.contains("The Decline and Fall."), "{atd}");
    // The sources part is the bibliography.
    assert!(atd.contains("@@@!(bibliogramma)"), "{atd}");
    assert!(
        atd.contains("@& gibbon1776\n@: author\nGibbon, Edward\n:@\n\n@: title\nThe History of the Decline and Fall of the Roman Empire\n:@\n\n@: year\n1776\n:@\n\n@: location\nLondon\n:@\n\n@: publisher\nStrahan and Cadell\n:@\n&@.book"),
        "{atd}"
    );
    assert!(
        atd.contains("@& herodotus\n@: author\nHerodotus\n:@"),
        "{atd}"
    );
    assert!(atd.contains("@& austen1813\n"), "{atd}");
    let tmp = tmp_dir("docx-fields");
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
}

/// Run pandoc over a .docx when it is installed (an independent
/// reader of what we write); None when it is not.
fn pandoc_plain(path: &Path) -> Option<String> {
    let out = std::process::Command::new("pandoc")
        .arg(path)
        .args(["-t", "plain", "--wrap=none"])
        .output()
        .ok()?;
    assert!(
        out.status.success(),
        "pandoc rejected the document:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A litogramma document goes out as Word and comes back as the
/// same kanon: headings, lists, a quote, verse, notes of three
/// families, an index mark, cites with their bibliography, a table
/// with spans, an image with its caption, links and anchors.
#[test]
fn litogramma_round_trips_through_word() {
    let tmp = tmp_dir("docx-roundtrip");
    std::fs::create_dir_all(tmp.join("media")).unwrap();
    let png: Vec<u8> = {
        let doc_media = docx::docx_to_document_with_media(&fixture("tom-sawyer.docx"))
            .unwrap()
            .1;
        doc_media[0].bytes.clone()
    };
    std::fs::write(tmp.join("media/fence.png"), &png).unwrap();
    let atd = "\
@@@!litogramma

@=On Fences and Empires=@

@=:Mark Twain:=@

@# Empires
Rome fell slowly@^(f1) @@.@>[(gibbon1776)|(Gibbon, 1776, p. 15).@@ and Persia was @/vast/@ @>[(herodotus), see @>(fence) and @><https://www.gutenberg.org/ebooks/74><@.@%%(Rome)

@\"
Well, I lay if I get hold of you.
\"@

@## The fence
@(fence)The fence was @*thirty yards*@ long@^^^(e1) and nine feet high.@^!(c1)

@..
@.-(1)
whitewash
-.@

@.-(2)
the fence

@--
@-
thirty yards
-@

@-
nine feet high
-@
--@
-.@
..@

@+ Prices
@+=
@+:
Boy
:+@
@+:
Price
:+@
@+:
Note
:+@
=+@
@+-
@+:
@+_(2) Ben
:+@
@+:
@+>(2) an apple
:+@
-+@
@+-
@+:
a kite
:+@
@+:
later
:+@
-+@
+@

@<
@@@@(media/fence.png)
The fence
<@
##@
#@

@^
A footnote on Rome.
^@(f1)

@^^^
An endnote on the fence.
^^^@(e1)

@^!
Check the length against the first edition.
!^@(c1)

@@@!(bibliogramma)
@& gibbon1776
@: author
Gibbon, Edward
:@

@: title
The History of the Decline and Fall of the Roman Empire
:@

@: year
1776
:@
&@.book

@& herodotus
@: author
Herodotus
:@

@: title
The Histories
:@
&@.book
!@@@
";
    std::fs::write(tmp.join("doc.atd"), atd).unwrap();
    // Kanonizo renames media (media/fence.png -> media/m1.png) and
    // carries the bytes; the export reads them from there.
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let doc = result.document;
    let bytes = docx::document_to_docx(&doc, &|p| {
        result
            .media
            .iter()
            .find(|m| format!("media/{}", m.name) == p)
            .map(|m| m.bytes.clone())
    })
    .unwrap();
    let out = tmp.join("doc.docx");
    std::fs::write(&out, &bytes).unwrap();
    // An independent reader accepts it and sees the structure.
    if let Some(plain) = pandoc_plain(&out) {
        assert!(plain.contains("Rome fell slowly"), "{plain}");
        assert!(plain.contains("A footnote on Rome."), "{plain}");
        assert!(plain.contains("thirty yards"), "{plain}");
    }
    // Back through the importer: the same kanon.
    let (back, media) = docx::docx_to_document_with_media(&bytes).unwrap();
    assert_eq!(media.len(), 1);
    std::fs::create_dir_all(tmp.join("back/media")).unwrap();
    std::fs::write(tmp.join("back/media").join(&media[0].name), &media[0].bytes).unwrap();
    let back_path = tmp.join("back/doc.atd");
    std::fs::write(&back_path, dendron::serialize(&back)).unwrap();
    let back = kanonizo::kanonizo_file(&back_path).unwrap().document;
    assert_eq!(dendron::serialize(&back), dendron::serialize(&doc));
}

/// Build a minimal OOXML package around a body, for the fields a
/// reference manager writes.
fn minimal_docx(body: &str) -> Vec<u8> {
    use std::io::Write;
    let w = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {w}><w:body>{body}<w:sectPr/></w:body></w:document>"
    );
    let ct = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";
    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let opts = zip::write::SimpleFileOptions::default();
    for (name, data) in [
        ("[Content_Types].xml", ct.to_string()),
        ("_rels/.rels", rels.to_string()),
        ("word/document.xml", document),
    ] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn field(instr: &str, result: &str) -> String {
    format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>{result}</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        instr
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('"', "&quot;")
    )
}

/// Zotero's and Mendeley's CSL citation fields and EndNote's
/// EN.CITE field import as cites with their printed text as the
/// span, and the items they describe as bibliography entries;
/// their bibliography fields are dropped.
#[test]
fn reference_manager_fields_import() {
    let zotero = r#"ADDIN ZOTERO_ITEM CSL_CITATION {"citationID":"a1","properties":{"formattedCitation":"(Gibbon, 1776, p. 15)"},"citationItems":[{"id":12,"itemData":{"id":12,"type":"book","title":"The History of the Decline and Fall of the Roman Empire","author":[{"family":"Gibbon","given":"Edward"}],"issued":{"date-parts":[["1776"]]},"publisher":"Strahan and Cadell","publisher-place":"London","citation-key":"gibbon1776"},"locator":"15"}]}"#;
    let mendeley = r#"ADDIN CSL_CITATION {"citationItems":[{"id":"ITEM-1","itemData":{"id":"ITEM-1","type":"article-journal","title":"On the Histories","author":[{"family":"Herodotus"}],"container-title":"Hellenic Studies","volume":"3","page":"1-20","issued":{"date-parts":[[430]]}}}]}"#;
    let endnote = r#"ADDIN EN.CITE <EndNote><Cite><Author>Austen</Author><Year>1813</Year><RecNum>7</RecNum><record><rec-number>7</rec-number><ref-type name="Book">6</ref-type><contributors><authors><author>Austen, Jane</author></authors></contributors><titles><title>Pride and Prejudice</title></titles><dates><year>1813</year></dates><publisher>T. Egerton</publisher><pub-location>London</pub-location></record></Cite></EndNote>"#;
    let body = format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Rome fell slowly </w:t></w:r>{}<w:r><w:t xml:space=\"preserve\"> and Persia was vast </w:t></w:r>{}<w:r><w:t xml:space=\"preserve\">, unlike Bath </w:t></w:r>{}<w:r><w:t>.</w:t></w:r></w:p><w:p>{}</w:p>",
        field(zotero, "(Gibbon, 1776, p. 15)"),
        field(mendeley, "(Herodotus, 430)"),
        field(endnote, "(Austen, 1813)"),
        field(
            "ADDIN ZOTERO_BIBL {} CSL_BIBLIOGRAPHY",
            "Gibbon, E. (1776). The History."
        )
    );
    let doc = docx::docx_to_document(&minimal_docx(&body)).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("Rome fell slowly @@.@>[(gibbon1776)|(Gibbon, 1776, p. 15).@@ and Persia was vast @@.@>[(ITEM-1)|(Herodotus, 430).@@, unlike Bath @@.@>[(Austen1813)|(Austen, 1813).@@."),
        "{atd}"
    );
    assert!(!atd.contains("(1776). The History."), "{atd}");
    assert!(
        atd.contains("@& gibbon1776\n@: author\nGibbon, Edward\n:@\n\n@: title\nThe History of the Decline and Fall of the Roman Empire\n:@\n\n@: publisher\nStrahan and Cadell\n:@\n\n@: location\nLondon\n:@\n\n@: year\n1776\n:@\n&@.book"),
        "{atd}"
    );
    assert!(
        atd.contains("@& ITEM-1\n@: author\nHerodotus\n:@\n\n@: title\nOn the Histories\n:@\n\n@: journal\nHellenic Studies\n:@\n\n@: volume\n3\n:@\n\n@: pages\n1-20\n:@\n\n@: year\n430\n:@\n&@.article"),
        "{atd}"
    );
    assert!(
        atd.contains("@& Austen1813\n@: author\nAusten, Jane\n:@\n\n@: title\nPride and Prejudice\n:@\n\n@: year\n1813\n:@\n\n@: publisher\nT. Egerton\n:@\n\n@: location\nLondon\n:@\n&@.book"),
        "{atd}"
    );
    let tmp = tmp_dir("docx-managers");
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
}

/// A reference document supplies the styles of an export: its
/// styles part replaces the built-in set, the content is the same.
#[test]
fn reference_document_supplies_the_styles() {
    let reference = fixture("tom-sawyer.docx");
    let doc = docx::docx_to_document(&fixture("fields.docx")).unwrap();
    let plain = docx::document_to_docx(&doc, &|_| None).unwrap();
    let styled = docx::document_to_docx_with(&doc, &|_| None, Some(&reference)).unwrap();
    let part = |bytes: &[u8], name: &str| -> String {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        let mut s = String::new();
        zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
        s
    };
    let ref_styles = part(&reference, "word/styles.xml");
    assert_eq!(part(&styled, "word/styles.xml"), ref_styles);
    assert_ne!(part(&plain, "word/styles.xml"), ref_styles);
    assert_eq!(
        part(&styled, "word/document.xml"),
        part(&plain, "word/document.xml")
    );
    // pandoc's theme and font table ride along.
    assert!(part(&styled, "word/theme/theme1.xml").contains("a:theme"));
    assert!(part(&styled, "[Content_Types].xml").contains("theme+xml"));
    // The back-import reads the styled document the same.
    let back = docx::docx_to_document(&styled).unwrap();
    assert_eq!(
        dendron::serialize(&back),
        dendron::serialize(&docx::docx_to_document(&plain).unwrap())
    );
}
