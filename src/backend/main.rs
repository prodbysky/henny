use log::{error, warn, info};
use std::collections::HashMap;
use tiny_http::{Header, Response};
use url::form_urlencoded;

use tfidf::Search;

const QUERY_ENDPOINT: &str = "/query";

fn main() {
    env_logger::init();
    let args = Args::parse();

    let mut search = Search::default();
    if !std::path::Path::new(&args.doc_folder).exists() {
        error!("Document folder {} does not exist.", &args.doc_folder);
        return;
    }
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

        info!("Received request {:#?} from {:?}", &rq, rq.remote_addr());

        if rq.url().starts_with(QUERY_ENDPOINT) {
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
                    info!("{:?} made a query ({:?}), sending back {} results (collected in {:.2}s.)", rq.remote_addr(), &q, results.len(), time.elapsed().as_secs_f64());
                    let results = format!("{{\"results\": {}}}", serde_json::to_string(&results).unwrap());
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
                    warn!("Received request from {:?}, which had a missing query param (borked?)", rq.remote_addr());
                    Response::from_string(body)
                        .with_status_code(400)
                        .with_header(
                            Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=UTF-8"[..]).unwrap(),
                        )
                        .with_header(
                            Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                        )
                }
            };
            _ = rq.respond(response);
        }
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
    doc_folder: String 
}
