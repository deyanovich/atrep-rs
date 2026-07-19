//! atrep-lsp — language server for atrep documents (`.atd`/`.atk`)
//! and dialektos definitions (`.dia`/`.lektos`).
//!
//! stdio transport. v1 surface: publish-diagnostics (first parse
//! error, via atrep) and full-document semantic tokens
//! (lexical highlighting; sim symbols checked against the resolved
//! dialektos when resolution succeeds).

mod analysis;
mod scan;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    PublishDiagnosticsParams, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};

type Docs = HashMap<String, String>;

fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: scan::LEGEND.to_vec(),
                    token_modifiers: Vec::new(),
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                ..Default::default()
            },
        )),
        ..Default::default()
    })?;
    connection.initialize(capabilities)?;
    // main_loop consumes the connection so its channel ends are
    // dropped before join(), letting the io threads terminate.
    main_loop(connection)?;
    io_threads.join()?;
    Ok(())
}

fn main_loop(connection: Connection) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let mut docs: Docs = HashMap::new();
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                handle_request(&connection, &docs, req)?;
            }
            Message::Notification(note) => {
                handle_notification(&connection, &mut docs, note)?;
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn handle_request(
    connection: &Connection,
    docs: &Docs,
    req: Request,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    match req.method.as_str() {
        "textDocument/semanticTokens/full" => {
            let (id, params): (RequestId, SemanticTokensParams) =
                req.extract("textDocument/semanticTokens/full")?;
            let uri = params.text_document.uri;
            let response = match docs.get(uri.as_str()) {
                Some(text) => {
                    let path = uri_to_path(&uri);
                    let dial = analysis::resolve_dialektos(text, &path);
                    let data = scan::encode(scan::scan(text, dial.as_ref()));
                    let result = SemanticTokensResult::Tokens(SemanticTokens {
                        result_id: None,
                        data,
                    });
                    Response::new_ok(id, serde_json::to_value(result)?)
                }
                None => Response::new_ok(id, serde_json::Value::Null),
            };
            connection.sender.send(Message::Response(response))?;
        }
        _ => {
            let response = Response::new_err(
                req.id,
                ErrorCode::MethodNotFound as i32,
                format!("unhandled method: {}", req.method),
            );
            connection.sender.send(Message::Response(response))?;
        }
    }
    Ok(())
}

fn handle_notification(
    connection: &Connection,
    docs: &mut Docs,
    note: Notification,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    match note.method.as_str() {
        "textDocument/didOpen" => {
            let params: DidOpenTextDocumentParams = note.extract("textDocument/didOpen")?;
            let uri = params.text_document.uri;
            docs.insert(uri.as_str().to_string(), params.text_document.text);
            publish(connection, docs, uri)?;
        }
        "textDocument/didChange" => {
            let params: DidChangeTextDocumentParams = note.extract("textDocument/didChange")?;
            let uri = params.text_document.uri;
            // FULL sync: the last change carries the whole document.
            if let Some(change) = params.content_changes.into_iter().next_back() {
                docs.insert(uri.as_str().to_string(), change.text);
            }
            publish(connection, docs, uri)?;
        }
        "textDocument/didClose" => {
            let params: DidCloseTextDocumentParams = note.extract("textDocument/didClose")?;
            let uri = params.text_document.uri;
            docs.remove(uri.as_str());
            // clear stale diagnostics
            send_diagnostics(connection, uri, Vec::new())?;
        }
        _ => {}
    }
    Ok(())
}

fn publish(
    connection: &Connection,
    docs: &Docs,
    uri: Uri,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let Some(text) = docs.get(uri.as_str()) else {
        return Ok(());
    };
    let path = uri_to_path(&uri);
    let diagnostics = analysis::diagnostics(text, &path);
    send_diagnostics(connection, uri, diagnostics)
}

fn send_diagnostics(
    connection: &Connection,
    uri: Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    connection
        .sender
        .send(Message::Notification(Notification::new(
            "textDocument/publishDiagnostics".to_string(),
            params,
        )))?;
    Ok(())
}

/// Filesystem path for a `file://` URI (percent-decoded); other
/// schemes fall back to a synthetic buffer path, which still lets
/// std dialektoi resolve.
fn uri_to_path(uri: &Uri) -> PathBuf {
    let s = uri.as_str();
    if let Some(rest) = s.strip_prefix("file://") {
        let path_part = match rest.find('/') {
            Some(0) => rest,
            Some(i) => &rest[i..], // strip authority
            None => rest,
        };
        return PathBuf::from(percent_decode(path_part));
    }
    Path::new("untitled.atd").to_path_buf()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn file_uri_decodes() {
        let uri = Uri::from_str("file:///srv/texts/x%20y.atd").unwrap();
        assert_eq!(uri_to_path(&uri), PathBuf::from("/srv/texts/x y.atd"));
    }

    #[test]
    fn non_file_uri_falls_back() {
        let uri = Uri::from_str("untitled:Untitled-1").unwrap();
        assert_eq!(uri_to_path(&uri), PathBuf::from("untitled.atd"));
    }
}
