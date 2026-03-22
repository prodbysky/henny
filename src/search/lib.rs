pub mod bm25;
pub mod tfidf;

use std::{collections::HashMap, io, path::PathBuf, sync::Mutex};

pub use bm25::BM25;
use lasso::{Rodeo, Spur};
pub use tfidf::TfIdf;

pub trait SearchEngine {
    fn query(&mut self, query: &[&str]) -> Vec<&str>;
    fn add_dir(&mut self, dir_path: &std::path::Path) -> Option<Vec<DocumentCreateError>>;
}

fn collect_paths(
    dir: &std::path::Path,
    out: &mut Vec<PathBuf>,
    errs: &mut Vec<DocumentCreateError>,
) -> Result<(), io::Error> {
    for entry in dir.read_dir()? {
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else {
            errs.push(DocumentCreateError::IOError(io::Error::new(
                io::ErrorKind::Other,
                format!("metadata error for {}", entry.path().display()),
            )));
            continue;
        };
        if meta.is_file() {
            out.push(entry.path());
        } else {
            collect_paths(&entry.path(), out, errs)?;
        }
    }
    Ok(())
}

fn extract_text(path: &std::path::Path) -> Result<String, DocumentCreateError> {
    match path.extension() {
        Some(s) => match s.to_str().unwrap() {
            "xml" | "xhtml" => {
                let file = std::io::BufReader::new(std::fs::File::open(path)?);
                let parser = xml::EventReader::new(file);
                let mut text = String::with_capacity(1024 * 256);
                for e in parser {
                    if let Ok(xml::reader::XmlEvent::Characters(c)) = e {
                        text.push_str(&c);
                        text.push(' ');
                    }
                }
                Ok(text)
            }
            "pdf" => {
                let doc = lopdf::Document::load(path)?;
                if doc.is_encrypted() {
                    return Err(DocumentCreateError::EncryptedPDF);
                } else {
                    return Ok(doc.extract_text(&doc.get_pages().into_keys().collect::<Vec<_>>())?);
                }
            }
            "html" => {
                let text = html2md::rewrite_html(&std::fs::read_to_string(path)?, false);
                return Ok(text);
            }
            ext => Err(DocumentCreateError::UnsupportedFileExtension(Some(
                ext.to_string(),
            ))),
        },
        _ => Err(DocumentCreateError::UnsupportedFileExtension(None)),
    }
}

fn parse_file(
    rodeo: &Mutex<Rodeo>,
    path: &std::path::Path,
) -> Result<(usize, Vec<(Spur, u32)>), DocumentCreateError> {
    let text = extract_text(path)?;

    let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);

    let mut counts: HashMap<Spur, u32> = HashMap::new();
    let mut current_word = String::new();

    let mut record = |word: &str| {
        if word.is_empty() {
            return;
        }
        let stemmed = stemmer.stem(word).to_lowercase();
        if stemmed.is_empty() {
            return;
        }
        let spur = rodeo
            .lock()
            .expect("Rodeo Mutex poisoned")
            .get_or_intern(&stemmed);
        *counts.entry(spur).or_insert(0) += 1;
    };

    for c in text.chars() {
        if c.is_alphanumeric() || c == '\'' || c == '-' {
            current_word.push(c);
        } else {
            record(&current_word);
            current_word.clear();
        }
    }
    record(&current_word);

    let mut words: Vec<(Spur, u32)> = counts.into_iter().collect();
    words.sort_unstable_by_key(|&(s, _)| s);
    words.shrink_to_fit();
    Ok((text.len(), words))
}

#[derive(Debug)]
pub enum DocumentCreateError {
    UnsupportedFileExtension(Option<String>),
    FailedToExtractPDFText(lopdf::Error),
    IOError(std::io::Error),
    EncryptedPDF,
}

impl std::error::Error for DocumentCreateError {}

impl std::fmt::Display for DocumentCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFileExtension(None) => write!(f, "Can't index binary file"),
            Self::UnsupportedFileExtension(Some(ext)) => {
                write!(f, "Can't index file with extension: {ext}")
            }
            Self::FailedToExtractPDFText(err) => {
                write!(f, "Failed to extract text from pdf: {err}")
            }
            Self::IOError(io_err) => write!(f, "An IO error occurred: {io_err}"),
            Self::EncryptedPDF => write!(f, "Couldn't extract text from an encrypted pdf file"),
        }
    }
}

impl From<lopdf::Error> for DocumentCreateError {
    fn from(value: lopdf::Error) -> Self {
        Self::FailedToExtractPDFText(value)
    }
}

impl From<std::io::Error> for DocumentCreateError {
    fn from(value: std::io::Error) -> Self {
        Self::IOError(value)
    }
}
