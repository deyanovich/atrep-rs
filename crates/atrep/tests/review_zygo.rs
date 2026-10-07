//! Review follow-up (zygo group): regressions for the zygosis,
//! quasialign and collation findings, and the CoNLL-U / corpus
//! importer findings in epimerismos.

use std::path::Path;

use atrep::Document;
use atrep::dendron::serialize;
use atrep::zygosis::{collation, quasialign, zygosis, zygosis_split};

fn parse(src: &str) -> Document {
    atrep::parser::parse_document(src, Path::new("t.atd")).unwrap()
}

fn wit(id: &str, src: &str) -> (String, Document) {
    (id.to_string(), parse(src))
}

// ---- collation anchors under a headed container (zygosis.rs:104)

const HEADED_A: &str = "@@@!litogramma

@# Book One
@(\"steph:1a\")Sing goddess the wrath of Achilles son of Peleus. That brought countless ills upon the Achaeans.
#@
";
const HEADED_B: &str = "@@@!litogramma

@# Book One
@(\"steph:1a\")Sing goddess the anger of Achilles son of Peleus. That brought countless woes upon the Achaeans.
#@
";

#[test]
fn collation_anchors_correctly_under_a_headed_container() {
    let out = collation(&[wit("A", HEADED_A), wit("B", HEADED_B)], "steph", "A").unwrap();
    let s = serialize(&out);
    assert!(s.contains("wrath@^!(app-1a-1)"), "{s}");
    assert!(s.contains("ills@^!(app-1a-2)"), "{s}");
    assert!(s.contains("1a wrath A : anger B"), "{s}");
}

// ---- a titled poem: the lemma is not repeated on continuations
// (zygosis.rs:191) and collation anchors after the first cut

const POEM_A: &str = "@@@!litogramma

@~ The Ode
@(\"steph:1a\")Happy the man whose wish and care
A few paternal acres bound.
@(\"steph:1b\")Content to breathe his native air
In his own ground.
~@
";
const POEM_B: &str = "@@@!litogramma

@~ The Ode
@(\"steph:1a\")Happy the man whose wish and care
A few paternal acres bound.
@(\"steph:1b\")Content to breathe his native wind
In his own ground.
~@
";

#[test]
fn stichoi_lemma_rides_the_first_segment_only() {
    let out = zygosis(&[wit("a", POEM_A), wit("b", POEM_B)], "steph").unwrap();
    let s = serialize(&out);
    // Once per witness, under 1a; the 1b continuation is bare.
    assert_eq!(s.matches("@~ The Ode").count(), 2, "{s}");
    let at_1b = s.find("steph:1b").unwrap();
    assert!(!s[at_1b..].contains("The Ode"), "{s}");
}

#[test]
fn collation_in_a_titled_poem_anchors_after_the_first_cut() {
    let out = collation(&[wit("A", POEM_A), wit("B", POEM_B)], "steph", "A").unwrap();
    let s = serialize(&out);
    assert!(s.contains("native air@^!(app-1b-1)"), "{s}");
}

// ---- a milestone opening the first line keeps the block's onym
// (zygosis.rs:185)

const ONYM_A: &str = "@@@!litogramma

See the ode @>(ode).

@~ The Ode
@(\"steph:1a\")Happy the man whose wish and care
A few paternal acres bound.
~@(ode)
";
const ONYM_B: &str = "@@@!litogramma

@(\"steph:1a\")Other text.
";

#[test]
fn stichoi_onym_survives_a_milestone_on_the_first_line() {
    let out = zygosis(&[wit("a", ONYM_A), wit("b", ONYM_B)], "steph").unwrap();
    let s = serialize(&out);
    assert!(s.contains("~@(ode)"), "{s}");
    assert!(s.contains("@~ The Ode"), "{s}");
}

// ---- quasialign sees milestones inside endos (zygosis.rs:967)

fn sentences(n: usize, stem: &str) -> String {
    (1..=n)
        .map(|i| format!("Sentence number {i} of the {stem} part."))
        .collect::<Vec<_>>()
        .join(" ")
}

fn coords(doc: &Document) -> Vec<String> {
    let s = serialize(doc);
    s.match_indices("@(\"steph:")
        .map(|(i, _)| {
            let rest = &s[i + 9..];
            rest[..rest.find('"').unwrap()].to_string()
        })
        .collect()
}

#[test]
fn quasialign_tracks_a_milestone_inside_an_endo() {
    let a = format!(
        "@@@!litogramma\n\n@(\"steph:1a\"){} @/Quoted @(\"steph:1b\")inside emphasis./@ {}\n",
        sentences(8, "first"),
        sentences(8, "second")
    );
    let b = format!(
        "@@@!litogramma\n\n@(\"steph:1a\"){} Quoted @(\"steph:1b\")inside emphasis. {}\n",
        sentences(8, "first"),
        sentences(8, "second")
    );
    let mut docs = vec![parse(&a), parse(&b)];
    quasialign(&mut docs, "steph", 60, None, None, false).unwrap();
    // Both witnesses cut under both anchors, in the same order,
    // and the files still weave.
    assert_eq!(
        coords(&docs[0]),
        coords(&docs[1]),
        "{}",
        serialize(&docs[0])
    );
    assert!(coords(&docs[0]).iter().any(|c| c.starts_with("1b|")));
    zygosis(
        &[("a".into(), docs[0].clone()), ("b".into(), docs[1].clone())],
        "steph",
    )
    .unwrap();
}

// ---- the whitespace tier cuts single-spaced text (zygosis.rs:739,
// :488)

fn words(n: usize) -> String {
    (0..n)
        .map(|i| ["alpha", "beta", "gamma", "delta"][i % 4])
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn quasialign_falls_back_to_single_spaces() {
    let src = format!("@@@!litogramma\n\n@(\"steph:1a\"){}\n", words(80));
    let mut docs = vec![parse(&src)];
    let report = quasialign(&mut docs, "steph", 100, None, None, false).unwrap();
    assert!(report.inserted[0] >= 3, "{}", serialize(&docs[0]));
    let s = serialize(&docs[0]);
    // Every cut opens on a word, never on a space or mid-word.
    for (i, _) in s.match_indices("\")") {
        let next = s[i + 2..].chars().next().unwrap();
        assert!(next.is_alphabetic(), "{s}");
    }
}

#[test]
fn zygosis_split_falls_back_to_single_spaces() {
    let a = format!("@@@!litogramma\n\n@(\"steph:1a\"){}\n", words(80));
    let b = format!("@@@!litogramma\n\n@(\"steph:1a\"){}\n", words(60));
    let out = zygosis_split(&[wit("a", &a), wit("b", &b)], "steph", Some(100)).unwrap();
    let s = serialize(&out);
    assert!(s.matches(".@@@.zyg-a").count() >= 3, "{s}");
}

// ---- --refine extends the binary path (zygosis.rs:1333)

#[test]
fn refine_extends_the_binary_path() {
    let src = format!(
        "@@@!litogramma\n\n@(\"steph:3\"){}\n\n@(\"steph:3|0.1\"){}\n\n@(\"steph:3|1\"){}\n\n@(\"steph:3|1.1\"){}\n",
        sentences(4, "first"),
        sentences(4, "second"),
        sentences(4, "third"),
        sentences(4, "fourth")
    );
    let mut docs = vec![parse(&src)];
    let report = quasialign(&mut docs, "steph", 90, None, None, true).unwrap();
    assert_eq!(report.inserted, vec![4], "{}", serialize(&docs[0]));
    assert_eq!(
        coords(&docs[0]),
        [
            "3", "3|0.0.1", "3|0.1", "3|0.1.1", "3|1", "3|1.0.1", "3|1.1", "3|1.1.1"
        ]
    );
}

// ---- a milestone repeated inside one witness is named as such
// (zygosis.rs:1718)

const DUP_A: &str = "@@@!litogramma

Sing @(\"steph:1a\") goddess the wrath. @(\"steph:1b\") Of Achilles son of Peleus. @(\"steph:1a\") Again the same.
";
const DUP_B: &str = "@@@!litogramma

Sing @(\"steph:1a\") goddess the wrath. @(\"steph:1b\") Of Achilles son of Peleus.
";

#[test]
fn a_repeated_milestone_is_rejected_by_name() {
    let expect = |e: atrep::Error| {
        let msg = e.to_string();
        assert!(
            msg.contains("steph:1a") && msg.contains("more than once"),
            "{msg}"
        );
        assert!(!msg.contains("disagree"), "{msg}");
    };
    expect(zygosis(&[wit("a", DUP_A), wit("b", DUP_B)], "steph").unwrap_err());
    expect(collation(&[wit("A", DUP_A), wit("B", DUP_B)], "steph", "B").unwrap_err());
    let mut docs = vec![parse(DUP_A)];
    expect(
        quasialign(&mut docs, "steph", 20, None, None, false)
            .err()
            .unwrap(),
    );
}

// ---- zygo --split measures and subdivides verse (zygosis.rs:453)

fn verse(stem: &str, strophes: usize, lines: usize) -> String {
    let mut s = String::from("@@@!litogramma\n\n@(\"steph:1a\")\n\n@~ The Ode\n");
    for st in 0..strophes {
        if st > 0 {
            s.push('\n');
        }
        for l in 1..=lines {
            s.push_str(&format!(
                "{stem} strophe {st} line number {l} padded out to a decent length,\n"
            ));
        }
    }
    s.push_str("~@(ode) Pope\n");
    s
}

#[test]
fn zygosis_split_subdivides_verse_between_lines() {
    let (a, b) = (verse("Alpha", 1, 40), verse("Beta", 1, 40));
    let whole = zygosis(&[wit("a", &a), wit("b", &b)], "steph").unwrap();
    let out = zygosis_split(&[wit("a", &a), wit("b", &b)], "steph", Some(200)).unwrap();
    let s = serialize(&out);
    let rounds = s.matches(".@@@.zyg-a").count();
    assert!(rounds >= 8, "expected subdivision, got {rounds}: {s}");
    assert_eq!(rounds, s.matches(".@@@.zyg-b").count(), "{s}");
    // No line is cut or lost, the title, onym and hypograph ride
    // once, and the zygoma still parses.
    for line in serialize(&whole)
        .lines()
        .filter(|l| l.contains("line number"))
    {
        assert_eq!(s.matches(line).count(), 1, "{line}\n{s}");
    }
    assert_eq!(s.matches("@~ The Ode").count(), 2, "{s}");
    assert_eq!(s.matches("~@(ode)").count(), 2, "{s}");
    assert_eq!(s.matches("Pope").count(), 2, "{s}");
    assert_eq!(serialize(&parse(&s)), s);
}

// ---- many coordinates weave and quasialign in linear time
// (zygosis.rs:1277)

fn many_verses(n: usize, stem: &str) -> Document {
    use atrep::dendron::{Annotations, Block, Inline};
    Document {
        dialect_id: "litogramma".into(),
        dialect_version: None,
        blocks: (0..n)
            .map(|i| {
                Block::Paragraph(vec![
                    Inline::Milestone {
                        scheme: "bcv".into(),
                        value: format!("v{i}"),
                        ann: Annotations {
                            onym: None,
                            genoses: Vec::new(),
                        },
                    },
                    Inline::Text(format!(
                        "Verse {i} of the {stem} witness, with a few words."
                    )),
                ])
            })
            .collect(),
    }
}

/// The bound is generous: the quadratic scans took 30 to 50
/// seconds here in a debug build, the indexed ones well under
/// one.
#[test]
fn twenty_thousand_coordinates_stay_linear() {
    let n = 20_000;
    let (a, b) = (many_verses(n, "first"), many_verses(n, "second"));
    let t = std::time::Instant::now();
    let out = zygosis(&[("a".into(), a.clone()), ("b".into(), b.clone())], "bcv").unwrap();
    assert_eq!(out.blocks.len(), 3 * n);
    let mut docs = vec![a, b];
    let report = quasialign(&mut docs, "bcv", 2000, None, None, false).unwrap();
    assert_eq!(report.inserted, vec![0, 0]);
    assert!(t.elapsed().as_secs() < 10, "took {:?}", t.elapsed());
}

// ---- the word diff keeps one bit per cell (zygosis.rs:1380)

/// Counts the live and peak heap bytes of the current thread, so
/// a test can bound what one call allocates while its siblings
/// run in parallel.
struct Counting;

thread_local! {
    static LIVE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PEAK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let _ = LIVE.try_with(|live| {
            live.set(live.get() + layout.size());
            let _ = PEAK.try_with(|peak| peak.set(peak.get().max(live.get())));
        });
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        let _ = LIVE.try_with(|live| live.set(live.get().saturating_sub(layout.size())));
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// A deterministic word stream over a small vocabulary: many
/// repeats, so the diff meets ties at every turn.
fn word_stream(n: usize, mut seed: u64) -> String {
    const VOCAB: [&str; 10] = [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
    ];
    (0..n)
        .map(|_| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            VOCAB[(seed >> 33) as usize % VOCAB.len()]
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    })
}

fn one_segment(text: &str) -> String {
    format!("@@@!litogramma\n\n@(\"steph:1a\"){text}\n")
}

/// The apparatus is what the full LCS table produced: the digest
/// was taken from the table-based diff before it was replaced.
#[test]
fn word_diff_output_is_unchanged() {
    let a = one_segment(&word_stream(1500, 1));
    let b = one_segment(&word_stream(1400, 2));
    let out = collation(&[wit("A", &a), wit("B", &b)], "steph", "A").unwrap();
    let s = serialize(&out);
    assert_eq!(s.matches("@^!(app-1a-").count(), 530, "apparatus entries");
    assert_eq!(
        fnv(&s),
        6535700174402649690,
        "digest of the collated output"
    );
}

/// 4,000 by 4,000 words: the usize table alone was 128 MB.
#[test]
fn word_diff_memory_is_bounded() {
    let a = wit("A", &one_segment(&word_stream(4000, 3)));
    let b = wit("B", &one_segment(&word_stream(4000, 4)));
    let before = LIVE.with(|l| l.get());
    PEAK.with(|p| p.set(before));
    collation(&[a, b], "steph", "A").unwrap();
    let peak = PEAK.with(|p| p.get()) - before;
    assert!(peak < 32 << 20, "peak heap {} MB", peak >> 20);
}

// ---- CoNLL-U with CRLF line ends (epimerismos.rs:1372)

const CONLLU: &str = "# sent_id = 1
# text = Sing goddess.
1\tSing\tsing\tVERB\t_\t_\t0\troot\t_\t_
2\tgoddess\tgoddess\tNOUN\t_\t_\t1\tobj\t_\tSpaceAfter=No
3\t.\t.\tPUNCT\t_\t_\t1\tpunct\t_\t_

# sent_id = 2
# text = The wrath.
1\tThe\tthe\tDET\t_\t_\t2\tdet\t_\t_
2\twrath\twrath\tNOUN\t_\t_\t0\troot\t_\tSpaceAfter=No
3\t.\t.\tPUNCT\t_\t_\t2\tpunct\t_\t_

";

#[test]
fn conllu_with_crlf_keeps_its_sentences() {
    use atrep::epimerismos::conllu_to_document;
    let lf = conllu_to_document(CONLLU).unwrap();
    let crlf = conllu_to_document(&CONLLU.replace('\n', "\r\n")).unwrap();
    assert_eq!(lf.blocks.len(), 2);
    assert_eq!(serialize(&crlf), serialize(&lf));
}

// ---- a self-closing element is empty, not open to the end
// (epimerismos.rs:668, :904, :1073, :1080, :1099)

#[test]
fn rnc_self_closing_title_keeps_the_body() {
    use atrep::epimerismos::rnc_to_document;
    let xml = |title: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html>\n<head>\n{title}\n</head>\n<body>\n\
             <p><se><w><ana lex=\"sing\" gr=\"V\"/>Sing</w> <w><ana lex=\"goddess\" gr=\"S\"/>goddess</w>.</se></p>\n\
             </body>\n</html>\n"
        )
    };
    let s = serialize(&rnc_to_document(&xml("<title/>")).unwrap());
    assert!(s.contains("@!=(goddess)"), "{s}");
    assert!(!s.contains("@="), "{s}");
    assert_eq!(
        s,
        serialize(&rnc_to_document(&xml("<title></title>")).unwrap())
    );
}

#[test]
fn opencorpora_self_closing_source_keeps_the_sentences() {
    use atrep::epimerismos::opencorpora_to_document;
    let token = |id: usize, form: &str, lemma: &str| {
        format!(
            "<token id=\"{id}\" text=\"{form}\"><tfr rev_id=\"{id}\" t=\"{form}\">\
             <v><l id=\"{id}\" t=\"{lemma}\"><g v=\"NOUN\"/></l></v></tfr></token>\n"
        )
    };
    let xml = |source: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<annotation version=\"2.0\" revision=\"0\">\n\
             <text id=\"1\" parent=\"0\" name=\"Iliad\">\n<paragraphs>\n<paragraph id=\"1\">\n\
             <sentence id=\"1\">\n{source}\n<tokens>\n{}</tokens>\n</sentence>\n\
             <sentence id=\"2\">\n<source>The wrath</source>\n<tokens>\n{}{}</tokens>\n</sentence>\n\
             </paragraph>\n</paragraphs>\n</text>\n</annotation>\n",
            token(1, "Sing", "sing"),
            token(2, "The", "the"),
            token(3, "wrath", "wrath"),
        )
    };
    let s = serialize(&opencorpora_to_document(&xml("<source/>")).unwrap());
    assert!(s.contains("@!=(sing)") && s.contains("@!=(wrath)"), "{s}");
    assert_eq!(
        s,
        serialize(&opencorpora_to_document(&xml("<source></source>")).unwrap())
    );
}

#[test]
fn proiel_self_closing_titles_keep_the_text() {
    use atrep::epimerismos::proiel_to_document;
    let xml = |title: &str, author: &str, div_title: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<proiel schema-version=\"2.0\">\n\
             <source id=\"atrep\" language=\"und\">\n{title}\n{author}\n<div>\n{div_title}\n\
             <sentence id=\"1\">\n\
             <token id=\"1\" form=\"Sing\" citation-part=\"IL 1.1\" lemma=\"sing\" part-of-speech=\"V-\" relation=\"pred\" presentation-after=\" \"/>\n\
             <token id=\"2\" form=\"goddess\" citation-part=\"IL 1.1\" lemma=\"goddess\" part-of-speech=\"Nb\" head-id=\"1\" relation=\"voc\" presentation-after=\".\"/>\n\
             </sentence>\n</div>\n</source>\n</proiel>\n"
        )
    };
    let empty = serialize(
        &proiel_to_document(&xml(
            "<title></title>",
            "<author></author>",
            "<title></title>",
        ))
        .unwrap(),
    );
    assert!(empty.contains("@!=(goddess)"), "{empty}");
    // An empty title, author or division title is no title.
    assert!(!empty.contains("@=") && !empty.contains("@#"), "{empty}");
    // Each self-closing site on its own, then all three.
    for (t, a, d) in [
        ("<title/>", "<author></author>", "<title></title>"),
        ("<title></title>", "<author/>", "<title></title>"),
        ("<title></title>", "<author></author>", "<title/>"),
        ("<title/>", "<author/>", "<title/>"),
    ] {
        let s = serialize(&proiel_to_document(&xml(t, a, d)).unwrap());
        assert_eq!(s, empty, "{t} {a} {d}");
    }
}

#[test]
fn zygosis_split_prefers_strophe_heads() {
    let (a, b) = (verse("Alpha", 2, 3), verse("Beta", 2, 3));
    let out = zygosis_split(&[wit("a", &a), wit("b", &b)], "steph", Some(200)).unwrap();
    let s = serialize(&out);
    // One cut per witness, at the strophe head: no strophe is
    // split and no blank line survives inside a block.
    assert_eq!(s.matches(".@@@.zyg-a").count(), 2, "{s}");
    assert!(s.contains("@~\nAlpha strophe 1 line number 1 "), "{s}");
    assert!(!s.contains(",\n\nAlpha"), "{s}");
}
