//! Metagraphe (spec v0.13 draft, "The Two Spellings"): the
//! plerographic spelling writes sim names in braces; both
//! spellings parse to the identical dendron, kanon, and litos.

use std::path::{Path, PathBuf};

use atrep::{dendron, dialektos, kanonizo, litosis, parser};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const DIA: &str = "@@@!atrep\n\n\
    @=== section\n@= [lemma]\ngrammata\n=@\n===@\n\n\
    @=== emphasis\n@/ grammata /@\n===@\n\n\
    @=== verse\n@|(n)\n===@\n";

const DOC: &str = "@@@!demo\n\n\
    @= The Title\n\
    Prose with @/styled/@ text and a marker @|(3).\n\
    =@\n";

#[test]
fn metagraphe_round_trips_and_preserves_identity() {
    let tmp = tmp_dir("metagraphe-roundtrip");
    std::fs::write(tmp.join("demo.lektos"), DIA).unwrap();
    std::fs::write(tmp.join("doc.atd"), DOC).unwrap();

    let doc = parser::parse_document(DOC, &tmp.join("doc.atd")).unwrap();
    let dial = dialektos::resolve(&tmp, "demo").unwrap();

    // Plerographo: names in braces, core forms untouched.
    let plero = dendron::serialize_plerographic(&doc, &dial).unwrap();
    assert!(plero.contains("@{section} The Title"), "{plero}");
    assert!(plero.contains("@{emphasis}styled{emphasis}@"), "{plero}");
    assert!(plero.contains("@{verse}(3)"), "{plero}");
    assert!(plero.contains("{section}@"), "{plero}");

    // The plerographic spelling parses to the identical dendron.
    std::fs::write(tmp.join("plero.atd"), &plero).unwrap();
    let re = parser::parse_document(&plero, &tmp.join("plero.atd")).unwrap();
    assert_eq!(dendron::serialize(&re), dendron::serialize(&doc));

    // Brachygrapho (plain serialize) closes the loop.
    assert_eq!(dendron::serialize(&re), DOC);

    // Identical kanon and litos ID from either spelling.
    let k1 = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let k2 = kanonizo::kanonizo_file(&tmp.join("plero.atd")).unwrap();
    assert_eq!(k1.kanon, k2.kanon);
    let lookup = litosis::media_from_dir(&tmp);
    let l1 = litosis::litosis(&k1.document, &lookup).unwrap();
    let l2 = litosis::litosis(&k2.document, &lookup).unwrap();
    assert_eq!(l1.litos_id, l2.litos_id);
}

#[test]
fn spellings_mix_freely() {
    let tmp = tmp_dir("metagraphe-mix");
    std::fs::write(tmp.join("demo.lektos"), DIA).unwrap();
    let mixed = "@@@!demo\n\n\
        @{section} The Title\n\
        Prose with @/styled/@ text and @{emphasis}named{emphasis}@ style.\n\
        {section}@\n";
    std::fs::write(tmp.join("mixed.atd"), mixed).unwrap();
    let doc = parser::parse_document(mixed, &tmp.join("mixed.atd")).unwrap();
    let s = dendron::serialize(&doc);
    assert!(s.contains("@= The Title"), "{s}");
    assert!(s.contains("@/styled/@"), "{s}");
    assert!(s.contains("@/named/@"), "{s}");
}

#[test]
fn duplicate_names_are_not_addressable() {
    let tmp = tmp_dir("metagraphe-dup");
    // Two sims sharing the name `emphasis` — legal.
    let dia = "@@@!atrep\n\n\
        @=== emphasis\n@/ grammata /@\n===@\n\n\
        @=== emphasis\n@% grammata %@\n===@\n";
    std::fs::write(tmp.join("dup.lektos"), dia).unwrap();

    // Brachygraphic use stays fully valid.
    let ok = "@@@!dup\n\nBoth @/one/@ and @%two%@.\n";
    std::fs::write(tmp.join("ok.atd"), ok).unwrap();
    let doc = parser::parse_document(ok, &tmp.join("ok.atd")).unwrap();

    // A braced reference to the duplicated name errors.
    let bad = "@@@!dup\n\nA @{emphasis}styled{emphasis}@ word.\n";
    std::fs::write(tmp.join("bad.atd"), bad).unwrap();
    let err = parser::parse_document(bad, &tmp.join("bad.atd")).unwrap_err();
    assert!(format!("{err}").contains("duplicated"), "{err}");

    // Plerographo of a document using the duplicated name errors.
    let dial = dialektos::resolve(&tmp, "dup").unwrap();
    let err = dendron::serialize_plerographic(&doc, &dial).unwrap_err();
    assert!(format!("{err}").contains("addressable"), "{err}");
}

#[test]
fn braces_are_reserved_in_symbols() {
    let tmp = tmp_dir("metagraphe-braces");
    let dia = "@@@!atrep\n\n@=== curly\n@{ grammata }@\n===@\n";
    std::fs::write(tmp.join("curly.lektos"), dia).unwrap();
    let err = dialektos::resolve(&tmp, "curly").unwrap_err();
    assert!(format!("{err}").contains("reserved"), "{err}");
}

#[test]
fn unknown_braced_name_errors() {
    let tmp = tmp_dir("metagraphe-unknown");
    std::fs::write(tmp.join("demo.lektos"), DIA).unwrap();
    let bad = "@@@!demo\n\nA @{nonexistent}x{nonexistent}@ word.\n";
    std::fs::write(tmp.join("bad.atd"), bad).unwrap();
    let err = parser::parse_document(bad, &tmp.join("bad.atd")).unwrap_err();
    assert!(format!("{err}").contains("nonexistent"), "{err}");
}

const FR_GLOSSA: &str = "@@@!atrep-glossa\n@=demo=>fr\n\n\
    @= rubrique\n\
    @/ accentuation\n";

#[test]
fn glossae_localize_the_plerographic_spelling() {
    let tmp = tmp_dir("metagraphe-glossa");
    std::fs::write(tmp.join("demo.lektos"), DIA).unwrap();
    std::fs::write(tmp.join("demo.fr.glossa"), FR_GLOSSA).unwrap();
    std::fs::write(tmp.join("doc.atd"), DOC).unwrap();

    let doc = parser::parse_document(DOC, &tmp.join("doc.atd")).unwrap();
    let dial = dialektos::resolve(&tmp, "demo").unwrap();
    assert!(dial.glossae.contains_key("fr"));

    // French plerographo: glossa names, with fallback to the
    // primary name where the glossa is silent (verse).
    let fr = dendron::serialize_plerographic_in(&doc, &dial, Some("fr")).unwrap();
    assert!(fr.contains("@{rubrique} The Title"), "{fr}");
    assert!(fr.contains("@{accentuation}styled{accentuation}@"), "{fr}");
    assert!(fr.contains("@{verse}(3)"), "{fr}");

    // Localized names join the plerographic address space: the
    // French document parses, to the identical dendron.
    std::fs::write(tmp.join("fr.atd"), &fr).unwrap();
    let re = parser::parse_document(&fr, &tmp.join("fr.atd")).unwrap();
    assert_eq!(dendron::serialize(&re), dendron::serialize(&doc));

    // Identical kanon from either spelling.
    let k1 = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let k2 = kanonizo::kanonizo_file(&tmp.join("fr.atd")).unwrap();
    assert_eq!(k1.kanon, k2.kanon);

    // An unknown language errors.
    let err = dendron::serialize_plerographic_in(&doc, &dial, Some("de")).unwrap_err();
    assert!(format!("{err}").contains("de.glossa"), "{err}");
}

#[test]
fn glossa_validation_errors() {
    let tmp = tmp_dir("metagraphe-glossa-bad");
    std::fs::write(tmp.join("demo.lektos"), DIA).unwrap();

    // Undefined symbol.
    std::fs::write(
        tmp.join("demo.fr.glossa"),
        "@@@!atrep-glossa\n@=demo=>fr\n\n@?? mystere\n",
    )
    .unwrap();
    let err = dialektos::resolve(&tmp, "demo").unwrap_err();
    assert!(format!("{err}").contains("not defined"), "{err}");

    // Two symbols sharing a localized name.
    std::fs::write(
        tmp.join("demo.fr.glossa"),
        "@@@!atrep-glossa\n@=demo=>fr\n\n@= pareil\n@/ pareil\n",
    )
    .unwrap();
    let err = dialektos::resolve(&tmp, "demo").unwrap_err();
    assert!(format!("{err}").contains("shared"), "{err}");
}

#[test]
fn cross_language_name_collisions_are_ambiguous() {
    let tmp = tmp_dir("metagraphe-glossa-ambig");
    std::fs::write(tmp.join("demo.lektos"), DIA).unwrap();
    // French names `emphasis` (the primary name of another sim!)
    // for the section sim: `@{emphasis}` is now ambiguous.
    std::fs::write(
        tmp.join("demo.fr.glossa"),
        "@@@!atrep-glossa\n@=demo=>fr\n\n@= emphasis\n",
    )
    .unwrap();
    let bad = "@@@!demo\n\nA @{emphasis}styled{emphasis}@ word.\n";
    std::fs::write(tmp.join("bad.atd"), bad).unwrap();
    let err = parser::parse_document(bad, &tmp.join("bad.atd")).unwrap_err();
    assert!(format!("{err}").contains("duplicated"), "{err}");
}

#[test]
fn sim_names_follow_the_name_grammar() {
    let tmp = tmp_dir("metagraphe-name-grammar");
    // Spaces, punctuation, boundary hyphens: all rejected.
    for bad in ["my name", "name!", "-name", "name-"] {
        let dia = format!("@@@!atrep\n\n@=== {bad}\n@/ grammata /@\n===@\n");
        std::fs::write(tmp.join("bad.lektos"), dia).unwrap();
        let err = dialektos::resolve(&tmp, "bad").unwrap_err();
        assert!(format!("{err}").contains("alphanumeric"), "`{bad}`: {err}");
    }
    // Any script is first-class.
    let dia = "@@@!atrep\n\n@=== ἔμφασις-1\n@/ grammata /@\n===@\n";
    std::fs::write(tmp.join("ok.lektos"), dia).unwrap();
    assert!(dialektos::resolve(&tmp, "ok").is_ok());

    // Glossa names obey the same grammar.
    std::fs::write(
        tmp.join("ok.fr.glossa"),
        "@@@!atrep-glossa\n@=ok=>fr\n\n@/ note de bas\n",
    )
    .unwrap();
    let err = dialektos::resolve(&tmp, "ok").unwrap_err();
    assert!(format!("{err}").contains("alphanumeric"), "{err}");
}
