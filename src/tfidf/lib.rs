// TODO: Make this more of a library (good errors, performance etc)
use log::warn;
use std::collections::HashMap;

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
    pub fn query(&mut self, terms: &[&str]) -> Vec<String> {
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
            .map(|(p, d)| (p, d))
            .filter(|(_p, d)| **d != 0.0)
            .map(|(p, _)| p.to_owned().clone())
            .collect()
    }
    pub fn add_dir(&mut self, p: &std::path::Path) -> Result<(), ()> {
        for d in p.read_dir().map_err(|e| {
            warn!("Failed to read dir {}: {e}", p.display());
        })? {
            let Ok(d) = d else { continue };
            let Ok(meta) = d.metadata() else {
                warn!(
                    "Failed to retrieve metadata for file {}",
                    d.path().display()
                );
                continue;
            };
            if meta.is_file() {
                let Ok(doc) = self.new_doc(&d.path()) else {
                    warn!("Failed to create doc. from file {}", d.path().display());
                    continue;
                };
                self.docs
                    .insert(d.path().to_string_lossy().to_string(), doc);
            } else {
                _ = self.add_dir(&d.path());
            }
        }
        Ok(())
    }
    fn new_doc(&mut self, p: &std::path::Path) -> Result<Doc, ()> {
        match p.extension() {
            Some(s) => match s.to_str().unwrap() {
                "xml" | "xhtml" => {
                    let file = std::io::BufReader::new(std::fs::File::open(p).map_err(|e| {
                        warn!("Failed to open xml file {}: {e}", p.display());
                    })?);
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
                        return Err(());
                    }
                    Ok(Doc::from_text(
                        &doc.extract_text(&doc.get_pages().into_keys().collect::<Vec<_>>())
                            .unwrap(),
                    ))
                }
                "html" => Ok(Doc::from_text(&nanohtml2text::html2text(
                    &std::fs::read_to_string(p).map_err(|e| {
                        warn!("Failed to extract text from html doc. {}: {e}", p.display())
                    })?,
                ))),
                _ => Err(()),
            },
            _ => Err(()),
        }
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
