//! Regression tests from the code review of the CLI and the
//! language server: both binaries are driven as processes.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const DOC: &str = "@@@!litogramma\n\n\
    @# The Charges\n\
    @(\"steph:17a\")How you have been affected@^!(n1).\n\n\
    @^!\nThe famous opening.\n!^@(n1)\n\n\
    @## First Accusers\nMore prose here.\n##@\n\
    #@\n\n\
    @# The Defence\n@(\"steph:18a\")From the beginning.\n#@\n";

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("atrep-review-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn atrep() -> Command {
    Command::new(env!("CARGO_BIN_EXE_atrep"))
}

/// Run the CLI; (exit ok, stdout, stderr).
fn run(args: &[&str], cwd: &Path) -> (bool, String, String) {
    let out = atrep().args(args).current_dir(cwd).output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ------------------------------------------------------- LSP

fn frame(m: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(m).unwrap();
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend(body);
    out
}

/// Pipe the messages into atrep-lsp (after initialize) and return
/// every message the server emitted plus whether it exited cleanly.
fn lsp(msgs: &[Value]) -> (Vec<Value>, bool) {
    let mut payload = frame(&json!({
        "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {"processId": null, "rootUri": null, "capabilities": {}}
    }));
    payload.extend(frame(
        &json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    ));
    for m in msgs {
        payload.extend(frame(m));
    }
    payload.extend(frame(
        &json!({"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": null}),
    ));
    payload.extend(frame(
        &json!({"jsonrpc": "2.0", "method": "exit", "params": null}),
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_atrep-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&payload).unwrap();
    let out = child.wait_with_output().unwrap();
    let bytes = out.stdout;
    let mut msgs = Vec::new();
    let mut i = 0;
    while let Some(h) = bytes[i..].windows(4).position(|w| w == b"\r\n\r\n") {
        let header = String::from_utf8_lossy(&bytes[i..i + h]).into_owned();
        let len: usize = header
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let start = i + h + 4;
        msgs.push(serde_json::from_slice(&bytes[start..start + len]).unwrap());
        i = start + len;
    }
    (msgs, out.status.success())
}

fn response(msgs: &[Value], id: i64) -> &Value {
    msgs.iter()
        .find(|m| m["id"] == json!(id))
        .unwrap_or_else(|| panic!("no response {id} in {msgs:?}"))
}

fn did_open(uri: &str, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": {"textDocument": {"uri": uri, "languageId": "atrep", "version": 1, "text": text}}
    })
}

#[test]
fn lsp_answers_malformed_request_with_invalid_params_and_lives_on() {
    let uri = "file:///srv/texts/apology.atd";
    let (msgs, ok) = lsp(&[
        did_open(uri, DOC),
        json!({"jsonrpc": "2.0", "id": 1, "method": "textDocument/hover", "params": {}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "textDocument/documentSymbol",
               "params": {"textDocument": {"uri": uri}}}),
    ]);
    assert!(ok, "server must exit cleanly after shutdown/exit");
    let err = response(&msgs, 1);
    assert_eq!(err["error"]["code"], json!(-32602), "{err}");
    let syms = response(&msgs, 2);
    assert_eq!(
        syms["result"][0]["name"],
        json!("section The Charges"),
        "{syms}"
    );
    assert!(response(&msgs, 99)["result"].is_null());
}

#[test]
fn lsp_ignores_malformed_notification() {
    let uri = "file:///srv/texts/apology.atd";
    let (msgs, ok) = lsp(&[
        json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"bogus": 1}}),
        did_open(uri, DOC),
        json!({"jsonrpc": "2.0", "id": 2, "method": "textDocument/foldingRange",
               "params": {"textDocument": {"uri": uri}}}),
    ]);
    assert!(ok);
    let folds = response(&msgs, 2);
    assert_eq!(folds["result"].as_array().unwrap().len(), 4, "{folds}");
}

// ------------------------------------------------------- outputs

#[test]
fn endo_refuses_an_unknown_extension_and_leaves_the_file_alone() {
    let dir = tmp_dir("endo-atd");
    let victim = dir.join("victim.atd");
    std::fs::write(&victim, DOC).unwrap();
    // Without --output the default output is the input itself.
    let (ok, _, err) = run(&["endo", "victim.atd"], &dir);
    assert!(!ok);
    assert!(
        err.contains("victim.atd") && err.contains("--output"),
        "{err}"
    );
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), DOC);
    // With one, the extension has no importer.
    let (ok, _, err) = run(&["endo", "victim.atd", "-o", "copy.atd"], &dir);
    assert!(!ok);
    assert!(err.contains("victim.atd") && err.contains("`atd`"), "{err}");
    assert!(!dir.join("copy.atd").exists());
}

#[test]
fn kanonizo_refuses_the_default_output_over_its_input() {
    let dir = tmp_dir("kanonizo-atk");
    let kanon = dir.join("k.atk");
    std::fs::write(&kanon, DOC).unwrap();
    let (ok, _, err) = run(&["kanonizo", "k.atk"], &dir);
    assert!(!ok);
    assert!(err.contains("k.atk") && err.contains("--output"), "{err}");
    assert_eq!(std::fs::read_to_string(&kanon).unwrap(), DOC);
    // An explicit output elsewhere is fine.
    let (ok, out, err) = run(&["kanonizo", "k.atk", "-o", "k2.atk"], &dir);
    assert!(ok, "{err}");
    assert!(out.contains("k2.atk"));
}

#[test]
fn io_errors_name_their_path() {
    let dir = tmp_dir("io-paths");
    let (ok, _, err) = run(&["kanonizo", "nonexist.atd"], &dir);
    assert!(!ok);
    assert!(err.contains("nonexist.atd"), "{err}");
    std::fs::write(dir.join("doc.atd"), DOC).unwrap();
    let (ok, _, err) = run(&["kanonizo", "doc.atd", "-o", "no-such-dir/doc.atk"], &dir);
    assert!(!ok);
    assert!(err.contains("no-such-dir/doc.atk"), "{err}");
}

// ---------------------------------------------------- quasialign

/// A witness with one coordinate whose segment overflows a
/// 30-character ceiling, and a comment the rewrite cannot keep.
const WITNESS: &str = "@@@!litogramma\n\n@@/ scribal note /@@\n@# Title\n\
    @(\"steph:17a\")One two three four five six seven eight nine ten. \
    Eleven twelve.\n#@\n";

#[cfg(unix)]
#[test]
fn quasialign_failure_leaves_every_file_untouched() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmp_dir("quasialign-fail");
    let ro = dir.join("ro");
    std::fs::create_dir(&ro).unwrap();
    std::fs::write(dir.join("w1.atd"), WITNESS).unwrap();
    std::fs::write(ro.join("w2.atd"), WITNESS).unwrap();
    // A read-only directory refuses the staged temp file.
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
    let (ok, _, err) = run(
        &["quasialign", "w1.atd", "ro/w2.atd", "--max-segment", "30"],
        &dir,
    );
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!ok);
    assert!(err.contains("ro/w2.atd"), "{err}");
    // The first file was not rewritten, and no temp file remains.
    assert_eq!(
        std::fs::read_to_string(dir.join("w1.atd")).unwrap(),
        WITNESS
    );
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!names.iter().any(|n| n.ends_with(".tmp")), "{names:?}");
}

#[test]
fn quasialign_output_dir_keeps_the_sources_and_warns_only_in_place() {
    let dir = tmp_dir("quasialign-outdir");
    std::fs::write(dir.join("w1.atd"), WITNESS).unwrap();
    let (ok, out, err) = run(
        &["quasialign", "w1.atd", "--max-segment", "30", "-o", "cut"],
        &dir,
    );
    assert!(ok, "{err}");
    assert!(
        out.contains("cut/w1.atd: 2 quasi-milestone(s) inserted"),
        "{out}"
    );
    assert!(!err.contains("comments"), "{err}");
    assert_eq!(
        std::fs::read_to_string(dir.join("w1.atd")).unwrap(),
        WITNESS
    );
    let cut = std::fs::read_to_string(dir.join("cut/w1.atd")).unwrap();
    assert!(cut.contains("steph:17a|1"), "{cut}");
    // In place, the comment is lost and the loss is announced.
    let (ok, _, err) = run(&["quasialign", "w1.atd", "--max-segment", "30"], &dir);
    assert!(ok, "{err}");
    assert!(err.contains("w1.atd") && err.contains("comments"), "{err}");
    assert!(
        !std::fs::read_to_string(dir.join("w1.atd"))
            .unwrap()
            .contains("scribal")
    );
}

// ------------------------------------------- symbols, columns

#[test]
fn lsp_document_symbols_stay_fast_on_a_large_document() {
    // 1500 sections, each with a milestone and an onym-bearing
    // note: 3000 blocks. Recomputing the standalone onyms per
    // block made this take over half a minute.
    let mut text = String::from("@@@!litogramma\n");
    for i in 0..1500 {
        text.push_str(&format!(
            "\n@# Section {i}\n@(\"steph:{i}a\")Prose of section {i}@^!(n{i}).\n\
             @^!\nnote {i}\n!^@(n{i})\n#@\n"
        ));
    }
    let uri = "file:///srv/texts/long.atd";
    let started = std::time::Instant::now();
    let (msgs, ok) = lsp(&[
        did_open(uri, &text),
        json!({"jsonrpc": "2.0", "id": 1, "method": "textDocument/documentSymbol",
               "params": {"textDocument": {"uri": uri}}}),
    ]);
    let elapsed = started.elapsed();
    assert!(ok);
    assert_eq!(response(&msgs, 1)["result"].as_array().unwrap().len(), 1500);
    assert!(
        elapsed < std::time::Duration::from_secs(15),
        "documentSymbol took {elapsed:?}"
    );
}

#[test]
fn lsp_diagnostic_column_counts_the_buffer_not_its_nfc_form() {
    // Four decomposed e + U+0301 precede the undefined sim: the
    // parser sees four characters there, the buffer holds eight.
    let line = "e\u{301}e\u{301}e\u{301}e\u{301} @zz(x) tail";
    let text = format!("@@@!litogramma\n\n{line}\n");
    let (msgs, ok) = lsp(&[did_open("file:///srv/texts/accents.atd", &text)]);
    assert!(ok);
    let diag = msgs
        .iter()
        .find(|m| m["method"] == json!("textDocument/publishDiagnostics"))
        .unwrap();
    let start = &diag["params"]["diagnostics"][0]["range"]["start"];
    assert_eq!(start["line"], json!(2), "{diag}");
    let column = line[..line.find('@').unwrap()].encode_utf16().count();
    assert_eq!(column, 9);
    assert_eq!(start["character"], json!(column), "{diag}");
}

#[test]
fn outline_and_structure_parse_the_nfc_form_as_check_does() {
    // A decomposed onym (n + U+0301) is valid only once composed.
    let text = "@@@!litogramma\n\n@# Title\nSee@^!(n\u{301}).\n\
        @^!\nnote\n!^@(n\u{301})\n#@\n";
    let dir = tmp_dir("nfc-outline");
    std::fs::write(dir.join("d.atd"), text).unwrap();
    let (ok, _, err) = run(&["check", "d.atd"], &dir);
    assert!(ok, "{err}");
    let (ok, out, err) = run(&["outline", "d.atd"], &dir);
    assert!(ok, "{err}");
    assert!(out.contains("key=\u{144}"), "{out}");
    let uri = "file:///srv/texts/d.atd";
    let (msgs, ok) = lsp(&[
        did_open(uri, text),
        json!({"jsonrpc": "2.0", "id": 1, "method": "textDocument/foldingRange",
               "params": {"textDocument": {"uri": uri}}}),
    ]);
    assert!(ok);
    assert!(response(&msgs, 1)["result"].is_array(), "{msgs:?}");
}

// --------------------------------------------------------- fetch

/// A local server answering every request with `status`; returns
/// its base URL and the count of requests served.
fn serve_status(status: &'static str) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::io::Read;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let served = std::sync::Arc::new(AtomicUsize::new(0));
    let count = served.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            while !req.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => req.extend_from_slice(&buf[..n]),
                }
            }
            count.fetch_add(1, Ordering::SeqCst);
            let _ = stream.write_all(
                format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            );
        }
    });
    (base, served)
}

#[test]
fn a_remote_404_is_asked_once_and_a_503_is_retried() {
    use std::sync::atomic::Ordering;
    let dir = tmp_dir("fetch-retry");
    let (base, served) = serve_status("404 Not Found");
    std::fs::write(
        dir.join("doc.atd"),
        format!("@@@!litogramma\n\n@@@@({base}/missing.svg)\n"),
    )
    .unwrap();
    let (ok, _, err) = run(&["kanonizo", "doc.atd", "--retries", "2"], &dir);
    assert!(!ok);
    assert!(err.contains("missing.svg"), "{err}");
    assert_eq!(served.load(Ordering::SeqCst), 1, "a 404 is final");

    let (base, served) = serve_status("503 Service Unavailable");
    std::fs::write(
        dir.join("doc.atd"),
        format!("@@@!litogramma\n\n@@@@({base}/busy.svg)\n"),
    )
    .unwrap();
    let (ok, _, err) = run(&["kanonizo", "doc.atd", "--retries", "2"], &dir);
    assert!(!ok);
    assert!(err.contains("busy.svg"), "{err}");
    assert_eq!(served.load(Ordering::SeqCst), 3, "a 503 is retried");
}

// ----------------------------------------------------------- fb2

#[test]
fn endo_refuses_an_fb2_binary_id_that_is_not_a_file_name() {
    // "iVBORw0KGgo=" is the eight-byte PNG signature.
    let book = |id: &str| {
        format!(
            r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description><title-info><book-title>The Odyssey</book-title></title-info></description>
<body>
<title><p>The Odyssey</p></title>
<section><title><p>Book I</p></title><p>Tell me, O Muse.</p></section>
</body>
<binary id="{id}" content-type="image/png">iVBORw0KGgo=</binary>
</FictionBook>
"##
        )
    };
    let dir = tmp_dir("fb2-id");
    // A drive-qualified id is refused before anything is written.
    std::fs::write(dir.join("book.fb2"), book("C:cover.png")).unwrap();
    let (ok, _, err) = run(&["endo", "book.fb2"], &dir);
    assert!(!ok);
    assert!(err.contains("C:cover.png"), "{err}");
    assert!(!dir.join("book.atd").exists());
    assert!(!dir.join("media").exists());
    // Separators are flattened by the importer, so a climbing id
    // lands as one file name inside media/ and nowhere else.
    std::fs::write(dir.join("book.fb2"), book(r"..\..\cover.png")).unwrap();
    let (ok, _, err) = run(&["endo", "book.fb2"], &dir);
    assert!(ok, "{err}");
    assert!(dir.join("media/..-..-cover.png").exists());
    assert!(!dir.parent().unwrap().join("cover.png").exists());
    std::fs::remove_file(dir.join("book.atd")).unwrap();
    std::fs::remove_dir_all(dir.join("media")).unwrap();
    // A plain id still lands in media/.
    std::fs::write(dir.join("book.fb2"), book("cover.png")).unwrap();
    let (ok, _, err) = run(&["endo", "book.fb2"], &dir);
    assert!(ok, "{err}");
    assert!(dir.join("media/cover.png").exists());
}

// ------------------------------------- compose, spellable output

#[test]
fn compose_refuses_a_composite_that_is_not_total() {
    // at-markdown's footnote sim has no image in at-html, so the
    // derived embedding is partial. Written as a .hom it would be
    // tried before the embedding and fail every later morph of the
    // pair, so nothing is written.
    let dir = tmp_dir("compose-partial");
    let (ok, _, err) = run(&["compose", "at-markdown", "at-html"], &dir);
    assert!(!ok);
    assert!(err.contains("not total") && err.contains('^'), "{err}");
    assert!(!dir.join("at-markdown.at-html.hom").exists());
    // A total chain still composes.
    let (ok, _, err) = run(&["compose", "at-rst", "at-html", "at-org"], &dir);
    assert!(ok, "{err}");
    assert!(dir.join("at-rst.at-org.hom").exists());
}

#[test]
fn endo_refuses_a_tree_with_no_spelling() {
    // A literal backslash directly before a sim would be written
    // as `\@`, which reads back as an escaped sigil.
    let dir = tmp_dir("endo-backslash");
    std::fs::write(
        dir.join("path.html"),
        "<p>Path C:\\<em>Tom Sawyer</em> end.</p>\n",
    )
    .unwrap();
    let (ok, _, err) = run(&["endo", "path.html"], &dir);
    assert!(!ok);
    assert!(err.contains("backslash"), "{err}");
    assert!(!dir.join("path.atd").exists());
}
