//! Embedded blocks targeted by dialektos name: the
//! `*englossis <dialektos>` pattern chooses the rule, and the
//! dialektos slot `@(!<dialektos>)` chooses the place.

use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::{exo, kanonizo};

/// A host dialektos and two micro-dialektoi to embed, with their
/// own exos for the `txt` target.
fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, content) in [
        (
            "opus.lektos",
            "@@@!atrep\n\n@=== title\n@= grammata =@\n===@\n",
        ),
        (
            "impressum.lektos",
            "@@@!atrep\n\n@=== publisher\n@: grammata :@\n===@\n",
        ),
        (
            "fontes.lektos",
            "@@@!atrep\n\n@=== source\n@& grammata &@\n===@\n",
        ),
        (
            "impressum.txt.exo",
            "@@@!atrep-exo\n@=impressum=>txt\n\n\
             @-> *paragraph\n@(grammata)\n>-@\n\n\
             @-> :\npublisher=@(grammata)\n>-@\n",
        ),
        (
            "fontes.txt.exo",
            "@@@!atrep-exo\n@=fontes=>txt\n\n\
             @-> *paragraph\n@(grammata)\n>-@\n\n\
             @-> &\nsource=@(grammata)\n>-@\n",
        ),
    ] {
        std::fs::write(dir.join(file), content).unwrap();
    }
    dir
}

/// The rules every host exo here shares.
const HOST: &str = "@@@!atrep-exo\n@=opus=>txt\n\n\
    @-> *paragraph\nP(@(grammata))\n>-@\n\n";

fn render(dir: &Path, exo_rules: &str, doc: &str) -> atrep::Result<String> {
    std::fs::write(dir.join("opus.txt.exo"), format!("{HOST}{exo_rules}")).unwrap();
    std::fs::write(dir.join("doc.atd"), doc).unwrap();
    let kanon = kanonizo::kanonizo_file(&dir.join("doc.atd"))?;
    let x = exo::resolve_exo(dir, "opus", "txt")?;
    exo::render(&kanon.document, &x, dir)
}

const DOC: &str = "@@@!opus\n\n\
    @=The Histories=@\n\n\
    These are the researches of Herodotus.\n\n\
    @@@!(impressum)\n@:Clarendon Press:@\n!@@@\n\n\
    He was of Halicarnassus.\n\n\
    @@@!(fontes)\n@&The Rawlinson translation&@\n!@@@\n";

/// A rule that names the embedded dialektos is that dialektos's;
/// the bare rule takes every other; genos variants attach to the
/// named pattern as to any.
#[test]
fn rule_chosen_by_dialektos_name() {
    let dir = tmp_dir("exo-englossis-rule");
    let rules = "@-> *document\n@(grammata)\n>-@\n\n\
        @-> =\nT(@(grammata))\n>-@\n\n\
        @-> *englossis fontes\nSOURCES{@(grammata)}\n>-@\n\n\
        @-> *englossis fontes.draft\nDRAFT{@(grammata)}\n>-@\n\n\
        @-> *englossis\nOTHER[@(dialect)]{@(grammata)}\n>-@\n";
    let out = render(&dir, rules, DOC).unwrap();
    assert_eq!(
        out,
        "P(T(The Histories))\n\
         P(These are the researches of Herodotus.)\n\
         OTHER[impressum]{publisher=Clarendon Press}\n\
         P(He was of Halicarnassus.)\n\
         SOURCES{source=The Rawlinson translation}"
    );

    let draft = DOC.replace("&@\n!@@@\n", "&@\n!@@@.draft\n");
    let out = render(&dir, rules, &draft).unwrap();
    assert!(
        out.ends_with("DRAFT{source=The Rawlinson translation}"),
        "{out}"
    );
}

/// With no bare rule, a block of a dialektos no rule names is
/// refused, and the refusal names the dialektos.
#[test]
fn unnamed_dialektos_without_a_bare_rule_is_refused() {
    let dir = tmp_dir("exo-englossis-refused");
    let rules = "@-> *document\n@(grammata)\n>-@\n\n\
        @-> =\nT(@(grammata))\n>-@\n\n\
        @-> *englossis fontes\nSOURCES{@(grammata)}\n>-@\n";
    let err = render(&dir, rules, DOC).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::ExoUnhandled(_)), "{err}");
    assert!(err.to_string().contains("`*englossis impressum`"), "{err}");
}

/// A dialektos slot places the block: it renders as it would
/// where it stands, appears where the slot is, and leaves nothing
/// (not a line) in the flow.
#[test]
fn slot_places_the_block() {
    let dir = tmp_dir("exo-englossis-slot");
    let rules = "@-> *document\n\
        HEAD@[ imprint: @(!impressum)]@\n@(grammata)\nEND\n>-@\n\n\
        @-> =\nT(@(grammata))\n>-@\n\n\
        @-> *englossis impressum\n<@(grammata)>\n>-@\n\n\
        @-> *englossis\nOTHER[@(dialect)]{@(grammata)}\n>-@\n";
    let out = render(&dir, rules, DOC).unwrap();
    assert_eq!(
        out,
        "HEAD imprint: <publisher=Clarendon Press>\n\
         P(T(The Histories))\n\
         P(These are the researches of Herodotus.)\n\
         P(He was of Halicarnassus.)\n\
         OTHER[fontes]{source=The Rawlinson translation}\n\
         END"
    );
}

/// No block of the dialektos: the slot is empty and its
/// conditional section goes. Several: joined in document order,
/// as block renderings are.
#[test]
fn slot_absent_and_repeated() {
    let dir = tmp_dir("exo-englossis-count");
    let rules = "@-> *document\n\
        HEAD@[ imprint: @(!impressum)]@\n@(grammata)\n>-@\n\n\
        @-> =\nT(@(grammata))\n>-@\n\n\
        @-> *englossis\n<@(grammata)>\n>-@\n";
    let none = "@@@!opus\n\n@=The Histories=@\n";
    assert_eq!(
        render(&dir, rules, none).unwrap(),
        "HEAD\nP(T(The Histories))"
    );

    let two = "@@@!opus\n\n@=The Histories=@\n\n\
        @@@!(impressum)\n@:Clarendon Press:@\n!@@@\n\n\
        Book one.\n\n\
        @@@!(impressum)\n@:John Murray:@\n!@@@\n";
    assert_eq!(
        render(&dir, rules, two).unwrap(),
        "HEAD imprint: <publisher=Clarendon Press>\n<publisher=John Murray>\n\
         P(T(The Histories))\nP(Book one.)"
    );
}

/// The slot may stand in any template, one filled before the
/// block is reached included; as a gate it tests for the block
/// and prints nothing, and the block stays placed.
#[test]
fn slot_in_any_template_and_as_a_gate() {
    let dir = tmp_dir("exo-englossis-any");
    let rules = "@-> *document\n@(grammata)\n>-@\n\n\
        @-> =\nT(@(grammata))@[ by @(!impressum)]@\n>-@\n\n\
        @-> *englossis\n<@(grammata)>\n>-@\n";
    let out = render(&dir, rules, DOC).unwrap();
    assert!(
        out.starts_with("P(T(The Histories) by <publisher=Clarendon Press>)\n"),
        "{out}"
    );
    assert_eq!(out.matches("Clarendon Press").count(), 1, "{out}");

    let rules = "@-> *document\n@[imprinted@(?!impressum)\n]@@(grammata)\n>-@\n\n\
        @-> =\nT(@(grammata))\n>-@\n\n\
        @-> *englossis\n<@(grammata)>\n>-@\n";
    let out = render(&dir, rules, DOC).unwrap();
    assert!(out.starts_with("imprinted\nP(T(The Histories))"), "{out}");
    assert!(!out.contains("Clarendon Press"), "{out}");
}

/// What the slot and the pattern do not take.
#[test]
fn malformed_forms_are_errors() {
    let dir = tmp_dir("exo-englossis-errors");
    for rules in [
        // the raw form: the block is rendered already
        "@-> *document\n@((!impressum))\n>-@\n",
        // no dialektos named
        "@-> *document\n@(!)\n>-@\n",
        // not a dialektos identifier
        "@-> *document\n@(grammata)\n>-@\n\n@-> *englossis fontes-\nX\n>-@\n",
    ] {
        let err = render(&dir, rules, DOC).unwrap_err();
        assert!(
            matches!(err.kind, ErrorKind::InvalidExo(_)),
            "{rules}: {err}"
        );
    }
}
