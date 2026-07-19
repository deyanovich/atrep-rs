//! The at-org syntax mapper: import golden, canonical fixed
//! point, and transitive routes through the std graph.

use std::path::{Path, PathBuf};

use atrep::{dendron, endo, exo, kanonizo, morph};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SAMPLE_ORG: &str = "\
#+TITLE: Notes on Solitude
#+AUTHOR: skipped metadata

* Retirement

Pope praised /rural quiet/ and *self-sufficiency*, with
_measured_ days and +no crowds+, quoting [fn:1] the ancients.

[fn:1] Horace above all.

** Sources

- Horace :: the Epodes
- reading list
1. the Odes
2. the Epistles

#+BEGIN_QUOTE
Happy the man, whose wish and care a few paternal acres bound.
#+END_QUOTE

#+BEGIN_VERSE
Thus let me live, unseen, unknown;
Thus unlamented let me die;

Steal from the world, and not a stone
Tell where I lie.
#+END_VERSE

| Poem | Year |
|------+------|
| Ode on Solitude | 1700 |

#+BEGIN_SRC rust
fn quiet() {}
#+END_SRC

See [[https://example.org/pope][the archive]] and =verbatim=
with ~code~ spans.
";

#[test]
fn org_endo_produces_canonical_atd() {
    let doc = endo::org_to_document(SAMPLE_ORG).unwrap();
    assert_eq!(doc.dialect_id, "at-org");
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@#Notes on Solitude#@"));
    assert!(atd.contains("@#Retirement#@"));
    assert!(atd.contains("@##Sources##@"));
    assert!(atd.contains("@/rural quiet/@ and @*self-sufficiency*@"));
    assert!(atd.contains("@_measured_@ days and @+no crowds+@"));
    assert!(atd.contains("quoting @^(1) the ancients"));
    assert!(atd.contains("@^\nHorace above all.\n^@(1)"));
    assert!(atd.contains("@:: Horace\nthe Epodes\n::@"));
    assert!(atd.contains("@-\nreading list\n-@"));
    assert!(atd.contains("@.(1)\nthe Odes\n.@"));
    assert!(atd.contains("@>\nHappy the man"));
    assert!(atd.contains("@~\nThus let me live, unseen, unknown;\n"));
    assert!(atd.contains("Tell where I lie.\n~@"));
    assert!(atd.contains("@|\n| Poem | Year |\n|------+------|\n| Ode on Solitude | 1700 |\n|@"));
    assert!(atd.contains("\"@@@.rust") || atd.contains("@@@\"\nfn quiet() {}\n\"@@@.rust"));
    assert!(atd.contains("the archive (@><https://example.org/pope><@)"));
    assert!(atd.contains("@@\"verbatim\"@@"));
    assert!(!atd.contains("skipped metadata"));
}

/// Org -> at-org -> kanon -> org is a fixed point from the
/// first canonical output onward.
#[test]
fn org_roundtrip_is_idempotent() {
    let tmp = tmp_dir("org-roundtrip");
    let cycle = |org: &str| -> String {
        let doc = endo::org_to_document(org).unwrap();
        std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
        let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
        let x = exo::resolve_exo(&tmp, "at-org", "org").unwrap();
        exo::render(&kanon.document, &x, &tmp).unwrap()
    };
    let org1 = cycle(SAMPLE_ORG);
    let org2 = cycle(&org1);
    assert_eq!(org1, org2);
    assert!(org1.contains("[fn:o1]"));
    assert!(org1.contains("#+BEGIN_VERSE"));
}

/// at-org reaches the whole flat family transitively, and the
/// lossy return drops what org cannot say.
#[test]
fn org_routes_through_the_std_graph() {
    let tmp = tmp_dir("org-routes");
    let doc =
        endo::org_to_document("* Title\n\nWith /em/, *bold*, _under_, and +gone+.\n\n- point\n")
            .unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();

    let route = morph::resolve_route(&tmp, "at-org", "at-markdown").unwrap();
    let hops: Vec<(&str, &str)> = route
        .iter()
        .map(|m| (m.source.as_str(), m.target.as_str()))
        .collect();
    assert_eq!(hops, [("at-org", "at-html"), ("at-html", "at-markdown")]);
    let out = morph::apply_route(&kanon.document, &route).unwrap();
    let md_kanon = dendron::serialize(&out);
    assert!(md_kanon.contains("@*em*@, @**bold**@"));
    // Underline and strike survived to at-html as spans, then
    // dissolved into markdown.
    assert!(md_kanon.contains("under, and gone"));

    // Reverse: markdown reaches org.
    let back = morph::resolve_route(&tmp, "at-markdown", "at-org").unwrap();
    let again = morph::apply_route(&out, &back).unwrap();
    let x = exo::resolve_exo(&tmp, "at-org", "org").unwrap();
    let org = exo::render(&again, &x, &tmp).unwrap();
    assert!(org.contains("* Title"));
    assert!(org.contains("/em/, *bold*"));
}
