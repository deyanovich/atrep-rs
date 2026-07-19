//! The tasso pass: the ordering subset of kanonizo — autonym
//! pinning, taxis sequencing, vocabulary normalization — with
//! onyms left as authored and no deixis validation.

use std::path::{Path, PathBuf};

use atrep::dendron::{Block, Taxis};
use atrep::{dialektos, kanonizo, parser};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const DIA: &str = "\
@@@!atrep

@=== entry
@![(taxis)] lemma
autonym
grammata
!@
@\"an entry\"@
===@

@=== section
@#[(taxis)] lemma
grammata
#@
@\"a section\"@
===@

@=== note
@^
grammata
^@
@\"a note; onymized on the episim, attached by deixis\"@
===@

@=== span
@/ grammata /@
@% genera
@\"a span\"@
===@

@==% genera
liber: book buch livre
%==@
";

const DOC: &str = "\
@@@!test

@#() First
Alpha @/x/@.book and a note@^(nope) here.
#@(intro)

@#() Second
Beta.
#@

@! Rose
A flower.
!@
";

fn setup(name: &str) -> PathBuf {
    let tmp = tmp_dir(name);
    std::fs::write(tmp.join("test.dia"), DIA).unwrap();
    tmp
}

#[test]
fn tasso_settles_without_canonicalizing() {
    let tmp = setup("tasso-settle");
    let doc_path = tmp.join("doc.atd");
    let mut doc = parser::parse_document(DOC, &doc_path).unwrap();
    let dial = dialektos::resolve(&tmp, "test").unwrap();
    kanonizo::tasso(&mut doc, &dial, &tmp).unwrap();

    // Sibling-run taxis sequenced in place.
    let taxes: Vec<Option<Taxis>> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Para { symbol, taxis, .. } if symbol == "#" => Some(*taxis),
            _ => None,
        })
        .collect();
    assert_eq!(
        taxes,
        vec![Some(Taxis::Explicit(1)), Some(Taxis::Explicit(2))]
    );

    // The authored onym survives untouched (no o1 renumbering);
    // the autonym entry pinned its lemma as its onym.
    let onyms: Vec<Option<&str>> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Para { symbol, ann, .. } => {
                Some((symbol.as_str(), ann.onym.as_deref()))
            }
            _ => None,
        })
        .map(|(_, o)| o)
        .collect();
    assert_eq!(onyms, vec![Some("intro"), None, Some("Rose")]);

    // The genos alias normalized against the vocabulary.
    let serialized = atrep::dendron::serialize(&doc);
    assert!(serialized.contains("@/x/@.liber"));

    // The dangling deixis was tolerated: tasso does not validate
    // deixes, while full kanonizo rejects the same document.
    std::fs::write(&doc_path, DOC).unwrap();
    assert!(kanonizo::kanonizo_file(&doc_path).is_err());
}
