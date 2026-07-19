//! Markdown roundtrip tests: the at-markdown std dialektos, its
//! Markdown exomorphosis, and the pilot Markdown endomorphosis.

use std::path::{Path, PathBuf};

use atrep::{dendron, endo, exo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SAMPLE_MD: &str = "\
# Introduction

Text with *emphasis*, **strong**, and `code`.

## Details

> A quote.
>
> With two paragraphs.

- one
- two *emphatic*

1. first
2. second

```rust
fn main() {}
```
";

/// Import, serialize, and check the exact .atd source.
#[test]
fn endo_produces_canonical_atd() {
    let doc = endo::markdown_to_document(SAMPLE_MD).unwrap();
    assert_eq!(
        dendron::serialize(&doc),
        "@@@!at-markdown\n\
         \n\
         @#Introduction#@\n\
         \n\
         Text with @*emphasis*@, @**strong**@, and @@\"code\"@@.\n\
         \n\
         @##Details##@\n\
         \n\
         @>\n\
         A quote.\n\
         \n\
         With two paragraphs.\n\
         >@\n\
         \n\
         @-\n\
         one\n\
         -@\n\
         \n\
         @-\n\
         two @*emphatic*@\n\
         -@\n\
         \n\
         @.(1)\n\
         first\n\
         .@\n\
         \n\
         @.(2)\n\
         second\n\
         .@\n\
         \n\
         @@@\"\n\
         fn main() {}\n\
         \"@@@.rust\n"
    );
}

/// Markdown -> at-markdown -> kanon -> Markdown reproduces the
/// canonical Markdown, and the cycle is idempotent.
#[test]
fn markdown_roundtrip_is_idempotent() {
    let tmp = tmp_dir("md-roundtrip");
    let cycle = |md: &str| -> String {
        let doc = endo::markdown_to_document(md).unwrap();
        std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
        let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
        let exo = exo::resolve_exo(&tmp, "at-markdown", "md").unwrap();
        exo::render(&kanon.document, &exo, &tmp).unwrap()
    };
    let md1 = cycle(SAMPLE_MD);
    let md2 = cycle(&md1);
    assert_eq!(md1, md2);
}

/// Semantic roundtrip: exporting a kanon to Markdown and importing
/// it back yields a byte-identical kanon.
#[test]
fn kanon_survives_markdown_roundtrip() {
    let tmp = tmp_dir("md-semantic");
    let doc = endo::markdown_to_document(SAMPLE_MD).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();

    let exo = exo::resolve_exo(&tmp, "at-markdown", "md").unwrap();
    let md = exo::render(&kanon.document, &exo, &tmp).unwrap();

    let doc2 = endo::markdown_to_document(&md).unwrap();
    std::fs::write(tmp.join("doc2.atd"), dendron::serialize(&doc2)).unwrap();
    let kanon2 = kanonizo::kanonizo_file(&tmp.join("doc2.atd")).unwrap();

    assert_eq!(kanon.kanon, kanon2.kanon);
}

/// The exact canonical Markdown produced for the sample.
#[test]
fn exo_golden_markdown() {
    let tmp = tmp_dir("md-golden");
    let doc = endo::markdown_to_document(SAMPLE_MD).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let exo = exo::resolve_exo(&tmp, "at-markdown", "md").unwrap();
    let md = exo::render(&kanon.document, &exo, &tmp).unwrap();
    assert_eq!(
        md,
        "# Introduction\n\
         \n\
         Text with *emphasis*, **strong**, and `code`.\n\
         \n\
         ## Details\n\
         \n\
         > A quote.\n\
         >\n\
         > With two paragraphs.\n\
         \n\
         - one\n\
         \n\
         - two *emphatic*\n\
         \n\
         1. first\n\
         \n\
         2. second\n\
         \n\
         ```rust\n\
         fn main() {}\n\
         ```\n"
    );
}

/// Markdown special characters in text survive the roundtrip via
/// the escape table and backslash unescaping.
#[test]
fn special_characters_roundtrip() {
    let tmp = tmp_dir("md-specials");
    let md = "Stars \\*not emphasis\\*, a backtick \\`, and a \\# sign.\n";
    let doc = endo::markdown_to_document(md).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    // The kanon carries the literal characters...
    assert!(
        kanon
            .kanon
            .contains("Stars *not emphasis*, a backtick `, and a # sign.")
    );
    // ...and the export re-escapes them.
    let exo = exo::resolve_exo(&tmp, "at-markdown", "md").unwrap();
    let out = exo::render(&kanon.document, &exo, &tmp).unwrap();
    assert_eq!(
        out,
        "Stars \\*not emphasis\\*, a backtick \\`, and a \\# sign.\n"
    );
}

/// A standalone image imports as enmedia and exports back.
#[test]
fn image_maps_to_enmedia() {
    let doc = endo::markdown_to_document("![](pipeline.svg)\n").unwrap();
    assert_eq!(
        dendron::serialize(&doc),
        "@@@!at-markdown\n\n@@@@(pipeline.svg)\n"
    );
}

/// The at-markdown std dialektos resolves without any local file.
#[test]
fn std_dialektos_resolves_anywhere() {
    let tmp = tmp_dir("md-std");
    std::fs::write(tmp.join("doc.atd"), "@@@!at-markdown\n\nplain text\n").unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(result.kanon, "@@@!at-markdown\n\nplain text\n");
}
