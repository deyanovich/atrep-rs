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
