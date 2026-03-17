// TODO: Make this more of a library (good errors, performance etc)
use log::warn;
use std::{collections::HashMap, io};

#[derive(Debug, Default)]
pub struct Search {
    docs: HashMap<String, Doc>,
    cache: QueryScoreCache,
}

#[derive(Debug, Default)]
pub struct QueryScoreCache {
    tf: HashMap<(String, String), f64>,
    idf: HashMap<(String, String), f64>,
}

impl QueryScoreCache {
    pub fn get_tf(&self, path: String, term: String) -> Option<&f64> {
        self.tf.get(&(path, term))
    }
    pub fn get_idf(&self, path: String, term: String) -> Option<&f64> {
        self.idf.get(&(path, term))
    }
    pub fn set_tf(&mut self, path: String, term: String, tf: f64) {
        self.tf.insert((path, term), tf);
    }
    pub fn set_idf(&mut self, path: String, term: String, idf: f64) {
        self.idf.insert((path, term), idf);
    }
}

impl Search {
    pub fn query(&mut self, terms: &[&str]) -> Vec<&str> {
        let stem = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
        let mut terms_ = vec![];
        for t in terms {
            terms_.push(stem.stem(t).to_string());
        }
        let mut docs = vec![];
        for (k, v) in self.docs.iter() {
            let mut score = 0.0;
            for t in &terms_ {
                let t = t.to_lowercase();

                let tf = if let Some(tf_score) = self.cache.get_tf(k.to_string(), t.to_string()) {
                    *tf_score
                } else {
                    let Some(count) = v.words.get(&t) else {
                        continue;
                    };
                    let tf = *count as f64 / v.doc_word_count as f64;
                    self.cache.set_tf(k.to_string(), t.to_string(), tf);
                    tf
                };

                let idf = if let Some(idf_score) = self.cache.get_idf(k.to_string(), t.to_string())
                {
                    *idf_score
                } else {
                    let idf = (self.docs.iter().count() as f64
                        / self
                            .docs
                            .iter()
                            .filter(|(_, d)| d.words.contains_key(&t))
                            .count() as f64)
                        .log2();
                    self.cache.set_idf(k.to_string(), t.to_string(), idf);
                    idf
                };
                score += tf * idf;
            }
            docs.push((k, score));
        }
        docs.sort_by(|(_, b1), (_, a1)| a1.total_cmp(b1));
        docs.iter()
            .filter(|(_p, d)| *d != 0.0)
            .map(|(p, _)| p.as_str())
            .collect()
    }
    pub fn add_dir(&mut self, p: &std::path::Path) -> Result<Vec<DocumentCreateError>, io::Error> {
        let mut errs = vec![];
        for d in p.read_dir()? {
            let Ok(d) = d else { continue };
            let Ok(meta) = d.metadata() else {
                warn!(
                    "Failed to retrieve metadata for file {}",
                    d.path().display()
                );
                continue;
            };
            if meta.is_file() {
                let doc = match self.new_doc(&d.path()) {
                    Ok(d) => d,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };
                self.docs
                    .insert(d.path().to_string_lossy().to_string(), doc);
            } else {
                // uhhhhh
                errs.extend(self.add_dir(&d.path())?);
            }
        }
        Ok(errs)
    }
    fn new_doc(&mut self, p: &std::path::Path) -> Result<Doc, DocumentCreateError> {
        match p.extension() {
            Some(s) => match s.to_str().unwrap() {
                "xml" | "xhtml" => {
                    let file = std::io::BufReader::new(std::fs::File::open(p)?);
                    let parser = xml::EventReader::new(file);
                    let mut text = String::with_capacity(1024 * 256);
                    for e in parser {
                        if let Ok(xml::reader::XmlEvent::Characters(c)) = e {
                            text.push_str(&c);
                            text.push(' ');
                        }
                    }
                    Ok(Doc::from_text(&text))
                }
                "pdf" => {
                    let doc = lopdf::Document::load(&p).unwrap();
                    if doc.is_encrypted() {
                        return Err(DocumentCreateError::EncryptedPDF);
                    }
                    Ok(Doc::from_text(&doc.extract_text(
                        &doc.get_pages().into_keys().collect::<Vec<_>>(),
                    )?))
                }
                "html" => Ok(Doc::from_text(&nanohtml2text::html2text(
                    &std::fs::read_to_string(p)?,
                ))),
                ext => Err(DocumentCreateError::UnsupportedFileExtension(Some(
                    ext.to_string(),
                ))),
            },
            _ => Err(DocumentCreateError::UnsupportedFileExtension(None)),
        }
    }
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

#[derive(Debug, Default)]
pub struct Doc {
    words: HashMap<String, usize>,
    doc_word_count: usize,
}

impl Doc {
    pub fn from_text(text: &str) -> Self {
        let stem = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);
        let mut words_map = HashMap::new();
        let mut current_word = String::new();
        let mut doc_word_count = 0;

        let add_to_map = |word: &str, map: &mut HashMap<String, usize>| {
            if !word.is_empty() {
                *map.entry(word.to_string()).or_insert(0) += 1;
            }
        };

        for c in text.chars() {
            if c.is_alphanumeric() || c == '\'' || c == '-' {
                current_word.push(c);
            } else {
                add_to_map(&stem.stem(&current_word).to_string(), &mut words_map);
                current_word.clear();
                doc_word_count += 1;
            }
        }

        add_to_map(&stem.stem(&current_word).to_string(), &mut words_map);

        Doc {
            words: words_map,
            doc_word_count,
        }
    }
}
