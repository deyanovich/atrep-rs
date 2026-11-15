//! Litogramma out to TEI and back: the export through at-tei
//! (`litogramma.at-tei.hom`, `at-tei.tei.exo`) writes a complete
//! TEI document whether or not the source opens with a title, and
//! the TEI importer reads it back as the document that went out.
//!
//! The importer ranks a division by its nesting and leaves the
//! numbering to kanonizo, so a round trip keeps the tree of
//! divisions and their heads but not the rung each one stood on
//! (`#` comes back as `===`) nor an explicit taxis. The comparison
//! is made in at-tei, where every division is the one sim, with
//! rank and taxis levelled.

use std::path::{Path, PathBuf};

use atrep::dendron::{self, Block, Document};
use atrep::{endo, exo, kanonizo, morph};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A novel as the corpus has them: no title sim, the book a
/// level-one section, its chapters below.
const OUTLINE: &str = "\
@@@!litogramma

@#() The Iliad

@##() Book I
Sing, goddess, the anger of Achilles son of Peleus.

Many a brave soul did it send hurrying down to Hades.
##@

@##() Book II
Now the other gods and the armed warriors on the plain slept
soundly.
##@

#@
";

/// An act and a scene, a block stage direction, and inline ones:
/// opening a speech, after a dash with no space before it, and
/// before a full stop with no space after it. The padding inside
/// the delimiters is source formatting and no part of the text.
const PLAY: &str = "\
@@@!litogramma

@#() Hamlet, Prince of Denmark

@:= ACT I

@:# SCENE II. Elsinore. A room of state in the Castle.

@:[
Enter the King, the Queen, and Hamlet.
]:@

@: KING
But now, my cousin Hamlet, and my son—
:@

@: HAMLET
@:( Aside. ):@ A little more than kin, and less than kind.
:@

@: POLONIUS
We will bestow ourselves.—@:( To Ophelia. ):@ Read on this book.
I hear him coming @:( listening ):@.
:@

#:@

=:@

#@
";

/// A paper: a title sim, bare citations, one with a locator, and
/// the bibliography they point into.
const PAPER: &str = "\
@@@!litogramma

@=The wrath and the return=@

The Iliad opens on a quarrel @>[(homer-iliad); the Odyssey on an
absence @>[(homer-odyssey). Herodotus
@@.@>[(herodotus)bk. 2.@@ takes the long way round.

@@@!(bibliogramma)

@& homer-iliad
@: author
Homer
:@
@: title
The Iliad
:@
&@.book

@& homer-odyssey
@: author
Homer
:@
@: title
The Odyssey
:@
&@.book

@& herodotus
@: author
Herodotus
:@
@: title
The Histories
:@
&@.book

!@@@
";

fn kanon_of(tmp: &Path, name: &str, source: &str) -> Document {
    let path = tmp.join(name);
    std::fs::write(&path, source).unwrap();
    kanonizo::kanonizo_file(&path).unwrap().document
}

/// What `atrep exo FILE.atd tei` does: resolve the export, apply
/// its route, render.
fn export(tmp: &Path, doc: &Document) -> String {
    let (route, x) = exo::resolve_export(tmp, &doc.dialect_id, "tei", None).unwrap();
    let mapped = morph::apply_route(doc, &route).unwrap();
    exo::render(&mapped, &x, tmp).unwrap()
}

/// The document mapped into at-tei, division rank and taxis
/// levelled, in canonical form.
fn in_at_tei(tmp: &Path, doc: &Document) -> String {
    fn level(blocks: &mut [Block]) {
        for block in blocks {
            if let Block::Para {
                symbol,
                taxis,
                children,
                ann,
                ..
            } = block
            {
                if symbol == "#" {
                    *taxis = None;
                    ann.genoses.clear();
                }
                level(children);
            }
        }
    }
    let route = morph::resolve_route(tmp, "litogramma", "at-tei").unwrap();
    let mut mapped = morph::apply_route(doc, &route).unwrap();
    level(&mut mapped.blocks);
    dendron::serialize(&mapped)
}

/// Export, import, and hold the two ends against each other.
/// Returns the TEI for the assertions particular to a test.
fn round_trip(name: &str, source: &str) -> String {
    let tmp = tmp_dir(name);
    let original = kanon_of(&tmp, "original.atd", source);
    let tei = export(&tmp, &original);
    let imported = endo::tei_to_document(&tei).unwrap();
    assert_eq!(imported.dialect_id, "litogramma");
    let back = kanon_of(&tmp, "back.atd", &dendron::serialize(&imported));
    assert_eq!(in_at_tei(&tmp, &original), in_at_tei(&tmp, &back));
    tei
}

/// A document that opens with a section and no title sim still
/// exports whole: header, text and body around the divisions, and
/// the level-one section a division with its head.
#[test]
fn titleless_outline_is_a_complete_document() {
    let tei = round_trip("tei-export-outline", OUTLINE);
    assert!(tei.starts_with(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <TEI xmlns=\"http://www.tei-c.org/ns/1.0\">\n\
         <teiHeader>\n"
    ));
    // No title sim, no title: the importer reads the empty element
    // as none, and the book's name stays the head of its division.
    assert!(tei.contains("<titleStmt>\n<title/>\n</titleStmt>"));
    assert!(tei.contains(
        "</teiHeader>\n<text>\n<body>\n\
         <div type=\"section\" n=\"1\">\n<head>The Iliad</head>\n\
         <div type=\"subsection\" n=\"1\">\n<head>Book I</head>\n"
    ));
    assert!(tei.ends_with("</div>\n</body>\n</text>\n</TEI>"));
    assert_eq!(tei.matches("<text>").count(), 1);
    assert_eq!(tei.matches("<teiHeader>").count(), 1);
}

/// The imported outline is the book and its two chapters, none of
/// it hoisted into a title.
#[test]
fn titleless_outline_comes_back_untitled() {
    let tmp = tmp_dir("tei-export-untitled");
    let original = kanon_of(&tmp, "original.atd", OUTLINE);
    let imported = endo::tei_to_document(&export(&tmp, &original)).unwrap();
    let [
        Block::Para {
            lemma, children, ..
        },
    ] = imported.blocks.as_slice()
    else {
        panic!("one division expected, found {:#?}", imported.blocks);
    };
    assert_eq!(dendron::serialize_inline_fragment(lemma), "The Iliad");
    let chapters = children
        .iter()
        .filter(|b| matches!(b, Block::Para { .. }))
        .count();
    assert_eq!(chapters, 2);
}

/// Acts, scenes, speeches and stage directions survive, and an
/// inline direction sits against its neighbours exactly as the
/// kanon has it: the padding inside its delimiters is not text.
#[test]
fn play_fragment_round_trips() {
    let tei = round_trip("tei-export-play", PLAY);
    assert!(tei.contains("<div type=\"act\">\n<head>ACT I</head>"));
    assert!(tei.contains("<stage>\n<p>Enter the King, the Queen, and Hamlet.</p>\n</stage>"));
    assert!(tei.contains("<sp>\n<speaker>HAMLET</speaker>\n<p><stage>Aside.</stage> A little"));
    assert!(tei.contains("ourselves.—<stage>To Ophelia.</stage> Read on this book."));
    assert!(tei.contains("coming <stage>listening</stage>.</p>"));
}

/// A title sim rides the header, a citation keeps its target and
/// its locator, and the bibliography comes back entry for entry.
#[test]
fn paper_with_citations_round_trips() {
    let tei = round_trip("tei-export-paper", PAPER);
    assert!(tei.contains("<titleStmt>\n<title>The wrath and the return</title>\n</titleStmt>"));
    assert!(!tei.contains("<title/>"));
    assert_eq!(tei.matches("<text>").count(), 1);
    assert_eq!(tei.matches("<teiHeader>").count(), 1);
    assert!(tei.contains("<ptr type=\"bibr\" target=\"#homer-iliad\"/>"));
    assert!(tei.contains("<ref type=\"bibr\" target=\"#herodotus\">bk. 2</ref>"));
    assert!(tei.contains("<bibl xml:id=\"homer-odyssey\" type=\"liber\">"));

    // Citations and bibliography are the same in litogramma too,
    // where no division stands between the two ends.
    let tmp = tmp_dir("tei-export-paper-kanon");
    let original = kanon_of(&tmp, "original.atd", PAPER);
    let imported = endo::tei_to_document(&export(&tmp, &original)).unwrap();
    let back = kanon_of(&tmp, "back.atd", &dendron::serialize(&imported));
    assert_eq!(dendron::serialize(&original), dendron::serialize(&back));
}

/// Litogramma has no document exomorphosis for TEI of its own, so
/// the export resolves through at-tei, and gives what the morphism
/// followed by the at-tei exomorphosis gives.
#[test]
fn export_resolves_through_at_tei() {
    let tmp = tmp_dir("tei-export-route");
    let (route, x) = exo::resolve_export(&tmp, "litogramma", "tei", None).unwrap();
    let hops: Vec<(&str, &str)> = route
        .iter()
        .map(|m| (m.source.as_str(), m.target.as_str()))
        .collect();
    assert_eq!(hops, [("litogramma", "at-tei")]);
    assert_eq!(x.source, "at-tei");

    for (name, source) in [
        ("outline.atd", OUTLINE),
        ("play.atd", PLAY),
        ("paper.atd", PAPER),
    ] {
        let doc = kanon_of(&tmp, name, source);
        let m = morph::resolve_morph(&tmp, "litogramma", "at-tei").unwrap();
        let mapped = morph::apply(&doc, &m).unwrap();
        let two_steps = exo::render(
            &mapped,
            &exo::resolve_exo(&tmp, "at-tei", "tei").unwrap(),
            &tmp,
        )
        .unwrap();
        assert_eq!(export(&tmp, &doc), two_steps);
    }
}

/// A dialektos that renders the target itself exports directly.
#[test]
fn own_document_exo_needs_no_route() {
    let tmp = tmp_dir("tei-export-direct");
    for (dialect, target) in [("litogramma", "html"), ("at-tei", "tei")] {
        let (route, x) = exo::resolve_export(&tmp, dialect, target, None).unwrap();
        assert!(route.is_empty());
        assert_eq!(x.source, dialect);
        assert!(x.renders_documents());
    }
}

/// An embedded block is a bibliography only when its dialektos
/// is bibliogramma: a block of the document's own dialektos (what
/// a transcluded file becomes) stands in the flow as it renders,
/// in TEI and in LaTeX.
#[test]
fn embedded_block_of_the_same_dialektos_is_no_bibliography() {
    let tmp = tmp_dir("tei-export-own-block");
    let doc = kanon_of(
        &tmp,
        "books.atd",
        "@@@!litogramma\n\n\
         @=The Iliad=@\n\n\
         Sing, goddess, the wrath of Achilles.\n\n\
         @@@!(litogramma)\n\
         @#(1) Book I\n\
         The wrath of Peleus' son.\n\
         #@\n\
         !@@@\n",
    );
    let tei = export(&tmp, &doc);
    assert!(tei.contains("The wrath of Peleus' son."), "{tei}");
    assert!(!tei.contains("<listBibl>"), "{tei}");

    let x = exo::resolve_exo(&tmp, "litogramma", "latex").unwrap();
    let latex = exo::render(&doc, &x, &tmp).unwrap();
    assert!(latex.contains("The wrath of Peleus' son."), "{latex}");
    assert!(!latex.contains("\\bibliography{"), "{latex}");
}

/// The routing is one rule for every format, so it is held over
/// the whole standard library: no dialektos has two nearest
/// renderers for a target, and the routes the markup formats take
/// are the ones written here.
#[test]
fn std_routes_are_never_ambiguous() {
    let tmp = tmp_dir("tei-export-std-routes");
    let targets = [
        "tei", "docbook", "html", "xhtml", "md", "org", "rst", "dj", "latex", "gemtext", "epub",
        "kindle", "usfm", "usx", "osis", "bib", "bibtex",
    ];
    let mut table: Vec<String> = Vec::new();
    for dialect in atrep::dialektos::std_ids() {
        for target in targets {
            match exo::resolve_export(&tmp, dialect, target, None) {
                Ok((route, x)) if x.renders_documents() => {
                    let hops: Vec<&str> = std::iter::once(dialect)
                        .chain(route.iter().map(|m| m.target.as_str()))
                        .collect();
                    table.push(format!("{target}: {}", hops.join(" => ")));
                }
                // No renderer in reach: the exo of the dialektos
                // itself, or none, as before there was a route.
                Ok(_) => {}
                Err(e) => assert!(
                    !matches!(e.kind, atrep::ErrorKind::AmbiguousMorphRoute(_)),
                    "{dialect} to {target}: {e}"
                ),
            }
        }
    }
    // The whole table, for a run with `--nocapture`.
    println!("{}", table.join("\n"));
    for expected in [
        "tei: litogramma => at-tei",
        "docbook: litogramma => at-docbook",
        "xhtml: litogramma => at-html",
        "md: litogramma => at-html => at-markdown",
        "org: litogramma => at-html => at-org",
        "rst: litogramma => at-html => at-rst",
        "dj: litogramma => at-html => at-markdown => at-djot",
        "html: litogramma",
        "latex: litogramma",
    ] {
        assert!(
            table.iter().any(|row| row == expected),
            "missing `{expected}` in:\n{}",
            table.join("\n")
        );
    }
    // Lexigramma has no morphism to at-tei: nothing is routed.
    assert!(!table.iter().any(|row| row.starts_with("tei: lexigramma")));
}

/// The export goes into a git tree and behind a checksum: the same
/// input gives the same bytes, with every definition resolved anew
/// (no timestamp, no generated id, no order taken from a hash map).
#[test]
fn export_is_byte_stable() {
    let tmp = tmp_dir("tei-export-stable");
    for (name, source) in [
        ("outline.atd", OUTLINE),
        ("play.atd", PLAY),
        ("paper.atd", PAPER),
    ] {
        let first = export(&tmp, &kanon_of(&tmp, name, source));
        for _ in 0..4 {
            assert_eq!(export(&tmp, &kanon_of(&tmp, name, source)), first);
        }
    }
}

/// A core milestone exports as TEI's milestone, the scheme its
/// unit and the value its n, and imports back as it went out.
#[test]
fn core_milestone_round_trips() {
    let tei = round_trip(
        "tei-export-milestone",
        "@@@!litogramma\n\n@=The Histories=@\n\n\
         These are the researches of Herodotus@(\"section:1.1\") of Halicarnassus.\n",
    );
    assert!(
        tei.contains("Herodotus<milestone unit=\"section\" n=\"1.1\"/> of"),
        "{tei}"
    );
}

/// Emphasis that names a TEI phrase element, as the importer
/// writes foreign, title and term, exports as that element and
/// imports back as it went out; plain emphasis stays emph.
#[test]
fn emphasis_naming_a_tei_element_exports_as_it() {
    let tei = round_trip(
        "tei-export-emph-genos",
        "@@@!litogramma\n\n@=The Histories=@\n\n\
         He called it @/historia/@.foreign, as in the @/Histories/@.title; \
         the @/logos/@.term is his unit, and this is @/plain/@ emphasis.\n",
    );
    assert!(
        tei.contains(
            "<foreign>historia</foreign>, as in the <title>Histories</title>; \
             the <term>logos</term> is his unit, and this is <emph>plain</emph> emphasis."
        ),
        "{tei}"
    );
}

/// A language genos beside foreign is not written back: the
/// element exports, its xml:lang does not.
#[test]
fn foreign_exports_without_its_language() {
    let tmp = tmp_dir("tei-export-foreign-lang");
    let doc = kanon_of(
        &tmp,
        "doc.atd",
        "@@@!litogramma\n\n@=The Histories=@\n\nHe called it @/historia/@.foreign.grc.\n",
    );
    let tei = export(&tmp, &doc);
    assert!(tei.contains("<foreign>historia</foreign>."), "{tei}");
}
