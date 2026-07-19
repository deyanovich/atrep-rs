//! Dialektos-defined stichoi sims (F1): the `stichos` ostensive
//! keyword declares line-structured grammata.

use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::{exo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // A verse-bearing dialektos: `~` is a stichoi para-simmere
    // with optional lemma/taxis and hypograph; `/` is an endo.
    std::fs::write(
        dir.join("versi.lektos"),
        "@@@!atrep\n\n\
         @=== emphasis\n@/ grammata /@\n===@\n\n\
         @=== verse\n@~[(taxis)] [lemma]\nstichos\n~@ [hypograph]\n===@\n",
    )
    .unwrap();
    dir
}

const VERSE_ATD: &str = "@@@!versi\n\n\
    Before the poem.\n\n\
    @~() The Ode\n\
    Happy the man, whose @/wish/@ and care\n\
    A few paternal acres bound.\n\
    \n\
    Blest, who can unconcern'dly find\n\
    Hours, days, and years slide soft away.\n\
    ~@(ode) Pope, c. 1700\n";

#[test]
fn stichoi_sim_parses_and_kanonizes() {
    let tmp = tmp_dir("stichoi-kanon");
    std::fs::write(tmp.join("doc.atd"), VERSE_ATD).unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    // Line structure survives; taxis is evaluated; the unreferenced
    // onym is dropped.
    assert_eq!(
        result.kanon,
        "@@@!versi\n\n\
         Before the poem.\n\n\
         @~(1) The Ode\n\
         Happy the man, whose @/wish/@ and care\n\
         A few paternal acres bound.\n\
         \n\
         Blest, who can unconcern'dly find\n\
         Hours, days, and years slide soft away.\n\
         ~@ Pope, c. 1700\n"
    );
    // The kanon reparses to the same kanon (idempotency).
    std::fs::write(tmp.join("doc.atk"), &result.kanon).unwrap();
    let again = kanonizo::kanonizo_file(&tmp.join("doc.atk")).unwrap();
    assert_eq!(again.kanon, result.kanon);
}

#[test]
fn stichoi_taxis_runs_are_validated() {
    let tmp = tmp_dir("stichoi-taxis");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!versi\n\n\
         @~() One\nline\n~@\n\n\
         @~(3) Two\nline\n~@\n",
    )
    .unwrap();
    let err = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap_err();
    assert!(matches!(
        err.kind,
        ErrorKind::TaxisInconsistent {
            expected: 2,
            found: 3
        }
    ));
}

#[test]
fn stichoi_sim_renders_via_symbol_rule() {
    let tmp = tmp_dir("stichoi-exo");
    std::fs::write(tmp.join("doc.atd"), VERSE_ATD).unwrap();
    std::fs::write(
        tmp.join("versi.html.exo"),
        "@@@!atrep-exo\n@=versi=>html\n\n\
         @-> *document\n@(grammata)\n>-@\n\n\
         @-> *paragraph\n<p>@(grammata)</p>\n>-@\n\n\
         @-> /\n<em>@(grammata)</em>\n>-@\n\n\
         @-> ~\n<div class=\"verse\" data-taxis=\"@(taxis)\">\n\
         <h4>@(lemma)</h4>\n@(grammata)\n\
         <footer>@(hypograph)</footer>\n</div>\n>-@\n\n\
         @-> *strophe\n<p class=\"strophe\">\n@(grammata)\n</p>\n>-@\n\n\
         @-> *stichos\n@(grammata)<br/>\n>-@\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "versi", "html").unwrap();
    let html = exo::render(&result.document, &x, &tmp).unwrap();
    assert_eq!(
        html,
        "<p>Before the poem.</p>\n\
         <div class=\"verse\" data-taxis=\"1\">\n\
         <h4>The Ode</h4>\n\
         <p class=\"strophe\">\n\
         Happy the man, whose <em>wish</em> and care<br/>\n\
         A few paternal acres bound.<br/>\n\
         </p>\n\
         <p class=\"strophe\">\n\
         Blest, who can unconcern'dly find<br/>\n\
         Hours, days, and years slide soft away.<br/>\n\
         </p>\n\
         <footer>Pope, c. 1700</footer>\n\
         </div>"
    );
}

/// F2: leading and internal whitespace in stichos lines is
/// authorial content — preserved through kanonizo and litosis;
/// trailing whitespace is trimmed; a whitespace-only line is a
/// strophe separator.
#[test]
fn stichos_whitespace_is_preserved() {
    let tmp = tmp_dir("stichoi-whitespace");
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!versi\n\n\
         @~ Easter Wings\n\
         Lord, who createdst man in wealth and store,\n\
         \x20\x20\x20\x20Though foolishly he lost the same,\n\
         Decaying more   and more,\n\
         \x20\x20\n\
         With thee\n\
         O let me rise   \n\
         ~@\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(
        result.kanon,
        "@@@!versi\n\n\
         @~ Easter Wings\n\
         Lord, who createdst man in wealth and store,\n\
         \x20\x20\x20\x20Though foolishly he lost the same,\n\
         Decaying more   and more,\n\
         \n\
         With thee\n\
         O let me rise\n\
         ~@\n"
    );
    // Idempotent through a reparse.
    std::fs::write(tmp.join("doc.atk"), &result.kanon).unwrap();
    let again = kanonizo::kanonizo_file(&tmp.join("doc.atk")).unwrap();
    assert_eq!(again.kanon, result.kanon);
    // The indentation is content: it survives into the litos.
    let lookup = atrep::litosis::media_from_dir(&tmp);
    let litos = atrep::litosis::litosis(&result.document, &lookup).unwrap();
    assert!(litos.litos.contains("\n    Though foolishly"));
    // Re-indenting changes the litos ID.
    std::fs::write(
        tmp.join("doc2.atd"),
        std::fs::read_to_string(tmp.join("doc.atd"))
            .unwrap()
            .replace("    Though", "  Though"),
    )
    .unwrap();
    let result2 = kanonizo::kanonizo_file(&tmp.join("doc2.atd")).unwrap();
    let litos2 = atrep::litosis::litosis(&result2.document, &lookup).unwrap();
    assert_ne!(litos.litos_id, litos2.litos_id);
}

#[test]
fn definition_roundtrip_preserves_stichos_keyword() {
    let tmp = tmp_dir("stichoi-lektos");
    let result = kanonizo::kanonizo_definition_file(&tmp.join("versi.lektos")).unwrap();
    assert!(
        result
            .kanon
            .contains("@~[(taxis)] [lemma]\nstichos\n~@ [hypograph]")
    );
    // Canonical form is idempotent.
    std::fs::write(tmp.join("versi2.lektos"), &result.kanon).unwrap();
    let again = kanonizo::kanonizo_definition_file(&tmp.join("versi2.lektos")).unwrap();
    assert_eq!(again.kanon, result.kanon);
}
