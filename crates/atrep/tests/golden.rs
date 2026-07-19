//! Golden test: the spec's worked-example appendix ("From Deltos to
//! Litos"), with the remote media URL replaced by a local file
//! (remote fetching is outside the pilot's scope).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use atrep::kanonizo;
use atrep::litosis;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const EXPECTED_KANON: &str = "\
@@@!exempli

@# Introduction
Atrep documents keep meaning and presentation apart. \
See @^(o1) for the diagram (as sent from me\\@example.com).
#@.overview

@=(1) Pipeline
@@@@(media/m1.svg)
@:The processing pipeline.:@
=@(o1).diagram

@@@\"
parse -> validate -> kanonize
\"@@@
";

#[test]
fn worked_example_kanon() {
    let result = kanonizo::kanonizo_file(&fixtures().join("intro.atd")).unwrap();
    assert_eq!(result.kanon, EXPECTED_KANON);

    // Media bundling: one file, canonical name, correct checksum.
    assert_eq!(result.media.len(), 1);
    let media = &result.media[0];
    assert_eq!(media.name, "m1.svg");
    assert_eq!(media.source, "pipeline.svg");
    let svg = std::fs::read(fixtures().join("pipeline.svg")).unwrap();
    assert_eq!(media.sha256, hex(&Sha256::digest(&svg)));
}

#[test]
fn worked_example_litos() {
    let result = kanonizo::kanonizo_file(&fixtures().join("intro.atd")).unwrap();
    let svg = std::fs::read(fixtures().join("pipeline.svg")).unwrap();
    let svg_sha = hex(&Sha256::digest(&svg));

    // Resolve media from the kanonizo result, as `atrep litos`
    // would resolve it from the extracted archive.
    let lookup = |param: &str| -> atrep::Result<Vec<u8>> {
        assert_eq!(param, "media/m1.svg");
        Ok(svg.clone())
    };
    let litos = litosis::litosis(&result.document, &lookup).unwrap();

    let expected = format!(
        "\
@@@!exempli

@# Introduction
Atrep documents keep meaning and presentation apart. \
See for the diagram (as sent from me\\@example.com).
#@

@=(1) Pipeline
@@@@[SHA256:{svg_sha}]
@:The processing pipeline.:@
=@

@@@\"
parse -> validate -> kanonize
\"@@@
"
    );
    assert_eq!(litos.litos, expected);
    assert_eq!(litos.litos_id, hex(&Sha256::digest(expected.as_bytes())));
}

/// Kanonizo is idempotent: canonicalizing a kanon reproduces it.
#[test]
fn kanonizo_idempotent() {
    let result = kanonizo::kanonizo_file(&fixtures().join("intro.atd")).unwrap();

    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("idempotency");
    std::fs::create_dir_all(tmp.join("media")).unwrap();
    std::fs::copy(
        fixtures().join("exempli.lektos"),
        tmp.join("exempli.lektos"),
    )
    .unwrap();
    std::fs::copy(fixtures().join("pipeline.svg"), tmp.join("media/m1.svg")).unwrap();
    std::fs::write(tmp.join("intro.atk"), &result.kanon).unwrap();

    let again = kanonizo::kanonizo_file(&tmp.join("intro.atk")).unwrap();
    // The second pass renames media/m1.svg to media/m1.svg again.
    assert_eq!(again.kanon, result.kanon);
}
