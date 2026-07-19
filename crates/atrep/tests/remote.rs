//! Remote transclusion and media tests. Everything is served from
//! an ephemeral 127.0.0.1 listener; no test touches the network.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use atrep::error::ErrorKind;
use atrep::kanonizo;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Fresh per-test scratch directory with the `exempli` dialektos.
fn tmp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        fixtures().join("exempli.lektos"),
        dir.join("exempli.lektos"),
    )
    .unwrap();
    dir
}

struct Route {
    path: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

/// Serve the built routes (404 for unknown paths) for up to
/// `max_requests` requests on a fresh localhost port; returns the
/// base URL, which is also passed to the route builder so bodies
/// can reference the server itself.
fn serve(build: impl FnOnce(&str) -> Vec<Route>, max_requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let base = format!("http://127.0.0.1:{port}");
    let routes = build(&base);
    std::thread::spawn(move || {
        for _ in 0..max_requests {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut req = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        req.extend_from_slice(&buf[..n]);
                        if req.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
            let req = String::from_utf8_lossy(&req);
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            match routes.iter().find(|r| r.path == path) {
                Some(r) => {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n",
                        r.content_type,
                        r.body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&r.body);
                }
                None => {
                    let _ = stream.write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\
                          Connection: close\r\n\r\n",
                    );
                }
            }
        }
    });
    base
}

#[test]
fn remote_media_fetched_and_bundled() {
    let svg = std::fs::read(fixtures().join("pipeline.svg")).unwrap();
    let svg_body = svg.clone();
    let base = serve(
        |_| {
            vec![Route {
                path: "/pipeline.svg",
                content_type: "image/svg+xml",
                body: svg_body,
            }]
        },
        1,
    );
    let tmp = tmp_dir("remote-media");
    std::fs::write(
        tmp.join("doc.atd"),
        format!("@@@!exempli\n\n@@@@({base}/pipeline.svg)\n"),
    )
    .unwrap();

    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(result.kanon, "@@@!exempli\n\n@@@@(media/m1.svg)\n");
    assert_eq!(result.media.len(), 1);
    let m = &result.media[0];
    assert_eq!(m.name, "m1.svg");
    assert_eq!(m.source, format!("{base}/pipeline.svg"));
    assert_eq!(m.bytes, svg);

    // The archive carries the kanon, the manifest (with the source
    // URL and checksum), and the fetched bytes.
    kanonizo::write_outputs(&result, &tmp.join("doc.atk")).unwrap();
    let file = std::fs::File::open(tmp.join("doc.atk.tar.gz")).unwrap();
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().display().to_string();
        let mut content = Vec::new();
        entry.read_to_end(&mut content).unwrap();
        entries.push((name, content));
    }
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["doc.atk", "media-manifest.json", "media/m1.svg"]);
    assert_eq!(entries[2].1, svg);
    let manifest = String::from_utf8(entries[1].1.clone()).unwrap();
    assert!(manifest.contains(&format!("{base}/pipeline.svg")));
    assert!(manifest.contains(&m.sha256));
}

#[test]
fn remote_media_extension_from_content_type() {
    let base = serve(
        |_| {
            vec![Route {
                path: "/image",
                content_type: "image/png",
                body: b"opaque body without magic".to_vec(),
            }]
        },
        1,
    );
    let tmp = tmp_dir("remote-media-ct");
    std::fs::write(
        tmp.join("doc.atd"),
        format!("@@@!exempli\n\n@@@@({base}/image)\n"),
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(result.media[0].name, "m1.png");
}

#[test]
fn remote_media_extension_from_magic_bytes() {
    let mut body = b"\x89PNG\r\n\x1a\n".to_vec();
    body.extend_from_slice(b"payload");
    let base = serve(
        |_| {
            vec![Route {
                path: "/blob",
                content_type: "application/octet-stream",
                body,
            }]
        },
        1,
    );
    let tmp = tmp_dir("remote-media-magic");
    std::fs::write(
        tmp.join("doc.atd"),
        format!("@@@!exempli\n\n@@@@({base}/blob)\n"),
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(result.media[0].name, "m1.png");
}

#[test]
fn remote_media_failure_is_error() {
    // Serve enough 404s to exhaust the default retries (1 + 2).
    let base = serve(|_| Vec::new(), 3);
    let tmp = tmp_dir("remote-media-404");
    std::fs::write(
        tmp.join("doc.atd"),
        format!("@@@!exempli\n\n@@@@({base}/missing.svg)\n"),
    )
    .unwrap();
    let err = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::MissingResource(_)));
}

#[test]
fn remote_anaphor_englossis_expands() {
    // The remote document's `exempli` declaration resolves against
    // the including document's directory (pilot decision).
    let base = serve(
        |_| {
            vec![Route {
                path: "/part.atd",
                content_type: "text/plain",
                body: b"@@@!exempli\n\nremote paragraph\n".to_vec(),
            }]
        },
        1,
    );
    let tmp = tmp_dir("remote-englossis");
    std::fs::write(
        tmp.join("doc.atd"),
        format!("@@@!exempli\n\n@@@({base}/part.atd)\n"),
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(
        result.kanon,
        "@@@!exempli\n\n@@@!(exempli)\nremote paragraph\n!@@@\n"
    );
}

#[test]
fn remote_anaphor_enlexis_verbatim() {
    let base = serve(
        |_| {
            vec![Route {
                path: "/steps.txt",
                content_type: "text/plain",
                body: b"one\ntwo".to_vec(),
            }]
        },
        1,
    );
    let tmp = tmp_dir("remote-enlexis");
    std::fs::write(
        tmp.join("doc.atd"),
        format!("@@@!exempli\n\n@@@+({base}/steps.txt)\n"),
    )
    .unwrap();
    let result = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap();
    assert_eq!(result.kanon, "@@@!exempli\n\n@@@\"\none\ntwo\n\"@@@\n");
}

#[test]
fn remote_transclusion_cycle() {
    let base = serve(
        |base| {
            vec![Route {
                path: "/self.atd",
                content_type: "text/plain",
                body: format!("@@@!exempli\n\n@@@({base}/self.atd)\n").into_bytes(),
            }]
        },
        1,
    );
    let tmp = tmp_dir("remote-cycle");
    std::fs::write(
        tmp.join("doc.atd"),
        format!("@@@!exempli\n\n@@@({base}/self.atd)\n"),
    )
    .unwrap();
    let err = kanonizo::kanonizo_file(&tmp.join("doc.atd")).unwrap_err();
    assert!(matches!(err.kind, ErrorKind::TransclusionCycle(_)));
}
