//! Remote resource fetching for kanonizo (spec: chapter "Kanonizo",
//! "Media Bundling"): HTTP(S) GET with configurable timeout and
//! retry, behind a trait so tests can stub the network. The spec
//! pins the fetch policy down for media only; the pilot applies the
//! same policy to remote transclusion (see README spec gaps).

use std::time::Duration;

use crate::error::{Error, ErrorKind, Result};

/// A fetched resource.
#[derive(Debug)]
pub struct Fetched {
    pub bytes: Vec<u8>,
    /// The raw `Content-Type` header value, if present.
    pub content_type: Option<String>,
}

/// One fetch attempt; non-2xx responses and transport failures are
/// errors.
pub trait Fetcher {
    fn fetch(&self, url: &str) -> Result<Fetched>;
}

/// Network policy for remote fetches.
#[derive(Debug, Clone)]
pub struct FetchConfig {
    pub timeout: Duration,
    /// Additional attempts after the first one fails.
    pub retries: u32,
    pub retry_backoff: Duration,
}

impl Default for FetchConfig {
    fn default() -> Self {
        FetchConfig {
            timeout: Duration::from_secs(30),
            retries: 2,
            retry_backoff: Duration::from_millis(250),
        }
    }
}

/// Fetcher for contexts with no network capability (the `net`
/// feature is off): every remote reference is an error.
pub struct DeniedFetcher;

impl Fetcher for DeniedFetcher {
    fn fetch(&self, url: &str) -> Result<Fetched> {
        Err(Error::new(ErrorKind::MissingResource(format!(
            "{url}: remote fetching is disabled (built without the `net` feature)"
        ))))
    }
}

/// HTTP(S) fetcher backed by ureq.
#[cfg(feature = "net")]
pub struct HttpFetcher {
    agent: ureq::Agent,
}

#[cfg(feature = "net")]
impl HttpFetcher {
    pub fn new(cfg: &FetchConfig) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(cfg.timeout))
            .build();
        HttpFetcher {
            agent: config.new_agent(),
        }
    }
}

#[cfg(feature = "net")]
impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str) -> Result<Fetched> {
        let missing = |e: &dyn std::fmt::Display| {
            Error::new(ErrorKind::MissingResource(format!("{url}: {e}")))
        };
        let mut resp = self.agent.get(url).call().map_err(|e| missing(&e))?;
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = resp
            .body_mut()
            .with_config()
            .limit(1024 * 1024 * 1024) // 1 GiB pilot cap
            .read_to_vec()
            .map_err(|e| missing(&e))?;
        Ok(Fetched {
            bytes,
            content_type,
        })
    }
}

/// Run `f.fetch(url)` under the configured retry policy.
pub fn fetch_with_retry(f: &dyn Fetcher, cfg: &FetchConfig, url: &str) -> Result<Fetched> {
    let mut last = None;
    for attempt in 0..=cfg.retries {
        if attempt > 0 && !cfg.retry_backoff.is_zero() {
            std::thread::sleep(cfg.retry_backoff);
        }
        match f.fetch(url) {
            Ok(fetched) => return Ok(fetched),
            Err(e) => last = Some(e),
        }
    }
    Err(last.expect("at least one attempt"))
}

/// Infer the canonical media extension, in spec order: the source
/// path/URL extension, then the Content-Type, then magic bytes,
/// then `bin`. Lowercased for deterministic canonical names.
pub(crate) fn infer_extension(source: &str, content_type: Option<&str>, bytes: &[u8]) -> String {
    if let Some(ext) = source_extension(source) {
        return ext;
    }
    if let Some(ext) = content_type.and_then(content_type_extension) {
        return ext.to_string();
    }
    if let Some(ext) = magic_extension(bytes) {
        return ext.to_string();
    }
    "bin".to_string()
}

/// Extension of the last path segment, ignoring any URL query or
/// fragment; accepted when 1-8 ASCII alphanumerics.
fn source_extension(source: &str) -> Option<String> {
    let path = source.split(['?', '#']).next().unwrap_or(source);
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    let ok = !stem.is_empty()
        && !ext.is_empty()
        && ext.len() <= 8
        && ext.chars().all(|c| c.is_ascii_alphanumeric());
    ok.then(|| ext.to_ascii_lowercase())
}

fn content_type_extension(ct: &str) -> Option<&'static str> {
    let essence = ct
        .split(';')
        .next()
        .unwrap_or(ct)
        .trim()
        .to_ascii_lowercase();
    Some(match essence.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        _ => return None,
    })
}

fn magic_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some("jpg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("gif");
    }
    if bytes.starts_with(b"%PDF") {
        return Some("pdf");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    let head = &bytes[..bytes.len().min(256)];
    if let Ok(text) = std::str::from_utf8(head) {
        let t = text.trim_start();
        if t.starts_with("<?xml") || t.starts_with("<svg") {
            return Some("svg");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Stub fetcher failing the first `fail` attempts.
    struct Flaky {
        fail: u32,
        calls: Cell<u32>,
    }

    impl Fetcher for Flaky {
        fn fetch(&self, url: &str) -> Result<Fetched> {
            let n = self.calls.get() + 1;
            self.calls.set(n);
            if n <= self.fail {
                Err(Error::new(ErrorKind::MissingResource(format!(
                    "{url}: transient"
                ))))
            } else {
                Ok(Fetched {
                    bytes: b"ok".to_vec(),
                    content_type: None,
                })
            }
        }
    }

    fn cfg(retries: u32) -> FetchConfig {
        FetchConfig {
            timeout: Duration::from_secs(1),
            retries,
            retry_backoff: Duration::ZERO,
        }
    }

    #[test]
    fn retry_succeeds_after_transient_failure() {
        let f = Flaky {
            fail: 1,
            calls: Cell::new(0),
        };
        let fetched = fetch_with_retry(&f, &cfg(1), "http://x/").unwrap();
        assert_eq!(fetched.bytes, b"ok");
        assert_eq!(f.calls.get(), 2);
    }

    #[test]
    fn retry_exhausted_is_error() {
        let f = Flaky {
            fail: 2,
            calls: Cell::new(0),
        };
        let err = fetch_with_retry(&f, &cfg(1), "http://x/").unwrap_err();
        assert!(matches!(err.kind, ErrorKind::MissingResource(_)));
        assert_eq!(f.calls.get(), 2);
    }

    #[test]
    fn infer_extension_table() {
        // Source extension wins, query/fragment stripped, lowercased.
        assert_eq!(infer_extension("http://x/a/pic.SVG?v=1", None, b""), "svg");
        assert_eq!(infer_extension("dir/pic.png", None, b""), "png");
        // Content-Type when the path has no usable extension.
        assert_eq!(
            infer_extension("http://x/image", Some("image/png; charset=binary"), b"x"),
            "png"
        );
        // Magic bytes when neither names a type.
        assert_eq!(
            infer_extension(
                "http://x/blob",
                Some("application/octet-stream"),
                b"%PDF-1.4"
            ),
            "pdf"
        );
        assert_eq!(
            infer_extension("http://x/blob", None, b"\x89PNG\r\n\x1a\nrest"),
            "png"
        );
        assert_eq!(infer_extension("http://x/blob", None, b"<svg xmlns"), "svg");
        // Fallback.
        assert_eq!(infer_extension("http://x/blob", None, b"???"), "bin");
        // A dotfile-like segment is not an extension.
        assert_eq!(infer_extension("http://x/.hidden", None, b"???"), "bin");
    }
}
