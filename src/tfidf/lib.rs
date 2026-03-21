use lasso::{Rodeo, RodeoReader, Spur};
use log::warn;
use rayon::prelude::*;
use std::{collections::HashMap, io, path::PathBuf, sync::Mutex};

pub struct Search {
    docs: HashMap<String, Doc>,
    idf_cache: Vec<f64>,
    doc_count: usize,
    tf_strat: TermFreqStrategy,
    idf_strat: InverseDocFreqStrategy,
    rodeo: RodeoReader,
    stemmer: rust_stemmers::Stemmer
}

impl Default for Search {
    fn default() -> Self {
        Self {
            docs: HashMap::default(),
            idf_cache: vec![],
            doc_count: 0,
            tf_strat: TermFreqStrategy::default(),
            idf_strat: InverseDocFreqStrategy::default(),
            rodeo: Rodeo::default().into_reader(),
            stemmer: rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English)
        }
    }
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

        let spurs: Vec<Spur> = terms
            .iter()
            .filter_map(|t| self.rodeo.get(&self.stemmer.stem(t).to_lowercase()))
            .collect();

        let mut scores: Vec<(&str, f64)> = self
            .docs
            .par_iter()
            .map(|(path, doc)| {
                let score = spurs.iter().fold(0.0_f64, |acc, spur| {
                    let Some(&tf_count) = doc.get(spur) else {
                        return acc;
                    };
                    let Some(&idf) = self.idf_cache.get(spur_index(*spur)) else {
                        return acc;
                    };
                    let tf = match self.tf_strat {
                        TermFreqStrategy::Binary => (tf_count != 0) as i64 as f64,
                        TermFreqStrategy::RawCount => tf_count as f64,
                        TermFreqStrategy::TermFreq => tf_count as f64 / doc.doc_word_count as f64,
                        TermFreqStrategy::LogNorm => (tf_count as f64 + 1.0).log2(),
                        TermFreqStrategy::DoubleNorm => {
                            0.5 + 0.5 * (tf_count as f64 / doc.max_count() as f64)
                        }
                        TermFreqStrategy::DoubleNormK(k) => {
                            k + (1.0 - k) * (tf_count as f64 / doc.max_count() as f64)
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
        let rodeo = Mutex::new(Rodeo::default());

        let mut file_paths = Vec::new();
        let mut walk_errs = Vec::new();
        collect_paths(p, &mut file_paths, &mut walk_errs)?;

        let results: Vec<(String, Result<Doc, DocumentCreateError>)> = file_paths
            .into_par_iter()
            .map(|path| {
                let key = path.to_string_lossy().into_owned();
                let result = Doc::from_path(&path, &rodeo);
                (key, result)
            })
            .collect();

        let mut errs: Vec<DocumentCreateError> = walk_errs;
        for (key, result) in results {
            match result {
                Ok(doc) => {
                    self.docs.insert(key, doc);
                }
                Err(e) => errs.push(e),
            }
        }

        let built = rodeo
            .into_inner()
            .expect("Rodeo Mutex was poisoned during indexing");
        self.rodeo = built.into_reader();

        errs.extend(self.rebuild_idf_cache());
        Ok(errs)
    }

    fn rebuild_idf_cache(&mut self) -> Vec<DocumentCreateError> {
        self.doc_count = self.docs.len();
        let n = self.doc_count as f64;

        let num_slots = self.rodeo.len() + 1;
        self.idf_cache.clear();
        self.idf_cache.resize(num_slots, 0.0);

        let mut df: HashMap<Spur, u32> = HashMap::new();
        for doc in self.docs.values() {
            for &spur in doc.spurs() {
                *df.entry(spur).or_insert(0) += 1;
            }
        }

        for (spur, count) in df {
            let idf = match self.idf_strat {
                InverseDocFreqStrategy::Unary => 1.0,
                InverseDocFreqStrategy::IDF => (n / count as f64).log2(),
                InverseDocFreqStrategy::IDFSmooth => (n / (1 + count) as f64).log2() + 1.0,
                InverseDocFreqStrategy::ProbabilisticIDF => {
                    ((n - count as f64) / count as f64).log2()
                }
            };
            self.idf_cache[spur_index(spur)] = idf;
        }

        vec![]
    }
}

fn collect_paths(
    dir: &std::path::Path,
    out: &mut Vec<PathBuf>,
    errs: &mut Vec<DocumentCreateError>,
) -> Result<(), io::Error> {
    for entry in dir.read_dir()? {
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else {
            warn!("Failed to retrieve metadata for {}", entry.path().display());
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

#[inline(always)]
fn spur_index(s: Spur) -> usize {
    s.into_inner().get() as usize
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
    words: Vec<(Spur, u32)>,
    doc_word_count: u32,
}

impl Doc {
    pub fn get(&self, spur: &Spur) -> Option<&u32> {
        self.words
            .binary_search_by_key(spur, |&(s, _)| s)
            .ok()
            .map(|i| &self.words[i].1)
    }

    pub fn spurs(&self) -> impl Iterator<Item = &Spur> {
        self.words.iter().map(|(s, _)| s)
    }

    pub fn max_count(&self) -> u32 {
        self.words.iter().map(|&(_, c)| c).max().unwrap_or(1)
    }

    pub fn from_path(
        p: &std::path::Path,
        rodeo: &Mutex<Rodeo>,
    ) -> Result<Self, DocumentCreateError> {
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
                    Ok(Self::from_text(&text, rodeo))
                }
                "pdf" => {
                    let doc = lopdf::Document::load(p).unwrap();
                    if doc.is_encrypted() {
                        return Err(DocumentCreateError::EncryptedPDF);
                    }
                    Ok(Self::from_text(
                        &doc.extract_text(&doc.get_pages().into_keys().collect::<Vec<_>>())?,
                        rodeo,
                    ))
                }
                "html" => {
                    let text = html2md::rewrite_html(&std::fs::read_to_string(p)?, false);
                    Ok(Self::from_text(&text, rodeo))
                }
                ext => Err(DocumentCreateError::UnsupportedFileExtension(Some(
                    ext.to_string(),
                ))),
            },
            _ => Err(DocumentCreateError::UnsupportedFileExtension(None)),
        }
    }

    pub fn from_text(text: &str, rodeo: &Mutex<Rodeo>) -> Self {
        let stemmer = rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English);

        let mut counts: HashMap<Spur, u32> = HashMap::new();
        let mut current_word = String::new();
        let mut doc_word_count: u32 = 0;

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
                doc_word_count += 1;
            }
        }
        record(&current_word);

        let mut words: Vec<(Spur, u32)> = counts.into_iter().collect();
        words.sort_unstable_by_key(|&(s, _)| s);
        words.shrink_to_fit();

        Doc {
            words,
            doc_word_count,
        }
    }
}
