//! Regression tests for the exomorphosis review findings (the
//! renderer in `exo.rs` and the standard-library `.exo` files).
//! Each test reproduces one finding and pins the corrected
//! behaviour.

use std::path::{Path, PathBuf};

use atrep::{dendron, exo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("review-exo-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn kanon_of(tmp: &Path, atd: &str) -> dendron::Document {
    let path = tmp.join("doc.atd");
    std::fs::write(&path, atd).unwrap();
    kanonizo::kanonizo_file(&path).unwrap().document
}

fn render(tmp: &Path, doc: &dendron::Document, dialect: &str, target: &str) -> String {
    let x = exo::resolve_exo(tmp, dialect, target).unwrap();
    exo::render(doc, &x, tmp).unwrap()
}

/// Run an external tool over `content` when it is installed and
/// return its stdout; a missing tool yields None so the check is
/// skipped rather than failed. The tool rejecting the content is
/// a failure.
fn run_tool(tool: &str, args: &[&str], content: &str) -> Option<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{tool} rejected the output:\n{}\n---\n{content}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn validate(tool: &str, args: &[&str], content: &str) {
    let _ = run_tool(tool, args, content);
}

fn xmllint(xml: &str) {
    validate("xmllint", &["--noout", "-"], xml);
}

// ---------------------------------------------------------------
// litogramma.latex.exo: citation keys
// ---------------------------------------------------------------

/// A citation key is a BibTeX identifier: the LaTeX rules write
/// it verbatim, matching the key the companion .bib carries, so a
/// key with `_` resolves.
#[test]
fn latex_cite_key_is_written_verbatim() {
    let tmp = tmp_dir("cite-key");
    let kanon = kanon_of(
        &tmp,
        "\
@@@!litogramma

@=On Fences=@

Rome fell slowly @>[(gibbon_1776) and @@.@>[(gibbon_1776)ch. 15.@@ as well.

@@@!(bibliogramma)
@& gibbon_1776
@: author
Gibbon, Edward
:@

@: title
The History of the Decline and Fall of the Roman Empire
:@
&@.book
!@@@
",
    );
    let x = exo::resolve_exo(&tmp, "litogramma", "latex").unwrap();
    let (latex, aux) = exo::render_with_aux(&kanon, &x, &tmp).unwrap();
    assert!(
        latex.contains(r"\ltcite{gibbon_1776} and \ltcitespan{gibbon_1776}{ch. 15}"),
        "{latex}"
    );
    let bib = aux
        .iter()
        .find(|(d, t, _)| d == "bibliogramma" && t == "bib")
        .map(|(_, _, c)| c.as_str())
        .expect("companion .bib");
    assert!(bib.contains("@book{gibbon_1776,"), "{bib}");
    // The class cites plainly when the span is empty (a diaphane
    // holding a bare cite), instead of printing an empty locator.
    assert!(x.assets.iter().any(|(_, body)| {
        body.contains(r"\if\relax\detokenize{#2}\relax\cite{#1}\else\cite[#2]{#1}\fi")
    }),);
}

// ---------------------------------------------------------------
// Footnotes in the lightweight-markup exos
// ---------------------------------------------------------------

const FOOTNOTE_ATD: &str = "\
@@@!at-markdown

@#The war#@

The emus advanced.@^(count) They kept coming.

@^
Twenty thousand of them.
^@(count)
";

/// at-markdown defines the footnote sim; its Markdown exo renders
/// the callout as `[^key]` and the body as `[^key]: ...`, and the
/// import reads the result back to the same kanon.
#[test]
fn markdown_footnote_exports_and_round_trips() {
    use atrep::endo;
    let tmp = tmp_dir("md-footnote");
    let kanon = kanon_of(&tmp, FOOTNOTE_ATD);
    let md = render(&tmp, &kanon, "at-markdown", "md");
    assert_eq!(
        md,
        "# The war\n\nThe emus advanced.[^o1] They kept coming.\n\n[^o1]: Twenty thousand of them.\n"
    );
    validate("cmark-gfm", &["-e", "footnotes"], &md);
    let back = endo::markdown_to_document(&md).unwrap();
    let back_dir = tmp_dir("md-footnote-back");
    let back = kanon_of(&back_dir, &dendron::serialize(&back));
    assert_eq!(dendron::serialize(&back), dendron::serialize(&kanon));
}

/// A multi-paragraph footnote body hangs its continuation under
/// a four-space indent, the shape the importer reads.
#[test]
fn markdown_footnote_body_hangs_continuation() {
    let tmp = tmp_dir("md-footnote-multi");
    let kanon = kanon_of(
        &tmp,
        "\
@@@!at-markdown

Text.@^(n)

@^
First paragraph.

Second paragraph.
^@(n)
",
    );
    let md = render(&tmp, &kanon, "at-markdown", "md");
    assert_eq!(
        md,
        "Text.[^o1]\n\n[^o1]: First paragraph.\n\n    Second paragraph.\n"
    );
}

/// The HTML rendering of a Markdown footnote: a superscript link
/// for the callout and an aside for the body, the litogramma
/// shape, overlaid on the inherited at-html rules.
#[test]
fn markdown_footnote_exports_to_html() {
    let tmp = tmp_dir("md-footnote-html");
    let kanon = kanon_of(&tmp, FOOTNOTE_ATD);
    let html = render(&tmp, &kanon, "at-markdown", "html");
    assert!(
        html.contains(
            "<p>The emus advanced.<sup class=\"footnote-ref\"><a href=\"#o1\">&#8224;</a></sup> They kept coming.</p>"
        ),
        "{html}"
    );
    assert!(
        html.contains(
            "<aside class=\"footnote\" id=\"o1\">\n<p>Twenty thousand of them.</p>\n</aside>"
        ),
        "{html}"
    );
    // The inherited rules still apply.
    assert!(html.contains("<h1>The war</h1>"), "{html}");
}

/// at-djot's footnote renders in the same `[^label]` shape.
#[test]
fn djot_footnote_exports() {
    let tmp = tmp_dir("dj-footnote");
    let kanon = kanon_of(
        &tmp,
        "\
@@@!at-djot

The emus advanced.@^(count) They kept coming.

@^
Twenty thousand.
^@(count)
",
    );
    let dj = render(&tmp, &kanon, "at-djot", "dj");
    assert_eq!(
        dj,
        "The emus advanced.[^o1] They kept coming.\n\n[^o1]: Twenty thousand.\n"
    );
}

// ---------------------------------------------------------------
// at-html.xhtml.exo: the link rule
// ---------------------------------------------------------------

/// The xhtml exo renders a link like the html exo does, instead
/// of failing on the sim.
#[test]
fn xhtml_renders_links() {
    let tmp = tmp_dir("xhtml-link");
    let kanon = kanon_of(
        &tmp,
        "@@@!at-html\n\nSee @><https://example.org/iliad><@ here.\n",
    );
    let xhtml = render(&tmp, &kanon, "at-html", "xhtml");
    assert!(
        xhtml.contains(
            "<p>See <a href=\"https://example.org/iliad\">https://example.org/iliad</a> here.</p>"
        ),
        "{xhtml}"
    );
    xmllint(&xhtml);
}

// ---------------------------------------------------------------
// at-tei.tei.exo: the link target
// ---------------------------------------------------------------

/// A link target lands in an attribute value, so the TEI link
/// rule escapes it: a query string with `&` gives well-formed
/// XML.
#[test]
fn tei_link_target_is_escaped() {
    let tmp = tmp_dir("tei-attrs");
    let kanon = kanon_of(
        &tmp,
        "\
@@@!at-tei

@_
Polly went to @,@?.(troy)Troy,@.placename and @><https://example.org/?a=1&b=2><@.
_@.text
",
    );
    let tei = render(&tmp, &kanon, "at-tei", "tei");
    assert!(
        tei.contains("<ref target=\"https://example.org/?a=1&amp;b=2\"/>"),
        "{tei}"
    );
    assert!(
        tei.contains("<placeName ref=\"#troy\">Troy</placeName>"),
        "{tei}"
    );
    xmllint(&tei);
}

// ---------------------------------------------------------------
// exo.rs: table cells
// ---------------------------------------------------------------

/// A framed row keeps an empty first or last cell, and a row of
/// dash cells after the header rule is a row, not a second rule.
#[test]
fn table_keeps_empty_edge_cells_and_dash_rows() {
    let tmp = tmp_dir("table-cells");
    let kanon = kanon_of(
        &tmp,
        "\
@@@!litogramma

@+ Heroes
| Name | Book |
|------+------|
|  | Iliad |
| - | - |
| Hector | Iliad |
| Odysseus |  |
+@
",
    );
    let html = render(&tmp, &kanon, "litogramma", "html");
    assert!(
        html.contains("<tr><th>Name</th><th>Book</th></tr>"),
        "{html}"
    );
    assert!(html.contains("<tr><td></td><td>Iliad</td></tr>"), "{html}");
    assert!(html.contains("<tr><td>-</td><td>-</td></tr>"), "{html}");
    assert!(
        html.contains("<tr><td>Hector</td><td>Iliad</td></tr>"),
        "{html}"
    );
    assert!(
        html.contains("<tr><td>Odysseus</td><td></td></tr>"),
        "{html}"
    );
    assert!(!html.contains("------"), "{html}");
}

// ---------------------------------------------------------------
// exo.rs: aliased imports
// ---------------------------------------------------------------

/// An aliased import carries the sim's *row and *cell rules
/// (and term-keyed variants) along to the alias, not only the
/// sim, solo and deixis rules.
#[test]
fn aliased_import_carries_row_and_cell_rules() {
    let tmp = tmp_dir("alias-rows");
    std::fs::write(tmp.join("tabl.dia"), "@@@!atrep\n\n@@::litogramma::+ #\n").unwrap();
    std::fs::write(
        tmp.join("tabl.html.exo"),
        "\
@@@!atrep-exo
@=tabl=>html

@-> *document
@(grammata)
>-@

@-> *paragraph
<p>@(grammata)</p>
>-@

@-> *strophe
[strophe]@(grammata)
>-@

@-> *stichos
[stichos]@(grammata)
>-@
",
    )
    .unwrap();
    let kanon = kanon_of(
        &tmp,
        "\
@@@!tabl

@# Heroes
| Name | Book |
|------+------|
| Hector | Iliad |
#@
",
    );
    let html = render(&tmp, &kanon, "tabl", "html");
    assert!(
        html.contains("<table>\n<caption>Heroes</caption>\n<tr><th>Name</th><th>Book</th></tr>\n<tr><td>Hector</td><td>Iliad</td></tr>\n</table>"),
        "{html}"
    );
    assert!(!html.contains("[strophe]"), "{html}");
}

// ---------------------------------------------------------------
// exo.rs: auxiliary outputs of nested renderers
// ---------------------------------------------------------------

/// A bibliography embedded inside an embedded dialektos reaches
/// the companion .bib: the inner renderer's auxiliary outputs
/// travel up to the host.
#[test]
fn nested_englossis_routes_aux_outputs() {
    let tmp = tmp_dir("nested-aux");
    let kanon = kanon_of(
        &tmp,
        "\
@@@!koine

Outer koine text.

@@@!(litogramma)
Inner litogramma cites @>[(gibbon1776).

@@@!(bibliogramma)
@& gibbon1776
@: author
Gibbon, Edward
:@
&@.book
!@@@
!@@@
",
    );
    let x = exo::resolve_exo(&tmp, "koine", "latex").unwrap();
    let (latex, aux) = exo::render_with_aux(&kanon, &x, &tmp).unwrap();
    assert!(latex.contains(r"\bibliography{\jobname}"), "{latex}");
    let bib = aux
        .iter()
        .find(|(d, t, _)| d == "bibliogramma" && t == "bib")
        .map(|(_, _, c)| c.as_str())
        .expect("companion .bib from the nested bibliography");
    assert!(bib.contains("@book{gibbon1776,"), "{bib}");
}

// ---------------------------------------------------------------
// at-markdown.md.exo: autolinks and ordered items
// ---------------------------------------------------------------

/// A CommonMark autolink takes no backslash escapes, so the URL
/// is written raw and reads back unchanged.
#[test]
fn markdown_autolink_target_is_raw() {
    use atrep::endo;
    let tmp = tmp_dir("md-autolink");
    let kanon = kanon_of(
        &tmp,
        "@@@!at-markdown\n\nSee @><https://example.org/iliad_book_1><@ here.\n",
    );
    let md = render(&tmp, &kanon, "at-markdown", "md");
    assert_eq!(md, "See <https://example.org/iliad_book_1> here.\n");
    let back = endo::markdown_to_document(&md).unwrap();
    let back = kanon_of(&tmp_dir("md-autolink-back"), &dendron::serialize(&back));
    assert_eq!(dendron::serialize(&back), dendron::serialize(&kanon));
}

/// Continuation paragraphs of ordered items hang under four
/// spaces, so an item numbered ten or more keeps them.
#[test]
fn markdown_two_digit_items_keep_continuations() {
    let tmp = tmp_dir("md-ordered");
    let mut atd = String::from("@@@!at-markdown\n");
    for n in 1..=10 {
        atd.push_str(&format!("\n@.({n})\nitem {n}\n.@\n"));
    }
    atd.truncate(atd.len() - 3);
    atd.push_str("\nsecond paragraph of ten\n.@\n");
    let kanon = kanon_of(&tmp, &atd);
    let md = render(&tmp, &kanon, "at-markdown", "md");
    assert!(md.contains("1. item 1\n"), "{md}");
    assert!(
        md.ends_with("10. item 10\n\n    second paragraph of ten\n"),
        "{md}"
    );
    if let Some(html) = run_tool("cmark-gfm", &[], &md) {
        assert!(
            html.contains("<li>\n<p>item 10</p>\n<p>second paragraph of ten</p>\n</li>"),
            "{html}"
        );
    }
}

// ---------------------------------------------------------------
// litogramma exos: deixes to structure
// ---------------------------------------------------------------

/// A deixis may point at any para-simmere; one aimed at a
/// section renders in every litogramma target as the `>`
/// reference does.
#[test]
fn deixis_to_a_section_renders() {
    let tmp = tmp_dir("deixis-section");
    let kanon = kanon_of(
        &tmp,
        "\
@@@!litogramma

@# Intro
Text.
#@(intro)

@_ Aside
Boxed.
_@(box)

See @#(intro), @_(box) and @>(intro).
",
    );
    let html = render(&tmp, &kanon, "litogramma", "html");
    assert!(
        html.contains("<p>See <a class=\"ref\" href=\"#o1\">[o1]</a>, <a class=\"ref\" href=\"#o2\">[o2]</a> and <a class=\"ref\" href=\"#o1\">[o1]</a>.</p>"),
        "{html}"
    );
    let latex = render(&tmp, &kanon, "litogramma", "latex");
    assert!(
        latex.contains(r"See \ltref{o1}, \ltref{o2} and \ltref{o1}."),
        "{latex}"
    );
    let gemtext = render(&tmp, &kanon, "litogramma", "gemtext");
    assert!(gemtext.contains("See ,  and ."), "{gemtext}");
}

// ---------------------------------------------------------------
// XML and HTML targets: quotes in attribute values
// ---------------------------------------------------------------

/// Monosim parameters land in attribute values, so the XML and
/// HTML escape tables cover `"`: a page number with an inch mark
/// gives well-formed TEI.
#[test]
fn tei_attributes_escape_quotes() {
    let tmp = tmp_dir("tei-quotes");
    let kanon = kanon_of(
        &tmp,
        "@@@!at-tei\n\n@_\nPolly read on @|(12\").pb to the end.\n_@.text\n",
    );
    let tei = render(&tmp, &kanon, "at-tei", "tei");
    assert!(tei.contains("<pb n=\"12&quot;\"/>"), "{tei}");
    xmllint(&tei);
}

/// The same for the litogramma HTML export: a hidden index term
/// and a cite key carrying a quote.
#[test]
fn html_attributes_escape_quotes() {
    let tmp = tmp_dir("html-quotes");
    let kanon = kanon_of(
        &tmp,
        "@@@!litogramma\n\nIndex @%%(Pride\"Prejudice) and cite @>[(a\"b) here.\n",
    );
    let html = render(&tmp, &kanon, "litogramma", "html");
    assert!(
        html.contains("<span class=\"index-term\" data-term=\"Pride&quot;Prejudice\"></span>"),
        "{html}"
    );
    assert!(
        html.contains("<cite data-key=\"a&quot;b\"></cite>"),
        "{html}"
    );
}
