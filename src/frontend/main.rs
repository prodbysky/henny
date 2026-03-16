use log::error;
use tiny_http::{Header, Request, Response};

fn main() {
    env_logger::init();
    let server = tiny_http::Server::http("0.0.0.0:7070").unwrap();
    loop {
        let server_request = match server.recv() {
            Ok(rq) => rq,
            Err(e) => {
                error!("Http recv. error: {e}");
                continue;
            }
        };

        match server_request.url() {
            "/" => handle_root(server_request),
            "/index.css" => handle_css(server_request),
            "/index.js" => handle_js(server_request),
            _ => handle_404(server_request),
        };
    }
}

fn handle_root(rq: Request) {
    let index = std::fs::read_to_string("res/index.html").unwrap();
    let resp = Response::from_string(&index).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=UTF-8"[..]).unwrap(),
    );
    _ = rq.respond(resp);
}
fn handle_css(rq: Request) {
    let css = std::fs::read_to_string("res/index.css").unwrap();
    let resp = Response::from_string(&css).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/css; charset=UTF-8"[..]).unwrap(),
    );
    _ = rq.respond(resp);
}
fn handle_js(rq: Request) {
    let js = std::fs::read_to_string("res/index.js").unwrap();
    let resp = Response::from_string(&js).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/js; charset=UTF-8"[..]).unwrap(),
    );
    _ = rq.respond(resp);
}
fn handle_404(rq: Request) {
    let js = std::fs::read_to_string("res/404.html").unwrap();
    let resp = Response::from_string(&js).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=UTF-8"[..]).unwrap(),
    );
    _ = rq.respond(resp);
}
