pub mod tfidf;

pub use tfidf::TfIdf;

pub trait SearchEngine {
    fn query(&mut self, query: &[&str]) -> Vec<&str>;
    fn add_dir(&mut self, dir_path: &std::path::Path);
}


