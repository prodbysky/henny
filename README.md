# henny

A TF-IDF or BM25 based local document search engine with a simple web frontend.

## Overview

henny indexes a directory of documents and lets you query them via a search interface in the browser. Supported file formats are HTML, PDF, and XML/XHTML.

## Usage

### Building
```
cargo build --release
```

### Running

Start the frontend (serves the web UI):
```
cargo run
```

Then open `http://127.0.0.1:6969` in your browser. The port is customizable via CLI args

### Document directory

You can customize the served/indexed folder with cli args for the backend

## TODO
 - /status frontend - shows amount of docs, etc.
 - Persistent user accounts? Have a search hist., last downloaded docs. etc.
