use std::collections::HashMap;
use std::sync::Mutex;

use lasso::Rodeo;
use lasso::RodeoReader;
use lasso::Spur;
use rayon::prelude::*;

use super::DocumentCreateError;
use super::SearchEngine;

pub struct BM25 {
    docs: HashMap<String, Doc>,
    stemmer: rust_stemmers::Stemmer,
    rodeo: RodeoReader,
    doc_len_sum: usize,
    bm_cache: HashMap<(String, Spur), f64>
}

struct Doc {
    words: Vec<(Spur, u32)>,
}

impl Doc {
    pub fn from_path(
        p: &std::path::Path,
        rodeo: &Mutex<Rodeo>,
    ) -> Result<(Doc, usize), DocumentCreateError> {
        let (doc_len, words) = super::parse_file(rodeo, p)?;
        Ok((Doc { words }, doc_len))
    }

    pub fn get(&self, spur: &Spur) -> Option<&u32> {
        self.words
            .binary_search_by_key(spur, |&(s, _)| s)
            .ok()
            .map(|i| &self.words[i].1)
    }
}

impl BM25 {
    pub fn new() -> Self {
        Self {
            docs: Default::default(),
            stemmer: rust_stemmers::Stemmer::create(rust_stemmers::Algorithm::English),
            doc_len_sum: 0,
            rodeo: Rodeo::default().into_reader(),
            bm_cache: Default::default()
        }
    }

    pub fn doc_count(&self) -> usize {
        self.docs.len()
    }

    pub fn avg_doc_len(&self) -> f64 {
        if self.doc_count() == 0 {
            1.0
        } else {
            self.doc_len_sum as f64 / self.doc_count() as f64
        }
    }
}

impl SearchEngine for BM25 {
    fn add_dir(&mut self, dir_path: &std::path::Path) -> Option<Vec<DocumentCreateError>> {
        let rodeo = Mutex::new(Rodeo::default());
        let mut file_paths = vec![];
        let mut walk_errs = vec![];
        super::collect_paths(dir_path, &mut file_paths, &mut walk_errs).unwrap();
        let results: Vec<(String, Result<(Doc, usize), DocumentCreateError>)> = file_paths
            .into_par_iter()
            .map(|path| {
                let key = path.to_string_lossy().into_owned();
                let result = Doc::from_path(&path, &rodeo);
                (key, result)
            })
            .collect();

        let mut errs = vec![];
        for (key, result) in results {
            match result {
                Ok((d, s)) => {
                    self.doc_len_sum += s;
                    self.docs.insert(key, d);
                }
                Err(e) => {
                    errs.push(e);
                }
            }
        }
        self.rodeo = rodeo.into_inner().unwrap().into_reader();
        if errs.is_empty() {
            return None;
        } else {
            return Some(errs);
        }
    }

    fn query(&mut self, query: &[&str]) -> Vec<&str> {
        let k = 1.2;
        let b = 0.75;
        let spurs: Vec<Spur> = query
            .iter()
            .filter_map(|t| self.rodeo.get(&self.stemmer.stem(t).to_lowercase()))
            .collect();
        let mut results = vec![];

        for (path, doc) in self.docs.iter() {
            let dl = doc.words.iter().fold(0u32, |acc, (_, c)| acc + c) as f64;
            let mut score = 0.0;
            for term in &spurs {
                let Some(&count) = doc.get(term) else { continue };
                let tf = count as f64;
                let bm = match self.bm_cache.get(&(path.to_string(), *term)) {
                    None => {
                        let bm = (tf * (k + 1.0))
                            / (tf + k * (1.0 - b + b * (dl / self.avg_doc_len())));
                        self.bm_cache.insert((path.to_string(), *term), bm);
                        bm
                    }
                    Some(&bm) => bm
                };
                let doc_count_containing_term = self.docs.iter()
                    .filter(|(_, d)| d.get(term).is_some_and(|x| *x != 0))
                    .count() as f64;
                let idf = ((self.doc_count() as f64 - doc_count_containing_term + 0.5)
                    / (doc_count_containing_term + 0.5)
                    + 1.0)
                    .ln();
                score += bm * idf;
            }
            results.push((path.as_str(), score));
        }

        results.sort_unstable_by(|(_, a), (_, b)| b.total_cmp(a));
        results
            .into_iter()
            .filter(|(_, score)| *score != 0.0)
            .map(|(path, _)| path)
            .collect()
    }
}
