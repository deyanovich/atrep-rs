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
