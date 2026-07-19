//! The at-html syntax mapper (both directions) and exomorphosis
//! rule inheritance: at-markdown's HTML rendering is acquired
//! entirely through its lineage from at-html.

use std::path::{Path, PathBuf};

use atrep::{dendron, endo, exo, kanonizo};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SAMPLE_HTML: &str = "\
<!doctype html>
<html>
<body>
<h1>Solitude</h1>
<p>Text with <em>emphasis</em>, <strong>strength</strong>, and
<span class=\"gloss\">a gloss</span> beside <code>x &lt; y</code>.</p>
<blockquote>
<p>A quoted thought.</p>
</blockquote>
<ul>
<li>first point</li>
<li>second point</li>
</ul>
<ol>
<li value=\"1\">one</li>
<li value=\"2\">two</li>
</ol>
<div class=\"note\">
<p>Boxed remark.</p>
</div>
<pre><code>if a &amp;&amp; b { run() }
</code></pre>
</body>
</html>
";

/// Import, serialize, check the exact at-html .atd source.
#[test]
fn html_endo_produces_canonical_atd() {
    let doc = endo::html_to_document(SAMPLE_HTML).unwrap();
    assert_eq!(
        dendron::serialize(&doc),
        "@@@!at-html\n\
         \n\
         @#Solitude#@\n\
         \n\
         Text with @/emphasis/@, @!strength!@, and\n\
         @,a gloss,@.gloss beside @@\"x < y\"@@.\n\
         \n\
         @>\n\
         A quoted thought.\n\
         >@\n\
         \n\
         @--\n\
         @-\n\
         first point\n\
         -@\n\
         \n\
         @-\n\
         second point\n\
         -@\n\
         --@\n\
         \n\
         @..\n\
         @.(1)\n\
         one\n\
         .@\n\
         \n\
         @.(2)\n\
         two\n\
         .@\n\
         ..@\n\
         \n\
         @_\n\
         Boxed remark.\n\
         _@.note\n\
         \n\
         @@@\"\n\
         if a && b { run() }\n\
         \"@@@\n"
    );
}

/// HTML -> at-html -> kanon -> HTML is a fixed point on the
/// canonical subset.
#[test]
fn html_roundtrip_is_idempotent() {
    let tmp = tmp_dir("html-roundtrip");
    let cycle = |html: &str| -> String {
        let doc = endo::html_to_document(html).unwrap();
        std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
        let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
        let x = exo::resolve_exo(&tmp, "at-html", "html").unwrap();
        exo::render(&kanon.document, &x, &tmp).unwrap()
    };
    let html1 = cycle(SAMPLE_HTML);
    let html2 = cycle(&html1);
    assert_eq!(html1, html2);
    // The genos survives as a class, the escape table as entities.
    assert!(html1.contains("<span class=\"gloss\">a gloss</span>"));
    assert!(html1.contains("<code>x &lt; y</code>"));
    assert!(html1.contains("if a &amp;&amp; b { run() }"));
}

/// The showcase: at-markdown has NO html .exo of its own - its
/// HTML rendering is acquired entirely through the lineage from
/// at-html, with the emphasis and strong rules remapped along the
/// aliases (/ -> *, ! -> **) and the escape table inherited.
#[test]
fn at_markdown_html_export_is_inherited() {
    let tmp = tmp_dir("md-html-inherit");
    let md = "# Title\n\nText with *emphasis*, **strength** & `code`.\n\n> Quote.\n";
    let doc = endo::markdown_to_document(md).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "at-markdown", "html").unwrap();
    let html = exo::render(&kanon.document, &x, &tmp).unwrap();
    assert_eq!(
        html,
        "<!doctype html>\n\
         <html>\n\
         <body>\n\
         <h1>Title</h1>\n\
         <p>Text with <em>emphasis</em>, <strong>strength</strong> \
         &amp; <code>code</code>.</p>\n\
         <blockquote>\n\
         <p>Quote.</p>\n\
         </blockquote>\n\
         </body>\n\
         </html>"
    );
}

/// A child's own .exo overrides same-pattern inherited rules and
/// adds new ones; everything else falls through to the parent.
#[test]
fn child_exo_overrides_inherited_rules() {
    let tmp = tmp_dir("exo-override");
    std::fs::write(
        tmp.join("base.lektos"),
        "@@@!atrep\n\n\
         @=== emphasis\n@/ grammata /@\n===@\n\n\
         @=== term\n@: grammata :@\n===@\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("base.html.exo"),
        "@@@!atrep-exo\n@=base=>html\n\n\
         @-> *document\n@(grammata)\n>-@\n\n\
         @-> *paragraph\n<p>@(grammata)</p>\n>-@\n\n\
         @-> /\n<em>@(grammata)</em>\n>-@\n\n\
         @-> :\n<dfn>@(grammata)</dfn>\n>-@\n",
    )
    .unwrap();
    std::fs::write(tmp.join("child.dia"), "@@@!atrep\n\n@@::base\n").unwrap();
    std::fs::write(
        tmp.join("child.html.exo"),
        "@@@!atrep-exo\n@=child=>html\n\n\
         @-> /\n<i class=\"house-style\">@(grammata)</i>\n>-@\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("doc.atd"),
        "@@@!child\n\nBoth @/styles/@ and @:terms:@ render.\n",
    )
    .unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "child", "html").unwrap();
    let html = exo::render(&kanon.document, &x, &tmp).unwrap();
    assert_eq!(
        html,
        "<p>Both <i class=\"house-style\">styles</i> and \
         <dfn>terms</dfn> render.</p>"
    );
}

/// Later inheritance declarations override earlier ones when two
/// parents supply the same pattern.
#[test]
fn later_inheritance_op_wins() {
    let tmp = tmp_dir("exo-op-order");
    for (id, tag) in [("one", "em"), ("two", "i")] {
        std::fs::write(
            tmp.join(format!("{id}.lektos")),
            "@@@!atrep\n\n@=== emphasis\n@/ grammata /@\n===@\n",
        )
        .unwrap();
        std::fs::write(
            tmp.join(format!("{id}.html.exo")),
            format!(
                "@@@!atrep-exo\n@={id}=>html\n\n\
                 @-> *document\n@(grammata)\n>-@\n\n\
                 @-> *paragraph\n<p>@(grammata)</p>\n>-@\n\n\
                 @-> /\n<{tag}>@(grammata)</{tag}>\n>-@\n"
            ),
        )
        .unwrap();
    }
    // `two` re-imports the emphasis under a fresh alias, so the
    // definitions do not conflict; its structural rules override
    // one's per the later-op-wins rule.
    std::fs::write(
        tmp.join("child.dia"),
        "@@@!atrep\n\n@@::one\n\n@@::two::/ !\n",
    )
    .unwrap();
    std::fs::write(tmp.join("doc.atd"), "@@@!child\n\n@/a/@ and @!b!@\n").unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "child", "html").unwrap();
    let html = exo::render(&kanon.document, &x, &tmp).unwrap();
    // one's `/` rule intact; two's `/` rule arrived remapped to `!`.
    assert_eq!(html, "<p><em>a</em> and <i>b</i></p>");
}

/// A dialektos with no own .exo but no inheritable rules either
/// still fails resolution.
#[test]
fn no_rules_anywhere_is_unresolvable() {
    let tmp = tmp_dir("exo-nothing");
    std::fs::write(
        tmp.join("lonely.lektos"),
        "@@@!atrep\n\n@=== emphasis\n@/ grammata /@\n===@\n",
    )
    .unwrap();
    let err = exo::resolve_exo(&tmp, "lonely", "html").unwrap_err();
    assert!(matches!(
        err.kind,
        atrep::error::ErrorKind::UnresolvableExo(_)
    ));
}

/// The verse stichoi sim survives the HTML round trip: strophes,
/// authorial leading spaces, lemma, and hypograph all invert.
#[test]
fn verse_roundtrips_through_html() {
    let tmp = tmp_dir("html-verse");
    let source = "@@@!at-html\n\
        \n\
        @~ The Ode\n\
        Happy the man,\n\
        \x20\x20\x20In his own ground.\n\
        \n\
        Blest, who can find\n\
        ~@ written c. 1700\n";
    std::fs::write(tmp.join("doc.atd"), source).unwrap();
    let kanon = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    let x = exo::resolve_exo(&tmp, "at-html", "html").unwrap();
    let html = exo::render(&kanon.document, &x, &tmp).unwrap();
    assert!(html.contains("<div class=\"verse\">"));
    assert!(html.contains("<p class=\"verse-title\">The Ode</p>"));
    assert!(html.contains("Happy the man,<br/>"));
    assert!(html.contains("<p class=\"verse-attribution\">written c. 1700</p>"));

    let doc = endo::html_to_document(&html).unwrap();
    std::fs::write(tmp.join("back.atd"), dendron::serialize(&doc)).unwrap();
    let back = kanonizo::kanonizo_file(&tmp.join("back.atd")).unwrap();
    assert_eq!(
        dendron::serialize(&back.document),
        dendron::serialize(&kanon.document)
    );
}
