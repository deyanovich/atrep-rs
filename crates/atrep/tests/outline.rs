//! The structural outline: block spans recorded by the parser,
//! points located post-parse (litogramma-vim's data source).

use std::path::Path;

use atrep::{outline, parser};

const DOC: &str = "@@@!litogramma\n\n\
    @# The Charges\n\
    @(\"steph:17a\")How you have been affected@^!(n1).\n\n\
    @^!\nThe famous opening.\n!^@(n1)\n\n\
    @## First Accusers\nMore prose here.\n##@\n\
    #@\n\n\
    @# The Defence\n@(\"steph:18a\")From the beginning.\n#@\n";

#[test]
fn outline_records_blocks_and_points() {
    let (doc, blocks, dial) = parser::parse_document_outline(DOC, Path::new("t.atd")).unwrap();
    let o = outline::assemble(&doc, blocks, DOC, &dial);
    assert_eq!(o.dialektos, "litogramma");

    let names: Vec<_> = o
        .blocks
        .iter()
        .map(|b| (b.name.as_deref().unwrap_or("?"), b.depth, b.start, b.end))
        .collect();
    assert_eq!(
        names,
        vec![
            ("section", 0, 3, 13),
            ("manuscript-note", 1, 6, 8),
            ("subsection", 1, 10, 12),
            ("section", 0, 15, 17),
        ]
    );
    assert_eq!(o.blocks[0].lemma, "The Charges");

    let ms: Vec<_> = o
        .milestones
        .iter()
        .map(|m| (m.key.as_str(), m.line))
        .collect();
    assert_eq!(ms, vec![("steph:17a", 4), ("steph:18a", 16)]);
    // The onym points at its declaration (the episim line), not
    // the deixis that references it.
    assert_eq!(o.onyms[0].key, "n1");
    assert_eq!(o.onyms[0].line, 8);
    assert_eq!(o.deixes[0].key, "^!(n1)");
    assert_eq!(o.deixes[0].line, 4);
}
