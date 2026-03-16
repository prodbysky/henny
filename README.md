# henny

A TF-IDF based local document search engine with a simple web frontend.

## Overview

henny indexes a directory of documents and lets you query them via a search interface in the browser. Documents are ranked by TF-IDF score. Supported file formats are HTML, PDF, and XML/XHTML.
The project is split into three parts:

- `tfidf` - library crate implementing document indexing and search
- `backend` - HTTP server exposing a `/query` endpoint
- `frontend` - HTTP server serving the kinda-static web UI

## Usage

### Building
```
cargo build --release
```

### Running

Start the backend (indexes documents and serves queries):
```
cargo run --bin backend
```

Start the frontend (serves the web UI):
```
cargo run --bin frontend
```

Then open `http://127.0.0.1:7070` in your browser.

### Document directory

You can customize the served/indexed folder with cli args for the backend

## Dependencies

- [lopdf](https://github.com/J-F-Liu/lopdf) - PDF parsing
- [nanohtml2text](https://crates.io/crates/nanohtml2text) - HTML text extraction
- [xml-rs](https://crates.io/crates/xml-rs) - XML parsing
- [rust-stemmers](https://crates.io/crates/rust-stemmers) - Snowball stemming
- [tiny_http](https://crates.io/crates/tiny_http) - HTTP server
- [serde_json](https://crates.io/crates/serde_json) - JSON serialization
- [clap](https://crates.io/crates/clap) - God mista zozin would be dissapointed

## TODO
 - Pagination: `?query=foo&limit=10&offset=0`
 - Show tf-idf score next to result in query
 - Allow user to click on result and send that stuff over
 - /status frontend - shows amount of docs, etc.
 - Persistent user accounts? Have a search hist., last downloaded docs. etc.
 - 
