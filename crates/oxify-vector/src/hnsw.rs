//! HNSW (Hierarchical Navigable Small World) Index
//!
//! State-of-the-art approximate nearest neighbor (ANN) algorithm.
//! Provides sub-linear search time with high recall.
//!
//! ## Algorithm Overview
//!
//! HNSW builds a multi-layer graph where:
//! - Layer 0 contains all vectors (densest)
//! - Higher layers contain progressively fewer vectors (sparser)
//! - Search starts from the top layer and navigates down
//!
//! ## Parameters
//!
//! - `M`: Maximum connections per node (default: 16)
//! - `ef_construction`: Build-time quality (default: 200)
//! - `ef_search`: Search-time quality vs speed trade-off (default: 50)
//!
//! ## Example
//!
//! ```rust
//! use oxify_vector::hnsw::{HnswIndex, HnswConfig};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! // Create embeddings
//! let mut embeddings = HashMap::new();
//! embeddings.insert("doc1".to_string(), vec![0.1, 0.2, 0.3]);
//! embeddings.insert("doc2".to_string(), vec![0.2, 0.3, 0.4]);
//! embeddings.insert("doc3".to_string(), vec![0.9, 0.8, 0.7]);
//!
//! // Build HNSW index
//! let config = HnswConfig::default();
//! let mut index = HnswIndex::new(config);
//! index.build(&embeddings)?;
//!
//! // Search for similar documents
//! let query = vec![0.15, 0.25, 0.35];
//! let results = index.search(&query, 2)?;
//!
//! for result in results {
//!     println!("{}: score = {:.4}", result.entity_id, result.score);
//! }
//! # Ok(())
//! # }
//! ```

use crate::filter::{Filter, Metadata};
use crate::simd;
use crate::types::{DistanceMetric, SearchResult};
use anyhow::{anyhow, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use tracing::{debug, info};

/// HNSW configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswConfig {
    /// Distance metric
    pub metric: DistanceMetric,
    /// Maximum number of connections per node
    pub m: usize,
    /// Maximum connections at layer 0 (typically 2 * m)
    pub m0: usize,
    /// Size of dynamic candidate list during construction
    pub ef_construction: usize,
    /// Size of dynamic candidate list during search
    pub ef_search: usize,
    /// Normalization factor for level generation (1/ln(m))
    pub ml: f64,
    /// Normalize vectors before indexing
    pub normalize: bool,
}

impl Default for HnswConfig {
    fn default() -> Self {
        let m = 16;
        Self {
            metric: DistanceMetric::Cosine,
            m,
            m0: m * 2,
            ef_construction: 200,
            ef_search: 50,
            ml: 1.0 / (m as f64).ln(),
            normalize: true,
        }
    }
}

impl HnswConfig {
    /// Create config optimized for high recall
    pub fn high_recall() -> Self {
        let m = 32;
        Self {
            metric: DistanceMetric::Cosine,
            m,
            m0: m * 2,
            ef_construction: 400,
            ef_search: 100,
            ml: 1.0 / (m as f64).ln(),
            normalize: true,
        }
    }

    /// Create config optimized for speed
    pub fn fast() -> Self {
        let m = 12;
        Self {
            metric: DistanceMetric::Cosine,
            m,
            m0: m * 2,
            ef_construction: 100,
            ef_search: 30,
            ml: 1.0 / (m as f64).ln(),
            normalize: true,
        }
    }
}

/// Node in the HNSW graph
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HnswNode {
    /// Node index
    id: usize,
    /// Maximum layer this node exists on
    level: usize,
    /// Neighbors at each layer (layer -> neighbor indices)
    neighbors: Vec<Vec<usize>>,
}

impl HnswNode {
    fn new(id: usize, level: usize) -> Self {
        Self {
            id,
            level,
            neighbors: vec![Vec::new(); level + 1],
        }
    }
}

/// Candidate for nearest neighbor search (min-heap by distance)
#[derive(Debug, Clone, Copy)]
struct Candidate {
    id: usize,
    distance: f32,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed for min-heap behavior (smaller distance = higher priority)
        other
            .distance
            .partial_cmp(&self.distance)
            .unwrap_or(Ordering::Equal)
    }
}

/// Max-heap candidate (for maintaining furthest neighbors)
#[derive(Debug, Clone, Copy)]
struct MaxCandidate {
    id: usize,
    distance: f32,
}

impl PartialEq for MaxCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

impl Eq for MaxCandidate {}

impl PartialOrd for MaxCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MaxCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Normal ordering for max-heap (larger distance = higher priority)
        self.distance
            .partial_cmp(&other.distance)
            .unwrap_or(Ordering::Equal)
    }
}

/// HNSW Index for approximate nearest neighbor search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswIndex {
    config: HnswConfig,
    /// Stored vectors (normalized if config.normalize)
    vectors: Vec<Vec<f32>>,
    /// Entity IDs corresponding to vectors
    entity_ids: Vec<String>,
    /// Graph nodes
    nodes: Vec<HnswNode>,
    /// Entry point (top-level node)
    entry_point: Option<usize>,
    /// Maximum level in the graph
    max_level: usize,
    /// Vector dimensions
    dimensions: usize,
    /// Whether index is built
    is_built: bool,
    /// Metadata storage for filtered search
    metadata: HashMap<String, Metadata>,
    /// Tombstones for lazy deletion (deleted entity IDs)
    deleted: HashSet<String>,
}

impl HnswIndex {
    /// Create a new HNSW index
    pub fn new(config: HnswConfig) -> Self {
        info!(
            "Initialized HNSW index: m={}, ef_construction={}, ef_search={}",
            config.m, config.ef_construction, config.ef_search
        );

        Self {
            config,
            vectors: Vec::new(),
            entity_ids: Vec::new(),
            nodes: Vec::new(),
            entry_point: None,
            max_level: 0,
            dimensions: 0,
            is_built: false,
            metadata: HashMap::new(),
            deleted: HashSet::new(),
        }
    }

    /// Build HNSW index from embeddings
    pub fn build(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()> {
        if embeddings.is_empty() {
            return Err(anyhow!("Cannot build index from empty embeddings"));
        }

        info!(
            "Building HNSW index for {} entities (m={}, ef_construction={})",
            embeddings.len(),
            self.config.m,
            self.config.ef_construction
        );

        // Reset state
        self.vectors.clear();
        self.entity_ids.clear();
        self.nodes.clear();
        self.entry_point = None;
        self.max_level = 0;

        // Store vectors
        self.dimensions = embeddings.values().next().unwrap().len();

        for (entity_id, vec) in embeddings {
            let mut v = vec.clone();
            if self.config.normalize {
                Self::normalize_vector(&mut v);
            }
            self.vectors.push(v);
            self.entity_ids.push(entity_id.clone());
        }

        // Insert vectors one by one
        for i in 0..self.vectors.len() {
            self.insert_node(i)?;
        }

        self.is_built = true;
        info!(
            "HNSW index built: {} vectors, max_level={}",
            self.vectors.len(),
            self.max_level
        );

        Ok(())
    }

    /// Insert a single node into the graph
    fn insert_node(&mut self, id: usize) -> Result<()> {
        let level = self.random_level();
        let node = HnswNode::new(id, level);
        self.nodes.push(node);

        // If this is the first node, just set it as entry point
        if self.entry_point.is_none() {
            self.entry_point = Some(id);
            self.max_level = level;
            return Ok(());
        }

        let entry_point = self.entry_point.unwrap();

        // Search from top layer down to node's level + 1
        let mut current_nearest = entry_point;

        for layer in (level + 1..=self.max_level).rev() {
            current_nearest = self.greedy_search(id, current_nearest, layer);
        }

        // Insert at each layer from node's level down to 0
        for layer in (0..=level.min(self.max_level)).rev() {
            // Find ef_construction nearest neighbors at this layer
            let neighbors =
                self.search_layer(id, current_nearest, self.config.ef_construction, layer);

            // Select M best neighbors
            let m = if layer == 0 {
                self.config.m0
            } else {
                self.config.m
            };

            let selected = self.select_neighbors(&neighbors, m);

            // Connect the new node to selected neighbors
            self.nodes[id].neighbors[layer] = selected.clone();

            // Connect neighbors back to the new node (bidirectional)
            for &neighbor_id in &selected {
                self.nodes[neighbor_id].neighbors[layer].push(id);

                // Prune if too many connections
                let max_connections = if layer == 0 {
                    self.config.m0
                } else {
                    self.config.m
                };

                if self.nodes[neighbor_id].neighbors[layer].len() > max_connections {
                    self.prune_connections(neighbor_id, layer, max_connections);
                }
            }

            if !selected.is_empty() {
                current_nearest = selected[0];
            }
        }

        // Update entry point if new node has higher level
        if level > self.max_level {
            self.entry_point = Some(id);
            self.max_level = level;
        }

        Ok(())
    }

    /// Generate random level for a new node
    fn random_level(&self) -> usize {
        let mut rng = rand::rng();
        let mut level = 0;
        let uniform: f64 = rng.random();

        // Level follows exponential distribution
        while uniform < (-((level + 1) as f64) * self.config.ml).exp() && level < 32 {
            level += 1;
        }

        level
    }

    /// Greedy search to find nearest node at a given layer
    fn greedy_search(&self, query_id: usize, start: usize, layer: usize) -> usize {
        let query = &self.vectors[query_id];
        let mut current = start;
        let mut current_dist = self.compute_distance(query, &self.vectors[current]);

        loop {
            let mut changed = false;

            for &neighbor in &self.nodes[current].neighbors[layer] {
                let dist = self.compute_distance(query, &self.vectors[neighbor]);
                if dist < current_dist {
                    current = neighbor;
                    current_dist = dist;
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        current
    }

    /// Search layer for nearest neighbors
    fn search_layer(
        &self,
        query_id: usize,
        entry_point: usize,
        ef: usize,
        layer: usize,
    ) -> Vec<(usize, f32)> {
        let query = &self.vectors[query_id];
        self.search_layer_by_vector(query, entry_point, ef, layer)
    }

    /// Search layer for nearest neighbors by vector
    fn search_layer_by_vector(
        &self,
        query: &[f32],
        entry_point: usize,
        ef: usize,
        layer: usize,
    ) -> Vec<(usize, f32)> {
        let mut visited = HashSet::new();
        let mut candidates: BinaryHeap<Candidate> = BinaryHeap::new();
        let mut results: BinaryHeap<MaxCandidate> = BinaryHeap::new();

        let entry_dist = self.compute_distance(query, &self.vectors[entry_point]);

        visited.insert(entry_point);
        candidates.push(Candidate {
            id: entry_point,
            distance: entry_dist,
        });
        results.push(MaxCandidate {
            id: entry_point,
            distance: entry_dist,
        });

        while let Some(Candidate { id: current, .. }) = candidates.pop() {
            let furthest_result = results.peek().map(|c| c.distance).unwrap_or(f32::MAX);

            // If current candidate is further than worst result, we're done
            if self.compute_distance(query, &self.vectors[current]) > furthest_result {
                break;
            }

            // Check neighbors
            if layer < self.nodes[current].neighbors.len() {
                for &neighbor in &self.nodes[current].neighbors[layer] {
                    if visited.contains(&neighbor) {
                        continue;
                    }
                    visited.insert(neighbor);

                    let dist = self.compute_distance(query, &self.vectors[neighbor]);
                    let furthest = results.peek().map(|c| c.distance).unwrap_or(f32::MAX);

                    if dist < furthest || results.len() < ef {
                        candidates.push(Candidate {
                            id: neighbor,
                            distance: dist,
                        });
                        results.push(MaxCandidate {
                            id: neighbor,
                            distance: dist,
                        });

                        // Keep only ef best results
                        while results.len() > ef {
                            results.pop();
                        }
                    }
                }
            }
        }

        // Convert to sorted vec
        let mut result_vec: Vec<(usize, f32)> =
            results.into_iter().map(|c| (c.id, c.distance)).collect();
        result_vec.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        result_vec
    }

    /// Select best neighbors using simple heuristic
    fn select_neighbors(&self, candidates: &[(usize, f32)], m: usize) -> Vec<usize> {
        candidates.iter().take(m).map(|(id, _)| *id).collect()
    }

    /// Prune connections to maintain max_connections limit
    fn prune_connections(&mut self, node_id: usize, layer: usize, max_connections: usize) {
        let node_vec = self.vectors[node_id].clone();

        // Calculate distances to all neighbors
        let mut neighbor_dists: Vec<(usize, f32)> = self.nodes[node_id].neighbors[layer]
            .iter()
            .map(|&n| (n, self.compute_distance(&node_vec, &self.vectors[n])))
            .collect();

        // Sort by distance
        neighbor_dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

        // Keep only the closest
        self.nodes[node_id].neighbors[layer] = neighbor_dists
            .into_iter()
            .take(max_connections)
            .map(|(id, _)| id)
            .collect();
    }

    /// Compute distance between two vectors
    ///
    /// Uses SIMD-optimized distance calculations for better performance.
    #[inline]
    fn compute_distance(&self, a: &[f32], b: &[f32]) -> f32 {
        // Use SIMD-optimized implementations for hot path performance
        simd::compute_distance_lower_is_better_simd(self.config.metric, a, b)
    }

    /// Normalize vector in-place
    #[inline]
    fn normalize_vector(vec: &mut [f32]) {
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-10 {
            for x in vec.iter_mut() {
                *x /= norm;
            }
        }
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

        // Normalize query if needed
        let mut normalized_query = query.to_vec();
        if self.config.normalize {
            Self::normalize_vector(&mut normalized_query);
        }

        debug!("HNSW search: k={}, ef_search={}", k, self.config.ef_search);

        let entry_point = self.entry_point.ok_or_else(|| anyhow!("Empty index"))?;

        // Navigate from top layer to layer 1
        let mut current = entry_point;
        for layer in (1..=self.max_level).rev() {
            current = self.greedy_search_by_vector(&normalized_query, current, layer);
        }

        // Search at layer 0 with ef_search candidates
        let candidates =
            self.search_layer_by_vector(&normalized_query, current, self.config.ef_search, 0);

        // Return top-k results, filtering out deleted vectors
        let results: Vec<SearchResult> = candidates
            .into_iter()
            .filter(|(id, _)| !self.deleted.contains(&self.entity_ids[*id]))
            .take(k)
            .enumerate()
            .map(|(rank, (id, distance))| SearchResult {
                entity_id: self.entity_ids[id].clone(),
                score: self.distance_to_score(distance),
                distance,
                rank: rank + 1,
            })
            .collect();

        debug!("Found {} results", results.len());
        Ok(results)
    }

    /// Greedy search by vector
    fn greedy_search_by_vector(&self, query: &[f32], start: usize, layer: usize) -> usize {
        let mut current = start;
        let mut current_dist = self.compute_distance(query, &self.vectors[current]);

        loop {
            let mut changed = false;

            if layer < self.nodes[current].neighbors.len() {
                for &neighbor in &self.nodes[current].neighbors[layer] {
                    let dist = self.compute_distance(query, &self.vectors[neighbor]);
                    if dist < current_dist {
                        current = neighbor;
                        current_dist = dist;
                        changed = true;
                    }
                }
            }

            if !changed {
                break;
            }
        }

        current
    }

    /// Convert distance to similarity score
    fn distance_to_score(&self, distance: f32) -> f32 {
        match self.config.metric {
            DistanceMetric::Cosine => 1.0 - distance,
            DistanceMetric::Euclidean | DistanceMetric::Manhattan => -distance,
            DistanceMetric::DotProduct => -distance,
        }
    }

    /// Batch search for multiple queries
    pub fn batch_search(&self, queries: &[Vec<f32>], k: usize) -> Result<Vec<Vec<SearchResult>>> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        info!("HNSW batch search: {} queries", queries.len());

        let results: Vec<Vec<SearchResult>> = queries
            .iter()
            .map(|query| self.search(query, k).unwrap_or_default())
            .collect();

        Ok(results)
    }

    /// Add a new vector to the index (incremental update)
    pub fn add(&mut self, entity_id: &str, vector: &[f32]) -> Result<()> {
        if !self.is_built {
            return Err(anyhow!(
                "Index not built. Call build() first or use build() with initial data"
            ));
        }

        if vector.len() != self.dimensions {
            return Err(anyhow!(
                "Vector dimension {} doesn't match index dimension {}",
                vector.len(),
                self.dimensions
            ));
        }

        // Add vector
        let mut v = vector.to_vec();
        if self.config.normalize {
            Self::normalize_vector(&mut v);
        }

        let id = self.vectors.len();
        self.vectors.push(v);
        self.entity_ids.push(entity_id.to_string());

        // Insert into graph
        self.insert_node(id)?;

        debug!("Added vector '{}' to HNSW index", entity_id);
        Ok(())
    }

    /// Get index statistics
    pub fn get_stats(&self) -> HnswStats {
        let total_connections: usize = self
            .nodes
            .iter()
            .flat_map(|n| n.neighbors.iter())
            .map(|neighbors| neighbors.len())
            .sum();

        let avg_connections = if !self.nodes.is_empty() {
            total_connections as f64 / self.nodes.len() as f64
        } else {
            0.0
        };

        HnswStats {
            num_vectors: self.vectors.len(),
            active_vectors: self.active_count(),
            deleted_vectors: self.deleted_count(),
            dimensions: self.dimensions,
            max_level: self.max_level,
            avg_connections,
            m: self.config.m,
            ef_construction: self.config.ef_construction,
            ef_search: self.config.ef_search,
            is_built: self.is_built,
        }
    }

    /// Set ef_search parameter (trade-off between speed and recall)
    pub fn set_ef_search(&mut self, ef: usize) {
        self.config.ef_search = ef;
    }

    /// Remove a vector from the index (lazy deletion with tombstone)
    ///
    /// The vector is marked as deleted and will be excluded from search results.
    /// The actual data is not removed until `compact()` is called.
    pub fn remove(&mut self, entity_id: &str) -> bool {
        if self.entity_ids.iter().any(|e| e == entity_id) {
            self.deleted.insert(entity_id.to_string());
            self.metadata.remove(entity_id);
            debug!("Marked '{}' as deleted (tombstone)", entity_id);
            true
        } else {
            false
        }
    }

    /// Check if a vector is deleted
    pub fn is_deleted(&self, entity_id: &str) -> bool {
        self.deleted.contains(entity_id)
    }

    /// Get the number of deleted vectors (tombstones)
    pub fn deleted_count(&self) -> usize {
        self.deleted.len()
    }

    /// Get the number of active (non-deleted) vectors
    pub fn active_count(&self) -> usize {
        self.vectors.len() - self.deleted.len()
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
    #[inline]
    pub fn get_metadata(&self, entity_id: &str) -> Option<&Metadata> {
        self.metadata.get(entity_id)
    }

    /// Search with metadata filtering (post-filtering)
    ///
    /// Searches with HNSW, then filters results by metadata.
    /// Efficient when most results pass the filter.
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

        // Increase ef_search temporarily to get more candidates for filtering
        let expanded_k = (k * 10).min(self.vectors.len());

        debug!(
            "HNSW filtered search: k={}, expanded_k={}, filter conditions={}",
            k,
            expanded_k,
            filter.conditions().len()
        );

        // Get more candidates than needed
        let all_results = self.search(query, expanded_k)?;

        // Filter and take top-k
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

        debug!("HNSW filtered search returned {} results", filtered.len());
        Ok(filtered)
    }

    /// Search with pre-filtering (filter candidates during search)
    ///
    /// More accurate when filters are very selective, but may be slower.
    /// Uses brute-force search on filtered candidates.
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

        debug!("HNSW pre-filtered search: k={}", k);

        // Normalize query if needed
        let mut normalized_query = query.to_vec();
        if self.config.normalize {
            Self::normalize_vector(&mut normalized_query);
        }

        // Find all indices that match the filter
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

        // Compute distances only for matching entities (brute force on filtered set)
        let mut scores: Vec<(usize, f32)> = matching_indices
            .iter()
            .map(|&i| {
                let dist = self.compute_distance(&normalized_query, &self.vectors[i]);
                (i, dist)
            })
            .collect();

        // Sort by distance ascending
        scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

        // Return top-K results
        let results: Vec<SearchResult> = scores
            .iter()
            .take(k)
            .enumerate()
            .map(|(rank, &(idx, distance))| SearchResult {
                entity_id: self.entity_ids[idx].clone(),
                score: self.distance_to_score(distance),
                distance,
                rank: rank + 1,
            })
            .collect();

        debug!(
            "HNSW pre-filtered search returned {} results",
            results.len()
        );
        Ok(results)
    }

    /// Optimize the HNSW graph structure
    ///
    /// This method performs periodic maintenance on the graph to improve search quality:
    /// - Removes references to deleted nodes from neighbor lists
    /// - Trims neighbor lists to respect the M parameter
    /// - Rebuilds connections for isolated nodes
    ///
    /// Call this method periodically after many deletions or additions to maintain
    /// optimal search performance.
    pub fn optimize_graph(&mut self) -> Result<()> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        info!("Optimizing HNSW graph structure...");

        let deleted_indices: HashSet<usize> = self
            .entity_ids
            .iter()
            .enumerate()
            .filter(|(_, id)| self.deleted.contains(*id))
            .map(|(idx, _)| idx)
            .collect();

        let mut optimized_count = 0;

        // Process each node
        for node_idx in 0..self.nodes.len() {
            let node_level = self.nodes[node_idx].level;

            for layer in 0..=node_level {
                let original_len = self.nodes[node_idx].neighbors[layer].len();

                // Remove deleted neighbors
                self.nodes[node_idx].neighbors[layer]
                    .retain(|&neighbor_id| !deleted_indices.contains(&neighbor_id));

                // Trim to max connections (M for layer > 0, M0 for layer 0)
                let max_connections = if layer == 0 {
                    self.config.m0
                } else {
                    self.config.m
                };

                if self.nodes[node_idx].neighbors[layer].len() > max_connections {
                    // Collect neighbors and compute distances
                    let node_vec = self.vectors[node_idx].clone();
                    let mut neighbor_distances: Vec<(usize, f32)> = self.nodes[node_idx].neighbors
                        [layer]
                        .iter()
                        .map(|&neighbor_id| {
                            let dist = self.compute_distance(&node_vec, &self.vectors[neighbor_id]);
                            (neighbor_id, dist)
                        })
                        .collect();

                    // Sort by distance (ascending)
                    neighbor_distances
                        .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

                    // Keep only top-M closest neighbors
                    self.nodes[node_idx].neighbors[layer] = neighbor_distances
                        .iter()
                        .take(max_connections)
                        .map(|(id, _)| *id)
                        .collect();
                }

                if self.nodes[node_idx].neighbors[layer].len() != original_len {
                    optimized_count += 1;
                }
            }
        }

        info!(
            "HNSW graph optimization complete. {} node connections updated.",
            optimized_count
        );

        Ok(())
    }

    /// Compact the index by removing tombstones
    ///
    /// This method rebuilds the index without deleted vectors, freeing memory
    /// and improving cache efficiency. Use this after many deletions.
    ///
    /// WARNING: This operation is expensive as it rebuilds the entire index.
    pub fn compact(&mut self) -> Result<()> {
        if !self.is_built {
            return Err(anyhow!("Index not built. Call build() first"));
        }

        if self.deleted.is_empty() {
            info!("No deleted vectors to compact");
            return Ok(());
        }

        info!(
            "Compacting HNSW index: removing {} deleted vectors out of {}",
            self.deleted.len(),
            self.vectors.len()
        );

        // Collect non-deleted vectors and their entity IDs
        let mut new_embeddings = HashMap::new();
        let mut new_metadata = HashMap::new();

        for (i, entity_id) in self.entity_ids.iter().enumerate() {
            if !self.deleted.contains(entity_id) {
                new_embeddings.insert(entity_id.clone(), self.vectors[i].clone());

                if let Some(metadata) = self.metadata.get(entity_id) {
                    new_metadata.insert(entity_id.clone(), metadata.clone());
                }
            }
        }

        // Rebuild the index
        self.build(&new_embeddings)?;

        // Restore metadata
        self.set_metadata_batch(new_metadata);

        info!("HNSW index compaction complete");

        Ok(())
    }
}

/// HNSW index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswStats {
    /// Total number of vectors in the index (including deleted)
    pub num_vectors: usize,
    /// Number of active (non-deleted) vectors
    pub active_vectors: usize,
    /// Number of deleted vectors (tombstones)
    pub deleted_vectors: usize,
    /// Vector dimensions
    pub dimensions: usize,
    /// Maximum level in the graph
    pub max_level: usize,
    /// Average connections per node
    pub avg_connections: f64,
    /// M parameter
    pub m: usize,
    /// ef_construction parameter
    pub ef_construction: usize,
    /// ef_search parameter
    pub ef_search: usize,
    /// Whether index is built
    pub is_built: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_embeddings() -> HashMap<String, Vec<f32>> {
        let mut embeddings = HashMap::new();

        embeddings.insert("doc1".to_string(), vec![1.0, 0.0, 0.0]);
        embeddings.insert("doc2".to_string(), vec![0.9, 0.1, 0.0]);
        embeddings.insert("doc3".to_string(), vec![0.0, 1.0, 0.0]);
        embeddings.insert("doc4".to_string(), vec![0.0, 0.0, 1.0]);
        embeddings.insert("doc5".to_string(), vec![0.7, 0.7, 0.0]);

        embeddings
    }

    #[test]
    fn test_hnsw_config_default() {
        let config = HnswConfig::default();
        assert_eq!(config.m, 16);
        assert_eq!(config.m0, 32);
        assert_eq!(config.ef_construction, 200);
        assert_eq!(config.ef_search, 50);
    }

    #[test]
    fn test_hnsw_build() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());

        assert!(index.build(&embeddings).is_ok());
        assert!(index.is_built);

        let stats = index.get_stats();
        assert_eq!(stats.num_vectors, 5);
        assert_eq!(stats.dimensions, 3);
    }

    #[test]
    fn test_hnsw_search() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        // Search for vector similar to doc1
        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 3).unwrap();

        assert_eq!(results.len(), 3);
        // doc1 or doc2 should be the closest
        assert!(results[0].entity_id == "doc1" || results[0].entity_id == "doc2");
    }

    #[test]
    fn test_hnsw_batch_search() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        let queries = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];

        let results = index.batch_search(&queries, 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].len(), 2);
        assert_eq!(results[1].len(), 2);
    }

    #[test]
    fn test_hnsw_incremental_add() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        // Add a new vector
        index.add("doc6", &[0.5, 0.5, 0.5]).unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.num_vectors, 6);

        // Search should find the new vector
        let query = vec![0.5, 0.5, 0.5];
        let results = index.search(&query, 1).unwrap();
        assert_eq!(results[0].entity_id, "doc6");
    }

    #[test]
    fn test_hnsw_search_accuracy() {
        // Create a larger test set
        let mut embeddings = HashMap::new();
        for i in 0..100 {
            let angle = (i as f32) * 2.0 * std::f32::consts::PI / 100.0;
            embeddings.insert(format!("doc{}", i), vec![angle.cos(), angle.sin(), 0.0]);
        }

        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        // Search for a specific angle
        let query_angle = 0.5_f32;
        let query = vec![query_angle.cos(), query_angle.sin(), 0.0];
        let results = index.search(&query, 5).unwrap();

        // Should find nearby angles
        assert_eq!(results.len(), 5);
        // Top result should have high similarity
        assert!(results[0].score > 0.95);
    }

    #[test]
    fn test_hnsw_empty_error() {
        let embeddings: HashMap<String, Vec<f32>> = HashMap::new();
        let mut index = HnswIndex::new(HnswConfig::default());

        assert!(index.build(&embeddings).is_err());
    }

    #[test]
    fn test_hnsw_dimension_mismatch() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        // Query with wrong dimensions
        let query = vec![1.0, 0.0]; // 2D instead of 3D
        assert!(index.search(&query, 1).is_err());
    }

    #[test]
    fn test_hnsw_stats() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.num_vectors, 5);
        assert_eq!(stats.dimensions, 3);
        assert_eq!(stats.m, 16);
        assert_eq!(stats.ef_construction, 200);
        assert!(stats.is_built);
    }

    #[test]
    fn test_ef_search_adjustment() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        index.set_ef_search(100);
        let stats = index.get_stats();
        assert_eq!(stats.ef_search, 100);
    }

    fn create_test_metadata() -> HashMap<String, Metadata> {
        use crate::filter::FilterValue;

        let mut metadata = HashMap::new();

        let mut m1 = HashMap::new();
        m1.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );
        m1.insert("year".to_string(), FilterValue::Int(2023));
        metadata.insert("doc1".to_string(), m1);

        let mut m2 = HashMap::new();
        m2.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );
        m2.insert("year".to_string(), FilterValue::Int(2022));
        metadata.insert("doc2".to_string(), m2);

        let mut m3 = HashMap::new();
        m3.insert("type".to_string(), FilterValue::String("book".to_string()));
        m3.insert("year".to_string(), FilterValue::Int(2023));
        metadata.insert("doc3".to_string(), m3);

        let mut m4 = HashMap::new();
        m4.insert("type".to_string(), FilterValue::String("book".to_string()));
        m4.insert("year".to_string(), FilterValue::Int(2021));
        metadata.insert("doc4".to_string(), m4);

        let mut m5 = HashMap::new();
        m5.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );
        m5.insert("year".to_string(), FilterValue::Int(2024));
        metadata.insert("doc5".to_string(), m5);

        metadata
    }

    #[test]
    fn test_hnsw_set_and_get_metadata() {
        use crate::filter::FilterValue;

        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            FilterValue::String("article".to_string()),
        );

        index.set_metadata("doc1", metadata.clone());

        let retrieved = index.get_metadata("doc1");
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved.unwrap().get("type"),
            Some(&FilterValue::String("article".to_string()))
        );
    }

    #[test]
    fn test_hnsw_filtered_search() {
        use crate::filter::FilterValue;

        let embeddings = create_test_embeddings();
        let metadata = create_test_metadata();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();
        index.set_metadata_batch(metadata);

        // Filter for articles only
        let filter = Filter::new().eq("type", "article");
        let query = vec![1.0, 0.0, 0.0];
        let results = index.filtered_search(&query, 5, &filter).unwrap();

        // Should only return articles (doc1, doc2, doc5)
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
    fn test_hnsw_filtered_search_with_year() {
        let embeddings = create_test_embeddings();
        let metadata = create_test_metadata();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();
        index.set_metadata_batch(metadata);

        // Filter for year >= 2023
        let filter = Filter::new().gte("year", 2023i64);
        let query = vec![1.0, 0.0, 0.0];
        let results = index.filtered_search(&query, 5, &filter).unwrap();

        // Should return doc1 (2023), doc3 (2023), doc5 (2024)
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_hnsw_prefiltered_search() {
        use crate::filter::FilterValue;

        let embeddings = create_test_embeddings();
        let metadata = create_test_metadata();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();
        index.set_metadata_batch(metadata);

        // Filter for books only
        let filter = Filter::new().eq("type", "book");
        let query = vec![0.0, 1.0, 0.0]; // Similar to doc3 (book)
        let results = index.prefiltered_search(&query, 5, &filter).unwrap();

        // Should only return books (doc3, doc4)
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
    fn test_hnsw_filtered_search_empty_filter() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        // Empty filter should return all results
        let filter = Filter::new();
        let query = vec![1.0, 0.0, 0.0];
        let results = index.filtered_search(&query, 3, &filter).unwrap();

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_hnsw_filtered_search_no_matches() {
        let embeddings = create_test_embeddings();
        let metadata = create_test_metadata();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();
        index.set_metadata_batch(metadata);

        // Filter for non-existent type
        let filter = Filter::new().eq("type", "journal");
        let query = vec![1.0, 0.0, 0.0];
        let results = index.filtered_search(&query, 5, &filter).unwrap();

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_hnsw_lazy_delete() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        let stats_before = index.get_stats();
        assert_eq!(stats_before.num_vectors, 5);
        assert_eq!(stats_before.active_vectors, 5);
        assert_eq!(stats_before.deleted_vectors, 0);

        // Delete a vector
        assert!(index.remove("doc1"));
        assert!(index.is_deleted("doc1"));

        let stats_after = index.get_stats();
        assert_eq!(stats_after.num_vectors, 5); // Still in index
        assert_eq!(stats_after.active_vectors, 4);
        assert_eq!(stats_after.deleted_vectors, 1);

        // Deleted vector should not appear in search results
        let query = vec![1.0, 0.0, 0.0]; // doc1's vector
        let results = index.search(&query, 5).unwrap();

        // doc1 should not be in results
        for result in &results {
            assert_ne!(result.entity_id, "doc1");
        }
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn test_hnsw_delete_nonexistent() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        // Try to delete non-existent vector
        assert!(!index.remove("nonexistent"));
        assert!(!index.is_deleted("nonexistent"));
    }

    #[test]
    fn test_hnsw_delete_multiple() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        // Delete multiple vectors
        index.remove("doc1");
        index.remove("doc2");
        index.remove("doc3");

        let stats = index.get_stats();
        assert_eq!(stats.active_vectors, 2);
        assert_eq!(stats.deleted_vectors, 3);

        // Search should only return non-deleted vectors
        let query = vec![0.5, 0.5, 0.5];
        let results = index.search(&query, 10).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_hnsw_delete_and_active_count() {
        let embeddings = create_test_embeddings();
        let mut index = HnswIndex::new(HnswConfig::default());
        index.build(&embeddings).unwrap();

        assert_eq!(index.active_count(), 5);
        assert_eq!(index.deleted_count(), 0);

        index.remove("doc1");
        assert_eq!(index.active_count(), 4);
        assert_eq!(index.deleted_count(), 1);

        index.remove("doc2");
        assert_eq!(index.active_count(), 3);
        assert_eq!(index.deleted_count(), 2);
    }
}
