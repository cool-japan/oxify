//! Vector search implementation
//!
//! Ported from OxiRS (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) OxiRS Contributors
//! Adapted for OxiFY (simplified for LLM workflow focus)
//! License: MIT OR Apache-2.0 (compatible with OxiRS)

use crate::filter::{Filter, Metadata};
use crate::simd;
use crate::types::{DistanceMetric, IndexStats, SearchConfig, SearchResult};
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// Vector search index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchIndex {
    config: SearchConfig,
    embeddings: HashMap<String, Vec<f32>>,
    entity_ids: Vec<String>,
    embedding_matrix: Option<Vec<Vec<f32>>>,
    dimensions: usize,
    is_built: bool,
    /// Metadata storage for filtered search
    metadata: HashMap<String, Metadata>,
}

impl VectorSearchIndex {
    /// Create new vector search index
    pub fn new(config: SearchConfig) -> Self {
        info!(
            "Initialized vector search index: metric={:?}, parallel={}",
            config.metric, config.parallel
        );

        Self {
            config,
            embeddings: HashMap::new(),
            entity_ids: Vec::new(),
            embedding_matrix: None,
            dimensions: 0,
            is_built: false,
            metadata: HashMap::new(),
        }
    }

    /// Build search index from embeddings
    pub fn build(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()> {
        if embeddings.is_empty() {
            return Err(anyhow!("Cannot build index from empty embeddings"));
        }

        info!(
            "Building vector search index for {} entities",
            embeddings.len()
        );

        // Store embeddings
        self.embeddings = embeddings.clone();
        self.entity_ids = embeddings.keys().cloned().collect();
        self.dimensions = embeddings
            .values()
            .next()
            .expect("invariant: embeddings non-empty")
            .len();

        // Build embedding matrix for efficient search
        let mut matrix = Vec::new();
        for entity_id in &self.entity_ids {
            let mut emb = self.embeddings[entity_id].clone();

            // Normalize if configured
            if self.config.normalize {
                Self::normalize_vector(&mut emb);
            }

            matrix.push(emb);
        }
        self.embedding_matrix = Some(matrix);

        self.is_built = true;

        info!("Vector search index built successfully");
        Ok(())
    }

    /// Search for K nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        if query.len() != self.dimensions {
            return Err(anyhow!(
                "Query dimension {} doesn't match index dimension {}",
                query.len(),
                self.dimensions
            ));
        }

        // Normalize query if configured
        let mut normalized_query = query.to_vec();
        if self.config.normalize {
            Self::normalize_vector(&mut normalized_query);
        }

        debug!("Searching for {} nearest neighbors", k);
        self.exact_search(&normalized_query, k)
    }

    /// Exact brute-force search
    fn exact_search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        let matrix = self
            .embedding_matrix
            .as_ref()
            .expect("invariant: embedding_matrix built during index build");

        // Compute distances/similarities to all entities
        let scores: Vec<(usize, f32)> = if self.config.parallel {
            (0..self.entity_ids.len())
                .into_par_iter()
                .map(|i| {
                    let score = self.compute_similarity(query, &matrix[i]);
                    (i, score)
                })
                .collect()
        } else {
            (0..self.entity_ids.len())
                .map(|i| {
                    let score = self.compute_similarity(query, &matrix[i]);
                    (i, score)
                })
                .collect()
        };

        // Sort by score descending
        let mut sorted_scores = scores;
        sorted_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top-K results
        let results: Vec<SearchResult> = sorted_scores
            .iter()
            .take(k.min(self.entity_ids.len()))
            .enumerate()
            .map(|(rank, &(idx, score))| SearchResult {
                entity_id: self.entity_ids[idx].clone(),
                score,
                distance: self.score_to_distance(score),
                rank: rank + 1,
            })
            .collect();

        debug!("Found {} results", results.len());
        Ok(results)
    }

    /// Batch search for multiple queries
    pub fn batch_search(&self, queries: &[Vec<f32>], k: usize) -> Result<Vec<Vec<SearchResult>>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        info!("Batch searching for {} queries", queries.len());

        let results: Vec<Vec<SearchResult>> = if self.config.parallel {
            queries
                .par_iter()
                .map(|query| self.search(query, k).unwrap_or_default())
                .collect()
        } else {
            queries
                .iter()
                .map(|query| self.search(query, k).unwrap_or_default())
                .collect()
        };

        Ok(results)
    }

    /// Compute similarity between two vectors
    ///
    /// Uses SIMD-optimized distance calculations for better performance.
    #[inline]
    fn compute_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        // Use SIMD-optimized implementations for hot path performance
        simd::compute_distance_simd(self.config.metric, a, b)
    }

    /// Convert score to distance
    #[inline]
    fn score_to_distance(&self, score: f32) -> f32 {
        match self.config.metric {
            DistanceMetric::Cosine => 1.0 - score, // Cosine distance
            DistanceMetric::Euclidean | DistanceMetric::Manhattan => -score, // Already negative
            DistanceMetric::DotProduct => -score,
        }
    }

    /// Normalize vector in-place (SIMD-optimized)
    #[inline]
    fn normalize_vector(vec: &mut [f32]) {
        simd::normalize_vector_simd(vec);
    }

    /// Get index statistics
    pub fn get_stats(&self) -> IndexStats {
        IndexStats {
            num_entities: self.entity_ids.len(),
            dimensions: self.dimensions,
            is_built: self.is_built,
            metric: self.config.metric,
        }
    }

    /// Find entities within a radius
    pub fn radius_search(&self, query: &[f32], radius: f32) -> Result<Vec<SearchResult>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        let all_results = self.search(query, self.entity_ids.len())?;

        Ok(all_results
            .into_iter()
            .filter(|r| r.distance <= radius)
            .collect())
    }

    /// Set metadata for an entity
    pub fn set_metadata(&mut self, entity_id: &str, metadata: Metadata) {
        self.metadata.insert(entity_id.to_string(), metadata);
    }

    /// Set metadata for multiple entities
    pub fn set_metadata_batch(&mut self, metadata_map: HashMap<String, Metadata>) {
        self.metadata.extend(metadata_map);
    }

    /// Get metadata for an entity
    pub fn get_metadata(&self, entity_id: &str) -> Option<&Metadata> {
        self.metadata.get(entity_id)
    }

    /// Search with metadata filtering (post-filtering)
    ///
    /// Post-filtering searches all vectors first, then filters results.
    /// This is efficient when the filter is selective.
    pub fn filtered_search(
        &self,
        query: &[f32],
        k: usize,
        filter: &Filter,
    ) -> Result<Vec<SearchResult>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        if filter.is_empty() {
            return self.search(query, k);
        }

        debug!(
            "Filtered search: k={}, filter conditions={}",
            k,
            filter.conditions().len()
        );

        // Post-filtering: search more candidates than needed, then filter
        // We search for all candidates and filter down
        let all_results = self.search(query, self.entity_ids.len())?;

        let filtered: Vec<SearchResult> = all_results
            .into_iter()
            .filter(|r| {
                self.metadata
                    .get(&r.entity_id)
                    .is_some_and(|m| filter.matches(m))
            })
            .take(k)
            .enumerate()
            .map(|(i, mut r)| {
                r.rank = i + 1; // Re-rank after filtering
                r
            })
            .collect();

        debug!("Filtered search returned {} results", filtered.len());
        Ok(filtered)
    }

    /// Search with pre-filtering (filter before computing distances)
    ///
    /// Pre-filtering only computes distances for entities matching the filter.
    /// This is efficient when the filter discards most entities.
    pub fn prefiltered_search(
        &self,
        query: &[f32],
        k: usize,
        filter: &Filter,
    ) -> Result<Vec<SearchResult>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        if query.len() != self.dimensions {
            return Err(anyhow!(
                "Query dimension {} doesn't match index dimension {}",
                query.len(),
                self.dimensions
            ));
        }

        if filter.is_empty() {
            return self.search(query, k);
        }

        debug!("Pre-filtered search: k={}", k);

        // Normalize query if configured
        let mut normalized_query = query.to_vec();
        if self.config.normalize {
            Self::normalize_vector(&mut normalized_query);
        }

        let matrix = self
            .embedding_matrix
            .as_ref()
            .expect("invariant: embedding_matrix built during index build");

        // Pre-filter: only compute distances for matching entities
        let matching_indices: Vec<usize> = (0..self.entity_ids.len())
            .filter(|&i| {
                self.metadata
                    .get(&self.entity_ids[i])
                    .is_some_and(|m| filter.matches(m))
            })
            .collect();

        if matching_indices.is_empty() {
            return Ok(Vec::new());
        }

        // Compute distances only for matching entities
        let scores: Vec<(usize, f32)> = if self.config.parallel {
            matching_indices
                .par_iter()
                .map(|&i| {
                    let score = self.compute_similarity(&normalized_query, &matrix[i]);
                    (i, score)
                })
                .collect()
        } else {
            matching_indices
                .iter()
                .map(|&i| {
                    let score = self.compute_similarity(&normalized_query, &matrix[i]);
                    (i, score)
                })
                .collect()
        };

        // Sort by score descending
        let mut sorted_scores = scores;
        sorted_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top-K results
        let results: Vec<SearchResult> = sorted_scores
            .iter()
            .take(k)
            .enumerate()
            .map(|(rank, &(idx, score))| SearchResult {
                entity_id: self.entity_ids[idx].clone(),
                score,
                distance: self.score_to_distance(score),
                rank: rank + 1,
            })
            .collect();

        debug!("Pre-filtered search returned {} results", results.len());
        Ok(results)
    }

    /// Add a single vector to the index incrementally (without full rebuild)
    ///
    /// This is more efficient than rebuilding when adding a few vectors.
    /// For bulk additions, consider using `add_vectors` or `build`.
    pub fn add_vector(&mut self, entity_id: String, mut embedding: Vec<f32>) -> Result<()> {
        if self.is_built && embedding.len() != self.dimensions {
            return Err(anyhow!(
                "Vector dimension {} doesn't match index dimension {}",
                embedding.len(),
                self.dimensions
            ));
        }

        // Check for duplicate entity ID
        if self.embeddings.contains_key(&entity_id) {
            return Err(anyhow!("Entity '{}' already exists in index", entity_id));
        }

        // If this is the first vector, set dimensions
        if !self.is_built {
            self.dimensions = embedding.len();
        }

        // Normalize if configured
        if self.config.normalize {
            Self::normalize_vector(&mut embedding);
        }

        // Add to embeddings map
        self.embeddings.insert(entity_id.clone(), embedding.clone());

        // Add to entity IDs
        self.entity_ids.push(entity_id);

        // Add to embedding matrix
        if let Some(ref mut matrix) = self.embedding_matrix {
            matrix.push(embedding);
        } else {
            self.embedding_matrix = Some(vec![embedding]);
        }

        self.is_built = true;
        debug!("Added vector to index (total: {})", self.entity_ids.len());
        Ok(())
    }

    /// Add multiple vectors to the index incrementally
    ///
    /// More efficient than calling `add_vector` multiple times.
    pub fn add_vectors(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        info!("Adding {} vectors to index", embeddings.len());

        // Validate all vectors before adding
        for (entity_id, embedding) in embeddings {
            if self.is_built && embedding.len() != self.dimensions {
                return Err(anyhow!(
                    "Vector dimension {} doesn't match index dimension {}",
                    embedding.len(),
                    self.dimensions
                ));
            }
            if self.embeddings.contains_key(entity_id) {
                return Err(anyhow!("Entity '{}' already exists in index", entity_id));
            }
        }

        // If this is the first batch, set dimensions
        if !self.is_built {
            self.dimensions = embeddings
                .values()
                .next()
                .expect("invariant: embeddings non-empty")
                .len();
        }

        // Add all vectors
        for (entity_id, embedding) in embeddings {
            let mut emb = embedding.clone();
            if self.config.normalize {
                Self::normalize_vector(&mut emb);
            }

            self.embeddings.insert(entity_id.clone(), emb.clone());
            self.entity_ids.push(entity_id.clone());

            if let Some(ref mut matrix) = self.embedding_matrix {
                matrix.push(emb);
            } else {
                self.embedding_matrix = Some(vec![emb]);
            }
        }

        self.is_built = true;
        info!(
            "Added vectors successfully (total: {})",
            self.entity_ids.len()
        );
        Ok(())
    }

    /// Remove a vector from the index
    pub fn remove_vector(&mut self, entity_id: &str) -> Result<()> {
        if !self.embeddings.contains_key(entity_id) {
            return Err(anyhow!("Entity '{}' not found in index", entity_id));
        }

        // Find the index of the entity
        let idx = self
            .entity_ids
            .iter()
            .position(|id| id == entity_id)
            .ok_or_else(|| anyhow!("Entity '{}' not found in entity_ids", entity_id))?;

        // Remove from embeddings map
        self.embeddings.remove(entity_id);

        // Remove from entity IDs
        self.entity_ids.remove(idx);

        // Remove from embedding matrix
        if let Some(ref mut matrix) = self.embedding_matrix {
            matrix.remove(idx);
        }

        // Remove metadata if exists
        self.metadata.remove(entity_id);

        // If index is now empty, mark as not built
        if self.embeddings.is_empty() {
            self.is_built = false;
            self.dimensions = 0;
        }

        debug!(
            "Removed vector from index (remaining: {})",
            self.entity_ids.len()
        );
        Ok(())
    }

    /// Remove multiple vectors from the index
    pub fn remove_vectors(&mut self, entity_ids: &[&str]) -> Result<()> {
        info!("Removing {} vectors from index", entity_ids.len());

        for entity_id in entity_ids {
            self.remove_vector(entity_id)?;
        }

        info!(
            "Removed vectors successfully (remaining: {})",
            self.entity_ids.len()
        );
        Ok(())
    }

    /// Update an existing vector in the index
    pub fn update_vector(&mut self, entity_id: &str, mut new_embedding: Vec<f32>) -> Result<()> {
        if !self.embeddings.contains_key(entity_id) {
            return Err(anyhow!("Entity '{}' not found in index", entity_id));
        }

        if new_embedding.len() != self.dimensions {
            return Err(anyhow!(
                "Vector dimension {} doesn't match index dimension {}",
                new_embedding.len(),
                self.dimensions
            ));
        }

        // Normalize if configured
        if self.config.normalize {
            Self::normalize_vector(&mut new_embedding);
        }

        // Find the index of the entity
        let idx = self
            .entity_ids
            .iter()
            .position(|id| id == entity_id)
            .ok_or_else(|| anyhow!("Entity '{}' not found in entity_ids", entity_id))?;

        // Update embeddings map
        self.embeddings
            .insert(entity_id.to_string(), new_embedding.clone());

        // Update embedding matrix
        if let Some(ref mut matrix) = self.embedding_matrix {
            matrix[idx] = new_embedding;
        }

        debug!("Updated vector in index: {}", entity_id);
        Ok(())
    }

    /// Clear all vectors from the index
    pub fn clear(&mut self) {
        self.embeddings.clear();
        self.entity_ids.clear();
        self.embedding_matrix = None;
        self.metadata.clear();
        self.dimensions = 0;
        self.is_built = false;
        info!("Index cleared");
    }

    /// Get the number of vectors in the index
    #[inline]
    pub fn len(&self) -> usize {
        self.entity_ids.len()
    }

    /// Check if the index is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entity_ids.is_empty()
    }

    /// Check if a vector exists in the index
    #[inline]
    pub fn contains(&self, entity_id: &str) -> bool {
        self.embeddings.contains_key(entity_id)
    }

    /// Get a vector from the index
    #[inline]
    pub fn get_vector(&self, entity_id: &str) -> Option<&Vec<f32>> {
        self.embeddings.get(entity_id)
    }

    /// Merge another index into this one
    ///
    /// Combines vectors from another index, handling duplicates and dimension checks.
    /// By default, duplicates are skipped. Use `overwrite_duplicates` to update existing vectors.
    pub fn merge(&mut self, other: &VectorSearchIndex, overwrite_duplicates: bool) -> Result<()> {
        if !other.is_built {
            return Err(anyhow!("Cannot merge from an unbuilt index"));
        }

        // If this index is empty, we can directly adopt the other index's dimensions
        if !self.is_built && other.is_built {
            self.dimensions = other.dimensions;
        }

        // Check dimension compatibility if both are built
        if self.is_built && other.is_built && self.dimensions != other.dimensions {
            return Err(anyhow!(
                "Cannot merge indexes with different dimensions: {} vs {}",
                self.dimensions,
                other.dimensions
            ));
        }

        info!("Merging index with {} vectors", other.entity_ids.len());

        let mut added = 0;
        let mut updated = 0;
        let mut skipped = 0;

        for entity_id in &other.entity_ids {
            let embedding = &other.embeddings[entity_id];

            if self.embeddings.contains_key(entity_id) {
                if overwrite_duplicates {
                    // Update existing vector
                    self.update_vector(entity_id, embedding.clone())?;
                    updated += 1;
                } else {
                    // Skip duplicate
                    skipped += 1;
                }
            } else {
                // Add new vector
                self.add_vector(entity_id.clone(), embedding.clone())?;
                added += 1;
            }

            // Merge metadata if exists
            if let Some(metadata) = other.metadata.get(entity_id) {
                self.metadata.insert(entity_id.clone(), metadata.clone());
            }
        }

        info!(
            "Merge complete: added={}, updated={}, skipped={}",
            added, updated, skipped
        );
        Ok(())
    }

    /// Create a new index by merging multiple indexes
    ///
    /// This is more efficient than merging sequentially when combining many indexes.
    pub fn merge_multiple(indexes: &[&VectorSearchIndex]) -> Result<VectorSearchIndex> {
        if indexes.is_empty() {
            return Err(anyhow!("Cannot merge zero indexes"));
        }

        // Find a built index to get config and dimensions
        let first_built = indexes
            .iter()
            .find(|idx| idx.is_built)
            .ok_or_else(|| anyhow!("At least one index must be built"))?;

        let dimensions = first_built.dimensions;
        let config = first_built.config.clone();

        // Validate all indexes have compatible dimensions
        for (i, index) in indexes.iter().enumerate() {
            if index.is_built && index.dimensions != dimensions {
                return Err(anyhow!(
                    "Index {} has incompatible dimensions: {} vs {}",
                    i,
                    index.dimensions,
                    dimensions
                ));
            }
        }

        info!("Merging {} indexes into one", indexes.len());

        // Collect all embeddings
        let mut all_embeddings = HashMap::new();
        let mut all_metadata = HashMap::new();

        for index in indexes {
            for (entity_id, embedding) in &index.embeddings {
                // Last index wins for duplicates
                all_embeddings.insert(entity_id.clone(), embedding.clone());
            }

            for (entity_id, metadata) in &index.metadata {
                all_metadata.insert(entity_id.clone(), metadata.clone());
            }
        }

        // Build merged index
        let mut merged = VectorSearchIndex::new(config);
        merged.build(&all_embeddings)?;
        merged.metadata = all_metadata;

        info!("Merged index contains {} vectors", merged.len());
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::FilterValue;

    fn create_test_embeddings() -> HashMap<String, Vec<f32>> {
        let mut embeddings = HashMap::new();

        // Create some test embeddings
        embeddings.insert("entity1".to_string(), vec![1.0, 0.0, 0.0]);
        embeddings.insert("entity2".to_string(), vec![0.9, 0.1, 0.0]);
        embeddings.insert("entity3".to_string(), vec![0.0, 1.0, 0.0]);
        embeddings.insert("entity4".to_string(), vec![0.0, 0.0, 1.0]);
        embeddings.insert("entity5".to_string(), vec![0.7, 0.7, 0.0]);

        embeddings
    }

    fn create_test_metadata() -> HashMap<String, Metadata> {
        let mut metadata = HashMap::new();

        let mut m1 = HashMap::new();
        m1.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );
        m1.insert("year".to_string(), FilterValue::Int(2023));
        metadata.insert("entity1".to_string(), m1);

        let mut m2 = HashMap::new();
        m2.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );
        m2.insert("year".to_string(), FilterValue::Int(2022));
        metadata.insert("entity2".to_string(), m2);

        let mut m3 = HashMap::new();
        m3.insert("type".to_string(), FilterValue::String("book".to_string()));
        m3.insert("year".to_string(), FilterValue::Int(2023));
        metadata.insert("entity3".to_string(), m3);

        let mut m4 = HashMap::new();
        m4.insert("type".to_string(), FilterValue::String("book".to_string()));
        m4.insert("year".to_string(), FilterValue::Int(2021));
        metadata.insert("entity4".to_string(), m4);

        let mut m5 = HashMap::new();
        m5.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );
        m5.insert("year".to_string(), FilterValue::Int(2024));
        metadata.insert("entity5".to_string(), m5);

        metadata
    }

    #[test]
    fn test_index_creation() {
        let config = SearchConfig::default();
        let index = VectorSearchIndex::new(config);

        assert!(!index.is_built);
        assert_eq!(index.dimensions, 0);
    }

    #[test]
    fn test_index_building() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());

        assert!(index.build(&embeddings).is_ok());
        assert!(index.is_built);
        assert_eq!(index.dimensions, 3);
        assert_eq!(index.entity_ids.len(), 5);
    }

    #[test]
    fn test_search() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        // Search for entity similar to entity1
        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 3).unwrap();

        assert_eq!(results.len(), 3);
        // entity1 should be the closest (or entity2 which is very similar)
        assert!(results[0].entity_id == "entity1" || results[0].entity_id == "entity2");
    }

    #[test]
    fn test_batch_search() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        let queries = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        let results = index.batch_search(&queries, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].len(), 2);
        assert_eq!(results[1].len(), 2);
    }

    #[test]
    fn test_get_stats() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.num_entities, 5);
        assert_eq!(stats.dimensions, 3);
        assert!(stats.is_built);
        assert_eq!(stats.metric, DistanceMetric::Cosine);
    }

    #[test]
    fn test_set_and_get_metadata() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );

        index.set_metadata("entity1", metadata.clone());

        let retrieved = index.get_metadata("entity1");
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved.unwrap().get("type"),
            Some(&FilterValue::String("article".to_string()))
        );
    }

    #[test]
    fn test_filtered_search() {
        let embeddings = create_test_embeddings();
        let metadata = create_test_metadata();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();
        index.set_metadata_batch(metadata);

        // Filter for articles only
        let filter = Filter::new().eq("type", "article");
        let query = vec![1.0, 0.0, 0.0];
        let results = index.filtered_search(&query, 5, &filter).unwrap();

        // Should only return articles (entity1, entity2, entity5)
        assert_eq!(results.len(), 3);
        for result in &results {
            let meta = index.get_metadata(&result.entity_id).unwrap();
            assert_eq!(
                meta.get("type"),
                Some(&FilterValue::String("article".to_string()))
            );
        }
    }

    #[test]
    fn test_filtered_search_with_year() {
        let embeddings = create_test_embeddings();
        let metadata = create_test_metadata();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();
        index.set_metadata_batch(metadata);

        // Filter for year >= 2023
        let filter = Filter::new().gte("year", 2023i64);
        let query = vec![1.0, 0.0, 0.0];
        let results = index.filtered_search(&query, 5, &filter).unwrap();

        // Should return entity1 (2023), entity3 (2023), entity5 (2024)
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_prefiltered_search() {
        let embeddings = create_test_embeddings();
        let metadata = create_test_metadata();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();
        index.set_metadata_batch(metadata);

        // Filter for books only
        let filter = Filter::new().eq("type", "book");
        let query = vec![0.0, 1.0, 0.0]; // Similar to entity3 (book)
        let results = index.prefiltered_search(&query, 5, &filter).unwrap();

        // Should only return books (entity3, entity4)
        assert_eq!(results.len(), 2);
        for result in &results {
            let meta = index.get_metadata(&result.entity_id).unwrap();
            assert_eq!(
                meta.get("type"),
                Some(&FilterValue::String("book".to_string()))
            );
        }
    }

    #[test]
    fn test_filtered_search_empty_filter() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        // Empty filter should return all results
        let filter = Filter::new();
        let query = vec![1.0, 0.0, 0.0];
        let results = index.filtered_search(&query, 3, &filter).unwrap();

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_filtered_search_no_matches() {
        let embeddings = create_test_embeddings();
        let metadata = create_test_metadata();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();
        index.set_metadata_batch(metadata);

        // Filter for non-existent type
        let filter = Filter::new().eq("type", "journal");
        let query = vec![1.0, 0.0, 0.0];
        let results = index.filtered_search(&query, 5, &filter).unwrap();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_add_vector() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        let initial_len = index.len();

        // Add a new vector
        let result = index.add_vector("entity6".to_string(), vec![0.5, 0.5, 0.5]);
        assert!(result.is_ok());
        assert_eq!(index.len(), initial_len + 1);
        assert!(index.contains("entity6"));

        // Verify we can search with the new vector
        let query = vec![0.5, 0.5, 0.5];
        let results = index.search(&query, 1).unwrap();
        assert_eq!(results[0].entity_id, "entity6");
    }

    #[test]
    fn test_add_vector_duplicate() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        // Try to add duplicate entity
        let result = index.add_vector("entity1".to_string(), vec![0.5, 0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_vector_dimension_mismatch() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        // Try to add vector with wrong dimension
        let result = index.add_vector("entity6".to_string(), vec![0.5, 0.5]); // Wrong dimension
        assert!(result.is_err());
    }

    #[test]
    fn test_add_vectors() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        let initial_len = index.len();

        // Add multiple vectors
        let mut new_embeddings = HashMap::new();
        new_embeddings.insert("entity6".to_string(), vec![0.5, 0.5, 0.5]);
        new_embeddings.insert("entity7".to_string(), vec![0.6, 0.6, 0.6]);

        let result = index.add_vectors(&new_embeddings);
        assert!(result.is_ok());
        assert_eq!(index.len(), initial_len + 2);
        assert!(index.contains("entity6"));
        assert!(index.contains("entity7"));
    }

    #[test]
    fn test_remove_vector() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        let initial_len = index.len();

        // Remove a vector
        let result = index.remove_vector("entity1");
        assert!(result.is_ok());
        assert_eq!(index.len(), initial_len - 1);
        assert!(!index.contains("entity1"));

        // Verify search still works
        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 5).unwrap();
        assert!(!results.iter().any(|r| r.entity_id == "entity1"));
    }

    #[test]
    fn test_remove_vector_not_found() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        // Try to remove non-existent vector
        let result = index.remove_vector("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_vectors() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        let initial_len = index.len();

        // Remove multiple vectors
        let result = index.remove_vectors(&["entity1", "entity2"]);
        assert!(result.is_ok());
        assert_eq!(index.len(), initial_len - 2);
        assert!(!index.contains("entity1"));
        assert!(!index.contains("entity2"));
    }

    #[test]
    fn test_update_vector() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        // Update a vector
        let new_embedding = vec![0.9, 0.9, 0.9];
        let result = index.update_vector("entity1", new_embedding.clone());
        assert!(result.is_ok());

        // Verify the vector was updated
        let retrieved = index.get_vector("entity1").unwrap();
        // Account for normalization
        let mut expected = new_embedding.clone();
        VectorSearchIndex::normalize_vector(&mut expected);
        assert_eq!(retrieved.len(), expected.len());
        for (a, b) in retrieved.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_update_vector_not_found() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        // Try to update non-existent vector
        let result = index.update_vector("nonexistent", vec![0.5, 0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_vector_dimension_mismatch() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        // Try to update with wrong dimension
        let result = index.update_vector("entity1", vec![0.5, 0.5]); // Wrong dimension
        assert!(result.is_err());
    }

    #[test]
    fn test_clear() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        assert!(!index.is_empty());
        assert!(index.is_built);

        index.clear();

        assert_eq!(index.len(), 0);
        assert!(index.is_empty());
        assert!(!index.is_built);
        assert_eq!(index.dimensions, 0);
    }

    #[test]
    fn test_get_vector() {
        let embeddings = create_test_embeddings();
        let mut index = VectorSearchIndex::new(SearchConfig::default());
        index.build(&embeddings).unwrap();

        // Get an existing vector
        let vector = index.get_vector("entity1");
        assert!(vector.is_some());

        // Try to get non-existent vector
        let vector = index.get_vector("nonexistent");
        assert!(vector.is_none());
    }

    #[test]
    fn test_incremental_build() {
        let mut index = VectorSearchIndex::new(SearchConfig::default());

        // Build index incrementally
        index
            .add_vector("entity1".to_string(), vec![1.0, 0.0, 0.0])
            .unwrap();
        index
            .add_vector("entity2".to_string(), vec![0.0, 1.0, 0.0])
            .unwrap();
        index
            .add_vector("entity3".to_string(), vec![0.0, 0.0, 1.0])
            .unwrap();

        assert_eq!(index.len(), 3);
        assert!(index.is_built);
        assert_eq!(index.dimensions, 3);

        // Verify search works
        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 1).unwrap();
        assert_eq!(results[0].entity_id, "entity1");
    }

    #[test]
    fn test_merge_indexes() {
        let embeddings1 = create_test_embeddings();
        let mut index1 = VectorSearchIndex::new(SearchConfig::default());
        index1.build(&embeddings1).unwrap();

        // Create second index with different entities
        let mut embeddings2 = HashMap::new();
        embeddings2.insert("entity6".to_string(), vec![0.6, 0.6, 0.0]);
        embeddings2.insert("entity7".to_string(), vec![0.7, 0.7, 0.0]);

        let mut index2 = VectorSearchIndex::new(SearchConfig::default());
        index2.build(&embeddings2).unwrap();

        let initial_len = index1.len();

        // Merge index2 into index1
        let result = index1.merge(&index2, false);
        assert!(result.is_ok());
        assert_eq!(index1.len(), initial_len + 2);
        assert!(index1.contains("entity6"));
        assert!(index1.contains("entity7"));
    }

    #[test]
    fn test_merge_with_duplicates_skip() {
        let embeddings1 = create_test_embeddings();
        let mut index1 = VectorSearchIndex::new(SearchConfig::default());
        index1.build(&embeddings1).unwrap();

        // Create second index with overlapping entities
        let mut embeddings2 = HashMap::new();
        embeddings2.insert("entity1".to_string(), vec![0.9, 0.9, 0.9]); // Duplicate
        embeddings2.insert("entity6".to_string(), vec![0.6, 0.6, 0.0]); // New

        let mut index2 = VectorSearchIndex::new(SearchConfig::default());
        index2.build(&embeddings2).unwrap();

        let initial_len = index1.len();

        // Merge with skip duplicates (default)
        let result = index1.merge(&index2, false);
        assert!(result.is_ok());
        assert_eq!(index1.len(), initial_len + 1); // Only entity6 added
    }

    #[test]
    fn test_merge_with_duplicates_overwrite() {
        let embeddings1 = create_test_embeddings();
        let mut index1 = VectorSearchIndex::new(SearchConfig::default());
        index1.build(&embeddings1).unwrap();

        // Create second index with overlapping entities
        let mut embeddings2 = HashMap::new();
        embeddings2.insert("entity1".to_string(), vec![0.9, 0.9, 0.9]); // Duplicate

        let mut index2 = VectorSearchIndex::new(SearchConfig::default());
        index2.build(&embeddings2).unwrap();

        let initial_len = index1.len();

        // Merge with overwrite duplicates
        let result = index1.merge(&index2, true);
        assert!(result.is_ok());
        assert_eq!(index1.len(), initial_len); // Same count, but entity1 updated

        // Verify entity1 was updated
        let vector = index1.get_vector("entity1").unwrap();
        // Account for normalization
        let mut expected = vec![0.9, 0.9, 0.9];
        VectorSearchIndex::normalize_vector(&mut expected);
        for (a, b) in vector.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_merge_dimension_mismatch() {
        let embeddings1 = create_test_embeddings(); // 3D
        let mut index1 = VectorSearchIndex::new(SearchConfig::default());
        index1.build(&embeddings1).unwrap();

        // Create index with different dimensions
        let mut embeddings2 = HashMap::new();
        embeddings2.insert("entity6".to_string(), vec![0.6, 0.6]); // 2D

        let mut index2 = VectorSearchIndex::new(SearchConfig::default());
        index2.build(&embeddings2).unwrap();

        // Merge should fail due to dimension mismatch
        let result = index1.merge(&index2, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_multiple_indexes() {
        // Create three indexes
        let mut index1 = VectorSearchIndex::new(SearchConfig::default());
        let mut embeddings1 = HashMap::new();
        embeddings1.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        embeddings1.insert("doc2".to_string(), vec![0.9, 0.1, 0.0]);
        index1.build(&embeddings1).unwrap();

        let mut index2 = VectorSearchIndex::new(SearchConfig::default());
        let mut embeddings2 = HashMap::new();
        embeddings2.insert("doc3".to_string(), vec![0.0, 1.0, 0.0]);
        embeddings2.insert("doc4".to_string(), vec![0.1, 0.9, 0.0]);
        index2.build(&embeddings2).unwrap();

        let mut index3 = VectorSearchIndex::new(SearchConfig::default());
        let mut embeddings3 = HashMap::new();
        embeddings3.insert("doc5".to_string(), vec![0.0, 0.0, 1.0]);
        index3.build(&embeddings3).unwrap();

        // Merge all three
        let merged = VectorSearchIndex::merge_multiple(&[&index1, &index2, &index3]);
        assert!(merged.is_ok());

        let merged = merged.unwrap();
        assert_eq!(merged.len(), 5);
        assert!(merged.contains("doc1"));
        assert!(merged.contains("doc3"));
        assert!(merged.contains("doc5"));

        // Verify search works on merged index
        let query = vec![1.0, 0.0, 0.0];
        let results = merged.search(&query, 2).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_merge_multiple_empty() {
        let result = VectorSearchIndex::merge_multiple(&[]);
        assert!(result.is_err());
    }

    // Property-based tests
    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        // Strategy for generating valid vectors
        fn vector_strategy(dim: usize) -> impl Strategy<Value = Vec<f32>> {
            proptest::collection::vec(-1.0f32..1.0f32, dim..=dim)
        }

        // Strategy for generating embeddings
        fn embeddings_strategy(
            count: usize,
            dim: usize,
        ) -> impl Strategy<Value = HashMap<String, Vec<f32>>> {
            proptest::collection::vec(
                (
                    proptest::string::string_regex("[a-z0-9]{5,10}").unwrap(),
                    vector_strategy(dim),
                ),
                count..=count,
            )
            .prop_map(|pairs| pairs.into_iter().collect())
        }

        proptest! {
            /// Property: Search should never panic with valid inputs
            #[test]
            fn prop_search_never_panics(
                embeddings in embeddings_strategy(10, 128),
                query in vector_strategy(128),
                k in 1usize..10
            ) {
                let mut index = VectorSearchIndex::new(SearchConfig::default());
                index.build(&embeddings).unwrap();
                let _ = index.search(&query, k);
            }

            /// Property: Top-k results should always have k or fewer elements
            #[test]
            fn prop_search_respects_k(
                embeddings in embeddings_strategy(20, 64),
                query in vector_strategy(64),
                k in 1usize..15
            ) {
                let mut index = VectorSearchIndex::new(SearchConfig::default());
                index.build(&embeddings).unwrap();
                let results = index.search(&query, k).unwrap();
                prop_assert!(results.len() <= k);
                prop_assert!(results.len() <= embeddings.len());
            }

            /// Property: Search results should be sorted by score (descending)
            #[test]
            fn prop_search_results_sorted(
                embeddings in embeddings_strategy(15, 32),
                query in vector_strategy(32),
                k in 2usize..10
            ) {
                let mut index = VectorSearchIndex::new(SearchConfig::default());
                index.build(&embeddings).unwrap();
                let results = index.search(&query, k).unwrap();

                for i in 1..results.len() {
                    prop_assert!(results[i-1].score >= results[i].score,
                        "Results not sorted: {} < {}", results[i-1].score, results[i].score);
                }
            }

            /// Property: Ranks should be consecutive starting from 1
            #[test]
            fn prop_search_ranks_consecutive(
                embeddings in embeddings_strategy(10, 16),
                query in vector_strategy(16),
                k in 1usize..8
            ) {
                let mut index = VectorSearchIndex::new(SearchConfig::default());
                index.build(&embeddings).unwrap();
                let results = index.search(&query, k).unwrap();

                for (i, result) in results.iter().enumerate() {
                    prop_assert_eq!(result.rank, i + 1);
                }
            }

            /// Property: Searching for the same query twice should yield same results
            #[test]
            fn prop_search_deterministic(
                embeddings in embeddings_strategy(12, 48),
                query in vector_strategy(48),
                k in 1usize..10
            ) {
                let mut index = VectorSearchIndex::new(SearchConfig::default());
                index.build(&embeddings).unwrap();

                let results1 = index.search(&query, k).unwrap();
                let results2 = index.search(&query, k).unwrap();

                prop_assert_eq!(results1.len(), results2.len());
                for (r1, r2) in results1.iter().zip(results2.iter()) {
                    prop_assert_eq!(&r1.entity_id, &r2.entity_id);
                    prop_assert!((r1.score - r2.score).abs() < 1e-6);
                }
            }

            /// Property: Batch search should return one result set per query
            #[test]
            fn prop_batch_search_count(
                embeddings in embeddings_strategy(10, 32),
                num_queries in 1usize..5,
                k in 1usize..8
            ) {
                let mut index = VectorSearchIndex::new(SearchConfig::default());
                index.build(&embeddings).unwrap();

                let queries: Vec<Vec<f32>> = (0..num_queries)
                    .map(|i| vec![i as f32; 32])
                    .collect();

                let results = index.batch_search(&queries, k).unwrap();
                prop_assert_eq!(results.len(), num_queries);

                for result_set in results {
                    prop_assert!(result_set.len() <= k);
                }
            }

            /// Property: Distance should always be non-negative
            #[test]
            fn prop_distance_non_negative(
                embeddings in embeddings_strategy(8, 24),
                query in vector_strategy(24),
                k in 1usize..6
            ) {
                let mut index = VectorSearchIndex::new(SearchConfig::default());
                index.build(&embeddings).unwrap();
                let results = index.search(&query, k).unwrap();

                for result in results {
                    prop_assert!(result.distance >= 0.0,
                        "Negative distance: {}", result.distance);
                }
            }

            /// Property: Empty embeddings should fail to build
            #[test]
            fn prop_empty_embeddings_fail(_dim in 1usize..128) {
                let embeddings: HashMap<String, Vec<f32>> = HashMap::new();
                let mut index = VectorSearchIndex::new(SearchConfig::default());
                prop_assert!(index.build(&embeddings).is_err());
            }

            /// Property: Query dimension mismatch should fail
            #[test]
            fn prop_dimension_mismatch_fail(
                embeddings in embeddings_strategy(5, 64),
                wrong_dim in 1usize..128
            ) {
                prop_assume!(wrong_dim != 64); // Ensure dimension is wrong

                let mut index = VectorSearchIndex::new(SearchConfig::default());
                index.build(&embeddings).unwrap();

                let query = vec![0.0; wrong_dim];
                let result = index.search(&query, 3);
                prop_assert!(result.is_err());
            }

            /// Property: Normalized vectors should have unit norm
            #[test]
            fn prop_normalize_unit_norm(mut vec in vector_strategy(128)) {
                VectorSearchIndex::normalize_vector(&mut vec);
                let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                prop_assert!((norm - 1.0).abs() < 1e-5, "Norm: {}", norm);
            }
        }
    }
}
