//! The deixis: a core pointing reference `@<sym>(<onym>)` to an
//! onymized para-simmere (litogramma F5), surviving litosis with
//! fresh renumbering (F4).

use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::{exo, kanonizo, litosis};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // `^` note body (taxis-less para), `=` figure (optional
    // taxis), `>` referencing monosim, `/` emphasis endo.
    std::fs::write(
        dir.join("notae.lektos"),
        "@@@!atrep\n\n\
         @=== emphasis\n@/ grammata /@\n===@\n\n\
         @=== figure\n@=[(taxis)] lemma\ngrammata\n=@\n===@\n\n\
         @=== note\n@^\ngrammata\n^@\n===@\n\n\
         @=== ref\n@>(onym)\n===@\n",
    )
    .unwrap();
    dir
}

fn kanon(tmp: &Path, name: &str, source: &str) -> atrep::Result<kanonizo::KanonResult> {
    std::fs::write(tmp.join(name), source).unwrap();
    kanonizo::kanonizo_file(&tmp.join(name))
}

#[test]
fn deixis_kanonizes_with_its_target() {
    let tmp = tmp_dir("deixis-kanon");
    let result = kanon(
        &tmp,
        "doc.atd",
        "@@@!notae\n\n\
         A claim@^(fn-note) needing support.\n\n\
         @^\n\
         The supporting @/note/@ text.\n\
         ^@(fn-note)\n",
    )
    .unwrap();
    assert_eq!(
        result.kanon,
        "@@@!notae\n\n\
         A claim@^(o1) needing support.\n\n\
         @^\n\
         The supporting @/note/@ text.\n\
         ^@(o1)\n"
    );
    // Idempotent through a reparse.
    std::fs::write(tmp.join("doc.atk"), &result.kanon).unwrap();
    let again = kanonizo::kanonizo_file(&tmp.join("doc.atk")).unwrap();
    assert_eq!(again.kanon, result.kanon);
}

#[test]
fn line_start_disambiguation() {
    let tmp = tmp_dir("deixis-linestart");
    // Taxis-less para + group at line start: a paragraph deixis.
    let result = kanon(
        &tmp,
        "solo.atd",
        "@@@!notae\n\n@^(n1)\n\n@^\nBody.\n^@(n1)\n",
    )
    .unwrap();
    assert!(result.kanon.contains("@^(o1)\n"));
    // Figure with numeric/empty group still opens a block.
    let result = kanon(&tmp, "fig.atd", "@@@!notae\n\n@=() Plate\nbody\n=@\n").unwrap();
    assert!(result.kanon.contains("@=(1) Plate"));
    // Figure with a non-numeric group at line start: a deixis.
    let result = kanon(
        &tmp,
        "figref.atd",
        "@@@!notae\n\n@=() Plate\nbody\n=@(pl)\n\n@=(pl)\n",
    )
    .unwrap();
    assert!(result.kanon.contains("=@(o1)"));
    assert!(result.kanon.ends_with("@=(o1)\n"));
}

#[test]
fn deixis_errors() {
    let tmp = tmp_dir("deixis-errors");
    // Undefined onym.
    let err = kanon(&tmp, "undef.atd", "@@@!notae\n\nSee@^(nope) here.\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::DeixisUndefinedOnym(_)));
    // Onym attached to a figure, referenced as a note.
    let err = kanon(
        &tmp,
        "mismatch.atd",
        "@@@!notae\n\nSee@^(fig) here.\n\n@=() Plate\nbody\n=@(fig)\n",
    )
    .unwrap_err();
    assert!(matches!(err.kind, ErrorKind::DeixisTargetMismatch { .. }));
    // Onym attached to an inline.
    let err = kanon(
        &tmp,
        "inline.atd",
        "@@@!notae\n\nSee@^(em) and @/word/@(em).\n",
    )
    .unwrap_err();
    assert!(matches!(err.kind, ErrorKind::DeixisTargetMismatch { .. }));
    // A para-sim symbol inline without a parenthesis still mimics.
    let err = kanon(&tmp, "mimic.atd", "@@@!notae\n\ntext @^ misuse\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::EndoMimicsPara));
}

/// The litos keeps the deixis and its target's onym, renumbered
/// afresh so stripped metadata cannot influence the hash.
#[test]
fn litos_keeps_deixes_with_fresh_numbering() {
    let tmp = tmp_dir("deixis-litos");
    // A monosim-referenced figure (kanon o1) precedes the
    // deixis-referenced note (kanon o2).
    let source = "@@@!notae\n\n\
        @=() Plate\nbody\n=@(fig)\n\n\
        See @>(fig) and the claim@^(fn) here.\n\n\
        @^\nNote text.\n^@(fn)\n";
    let result = kanon(&tmp, "doc.atd", source).unwrap();
    assert!(result.kanon.contains("=@(o1)"));
    assert!(result.kanon.contains("@^(o2)"));

    let lookup = litosis::media_from_dir(&tmp);
    let litos = litosis::litosis(&result.document, &lookup).unwrap();
    // The figure's onym and the @> monosim are stripped; the note
    // pair survives, renumbered afresh from o1.
    assert_eq!(
        litos.litos,
        "@@@!notae\n\n\
         @=(1) Plate\nbody\n=@\n\n\
         See and the claim@^(o1) here.\n\n\
         @^\nNote text.\n^@(o1)\n"
    );

    // Removing the figure's monosim reference (stripped metadata)
    // must not change the litos ID.
    let source2 = "@@@!notae\n\n\
        @=() Plate\nbody\n=@\n\n\
        See and the claim@^(fn) here.\n\n\
        @^\nNote text.\n^@(fn)\n";
    let result2 = kanon(&tmp, "doc2.atd", source2).unwrap();
    let litos2 = litosis::litosis(&result2.document, &lookup).unwrap();
    assert_eq!(litos.litos_id, litos2.litos_id);

    // Renaming the source identifier must not change it either.
    let source3 = source
        .replace("(fn)", "(anmerkung)")
        .replace("@^(fn)", "@^(anmerkung)");
    let result3 = kanon(&tmp, "doc3.atd", &source3).unwrap();
    let litos3 = litosis::litosis(&result3.document, &lookup).unwrap();
    assert_eq!(litos.litos_id, litos3.litos_id);
}

#[test]
fn deixis_renders_via_exo_pattern() {
    let tmp = tmp_dir("deixis-exo");
    std::fs::write(
        tmp.join("notae.html.exo"),
        "@@@!atrep-exo\n@=notae=>html\n\n\
         @-> *document\n@(grammata)\n>-@\n\n\
         @-> *paragraph\n<p>@(grammata)</p>\n>-@\n\n\
         @-> /\n<em>@(grammata)</em>\n>-@\n\n\
         @-> ^\n<aside class=\"note\" id=\"@(onym)\">\n@(grammata)\n</aside>\n>-@\n\n\
         @-> *deixis ^\n<sup><a href=\"#@(onym)\">*</a></sup>\n>-@\n",
    )
    .unwrap();
    let result = kanon(
        &tmp,
        "doc.atd",
        "@@@!notae\n\nA claim@^(fn) here.\n\n@^\nNote.\n^@(fn)\n",
    )
    .unwrap();
    let x = exo::resolve_exo(&tmp, "notae", "html").unwrap();
    let html = exo::render(&result.document, &x, &tmp).unwrap();
    assert_eq!(
        html,
        "<p>A claim<sup><a href=\"#o1\">*</a></sup> here.</p>\n\
         <aside class=\"note\" id=\"o1\">\n\
         <p>Note.</p>\n\
         </aside>"
    );
}
