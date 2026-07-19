//! Zygosis segment subdivision (--split): recursive proportional
//! halving of oversized coordinate slices.

use atrep::dendron::{Annotations, Block, Document, Inline, serialize};
use atrep::zygosis::{zygosis, zygosis_split};

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

fn witness(text: &str) -> Document {
    Document {
        dialect_id: "litogramma".into(),
        dialect_version: None,
        blocks: vec![Block::Paragraph(vec![
            milestone("1a"),
            Inline::Text(text.into()),
        ])],
    }
}

const LONG_A: &str = "First sentence of the original, long enough to matter. \
Second sentence of the original, also of a good length. \
Third sentence of the original, rounding out the slice. \
Fourth sentence of the original, the very last one.";
const LONG_B: &str = "Erster Satz der Übersetzung, lang genug für die Probe. \
Zweiter Satz der Übersetzung, ebenfalls von guter Länge. \
Dritter Satz der Übersetzung, der die Scheibe rundet. \
Vierter Satz der Übersetzung, der allerletzte.";

#[test]
fn oversized_pairs_split_with_matching_rounds() {
    let out = zygosis_split(
        &[("a".into(), witness(LONG_A)), ("b".into(), witness(LONG_B))],
        "steph",
        Some(120),
    )
    .unwrap();
    let s = serialize(&out);
    // One milestone only — subdivision never synthesizes coordinates.
    assert_eq!(s.matches("steph:1a").count(), 1, "{s}");
    // Both witnesses split into the same number of sub-slices,
    // interleaved a, b, a, b.
    let a = s.matches(".@@@.zyg-a").count();
    let b = s.matches(".@@@.zyg-b").count();
    assert_eq!(a, b, "{s}");
    assert!(a >= 2, "expected subdivision, got {a} rounds: {s}");
    // Cuts land at sentence boundaries: sub-slices start on
    // sentence heads, and no sentence is beheaded.
    assert!(s.contains("Third sentence of the original"), "{s}");
    assert!(s.contains("Dritter Satz der Übersetzung"), "{s}");
}

#[test]
fn fitting_pairs_pass_through_whole() {
    let with = zygosis_split(
        &[
            ("a".into(), witness("Short one. Done.")),
            ("b".into(), witness("Kurz. Fertig.")),
        ],
        "steph",
        Some(120),
    )
    .unwrap();
    let without = zygosis(
        &[
            ("a".into(), witness("Short one. Done.")),
            ("b".into(), witness("Kurz. Fertig.")),
        ],
        "steph",
    )
    .unwrap();
    assert_eq!(serialize(&with), serialize(&without));
}

#[test]
fn unsplittable_slices_degrade_gracefully() {
    // A single sentence with no interior boundary of any tier
    // (no sentence end, no comma, no space) cannot be halved:
    // the pair must come through whole, not error.
    let unbroken = "x".repeat(300);
    let out = zygosis_split(
        &[
            ("a".into(), witness(&unbroken)),
            ("b".into(), witness(&unbroken)),
        ],
        "steph",
        Some(120),
    )
    .unwrap();
    let s = serialize(&out);
    assert_eq!(s.matches(".@@@.zyg-a").count(), 1, "{s}");
    assert!(s.contains(&unbroken), "{s}");
}

// ---- quasialign: the in-file counterpart of --split ----

use atrep::zygosis::quasialign;

fn sentences(n: usize, stem: &str) -> String {
    (1..=n)
        .map(|i| format!("{stem} sentence number {i} with a bit of length to it."))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn quasialign_inserts_binary_path_labels() {
    let mut docs = vec![
        witness(&sentences(8, "Original")),
        witness(&sentences(8, "Translated")),
    ];
    let report = quasialign(&mut docs, "steph", 120, None, None).unwrap();
    assert_eq!(report.inserted[0], report.inserted[1]);
    assert!(
        report.inserted[0] >= 3,
        "expected recursive cuts, got {}",
        report.inserted[0]
    );
    let s = serialize(&docs[0]);
    // The first cut and its half-cuts, all quasi-tagged.
    assert!(s.contains(r#"@("steph:1a|1")"#), "{s}");
    assert!(s.contains(r#"@("steph:1a|0.1")"#), "{s}");
    assert!(s.contains(r#"@("steph:1a|1.1")"#), "{s}");
    // No genos rides the quasi milestone (owner ruling: the
    // quasi marker lives in the coordinate value).
    assert!(!s.contains(".quasi"), "{s}");
    // Cuts land on sentence heads.
    assert!(!s.contains("with a bit@("), "{s}");
    // Round-trips through the parser: the pipe notation is a
    // valid milestone value. (Reparse under koine, which
    // resolves from the standard library; the milestone syntax
    // is core.)
    let koine = s.replacen("@@@!litogramma", "@@@!koine", 1);
    let reparsed =
        atrep::parser::parse_document(&koine, std::path::Path::new("t.atd")).unwrap();
    assert_eq!(serialize(&reparsed), koine);
}

#[test]
fn quasialign_is_idempotent_and_weaves_plain() {
    let mut docs = vec![
        witness(&sentences(8, "Original")),
        witness(&sentences(8, "Translated")),
    ];
    quasialign(&mut docs, "steph", 120, None, None).unwrap();
    let first = (serialize(&docs[0]), serialize(&docs[1]));
    // A second run finds the quasi cuts and touches nothing.
    let again = quasialign(&mut docs, "steph", 120, None, None).unwrap();
    assert_eq!(again.inserted, vec![0, 0]);
    assert_eq!(again.skipped, vec!["1a".to_string()]);
    assert_eq!(first.0, serialize(&docs[0]));
    // The quasialigned pair weaves with plain zygo: the quasi
    // coordinates become segment heads of the zygoma.
    let woven = zygosis(
        &[("a".into(), docs[0].clone()), ("b".into(), docs[1].clone())],
        "steph",
    )
    .unwrap();
    let w = serialize(&woven);
    assert!(w.contains("steph:1a|1"), "{w}");
    let a = w.matches(".@@@.zyg-a").count();
    assert_eq!(a, w.matches(".@@@.zyg-b").count(), "{w}");
    assert!(a >= 4, "{w}");
}

#[test]
fn quasialign_under_threshold_is_a_no_op() {
    let mut docs = vec![witness("Small. Tiny."), witness("Klein. Winzig.")];
    let before = serialize(&docs[0]);
    let report = quasialign(&mut docs, "steph", 120, None, None).unwrap();
    assert_eq!(report.inserted, vec![0, 0]);
    assert_eq!(before, serialize(&docs[0]));
}

#[test]
fn quasialign_prefix_scopes_to_one_witness() {
    // A synopsis file: both witnesses ride genos-tagged
    // paradiaphanes, each carrying the coordinate inside its own
    // stream. --prefix zyg-grc drives the alignment by the first
    // stream alone and cuts only there.
    let diaphane = |id: &str, text: &str| Block::ParaDiaphane {
        children: vec![Block::Paragraph(vec![
            milestone("1a"),
            Inline::Text(text.into()),
        ])],
        ann: Annotations {
            onym: None,
            genoses: vec![format!("zyg-{id}")],
        },
    };
    let mut docs = vec![Document {
        dialect_id: "litogramma".into(),
        dialect_version: None,
        blocks: vec![
            diaphane("grc", &sentences(8, "Original")),
            diaphane("eng", &sentences(8, "Translated")),
        ],
    }];
    let report = quasialign(&mut docs, "steph", 120, Some("zyg-grc"), None).unwrap();
    assert!(report.inserted[0] >= 3, "{}", report.inserted[0]);
    let s = serialize(&docs[0]);
    // Cuts sit inside the grc stream only.
    let grc = s.split(".@@@.zyg-grc").next().unwrap();
    assert!(grc.contains(r#"@("steph:1a|1")"#), "{s}");
    let eng = s.split(".@@@.zyg-grc").nth(1).unwrap();
    assert!(!eng.contains("1a|"), "{s}");
    // A second run scoped to the other witness cuts that stream
    // at its own midpoints, reusing the same label space.
    let report = quasialign(&mut docs, "steph", 120, Some("zyg-eng"), None).unwrap();
    assert!(report.inserted[0] >= 3, "{}", report.inserted[0]);
    let s = serialize(&docs[0]);
    let eng = s.split(".@@@.zyg-grc").nth(1).unwrap();
    assert!(eng.contains(r#"@("steph:1a|1")"#), "{s}");
}
