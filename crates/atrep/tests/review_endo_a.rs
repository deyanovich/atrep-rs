//! Regression tests for the review findings in the shared
//! importer helpers and the Markdown, HTML, RST, Org, Djot,
//! DocBook and BibTeX endomorphoses.

use atrep::{dendron, endo};

/// A `>` line without the space after the marker is still a
/// quote line; the importer used to loop forever on it.
#[test]
fn djot_quote_without_space_terminates() {
    let doc = endo::djot_to_document("Text.\n\n>Quoted without space.\n").unwrap();
    let atd = dendron::serialize(&doc);
    assert!(atd.contains("@>\nQuoted without space.\n>@"), "{atd}");
}

/// Every block importer recurses once per nesting level; past a
/// generous bound the import fails with a clear error instead of
/// overflowing the stack. A deep-but-sane ladder still imports.
/// Non-ASCII text in an Org link's target or description used to
/// push the scan past the link's end (byte offset added to a
/// char index) and drop the prose after it.
#[test]
fn org_link_with_non_ascii_keeps_the_rest_of_the_line() {
    let org =
        "* Title\n\nSee [[https://example.org/iliad][Ἰλιάς]] and then the rest of the line.\n";
    let atd = dendron::serialize(&endo::org_to_document(org).unwrap());
    assert!(
        atd.contains("See Ἰλιάς (@><https://example.org/iliad><@) and then the rest of the line."),
        "{atd}"
    );
    let org = "See [[https://example.org/Ἰλιάς]] and then the rest.\n";
    let atd = dendron::serialize(&endo::org_to_document(org).unwrap());
    assert!(
        atd.contains("See @><https://example.org/Ἰλιάς><@ and then the rest."),
        "{atd}"
    );
}

/// Every opener with no closer used to rescan to the end of the
/// paragraph (quadratic); a long run of them now imports in
/// linear time, and the text survives as prose.
#[test]
fn unclosed_openers_scan_linearly() {
    let started = std::time::Instant::now();
    let n = 10000;
    let atd = dendron::serialize(&endo::markdown_to_document(&"[a ".repeat(n)).unwrap());
    assert!(atd.contains("[a [a [a "), "{}", &atd[..60]);
    let atd = dendron::serialize(&endo::markdown_to_document(&"<a ".repeat(n)).unwrap());
    assert!(atd.contains("<a <a <a "), "{}", &atd[..60]);
    let atd = dendron::serialize(&endo::org_to_document(&"[[a ".repeat(n)).unwrap());
    assert!(atd.contains("[[a [[a "), "{}", &atd[..60]);
    let atd = dendron::serialize(&endo::org_to_document(&"[fn:a ".repeat(n)).unwrap());
    assert!(atd.contains("[fn:a [fn:a "), "{}", &atd[..60]);
    let atd = dendron::serialize(&endo::org_to_document(&"*a ".repeat(n)).unwrap());
    assert!(atd.contains("*a *a *a "), "{}", &atd[..60]);
    let atd = dendron::serialize(&endo::djot_to_document(&"[a ".repeat(n)).unwrap());
    assert!(atd.contains("[a [a [a "), "{}", &atd[..60]);
    let atd = dendron::serialize(&endo::djot_to_document(&"_a ".repeat(n)).unwrap());
    assert!(atd.contains("_a _a _a "), "{}", &atd[..60]);
    let atd = dendron::serialize(&endo::rst_to_document(&"[#a ".repeat(n)).unwrap());
    assert!(atd.contains("[#a [#a "), "{}", &atd[..60]);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
}

/// Character references decode in one pass: the predefined
/// entities, `&nbsp;`, decimal and hexadecimal numerics. Unknown
/// or malformed ones stay literal.
#[test]
fn character_references_decode() {
    let html = "<html><body><p>He said &quot;Achilles&quot; &amp;&nbsp;wept &#8212; so &#x2020; \
                it&apos;s &amp;lt; &bogus; &#xZZ; &#0; R&D</p></body></html>";
    let atd = dendron::serialize(&endo::html_to_document(html).unwrap());
    assert!(
        atd.contains(
            "He said \"Achilles\" &\u{a0}wept \u{2014} so \u{2020} it's &lt; &bogus; &#xZZ; &#0; R&D"
        ),
        "{atd}"
    );
    // The same decoder serves attribute values.
    let atd = dendron::serialize(
        &endo::html_to_document("<p><a href=\"https://example.org/?a=1&amp;b=&#50;\"></a></p>")
            .unwrap(),
    );
    assert!(atd.contains("@><https://example.org/?a=1&b=2><@"), "{atd}");
}

/// A `>` inside a quoted attribute value does not end the tag.
#[test]
fn quoted_attribute_may_hold_a_closing_angle() {
    let atd = dendron::serialize(
        &endo::html_to_document("<p title=\"a>b\" class='x>y'>Sing, goddess.</p>").unwrap(),
    );
    assert!(atd.contains("Sing, goddess."), "{atd}");
    let xml = "<article><title>Iliad</title><para role=\"a>b\">Sing, goddess.</para></article>";
    let atd = dendron::serialize(&endo::docbook_to_document(xml).unwrap());
    assert!(atd.contains("Sing, goddess."), "{atd}");
}

/// A CDATA section is literal text, not a declaration to skip.
#[test]
fn cdata_is_text() {
    let xml = "<article><title>Iliad</title>\
               <para>Wrath <![CDATA[<of> Achilles & &amp; ]]]]>sung.</para>\
               <programlisting><![CDATA[if a < b && c > d { run() }]]></programlisting></article>";
    let atd = dendron::serialize(&endo::docbook_to_document(xml).unwrap());
    assert!(atd.contains("Wrath <of> Achilles & &amp; ]]sung."), "{atd}");
    assert!(atd.contains("if a < b && c > d { run() }"), "{atd}");
    let err = endo::docbook_to_document("<article><para><![CDATA[open</para></article>")
        .unwrap_err()
        .to_string();
    assert!(err.contains("unterminated CDATA"), "{err}");
    let atd = dendron::serialize(
        &endo::html_to_document("<pre><code><![CDATA[a < b]]></code></pre>").unwrap(),
    );
    assert!(atd.contains("a < b"), "{atd}");
}

/// An empty reading inside a TEI `choice` (`<expan/>`) has no
/// close tag; the importer used to run on looking for one.
#[test]
fn tei_choice_takes_an_empty_reading() {
    let tei = r#"<TEI xmlns="http://www.tei-c.org/ns/1.0">
<teiHeader><fileDesc><titleStmt><title>Moralia</title></titleStmt></fileDesc></teiHeader>
<text><body>
<p>See <choice><abbr>fig.</abbr><expan/></choice> one and <choice><sic/><corr>two</corr></choice> after.</p>
</body></text></TEI>"#;
    let atd = dendron::serialize(&endo::tei_to_document(tei).unwrap());
    assert!(atd.contains("See fig. one and two after."), "{atd}");
}

/// `@comment` needs no braces in BibTeX; without them the skip
/// used to run to the first `{` anywhere and swallow the next
/// entry.
#[test]
fn bibtex_comment_without_braces_keeps_the_next_entry() {
    let bib = "@comment this file lists classical works\n\
               @book{gibbon1776,\n  author = {Gibbon, Edward},\n  year = 1776\n}\n\
               @comment{braced, {nested} }\n\
               @book{austen1813,\n  author = {Austen, Jane},\n  year = 1813\n}\n\
               @comment trailing words";
    let atd = dendron::serialize(&endo::bibtex_to_document(bib).unwrap());
    assert!(atd.contains("@& gibbon1776\n"), "{atd}");
    assert!(atd.contains("@& austen1813\n"), "{atd}");
    assert!(!atd.contains("classical") && !atd.contains("braced") && !atd.contains("trailing"));
}

/// Braces protect a quote inside a quoted value.
#[test]
fn bibtex_quoted_value_keeps_a_braced_quote() {
    let bib = "@book{austen1813,\n  author = \"Austen, Jane\",\n  \
               title = \"Pride and {\"}Prejudice{\"}\",\n  year = 1813\n}\n";
    let atd = dendron::serialize(&endo::bibtex_to_document(bib).unwrap());
    assert!(
        atd.contains("@: title\nPride and {\"}Prejudice{\"}\n:@"),
        "{atd}"
    );
    assert!(atd.contains("@: year\n1813\n:@"), "{atd}");
}

/// The DocBook exo writes enmedia as a mediaobject and an onym
/// anchor as `<anchor xml:id>`; the importer reads both back.
#[test]
fn docbook_media_and_anchor_round_trip() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<article xmlns="http://docbook.org/ns/docbook" xmlns:xlink="http://www.w3.org/1999/xlink" version="5.0">
<title>Title</title>
<para>A paragraph.<anchor xml:id="intro"/></para>
<mediaobject><imageobject><imagedata fileref="media/m1.svg"/></imageobject></mediaobject>
</article>"#;
    let atd = dendron::serialize(&endo::docbook_to_document(xml).unwrap());
    assert!(atd.contains("A paragraph.@(intro)"), "{atd}");
    assert!(atd.contains("@@@@(media/m1.svg)"), "{atd}");
    let err =
        endo::docbook_to_document("<article><mediaobject><textobject/></mediaobject></article>")
            .unwrap_err()
            .to_string();
    assert!(
        err.contains("mediaobject without an imagedata fileref"),
        "{err}"
    );
}

/// The Org exo writes enmedia as a standalone `[[file:path]]`
/// and an onym anchor as `<<name>>`; the importer reads both
/// back instead of a link and literal prose.
#[test]
fn org_media_and_targets_round_trip() {
    let org = "* Title\n\nA paragraph.<<intro>> More, with [[file:notes.org]] inline.\n\n\
               [[file:media/m1.svg]]\n\nA <<<radio>>> target and a << loose pair >>.\n";
    let atd = dendron::serialize(&endo::org_to_document(org).unwrap());
    assert!(
        atd.contains("A paragraph.@(intro) More, with @><file:notes.org><@ inline."),
        "{atd}"
    );
    assert!(atd.contains("\n@@@@(media/m1.svg)\n"), "{atd}");
    assert!(
        atd.contains("A <<<radio>>> target and a << loose pair >>."),
        "{atd}"
    );
}

/// The HTML exo writes an onymized division as `<div id>`, a
/// deixis to it as `<a class="pointer" href="#id">`, and an onym
/// anchor as an empty `<span id>`; the importer reads all three
/// back. An ordinary fragment link is still a link.
#[test]
fn html_onyms_and_pointers_round_trip() {
    let html = "<!doctype html>\n<html>\n<body>\n\
                <p>A paragraph with an onym.<span id=\"intro\"></span></p>\n\
                <div id=\"o1\" class=\"note\">\n<p>Boxed.</p>\n</div>\n\
                <p>See <a class=\"pointer\" href=\"#o1\">&#8224;</a> and \
                <a href=\"#o1\">the box</a>.</p>\n</body>\n</html>\n";
    let atd = dendron::serialize(&endo::html_to_document(html).unwrap());
    assert!(atd.contains("A paragraph with an onym.@(intro)"), "{atd}");
    assert!(atd.contains("@_\nBoxed.\n_@(o1).note"), "{atd}");
    assert!(atd.contains("See @_(o1) and the box (@><#o1><@)."), "{atd}");
}

/// A nested quote block closes with its own END; the outer one
/// used to end at the first END_QUOTE and fail on the second.
#[test]
fn org_quote_blocks_nest() {
    let org = "#+BEGIN_QUOTE\nOuter.\n#+BEGIN_QUOTE\nInner.\n#+END_QUOTE\nBack to outer.\n\
               #+END_QUOTE\n\n#+BEGIN_SRC org\n#+BEGIN_QUOTE\n#+END_SRC\n";
    let atd = dendron::serialize(&endo::org_to_document(org).unwrap());
    assert!(
        atd.contains("@>\nOuter.\n\n@>\nInner.\n>@\n\nBack to outer.\n>@"),
        "{atd}"
    );
    // A verbatim body still ends at the first END.
    assert!(atd.contains("#+BEGIN_QUOTE\n\"@@@"), "{atd}");
}

#[test]
fn nesting_bound_org() {
    nesting(endo::org_to_document(&format!(
        "{}deep\n{}",
        deep("#+BEGIN_QUOTE\n", 2000),
        deep("#+END_QUOTE\n", 2000)
    )));
}

/// at-djot declares the footnote sim; the importer reads
/// `[^label]` as a deixis and `[^label]: body` as the footnote
/// body, with indented continuation, as the Markdown importer
/// does.
#[test]
fn djot_footnotes_import() {
    let dj = "The Achaeans advanced.[^count] They kept coming.\n\n\
              [^count]: A thousand ships,\n  by the catalogue.\n\n  And _more_ besides.\n\n\
              After [^ not a callout].\n";
    let doc = endo::djot_to_document(dj).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("The Achaeans advanced.@^(count) They kept coming."),
        "{atd}"
    );
    assert!(
        atd.contains("@^\nA thousand ships, by the catalogue.\n\nAnd @_more_@ besides.\n^@(count)"),
        "{atd}"
    );
    assert!(atd.contains("After [^ not a callout]."), "{atd}");
    // Same reading as the Markdown importer's.
    let md = endo::markdown_to_document(
        "The Achaeans advanced.[^count] They kept coming.\n\n[^count]: A thousand ships.\n",
    )
    .unwrap();
    let dj = endo::djot_to_document(
        "The Achaeans advanced.[^count] They kept coming.\n\n[^count]: A thousand ships.\n",
    )
    .unwrap();
    assert_eq!(md.blocks, dj.blocks);
}

/// Emphasis matching: a star between spaces is no delimiter, a
/// triple run is emphasis around strong, and an emphasis may
/// hold a strong.
#[test]
fn markdown_and_rst_emphasis_respect_flanking() {
    let md = "***both*** and **a * b** done, *one **two** three* and 2 * 3 * 4.\n";
    let atd = dendron::serialize(&endo::markdown_to_document(md).unwrap());
    assert!(
        atd.contains("@*@**both**@*@ and @**a * b**@ done, @*one @**two**@ three*@ and 2 * 3 * 4."),
        "{atd}"
    );
    let rst = "A **a * b** span, *x * y* too, and 2 * 3 * 4.\n";
    let atd = dendron::serialize(&endo::rst_to_document(rst).unwrap());
    assert!(
        atd.contains("A @**a * b**@ span, @*x * y*@ too, and 2 * 3 * 4."),
        "{atd}"
    );
    // Still linear when nothing closes.
    let started = std::time::Instant::now();
    endo::markdown_to_document(&"*a ".repeat(10000)).unwrap();
    endo::rst_to_document(&"*a ".repeat(10000)).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

/// A fence of four backticks closes on four, so a three-backtick
/// line inside it is content; it used to swallow the rest of the
/// document.
#[test]
fn longer_code_fence_closes_on_its_own_length() {
    let src = "````\ncode with ``` inside\n````\n\nAfter.\n";
    let doc = endo::markdown_to_document(src).unwrap();
    assert_eq!(doc.blocks.len(), 2, "{}", dendron::serialize(&doc));
    assert!(matches!(&doc.blocks[0],
        atrep::dendron::Block::VerbatimBlock { content, .. } if content == "code with ``` inside\n"));
    let doc = endo::djot_to_document(src).unwrap();
    assert_eq!(doc.blocks.len(), 2, "{}", dendron::serialize(&doc));
    assert!(matches!(&doc.blocks[0],
        atrep::dendron::Block::VerbatimBlock { content, .. } if content == "code with ``` inside"));
    // The info string still follows a longer fence.
    let doc = endo::markdown_to_document("```` rust\nfn quiet() {}\n`````\n").unwrap();
    assert!(dendron::serialize(&doc).contains(".rust"));
}

/// A field's body continues on indented lines; further
/// paragraphs used to come back as a sibling blockquote.
#[test]
fn rst_field_body_continues_on_indented_lines() {
    let rst = ":Author: Alexander Pope\n\n   And a second paragraph of the field.\n\n\
               :Work: An Essay\n   on Criticism\n\nAfter.\n";
    let atd = dendron::serialize(&endo::rst_to_document(rst).unwrap());
    assert!(
        atd.contains("@: Author\nAlexander Pope\n\nAnd a second paragraph of the field.\n:@"),
        "{atd}"
    );
    assert!(atd.contains("@: Work\nAn Essay on Criticism\n:@"), "{atd}");
    assert!(atd.contains("\nAfter.\n"), "{atd}");
    assert!(!atd.contains("@>"), "{atd}");
}

fn deep(s: &str, n: usize) -> String {
    s.repeat(n)
}

fn nesting(r: atrep::Result<atrep::Document>) {
    let err = r.expect_err("deep nesting must fail").to_string();
    assert!(err.contains("nesting deeper than"), "{err}");
}

#[test]
fn nesting_bound_markdown_quotes() {
    nesting(endo::markdown_to_document(&format!(
        "{} deep",
        deep(">", 20000)
    )));
    let doc = endo::markdown_to_document(&format!("{} deep", deep(">", 50))).unwrap();
    let atd = dendron::serialize(&doc);
    assert_eq!(atd.matches("@>\n").count(), 50);
    assert!(atd.contains("deep"));
}

#[test]
fn nesting_bound_markdown_items() {
    nesting(endo::markdown_to_document(&format!(
        "{}deep",
        deep("- ", 2000)
    )));
}

#[test]
fn nesting_bound_html_blocks() {
    nesting(endo::html_to_document(&format!(
        "{}<p>deep</p>{}",
        deep("<blockquote>", 2000),
        deep("</blockquote>", 2000)
    )));
}

#[test]
fn nesting_bound_html_inlines() {
    nesting(endo::html_to_document(&format!(
        "<p>{}deep{}</p>",
        deep("<em>", 2000),
        deep("</em>", 2000)
    )));
}

#[test]
fn nesting_bound_rst() {
    nesting(endo::rst_to_document(&format!("{}deep", deep("   ", 2000))));
}

#[test]
fn nesting_bound_djot() {
    nesting(endo::djot_to_document(&format!(
        "{} deep",
        deep("> ", 2000)
    )));
}

#[test]
fn nesting_bound_docbook() {
    nesting(endo::docbook_to_document(&format!(
        "<article><title>T</title>{}<para>deep</para>{}</article>",
        deep("<blockquote>", 2000),
        deep("</blockquote>", 2000)
    )));
}

/// The at-rst exo writes a fixed-width underline, and docutils
/// reads a short underline of four or more characters as a title
/// (with a warning), so a long heading must come back a heading.
#[test]
fn rst_title_takes_an_underline_shorter_than_itself() {
    let title = "The History of the Decline and Fall of the Roman Empire, Volume the First";
    let rst = format!("{title}\n{}\n\nText.\n", "=".repeat(56));
    let atd = dendron::serialize(&endo::rst_to_document(&rst).unwrap());
    assert!(atd.contains(&format!("@#{title}#@")), "{atd}");
    assert!(!atd.contains("===="), "{atd}");
    // Three characters are too short to be an underline at all.
    let atd = dendron::serialize(&endo::rst_to_document("Herodotus\n===\n").unwrap());
    assert!(!atd.contains("@#Herodotus#@"), "{atd}");
}
