//! TEI Lex-0 import: dictionary TEI becomes lexigramma.

const XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<TEI xmlns="http://www.tei-c.org/ns/1.0">
  <teiHeader><fileDesc><titleStmt><title>T</title></titleStmt></fileDesc></teiHeader>
  <text><body>
    <entry xml:id="bank1">
      <form type="lemma"><orth>bank</orth><pron notation="ipa">bæŋk</pron>
        <form type="inflected"><orth>banks</orth></form></form>
      <gramGrp><gram type="pos">noun</gram></gramGrp>
      <etym>From Old Norse <mentioned>bakki</mentioned></etym>
      <sense n="1">
        <def>The land alongside a river.</def>
        <cit type="example"><quote>the river bank</quote><bibl>Twain</bibl></cit>
        <cit type="translationEquivalent"><quote xml:lang="de">Ufer</quote></cit>
      </sense>
      <sense n="2">
        <def><usg type="geo">Brit.</usg> A slope.</def>
        <sense n="1"><def>An incline.</def></sense>
      </sense>
      <xr type="cf"><ref target="#bank2">bank</ref></xr>
    </entry>
    <entry xml:id="bank2">
      <form type="lemma"><orth>bank</orth></form>
      <sense n="1"><def>A financial establishment.</def></sense>
    </entry>
  </body></text>
</TEI>"##;

#[test]
fn lex0_imports_as_lexigramma() {
    let doc = atrep::endo::tei_to_document(XML).unwrap();
    assert_eq!(doc.dialect_id, "lexigramma");
    let s = atrep::dendron::serialize(&doc);
    // Homograph taxis assigned per lemma.
    assert!(s.contains("@!(1) bank"), "{s}");
    assert!(s.contains("@!(2) bank"), "{s}");
    // The form block, grammar and etymology.
    assert!(s.contains("@=%bæŋk%=@.ipa"), "{s}");
    // Inflected form embedded as a lookup key, not a variant.
    assert!(s.contains("@=*banks*=@"), "{s}");
    assert!(s.contains("@=&noun&=@"), "{s}");
    assert!(s.contains("@=<From Old Norse @~bakki~@>=@"), "{s}");
    // Senses with positional taxis, nested.
    assert!(s.contains("@:(1)"), "{s}");
    assert!(s.contains("@:(2)"), "{s}");
    // Citation with author annotation; equivalent with language.
    assert!(s.contains("@~the river bank~@ @,Twain,@.author"), "{s}");
    assert!(s.contains("@=>Ufer<=@.de"), "{s}");
    // Usage label with mapped genos.
    assert!(s.contains("@[Brit.]@.geo"), "{s}");
    // Cross-reference resolved from xml:id to the autonym.
    assert!(s.contains("@>(bank-2).cf"), "{s}");
}

#[test]
fn non_dictionary_tei_stays_literary() {
    let xml = r#"<TEI xmlns="http://www.tei-c.org/ns/1.0">
      <teiHeader><fileDesc><titleStmt><title>T</title></titleStmt></fileDesc></teiHeader>
      <text><body><div><head>One</head><p>Prose.</p></div></body></text></TEI>"#;
    let doc = atrep::endo::tei_to_document(xml).unwrap();
    assert_eq!(doc.dialect_id, "litogramma");
}
