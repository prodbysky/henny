// TODO: Make this more of a library (good errors, performance etc)
use log::warn;
use rayon::prelude::*;
use std::{collections::HashMap, io};

#[derive(Debug, Default)]
pub struct Search {
    docs: HashMap<String, Doc>,
    idf_cache: HashMap<String, f64>,
    doc_count: usize,
    tf_strat: TermFreqStrategy,
    idf_strat: InverseDocFreqStrategy,
}

#[derive(Debug, Default)]
enum TermFreqStrategy {
    Binary,
    RawCount,
    #[default]
    TermFreq,
    LogNorm,
    DoubleNorm,
    DoubleNormK(f64),
}

#[derive(Debug, Default)]
enum InverseDocFreqStrategy {
    Unary,
    #[default]
    IDF,
    IDFSmooth,
    ProbabilisticIDF,
}

impl Search {
    pub fn query(&self, terms: &[&str]) -> Vec<&str> {
        let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);

        let stemmed: Vec<String> = terms
            .iter()
            .map(|t| stemmer.stem(t).to_lowercase())
            .collect();

        let mut scores: Vec<(&str, f64)> = self
            .docs
            .par_iter()
            .map(|(path, doc)| {
                let score = stemmed.iter().fold(0.0_f64, |acc, term| {
                    let Some(&tf_count) = doc.words.get(term.as_str()) else {
                        return acc;
                    };
                    let Some(&idf) = self.idf_cache.get(term.as_str()) else {
                        return acc;
                    };
                    // https://en.wikipedia.org/wiki/Tf%E2%80%93idf#Definition
                    let tf = match self.tf_strat {
                        TermFreqStrategy::Binary => (tf_count != 0) as i64 as f64,
                        TermFreqStrategy::RawCount => tf_count as f64,
                        TermFreqStrategy::TermFreq => tf_count as f64 / doc.doc_word_count as f64,
                        TermFreqStrategy::LogNorm => (tf_count as f64 + 1.0).log2(),
                        TermFreqStrategy::DoubleNorm => {
                            0.5 + 0.5
                                * (tf_count as f64 / *doc.words.values().max().unwrap_or(&1) as f64)
                        }
                        TermFreqStrategy::DoubleNormK(k) => {
                            k + (1.0 - k)
                                * (tf_count as f64 / *doc.words.values().max().unwrap_or(&1) as f64)
                        }
                    };
                    acc + tf * idf
                });
                (path.as_str(), score)
            })
            .collect();

        scores.sort_unstable_by(|(_, a), (_, b)| b.total_cmp(a));
        scores
            .into_iter()
            .filter(|(_, score)| *score != 0.0)
            .map(|(path, _)| path)
            .collect()
    }

    pub fn add_dir(&mut self, p: &std::path::Path) -> Result<Vec<DocumentCreateError>, io::Error> {
        let errs = self.add_dir_inner(p)?;
        self.rebuild_idf_cache();
        Ok(errs)
    }


    fn add_dir_inner(&mut self, p: &std::path::Path) -> Result<Vec<DocumentCreateError>, io::Error> {
        let mut errs = vec![];
        for entry in p.read_dir()? {
            let Ok(entry) = entry else { continue };
            let Ok(meta) = entry.metadata() else {
                warn!("Failed to retrieve metadata for {}", entry.path().display());
                continue;
            };
            if meta.is_file() {
                match self.new_doc(&entry.path()) {
                    Ok(doc) => {
                        self.docs.insert(entry.path().to_string_lossy().into_owned(), doc);
                    }
                    Err(e) => errs.push(e),
                }
            } else {
                errs.extend(self.add_dir_inner(&entry.path())?); 
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
                "html" => {
                    let text = html2md::rewrite_html(&std::fs::read_to_string(p)?, false);
                    Ok(Doc::from_text(&text))
                }
                ext => Err(DocumentCreateError::UnsupportedFileExtension(Some(
                    ext.to_string(),
                ))),
            },
            _ => Err(DocumentCreateError::UnsupportedFileExtension(None)),
        }
    }
    fn rebuild_idf_cache(&mut self) {
        self.idf_cache.clear();
        self.doc_count = self.docs.len();

        let mut df: HashMap<&str, usize> = HashMap::new();
        for doc in self.docs.values() {
            for term in doc.words.keys() {
                *df.entry(term.as_str()).or_insert(0) += 1;
            }
        }

        let n = self.doc_count as f64;
        for (term, count) in df {
            let idf = match self.idf_strat {
                InverseDocFreqStrategy::Unary => 1.0,
                InverseDocFreqStrategy::IDF => (n / count as f64).log2(),
                InverseDocFreqStrategy::IDFSmooth => (n / (1 + count) as f64).log2() + 1.0,
                InverseDocFreqStrategy::ProbabilisticIDF => {
                    ((n - count as f64) / count as f64).log2()
                }
            };
            self.idf_cache.insert(term.to_string(), idf);
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
