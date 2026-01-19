//! BM25 keyword search implementation
//!
//! BM25 (Best Matching 25) is a probabilistic ranking function used for keyword search.
//! It's particularly effective when combined with semantic vector search (hybrid search).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// BM25 parameters
#[derive(Debug, Clone)]
pub struct Bm25Params {
    /// Term frequency saturation parameter (typical: 1.2-2.0)
    pub k1: f32,
    /// Document length normalization parameter (typical: 0.75)
    pub b: f32,
}

impl Default for Bm25Params {
    fn default() -> Self {
        Self { k1: 1.5, b: 0.75 }
    }
}

/// A document in the BM25 index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25Document {
    pub id: String,
    pub text: String,
    pub metadata: serde_json::Value,
}

/// BM25 search index
#[derive(Debug, Clone)]
pub struct Bm25Index {
    documents: Vec<Bm25Document>,
    /// Inverted index: term -> list of (doc_id, term_frequency)
    inverted_index: HashMap<String, Vec<(usize, usize)>>,
    /// Document lengths (number of terms)
    doc_lengths: Vec<usize>,
    /// Average document length
    avg_doc_length: f32,
    /// Number of documents containing each term
    doc_freq: HashMap<String, usize>,
    /// Total number of documents
    num_docs: usize,
    /// BM25 parameters
    params: Bm25Params,
}

impl Bm25Index {
    /// Create a new BM25 index from documents
    pub fn new(documents: Vec<Bm25Document>) -> Self {
        Self::with_params(documents, Bm25Params::default())
    }

    /// Create a new BM25 index with custom parameters
    pub fn with_params(documents: Vec<Bm25Document>, params: Bm25Params) -> Self {
        let num_docs = documents.len();
        let mut inverted_index: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
        let mut doc_lengths = Vec::with_capacity(num_docs);
        let mut doc_freq: HashMap<String, usize> = HashMap::new();

        // Build inverted index
        for (doc_id, doc) in documents.iter().enumerate() {
            let terms = tokenize(&doc.text);
            doc_lengths.push(terms.len());

            // Count term frequencies in this document
            let mut term_freq: HashMap<String, usize> = HashMap::new();
            for term in &terms {
                *term_freq.entry(term.clone()).or_insert(0) += 1;
            }

            // Update inverted index and document frequency
            for (term, freq) in term_freq {
                inverted_index
                    .entry(term.clone())
                    .or_default()
                    .push((doc_id, freq));

                // Count documents containing this term (only once per document)
                *doc_freq.entry(term).or_insert(0) += 1;
            }
        }

        let avg_doc_length = if num_docs > 0 {
            doc_lengths.iter().sum::<usize>() as f32 / num_docs as f32
        } else {
            0.0
        };

        Self {
            documents,
            inverted_index,
            doc_lengths,
            avg_doc_length,
            doc_freq,
            num_docs,
            params,
        }
    }

    /// Search the index with a query
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, f32)> {
        if self.num_docs == 0 {
            return Vec::new();
        }

        let query_terms = tokenize(query);
        let mut scores: HashMap<usize, f32> = HashMap::new();

        for term in query_terms {
            if let Some(postings) = self.inverted_index.get(&term) {
                let idf = self.idf(&term);

                for &(doc_id, term_freq) in postings {
                    let doc_length = self.doc_lengths[doc_id];
                    let score = self.bm25_score(term_freq, doc_length, idf);
                    *scores.entry(doc_id).or_insert(0.0) += score;
                }
            }
        }

        // Sort by score descending
        let mut results: Vec<(usize, f32)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top-k results with document IDs
        results
            .into_iter()
            .take(top_k)
            .map(|(doc_id, score)| (self.documents[doc_id].id.clone(), score))
            .collect()
    }

    /// Calculate IDF (Inverse Document Frequency) for a term
    fn idf(&self, term: &str) -> f32 {
        let doc_freq = self.doc_freq.get(term).copied().unwrap_or(0);
        if doc_freq == 0 {
            return 0.0;
        }

        // IDF = ln(1 + (N - df + 0.5) / (df + 0.5))
        let n = self.num_docs as f32;
        let df = doc_freq as f32;
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    /// Calculate BM25 score for a term in a document
    fn bm25_score(&self, term_freq: usize, doc_length: usize, idf: f32) -> f32 {
        let tf = term_freq as f32;
        let dl = doc_length as f32;
        let avgdl = self.avg_doc_length;

        // BM25 = IDF * (tf * (k1 + 1)) / (tf + k1 * (1 - b + b * (dl / avgdl)))
        let numerator = tf * (self.params.k1 + 1.0);
        let denominator =
            tf + self.params.k1 * (1.0 - self.params.b + self.params.b * (dl / avgdl));

        idf * (numerator / denominator)
    }

    /// Get document by ID
    pub fn get_document(&self, id: &str) -> Option<&Bm25Document> {
        self.documents.iter().find(|doc| doc.id == id)
    }
}

/// Simple tokenizer (lowercase + split on non-alphanumeric)
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let text = "Hello, World! This is a test.";
        let tokens = tokenize(text);
        assert_eq!(tokens, vec!["hello", "world", "this", "is", "a", "test"]);
    }

    #[test]
    fn test_bm25_search() {
        let docs = vec![
            Bm25Document {
                id: "doc1".to_string(),
                text: "The quick brown fox jumps over the lazy dog".to_string(),
                metadata: serde_json::Value::Null,
            },
            Bm25Document {
                id: "doc2".to_string(),
                text: "A quick brown dog runs in the park".to_string(),
                metadata: serde_json::Value::Null,
            },
            Bm25Document {
                id: "doc3".to_string(),
                text: "The lazy cat sleeps all day long".to_string(),
                metadata: serde_json::Value::Null,
            },
        ];

        let index = Bm25Index::new(docs);
        let results = index.search("quick brown dog", 2);

        assert_eq!(results.len(), 2);
        // doc2 should rank higher because it has more query terms
        assert_eq!(results[0].0, "doc2");
        assert!(results[0].1 > 0.0);
    }

    #[test]
    fn test_empty_index() {
        let index = Bm25Index::new(vec![]);
        let results = index.search("test", 10);
        assert_eq!(results.len(), 0);
    }
}
