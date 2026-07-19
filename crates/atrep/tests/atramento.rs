//! Atramento: the core endomorphosis into litogramma.
//! Conformance backbone: the superset/passthrough property,
//! the projection (idempotence) property, and the per-construct
//! sugar-to-sim mappings of the atramento spec.

use atrep::atramento::atramento_to_litogramma as compile;

fn c(src: &str) -> String {
    compile(src).unwrap()
}

// ----- conformance properties ------------------------------------

/// Canonical litogramma passes through byte-identical (modulo
/// the trailing newline the compiler guarantees).
#[test]
fn passthrough() {
    let lit = "\
@@@!litogramma

@=Solitude in the Early Ode=@

@=:A. Careful:=@

@=\"
The ode's program, read against its sources.
\"=@

@#(1) The Claim
Pope's early ode @>[(pope1700) already contains the @/whole program/@.@^(n1)

@^
Written at about twelve.
^@(n1)

@##(1) Counterpoint
Austen answers @>[(austen1811) in @*prose*@.

@\"
Beatus ille qui procul negotiis.
\"@
##@
#@
";
    assert_eq!(c(lit), lit);
}

/// The compiler is a projection: compiling twice equals
/// compiling once.
#[test]
fn projection() {
    let atr = "\
@@#(chapter)

# Loomings

Call me Ishmael, /quietly/ and *without ceremony*.@^why

@^why Because the name fits.

## The Watch

- first thing
- second thing

> Beatus ille.
> -- Horace

@: Estragon
Nothing to be done.

# The Carpet-Bag

The `sea-chest` was packed. See <https://example.org/whale>.
";
    let once = c(atr);
    let twice = c(&once);
    assert_eq!(once, twice);
}

/// A missing dialektos declaration is supplied.
#[test]
fn declaration_supplied() {
    let out = c("Just a paragraph.\n");
    assert!(out.starts_with("@@@!litogramma\n\n"));
    let lit = "@@@!litogramma\n\nAlready declared.\n";
    assert_eq!(c(lit), lit);
}

// ----- inline layer ----------------------------------------------

#[test]
fn emphasis_typewriter() {
    let out = c("A /very/ good day, a *bold* claim, a */loud whisper/*.\n");
    assert!(out.contains("@/very/@"));
    assert!(out.contains("@*bold*@"));
    assert!(out.contains("@*/loud whisper/*@"));
}

#[test]
fn emphasis_flanking_protects_prose() {
    let out = c("Use and/or, drive 60 km/h, dated 12/05/2026, compute 3 * 4 * 5.\n");
    assert!(out.contains("and/or"));
    assert!(out.contains("km/h"));
    assert!(out.contains("12/05/2026"));
    assert!(out.contains("3 * 4 * 5"));
    assert!(!out.contains("@/"));
    assert!(!out.contains("@*"));
}

#[test]
fn emphasis_nests_and_wraps_lines() {
    let out = c("He spoke *of a /dream within/ a dream*.\n");
    assert!(out.contains("@*of a @/dream within/@ a dream*@"));
    let out = c("An /emphasis wrapped\nacross lines/ survives.\n");
    assert!(out.contains("@/emphasis wrapped\nacross lines/@"));
}

#[test]
fn strict_emphasis_is_stable() {
    let lit = "@@@!litogramma\n\nAlready @/marked/@ and @*strong*@ text.\n";
    assert_eq!(c(lit), lit);
}

#[test]
fn escapes() {
    let out = c("A literal \\*star\\* and a \\/slash\\/ and \\@ stays.\n");
    assert!(out.contains("A literal *star* and a /slash/ and \\@ stays."));
}

#[test]
fn dashes_normalize() {
    let out = c("An em\u{2014}dash and an en\u{2013}dash and a double -- dash.\n");
    assert!(out.contains("An em--dash and an en-dash and a double -- dash."));
}

#[test]
fn code_spans_and_fences() {
    let out = c("Try `cat intro | wc -l` for counts.\n\n```go\ngo mod tidy\n```\n");
    assert!(out.contains("@@\"cat intro | wc -l\"@@"));
    assert!(out.contains("@@@\"\ngo mod tidy\n\"@@@.go"));
    // Code content is never sugar-processed.
    let out = c("Keep `a /b/ c` verbatim.\n");
    assert!(out.contains("@@\"a /b/ c\"@@"));
}

#[test]
fn autolinks() {
    let out = c("See <https://example.org/x> for details.\n");
    assert!(out.contains("@><https://example.org/x><@"));
    // Not an autolink: no scheme.
    let out = c("Compare a<b and c>d.\n");
    assert!(out.contains("a<b and c>d"));
}

#[test]
fn text_links_are_reserved() {
    let err = compile("A [link](https://example.org) here.\n").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("F9"), "unexpected error: {msg}");
}

// ----- structure -------------------------------------------------

#[test]
fn headings_with_declared_base() {
    let atr = "\
@@#(chapter)

# Loomings

Call me Ishmael.

## The Watch

Some years ago.

# The Carpet-Bag

I stuffed a shirt.
";
    let out = c(atr);
    let expected = "\
@@@!litogramma

@===(1) Loomings

Call me Ishmael.

@#(1) The Watch

Some years ago.
#@
===@

@===(2) The Carpet-Bag

I stuffed a shirt.
===@
";
    assert_eq!(out, expected);
}

#[test]
fn headings_default_base_is_section() {
    let out = c("# Methods\n\nText.\n");
    assert!(out.contains("@#(1) Methods"));
    assert!(out.trim_end().ends_with("#@"));
}

#[test]
fn unnumbered_heading() {
    let out = c("#! Preface\n\nText.\n");
    assert!(out.contains("@# Preface\n"));
    assert!(!out.contains("@#("));
}

#[test]
fn heading_too_deep_errors() {
    assert!(compile("##### Too deep\n").is_err());
}

#[test]
fn ordinal_sugar_in_headings() {
    let atr = "@@#(chapter)\n\n# Chapter @.@: The Whale\n\n## Section @..@ here\n";
    let out = c(atr);
    assert!(out.contains("@===(1) Chapter @.1.@: The Whale"));
    assert!(out.contains("@#(1) Section @.1.1.@ here"));
}

#[test]
fn top_matter_tail_elision() {
    let out = c("@= A Dream Within a Dream\n\n@=: Edgar Allan Poe\n");
    assert!(out.contains("@= A Dream Within a Dream =@"));
    assert!(out.contains("@=: Edgar Allan Poe :=@"));
}

// ----- lists -----------------------------------------------------

#[test]
fn unordered_list() {
    let out = c("- item one\n- item two\n");
    let expected = "@--\n@-\nitem one\n-@\n@-\nitem two\n-@\n--@";
    assert!(out.contains(expected), "got:\n{out}");
}

#[test]
fn ordered_list_numbers_recomputed() {
    let out = c(". first\n13. second\n");
    assert!(out.contains("@.-(1)\nfirst"));
    assert!(out.contains("@.-(2)\nsecond"));
}

#[test]
fn ordered_list_start_value() {
    let out = c("37. first\n. second\n");
    assert!(out.contains("@.-(37)"));
    assert!(out.contains("@.-(38)"));
}

#[test]
fn alphabetic_list_via_head() {
    let out = c("@.. D\n. first\n. second\n");
    assert!(out.contains("@.-(D)"));
    assert!(out.contains("@.-(E)"));
}

#[test]
fn list_genos_moves_to_episim() {
    let out = c("@..senses\n. a mirror\n. a model\n");
    assert!(out.contains("..@.senses"), "got:\n{out}");
}

#[test]
fn multi_paragraph_item() {
    let out = c("- item one\n\n  still item one\n- item two\n");
    assert!(
        out.contains("@-\nitem one\n\nstill item one\n-@"),
        "got:\n{out}"
    );
}

#[test]
fn definition_list() {
    let out = c(": lemma :: definition\n: another :: second\n");
    assert!(
        out.contains("@::;\n@:: lemma\n@;\ndefinition\n;@\n::@"),
        "got:\n{out}"
    );
    assert!(out.trim_end().ends_with(";::@"));
}

// ----- blocks ----------------------------------------------------

#[test]
fn blockquote_with_attribution() {
    let out = c("> Beatus ille qui procul negotiis.\n>\n> Solutus omni faenore.\n> -- Horace\n");
    let expected = "@\"\nBeatus ille qui procul negotiis.\n\nSolutus omni faenore.\n\"@ Horace";
    assert!(out.contains(expected), "got:\n{out}");
}

#[test]
fn verse_fences() {
    let out =
        c("~ Easter Wings\nLord, who createdst man,\n      Decaying more and more.\n~ 1633\n");
    assert!(out.contains("@~ Easter Wings\n"));
    assert!(out.contains("      Decaying more and more."));
    assert!(out.contains("~@ 1633"));
}

#[test]
fn figure_sugar() {
    let out = c("![Temperature plot](./plot.png)\n");
    assert!(
        out.contains("@<()\n@@@@(./plot.png)\nTemperature plot\n<@"),
        "got:\n{out}"
    );
    let out = c("![Pressure](./p.png|Graph)\n");
    assert!(out.contains("@<() Graph\n@@@@(./p.png)\nPressure\n<@"));
}

#[test]
fn aside_block() {
    let out = c("@|warning\nThis may be dangerous.\n|@\n");
    assert!(out.contains("@|<(atr-as-1)"));
    assert!(
        out.contains("@|\nThis may be dangerous.\n|@(atr-as-1).warning"),
        "got:\n{out}"
    );
}

#[test]
fn inline_aside_rejected() {
    assert!(compile("Do it @|idea|differently|@ now.\n").is_err());
}

// ----- notes -----------------------------------------------------

#[test]
fn note_callout_and_definition_sugar() {
    let atr = "Some text.@^13 More.@^historynote1\n\n@^13 The first note.\n\n@^historynote1: On history.\n";
    let out = c(atr);
    assert!(out.contains("Some text.@^(13) More.@^(historynote1)"));
    // Definitions move to the trailing region in reference order.
    let d13 = out.find("@^\nThe first note.\n^@(13)").unwrap();
    let dh = out.find("@^\nOn history.\n^@(historynote1)").unwrap();
    assert!(d13 < dh);
}

#[test]
fn inline_note() {
    let out = c("Some text@^This is the note.^@ in a paragraph.\n");
    assert!(out.contains("Some text@^(atr-fn-1) in a paragraph."));
    assert!(out.contains("@^\nThis is the note.\n^@(atr-fn-1)"));
}

#[test]
fn note_families() {
    let atr = "Text.@^^ch1 More.@^^^end1\n\n@^^ch1 A chapter endnote.\n\n@^^^end1 An endnote.\n";
    let out = c(atr);
    assert!(out.contains("Text.@^^(ch1) More.@^^^(end1)"));
    let ce = out.find("@^^\nA chapter endnote.\n^^@(ch1)").unwrap();
    let en = out.find("@^^^\nAn endnote.\n^^^@(end1)").unwrap();
    assert!(ce < en, "footnotes, chapter endnotes, endnotes order");
}

#[test]
fn strict_callouts_untouched() {
    let lit = "@@@!litogramma\n\nText.@^(n1)\n\n@^\nNote.\n^@(n1)\n";
    assert_eq!(c(lit), lit);
}

// ----- drama -----------------------------------------------------

#[test]
fn dialogue_auto_close() {
    let atr = "\
@: Estragon
Nothing to be done.

@: Vladimir
Nothing you can do about it.
";
    let out = c(atr);
    let expected = "\
@: Estragon
Nothing to be done.
:@

@: Vladimir
Nothing you can do about it.
:@
";
    assert!(out.ends_with(expected), "got:\n{out}");
}

#[test]
fn verse_dialogue_auto_close() {
    let atr = "@:~ Hamlet\nTo be, or not to be.\nThat is the question.\n\n@: Ophelia\nMy lord?\n";
    let out = c(atr);
    assert!(
        out.contains("To be, or not to be.\nThat is the question.\n~:@\n\n@: Ophelia"),
        "got:\n{out}"
    );
}

#[test]
fn acts_and_scenes_autonumber() {
    let atr = "@:= Act @.@\n\n@:# Scene @.@\n\n@: Estragon\nWell?\n";
    let out = c(atr);
    assert!(out.contains("@:=(1) Act @.1.@"));
    assert!(out.contains("@:#(1) Scene @.1.@"));
    assert!(out.trim_end().ends_with(":@\n#:@\n=:@"), "got:\n{out}");
}

#[test]
fn stage_directions() {
    let out = c("@:[ They do not move.\n");
    assert!(out.contains("@:[\nThey do not move.\n]:@"));
    let out = c("@: Hamlet @:( muttering\nA little more than kin.\n");
    assert!(out.contains("@: Hamlet @:( muttering ):@"));
    let out = c("You lie. @:( a beat ):@ You lie.\n");
    assert!(out.contains("You lie. @:( a beat ):@ You lie."));
}

#[test]
fn dramatis_personae() {
    let out = c("@:! Estragon\n@:! Vladimir\n");
    assert!(out.contains("@:! Estragon !:@"));
    assert!(out.contains("@:! Vladimir !:@"));
}

#[test]
fn prose_dialogue_line() {
    let out = c("@:- --Alors, pourquoi ? @:_ me dit-il. _:@\n");
    assert!(out.contains("@:- --Alors, pourquoi ? @:_ me dit-il. _:@ -:@"));
}

// ----- integration -----------------------------------------------

/// Compiled output kanonizes against the std litogramma.
#[test]
fn output_kanonizes() {
    let atr = "\
@= The Specimen

@=: A. Writer

@@#(chapter)

# Loomings

Call me /Ishmael/.@^why A *fine* start.

@^why The name fits.

## The Watch

- first thing
- second thing

> Beatus ille.
> -- Horace

~ Fragment
A verse line,
      an indented one.
~

@: Estragon
Nothing to be done.
";
    let out = compile(atr).unwrap();
    let tmp = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("atramento-kanonizo");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("doc.atd"), &out).unwrap();
    let kanon = atrep::kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap_or_else(|e| panic!("kanonizo failed: {e}\non:\n{out}"));
    let atd = atrep::dendron::serialize(&kanon.document);
    assert!(atd.contains("Ishmael"));
}
