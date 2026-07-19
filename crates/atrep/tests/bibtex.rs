//! The BibTeX endomorphosis into bibliogramma (the atrep-based
//! bibliography extension). Golden tests are definition-free;
//! the roundtrip fixed point runs in the litogramma repo where
//! bibliogramma.dia lives.

use atrep::{dendron, endo};

const SAMPLE_BIB: &str = r#"@article{pope1700,
  author = {Pope, Alexander},
  title = {Ode on Solitude},
  year = 1700,
  journal = "Juvenilia",
}

@comment{ignored entirely, {even nested} }

@book{austen1811,
  author = {Austen, Jane},
  title = {Sense and {S}ensibility},
  year = {1811}
}
"#;

#[test]
fn bibtex_endo_produces_bibliogramma() {
    let doc = endo::bibtex_to_document(SAMPLE_BIB).unwrap();
    assert_eq!(doc.dialect_id, "bibliogramma");
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@& pope1700\n"));
    assert!(atd.contains("&@.article"));
    assert!(atd.contains("@: author\nPope, Alexander\n:@"));
    assert!(atd.contains("@: year\n1700\n:@"));
    // Quoted values and bare numbers normalize the same way.
    assert!(atd.contains("@: journal\nJuvenilia\n:@"));
    // Inner braces are content (case protection), preserved.
    assert!(atd.contains("Sense and {S}ensibility"));
    assert!(atd.contains("&@.book"));
    assert!(!atd.contains("ignored"));
}

#[test]
fn bibtex_endo_is_strict_about_macros() {
    let err = endo::bibtex_to_document("@string{me = {Pope}}").unwrap_err();
    assert!(err.to_string().contains("@string"));
    let err = endo::bibtex_to_document("@article{k, title = jan # {x} }").unwrap_err();
    assert!(err.to_string().contains("out of subset"));
}
