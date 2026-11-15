//! `atrep exo` for a dialektos with no document exomorphosis of
//! its own for the target: the export goes through the dialektos
//! that maps the format, in one command.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A document with no title sim: the book is a level-one section.
const DOC: &str = "@@@!litogramma\n\n\
    @#() The Odyssey\n\n\
    @##() Book I\n\
    Tell me, O Muse, of that ingenious hero.\n\
    ##@\n\n\
    #@\n";

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("atrep-exo-route-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run the CLI; (exit ok, stdout, stderr).
fn run(args: &[&str], cwd: &Path) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_atrep"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Litogramma to TEI in one step, with nothing said of the route
/// (the command reads the same for every format), and the file
/// one the importer takes back.
#[test]
fn litogramma_to_tei_in_one_command() {
    let tmp = tmp_dir("tei");
    std::fs::write(tmp.join("doc.atd"), DOC).unwrap();

    let (ok, stdout, stderr) = run(&["exo", "doc.atd", "tei", "-o", "doc.tei.xml"], &tmp);
    assert!(ok, "{stderr}");
    assert_eq!(stdout.trim(), "doc.tei.xml");
    assert!(stderr.is_empty(), "{stderr}");
    let tei = std::fs::read_to_string(tmp.join("doc.tei.xml")).unwrap();
    assert!(
        tei.contains("<text>\n<body>\n<div type=\"section\" n=\"1\">\n<head>The Odyssey</head>")
    );

    // The same bytes on a second run.
    let (ok, _, stderr) = run(&["exo", "doc.atd", "tei", "-o", "again.tei.xml"], &tmp);
    assert!(ok, "{stderr}");
    assert_eq!(
        std::fs::read(tmp.join("again.tei.xml")).unwrap(),
        tei.as_bytes()
    );

    let (ok, _, stderr) = run(&["endo", "doc.tei.xml", "-o", "back.atd"], &tmp);
    assert!(ok, "{stderr}");
    let back = std::fs::read_to_string(tmp.join("back.atd")).unwrap();
    assert!(back.contains("The Odyssey"), "{back}");
    assert!(
        back.contains("Tell me, O Muse, of that ingenious hero."),
        "{back}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

/// A target the dialektos renders itself is as quiet, and one
/// nothing reaches is the error it always was.
#[test]
fn direct_and_unreachable_targets_are_unchanged() {
    let tmp = tmp_dir("direct");
    std::fs::write(tmp.join("doc.atd"), DOC).unwrap();

    let (ok, _, stderr) = run(&["exo", "doc.atd", "html", "-o", "doc.html"], &tmp);
    assert!(ok, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");

    let (ok, _, stderr) = run(&["exo", "doc.atd", "usfm", "-o", "doc.usfm"], &tmp);
    assert!(!ok);
    assert!(stderr.contains("no exomorphosis rule matches"), "{stderr}");
    let _ = std::fs::remove_dir_all(&tmp);
}
