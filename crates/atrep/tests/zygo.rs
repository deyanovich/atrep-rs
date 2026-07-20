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
    let reparsed = atrep::parser::parse_document(&koine, std::path::Path::new("t.atd")).unwrap();
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

fn witness_with(coords_and_texts: &[(&str, &str)]) -> Document {
    let mut inlines = Vec::new();
    for (coord, text) in coords_and_texts {
        inlines.push(milestone(coord));
        inlines.push(Inline::Text((*text).into()));
    }
    Document {
        dialect_id: "litogramma".into(),
        dialect_version: None,
        blocks: vec![Block::Paragraph(inlines)],
    }
}

#[test]
fn zygosis_validates_inputs() {
    let a = witness("alpha");
    // Fewer than two witnesses.
    let err = zygosis(&[("a".into(), a.clone())], "steph");
    assert!(format!("{}", err.unwrap_err()).contains("at least two"));
    // One dialektos per weave.
    let mut b = witness("beta");
    b.dialect_id = "koine".into();
    let err = zygosis(&[("a".into(), a.clone()), ("b".into(), b)], "steph");
    assert!(format!("{}", err.unwrap_err()).contains("one dialektos per weave"));
    // Duplicate id.
    let err = zygosis(
        &[("a".into(), a.clone()), ("a".into(), witness("beta"))],
        "steph",
    );
    assert!(format!("{}", err.unwrap_err()).contains("duplicate witness id"));
    // The identifier itself must satisfy the genos grammar
    // (spec v0.12.1): leading digits and capitals are invalid.
    for bad in ["1a", "B"] {
        let err = zygosis(
            &[(bad.to_string(), a.clone()), ("b".into(), witness("beta"))],
            "steph",
        );
        assert!(
            format!("{}", err.unwrap_err()).contains("not a valid identifier"),
            "id `{bad}` should be rejected"
        );
    }
}

#[test]
fn zygosis_alignment_contradiction_errors() {
    let a = witness_with(&[("1a", "alpha"), ("1b", "beta")]);
    let b = witness_with(&[("1b", "beta"), ("1a", "alpha")]);
    let err = zygosis(&[("a".into(), a), ("b".into(), b)], "steph");
    assert!(format!("{}", err.unwrap_err()).contains("disagree on coordinate order"),);
}

#[test]
fn zygosis_merges_disjoint_coordinates_with_proem() {
    // a carries 1a,1c; b carries 1b,1c: the merge interleaves
    // 1a,1b,1c. a's pre-milestone content forms its proem.
    let mut a = witness_with(&[("1a", "A-alpha"), ("1c", "A-gamma")]);
    a.blocks.insert(
        0,
        Block::Paragraph(vec![Inline::Text("Proem text.".into())]),
    );
    let b = witness_with(&[("1b", "B-beta"), ("1c", "B-gamma")]);
    let out = zygosis(&[("a".into(), a), ("b".into(), b)], "steph").unwrap();
    let s = serialize(&out);
    // Proem opens the zygoma, inside the witness's diaphane.
    assert!(s.contains("Proem text."), "{s}");
    // Merged coordinate order and per-witness diaphane tags.
    let pos = |needle: &str| {
        s.find(needle)
            .unwrap_or_else(|| panic!("missing {needle}: {s}"))
    };
    assert!(pos("steph:1a") < pos("steph:1b"), "{s}");
    assert!(pos("steph:1b") < pos("steph:1c"), "{s}");
    assert!(s.contains(".zyg-a"), "{s}");
    assert!(s.contains(".zyg-b"), "{s}");
    // Both witnesses contribute to the shared 1c segment.
    assert!(pos("A-gamma") > pos("steph:1c"), "{s}");
    assert!(pos("B-gamma") > pos("steph:1c"), "{s}");
    // The zygoma is a valid document: it reparses byte-stably.
    let reparsed = atrep::parser::parse_document(&s, std::path::Path::new("z.atd")).unwrap();
    assert_eq!(serialize(&reparsed), s);
}

#[test]
fn zygosis_hoists_containers_at_cuts() {
    // A cut inside an enclosing container hoists: the container
    // keeps its pre-cut content, the continuation flows at the
    // cutting level, and the heading is not repeated.
    let a = Document {
        dialect_id: "litogramma".into(),
        dialect_version: None,
        blocks: vec![Block::Para {
            symbol: "=".into(),
            taxis: None,
            lemma: vec![Inline::Text("Head".into())],
            children: vec![
                Block::Paragraph(vec![Inline::Text("Before the cut.".into())]),
                Block::Paragraph(vec![milestone("1a"), Inline::Text("After the cut.".into())]),
            ],
            hypograph: Vec::new(),
            bracket_matching: true,
            ann: Annotations::default(),
        }],
    };
    let b = witness_with(&[("1a", "Other.")]);
    let out = zygosis(&[("a".into(), a), ("b".into(), b)], "steph").unwrap();
    let s = serialize(&out);
    // The container survives in the proem with its pre-cut
    // content; the continuation is not wrapped (the heading
    // appears exactly once).
    assert_eq!(s.matches("Head").count(), 1, "{s}");
    let pos = |needle: &str| {
        s.find(needle)
            .unwrap_or_else(|| panic!("missing {needle}: {s}"))
    };
    assert!(pos("Before the cut.") < pos("steph:1a"), "{s}");
    assert!(pos("After the cut.") > pos("steph:1a"), "{s}");
}
