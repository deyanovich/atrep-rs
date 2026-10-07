//! Regression tests for the TEI importer review findings (group
//! endo-tei): the header, the note-body plumbing, the apparatus,
//! the dictionary path and the bibliography.

use atrep::{dendron, endo};

fn wrap(body: &str) -> String {
    format!(
        "<TEI xmlns=\"http://www.tei-c.org/ns/1.0\"><teiHeader><fileDesc><titleStmt>\
         <title>Iliad</title></titleStmt></fileDesc></teiHeader>\
         <text><body>{body}</body></text></TEI>"
    )
}

fn import(xml: &str) -> String {
    dendron::serialize(&endo::tei_to_document(xml).unwrap())
}

fn check(atd: &str) {
    atrep::check_source(atd, std::path::Path::new("<memory>.atd")).unwrap();
}

/// A failed literary import leaves no bibliography keys behind
/// for the next import on the thread: a dictionary's internal
/// ref stays a ref.
#[test]
fn failed_import_does_not_poison_the_next() {
    let failing = wrap(
        "<div><listBibl><bibl xml:id=\"x\">X</bibl></listBibl></div>\
         <p>a <unknownElement/> b</p>",
    );
    assert!(endo::tei_to_document(&failing).is_err());
    let lex = "<TEI><teiHeader><fileDesc><titleStmt><title>Lex</title></titleStmt></fileDesc>\
               </teiHeader><text><body><p>see <ref target=\"#x\">x</ref></p>\
               <entry xml:id=\"e1\"><form type=\"lemma\"><orth>bank</orth></form>\
               <sense><def>land</def></sense></entry></body></text></TEI>";
    let atd = import(lex);
    assert!(atd.contains("see x@>(x)"), "{atd}");
    assert!(!atd.contains(">["), "{atd}");
}

/// A note in the header title keeps its body, and the body's
/// first note takes the next number.
#[test]
fn header_title_note_keeps_its_body() {
    let xml = "<TEI><teiHeader><fileDesc><titleStmt><title>Iliad<note>on the title</note>\
               </title></titleStmt></fileDesc></teiHeader><text><body><p>x<note>body note\
               </note></p></body></text></TEI>";
    let atd = import(xml);
    assert!(
        atd.contains("@=Iliad@^(n1)=@\n\n@^\non the title\n^@(n1)"),
        "{atd}"
    );
    assert!(atd.contains("x@^(n2)\n\n@^\nbody note\n^@(n2)"), "{atd}");
    check(&atd);
}

/// A type="main" title in the skipped sourceDesc does not drop
/// the titleStmt title.
#[test]
fn main_title_in_source_desc_is_not_the_edition_title() {
    let xml = "<TEI><teiHeader><fileDesc><titleStmt><title>Odyssey</title><author>Homer\
               </author></titleStmt><sourceDesc><biblStruct><monogr><title type=\"main\">\
               Odyssey</title></monogr></biblStruct></sourceDesc></fileDesc></teiHeader>\
               <text><body><p>x</p></body></text></TEI>";
    let atd = import(xml);
    assert!(atd.contains("@=Odyssey=@"), "{atd}");
    assert!(atd.contains("@=:Homer:=@"), "{atd}");
}

/// A self-closing header title is an empty title.
#[test]
fn self_closing_header_title() {
    let xml = "<TEI><teiHeader><fileDesc><titleStmt><title/><author>Homer</author>\
               </titleStmt></fileDesc></teiHeader><text><body><p>x</p></body></text></TEI>";
    let atd = import(xml);
    assert!(atd.contains("@=:Homer:=@"), "{atd}");
}

/// A byte-order mark leads a file without complaint, on both the
/// literary and the dictionary path.
#[test]
fn bom_is_whitespace() {
    let xml = format!("\u{feff}{}", wrap("<p>x</p>"));
    assert!(import(&xml).contains("\nx\n"));
    let lex = "\u{feff}<TEI><teiHeader><fileDesc><titleStmt><title>Lex</title></titleStmt>\
               </fileDesc></teiHeader><text><body><entry xml:id=\"e1\"><form type=\"lemma\">\
               <orth>bank</orth></form><sense><def>land</def></sense></entry></body></text>\
               </TEI>";
    let atd = import(lex);
    assert!(!atd.contains('\u{feff}'), "{atd}");
}

/// A truncated file is an error, never a panic: an <sp> or a
/// <row> at the end of input.
#[test]
fn truncated_speech_and_row_are_errors() {
    for body in ["<sp>", "<sp>\n", "<table><row>", "<sp><speaker>A</speaker>"] {
        let xml = format!(
            "<TEI><teiHeader><fileDesc><titleStmt><title>Iliad</title></titleStmt>\
             </fileDesc></teiHeader><text><body>{body}"
        );
        let err = endo::tei_to_document(&xml).unwrap_err().to_string();
        assert!(err.contains("unterminated"), "{body}: {err}");
    }
}

/// A self-closing <said/> opens no speech paragraph, and a stray
/// </said> after it does not underflow the depth.
#[test]
fn self_closing_said_in_a_paragraph() {
    let atd = import(&wrap("<p><said/>Hello <said>there</said></p><p>next</p>"));
    assert!(atd.contains("Hello @\"\"there\"\"@.said\n\nnext"), "{atd}");
    let err = endo::tei_to_document(&wrap("<p><said/></said></p>")).unwrap_err();
    assert!(err.to_string().contains("unsupported"), "{err}");
}

/// A parallel-segmentation apparatus keeps its lem reading in
/// the text — in prose, in a verse line, in a verse quote and at
/// block level — and drops the variants.
#[test]
fn apparatus_keeps_the_lem_reading() {
    let atd = import(&wrap(
        "<p>Sing, <app><lem>goddess</lem><rdg wit=\"#A\">muse</rdg></app>, the wrath</p>",
    ));
    assert!(atd.contains("\nSing, goddess, the wrath\n"), "{atd}");
    assert!(!atd.contains("muse"), "{atd}");
    let atd = import(&wrap(
        "<lg><l>Sing, <app><rdgGrp><lem>goddess</lem><rdg>muse</rdg></rdgGrp>\
         <note>a witness note</note></app></l></lg>",
    ));
    assert!(atd.contains("@~\nSing, goddess\n~@"), "{atd}");
    let atd = import(&wrap(
        "<p>a <quote type=\"verse\"><l>x</l><app><lem>y</lem><rdg>z</rdg></app></quote></p>",
    ));
    assert!(atd.contains("a @\"\"x y\"\"@"), "{atd}");
    let atd = import(&wrap(
        "<app><lem><p>The first paragraph.</p></lem><rdg><p>Another.</p></rdg></app>\
         <app><lem>a loose reading</lem></app><p>after</p>",
    ));
    assert!(
        atd.contains("\nThe first paragraph.\n\na loose reading\n\nafter\n"),
        "{atd}"
    );
    assert!(!atd.contains("Another"), "{atd}");
    check(&atd);
}

/// A Lex-0 related entry (re) closes at its own end: the next
/// top-level entry is its own entry.
#[test]
fn lex0_related_entry_closes_at_re() {
    let lex = "<TEI><teiHeader><fileDesc><titleStmt><title>Lex</title></titleStmt></fileDesc>\
               </teiHeader><text><body><entry xml:id=\"e1\"><form type=\"lemma\"><orth>bank\
               </orth></form><sense n=\"1\"><def>land</def></sense><re xml:id=\"e1r\"><form \
               type=\"lemma\"><orth>bank holiday</orth></form><sense><def>a holiday</def>\
               </sense></re></entry><entry xml:id=\"e2\"><form type=\"lemma\"><orth>river\
               </orth></form><sense><def>a stream</def></sense></entry></body></text></TEI>";
    let atd = import(lex);
    assert!(atd.contains("@! river\n@:(1)\na stream\n:@\n!@"), "{atd}");
    assert!(!atd.contains("=~river~=@"), "{atd}");
    assert!(
        atd.contains("@! bank holiday\n@:(1)\na holiday\n:@\n!@\n!@"),
        "{atd}"
    );
}

/// Every inline run that can spawn a note keeps the body: a
/// stage direction in a verse speech, a said label, a figure or
/// table head, a cell, a list head, a cast item, a persName, a
/// cit or epigraph bibl, a front-matter title. Each import is a
/// valid document (kanonizo finds every deixis declared).
#[test]
fn note_bodies_land_everywhere() {
    let cases: [(&str, &str); 10] = [
        (
            "<sp><speaker>Achilles</speaker><l>Sing</l><stage>enters<note>a note</note>\
             </stage><l>goddess</l></sp>",
            "@:(enters@^(n1)):@",
        ),
        (
            "<p><said who=\"#a\"><label>Achilles.<note>a note</note></label> speech</said></p>",
            "Achilles@^(n1)",
        ),
        (
            "<figure><head>A map<note>a note</note></head><graphic url=\"map.png\"/></figure>",
            "A map@^(n1)",
        ),
        (
            "<table><head>Ships<note>a note</note></head><row><cell>a</cell></row></table>",
            "Ships@^(n1)",
        ),
        (
            "<table><row><cell>a<note>n</note></cell></row></table>",
            "| a@^(n1) |",
        ),
        (
            "<list><head>Heroes<note>a note</note></head><item>Achilles</item></list>",
            "Heroes@^(n1)",
        ),
        (
            "<castList><castItem>Achilles<note>a note</note></castItem></castList>",
            "Achilles@^(n1)",
        ),
        (
            "<listPerson><person xml:id=\"ach\"><persName>Achilles<note>a note</note>\
             </persName></person></listPerson>",
            "Achilles@^(n1)",
        ),
        (
            "<cit><quote>Beatus ille.</quote><bibl>Horace<note>a note</note></bibl></cit>",
            "Horace@^(n1)",
        ),
        (
            "<epigraph><quote>Beatus ille.</quote><bibl>Horace<note>a note</note></bibl>\
             </epigraph>",
            "Horace@^(n1)",
        ),
    ];
    for (body, callout) in cases {
        let atd = import(&wrap(body));
        assert!(atd.contains(callout), "{body}\n{atd}");
        assert!(atd.contains("^@(n1)"), "{body}\n{atd}");
        atrep::check_source(&atd, std::path::Path::new("<memory>.atd"))
            .unwrap_or_else(|e| panic!("{body}\n{atd}\n{e}"));
    }
    let xml = "<TEI><teiHeader><fileDesc><titleStmt><title>Iliad</title></titleStmt>\
               </fileDesc></teiHeader><text><front><docTitle><titlePart>The Iliad\
               <note>a note</note></titlePart></docTitle></front><body><p>x</p>\
               </body></text></TEI>";
    let atd = import(xml);
    assert!(atd.contains("@=The Iliad@^(n1)=@"), "{atd}");
    assert!(atd.contains("^@(n1)"), "{atd}");
    check(&atd);
}

/// A table cell keeps its wrapped content: a phrase, a name.
#[test]
fn table_cells_keep_wrapped_content() {
    let atd = import(&wrap(
        "<table><row><cell><hi rend=\"italic\">Iliad</hi></cell><cell>Homer \
         <persName ref=\"#h\">Homer</persName></cell></row></table>",
    ));
    assert!(atd.contains("| @/Iliad/@ | Homer @,"), "{atd}");
    assert!(atd.contains("Homer,@.persname |"), "{atd}");
    check(&atd);
}

/// A self-closing <ptr/> paired with the note that follows it
/// (the shape the at-tei exo writes) is the callout duplicate,
/// as a paired <ref> is: only the deixis remains.
#[test]
fn self_closing_ptr_before_its_note_is_dropped() {
    for callout in ["<ptr target=\"#o1\"/>", "<ref target=\"#o1\">1</ref>"] {
        let atd = import(&wrap(&format!(
            "<p>Pope{callout}<note xml:id=\"o1\" place=\"foot\">Early.</note> wrote.</p>"
        )));
        assert!(atd.contains("Pope@^(n1) wrote."), "{atd}");
        assert!(!atd.contains("@>(o1)"), "{atd}");
        check(&atd);
    }
}

/// A target that lists several pointers resolves each one: two
/// bibliography entries are two cites, two plain ids two refs.
#[test]
fn ref_with_several_targets() {
    let atd = import(&wrap(
        "<p>See <ref target=\"#gibbon1776 #herodotus\">both</ref> and \
         <ref target=\"#a #b\">these</ref>.</p>\
         <div><listBibl><bibl xml:id=\"gibbon1776\"><author>Gibbon, Edward</author>\
         <title>The Decline and Fall</title></bibl><bibl xml:id=\"herodotus\">\
         <author>Herodotus</author><title>The Histories</title></bibl></listBibl></div>",
    ));
    assert!(
        atd.contains("@@.@>[(gibbon1776)@>[(herodotus)both.@@"),
        "{atd}"
    );
    assert!(atd.contains("these@>(a)@>(b)."), "{atd}");
    assert!(!atd.contains("\\ #"), "{atd}");
}

/// The scheme a milestone takes from resp or ed is a valid
/// scheme name: a pointer's hash and an abbreviation's period go.
#[test]
fn milestone_scheme_from_resp_is_sanitized() {
    let atd = import(&wrap(
        "<p>a <milestone resp=\"#Bekker\" unit=\"page\" n=\"1094a\"/> b \
         <milestone unit=\"section\" resp=\"St.\" n=\"17a\"/> c</p>",
    ));
    assert!(atd.contains("@(\"bekker:1094a\").page"), "{atd}");
    assert!(atd.contains("@(\"st:17a\").section"), "{atd}");
    check(&atd);
}

/// An lg tolerates what a verse speech does between its lines: a
/// page break, a line break, a milestone (a line of its own), a
/// gap, a note (its callout rides the previous line).
#[test]
fn lg_tolerates_furniture_between_lines() {
    let atd = import(&wrap(
        "<lg><l>Sing, goddess</l><pb n=\"3\"/><lb/><l>the wrath</l></lg>",
    ));
    assert!(atd.contains("@~\nSing, goddess\nthe wrath\n~@"), "{atd}");
    let atd = import(&wrap(
        "<lg><l>Sing, goddess</l><milestone unit=\"card\" n=\"5\"/><l>the wrath</l></lg>",
    ));
    assert!(
        atd.contains("Sing, goddess\n@(\"perseus:card:5\").card\nthe wrath"),
        "{atd}"
    );
    let atd = import(&wrap(
        "<lg><l>Sing, goddess</l><note>a note</note><l>the wrath</l></lg>",
    ));
    assert!(atd.contains("Sing, goddess@^(n1)\nthe wrath\n~@"), "{atd}");
    assert!(atd.contains("@^\na note\n^@(n1)"), "{atd}");
    check(&atd);
}

/// The head of a nested lg (a stanza title) leads its strophe.
#[test]
fn nested_lg_heads_survive() {
    let atd = import(&wrap(
        "<lg><head>Ode</head><lg><head>I</head><l>a</l></lg>\
         <lg><head>II</head><l>b</l></lg></lg>",
    ));
    assert!(
        atd.contains("@~ Ode\n@,I,@.head\na\n\n@,II,@.head\nb\n~@"),
        "{atd}"
    );
    check(&atd);
}

/// A div whose head follows a milestone is still a section, and
/// a self-closing div is an empty division.
#[test]
fn div_head_after_a_milestone() {
    let atd = import(&wrap(
        "<div/><div><milestone unit=\"book\" n=\"1\"/><head>Book One</head><p>a</p></div>",
    ));
    assert!(
        atd.contains("@=== Book One\n@(\"book:1\")\n\na\n===@"),
        "{atd}"
    );
    assert!(!atd.contains(".head"), "{atd}");
    check(&atd);
}

/// Prose between the lines of a verse quote decodes its entities.
#[test]
fn verse_quote_prose_decodes_entities() {
    let atd = import(&wrap(
        "<p>a <quote type=\"verse\"><l>x</l> Castor &amp; Pollux <l>y</l></quote></p>",
    ));
    assert!(atd.contains("Castor & Pollux"), "{atd}");
    assert!(!atd.contains("&amp;"), "{atd}");
}

/// A standard cast list: castGroup nests, role / roleDesc / actor
/// read through inside a castItem.
#[test]
fn cast_list_with_groups_and_roles() {
    let atd = import(&wrap(
        "<castList><head>Persons</head><castGroup><head>Greeks</head>\
         <castItem xml:id=\"ach\"><role>Achilles</role>, <roleDesc>son of Peleus</roleDesc>\
         </castItem><castItem><role>Patroclus</role> <actor>a player</actor></castItem>\
         <roleDesc>the besiegers</roleDesc></castGroup>\
         <castItem><role>Priam</role></castItem></castList><p>x</p>",
    ));
    assert!(atd.contains("@#_Persons_#@"), "{atd}");
    assert!(atd.contains("@#_Greeks_#@"), "{atd}");
    assert!(atd.contains("@:!Achilles, son of Peleus!:@(ach)"), "{atd}");
    assert!(atd.contains("@:!Patroclus a player!:@"), "{atd}");
    assert!(atd.contains("\nthe besiegers\n"), "{atd}");
    assert!(atd.contains("@:!Priam!:@"), "{atd}");
    check(&atd);
}

/// A gloss list (label / item pairs, the shape the at-tei exo
/// writes) is a definition list.
#[test]
fn gloss_list_is_a_definition_list() {
    let atd = import(&wrap(
        "<list type=\"gloss\"><label>Menis</label><item>wrath</item>\
         <label>Thea</label><item>goddess</item></list>",
    ));
    assert!(
        atd.contains("@::;\n@:: Menis\n@;\nwrath\n;@\n::@\n\n@:: Thea\n@;\ngoddess\n;@\n::@\n;::@"),
        "{atd}"
    );
    check(&atd);
}

fn bibliography(entries: &str) -> String {
    import(&wrap(&format!(
        "<p>x</p><div type=\"bibliography\"><head>Bibliography</head>\
         <listBibl>{entries}</listBibl></div>"
    )))
}

/// A bibl type is the entry's genus only as a spellable genos.
#[test]
fn bibl_type_is_a_valid_genos() {
    let atd = bibliography(
        "<bibl xml:id=\"gibbon1776\" type=\"Journal Article\"><author>Gibbon, Edward</author>\
         <title>The Decline and Fall</title></bibl>\
         <bibl xml:id=\"herodotus\" type=\"??\"><author>Herodotus</author>\
         <title level=\"m\">The Histories</title></bibl>",
    );
    assert!(atd.contains("&@.journal-article"), "{atd}");
    assert!(
        atd.contains("@: title\nThe Histories\n:@\n&@.book"),
        "{atd}"
    );
    check(&atd);
}

/// A listBibl inside a footnote is harvested and skipped, not an
/// unsupported element.
#[test]
fn list_bibl_in_a_note() {
    let atd = import(&wrap(
        "<p>A note<note>See <listBibl><bibl xml:id=\"homer\"><author>Homer</author>\
         <title>Iliad</title></bibl></listBibl></note> here.</p>",
    ));
    assert!(atd.contains("A note@^(n1) here."), "{atd}");
    assert!(atd.contains("@& homer\n@: author\nHomer"), "{atd}");
    check(&atd);
}

/// Beside an analytic title, a monogr title that names no level
/// is the containing work.
#[test]
fn unlevelled_monogr_title_is_the_container() {
    let atd = bibliography(
        "<biblStruct xml:id=\"pope1711\"><analytic><author>Pope, Alexander</author>\
         <title level=\"a\">An Essay on Criticism</title></analytic>\
         <monogr><title>Miscellanies</title><imprint><date>1711</date></imprint></monogr>\
         </biblStruct>",
    );
    assert!(
        atd.contains("@: title\nAn Essay on Criticism\n:@\n\n@: booktitle\nMiscellanies\n:@"),
        "{atd}"
    );
    assert!(atd.contains("&@.incollection"), "{atd}");
    check(&atd);
}

/// A repeated element joins the first instead of vanishing.
#[test]
fn repeated_bibl_fields_join() {
    let atd = bibliography(
        "<bibl xml:id=\"gibbon1776\"><author>Gibbon, Edward</author>\
         <title>The Decline and Fall</title><biblScope unit=\"page\">1-43</biblScope>\
         <biblScope unit=\"page\">90-97</biblScope><note>First edition.</note>\
         <note>Six volumes.</note></bibl>",
    );
    assert!(atd.contains("@: pages\n1-43; 90-97\n:@"), "{atd}");
    assert!(
        atd.contains("@: note\nFirst edition.; Six volumes.\n:@"),
        "{atd}"
    );
    check(&atd);
}

/// A div that held only its head and the bibliography leaves no
/// hollow section; the entries still close the document.
#[test]
fn bibliography_div_leaves_no_hollow_section() {
    let atd =
        bibliography("<bibl xml:id=\"homer\"><author>Homer</author><title>Iliad</title></bibl>");
    assert!(!atd.contains("Bibliography"), "{atd}");
    assert!(atd.contains("@@@!(bibliogramma)"), "{atd}");
    check(&atd);
}

/// An entry without an xml:id keeps its place under a
/// synthesized key that takes no id the source spells.
#[test]
fn bibl_without_an_id_survives() {
    let atd = bibliography(
        "<bibl xml:id=\"bibl-2\"><author>Homer</author><title>Iliad</title></bibl>\
         <bibl>An entry without an id.</bibl>",
    );
    assert!(
        atd.contains("@& bibl-3\n@: note\nAn entry without an id.\n:@\n&@.misc"),
        "{atd}"
    );
    check(&atd);
}
