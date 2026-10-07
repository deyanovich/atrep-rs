//! Regression tests for the kanon review group (dendron serializer
//! and kanonizo). Remote cases are served from an ephemeral
//! 127.0.0.1 listener; no test touches the network.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::kanonizo;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Fresh per-test scratch directory holding the `exempli`
/// dialektos, nested one level down so a test can plant a file
/// above it.
fn tmp_dir(name: &str) -> PathBuf {
    let outer = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("review-kanon-{name}"));
    let _ = std::fs::remove_dir_all(&outer);
    let dir = outer.join("base");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        fixtures().join("exempli.lektos"),
        dir.join("exempli.lektos"),
    )
    .unwrap();
    dir
}

fn kanonizo_src(dir: &Path, name: &str, src: &str) -> atrep::Result<kanonizo::KanonResult> {
    let path = dir.join(name);
    std::fs::write(&path, src).unwrap();
    kanonizo::kanonizo_file(&path)
}

fn kanon(dir: &Path, src: &str) -> String {
    kanonizo_src(dir, "doc.atd", src).unwrap().kanon
}

fn error_text(dir: &Path, src: &str) -> String {
    kanonizo_src(dir, "doc.atd", src).unwrap_err().to_string()
}

struct Route {
    path: &'static str,
    body: Vec<u8>,
}

/// Serve the built routes (404 for unknown paths) for up to
/// `max_requests` requests on a fresh localhost port.
fn serve(routes: Vec<Route>, max_requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let base = format!("http://127.0.0.1:{port}");
    std::thread::spawn(move || {
        for _ in 0..max_requests {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        req.extend_from_slice(&buf[..n]);
                        if req.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
            let req = String::from_utf8_lossy(&req);
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            match routes.iter().find(|r| r.path == path) {
                Some(r) => {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n",
                        r.body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&r.body);
                }
                None => {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\
                          Connection: close\r\n\r\n",
                    );
                }
            }
        }
    });
    base
}

// ---------------------------------------------------------------
// Path containment (kanonizo.rs: transclusion and media targets)
// ---------------------------------------------------------------

#[test]
fn absolute_local_targets_are_refused() {
    let dir = tmp_dir("absolute");
    let secret = dir.parent().unwrap().join("secret.txt");
    std::fs::write(&secret, "do not embed\n").unwrap();
    let abs = secret.display().to_string();
    for src in [
        format!("@@@!exempli\n\n@@@+({abs})\n"),
        format!("@@@!exempli\n\n@@@({abs})\n"),
        format!("@@@!exempli\n\n@@@@({abs})\n"),
    ] {
        let err = kanonizo_src(&dir, "doc.atd", &src).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::MissingResource(_)), "{err}");
        assert!(err.to_string().contains("absolute"), "{err}");
    }
}

#[test]
fn targets_climbing_above_the_document_directory_are_refused() {
    let dir = tmp_dir("climb");
    let outer = dir.parent().unwrap();
    std::fs::write(outer.join("secret.txt"), "do not embed\n").unwrap();
    std::fs::write(outer.join("secret.atd"), "@@@!exempli\n\nleaked\n").unwrap();
    std::fs::copy(fixtures().join("pipeline.svg"), outer.join("secret.svg")).unwrap();
    for src in [
        "@@@!exempli\n\n@@@+(../secret.txt)\n",
        "@@@!exempli\n\n@@@(../secret.atd)\n",
        "@@@!exempli\n\n@@@@(../secret.svg)\n",
        "@@@!exempli\n\n@@@+(sub/../../secret.txt)\n",
    ] {
        let err = kanonizo_src(&dir, "doc.atd", src).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::MissingResource(_)), "{err}");
        assert!(err.to_string().contains("escapes"), "{err}");
    }
}

#[test]
fn nested_transclusion_may_climb_within_the_document_directory() {
    // A part in a subdirectory may reach a sibling of its includer:
    // the sandbox is the root document's directory, not the part's.
    let dir = tmp_dir("nested");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::copy(dir.join("exempli.lektos"), dir.join("sub/exempli.lektos")).unwrap();
    std::fs::write(dir.join("shared.txt"), "shared\n").unwrap();
    std::fs::write(
        dir.join("sub/part.atd"),
        "@@@!exempli\n\n@@@+(../shared.txt)\n",
    )
    .unwrap();
    let out = kanon(&dir, "@@@!exempli\n\n@@@(sub/part.atd)\n");
    assert_eq!(
        out,
        "@@@!exempli\n\n@@@!(exempli)\n@@@\"\nshared\n\"@@@\n!@@@\n"
    );
}

#[test]
fn remote_document_targets_resolve_against_its_url() {
    // The remote part's relative enlexis and media targets resolve
    // next to it on the server, not in the local directory (which
    // holds decoys with the same names).
    let svg = std::fs::read(fixtures().join("pipeline.svg")).unwrap();
    let base = serve(
        vec![
            Route {
                path: "/dir/part.atd",
                body: b"@@@!exempli\n\n@@@+(steps.txt)\n\n@@@+(../up.txt)\n\n@@@@(pipeline.svg)\n"
                    .to_vec(),
            },
            Route {
                path: "/dir/steps.txt",
                body: b"served steps".to_vec(),
            },
            Route {
                path: "/up.txt",
                body: b"served up".to_vec(),
            },
            Route {
                path: "/dir/pipeline.svg",
                body: svg.clone(),
            },
        ],
        4,
    );
    let dir = tmp_dir("remote-relative");
    std::fs::write(dir.join("steps.txt"), "local decoy\n").unwrap();
    std::fs::write(dir.join("up.txt"), "local decoy\n").unwrap();
    std::fs::write(dir.join("pipeline.svg"), "local decoy\n").unwrap();
    let result = kanonizo_src(
        &dir,
        "doc.atd",
        &format!("@@@!exempli\n\n@@@({base}/dir/part.atd)\n"),
    )
    .unwrap();
    assert!(result.kanon.contains("served steps"), "{}", result.kanon);
    assert!(result.kanon.contains("served up"), "{}", result.kanon);
    assert!(!result.kanon.contains("decoy"), "{}", result.kanon);
    assert_eq!(result.media.len(), 1);
    assert_eq!(result.media[0].source, format!("{base}/dir/pipeline.svg"));
    assert_eq!(result.media[0].bytes, svg);
}

#[test]
fn remote_document_root_relative_target_never_reads_local_files() {
    // `@@@+(/etc/hostname)` in a remote document is a root-relative
    // URL on the serving host: served, it embeds the server's
    // answer; unserved, it is a missing resource, never the local
    // file.
    let base = serve(
        vec![
            Route {
                path: "/part.atd",
                body: b"@@@!exempli\n\n@@@+(/etc/hostname)\n".to_vec(),
            },
            Route {
                path: "/etc/hostname",
                body: b"served hostname".to_vec(),
            },
        ],
        2,
    );
    let dir = tmp_dir("remote-root-relative");
    let out = kanon(&dir, &format!("@@@!exempli\n\n@@@({base}/part.atd)\n"));
    assert!(out.contains("served hostname"), "{out}");

    let base = serve(
        vec![Route {
            path: "/media.atd",
            body: b"@@@!exempli\n\n@@@@(/etc/hostname)\n".to_vec(),
        }],
        2,
    );
    let err = kanonizo_src(
        &dir,
        "doc.atd",
        &format!("@@@!exempli\n\n@@@({base}/media.atd)\n"),
    )
    .unwrap_err();
    assert!(matches!(err.kind, ErrorKind::MissingResource(_)), "{err}");
    assert!(err.to_string().contains(&base), "{err}");
}

// ---------------------------------------------------------------
// Serializer / parser agreement (dendron.rs)
// ---------------------------------------------------------------

/// A small dialektos covering the sim shapes the serializer
/// findings exercise: two endo sims, a section with taxis, lemma
/// and hypograph, a numbered figure, an autonym entry, a link
/// monosim.
const REVIEW_DIA: &str = "@@@!atrep

@=== emphasis
@, grammata ,@
===@

@=== note
@^ grammata ^@
===@

@=== section
@#[(taxis)] [lemma]
grammata
#@ [hypograph]
===@

@=== figure
@=[(taxis)] lemma
grammata
=@
===@

@=== entry
@![(taxis)] lemma
autonym
grammata
!@
===@

@=== link
@&(param)
===@
";

fn review_dir(name: &str) -> PathBuf {
    let dir = tmp_dir(name);
    std::fs::write(dir.join("review.dia"), REVIEW_DIA).unwrap();
    dir
}

/// Kanonizo the source, then kanonizo the kanon: the second pass
/// must parse and reproduce the first byte for byte.
fn idempotent_kanon(dir: &Path, src: &str) -> String {
    let first = kanonizo_src(dir, "doc.atd", src).unwrap().kanon;
    let again = kanonizo_src(dir, "doc.atk", &first)
        .unwrap_or_else(|e| panic!("kanon does not re-kanonize: {e}\n{first}"))
        .kanon;
    assert_eq!(first, again, "kanonizo is not idempotent");
    first
}

#[test]
fn diaphane_text_ending_in_its_episymbol_keeps_the_separator() {
    let dir = review_dir("diaphane-terminator");
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\nSee @@.Tom Sawyer.|@@\"x\"@@.@@.note here.\n",
    );
    assert_eq!(
        out,
        "@@@!review\n\nSee @@.Tom Sawyer.|@@\"x\"@@.@@.note here.\n"
    );
    // Nested in an endo sim, the diaphane's own episymbol is the
    // one that matters: `foo,` before a sim needs no separator
    // there (the authored one is consumed and not re-emitted).
    let out = idempotent_kanon(&dir, "@@@!review\n\n@,@@.foo,|@^x^@.@@.g,@ end.\n");
    assert_eq!(out, "@@@!review\n\n@,@@.foo,@^x^@.@@.g,@ end.\n");
}

#[test]
fn endo_axioma_text_ending_in_colon_keeps_the_separator() {
    use atrep::dendron::{Annotations, Block, Document, Inline};
    let dir = review_dir("axioma-terminator");
    let doc = Document {
        dialect_id: "review".into(),
        dialect_version: None,
        blocks: vec![Block::Paragraph(vec![
            Inline::EndoAxioma {
                onym: "ax".into(),
                content: vec![
                    Inline::Text("Note:".into()),
                    Inline::Endo {
                        symbol: ",".into(),
                        content: vec![Inline::Text("Iliad".into())],
                        bracket_matching: true,
                        ann: Annotations::default(),
                    },
                ],
            },
            Inline::Text(" then ".into()),
            Inline::AxiomaRef {
                onym: "ax".into(),
                enlexis: false,
            },
        ])],
    };
    let atd = atrep::dendron::serialize(&doc);
    assert_eq!(
        atd,
        "@@@!review\n\n@@:Note:|@,Iliad,@:@@(ax) then @@(:ax:)\n"
    );
    let path = dir.join("doc.atd");
    std::fs::write(&path, &atd).unwrap();
    let parsed = atrep::parser::parse_document(&atd, &path).unwrap();
    assert_eq!(parsed, doc);
}

#[test]
fn hypograph_opening_with_paren_or_genos_is_not_prefixed() {
    let dir = review_dir("hypograph");
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\n@# Title\nbody.\n#@ (Austen) said\n\n@# Two\nbody.\n#@ .note said\n",
    );
    assert!(out.contains("#@ (Austen) said\n"), "{out}");
    assert!(out.contains("#@ .note said\n"), "{out}");
    // Core stichoi hypograph likewise.
    let out = idempotent_kanon(&dir, "@@@!review\n\n@@@=\nline\n=@@@ (Austen)\n");
    assert!(out.contains("=@@@ (Austen)\n"), "{out}");
}

#[test]
fn onym_anchor_takes_no_separator() {
    let dir = review_dir("onym-anchor");
    let out = idempotent_kanon(&dir, "@@@!review\n\nx @(anc)(paren) y @&(anc) z.\n");
    assert_eq!(out, "@@@!review\n\nx @(o1)(paren) y @&(o1) z.\n");
    let out = idempotent_kanon(&dir, "@@@!review\n\nx @(anc).note y @&(anc) z.\n");
    assert_eq!(out, "@@@!review\n\nx @(o1).note y @&(o1) z.\n");
}

#[test]
fn backslash_pipe_in_text_survives_rekanonizo() {
    let dir = review_dir("backslash-pipe");
    // `\\|` reads as a literal backslash then an escaped pipe; the
    // kanon must spell the pair the same way, not drop a backslash
    // per pass.
    let out = idempotent_kanon(&dir, "@@@!review\n\nHuck a\\\\|b Finn.\n");
    assert_eq!(out, "@@@!review\n\nHuck a\\\\|b Finn.\n");
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\nA |@,b,@ c|| d\\\\| e @,f,@|g @,h,@\\\\|i.\n",
    );
    // (`@,f,@|g`: the authored separator after an episim is consumed
    // by the annotation scan, so the kanon has none.)
    assert_eq!(
        out,
        "@@@!review\n\nA @,b,@ c|| d\\\\| e @,f,@g @,h,@\\\\|i.\n"
    );
    // In a stichos too.
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\n@@@= Lemma\n|lead pipe\n\\\\|escaped\n  indented @,x,@\n=@@@ hypo\n",
    );
    assert_eq!(
        out,
        "@@@!review\n\n@@@= Lemma\n|lead pipe\n\\\\|escaped\n  indented @,x,@\n=@@@ hypo\n"
    );
}

#[test]
fn literal_backslash_before_a_sim_is_refused() {
    use atrep::dendron::{Annotations, Block, Document, Inline};
    let doc = Document {
        dialect_id: "review".into(),
        dialect_version: None,
        blocks: vec![Block::Paragraph(vec![
            Inline::Text("Path C:\\".into()),
            Inline::Endo {
                symbol: ",".into(),
                content: vec![Inline::Text("Tom Sawyer".into())],
                bracket_matching: true,
                ann: Annotations::default(),
            },
            Inline::Text(" end.".into()),
        ])],
    };
    let err = atrep::dendron::check_serializable(&doc).unwrap_err();
    assert!(err.to_string().contains("backslash"), "{err}");
    // A backslash elsewhere is fine.
    let mut ok = doc.clone();
    ok.blocks = vec![Block::Paragraph(vec![Inline::Text(
        "C:\\ and a\\@b".into(),
    )])];
    atrep::dendron::check_serializable(&ok).unwrap();

    // Through kanonizo: an axioma expansion can place a trailing
    // backslash directly before a sim; the kanon refuses rather
    // than emitting `\@`, which would read back as an escaped `@`.
    let dir = review_dir("backslash-sim");
    let err = error_text(
        &dir,
        "@@@!review\n\n@@:C:\\:@@(a)@@(:a:)@,Tom Sawyer,@ end.\n",
    );
    assert!(err.contains("backslash"), "{err}");
}

// ---------------------------------------------------------------
// Axioma expansion bound (kanonizo.rs)
// ---------------------------------------------------------------

#[test]
fn doubling_block_axiomata_hit_the_expansion_bound() {
    // Each definition references its predecessor twice: 40 links
    // would expand to 2^40 paragraphs. The bound reports it.
    let dir = review_dir("axioma-blocks");
    let mut src = String::from("@@@!review\n\n@@@:\nSing, O goddess.\n:@@@(a0)\n\n");
    for n in 1..=40 {
        let p = n - 1;
        src.push_str(&format!("@@@:\n@@@(:a{p}:)\n@@@(:a{p}:)\n:@@@(a{n})\n\n"));
    }
    src.push_str("@@@(:a40:)\n");
    let err = error_text(&dir, &src);
    assert!(err.contains("axioma expansion exceeds"), "{err}");
}

#[test]
fn doubling_inline_axiomata_hit_the_expansion_bound() {
    let dir = review_dir("axioma-inlines");
    let mut src = String::from("@@@!review\n\n@@:the wrath :@@(a0)");
    for n in 1..=40 {
        let p = n - 1;
        src.push_str(&format!("@@:@@(:a{p}:)@@(:a{p}:):@@(a{n})"));
    }
    src.push_str("@@(:a40:) of Achilles.\n");
    let err = error_text(&dir, &src);
    assert!(err.contains("axioma expansion exceeds"), "{err}");
}

#[test]
fn ordinary_axioma_reuse_stays_within_the_bound() {
    let dir = review_dir("axioma-ordinary");
    let mut src = String::from("@@@!review\n\n@@@:\nSing, O goddess.\n:@@@(a0)\n\n");
    for _ in 0..200 {
        src.push_str("@@@(:a0:)\n");
    }
    let out = kanon(&dir, &src);
    assert_eq!(out.matches("Sing, O goddess.").count(), 200);
}

// ---------------------------------------------------------------
// Whitespace normalization (kanonizo.rs)
// ---------------------------------------------------------------

#[test]
fn only_tabs_spaces_and_line_breaks_normalize() {
    // The spec names tabs and spaces. A no-break space, a thin
    // space and a narrow no-break space are typography, hence
    // content: they survive, and a run of them does not collapse.
    let dir = review_dir("whitespace");
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\nLouis\u{a0}XIV said \u{2009}«hello»\u{202f}!  Then\t\tleft\nthe\u{3000}\u{3000}room.  \n",
    );
    assert_eq!(
        out,
        "@@@!review\n\nLouis\u{a0}XIV said \u{2009}«hello»\u{202f}! Then left the\u{3000}\u{3000}room.\n"
    );
}

// ---------------------------------------------------------------
// Validation and canonicalization passes (kanonizo.rs)
// ---------------------------------------------------------------

#[test]
fn duplicate_milestone_inside_an_englossis_is_detected() {
    let dir = review_dir("milestone-englossis");
    let err = error_text(
        &dir,
        "@@@!review\n\n@(\"s:1\")alpha.\n\n@@@!(review)\n@(\"s:1\")beta.\n!@@@\n",
    );
    assert!(err.contains("duplicate milestone `s:1`"), "{err}");
    // Through a transcluded document as well.
    std::fs::write(dir.join("part.atd"), "@@@!review\n\n@(\"s:1\")beta.\n").unwrap();
    let err = error_text(&dir, "@@@!review\n\n@(\"s:1\")alpha.\n\n@@@(part.atd)\n");
    assert!(err.contains("duplicate milestone `s:1`"), "{err}");
}

#[test]
fn deixis_may_point_at_a_computed_autonym() {
    // The source leaves the entry's onym to be computed; the
    // deixis to it is valid, exactly as it is once the kanon pins
    // `!@(Achilles)`.
    let dir = review_dir("deixis-autonym");
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\n@! Achilles\nson of Peleus.\n!@\n\nSee @!(Achilles).\n",
    );
    assert_eq!(
        out,
        "@@@!review\n\n@! Achilles\nson of Peleus.\n!@(Achilles)\n\nSee @!(Achilles).\n"
    );
    // A deixis to nothing is still an error.
    let err = error_text(
        &dir,
        "@@@!review\n\n@! Achilles\nson of Peleus.\n!@\n\nSee @!(Hector).\n",
    );
    assert!(err.contains("Hector"), "{err}");
}

#[test]
fn monosim_onym_suffix_is_canonicalized_like_any_other() {
    let dir = review_dir("monosim-onym");
    // Referenced: renumbered in declaration order with the rest.
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\nSee @&(http://x)(mylink) then @&(mylink) and @,Iliad,@(em) then @&(em).\n",
    );
    assert_eq!(
        out,
        "@@@!review\n\nSee @&(http://x)(o1) then @&(o1) and @,Iliad,@(o2) then @&(o2).\n"
    );
    // Unreferenced: removed.
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\nSee @&(http://x)(mylink) and @,Iliad,@(myemph).\n",
    );
    assert_eq!(out, "@@@!review\n\nSee @&(http://x) and @,Iliad,@.\n");
}

/// A second dialektos whose `!` is a plain numbered para sim (in
/// `review` it is an autonym entry).
const PLAIN_DIA: &str = "@@@!atrep

@=== shout
@![(taxis)] lemma
grammata
!@
===@
";

#[test]
fn englossis_answers_to_its_own_dialektos_for_autonyms_and_taxis() {
    let dir = review_dir("englossis-dialektos");
    std::fs::write(dir.join("plain.dia"), PLAIN_DIA).unwrap();
    // Outer `review` (autonym `!`), inner `plain` (`!` is plain):
    // no onym is pinned inside the englossis, and the inner blank
    // taxis sequences by sibling run as in any plain sim.
    std::fs::write(
        dir.join("part.atd"),
        "@@@!plain\n\n@!() Hector\nson of Priam.\n!@\n\n@!() Paris\nhis brother.\n!@\n",
    )
    .unwrap();
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\n@! Achilles\nson of Peleus.\n!@\n\n@@@(part.atd)\n",
    );
    assert_eq!(
        out,
        "@@@!review\n\n@! Achilles\nson of Peleus.\n!@(Achilles)\n\n@@@!(plain)\n\
         @!(1) Hector\nson of Priam.\n!@\n\n@!(2) Paris\nhis brother.\n!@\n!@@@\n"
    );
    // The other way round: outer `plain`, inner `review` — the
    // inner entry is an autonym and pins its onym.
    std::fs::write(
        dir.join("entry.atd"),
        "@@@!review\n\n@! Hector\nson of Priam.\n!@\n",
    )
    .unwrap();
    let out = idempotent_kanon(
        &dir,
        "@@@!plain\n\n@!() Paris\nhis brother.\n!@\n\n@@@(entry.atd)\n",
    );
    assert_eq!(
        out,
        "@@@!plain\n\n@!(1) Paris\nhis brother.\n!@\n\n@@@!(review)\n\
         @! Hector\nson of Priam.\n!@(Hector)\n!@@@\n"
    );
}

#[test]
fn onym_canonicalization_scales_with_the_number_of_onyms() {
    // 20 000 referenced onyms: membership tests over a list made
    // this quadratic (tens of seconds in a debug build); with sets
    // it is a fraction of a second.
    let dir = review_dir("onym-scale");
    let n = 20_000;
    let mut src = String::from("@@@!review\n\n");
    for i in 0..n {
        src.push_str(&format!("@,Iliad,@(book{i}) @&(book{i})\n\n"));
    }
    let start = std::time::Instant::now();
    let out = kanon(&dir, &src);
    let elapsed = start.elapsed();
    assert!(
        out.contains(&format!("@,Iliad,@(o{n}) @&(o{n})\n")),
        "last onym"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "kanonizo of {n} onyms took {elapsed:?}"
    );
}

#[test]
fn blank_taxis_on_an_autonym_sim_numbers_per_lemma() {
    // No `()` survives into the kanon: an autonym sim's blank
    // taxis resolves in its own sequence, per lemma in document
    // order, and suffixes the pinned onym like an explicit one.
    let dir = review_dir("autonym-blank-taxis");
    let out = idempotent_kanon(
        &dir,
        "@@@!review\n\n@!() Hector\nson of Priam.\n!@\n\n@!() Ajax\nthe greater.\n!@\n\n\
         @!() Ajax\nthe lesser.\n!@\n",
    );
    assert_eq!(
        out,
        "@@@!review\n\n@!(1) Hector\nson of Priam.\n!@(Hector-1)\n\n\
         @!(1) Ajax\nthe greater.\n!@(Ajax-1)\n\n@!(2) Ajax\nthe lesser.\n!@(Ajax-2)\n"
    );
    assert!(!out.contains("()"), "{out}");
}

/// A stichos holding only an unreferenced anchor used to survive
/// as an empty line, which serializes as a strophe break.
#[test]
fn anchor_only_stichos_does_not_split_its_strophe() {
    let dir = tmp_dir("anchor-stichos");
    let src = "@@@!exempli\n\n@@@=\nalpha\n@(x)\nbeta\n=@@@\n";
    let k = kanon(&dir, src);
    assert!(k.contains("@@@=\nalpha\nbeta\n=@@@"), "{k}");
    assert_eq!(kanon(&dir, &k), k);
    // A referenced anchor keeps its line.
    let dir = review_dir("anchor-stichos-kept");
    let src = "@@@!review\n\n@@@=\nalpha\n@(x)\nbeta\n=@@@\n\nSee @&(x).\n";
    let k = idempotent_kanon(&dir, src);
    assert!(k.contains("alpha\n@(o1)\nbeta"), "{k}");
}
