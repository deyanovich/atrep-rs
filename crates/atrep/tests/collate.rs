//! Collation: the apparatus criticus derived from a witness
//! weave (zygosis slicing + word-level diff).

use atrep::dendron::{Annotations, Block, Document, Inline, serialize};
use atrep::zygosis::collation;

fn milestone(value: &str) -> Inline {
    Inline::Milestone {
        scheme: "steph".into(),
        value: value.into(),
        ann: Annotations {
            onym: None,
            genoses: Vec::new(),
        },
    }
}

fn doc(text_a: &str, text_b: &str) -> Document {
    Document {
        dialect_id: "litogramma".into(),
        dialect_version: None,
        blocks: vec![Block::Paragraph(vec![
            milestone("1a"),
            Inline::Text(text_a.into()),
            milestone("1b"),
            Inline::Text(text_b.into()),
        ])],
    }
}

#[test]
fn collation_derives_replacement_omission_and_addition() {
    let base = doc("the quick brown fox jumps", "over the lazy dog tonight");
    let w2 = doc("the quick red fox jumps", "over the lazy dog cat tonight");
    let w3 = doc("the quick brown fox", "over the lazy dog tonight");
    let out = collation(
        &[("B".into(), base), ("T".into(), w2), ("W".into(), w3)],
        "steph",
        "B",
    )
    .unwrap();
    let s = serialize(&out);
    // Replacement, keyed by segment, bold key = first token.
    assert!(s.contains("1a brown B : red T"), "{s}");
    // Omission at segment end.
    assert!(s.contains("1a jumps B : om. W"), "{s}");
    // Addition anchored after the preceding word.
    assert!(s.contains("1b post dog add. cat T"), "{s}");
    // Deixes sit after their lemma words in the base text.
    assert!(s.contains("brown@^!(app-1a-1)"), "{s}");
    assert!(s.contains("jumps@^!(app-1a-2)"), "{s}");
    assert!(s.contains("dog@^!(app-1b-1)"), "{s}");
    // Note bodies follow the block that anchors them.
    assert!(s.contains("@^!\n1a brown B : red T\n!^@(app-1a-1)"), "{s}");
}

#[test]
fn collation_requires_a_known_base() {
    let a = doc("x", "y");
    let b = doc("x", "y");
    let err = collation(&[("B".into(), a), ("T".into(), b)], "steph", "Z");
    assert!(err.is_err());
}

#[test]
fn collation_validates_sigla_and_dialektos() {
    let a = doc("x", "y");
    let b = doc("x", "y");
    // Duplicate siglum.
    let err = collation(
        &[("B".into(), a.clone()), ("B".into(), b.clone())],
        "steph",
        "B",
    );
    assert!(format!("{}", err.unwrap_err()).contains("duplicate witness siglum"));
    // Whitespace in a siglum.
    let err = collation(
        &[("B 1".into(), a.clone()), ("T".into(), b.clone())],
        "steph",
        "T",
    );
    assert!(format!("{}", err.unwrap_err()).contains("not a valid siglum"));
    // Uppercase manuscript capitals are welcome (free sigla,
    // spec v0.12.1) - this invocation validates.
    assert!(
        collation(
            &[("B".into(), a.clone()), ("T".into(), b.clone())],
            "steph",
            "B"
        )
        .is_ok()
    );
    // One dialektos per collation.
    let mut c = doc("x", "y");
    c.dialect_id = "koine".into();
    let err = collation(&[("B".into(), a), ("T".into(), c)], "steph", "B");
    assert!(format!("{}", err.unwrap_err()).contains("one dialektos per collation"));
}

#[test]
fn collation_rejects_contradictory_coordinate_order() {
    // Alignment is that of zygosis: shared coordinates must
    // merge order-consistently.
    let base = doc("alpha", "beta");
    let flipped = Document {
        dialect_id: "litogramma".into(),
        dialect_version: None,
        blocks: vec![Block::Paragraph(vec![
            milestone("1b"),
            Inline::Text("beta".into()),
            milestone("1a"),
            Inline::Text("alpha".into()),
        ])],
    };
    let err = collation(&[("B".into(), base), ("T".into(), flipped)], "steph", "B");
    assert!(format!("{}", err.unwrap_err()).contains("disagree on coordinate order"),);
}

#[test]
fn collation_drops_unanchorable_leading_addition() {
    // An addition before the first shared word has no anchor
    // word in the base; the entry is dropped (recorded limit).
    let base = doc("quick fox", "tail");
    let wit = doc("the quick fox", "tail");
    let out = collation(&[("B".into(), base), ("T".into(), wit)], "steph", "B").unwrap();
    let s = serialize(&out);
    assert!(!s.contains("add. the"), "{s}");
    assert!(!s.contains("app-"), "{s}");
}
