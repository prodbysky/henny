# henny

A TF-IDF or BM25 based local document search engine with a web/GTK4 frontend.

## Overview

henny indexes a directory of documents and lets you query them via a search interface in the browser. Supported file formats are HTML, PDF, and XML/XHTML.

## Usage

### Building
```
cargo build --release
```

### Running the backend
Start the backend (indexes docs, opens an API endpoint):
```
cargo run
```
Then open `http://127.0.0.1:6969` in your browser. The port is customizable via CLI args

### Using the GTK4 frontend
Ensure that the backend is running, after that run the henny_gtk4 executable.

### Document directory
You can customize the served/indexed folder with cli args for the backend

## API end-points
- /query - takes a url encoded string query (param. name: `query`), and optionally takes the max amount of query results (param. name: `n_result`, default: 255)
- /file - takes a path (param. name: `path`), and shoots back the file requested.


## TODO
 - /status frontend - shows amount of docs, etc.
 - Persistent user accounts? Have a search hist., last downloaded docs. etc.
