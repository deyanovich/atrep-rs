//! Controlled vocabularies: alias normalization in kanonizo,
//! the two-tier genos rule in litosis, and term-keyed exo
//! rules.

use std::path::{Path, PathBuf};

use atrep::{dialektos, exo, kanonizo, litosis};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const DIA: &str = "\
@@@!atrep

@=== entry
@& lemma
grammata
&@
@% genera
@\"an entry\"@
===@

@=== field
@: lemma=campi
grammata
:@
@\"a field\"@
===@

@==% genera
liber: book buch livre
%==@

@==% campi
auctor: author autor auteur
titulus: title titel titre
%==@
";

fn setup(name: &str) -> PathBuf {
    let tmp = tmp_dir(name);
    std::fs::write(tmp.join("biblio.dia"), DIA).unwrap();
    tmp
}

#[test]
fn aliases_cannot_fork_the_hash() {
    let tmp = setup("vocab-hash");
    let fr = "@@@!biblio\n\n@& k1\n@: auteur\nPope\n:@\n&@.livre\n";
    let en = "@@@!biblio\n\n@& k1\n@: Author\nPope\n:@\n&@.book\n";
    std::fs::write(tmp.join("fr.atd"), fr).unwrap();
    std::fs::write(tmp.join("en.atd"), en).unwrap();
    let fr = kanonizo::kanonizo_file(&tmp.join("fr.atd")).unwrap();
    let en = kanonizo::kanonizo_file(&tmp.join("en.atd")).unwrap();
    // Case-insensitive alias lookup, canonical Latin in the kanon.
    assert_eq!(fr.kanon, en.kanon);
    assert!(fr.kanon.contains("@: auctor"));
    assert!(fr.kanon.contains("&@.liber"));

    // The vocabulary genos is semantic: it survives litosis,
    // and both authorings share one litos id.
    let resolve = |id: &str| dialektos::resolve(&tmp, id).ok();
    let lookup = litosis::media_from_dir(&tmp);
    let lf = litosis::litosis_with(&fr.document, &lookup, &resolve).unwrap();
    let le = litosis::litosis_with(&en.document, &lookup, &resolve).unwrap();
    assert_eq!(lf.litos_id, le.litos_id);
    assert!(lf.litos.contains("&@.liber"));
}

#[test]
fn term_keyed_exo_rules() {
    let tmp = setup("vocab-exo");
    std::fs::write(
        tmp.join("biblio.out.exo"),
        "@@@!atrep-exo\n@=biblio=>out\n\n\
         @-> *document\n@(grammata)\n>-@\n\n\
         @-> *paragraph\n@(grammata)\n>-@\n\n\
         @-> &.liber\nBOOK[@(lemma)]\n@(grammata)\n>-@\n\n\
         @-> :(auctor)\nby=@(grammata)\n>-@\n\n\
         @-> :\n@(lemma)=@(grammata)\n>-@\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!biblio\n\n@& k1\n@: auteur\nPope\n:@\n@: titre\nOde\n:@\n&@.buch\n",
    )
    .unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "biblio", "out").unwrap();
    let out = exo::render(&kanon.document, &x, &tmp).unwrap();
    // The genos variant picked the entry rule; the term-keyed
    // rule translated auctor while titulus fell back.
    assert!(out.contains("BOOK[k1]"));
    assert!(out.contains("by=Pope"));
    assert!(out.contains("titulus=Ode"));
}
