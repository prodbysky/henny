use log::warn;
use std::collections::HashMap;
use tiny_http::{Header, Response};
use url::form_urlencoded;

use tfidf::Search;

const QUERY_ENDPOINT: &str = "/query";

fn main() {
    env_logger::init();
    let mut search = Search::default();
    _ = search.add_dir(std::path::Path::new("opengl-refs"));
    let server = tiny_http::Server::http("0.0.0.0:6969").unwrap();
    loop {
        let rq = match server.recv() {
            Ok(rq) => rq,
            Err(e) => {
                warn!("Http recv. error: {e}");
                continue;
            }
        };

        if rq.url().starts_with(QUERY_ENDPOINT) {
            let url = rq.url();
            let query_string = url.split('?').nth(1).unwrap_or("");

            let params: HashMap<_, _> =
                form_urlencoded::parse(query_string.as_bytes()).into_owned().collect();

            if let Some(query) = params.get("query") {
                let q = query.split_whitespace().collect::<Vec<_>>();
                let results = search.query(&q);
                let results = serde_json::to_string(&results).unwrap();
                let respo = Response::from_string(results).with_header(
                    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
                ).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=UTF-8"[..]).unwrap(),
                );
                _ = rq.respond(respo);
            }
        }
    }
}

