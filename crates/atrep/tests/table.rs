//! The table model: rows are records, cells are block-holding
//! fields named by their column; the pipe shorthand expands to the
//! same rows; a span (the hspan / vspan monosims first inside a
//! cell) covers the cells the rows below and the columns to the
//! right omit.

use std::path::{Path, PathBuf};

use atrep::{dendron, exo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn kanon(tmp: &Path, name: &str, atd: &str) -> Result<dendron::Document, atrep::error::Error> {
    let path = tmp.join(format!("{name}.atd"));
    std::fs::write(&path, atd).unwrap();
    kanonizo::kanonizo_file(&path).map(|r| r.document)
}

const SHORTHAND: &str = "\
@@@!litogramma

@=Census=@

@+ Census
Year | Population
---- | ----------
1998 | 12,400
1999 | 12,900
+@(tbl-1998) Data source: annual census.
";

const BLOCK: &str = "\
@@@!litogramma

@=Census=@

@+ Census
@+=
@+:
Year
:+@
@+:
Population
:+@
=+@
@+-
@+:
1998
:+@
@+:
12,400
:+@
-+@
@+-
@+:
1999
:+@
@+:
12,900
:+@
-+@
+@(tbl-1998) Data source: annual census.
";

#[test]
fn shorthand_and_block_form_kanonize_alike() {
    let tmp = tmp_dir("table-forms");
    let a = dendron::serialize(&kanon(&tmp, "short", SHORTHAND).unwrap());
    let b = dendron::serialize(&kanon(&tmp, "block", BLOCK).unwrap());
    assert_eq!(a, b, "{a}");
    // The dash rule made the first row a header row; cells are
    // the row's children, their content a paragraph.
    assert!(
        a.contains("@+=\n@+:\nYear\n:+@\n\n@+:\nPopulation\n:+@\n=+@"),
        "{a}"
    );
    assert!(
        a.contains("@+-\n@+:\n1998\n:+@\n\n@+:\n12,400\n:+@\n-+@"),
        "{a}"
    );
    // Idempotent: the kanon re-kanonizes to itself.
    let c = dendron::serialize(&kanon(&tmp, "again", &a).unwrap());
    assert_eq!(a, c);
}

#[test]
fn cells_hold_blocks_and_name_their_column() {
    let tmp = tmp_dir("table-blocks");
    let atd = "\
@@@!litogramma

@=Notes=@

@+ Notes
@+=
@+:
Year
:+@
@+:
Note
:+@
=+@
@+-
@+:
1998
:+@
@+:
Estimate only.

Revised in 1999 after the @/second/@ count.
:+@
-+@
@+-
@+: Year
2000
:+@
@+:
Final.
:+@
-+@
+@
";
    let k = dendron::serialize(&kanon(&tmp, "notes", atd).unwrap());
    assert!(
        k.contains("@+:\nEstimate only.\n\nRevised in 1999 after the @/second/@ count.\n:+@"),
        "{k}"
    );
    assert!(k.contains("@+: Year\n2000\n:+@"), "{k}");
}

const SPANS: &str = "\
@@@!litogramma

@=Spans=@

@+ Spans
@+=
@+:
A
:+@
@+:
B
:+@
@+:
C
:+@
=+@
@+-
@+:
@+_(2) tall
:+@
@+:
@+>(2) wide
:+@
-+@
@+-
@+:
b2
:+@
-+@
@+-
@+:
a3
:+@
-+@
+@
";

#[test]
fn spans_cover_omitted_cells_and_short_rows_are_padded() {
    let tmp = tmp_dir("table-spans");
    let k = dendron::serialize(&kanon(&tmp, "spans", SPANS).unwrap());
    // The row under the vspan writes two cells and gets no
    // padding in the covered column; the last row is padded to
    // three.
    assert!(
        k.contains("@+:\n@+_(2) tall\n:+@\n\n@+:\n@+>(2) wide\n:+@\n-+@"),
        "{k}"
    );
    assert!(k.contains("@+-\n@+:\nb2\n:+@\n\n@+:\n:+@\n-+@"), "{k}");
    assert!(
        k.contains("@+-\n@+:\na3\n:+@\n\n@+:\n:+@\n\n@+:\n:+@\n-+@"),
        "{k}"
    );

    // Too wide a row is refused; so is a span past the last row,
    // a cell under a vspan, and a span below 2.
    let wide = SPANS.replace(
        "@+:\na3\n:+@\n",
        "@+:\na3\n:+@\n@+:\nx\n:+@\n@+:\ny\n:+@\n@+:\nz\n:+@\n",
    );
    let err = kanon(&tmp, "wide", &wide).unwrap_err().to_string();
    assert!(err.contains("exceed the 3 column(s)"), "{err}");
    let under = SPANS.replace(
        "@+:\nb2\n:+@\n",
        "@+:\nb2\n:+@\n@+:\nc2\n:+@\n@+:\nd2\n:+@\n",
    );
    let err = kanon(&tmp, "under", &under).unwrap_err().to_string();
    assert!(err.contains("exceed the 3 column(s)"), "{err}");
    let past = SPANS.replace("@+_(2)", "@+_(4)");
    let err = kanon(&tmp, "past", &past).unwrap_err().to_string();
    assert!(err.contains("past the last row"), "{err}");
    let bad = SPANS.replace("@+_(2)", "@+_(1)");
    let err = kanon(&tmp, "bad", &bad).unwrap_err().to_string();
    assert!(err.contains("at least 2"), "{err}");
}

#[test]
fn table_content_is_rows_and_shorthand_only() {
    let tmp = tmp_dir("table-content");
    let atd = "\
@@@!litogramma

@=Bad=@

@+ Bad
@\"
A quote is not a row.
\"@
+@
";
    let err = kanon(&tmp, "bad", atd).unwrap_err().to_string();
    assert!(
        err.contains("neither a row nor a shorthand paragraph"),
        "{err}"
    );
}

#[test]
fn tables_export_with_spans() {
    let tmp = tmp_dir("table-exo");
    let atd = "\
@@@!litogramma

@=Spans=@

@+ Spans
@+=
@+:
A
:+@
@+:
B
:+@
=+@
@+-
@+:
@+_(2) tall
:+@
@+:
wide
:+@
-+@
@+-
@+:
b2
:+@
-+@
@+-
@+:
@+>(2) both
:+@
-+@
+@(tbl) Caption.
";
    let doc = kanon(&tmp, "spans", atd).unwrap();
    // HTML: header cells are th by their row (the `in` rule), the
    // spans attributes, the span monosims gone from the text.
    let x = exo::resolve_exo(&tmp, "litogramma", "html").unwrap();
    let html = exo::render(&doc, &x, &tmp).unwrap();
    assert!(html.contains("<th>\n<p>A</p>\n</th>"), "{html}");
    assert!(
        html.contains("<td rowspan=\"2\">\n<p>tall</p>\n</td>"),
        "{html}"
    );
    assert!(
        html.contains("<td colspan=\"2\">\n<p>both</p>\n</td>"),
        "{html}"
    );
    assert!(html.contains("<caption>Spans</caption>"), "{html}");
    let x = exo::resolve_exo(&tmp, "litogramma", "latex").unwrap();
    let tex = exo::render(&doc, &x, &tmp).unwrap();
    assert!(
        tex.contains("\\ltrow{\\ltcell{\\textbf{A\n}}{}{}\n\\ltcell{\\textbf{B\n}}{}{}}"),
        "{tex}"
    );
    assert!(
        tex.contains("\\ltcell{\\multirow{2}{*}{tall\n}}{}{2}"),
        "{tex}"
    );
    assert!(
        tex.contains("\\ltcell{\\multicolumn{2}{l}{both\n}}"),
        "{tex}"
    );
    std::fs::write(tmp.join("spans.tex"), &tex).unwrap();
    for (name, body) in &x.assets {
        std::fs::write(tmp.join(name), body).unwrap();
    }
    let x = exo::resolve_exo(&tmp, "litogramma", "gemtext").unwrap();
    let gmi = exo::render(&doc, &x, &tmp).unwrap();
    assert!(gmi.contains("tall\n[2 rows]\nwide\n"), "{gmi}");
}

/// A TEI table round-trips through at-tei: a labelled row is the
/// header row, cols and rows are the spans, and the import reads
/// them back.
#[test]
fn tables_round_trip_through_tei() {
    use atrep::{endo, morph};
    let tmp = tmp_dir("table-tei");
    let doc = kanon(&tmp, "spans", SPANS).unwrap();
    let route = morph::resolve_route(&tmp, "litogramma", "at-tei").unwrap();
    let at_tei = morph::apply_route(&doc, &route).unwrap();
    let x = exo::resolve_exo(&tmp, "at-tei", "tei").unwrap();
    let tei = exo::render(&at_tei, &x, &tmp).unwrap();
    assert!(
        tei.contains("<row role=\"label\">\n<cell>A</cell>"),
        "{tei}"
    );
    assert!(
        tei.contains("<cell rows=\"2\">tall</cell>\n<cell cols=\"2\">wide</cell>"),
        "{tei}"
    );
    let back_path = tmp.join("back.atd");
    std::fs::write(
        &back_path,
        dendron::serialize(&endo::tei_to_document(&tei).unwrap()),
    )
    .unwrap();
    let back = kanonizo::kanonizo_file(&back_path).unwrap().document;
    assert_eq!(dendron::serialize(&back), dendron::serialize(&doc));
}

/// at-html carries the table family: an HTML table imports into
/// it (a row of th a header row, colspan and rowspan the spans, the
/// caption the lemma), exports back through the html exo, and a
/// litogramma table morphs into it by identity.
#[test]
fn html_tables_import_and_export() {
    use atrep::{endo, morph};
    let html = r#"<html><body><table id="prices"><caption>Prices</caption>
<thead><tr><th>Boy</th><th>Price</th><th>Note</th></tr></thead>
<tbody><tr><td rowspan="2">Ben</td><td colspan="2">an apple</td></tr>
<tr><td>a kite</td><td><p>later</p><p>much later</p></td></tr></tbody></table></body></html>"#;
    let doc = endo::html_to_document(html).unwrap();
    let atd = dendron::serialize(&doc);
    assert!(
        atd.contains("@+ Prices\n@+=\n@+:\nBoy\n:+@\n\n@+:\nPrice\n:+@\n\n@+:\nNote\n:+@\n=+@"),
        "{atd}"
    );
    assert!(
        atd.contains("@+-\n@+:\n@+_(2) Ben\n:+@\n\n@+:\n@+>(2) an apple\n:+@\n-+@"),
        "{atd}"
    );
    assert!(
        atd.contains("@+:\nlater\n\nmuch later\n:+@\n-+@\n+@(prices)"),
        "{atd}"
    );
    let tmp = tmp_dir("table-html");
    let doc = kanon(&tmp, "prices", &atd).unwrap();
    let x = exo::resolve_exo(&tmp, "at-html", "html").unwrap();
    let out = exo::render(&doc, &x, &tmp).unwrap();
    // The id is an onym nothing refers to, which kanonizo drops.
    assert!(out.contains("<table>\n<caption>Prices</caption>"), "{out}");
    assert!(out.contains("<th>\n<p>Boy</p>\n</th>"), "{out}");
    assert!(
        out.contains("<td rowspan=\"2\">\n<p>Ben</p>\n</td>"),
        "{out}"
    );
    assert!(
        out.contains("<td colspan=\"2\">\n<p>an apple</p>\n</td>"),
        "{out}"
    );
    // Re-import of the export is the same kanon.
    let back = endo::html_to_document(&out).unwrap();
    let back = kanon(&tmp, "back", &dendron::serialize(&back)).unwrap();
    assert_eq!(dendron::serialize(&back), dendron::serialize(&doc));

    // litogramma -> at-html keeps a table.
    let lit = kanon(&tmp, "lit", SPANS).unwrap();
    let route = morph::resolve_route(&tmp, "litogramma", "at-html").unwrap();
    let at_html = morph::apply_route(&lit, &route).unwrap();
    let s = dendron::serialize(&at_html);
    assert!(s.contains("@+:\n@+_(2) tall\n:+@"), "{s}");
}
