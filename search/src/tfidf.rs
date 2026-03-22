use lasso::{Rodeo, RodeoReader, Spur};
use rayon::prelude::*;
use std::{collections::HashMap, sync::Mutex};

use super::DocumentCreateError;
use super::SearchEngine;

pub struct TfIdf {
    docs: HashMap<String, Doc>,
    idf_cache: Vec<f64>,
    doc_count: usize,
    tf_strat: TermFreqStrategy,
    idf_strat: InverseDocFreqStrategy,
    rodeo: RodeoReader,
    stemmer: rust_stemmers::Stemmer,
}

impl SearchEngine for TfIdf {
    fn query(&mut self, query: &[&str]) -> Vec<&str> {
        let spurs: Vec<Spur> = query
            .iter()
            .filter_map(|t| self.rodeo.get(self.stemmer.stem(t).to_lowercase()))
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

    fn add_dir(&mut self, dir_path: &std::path::Path) -> Option<Vec<DocumentCreateError>> {
        let rodeo = Mutex::new(Rodeo::default());

        let mut file_paths = Vec::new();
        let mut walk_errs = Vec::new();
        super::collect_paths(dir_path, &mut file_paths, &mut walk_errs).unwrap();

        let results: Vec<(String, Result<Doc, DocumentCreateError>)> = file_paths
            .into_par_iter()
            .map(|path| {
                let key = path.to_string_lossy().into_owned();
                let result = Doc::from_path(&path, &rodeo);
                (key, result)
            })
            .collect();

        for (key, result) in results {
            if let Ok(doc) = result {
                self.docs.insert(key, doc);
            }
        }

        let built = rodeo
            .into_inner()
            .expect("Rodeo Mutex was poisoned during indexing");
        self.rodeo = built.into_reader();

        walk_errs.extend(self.rebuild_idf_cache());
        if walk_errs.is_empty() {
            None
        } else {
            Some(walk_errs)
        }
    }
}

impl Default for TfIdf {
    fn default() -> Self {
        Self {
            docs: HashMap::default(),
            idf_cache: vec![],
            doc_count: 0,
            tf_strat: TermFreqStrategy::default(),
            idf_strat: InverseDocFreqStrategy::default(),
            rodeo: Rodeo::default().into_reader(),
            stemmer: rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English),
        }
    }
}

#[derive(Debug, Default)]
pub enum TermFreqStrategy {
    Binary,
    RawCount,
    #[default]
    TermFreq,
    LogNorm,
    DoubleNorm,
    DoubleNormK(f64),
}

#[derive(Debug, Default)]
pub enum InverseDocFreqStrategy {
    Unary,
    #[default]
    IDF,
    IDFSmooth,
    ProbabilisticIDF,
}

impl TfIdf {
    pub fn new(tf_strat: TermFreqStrategy, idf_strat: InverseDocFreqStrategy) -> Self {
        Self {
            tf_strat,
            idf_strat,
            ..Default::default()
        }
    }

    pub fn query(&self, terms: &[&str]) -> Vec<&str> {
        let spurs: Vec<Spur> = terms
            .iter()
            .filter_map(|t| self.rodeo.get(self.stemmer.stem(t).to_lowercase()))
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

#[inline(always)]
fn spur_index(s: Spur) -> usize {
    s.into_inner().get() as usize
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
        let (_doc_len, words) = super::parse_file(rodeo, p)?;
        let word_count = words.iter().fold(0, |acc, (_, count)| count + acc);
        Ok(Self {
            words,
            doc_word_count: word_count,
        })
    }
}
