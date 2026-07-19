//! The JATS endomorphosis: scholarly articles into litogramma
//! with references as an embedded bibliogramma englossis.
//! Definition-free golden test; the PDF pipeline runs in the
//! litogramma repo.

use atrep::{dendron, endo};

const SAMPLE: &str = r#"<article xmlns:xlink="http://www.w3.org/1999/xlink">
<front><article-meta>
<title-group><article-title>Solitude</article-title></title-group>
<contrib-group><contrib><name><surname>Careful</surname>
<given-names>A.</given-names></name></contrib></contrib-group>
<abstract><p>The program.</p></abstract>
</article-meta></front>
<body>
<sec><title>Claim</title>
<p>The ode <xref ref-type="bibr" rid="pope1700">[1]</xref> holds
the <italic>program</italic>.<fn><p>Early.</p></fn></p>
<sec><title>Inner</title><p>Nested <bold>prose</bold>.</p></sec>
</sec>
</body>
<back><ref-list>
<ref id="pope1700"><element-citation publication-type="journal">
<person-group><name><surname>Pope</surname>
<given-names>Alexander</given-names></name></person-group>
<article-title>Ode on Solitude</article-title>
<source>Juvenilia</source><year>1700</year>
</element-citation></ref>
</ref-list></back>
</article>
"#;

#[test]
fn jats_endo_produces_litogramma_with_bibliogramma() {
    let doc = endo::jats_to_document(SAMPLE).unwrap();
    assert_eq!(doc.dialect_id, "litogramma");
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@=Solitude=@"));
    assert!(atd.contains("@=:A. Careful:=@"));
    assert!(atd.contains("@# Claim\n"));
    assert!(atd.contains("@## Inner\n"));
    assert!(atd.contains("@>[(pope1700) holds"));
    assert!(atd.contains("@^(n1)"));
    assert!(atd.contains("@^\nEarly.\n^@(n1)"));
    // References embed as a bibliogramma englossis block.
    assert!(atd.contains("@@@!(bibliogramma)"));
    assert!(atd.contains("@& pope1700\n"));
    assert!(atd.contains("@: author\nPope, Alexander\n:@"));
    assert!(atd.contains("@: journal\nJuvenilia\n:@"));
    assert!(atd.contains("&@.article"));
}
