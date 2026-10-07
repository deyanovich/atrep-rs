//! Review follow-up (group atra): regressions for the atramento
//! compiler and the FictionBook 2 import/export.

use std::path::{Path, PathBuf};

use atrep::atramento::atramento_to_litogramma as compile;
use atrep::{dendron, fb2, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn c(src: &str) -> String {
    compile(src).unwrap()
}

/// Wrap a main body (and optional notes body) in a FictionBook.
fn book(body: &str, notes: &str) -> String {
    format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description><title-info><author><first-name>Mark</first-name><last-name>Twain</last-name></author><book-title>Tom Sawyer</book-title></title-info></description>
<body>{body}</body>
{notes}</FictionBook>
"##
    )
}

// ----- fb2: media ids --------------------------------------------

/// An image href and a binary id are file names under media/:
/// separators are flattened, and dot-only ids name nothing, so no
/// crafted file can point the bundle outside the document dir.
#[test]
fn fb2_media_ids_stay_inside_media_dir() {
    let src = book(
        r##"<section><title><p>Pic</p></title><image l:href="#../../../../etc/hostname"/><image l:href="#.."/><image l:href="#sub/pic.png"/><p>Caption.</p></section>"##,
        r##"<binary id=".." content-type="image/png">AAAA</binary>
<binary id="." content-type="image/png">AAAA</binary>
<binary id="sub/pic.png" content-type="image/png">AAAA</binary>
<binary id="..\..\x.png" content-type="image/png">AAAA</binary>
"##,
    );
    let (doc, media) = fb2::fb2_to_document_with_media(&src).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@@@@(media/..-..-..-..-etc-hostname)"),
        "{atd}"
    );
    assert!(atd.contains("@@@@(media/sub-pic.png)"), "{atd}");
    assert!(!atd.contains("media/..)"), "{atd}");
    assert!(!atd.contains("media/../"), "{atd}");
    let ids: Vec<&str> = media.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["sub-pic.png", "..-..-x.png"]);
    for id in ids {
        assert!(!id.contains('/') && !id.contains('\\'));
        assert!(id != "." && id != "..");
    }
}

// ----- fb2: nesting bound ----------------------------------------

#[test]
fn fb2_deep_inline_nesting_is_an_error_not_a_crash() {
    let deep = format!(
        "<section><title><p>Deep</p></title><p>{}x{}</p></section>",
        "<strong>".repeat(2500),
        "</strong>".repeat(2500)
    );
    let err = fb2::fb2_to_document(&book(&deep, "")).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("nested deeper than"), "{msg}");
}

#[test]
fn fb2_deep_block_nesting_is_an_error_not_a_crash() {
    let deep = format!(
        "{}<p>x</p>{}",
        "<section>".repeat(2500),
        "</section>".repeat(2500)
    );
    let err = fb2::fb2_to_document(&book(&deep, "")).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("nested deeper than"), "{msg}");
}

// ----- fb2: links and notes --------------------------------------

/// Only a type="note" link is a callout; another internal link is
/// a reference to its section when that section exists, else its
/// text; the reference exports as a link and re-imports as itself.
#[test]
fn fb2_internal_links_are_references_not_callouts() {
    let src = book(
        r##"<section id="chap1"><title><p>One</p></title><p>See <a l:href="#chap2">chapter two</a>, <a l:href="#nowhere">nowhere</a> and <a l:href="#n1">a note</a>.</p></section><section id="chap2"><title><p>Two</p></title><p>Here.</p></section>"##,
        r##"<body name="notes"><section id="n1"><title><p>1</p></title><p>The note.</p></section></body>
"##,
    );
    let doc = fb2::fb2_to_document(&src).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("See @>(chap2), nowhere and a note."), "{atd}");
    let tmp = tmp_dir("review-atra-links");
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document;
    let out = fb2::document_to_fb2(&kanon);
    assert!(
        out.contains(r##"See <a l:href="#o1">[o1]</a>, nowhere"##),
        "{out}"
    );
    let again = fb2::fb2_to_document(&out).unwrap();
    assert!(
        dendron::serialize(&again).contains("See @>(o1), nowhere"),
        "{}",
        dendron::serialize(&again)
    );
}

/// A note id that is not a valid onym is renamed, not dropped, on
/// the callout and the body alike.
#[test]
fn fb2_invalid_note_ids_are_renamed() {
    let src = book(
        r##"<section><title><p>Notes</p></title><p>Text<a l:href="#_1" type="note">[1]</a> more<a l:href="#n-2" type="note">[2]</a>.</p></section>"##,
        r##"<body name="notes"><section id="_1"><title><p>1</p></title><p>Underscore note.</p></section><section id="n-2"><title><p>2</p></title><p>Dash note.</p></section></body>
"##,
    );
    let doc = fb2::fb2_to_document(&src).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("Text@^(fb2-1) more@^(n-2)."), "{atd}");
    assert!(atd.contains("@^\nUnderscore note.\n^@(fb2-1)"), "{atd}");
    assert!(atd.contains("@^\nDash note.\n^@(n-2)"), "{atd}");
    let tmp = tmp_dir("review-atra-note-ids");
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
}

/// A callout whose note has no body keeps its text instead of
/// becoming a deixis kanonizo refuses.
#[test]
fn fb2_dangling_callout_stays_text() {
    let src = book(
        r##"<section><title><p>One</p></title><p>A note<a l:href="#n1" type="note">[1]</a> without body.</p></section>"##,
        "",
    );
    let doc = fb2::fb2_to_document(&src).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("A note[1] without body."), "{atd}");
    let tmp = tmp_dir("review-atra-dangling");
    std::fs::write(tmp.join("doc.atd"), &atd).unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
}

/// An external link whose text is its URL (the exporter's shape)
/// imports as the autolink alone, so the URL is not doubled on
/// every pass; other link text keeps the URL after it.
#[test]
fn fb2_external_link_text_round_trips() {
    let src = book(
        r##"<section><title><p>One</p></title><p>At <a l:href="http://example.org/odyssey">http://example.org/odyssey</a> and <a l:href="http://example.org/iliad">the Iliad</a>.</p></section>"##,
        "",
    );
    let doc = fb2::fb2_to_document(&src).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains(
            "At @><http://example.org/odyssey><@ and the Iliad (@><http://example.org/iliad><@)."
        ),
        "{atd}"
    );
    let out = fb2::document_to_fb2(&doc);
    assert!(
        out.contains(
            r##"At <a l:href="http://example.org/odyssey">http://example.org/odyssey</a> and"##
        ),
        "{out}"
    );
    let again = fb2::fb2_to_document(&out).unwrap();
    assert_eq!(dendron::serialize(&again), atd);
}

/// Every author in title-info is kept, and exported again.
#[test]
fn fb2_multiple_authors() {
    let src = r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description><title-info><author><first-name>Mark</first-name><last-name>Twain</last-name></author><author><first-name>Jane</first-name><last-name>Austen</last-name></author><book-title>Two Hands</book-title></title-info></description>
<body><section><title><p>One</p></title><p>Text.</p></section></body>
</FictionBook>
"##;
    let doc = fb2::fb2_to_document(src).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@=:Mark Twain:=@\n\n@=:Jane Austen:=@"),
        "{atd}"
    );
    let out = fb2::document_to_fb2(&doc);
    assert!(
        out.contains("<author><first-name>Mark</first-name><last-name>Twain</last-name></author>\n<author><first-name>Jane</first-name><last-name>Austen</last-name></author>\n"),
        "{out}"
    );
}

/// A note body nested inside a division is exported to the notes
/// body all the same, and a section reference beside its callout
/// is written as a link rather than dropped.
#[test]
fn fb2_export_gathers_nested_notes() {
    let atd = "\
@@@!litogramma

@#(1) One
Text@^(n1) and see @>(two).

@^
Nested note.
^@(n1)
#@

@#(2) Two
More.
#@(two)
";
    let tmp = tmp_dir("review-atra-nested-notes");
    std::fs::write(tmp.join("doc.atd"), atd).unwrap();
    let doc = atrep::check_file(&tmp.join("doc.atd")).unwrap();
    let out = fb2::document_to_fb2(&doc);
    assert!(
        out.contains(r##"<p>Text<a l:href="#n1" type="note">[1]</a> and see <a l:href="#two">[two]</a>.</p>"##),
        "{out}"
    );
    assert!(
        out.contains("<body name=\"notes\">\n<section id=\"n1\">\n<title><p>1</p></title>\n<p>Nested note.</p>\n</section>\n</body>"),
        "{out}"
    );
}

// ----- atramento: notes ------------------------------------------

/// An inline note that opens a paragraph is an inline note, not
/// a definition named after its first word.
#[test]
fn atramento_inline_note_opens_a_paragraph() {
    let out = c("@^This is an inline note.^@ Then the paragraph of Tom Sawyer continues.\n");
    assert!(
        out.contains("@^(atr-fn-1) Then the paragraph of Tom Sawyer continues.\n"),
        "{out}"
    );
    assert!(
        out.contains("@^\nThis is an inline note.\n^@(atr-fn-1)"),
        "{out}"
    );
    assert!(!out.contains("^@(This)"), "{out}");
}

/// A callout that lands at the start of a wrapped line stays a
/// callout; only a paragraph starting with one is a definition.
#[test]
fn atramento_callout_at_line_start_stays_in_its_paragraph() {
    let out = c("Some text of Tom Sawyer that wraps\n@^13 and continues here.\n\n@^13 The note.\n");
    assert!(
        out.contains("Some text of Tom Sawyer that wraps\n@^(13) and continues here.\n"),
        "{out}"
    );
    assert_eq!(out.matches("^@(13)").count(), 1, "{out}");
    assert!(out.contains("@^\nThe note.\n^@(13)"), "{out}");
    // Consecutive definition lines are still one definition each.
    let out = c("Text.@^a More.@^b\n\n@^a First note.\n@^b: Second note.\n");
    assert!(out.contains("@^\nFirst note.\n^@(a)"), "{out}");
    assert!(out.contains("@^\nSecond note.\n^@(b)"), "{out}");
}

// ----- atramento: dialogue closure -------------------------------

/// The episim of the enclosing division ends an open dialogue:
/// the implied `:@` goes before it, not after what follows.
#[test]
fn atramento_division_episim_closes_dialogue() {
    let out = c("@#(1) The Whale\n\n@: Ishmael\nCall me Ishmael.\n#@\n\nAfter the division.\n");
    assert!(
        out.contains("@: Ishmael\nCall me Ishmael.\n:@\n#@\n\nAfter the division.\n"),
        "{out}"
    );
    assert!(out.trim_end().ends_with("After the division."), "{out}");
    let tmp = tmp_dir("review-atra-dialogue-div");
    std::fs::write(tmp.join("doc.atd"), &out).unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap_or_else(|e| panic!("kanonizo failed: {e}\non:\n{out}"));

    // A sugar heading closed by hand, a verse speech inside it.
    let out = c("# The Whale\n\n@:~ Ishmael\nCall me Ishmael.\n#@\n\nAfter.\n");
    assert!(
        out.contains("@:~ Ishmael\nCall me Ishmael.\n~:@\n#@\n\nAfter.\n"),
        "{out}"
    );
    assert_eq!(out.matches("#@").count(), 1, "{out}");
}

/// A block stage direction ends the speech before it.
#[test]
fn atramento_stage_direction_closes_dialogue() {
    let out = c("@: Estragon\nWell?\n\n@:[ They do not move.\n\n@: Vladimir\nYes.\n");
    assert!(
        out.contains(
            "@: Estragon\nWell?\n:@\n\n@:[\nThey do not move.\n]:@\n\n@: Vladimir\nYes.\n:@\n"
        ),
        "{out}"
    );
}

// ----- atramento: lists ------------------------------------------

/// Continuation lines and nested lists align under the item's
/// text, which for a numbered item is three or more columns in.
#[test]
fn atramento_list_continuation_dedents_to_text_column() {
    let out = c(
        "1. first item\n   - nested under numbered\n   continuation\n13. second\n    - nested deeper\n",
    );
    let expected = "\
@..
@.-(1)
first item
@--
@-
nested under numbered
-@
--@
continuation
-.@
@.-(2)
second
@--
@-
nested deeper
-@
--@
-.@
..@
";
    assert!(out.ends_with(expected), "{out}");
    // Two columns under a `- ` item, as before.
    let out = c("- item one\n\n  still item one\n  - nested item\n");
    assert!(
        out.contains("@-\nitem one\n\nstill item one\n@--\n@-\nnested item\n-@\n--@\n-@"),
        "{out}"
    );
}

/// A list ordinal past the range of the taxis is an error, not a
/// panic (debug) or a wrapped number (release).
#[test]
fn atramento_list_ordinal_overflow_is_an_error() {
    let err = compile("@.. ZZZZZZZZZ\n. first\n. second\n").unwrap_err();
    assert!(format!("{err}").contains("out of range"), "{err}");
    let err = compile("4294967295. first\n. second\n").unwrap_err();
    assert!(format!("{err}").contains("out of range"), "{err}");
    // The last ordinal that fits still compiles.
    let out = c("4294967295. first\n");
    assert!(out.contains("@.-(4294967295)"), "{out}");
    let out = c("@.. AA\n. first\n. second\n");
    assert!(out.contains("@.-(AA)") && out.contains("@.-(AB)"), "{out}");
}

// ----- atramento: emphasis ---------------------------------------

/// A delimiter opens only after nothing, whitespace, or opening
/// punctuation, so the slashes of a bare URL stay literal.
#[test]
fn atramento_emphasis_opens_only_after_space_or_opening_punctuation() {
    let out = c("Visit https://example.org/odyssey/ for the Homer text.\n");
    assert!(
        out.contains("Visit https://example.org/odyssey/ for the Homer text."),
        "{out}"
    );
    assert!(!out.contains("@/"), "{out}");
    let out = c("See a:/b/ and 1-*2* here.\n");
    assert!(out.contains("See a:/b/ and 1-*2* here."), "{out}");
    // Brackets, quotation marks and an elision still open, and so
    // does a delimiter directly inside another.
    let out = c("An (/aside/), a \"/quoted/\" word, l'/Odyssée/, a /*nested*/ pair.\n");
    assert!(
        out.contains(
            "An (@/aside/@), a \"@/quoted/@\" word, l'@/Odyssée/@, a @/@*nested*@/@ pair."
        ),
        "{out}"
    );
}

/// A paragraph dense in openers without closers compiles in
/// linear time (it took minutes when every opener rescanned the
/// paragraph for its closer).
#[test]
fn atramento_unclosed_openers_compile_in_linear_time() {
    let src = format!("Prose{}\n", " /a".repeat(20_000));
    let started = std::time::Instant::now();
    let out = c(&src);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
    assert!(
        out.contains("Prose /a /a /a"),
        "unclosed openers stay literal"
    );
    assert!(!out.contains("@/"));
    // The answer is unchanged: a closer ahead still opens, one
    // behind does not.
    let out = c("a /b/ c /d\n");
    assert!(out.contains("a @/b/@ c /d"), "{out}");
}
