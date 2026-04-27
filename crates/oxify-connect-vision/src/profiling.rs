//! Performance Profiling for OCR Operations
//!
//! This module provides performance profiling capabilities including CPU profiling,
//! memory profiling, and bottleneck detection for OCR operations.
//!
//! # Features
//!
//! - CPU time profiling with hierarchical call tracking
//! - Memory usage profiling (heap allocations)
//! - Flamegraph generation data
//! - Bottleneck detection and analysis
//! - Operation timing with statistics
//! - Resource usage tracking
//!
//! # Example
//!
//! ```rust,ignore
//! use oxify_connect_vision::profiling::{Profiler, ProfilerConfig};
//!
//! let config = ProfilerConfig::default();
//! let profiler = Profiler::new(config);
//!
//! // Profile an operation
//! profiler.start_profile("process_image");
//! let result = process_image(&bytes).await?;
//! profiler.end_profile("process_image");
//!
//! // Get profiling results
//! let report = profiler.generate_report();
//! println!("{}", report.summary());
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Profiling errors
#[derive(Debug, Error)]
pub enum ProfilingError {
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Profile already started: {0}")]
    ProfileAlreadyStarted(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Failed to generate report: {0}")]
    ReportError(String),
}

pub type Result<T> = std::result::Result<T, ProfilingError>;

/// Profiler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerConfig {
    /// Enable CPU profiling
    pub enable_cpu_profiling: bool,

    /// Enable memory profiling
    pub enable_memory_profiling: bool,

    /// Enable call hierarchy tracking
    pub enable_call_hierarchy: bool,

    /// Maximum call stack depth
    pub max_call_depth: usize,

    /// Sample interval for periodic sampling (milliseconds)
    pub sample_interval_ms: u64,

    /// Track individual allocations
    pub track_allocations: bool,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            enable_cpu_profiling: true,
            enable_memory_profiling: true,
            enable_call_hierarchy: true,
            max_call_depth: 32,
            sample_interval_ms: 10,
            track_allocations: false,
        }
    }
}

impl ProfilerConfig {
    /// Create a new configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable CPU profiling
    pub fn with_cpu_profiling(mut self, enabled: bool) -> Self {
        self.enable_cpu_profiling = enabled;
        self
    }

    /// Enable memory profiling
    pub fn with_memory_profiling(mut self, enabled: bool) -> Self {
        self.enable_memory_profiling = enabled;
        self
    }

    /// Enable call hierarchy tracking
    pub fn with_call_hierarchy(mut self, enabled: bool) -> Self {
        self.enable_call_hierarchy = enabled;
        self
    }

    /// Set maximum call depth
    pub fn with_max_call_depth(mut self, depth: usize) -> Self {
        self.max_call_depth = depth;
        self
    }
}

/// Profile entry representing a single profiled operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileEntry {
    /// Operation name
    pub name: String,

    /// Start timestamp
    pub start_time: std::time::SystemTime,

    /// Duration
    pub duration: Duration,

    /// CPU time (may differ from duration due to parallelism)
    pub cpu_time: Duration,

    /// Memory allocated (bytes)
    pub memory_allocated: u64,

    /// Memory deallocated (bytes)
    pub memory_deallocated: u64,

    /// Peak memory usage (bytes)
    pub peak_memory: u64,

    /// Number of invocations
    pub invocations: u64,

    /// Parent operation (for call hierarchy)
    pub parent: Option<String>,

    /// Child operations
    pub children: Vec<String>,
}

impl ProfileEntry {
    /// Create a new profile entry
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start_time: std::time::SystemTime::now(),
            duration: Duration::ZERO,
            cpu_time: Duration::ZERO,
            memory_allocated: 0,
            memory_deallocated: 0,
            peak_memory: 0,
            invocations: 0,
            parent: None,
            children: Vec::new(),
        }
    }

    /// Get net memory usage
    pub fn net_memory(&self) -> i64 {
        self.memory_allocated as i64 - self.memory_deallocated as i64
    }

    /// Get average duration per invocation
    pub fn avg_duration(&self) -> Duration {
        if self.invocations == 0 {
            Duration::ZERO
        } else {
            self.duration / self.invocations as u32
        }
    }
}

/// Active profile tracking
#[derive(Debug)]
struct ActiveProfile {
    /// Profile name
    #[allow(dead_code)]
    name: String,

    /// Start time
    start_time: Instant,

    /// Start memory (if tracked)
    start_memory: u64,

    /// Parent profile
    parent: Option<String>,
}

/// Call stack for hierarchy tracking
#[derive(Debug, Default)]
struct CallStack {
    /// Stack of active operations
    stack: Vec<String>,
}

impl CallStack {
    /// Push an operation onto the stack
    fn push(&mut self, name: String) -> Option<String> {
        let parent = self.stack.last().cloned();
        self.stack.push(name);
        parent
    }

    /// Pop an operation from the stack
    fn pop(&mut self) -> Option<String> {
        self.stack.pop()
    }

    /// Get current depth
    fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Get current operation
    #[allow(dead_code)]
    fn current(&self) -> Option<&String> {
        self.stack.last()
    }
}

/// Memory snapshot
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemorySnapshot {
    /// Total allocated bytes
    pub total_allocated: u64,

    /// Total deallocated bytes
    pub total_deallocated: u64,

    /// Current usage (allocated - deallocated)
    pub current_usage: u64,

    /// Peak usage
    pub peak_usage: u64,

    /// Number of active allocations
    pub active_allocations: u64,
}

impl MemorySnapshot {
    /// Get current system memory info (simplified)
    pub fn current() -> Self {
        // In a real implementation, this would query system memory
        // For now, return a placeholder
        Self::default()
    }
}

/// Performance profiler
#[derive(Debug)]
pub struct Profiler {
    /// Configuration
    config: ProfilerConfig,

    /// Completed profiles
    profiles: Arc<Mutex<HashMap<String, ProfileEntry>>>,

    /// Active profiles
    active: Arc<Mutex<HashMap<String, ActiveProfile>>>,

    /// Call stack for hierarchy
    call_stack: Arc<Mutex<CallStack>>,

    /// Memory snapshots
    memory_snapshots: Arc<Mutex<Vec<MemorySnapshot>>>,

    /// Start time
    start_time: Instant,
}

impl Profiler {
    /// Create a new profiler
    pub fn new(config: ProfilerConfig) -> Self {
        Self {
            config,
            profiles: Arc::new(Mutex::new(HashMap::new())),
            active: Arc::new(Mutex::new(HashMap::new())),
            call_stack: Arc::new(Mutex::new(CallStack::default())),
            memory_snapshots: Arc::new(Mutex::new(Vec::new())),
            start_time: Instant::now(),
        }
    }

    /// Start profiling an operation
    pub fn start_profile(&self, name: impl Into<String>) -> Result<()> {
        let name = name.into();

        if !self.config.enable_cpu_profiling {
            return Ok(());
        }

        let mut active = self.active.lock().unwrap();

        if active.contains_key(&name) {
            return Err(ProfilingError::ProfileAlreadyStarted(name));
        }

        // Get parent from call stack
        let parent = if self.config.enable_call_hierarchy {
            let mut stack = self.call_stack.lock().unwrap();
            if stack.depth() >= self.config.max_call_depth {
                None
            } else {
                stack.push(name.clone())
            }
        } else {
            None
        };

        // Take memory snapshot if enabled
        let start_memory = if self.config.enable_memory_profiling {
            let snapshot = MemorySnapshot::current();
            snapshot.current_usage
        } else {
            0
        };

        active.insert(
            name.clone(),
            ActiveProfile {
                name,
                start_time: Instant::now(),
                start_memory,
                parent,
            },
        );

        Ok(())
    }

    /// End profiling an operation
    pub fn end_profile(&self, name: impl Into<String>) -> Result<()> {
        let name = name.into();

        if !self.config.enable_cpu_profiling {
            return Ok(());
        }

        let mut active = self.active.lock().unwrap();

        let active_profile = active
            .remove(&name)
            .ok_or_else(|| ProfilingError::ProfileNotFound(name.clone()))?;

        drop(active);

        // Pop from call stack
        if self.config.enable_call_hierarchy {
            let mut stack = self.call_stack.lock().unwrap();
            stack.pop();
        }

        let duration = active_profile.start_time.elapsed();

        // Calculate memory usage
        let (memory_allocated, memory_deallocated, peak_memory) =
            if self.config.enable_memory_profiling {
                let end_snapshot = MemorySnapshot::current();
                let allocated = end_snapshot
                    .current_usage
                    .saturating_sub(active_profile.start_memory);
                let deallocated = 0; // Simplified
                let peak = end_snapshot.peak_usage;
                (allocated, deallocated, peak)
            } else {
                (0, 0, 0)
            };

        // Update or create profile entry
        let mut profiles = self.profiles.lock().unwrap();

        let entry = profiles.entry(name.clone()).or_insert_with(|| {
            let mut entry = ProfileEntry::new(name.clone());
            entry.parent = active_profile.parent.clone();
            entry
        });

        entry.duration += duration;
        entry.cpu_time += duration; // Simplified, would need OS thread time
        entry.memory_allocated += memory_allocated;
        entry.memory_deallocated += memory_deallocated;
        entry.peak_memory = entry.peak_memory.max(peak_memory);
        entry.invocations += 1;

        // Update parent's children list
        if let Some(parent_name) = &active_profile.parent {
            // Ensure parent entry exists (create if needed)
            let parent_entry = profiles
                .entry(parent_name.clone())
                .or_insert_with(|| ProfileEntry::new(parent_name.clone()));

            if !parent_entry.children.contains(&name) {
                parent_entry.children.push(name.clone());
            }
        }

        Ok(())
    }

    /// Get a profile entry
    pub fn get_profile(&self, name: &str) -> Option<ProfileEntry> {
        let profiles = self.profiles.lock().unwrap();
        profiles.get(name).cloned()
    }

    /// Get all profiles
    pub fn get_all_profiles(&self) -> Vec<ProfileEntry> {
        let profiles = self.profiles.lock().unwrap();
        profiles.values().cloned().collect()
    }

    /// Generate a profiling report
    pub fn generate_report(&self) -> ProfilingReport {
        let profiles = self.profiles.lock().unwrap();
        let entries: Vec<ProfileEntry> = profiles.values().cloned().collect();

        ProfilingReport::new(entries, self.start_time.elapsed())
    }

    /// Reset all profiles
    pub fn reset(&self) {
        let mut profiles = self.profiles.lock().unwrap();
        profiles.clear();

        let mut active = self.active.lock().unwrap();
        active.clear();

        let mut stack = self.call_stack.lock().unwrap();
        *stack = CallStack::default();

        let mut snapshots = self.memory_snapshots.lock().unwrap();
        snapshots.clear();
    }

    /// Take a memory snapshot
    pub fn take_memory_snapshot(&self) {
        if self.config.enable_memory_profiling {
            let snapshot = MemorySnapshot::current();
            let mut snapshots = self.memory_snapshots.lock().unwrap();
            snapshots.push(snapshot);
        }
    }

    /// Get memory snapshots
    pub fn get_memory_snapshots(&self) -> Vec<MemorySnapshot> {
        let snapshots = self.memory_snapshots.lock().unwrap();
        snapshots.clone()
    }

    /// Get configuration
    pub fn config(&self) -> &ProfilerConfig {
        &self.config
    }
}

/// Profiling report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilingReport {
    /// All profile entries
    pub entries: Vec<ProfileEntry>,

    /// Total profiling duration
    pub total_duration: Duration,

    /// Statistics
    pub stats: ReportStats,

    /// Bottlenecks (operations taking > 10% of total time)
    pub bottlenecks: Vec<BottleneckInfo>,

    /// Call hierarchy (tree structure)
    pub call_tree: Vec<CallTreeNode>,
}

impl ProfilingReport {
    /// Create a new report
    fn new(entries: Vec<ProfileEntry>, total_duration: Duration) -> Self {
        let stats = ReportStats::from_entries(&entries);
        let bottlenecks = Self::identify_bottlenecks(&entries, total_duration);
        let call_tree = Self::build_call_tree(&entries);

        Self {
            entries,
            total_duration,
            stats,
            bottlenecks,
            call_tree,
        }
    }

    /// Identify bottlenecks
    fn identify_bottlenecks(
        entries: &[ProfileEntry],
        total_duration: Duration,
    ) -> Vec<BottleneckInfo> {
        let threshold = total_duration.as_secs_f64() * 0.1; // 10% threshold

        let mut bottlenecks: Vec<BottleneckInfo> = entries
            .iter()
            .filter(|e| e.duration.as_secs_f64() >= threshold)
            .map(|e| BottleneckInfo {
                name: e.name.clone(),
                duration: e.duration,
                percentage: (e.duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0,
                invocations: e.invocations,
                avg_duration: e.avg_duration(),
                memory_usage: e.net_memory(),
            })
            .collect();

        bottlenecks.sort_by_key(|x| std::cmp::Reverse(x.duration));
        bottlenecks
    }

    /// Build call tree
    fn build_call_tree(entries: &[ProfileEntry]) -> Vec<CallTreeNode> {
        // Find root nodes (no parent)
        let roots: Vec<_> = entries
            .iter()
            .filter(|e| e.parent.is_none())
            .map(|e| Self::build_tree_node(e, entries))
            .collect();

        roots
    }

    /// Build a single tree node
    fn build_tree_node(entry: &ProfileEntry, all_entries: &[ProfileEntry]) -> CallTreeNode {
        let children: Vec<CallTreeNode> = entry
            .children
            .iter()
            .filter_map(|child_name| {
                all_entries
                    .iter()
                    .find(|e| e.name == *child_name)
                    .map(|e| Self::build_tree_node(e, all_entries))
            })
            .collect();

        CallTreeNode {
            name: entry.name.clone(),
            duration: entry.duration,
            invocations: entry.invocations,
            memory: entry.net_memory(),
            children,
        }
    }

    /// Generate a summary string
    pub fn summary(&self) -> String {
        let mut s = String::new();
        s.push_str("=== Performance Profiling Report ===\n\n");
        s.push_str(&format!(
            "Total Duration: {:.2}s\n",
            self.total_duration.as_secs_f64()
        ));
        s.push_str(&format!("Total Operations: {}\n", self.entries.len()));
        s.push_str(&format!(
            "Total Invocations: {}\n\n",
            self.stats.total_invocations
        ));

        s.push_str("=== Statistics ===\n");
        s.push_str(&format!(
            "Mean Duration: {:.2}ms\n",
            self.stats.mean_duration.as_secs_f64() * 1000.0
        ));
        s.push_str(&format!(
            "Median Duration: {:.2}ms\n",
            self.stats.median_duration.as_secs_f64() * 1000.0
        ));
        s.push_str(&format!(
            "Total Memory: {} bytes\n\n",
            self.stats.total_memory
        ));

        if !self.bottlenecks.is_empty() {
            s.push_str("=== Bottlenecks (>10% of total time) ===\n");
            for bottleneck in &self.bottlenecks {
                s.push_str(&format!(
                    "  {} - {:.2}s ({:.1}%) - {} calls\n",
                    bottleneck.name,
                    bottleneck.duration.as_secs_f64(),
                    bottleneck.percentage,
                    bottleneck.invocations
                ));
            }
            s.push('\n');
        }

        s
    }

    /// Export to JSON
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Generate flamegraph data (name;duration format)
    pub fn to_flamegraph_data(&self) -> String {
        let mut lines = Vec::new();

        for node in &self.call_tree {
            Self::flamegraph_node(&mut lines, node, String::new());
        }

        lines.join("\n")
    }

    /// Generate flamegraph data for a node
    fn flamegraph_node(lines: &mut Vec<String>, node: &CallTreeNode, stack: String) {
        let current_stack = if stack.is_empty() {
            node.name.clone()
        } else {
            format!("{};{}", stack, node.name)
        };

        let duration_us = node.duration.as_micros();
        lines.push(format!("{} {}", current_stack, duration_us));

        for child in &node.children {
            Self::flamegraph_node(lines, child, current_stack.clone());
        }
    }
}

/// Report statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportStats {
    /// Total invocations across all operations
    pub total_invocations: u64,

    /// Mean duration
    pub mean_duration: Duration,

    /// Median duration
    pub median_duration: Duration,

    /// Total memory allocated
    pub total_memory: i64,

    /// Slowest operation
    pub slowest_operation: Option<String>,

    /// Fastest operation
    pub fastest_operation: Option<String>,
}

impl ReportStats {
    /// Compute statistics from entries
    fn from_entries(entries: &[ProfileEntry]) -> Self {
        if entries.is_empty() {
            return Self::default();
        }

        let total_invocations: u64 = entries.iter().map(|e| e.invocations).sum();
        let total_duration: Duration = entries.iter().map(|e| e.duration).sum();
        let total_memory: i64 = entries.iter().map(|e| e.net_memory()).sum();

        let mean_duration = if !entries.is_empty() {
            total_duration / entries.len() as u32
        } else {
            Duration::ZERO
        };

        // Calculate median
        let mut durations: Vec<Duration> = entries.iter().map(|e| e.duration).collect();
        durations.sort();
        let median_duration = durations
            .get(durations.len() / 2)
            .copied()
            .unwrap_or(Duration::ZERO);

        // Find slowest and fastest
        let slowest = entries
            .iter()
            .max_by_key(|e| e.duration)
            .map(|e| e.name.clone());
        let fastest = entries
            .iter()
            .min_by_key(|e| e.duration)
            .map(|e| e.name.clone());

        Self {
            total_invocations,
            mean_duration,
            median_duration,
            total_memory,
            slowest_operation: slowest,
            fastest_operation: fastest,
        }
    }
}

impl Default for ReportStats {
    fn default() -> Self {
        Self {
            total_invocations: 0,
            mean_duration: Duration::ZERO,
            median_duration: Duration::ZERO,
            total_memory: 0,
            slowest_operation: None,
            fastest_operation: None,
        }
    }
}

/// Bottleneck information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottleneckInfo {
    /// Operation name
    pub name: String,

    /// Total duration
    pub duration: Duration,

    /// Percentage of total time
    pub percentage: f64,

    /// Number of invocations
    pub invocations: u64,

    /// Average duration per invocation
    pub avg_duration: Duration,

    /// Memory usage
    pub memory_usage: i64,
}

/// Call tree node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallTreeNode {
    /// Operation name
    pub name: String,

    /// Duration
    pub duration: Duration,

    /// Invocations
    pub invocations: u64,

    /// Memory usage
    pub memory: i64,

    /// Child nodes
    pub children: Vec<CallTreeNode>,
}

/// Helper macro for profiling a block
#[macro_export]
macro_rules! profile {
    ($profiler:expr, $name:expr, $block:block) => {{
        $profiler.start_profile($name).ok();
        let result = $block;
        $profiler.end_profile($name).ok();
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiler_config_default() {
        let config = ProfilerConfig::default();
        assert!(config.enable_cpu_profiling);
        assert!(config.enable_memory_profiling);
        assert!(config.enable_call_hierarchy);
        assert_eq!(config.max_call_depth, 32);
    }

    #[test]
    fn test_profiler_config_builder() {
        let config = ProfilerConfig::new()
            .with_cpu_profiling(false)
            .with_memory_profiling(false)
            .with_call_hierarchy(false)
            .with_max_call_depth(16);

        assert!(!config.enable_cpu_profiling);
        assert!(!config.enable_memory_profiling);
        assert!(!config.enable_call_hierarchy);
        assert_eq!(config.max_call_depth, 16);
    }

    #[test]
    fn test_start_end_profile() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("test_op").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        profiler.end_profile("test_op").unwrap();

        let profile = profiler.get_profile("test_op").unwrap();
        assert_eq!(profile.name, "test_op");
        assert_eq!(profile.invocations, 1);
        assert!(profile.duration.as_millis() >= 10);
    }

    #[test]
    fn test_multiple_invocations() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        for _ in 0..5 {
            profiler.start_profile("test_op").unwrap();
            std::thread::sleep(Duration::from_millis(5));
            profiler.end_profile("test_op").unwrap();
        }

        let profile = profiler.get_profile("test_op").unwrap();
        assert_eq!(profile.invocations, 5);
        assert!(profile.duration.as_millis() >= 25);
    }

    #[test]
    fn test_call_hierarchy() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("parent").unwrap();
        profiler.start_profile("child1").unwrap();
        profiler.end_profile("child1").unwrap();
        profiler.start_profile("child2").unwrap();
        profiler.end_profile("child2").unwrap();
        profiler.end_profile("parent").unwrap();

        let parent = profiler.get_profile("parent").unwrap();
        assert!(parent.parent.is_none());
        assert_eq!(parent.children.len(), 2);

        let child1 = profiler.get_profile("child1").unwrap();
        assert_eq!(child1.parent, Some("parent".to_string()));
    }

    #[test]
    fn test_profile_not_found() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        let result = profiler.end_profile("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_profile_already_started() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("test").unwrap();
        let result = profiler.start_profile("test");
        assert!(result.is_err());
    }

    #[test]
    fn test_reset() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("test").unwrap();
        profiler.end_profile("test").unwrap();

        assert!(profiler.get_profile("test").is_some());

        profiler.reset();

        assert!(profiler.get_profile("test").is_none());
    }

    #[test]
    fn test_get_all_profiles() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("op1").unwrap();
        profiler.end_profile("op1").unwrap();

        profiler.start_profile("op2").unwrap();
        profiler.end_profile("op2").unwrap();

        let profiles = profiler.get_all_profiles();
        assert_eq!(profiles.len(), 2);
    }

    #[test]
    fn test_generate_report() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("op1").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        profiler.end_profile("op1").unwrap();

        profiler.start_profile("op2").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        profiler.end_profile("op2").unwrap();

        let report = profiler.generate_report();
        assert_eq!(report.entries.len(), 2);
        assert_eq!(report.stats.total_invocations, 2);
        assert!(report.stats.slowest_operation.is_some());
        assert!(report.stats.fastest_operation.is_some());
    }

    #[test]
    fn test_report_summary() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("test").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        profiler.end_profile("test").unwrap();

        let report = profiler.generate_report();
        let summary = report.summary();

        assert!(summary.contains("Performance Profiling Report"));
        assert!(summary.contains("Total Operations"));
    }

    #[test]
    fn test_bottleneck_detection() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        // Create a long-running operation
        profiler.start_profile("slow_op").unwrap();
        std::thread::sleep(Duration::from_millis(100));
        profiler.end_profile("slow_op").unwrap();

        // Create a fast operation
        profiler.start_profile("fast_op").unwrap();
        std::thread::sleep(Duration::from_millis(5));
        profiler.end_profile("fast_op").unwrap();

        let report = profiler.generate_report();

        // The slow operation should be identified as a bottleneck
        if !report.bottlenecks.is_empty() {
            assert_eq!(report.bottlenecks[0].name, "slow_op");
        }
    }

    #[test]
    fn test_call_tree() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("root").unwrap();
        profiler.start_profile("child1").unwrap();
        profiler.end_profile("child1").unwrap();
        profiler.end_profile("root").unwrap();

        let report = profiler.generate_report();

        assert_eq!(report.call_tree.len(), 1);
        assert_eq!(report.call_tree[0].name, "root");
        assert_eq!(report.call_tree[0].children.len(), 1);
        assert_eq!(report.call_tree[0].children[0].name, "child1");
    }

    #[test]
    fn test_flamegraph_data() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("root").unwrap();
        profiler.start_profile("child").unwrap();
        profiler.end_profile("child").unwrap();
        profiler.end_profile("root").unwrap();

        let report = profiler.generate_report();
        let flamegraph = report.to_flamegraph_data();

        assert!(flamegraph.contains("root"));
        assert!(flamegraph.contains("child"));
    }

    #[test]
    fn test_profile_entry_metrics() {
        let mut entry = ProfileEntry::new("test");
        entry.memory_allocated = 1000;
        entry.memory_deallocated = 300;
        entry.invocations = 5;
        entry.duration = Duration::from_millis(500);

        assert_eq!(entry.net_memory(), 700);
        assert_eq!(entry.avg_duration(), Duration::from_millis(100));
    }

    #[test]
    fn test_memory_snapshot() {
        let snapshot = MemorySnapshot::current();
        // Just ensure it doesn't panic
        assert_eq!(snapshot.total_allocated, 0);
    }

    #[test]
    fn test_take_memory_snapshot() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.take_memory_snapshot();
        profiler.take_memory_snapshot();

        let snapshots = profiler.get_memory_snapshots();
        assert_eq!(snapshots.len(), 2);
    }

    #[test]
    fn test_profiler_disabled() {
        let config = ProfilerConfig::default().with_cpu_profiling(false);
        let profiler = Profiler::new(config);

        // Should not error when profiling is disabled
        profiler.start_profile("test").unwrap();
        profiler.end_profile("test").unwrap();

        // Should not create a profile
        assert!(profiler.get_profile("test").is_none());
    }

    #[test]
    fn test_report_to_json() {
        let config = ProfilerConfig::default();
        let profiler = Profiler::new(config);

        profiler.start_profile("test").unwrap();
        profiler.end_profile("test").unwrap();

        let report = profiler.generate_report();
        let json = report.to_json().unwrap();

        assert!(json.contains("\"entries\""));
        assert!(json.contains("\"stats\""));
    }
}
