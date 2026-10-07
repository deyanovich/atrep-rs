//! Regression tests for the core-group review findings (parser,
//! lib entry points, litosis).

use std::path::{Path, PathBuf};

use atrep::dendron::{Block, Inline};
use atrep::error::ErrorKind;
use atrep::{kanonizo, litosis, parser};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Parse a source string as if it lived in the fixtures directory
/// (so the `exempli` dialektos resolves).
fn parse(source: &str) -> atrep::Result<atrep::Document> {
    parser::parse_document(source, &fixtures().join("virtual.atd"))
}

/// A scratch directory holding a copy of the `exempli` dialektos,
/// for the file-based entry points.
fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        fixtures().join("exempli.lektos"),
        dir.join("exempli.lektos"),
    )
    .unwrap();
    dir
}

fn kanon(tmp: &Path, name: &str, source: &str) -> atrep::Result<kanonizo::KanonResult> {
    std::fs::write(tmp.join(name), source).unwrap();
    kanonizo::kanonizo_file(&tmp.join(name))
}

/// Kanonize twice; the kanon must be a fixed point.
fn kanon_idempotent(tmp: &Path, source: &str) -> String {
    let first = kanon(tmp, "doc.atd", source).unwrap();
    let again = kanon(tmp, "doc.atk", &first.kanon).unwrap();
    assert_eq!(again.kanon, first.kanon, "kanonizo is not idempotent");
    first.kanon
}

// parser.rs:896 — a continuation line that opens with a deixis
// stays in its paragraph.

#[test]
fn deixis_at_line_start_continues_the_paragraph() {
    let tmp = tmp_dir("review-core-deixis-cont");
    let kanon = kanon_idempotent(
        &tmp,
        "@@@!exempli\n\n\
         See the figure.\n\
         @=(fig) shows the pipeline.\n\
         More text.\n\n\
         @=() Fig\nbody\n=@(fig)\n",
    );
    assert_eq!(
        kanon,
        "@@@!exempli\n\n\
         See the figure. @=(o1) shows the pipeline. More text.\n\n\
         @=(1) Fig\nbody\n=@(o1)\n"
    );
    // The plerographic spelling takes the same path.
    let kanon = kanon_idempotent(
        &tmp,
        "@@@!exempli\n\n\
         See the figure.\n\
         @{figure}(fig) shows the pipeline.\n\
         More text.\n\n\
         @=() Fig\nbody\n=@(fig)\n",
    );
    assert_eq!(
        kanon,
        "@@@!exempli\n\n\
         See the figure. @=(o1) shows the pipeline. More text.\n\n\
         @=(1) Fig\nbody\n=@(o1)\n"
    );
    // A line-initial reference that is a real block start still
    // breaks the paragraph.
    let doc = parse("@@@!exempli\n\ntext\n@=() Fig\nbody\n=@\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
}

// parser.rs:1250 and parse_blocks — nesting is bounded by a clear
// error, not a stack overflow.

/// Run on a thread with a stack large enough to reach the bound in
/// a debug build (test threads default to 2 MB).
fn on_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

fn is_nesting_error(err: &atrep::Error) -> bool {
    matches!(&err.kind, ErrorKind::Syntax(m) if m.contains("nesting exceeds"))
}

#[test]
fn deep_nesting_is_a_syntax_error() {
    on_big_stack(|| {
        // Inline simmeres on one line.
        let endo = format!("@@@!exempli\n\n{}x{}\n", "@:".repeat(500), ":@".repeat(500));
        assert!(is_nesting_error(&parse(&endo).unwrap_err()));
        // Para blocks.
        let para = format!(
            "@@@!exempli\n\n{}x\n{}",
            "@# T\n".repeat(800),
            "#@\n".repeat(800)
        );
        assert!(is_nesting_error(&parse(&para).unwrap_err()));
        // Diaphanes.
        let dia = format!(
            "@@@!exempli\n\n{}x\n{}",
            "@@@.\n".repeat(500),
            ".@@@\n".repeat(500)
        );
        assert!(is_nesting_error(&parse(&dia).unwrap_err()));
        // Moderate nesting of either kind still parses.
        let endo = format!("@@@!exempli\n\n{}x{}\n", "@:".repeat(100), ":@".repeat(100));
        parse(&endo).unwrap();
        let para = format!(
            "@@@!exempli\n\n{}x\n{}",
            "@# T\n".repeat(100),
            "#@\n".repeat(100)
        );
        parse(&para).unwrap();
    });
}

// parser.rs:1226 — resolving a sim reference must not copy the
// rest of the paragraph.

#[test]
fn many_sim_references_scan_in_linear_time() {
    let n = 20_000;
    let source = format!("@@@!exempli\n\n{}\n", "@:w:@ ".repeat(n));
    let started = std::time::Instant::now();
    let doc = parse(&source).unwrap();
    let elapsed = started.elapsed();
    let Block::Paragraph(inlines) = &doc.blocks[0] else {
        panic!("expected a paragraph");
    };
    assert_eq!(
        inlines
            .iter()
            .filter(|i| matches!(i, Inline::Endo { .. }))
            .count(),
        n
    );
    // Quadratic, this took ~26 s in a debug build.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "scanning {n} references took {elapsed:?}"
    );
}

// parser.rs:622 and :802 — a comment-only line in stichoi is not a
// stichos (an empty one would serialize as a strophe break), and
// the whitespace before an end-of-line comment is trailing.

#[test]
fn comment_lines_in_stichoi_leave_no_stichos() {
    let tmp = tmp_dir("review-core-verse-comment");
    // Mid-block: the strophe stays whole.
    let kanon = kanon_idempotent(
        &tmp,
        "@@@!exempli\n\n@@@=\nline one\n@@/ note\nline two\n=@@@\n",
    );
    assert_eq!(kanon, "@@@!exempli\n\n@@@=\nline one\nline two\n=@@@\n");
    // First and last line of the block.
    let kanon = kanon_idempotent(
        &tmp,
        "@@@!exempli\n\n@@@=\n@@/ lead\nline one\nline two\n@@/ trail\n=@@@\n",
    );
    assert_eq!(kanon, "@@@!exempli\n\n@@@=\nline one\nline two\n=@@@\n");
    // End-of-line comment: the space before it is trailing.
    let kanon = kanon_idempotent(&tmp, "@@@!exempli\n\n@@@=\nalpha @@/ c\nbeta\n=@@@\n");
    assert_eq!(kanon, "@@@!exempli\n\n@@@=\nalpha\nbeta\n=@@@\n");
    // A real blank line still separates strophes, and a comment
    // beside it adds no third.
    let doc = parse("@@@!exempli\n\n@@@=\nalpha\n\n@@/ c\nbeta\n=@@@\n").unwrap();
    let Block::Stichoi { strophes, .. } = &doc.blocks[0] else {
        panic!("expected stichoi");
    };
    assert_eq!(strophes.len(), 2);
    assert_eq!(strophes[0].0.len(), 1);
    assert_eq!(strophes[1].0.len(), 1);
}

// parser.rs:905 — a line-initial multi-line comment whose closer
// carries trailing text glues into the paragraph, as step_trigraph
// already reads it.

#[test]
fn multi_line_comment_with_tail_stays_in_the_paragraph() {
    let tmp = tmp_dir("review-core-cmt-tail");
    let kanon = kanon_idempotent(
        &tmp,
        "@@@!exempli\n\nPara line one.\n@@@/ comment\n/@@@ tail\n",
    );
    assert_eq!(kanon, "@@@!exempli\n\nPara line one. tail\n");
    // A standalone multi-line comment still breaks the paragraph.
    let doc = parse("@@@!exempli\n\nPara one.\n@@@/ comment\n/@@@\nPara two.\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    // Closed on its own line with text before the closer, too.
    let doc = parse("@@@!exempli\n\nPara one.\n@@@/ comment\nmore /@@@\nPara two.\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
}

// lib.rs:47 — a leading byte-order mark is not content and does
// not hide the dialektos declaration.

#[test]
fn leading_bom_is_stripped() {
    let tmp = tmp_dir("review-core-bom");
    let source = "\u{feff}@@@!exempli\n\nhello\n";
    // In-memory entry points.
    let doc = parse(source).unwrap();
    assert_eq!(doc.dialect_id, "exempli");
    let atrep::Checked::Document(doc) =
        atrep::check_source(source, &fixtures().join("virtual.atd")).unwrap()
    else {
        panic!("expected a document");
    };
    assert_eq!(doc.dialect_id, "exempli");
    assert!(atrep::is_definition_source("\u{feff}@@@!atrep\n"));
    // File entry points, and the kanon carries no mark.
    std::fs::write(tmp.join("doc.atd"), source).unwrap();
    atrep::check_file(&tmp.join("doc.atd")).unwrap();
    atrep::check_any(&tmp.join("doc.atd")).unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(result.kanon, "@@@!exempli\n\nhello\n");
}

// parser.rs:863 — an error inside an indented paragraph line
// reports its column in the source, not in the trimmed text.

#[test]
fn error_column_counts_paragraph_indentation() {
    let err = parse("@@@!exempli\n\ntext\n   more @?bad?@\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UndefinedSim(_)));
    let loc = err.location.unwrap();
    assert_eq!((loc.line, loc.col), (4, 9));
    // Each line carries its own indentation.
    let err = parse("@@@!exempli\n\n  one\n     two @?bad?@\n").unwrap_err();
    let loc = err.location.unwrap();
    assert_eq!((loc.line, loc.col), (4, 10));
    // An unindented line is unaffected.
    let err = parse("@@@!exempli\n\n   text\nmore @?bad?@\n").unwrap_err();
    let loc = err.location.unwrap();
    assert_eq!((loc.line, loc.col), (4, 6));
}

// parser.rs:217 — an empty group on a taxis-less para-sim is an
// auto-taxis the sim does not support, not a deixis with an empty
// onym.

#[test]
fn empty_group_on_taxis_less_sim_is_a_taxis_violation() {
    let err = parse("@@@!exempli\n\n@#() Title\nx\n#@\n").unwrap_err();
    assert!(
        matches!(&err.kind, ErrorKind::ComponentViolation(m) if m.contains("does not support a taxis")),
        "{:?}",
        err.kind
    );
    // A non-empty group there is still a deixis in a paragraph.
    let doc = parse("@@@!exempli\n\n@#(intro) is the opening.\n").unwrap();
    let Block::Paragraph(inlines) = &doc.blocks[0] else {
        panic!("expected a paragraph");
    };
    assert!(matches!(&inlines[0], Inline::Deixis { onym, .. } if onym == "intro"));
}

// parser.rs:1519 — a malformed genos is an invalid genos, not
// stray hypograph text.

#[test]
fn uppercase_genos_is_an_invalid_genos() {
    let err = parse("@@@!exempli\n\n@=(1) Fig\nbody\n=@(o1).Bad\n").unwrap_err();
    assert!(
        matches!(&err.kind, ErrorKind::InvalidGenos(g) if g == "Bad"),
        "{:?}",
        err.kind
    );
    // Inline, and with the case fault past the first letter.
    let err = parse("@@@!exempli\n\na @:b:@.goodBad c\n").unwrap_err();
    assert!(
        matches!(&err.kind, ErrorKind::InvalidGenos(g) if g == "goodBad"),
        "{:?}",
        err.kind
    );
    // Well-formed genoses and a sentence period are unaffected.
    let doc = parse("@@@!exempli\n\na @:b:@.one.two-three, and @:c:@. End\n").unwrap();
    let Block::Paragraph(inlines) = &doc.blocks[0] else {
        panic!("expected a paragraph");
    };
    let Inline::Endo { ann, .. } = &inlines[1] else {
        panic!("expected an endo");
    };
    assert_eq!(ann.genoses, ["one", "two-three"]);
}

// litosis.rs:231 — a stichos holding only metadata strips to
// nothing and must not turn into a strophe break.

fn litos_of(source: &str) -> litosis::LitosResult {
    litosis::litosis(&parse(source).unwrap(), &|_| Ok(Vec::new())).unwrap()
}

#[test]
fn metadata_only_stichos_does_not_split_the_strophe() {
    let one_strophe = litos_of("@@@!exempli\n\n@@@=\nalpha\nbeta\n=@@@\n");
    let two_strophes = litos_of("@@@!exempli\n\n@@@=\nalpha\n\nbeta\n=@@@\n");
    assert_ne!(one_strophe.litos_id, two_strophes.litos_id);
    // An onym anchor alone on a line.
    let anchored = litos_of("@@@!exempli\n\n@@@=\nalpha\n@(x)\nbeta\n=@@@\n");
    assert!(
        anchored.litos.contains("alpha\nbeta\n"),
        "{}",
        anchored.litos
    );
    assert_eq!(anchored.litos_id, one_strophe.litos_id);
    // A monosim alone on a line.
    let mono = litos_of("@@@!exempli\n\n@@@=\nalpha\n@^(a)\nbeta\n=@@@\n");
    assert_eq!(mono.litos_id, one_strophe.litos_id);
    // A strophe of nothing but metadata vanishes whole.
    let lone = litos_of("@@@!exempli\n\n@@@=\nalpha\n\n@(x)\n\nbeta\n=@@@\n");
    assert_eq!(lone.litos_id, two_strophes.litos_id);
}
