//! Morphisms (spec v0.11 draft, first slice): .hom/.iso rules,
//! implicit identity, derived embeddings, and the std pair
//! at-markdown <=> at-html.

use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::{dendron, endo, exo, kanonizo, morph};

fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn kanon_of(tmp: &Path, source: &str) -> atrep::Document {
    std::fs::write(tmp.join("doc.atd"), source).unwrap();
    kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document
}

/// at-markdown => at-html needs no morphism file: the embedding is
/// derived from at-markdown's lineage (aliases invert).
#[test]
fn derived_embedding_from_lineage() {
    let tmp = tmp_dir("morph-derived");
    let md = "# Title\n\nWith *em*, **strong**, and `code`.\n\n> Quote.\n";
    let doc = endo::markdown_to_document(md).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let doc = kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document;

    let m = morph::resolve_morph(&tmp, "at-markdown", "at-html").unwrap();
    let out = morph::apply(&doc, &m).unwrap();
    assert_eq!(
        dendron::serialize(&out),
        "@@@!at-html\n\
         \n\
         @#Title#@\n\
         \n\
         With @/em/@, @!strong!@, and @@\"code\"@@.\n\
         \n\
         @>\n\
         Quote.\n\
         >@\n"
    );
    // The morphed kanon renders with at-html's own exo rules.
    let x = exo::resolve_exo(&tmp, "at-html", "html").unwrap();
    let html = exo::render(&out, &x, &tmp).unwrap();
    assert!(html.contains("<em>em</em>"));
    assert!(html.contains("<strong>strong</strong>"));
}

/// The std .hom: at-html => at-markdown renames em/strong and
/// dissolves containers, span, and division.
#[test]
fn std_hom_unwraps_and_renames() {
    let tmp = tmp_dir("morph-std-hom");
    let html = "<h2>Part</h2>\n\
        <p>Take <em>this</em> with <span class=\"gloss\">a gloss</span>.</p>\n\
        <ul>\n<li>one</li>\n<li>two</li>\n</ul>\n\
        <div class=\"note\">\n<p>Boxed.</p>\n</div>\n";
    let doc = endo::html_to_document(html).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let doc = kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document;

    let m = morph::resolve_morph(&tmp, "at-html", "at-markdown").unwrap();
    let out = morph::apply(&doc, &m).unwrap();
    assert_eq!(
        dendron::serialize(&out),
        "@@@!at-markdown\n\
         \n\
         @##Part##@\n\
         \n\
         Take @*this*@ with a gloss.\n\
         \n\
         @-\n\
         one\n\
         -@\n\
         \n\
         @-\n\
         two\n\
         -@\n\
         \n\
         Boxed.\n"
    );
}

/// Flat markdown content survives the round trip:
/// at-markdown => at-html (derived) => at-markdown (std .hom).
#[test]
fn flat_content_roundtrips() {
    let tmp = tmp_dir("morph-roundtrip");
    let md = "# Title\n\nWith *em* and **strong**.\n\n- item one\n- item two\n";
    let doc = endo::markdown_to_document(md).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let doc = kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document;
    let original = dendron::serialize(&doc);

    let fwd = morph::resolve_morph(&tmp, "at-markdown", "at-html").unwrap();
    let there = morph::apply(&doc, &fwd).unwrap();
    let back = morph::resolve_morph(&tmp, "at-html", "at-markdown").unwrap();
    let again = morph::apply(&there, &back).unwrap();
    assert_eq!(dendron::serialize(&again), original);
}

/// One .iso file runs in both directions; the round trip is the
/// identity.
#[test]
fn iso_runs_both_directions() {
    let tmp = tmp_dir("morph-iso");
    for (id, em) in [("uno", "/"), ("duo", "%")] {
        std::fs::write(
            tmp.join(format!("{id}.lektos")),
            format!(
                "@@@!atrep\n\n\
                 @=== emphasis\n@{em} grammata {em}@\n===@\n\n\
                 @=== note\n@^\ngrammata\n^@\n===@\n"
            ),
        )
        .unwrap();
    }
    std::fs::write(
        tmp.join("uno.duo.iso"),
        "@@@!atrep-iso\n@=uno<=>duo\n\n@:: / %\n",
    )
    .unwrap();
    let doc = kanon_of(
        &tmp,
        "@@@!uno\n\nSome @/styled/@ text@^(n1).\n\n@^\nA note.\n^@(n1)\n",
    );
    let fwd = morph::resolve_morph(&tmp, "uno", "duo").unwrap();
    let there = morph::apply(&doc, &fwd).unwrap();
    let s = dendron::serialize(&there);
    assert!(s.starts_with("@@@!duo\n"));
    assert!(s.contains("@%styled%@"));
    // The note sim maps implicitly; its deixis follows.
    assert!(s.contains("@^(o1)"));

    // Reverse direction resolves from the same file.
    let back = morph::resolve_morph(&tmp, "duo", "uno").unwrap();
    let again = morph::apply(&there, &back).unwrap();
    assert_eq!(dendron::serialize(&again), dendron::serialize(&doc));
}

#[test]
fn morph_errors() {
    let tmp = tmp_dir("morph-errors");
    for id in ["aaa", "bbb"] {
        std::fs::write(
            tmp.join(format!("{id}.lektos")),
            "@@@!atrep\n\n\
             @=== emphasis\n@/ grammata /@\n===@\n\n\
             @=== extra\n@+ grammata +@\n===@\n",
        )
        .unwrap();
    }
    // Unresolvable: no file, no lineage.
    let err = morph::resolve_morph(&tmp, "aaa", "bbb").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnresolvableMorph(_)));

    // Iso with an unwrap rule.
    std::fs::write(
        tmp.join("aaa.bbb.iso"),
        "@@@!atrep-iso\n@=aaa<=>bbb\n\n@<< /\n",
    )
    .unwrap();
    let err = morph::resolve_morph(&tmp, "aaa", "bbb").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::InvalidMorph(_)));

    // Non-bijective iso: an explicit rename collides with an
    // implicit identity.
    std::fs::write(
        tmp.join("aaa.bbb.iso"),
        "@@@!atrep-iso\n@=aaa<=>bbb\n\n@:: + /\n",
    )
    .unwrap();
    let err = morph::resolve_morph(&tmp, "aaa", "bbb").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::InvalidMorph(_)));

    // Unmapped sim in the document: a hom that maps nothing for
    // `+` while the document uses it.
    std::fs::write(
        tmp.join("ccc.lektos"),
        "@@@!atrep\n\n@=== emphasis\n@/ grammata /@\n===@\n",
    )
    .unwrap();
    std::fs::write(tmp.join("aaa.ccc.hom"), "@@@!atrep-hom\n@=aaa=>ccc\n").unwrap();
    let doc = kanon_of(&tmp, "@@@!aaa\n\nBoth @/kinds/@ and @+more+@.\n");
    let m = morph::resolve_morph(&tmp, "aaa", "ccc").unwrap();
    let err = morph::apply(&doc, &m).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::MorphUnmapped(s) if s == "+"));
}

/// Component handling in renames is direction-of-loss aware:
/// a hom rename into a poorer form drops the unsupported
/// components (recorded loss), while an iso refuses it.
#[test]
fn lossy_rename_drops_components_hom_only() {
    let tmp = tmp_dir("morph-forms");
    std::fs::write(
        tmp.join("src.lektos"),
        "@@@!atrep\n\n@=== box\n@_[(taxis)] [lemma]\ngrammata\n_@\n===@\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("tgt.lektos"),
        "@@@!atrep\n\n@=== box\n@=\ngrammata\n=@\n===@\n",
    )
    .unwrap();
    // src box may carry taxis/lemma; tgt `=` supports neither.
    std::fs::write(
        tmp.join("src.tgt.hom"),
        "@@@!atrep-hom\n@=src=>tgt\n\n@:: _ =\n",
    )
    .unwrap();
    let doc = kanon_of(&tmp, "@@@!src\n\n@_(1) The Label\nInside.\n_@\n");
    let m = morph::resolve_morph(&tmp, "src", "tgt").unwrap();
    let out = morph::apply(&doc, &m).unwrap();
    assert_eq!(dendron::serialize(&out), "@@@!tgt\n\n@=\nInside.\n=@\n");

    // The same shape as an .iso is refused: isos stay strict.
    std::fs::remove_file(tmp.join("src.tgt.hom")).unwrap();
    std::fs::write(
        tmp.join("src.tgt.iso"),
        "@@@!atrep-iso\n@=src<=>tgt\n\n@:: _ =\n",
    )
    .unwrap();
    let err = morph::resolve_morph(&tmp, "src", "tgt").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::InvalidMorph(_)));
}

/// Named variants: several morphisms for one (source, target)
/// pair, selected explicitly; the default stays untouched.
#[test]
fn named_variant_is_a_second_representation() {
    let tmp = tmp_dir("morph-variant");
    for id in ["vsrc", "vtgt"] {
        std::fs::write(
            tmp.join(format!("{id}.lektos")),
            "@@@!atrep\n\n\
             @=== emphasis\n@/ grammata /@\n===@\n\n\
             @=== span\n@, grammata ,@\n===@\n",
        )
        .unwrap();
    }
    std::fs::write(
        tmp.join("vsrc.vtgt.hom"),
        "@@@!atrep-hom\n@=vsrc=>vtgt\n\n@:: / , .kept\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("vsrc.vtgt.plain.hom"),
        "@@@!atrep-hom\n@=vsrc=>vtgt\n\n@<< /\n",
    )
    .unwrap();
    let doc = kanon_of(&tmp, "@@@!vsrc\n\nSome @/styled/@ text.\n");

    let default = morph::resolve_morph(&tmp, "vsrc", "vtgt").unwrap();
    let out = morph::apply(&doc, &default).unwrap();
    assert!(dendron::serialize(&out).contains("@,styled,@.kept"));

    let plain = morph::resolve_morph_variant(&tmp, "vsrc", "vtgt", Some("plain")).unwrap();
    let out = morph::apply(&doc, &plain).unwrap();
    assert!(dendron::serialize(&out).contains("Some styled text."));

    // An unknown variant never falls back to the default.
    let err = morph::resolve_morph_variant(&tmp, "vsrc", "vtgt", Some("nope")).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::UnresolvableMorph(_)));
}

/// Composition closes over the action algebra: the derived
/// cases from the spec's closure table, verified by fused ==
/// staged over documents that exercise them.
#[test]
fn composition_closure_cases() {
    let tmp = tmp_dir("morph-compose-closure");
    // aa: para `box` with lemma; endos `em`, `hd`.
    std::fs::write(
        tmp.join("aa.lektos"),
        "@@@!atrep\n\n\
         @=== box\n@+ [lemma]\ngrammata\n+@\n===@\n\n\
         @=== emphasis\n@/ grammata /@\n===@\n",
    )
    .unwrap();
    // bb: heading endo `!`, emphasis `/`.
    std::fs::write(
        tmp.join("bb.lektos"),
        "@@@!atrep\n\n\
         @=== heading\n@! grammata !@\n===@\n\n\
         @=== emphasis\n@/ grammata /@\n===@\n",
    )
    .unwrap();
    // cc: emphasis only (bb's heading unwraps into it).
    std::fs::write(
        tmp.join("cc.lektos"),
        "@@@!atrep\n\n@=== emphasis\n@/ grammata /@\n===@\n",
    )
    .unwrap();
    // aa => bb extracts the box lemma into the heading.
    std::fs::write(
        tmp.join("aa.bb.hom"),
        "@@@!atrep-hom\n@=aa=>bb\n\n@<# + ! .boxed\n",
    )
    .unwrap();
    // bb => cc unwraps the heading.
    std::fs::write(tmp.join("bb.cc.hom"), "@@@!atrep-hom\n@=bb=>cc\n\n@<< !\n").unwrap();

    let doc = kanon_of(&tmp, "@@@!aa\n\n@+ The Title\nWith @/style/@ inside.\n+@\n");
    let f = morph::resolve_morph(&tmp, "aa", "bb").unwrap();
    let g = morph::resolve_morph(&tmp, "bb", "cc").unwrap();

    // Extract-then-unwrap-heading composes to the heading-less
    // extract: the lemma survives as a plain paragraph.
    let fg = morph::compose(&f, &g).unwrap();
    let fused = morph::apply(&doc, &fg).unwrap();
    let staged = morph::apply_route_staged(&doc, &[f.clone(), g.clone()]).unwrap();
    assert_eq!(dendron::serialize(&fused), dendron::serialize(&staged));
    assert!(dendron::serialize(&fused).contains("The Title\n\nWith @/style/@ inside."));

    // The composite serializes to the closure-completing rule
    // and the file round-trips through resolution.
    let hom = morph::serialize_hom(&fg);
    assert!(hom.contains("@<# +\n"), "normal form was:\n{hom}");
    std::fs::write(tmp.join("aa.cc.hom"), &hom).unwrap();
    let reloaded = morph::resolve_morph(&tmp, "aa", "cc").unwrap();
    assert_eq!(
        dendron::serialize(&morph::apply(&doc, &reloaded).unwrap()),
        dendron::serialize(&fused)
    );

    // Extract-then-drop-heading composes to unwrap: the lemma
    // vanishes, the content splices.
    std::fs::write(tmp.join("bb.cc.hom"), "@@@!atrep-hom\n@=bb=>cc\n\n@-- !\n").unwrap();
    std::fs::remove_file(tmp.join("aa.cc.hom")).unwrap();
    let g = morph::resolve_morph(&tmp, "bb", "cc").unwrap();
    let fg = morph::compose(&f, &g).unwrap();
    assert!(morph::serialize_hom(&fg).contains("@<< +\n"));
    let fused = morph::apply(&doc, &fg).unwrap();
    assert!(!dendron::serialize(&fused).contains("The Title"));
    assert!(dendron::serialize(&fused).contains("With @/style/@ inside."));
}

/// Fused and staged application agree over the std graph, and
/// the composite of a transitive route serializes, reloads, and
/// resolves as a direct edge.
#[test]
fn fused_route_matches_staged_on_std_graph() {
    let tmp = tmp_dir("morph-compose-std");
    let rst = "Title\n=====\n\nWith *em* and **strong**.\n\n.. note::\n\n   Boxed.\n\nterm\n   its definition\n";
    let doc = endo::rst_to_document(rst).unwrap();
    std::fs::write(tmp.join("doc.atd"), dendron::serialize(&doc)).unwrap();
    let doc = kanonizo::kanonizo_file(&tmp.join("doc.atd"))
        .unwrap()
        .document;

    for (from, to) in [("at-rst", "at-markdown"), ("at-markdown", "at-rst")] {
        let route = morph::resolve_route(&tmp, from, to).unwrap();
        if from == "at-rst" {
            let staged = morph::apply_route_staged(&doc, &route).unwrap();
            let fused = morph::apply_route(&doc, &route).unwrap();
            assert_eq!(dendron::serialize(&fused), dendron::serialize(&staged));
        }
        // Either direction: the fold composes.
        let mut fused = route[0].clone();
        for next in &route[1..] {
            fused = morph::compose(&fused, next).unwrap();
        }
        assert_eq!((fused.source.as_str(), fused.target.as_str()), (from, to));
        // The composite is a first-class artifact: it reloads as
        // the pair's direct morphism.
        std::fs::write(
            tmp.join(format!("{from}.{to}.hom")),
            morph::serialize_hom(&fused),
        )
        .unwrap();
        let direct = morph::resolve_morph(&tmp, from, to).unwrap();
        if from == "at-rst" {
            assert_eq!(
                dendron::serialize(&morph::apply(&doc, &direct).unwrap()),
                dendron::serialize(&morph::apply_route_staged(&doc, &route).unwrap())
            );
        }
    }
}

/// Normal forms decide morphism equality: two routes through a
/// commuting diamond compose to byte-equal .hom files.
#[test]
fn coherent_diamond_composes_equal() {
    let tmp = tmp_dir("morph-compose-diamond");
    for id in ["dd", "ee", "ff", "gg"] {
        std::fs::write(
            tmp.join(format!("{id}.lektos")),
            "@@@!atrep\n\n\
             @=== emphasis\n@/ grammata /@\n===@\n\n\
             @=== strong\n@* grammata *@\n===@\n",
        )
        .unwrap();
    }
    // Both paths rename `*` the same way end to end.
    for (a, b, rule) in [
        ("dd", "ee", "@:: * /"),
        ("ee", "gg", ""),
        ("dd", "ff", ""),
        ("ff", "gg", "@:: * /"),
    ] {
        std::fs::write(
            tmp.join(format!("{a}.{b}.hom")),
            format!("@@@!atrep-hom\n@={a}=>{b}\n\n{rule}\n"),
        )
        .unwrap();
    }
    let via = |mid: &str| {
        let f = morph::resolve_morph(&tmp, "dd", mid).unwrap();
        let g = morph::resolve_morph(&tmp, mid, "gg").unwrap();
        morph::serialize_hom(&morph::compose(&f, &g).unwrap())
    };
    let upper = via("ee");
    let lower = via("ff");
    assert_eq!(upper, lower, "diamond does not commute");
    // A deliberately different edge is detectably different.
    std::fs::write(tmp.join("ff.gg.hom"), "@@@!atrep-hom\n@=ff=>gg\n\n@-- *\n").unwrap();
    assert_ne!(upper, via("ff"));
}

/// The embedding test: a round trip that recovers every sim is
/// the identity; one that loses a sim is not.
#[test]
fn round_trip_identity_is_decidable() {
    let tmp = tmp_dir("morph-identity");
    // small: emphasis only. big: emphasis + strong.
    std::fs::write(
        tmp.join("small.lektos"),
        "@@@!atrep\n\n@=== emphasis\n@/ grammata /@\n===@\n",
    )
    .unwrap();
    std::fs::write(
        tmp.join("big.lektos"),
        "@@@!atrep\n\n\
         @=== emphasis\n@/ grammata /@\n===@\n\n\
         @=== strong\n@* grammata *@\n===@\n",
    )
    .unwrap();
    // small embeds in big (implicit identity both ways).
    std::fs::write(tmp.join("small.big.hom"), "@@@!atrep-hom\n@=small=>big\n").unwrap();
    std::fs::write(
        tmp.join("big.small.hom"),
        "@@@!atrep-hom\n@=big=>small\n\n@<< *\n",
    )
    .unwrap();

    let there = morph::resolve_morph(&tmp, "small", "big").unwrap();
    let back = morph::resolve_morph(&tmp, "big", "small").unwrap();
    assert!(morph::compose(&there, &back).unwrap().is_identity());
    // big does not embed in small: strong dissolves on the way.
    assert!(!morph::compose(&back, &there).unwrap().is_identity());
}
