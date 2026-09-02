//! at-djot and the graph's first standard isomorphism: the
//! at-djot <=> at-markdown pair converts losslessly in both
//! directions, round trips to the identity, and the two
//! dialektoi embed each other.

use std::path::{Path, PathBuf};

use atrep::{dendron, endo, exo, kanonizo, morph};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SAMPLE_DJ: &str = "\
# Solitude

Text with _emphasis_, *strength*, and `x < y` beside a
\\*literal star\\*.

> Quoted thought.

- first point
- second point

1. one
2. two

``` rust
fn quiet() {}
```
";

#[test]
fn djot_endo_and_roundtrip() {
    let tmp = tmp_dir("djot-roundtrip");
    let doc = endo::djot_to_document(SAMPLE_DJ).unwrap();
    assert_eq!(doc.dialect_id, "at-djot");
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@#Solitude#@"));
    assert!(atd.contains("@_emphasis_@, @*strength*@"));
    assert!(atd.contains("*literal star*"));

    let cycle = |dj: &str| -> String {
        let doc = endo::djot_to_document(dj).unwrap();
        std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
        let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
        let x = exo::resolve_exo(&tmp, "at-djot", "dj").unwrap();
        exo::render(&kanon.document, &x, &tmp).unwrap()
    };
    let dj1 = cycle(SAMPLE_DJ);
    let dj2 = cycle(&dj1);
    assert_eq!(dj1, dj2);
    assert!(dj1.contains("\\*literal star\\*"));
}

/// The std iso: both directions resolve from one file, the
/// round trip is the identity morphism, and a document
/// converts losslessly there and back.
#[test]
fn std_iso_is_lossless_both_ways() {
    let tmp = tmp_dir("djot-iso");
    let there = morph::resolve_morph(&tmp, "at-djot", "at-markdown").unwrap();
    let back = morph::resolve_morph(&tmp, "at-markdown", "at-djot").unwrap();
    assert!(morph::compose(&there, &back).unwrap().is_identity());
    assert!(morph::compose(&back, &there).unwrap().is_identity());

    let doc = endo::djot_to_document(SAMPLE_DJ).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let original = dendron::serialize(&kanon.document);

    let md = morph::apply(&kanon.document, &there).unwrap();
    let md_kanon = dendron::serialize(&md);
    assert!(md_kanon.starts_with("@@@!at-markdown\n"));
    assert!(md_kanon.contains("@*emphasis*@, @**strength**@"));
    let again = morph::apply(&md, &back).unwrap();
    assert_eq!(dendron::serialize(&again), original);

    // Onward through the graph: at-djot reaches at-html via the
    // iso plus at-markdown's derived embedding.
    let route = morph::resolve_route(&tmp, "at-djot", "at-html").unwrap();
    let hops: Vec<(&str, &str)> = route
        .iter()
        .map(|m| (m.source.as_str(), m.target.as_str()))
        .collect();
    assert_eq!(
        hops,
        [("at-djot", "at-markdown"), ("at-markdown", "at-html")]
    );
}

/// The iso pair reads links identically: autolinks are the link
/// sim, inline links project — and the djot exo re-emits the
/// autolink.
#[test]
fn djot_links_import() {
    let doc = endo::djot_to_document(
        "See <https://quarb.org/spec> and [the guide](https://quarb.org/guide).\n",
    )
    .unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@><https://quarb.org/spec><@"),
        "autolink: {atd}"
    );
    assert!(
        atd.contains("the guide (@><https://quarb.org/guide><@)"),
        "inline link projects: {atd}"
    );
}
