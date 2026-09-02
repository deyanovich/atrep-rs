//! at-docbook: the first nested std dialektos. Import golden,
//! canonical fixed point, and nesting preserved end to end.

use std::path::{Path, PathBuf};

use atrep::{dendron, endo, exo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SAMPLE_DOCBOOK: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<article xmlns="http://docbook.org/ns/docbook" version="5.0">
<section>
<title>Solitude</title>
<para>Pope praised <emphasis>rural quiet</emphasis> and
<emphasis role="strong">self-sufficiency</emphasis>, noting
<literal>x &lt; y</literal> along the way.<footnote>
<para>Horace above all.</para>
</footnote></para>
<section>
<title>Sources</title>
<itemizedlist>
<listitem><para>the Odes</para></listitem>
<listitem><para>the Epistles</para></listitem>
</itemizedlist>
<blockquote>
<para>Happy the man, whose wish and care.</para>
</blockquote>
<note>
<para>Mind the gap.</para>
</note>
<programlisting language="rust">fn quiet() {}</programlisting>
</section>
</section>
</article>
"#;

#[test]
fn docbook_endo_preserves_nesting() {
    let doc = endo::docbook_to_document(SAMPLE_DOCBOOK).unwrap();
    assert_eq!(doc.dialect_id, "at-docbook");
    let atd = dendron::serialize(&doc);
    // The subsection nests inside its parent section: the inner
    // section closes before the outer one.
    let outer = atd.find("@# Solitude").unwrap();
    let inner = atd.find("@# Sources").unwrap();
    assert!(inner > outer);
    assert_eq!(atd.matches("\n#@").count(), 2);
    assert!(atd.contains("@/rural quiet/@"));
    assert!(atd.contains("@*self-sufficiency*@"));
    assert!(atd.contains("@^(n1)"));
    assert!(atd.contains("@^\nHorace above all.\n^@(n1)"));
    assert!(atd.contains("@!\nMind the gap.\n!@"));
    assert!(atd.contains("fn quiet() {}"));
}

/// Canonical DocBook is a fixed point of export then import.
#[test]
fn docbook_roundtrip_is_idempotent() {
    let tmp = tmp_dir("docbook-roundtrip");
    let cycle = |xml: &str| -> String {
        let doc = endo::docbook_to_document(xml).unwrap();
        std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
        let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
        let x = exo::resolve_exo(&tmp, "at-docbook", "docbook").unwrap();
        exo::render(&kanon.document, &x, &tmp).unwrap()
    };
    let x1 = cycle(SAMPLE_DOCBOOK);
    let x2 = cycle(&x1);
    assert_eq!(x1, x2);
    assert!(x1.contains("<footnoteref linkend=\"o1\"/>"));
    assert!(x1.contains("<footnote xml:id=\"o1\">"));
    assert!(x1.contains("<section>\n<title>Sources</title>"));
}

/// Inline quotations and links: quote maps onto the koine
/// quotation sim; a link whose text is its target (or none, or
/// DocBook 4's ulink) is the visible-URL link sim exactly, a
/// hidden href projects as prose with the URL beside it, and an
/// internal linkend keeps its text. The export re-emits the link
/// with the xlink namespace declared on the root.
#[test]
fn docbook_quotes_and_links() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<article xmlns="http://docbook.org/ns/docbook" xmlns:xlink="http://www.w3.org/1999/xlink" version="5.0">
<para>He said <quote>enough</quote>; see <link xlink:href="https://example.org/d">the docs</link>,
<link xlink:href="https://example.org/x">https://example.org/x</link>, <link xlink:href="https://example.org/y"/>,
<ulink url="https://example.org/z"/> and <link linkend="s1">section one</link>.</para>
</article>
"#;
    let doc = endo::docbook_to_document(xml).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains(
            "He said @\"\"enough\"\"@; see the docs (@><https://example.org/d><@), \
             @><https://example.org/x><@, @><https://example.org/y><@, \
             @><https://example.org/z><@ and section one."
        ),
        "{atd}"
    );
    let tmp = tmp_dir("docbook-links");
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "at-docbook", "docbook").unwrap();
    let out = exo::render(&kanon.document, &x, &tmp).unwrap();
    assert!(
        out.contains("xmlns:xlink=\"http://www.w3.org/1999/xlink\""),
        "{out}"
    );
    assert!(
        out.contains("<link xlink:href=\"https://example.org/x\">https://example.org/x</link>"),
        "{out}"
    );
    assert!(out.contains("<quote>enough</quote>"), "{out}");
    // Fixed point: the export re-imports to the same document.
    let again = dendron::serialize(&endo::docbook_to_document(&out).unwrap());
    let kanon2 = {
        std::fs::write(tmp.join("doc2.atd"), &again).unwrap();
        kanonizo::kanonizo_file(&tmp.join("doc2.atd")).unwrap()
    };
    assert_eq!(out, exo::render(&kanon2.document, &x, &tmp).unwrap());
}

/// The root's title, bare or inside <info>, is the document
/// title standing alone as the first paragraph; a title no
/// container claims (a blockquote caption) settles as a plain
/// paragraph instead of leaking the sentinel. The export emits
/// <title> first inside the root, a fixed point, and the title
/// crosses into litogramma by identity.
#[test]
fn docbook_root_title() {
    let bare = r#"<article xmlns="http://docbook.org/ns/docbook" version="5.0">
<title>Solitude</title>
<para>Plain.</para>
<blockquote><title>Caption</title><para>Quoted.</para></blockquote>
</article>
"#;
    let doc = endo::docbook_to_document(bare).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.starts_with("@@@!at-docbook\n\n@=Solitude=@\n\nPlain.\n"),
        "{atd}"
    );
    assert!(atd.contains("Caption\n\nQuoted."), "{atd}");
    assert!(!atd.contains('\u{0}'), "{atd}");
    let info = r#"<book xmlns="http://docbook.org/ns/docbook" version="5.0">
<info><title>Solitude</title><author><personname>Pope</personname></author></info>
<para>Plain.</para>
</book>
"#;
    let atd2 = dendron::serialize(&endo::docbook_to_document(info).unwrap());
    assert!(
        atd2.starts_with("@@@!at-docbook\n\n@=Solitude=@\n\nPlain.\n"),
        "{atd2}"
    );
    let tmp = tmp_dir("docbook-title");
    let cycle = |xml: &str| -> String {
        let doc = endo::docbook_to_document(xml).unwrap();
        std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
        let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
        let x = exo::resolve_exo(&tmp, "at-docbook", "docbook").unwrap();
        exo::render(&kanon.document, &x, &tmp).unwrap()
    };
    let x1 = cycle(bare);
    assert!(
        x1.contains("version=\"5.0\">\n<title>Solitude</title>\n<para>Plain.</para>"),
        "{x1}"
    );
    assert_eq!(x1, cycle(&x1));
    // Into litogramma: the title maps by identity.
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let m = atrep::morph::resolve_morph(&tmp, "at-docbook", "litogramma").unwrap();
    let out = dendron::serialize(&atrep::morph::apply(&kanon.document, &m).unwrap());
    assert!(out.contains("@=Solitude=@"), "{out}");
}
