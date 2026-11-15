//! at-tei: the genos-grouped TEI syntax mapper. Form sims carry
//! structure; TEI element identity rides vocabulary genoses.

use std::path::{Path, PathBuf};

use atrep::{exo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn at_tei_renders_real_tei() {
    let tmp = tmp_dir("attei-render");
    let doc = "\
@@@!at-tei

@_
@= title
On Solitude
=@
_@.teiheader

@_
@#(1) The Claim
Pope@^(n1) in @,otium,@.foreign, so @,he,@.persname
@,said,@.socalled.

@^
Early.
^@(n1).foot

@--
@- gloss-label
the gloss text
-@
--@.gloss
#@.chapter
_@.text
";
    std::fs::write(tmp.join("doc.atd"), doc).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "at-tei", "tei").unwrap();
    let tei = exo::render(&kanon.document, &x, &tmp).unwrap();
    assert!(tei.contains("<teiHeader>"));
    assert!(tei.contains("<title><p>On Solitude</p></title>"));
    assert!(tei.contains("<div type=\"chapter\" n=\"1\">\n<head>The Claim</head>"));
    // camelCase restored from lowercased genoses.
    assert!(tei.contains("<persName>he</persName>"));
    assert!(tei.contains("<soCalled>said</soCalled>"));
    assert!(tei.contains("<foreign>otium</foreign>"));
    // The deixis/note pair becomes ptr/note with matching ids.
    assert!(tei.contains("<ptr target=\"#o1\"/>"));
    assert!(tei.contains("<note xml:id=\"o1\" place=\"foot\">"));
    assert!(tei.contains("<list type=\"gloss\">"));
    assert!(tei.contains("<label>gloss-label</label>"));
}

/// A document frames itself when it holds a text block, and is
/// framed when it holds none: either way one header, one text.
#[test]
fn text_and_body_are_written_once() {
    let tmp = tmp_dir("attei-frame");
    let x = exo::resolve_exo(&tmp, "at-tei", "tei").unwrap();
    let render = |name: &str, doc: &str| {
        std::fs::write(tmp.join(name), doc).unwrap();
        let kanon = kanonizo::kanonizo_file(&tmp.join(name)).unwrap();
        exo::render(&kanon.document, &x, &tmp).unwrap()
    };

    // No text block: the document is all body.
    let bare = render(
        "bare.atd",
        "@@@!at-tei\n\n@#(1) The Claim\nInside.\n#@.chapter\n",
    );
    assert!(bare.contains(
        "<titleStmt>\n<title/>\n</titleStmt>\n\
         <publicationStmt>\n<p>Exported from an atrep document.</p>\n</publicationStmt>"
    ));
    assert!(bare.contains(
        "</teiHeader>\n<text>\n<body>\n\
         <div type=\"chapter\" n=\"1\">\n<head>The Claim</head>\n<p>Inside.</p>\n</div>\n\
         </body>\n</text>\n</TEI>"
    ));

    // A text block with a lemma writes its own header and frame.
    let titled = render(
        "titled.atd",
        "@@@!at-tei\n\n@_ On Solitude\nInside.\n_@.text\n",
    );
    assert!(titled.contains("<title>On Solitude</title>"));
    assert!(!titled.contains("<title/>"));

    for tei in [&bare, &titled] {
        assert_eq!(tei.matches("<teiHeader>").count(), 1);
        assert_eq!(tei.matches("<text>").count(), 1);
        assert_eq!(tei.matches("<body>").count(), 1);
    }
}

/// Unknown genoses ride the fallbacks instead of failing.
#[test]
fn free_genoses_survive() {
    let tmp = tmp_dir("attei-free");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!at-tei\n\n@_\nInside.\n_@.theorema\n\n@,styled,@.glitter\n",
    )
    .unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "at-tei", "tei").unwrap();
    let tei = exo::render(&kanon.document, &x, &tmp).unwrap();
    assert!(tei.contains("<ab type=\"theorema\">"));
    assert!(tei.contains("<seg type=\"glitter\">styled</seg>"));
}
