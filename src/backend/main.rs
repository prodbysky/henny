use log::{error, info, warn};
use search::BM25;
use std::collections::HashMap;
use tiny_http::{Header, Request, Response};
use url::form_urlencoded;

use search::SearchEngine;
use search::TfIdf;

const QUERY_ENDPOINT: &str = "/query";
const FILE_ENDPOINT: &str = "/file";

struct Handler {
    applicable: fn(&str) -> bool,
    handle: fn(Request, &Args, &mut BM25, &mut Stats) -> Result<(), Error>,
}

const HANDLERS: &[Handler] = &[
    Handler {
        applicable: |s| s.starts_with(QUERY_ENDPOINT),
        handle: handle_query,
    },
    Handler {
        applicable: |s| s.starts_with(FILE_ENDPOINT),
        handle: handle_file_download,
    },
    Handler {
        applicable: |s| s == "/",
        handle: handle_root,
    },
    Handler {
        applicable: |s| s == "/index.css",
        handle: handle_css,
    },
    Handler {
        applicable: |s| s == "/index.js",
        handle: handle_js,
    },
    Handler {
        applicable: |_| true,
        handle: handle_404,
    },
];

fn main() {
    env_logger::init();
    let args = Args::parse();

    let mut stats = Stats::default();

    // let mut search = TfIdf::new(
    //     search::tfidf::TermFreqStrategy::DoubleNorm,
    //     search::tfidf::InverseDocFreqStrategy::IDFSmooth,
    // );

    let mut search = BM25::new();
    let time = std::time::Instant::now();
    for e in search.add_dir(std::path::Path::new(&args.doc_folder)).unwrap() {
        warn!("{e}");
    }
    info!("Indexing took: {:.2}", time.elapsed().as_secs_f64());
    let server = tiny_http::Server::http(format!("0.0.0.0:{}", args.port)).unwrap();
    loop {
        let rq = match server.recv() {
            Ok(rq) => rq,
            Err(e) => {
                warn!("Http recv. error: {e}");
                continue;
            }
        };

        stats.request_count += 1;

        let url = rq.url().to_string();
        let from = rq.remote_addr().cloned();

        HANDLERS.iter().find(|x| (x.applicable)(&url)).inspect(|h| {
            if let Err(e) = (h.handle)(rq, &args, &mut search, &mut stats) {
                stats.err_count += 1;
                error!("{e}");
            }
        });

        info!("Received request from {:?} for {}", from, url);
        info!("{}", &stats);
    }
}

#[derive(Debug)]
pub enum Error {
    JsonSerialization(serde_json::Error),
    HeaderCreate,
    Io(std::io::Error),
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::JsonSerialization(value)
    }
}

impl From<()> for Error {
    fn from(_value: ()) -> Self {
        Self::HeaderCreate
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JsonSerialization(err) => write!(f, "Failed to serialize some json: {err}"),
            Self::HeaderCreate => write!(f, "Failed to create a http header"),
            Self::Io(err) => write!(f, "An IO error occured: {err}"),
        }
    }
}

fn handle_query(
    rq: Request,
    _args: &Args,
    search: &mut BM25,
    stats: &mut Stats,
) -> Result<(), Error> {
    stats.query_count += 1;
    let url = rq.url();
    let query_string = url.split('?').nth(1).unwrap_or("");

    let params: HashMap<_, _> = form_urlencoded::parse(query_string.as_bytes())
        .into_owned()
        .collect();

    let response = match params.get("query") {
        Some(query) => {
            let q = query.split_whitespace().collect::<Vec<_>>();
            let time = std::time::Instant::now();
            let results = search.query(&q);
            let n_results = params
                .get("n_result")
                .unwrap_or(&"0".to_string())
                .parse()
                .unwrap_or(0)
                .clamp(0, results.len());
            stats.query_time += time.elapsed();
            let results = format!(
                "{{\"results\": {}}}",
                serde_json::to_string(&results[..n_results])?
            );
            Response::from_string(results)
                .with_header(Header::from_bytes(
                    &b"Access-Control-Allow-Origin"[..],
                    &b"*"[..],
                )?)
                .with_header(Header::from_bytes(
                    &b"Content-Type"[..],
                    &b"application/json; charset=UTF-8"[..],
                )?)
        }
        None => {
            let body = format!("{{\"error\": \"missing `query` parameter\"}}");
            Response::from_string(body)
                .with_status_code(400)
                .with_header(Header::from_bytes(
                    &b"Content-Type"[..],
                    &b"application/json; charset=UTF-8"[..],
                )?)
                .with_header(Header::from_bytes(
                    &b"Access-Control-Allow-Origin"[..],
                    &b"*"[..],
                )?)
        }
    };
    Ok(rq.respond(response)?)
}

fn handle_file_download(
    rq: Request,
    args: &Args,
    _: &mut BM25,
    stats: &mut Stats,
) -> Result<(), Error> {
    let url = rq.url().to_string();
    let query_string = url.split('?').nth(1).unwrap_or("");

    stats.download_count += 1;

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
                            stats.download_size += bytes.len();
                            let mime = mime_for_path(&canonical);
                            let filename = canonical
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            let disposition = format!("attachment; filename=\"{}\"", filename);
                            Response::from_data(bytes)
                                .with_header(Header::from_bytes(
                                    &b"Content-Type"[..],
                                    mime.as_bytes(),
                                )?)
                                .with_header(Header::from_bytes(
                                    &b"Content-Disposition"[..],
                                    disposition.as_bytes(),
                                )?)
                                .with_header(Header::from_bytes(
                                    &b"Access-Control-Allow-Origin"[..],
                                    &b"*"[..],
                                )?)
                        }
                        Err(_) => json_error(404, "file not found"),
                    }
                }
                _ => json_error(403, "access denied"),
            }
        }
        None => json_error(400, "missing `path` parameter"),
    };
    Ok(rq.respond(response)?)
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

fn handle_root(rq: Request, _: &Args, _: &mut BM25, _: &mut Stats) -> Result<(), Error> {
    let index = std::fs::read_to_string("res/index.html")?;
    let resp = Response::from_string(&index).with_header(Header::from_bytes(
        &b"Content-Type"[..],
        &b"text/html; charset=UTF-8"[..],
    )?);
    Ok(rq.respond(resp)?)
}

fn handle_css(rq: Request, _: &Args, _: &mut BM25, _: &mut Stats) -> Result<(), Error> {
    let css = std::fs::read_to_string("res/index.css")?;
    let resp = Response::from_string(&css).with_header(Header::from_bytes(
        &b"Content-Type"[..],
        &b"text/css; charset=UTF-8"[..],
    )?);
    Ok(rq.respond(resp)?)
}

fn handle_js(rq: Request, _: &Args, _: &mut BM25, _: &mut Stats) -> Result<(), Error> {
    let js = std::fs::read_to_string("res/index.js")?;
    let resp = Response::from_string(&js).with_header(Header::from_bytes(
        &b"Content-Type"[..],
        &b"text/js; charset=UTF-8"[..],
    )?);
    Ok(rq.respond(resp)?)
}

fn handle_404(rq: Request, _: &Args, _: &mut BM25, _: &mut Stats) -> Result<(), Error> {
    let js = std::fs::read_to_string("res/404.html")?;
    let resp = Response::from_string(&js).with_header(Header::from_bytes(
        &b"Content-Type"[..],
        &b"text/html; charset=UTF-8"[..],
    )?);
    Ok(rq.respond(resp)?)
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

#[derive(Debug, Default)]
struct Stats {
    request_count: usize,
    query_count: usize,
    err_count: usize,
    download_count: usize,
    query_time: std::time::Duration,
    download_size: usize,
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Statistics: ")?;
        writeln!(f, "    Request count:         {}", self.request_count)?;
        writeln!(f, "    Query count:           {}", self.query_count)?;
        writeln!(f, "    Error count:           {}", self.err_count)?;
        writeln!(f, "    File download count:   {}", self.download_count)?;
        writeln!(
            f,
            "    Average query time:    {} s.",
            self.query_time.as_secs_f64() / self.query_count as f64
        )?;
        writeln!(
            f,
            "    Whole downloaded size: {} kb.",
            self.download_size / 1024
        )?;
        if self.download_count != 0 {
            writeln!(
                f,
                "    Avg. downloaded size:  {} kb.",
                (self.download_size / 1024) as f64 / self.download_count as f64
            )?;
        } else {
            writeln!(f, "    Avg. downloaded size:  0 kb.")?;
        }
        Ok(())
    }
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
