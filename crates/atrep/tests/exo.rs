//! Exomorphosis engine tests: the spec's worked example rendered
//! to HTML, plus error conditions.

use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::{dialektos, exo, kanonizo};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["exempli.lektos", "exempli.html.exo"] {
        std::fs::copy(fixtures().join(f), dir.join(f)).unwrap();
    }
    dir
}

/// The spec's worked example (Metamorphoses chapter): the kanon of
/// intro.atd rendered with exempli.html.exo.
#[test]
fn spec_worked_example_html() {
    let result = kanonizo::kanonizo_file(&fixtures().join("intro.atd")).unwrap();
    let exo = exo::resolve_exo(&fixtures(), "exempli", "html").unwrap();
    let html = exo::render(&result.document, &exo, &fixtures()).unwrap();
    assert_eq!(
        html,
        "<!doctype html>\n\
         <html>\n\
         <body>\n\
         <section class=\"overview\">\n\
         <h1>Introduction</h1>\n\
         <p>Atrep documents keep meaning and presentation apart. \
         See <a href=\"#o1\">[o1]</a> for the diagram (as sent from \
         me@example.com).</p>\n\
         </section>\n\
         <figure id=\"o1\" class=\"diagram\">\n\
         <img src=\"media/m1.svg\"/>\n\
         <p><em class=\"caption\">The processing pipeline.</em></p>\n\
         </figure>\n\
         <pre>parse -&gt; validate -&gt; kanonize\n\
         </pre>\n\
         </body>\n\
         </html>"
    );
}

fn parse_exo(source: &str, dir: &Path) -> atrep::Result<exo::Exo> {
    let dial = dialektos::resolve(dir, "exempli").unwrap();
    exo::parse_exo_source(
        source,
        &dir.join("exempli.html.exo"),
        "exempli",
        "html",
        &dial,
    )
}

#[test]
fn genos_specificity_and_overrides() {
    let tmp = tmp_dir("exo-genos");
    // A genos-specific section rule alongside the general one.
    let source = "@@@!atrep-exo\n@=exempli=>html\n\n\
        @-> *document\n@(grammata)\n>-@\n\n\
        @-> *paragraph\n<p>@(grammata)</p>\n>-@\n\n\
        @-> #\n<section>\n@(grammata)\n</section>\n>-@\n\n\
        @-> #.overview\n<section class=\"ov\">\n@(grammata)\n</section>\n>-@\n";
    let exo = parse_exo(source, &tmp).unwrap();
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\n@# One\nplain\n#@\n\n@# Two\ntagged\n#@.overview\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let html = exo::render(&result.document, &exo, &tmp).unwrap();
    assert_eq!(
        html,
        "<section>\n<p>plain</p>\n</section>\n\
         <section class=\"ov\">\n<p>tagged</p>\n</section>"
    );
}

#[test]
fn solo_rule_replaces_paragraph() {
    let tmp = tmp_dir("exo-solo");
    let source = "@@@!atrep-exo\n@=exempli=>html\n\n\
        @-> *document\n@(grammata)\n>-@\n\n\
        @-> *paragraph\n<p>@(grammata)</p>\n>-@\n\n\
        @-> :\n<em>@(grammata)</em>\n>-@\n\n\
        @-> *solo :\n<h1>@(grammata)</h1>\n>-@\n\n\
        @-> ^\n<a href=\"#@(param)\">x</a>\n>-@\n\n\
        @-> *solo ^\n<nav data-onym=\"@(param)\"></nav>\n>-@\n\n\
        @-> *onym-anchor\n<span id=\"@(onym)\"></span>\n>-@\n";
    let exo = parse_exo(source, &tmp).unwrap();
    // Standalone endo/monosim become blocks; inline use stays
    // wrapped in <p>.
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\n@:Alone:@\n\n@^(fig)\n\nWith @:inline:@ use @(fig).\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let html = exo::render(&result.document, &exo, &tmp).unwrap();
    assert_eq!(
        html,
        "<h1>Alone</h1>\n<nav data-onym=\"o1\"></nav>\n\
         <p>With <em>inline</em> use <span id=\"o1\"></span>.</p>"
    );
}

#[test]
fn conditional_sections_gate_on_slots() {
    let tmp = tmp_dir("exo-conditional");
    let source = "@@@!atrep-exo\n@=exempli=>html\n\n\
        @-> *document\n@(grammata)\n>-@\n\n\
        @-> *paragraph\n<p>@(grammata)</p>\n>-@\n\n\
        @-> =\n<figure@[ id=\"@(onym)\"]@@[ data-taxis=\"@(taxis)\"]@>\n\
        @(grammata)\n\
        @[<figcaption>@(lemma)</figcaption>\n]@</figure>\n>-@\n\n\
        @-> ^\n<a href=\"#@(param)\">x</a>\n>-@\n";
    let exo = parse_exo(source, &tmp).unwrap();
    // One figure with an onym, one without (its id attribute and
    // the section around it must vanish). The intervening
    // paragraph breaks the taxis run, so both figures number 1.
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!exempli\n\n@=() Plate\nbody\n=@(pl)\n\nSee @^(pl).\n\n@=() Bare\nbare\n=@\n",
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let html = exo::render(&result.document, &exo, &tmp).unwrap();
    assert_eq!(
        html,
        "<figure id=\"o1\" data-taxis=\"1\">\n\
         <p>body</p>\n\
         <figcaption>Plate</figcaption>\n\
         </figure>\n\
         <p>See <a href=\"#o1\">x</a>.</p>\n\
         <figure data-taxis=\"1\">\n\
         <p>bare</p>\n\
         <figcaption>Bare</figcaption>\n\
         </figure>"
    );
}

#[test]
fn conditional_section_errors() {
    let tmp = tmp_dir("exo-conditional-errors");
    for bad in [
        // No slot inside.
        "@@@!atrep-exo\n@=exempli=>html\n\n@-> #\n@[static]@\n>-@\n",
        // Nested.
        "@@@!atrep-exo\n@=exempli=>html\n\n@-> #\n@[a@[@(onym)]@b]@\n>-@\n",
        // Unterminated.
        "@@@!atrep-exo\n@=exempli=>html\n\n@-> #\n@[<i>@(onym)</i>\n>-@\n",
    ] {
        let err = parse_exo(bad, &tmp).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::InvalidExo(_)), "{bad}");
    }
}

#[test]
fn unhandled_sim_is_error() {
    let tmp = tmp_dir("exo-unhandled");
    let source = "@@@!atrep-exo\n@=exempli=>html\n\n\
        @-> *document\n@(grammata)\n>-@\n\n\
        @-> *paragraph\n<p>@(grammata)</p>\n>-@\n";
    let exo = parse_exo(source, &tmp).unwrap();
    std::fs::write(tmp.join("doc.atd"), "@@@!exempli\n\n@# One\nx\n#@\n").unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let err = exo::render(&result.document, &exo, &tmp).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::ExoUnhandled(p) if p == "#"));
}

#[test]
fn duplicate_pattern_is_error() {
    let tmp = tmp_dir("exo-duplicate");
    let source = "@@@!atrep-exo\n@=exempli=>html\n\n\
        @-> #\na\n>-@\n\n\
        @-> #\nb\n>-@\n";
    let err = parse_exo(source, &tmp).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::InvalidExo(_)));
}

#[test]
fn ambiguous_equal_specificity_is_error() {
    let tmp = tmp_dir("exo-ambiguous");
    let source = "@@@!atrep-exo\n@=exempli=>html\n\n\
        @-> *document\n@(grammata)\n>-@\n\n\
        @-> *paragraph\n@(grammata)\n>-@\n\n\
        @-> #.a\nA\n>-@\n\n\
        @-> #.b\nB\n>-@\n";
    let exo = parse_exo(source, &tmp).unwrap();
    std::fs::write(tmp.join("doc.atd"), "@@@!exempli\n\n@# One\nx\n#@.a.b\n").unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let err = exo::render(&result.document, &exo, &tmp).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::ExoAmbiguous(_)));
}

#[test]
fn declaration_mismatch_is_error() {
    let tmp = tmp_dir("exo-mismatch");
    let source = "@@@!atrep-exo\n@=exempli=>latex\n";
    let err = parse_exo(source, &tmp).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::InvalidExo(_)));
}

#[test]
fn unknown_slot_and_bad_escape_are_errors() {
    let tmp = tmp_dir("exo-badbits");
    let bad_slot = "@@@!atrep-exo\n@=exempli=>html\n\n@-> #\n@(bogus)\n>-@\n";
    assert!(matches!(
        parse_exo(bad_slot, &tmp).unwrap_err().kind,
        ErrorKind::InvalidExo(_)
    ));
    let bad_escape = "@@@!atrep-exo\n@=exempli=>html\n\n@%\n&\n%@\n";
    assert!(matches!(
        parse_exo(bad_escape, &tmp).unwrap_err().kind,
        ErrorKind::InvalidExo(_)
    ));
}

#[test]
fn missing_exo_is_error() {
    let tmp = tmp_dir("exo-missing");
    let err = exo::resolve_exo(&tmp, "exempli", "latex").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnresolvableExo(_)));
}

/// The cell interpretation: `*row <sym> <sep>` splits stichoi
/// lines into cells at render time; the kanon is untouched. A
/// dash-run line marks header rows (.header variants).
#[test]
fn table_cells_render_through_row_rules() {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("exo-cells");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(
        tmp.join("tabular.lektos"),
        "@@@!atrep\n\n@=== table\n@+\nstichos\n+@\n===@\n\n@=== emphasis\n@/ grammata /@\n===@\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("tabular.html.exo"),
        "@@@!atrep-exo\n@=tabular=>html\n\n\
         @-> *document\n@(grammata)\n>-@\n\n\
         @-> *paragraph\n<p>@(grammata)</p>\n>-@\n\n\
         @-> /\n<em>@(grammata)</em>\n>-@\n\n\
         @-> +\n<table>\n@(grammata)\n</table>\n>-@\n\n\
         @-> *row + |\n<tr>@(cells)</tr>\n>-@\n\n\
         @-> *cell +\n<td>@(grammata)</td>\n>-@\n\n\
         @-> *cell + .header\n<th>@(grammata)</th>\n>-@\n\n\
         @-> *strophe\n@(grammata)\n>-@\n\n\
         @-> *stichos\n@(grammata)\n>-@\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!tabular\n\n@+\n| Poem | Year |\n|------+------|\n| @/Ode/@ | 1700 |\n+@\n",
    )
    .unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    // The kanon keeps rows as lines, separators as text.
    assert!(kanon.kanon.contains("| Poem | Year |"));
    let x = exo::resolve_exo(&tmp, "tabular", "html").unwrap();
    let html = exo::render(&kanon.document, &x, &tmp).unwrap();
    assert!(html.contains("<tr><th>Poem</th><th>Year</th></tr>"));
    assert!(html.contains("<tr><td><em>Ode</em></td><td>1700</td></tr>"));
    assert!(!html.contains("------"));
}
