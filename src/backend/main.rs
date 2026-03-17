use log::warn;
use std::collections::HashMap;
use tiny_http::{Header, Request, Response};
use url::form_urlencoded;

use tfidf::Search;

const QUERY_ENDPOINT: &str = "/query";
const FILE_ENDPOINT: &str = "/file";

fn main() {
    env_logger::init();
    let args = Args::parse();

    let mut search = Search::default();
    _ = search.add_dir(std::path::Path::new(&args.doc_folder));
    let server = tiny_http::Server::http(format!("0.0.0.0:{}", args.port)).unwrap();
    loop {
        let rq = match server.recv() {
            Ok(rq) => rq,
            Err(e) => {
                warn!("Http recv. error: {e}");
                continue;
            }
        };

        if rq.url().starts_with(QUERY_ENDPOINT) {
            handle_query(rq, &mut search);
        } else if rq.url().starts_with(FILE_ENDPOINT) {
            handle_file_download(rq, &args);
        }
    }
}

fn handle_query(rq: Request, search: &mut Search) {
    let url = rq.url();
    let query_string = url.split('?').nth(1).unwrap_or("");

    let params: HashMap<_, _> = form_urlencoded::parse(query_string.as_bytes())
        .into_owned()
        .collect();

    let response = match params.get("query") {
        Some(query) => {
            let q = query.split_whitespace().collect::<Vec<_>>();
            let results = search.query(&q);
            let results = format!(
                "{{\"results\": {}}}",
                serde_json::to_string(&results).unwrap()
            );
            Response::from_string(results)
                .with_header(
                    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                )
                .with_header(
                    Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/json; charset=UTF-8"[..],
                    )
                    .unwrap(),
                )
        }
        None => {
            let body = format!("{{\"error\": \"missing `query` parameter\"}}");
            Response::from_string(body)
                .with_status_code(400)
                .with_header(
                    Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/json; charset=UTF-8"[..],
                    )
                    .unwrap(),
                )
                .with_header(
                    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                )
        }
    };
    _ = rq.respond(response);
}
fn handle_file_download(rq: Request, args: &Args) {
    let url = rq.url().to_string();
    let query_string = url.split('?').nth(1).unwrap_or("");

    let params: HashMap<_, _> = form_urlencoded::parse(query_string.as_bytes())
        .into_owned()
        .collect();

    let response = match params.get("path") {
        Some(path) => {
            let requested = std::path::Path::new(path.as_str());
            let doc_root = std::fs::canonicalize(&args.doc_folder).unwrap_or_default();

            match std::fs::canonicalize(requested) {
                Ok(canonical) if canonical.starts_with(&doc_root) => {
                    match std::fs::read(&canonical) {
                        Ok(bytes) => {
                            let mime = mime_for_path(&canonical);
                            let filename = canonical
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            let disposition = format!("attachment; filename=\"{}\"", filename);
                            Response::from_data(bytes)
                                .with_header(
                                    Header::from_bytes(&b"Content-Type"[..], mime.as_bytes())
                                        .unwrap(),
                                )
                                .with_header(
                                    Header::from_bytes(
                                        &b"Content-Disposition"[..],
                                        disposition.as_bytes(),
                                    )
                                    .unwrap(),
                                )
                                .with_header(
                                    Header::from_bytes(
                                        &b"Access-Control-Allow-Origin"[..],
                                        &b"*"[..],
                                    )
                                    .unwrap(),
                                )
                        }
                        Err(_) => json_error(404, "file not found"),
                    }
                }
                _ => json_error(403, "access denied"),
            }
        }
        None => json_error(400, "missing `path` parameter"),
    };
    _ = rq.respond(response);
}

fn mime_for_path(p: &std::path::Path) -> String {
    match p.extension().and_then(|e| e.to_str()) {
        Some("pdf") => "application/pdf".into(),
        Some("html") => "text/html; charset=UTF-8".into(),
        Some("xhtml") => "application/xhtml+xml; charset=UTF-8".into(),
        Some("xml") => "application/xml; charset=UTF-8".into(),
        _ => "application/octet-stream".into(),
    }
}

fn json_error(status: u16, msg: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(format!("{{\"error\": \"{}\"}}", msg))
        .with_status_code(status)
        .with_header(
            Header::from_bytes(
                &b"Content-Type"[..],
                &b"application/json; charset=UTF-8"[..],
            )
            .unwrap(),
        )
        .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
}

use clap::Parser;

/// Backend server for henny webapp i guess
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Port to bind the server to
    #[arg(short, long, default_value_t = 6969)]
    port: u16,

    #[arg(short, long, default_value_t = ("hendocs/".to_string()))]
    doc_folder: String,
}
