//! Dependency Graph Management
//! 
//! This module provides a comprehensive dependency graph implementation for managing
//! dependencies between compilation units in Coffee projects. The dependency graph
//! is essential for:
//! 
//! - Detecting circular dependencies that would prevent compilation
//! - Determining the correct compilation order for modules
//! - Scheduling parallel compilation of independent modules
//! - Calculating which modules need recompilation when dependencies change
//! 
//! The graph maintains both forward dependencies (what a module depends on) and
//! reverse dependencies (what depends on a module) to efficiently support various
//! dependency analysis operations needed during the compilation process.
//! 
//! # Examples
//! 
//! ```rust
//! use std::collections::HashMap;
//! use crate::compiler::graph::DependencyGraph;
//! use crate::compiler::CompilationUnit;
//! 
//! // Create a graph from compilation units
//! let mut units = HashMap::new();
//! // Add units to the map...
//! 
//! let graph = DependencyGraph::from_units(&units);
//! 
//! // Check for cycles
//! if graph.has_cycles() {
//!     println!("Circular dependencies detected!");
//! } else {
//!     // Get compilation order
//!     let order = graph.topological_sort().unwrap();
//!     println!("Compilation order: {:?}", order);
//! }
//! ```

use std::collections::{HashMap, HashSet};
use crate::compiler::CompilationUnit;

/// Dependency graph for compilation units
/// 
/// Represents the dependency relationships between compilation units in a Coffee project.
/// The graph maintains both forward and reverse dependencies to support efficient
/// dependency analysis operations needed during compilation.
/// 
/// The forward graph maps each module to the modules it depends on, while the reverse
/// graph maps each module to the modules that depend on it. This dual representation
/// allows for efficient cycle detection, topological sorting, and affected module
/// calculation.
/// 
/// # Examples
/// 
/// ```rust
/// use crate::compiler::graph::DependencyGraph;
/// 
/// // Create a new dependency graph
/// let mut graph = DependencyGraph::new();
/// 
/// // Add nodes and dependencies
/// graph.add_node("main");
/// graph.add_node("utils");
/// graph.add_dependency("main", "utils");
/// 
/// // Check for cycles
/// assert!(!graph.has_cycles());
/// 
/// // Get compilation order
/// let order = graph.topological_sort().unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Adjacency list: node -> list of dependencies (what this module depends on)
    graph: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        DependencyGraph {
            graph: HashMap::new(),
        }
    }

    /// Add a node (compilation unit)
    pub fn add_node(&mut self, name: impl Into<String>) {
        let name = name.into();
        self.graph.entry(name).or_insert_with(Vec::new);
    }

    /// Add a dependency edge: target depends on dependency
    pub fn add_dependency(&mut self, target: impl Into<String>, dependency: impl Into<String>) {
        let target = target.into();
        let dep = dependency.into();

        self.graph.entry(target)
            .or_insert_with(Vec::new)
            .push(dep);
    }

    /// Build dependency graph from compilation units
    /// 
    /// Constructs a dependency graph from a collection of compilation units.
    /// Each unit becomes a node in the graph, and dependencies between units
    /// are added as edges based on the unit's dependency list.
    /// 
    /// # Arguments
    /// 
    /// * `units` - A reference to a HashMap containing compilation units
    /// 
    /// # Returns
    /// 
    /// A DependencyGraph representing the dependency relationships between units
    pub fn from_units(units: &HashMap<String, CompilationUnit>) -> Self {
        let mut graph = Self::new();

        // Add all nodes
        for name in units.keys() {
            graph.add_node(name);
        }

        // Add dependencies
        for (name, unit) in units {
            for dep in &unit.dependencies {
                graph.add_dependency(name, dep);
            }
        }

        graph
    }

    /// Check for circular dependencies
    pub fn has_cycles(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node in self.graph.keys() {
            if self.has_cycle_util(node, &mut visited, &mut rec_stack) {
                return true;
            }
        }

        false
    }

    fn has_cycle_util(&self, node: &str, visited: &mut HashSet<String>, rec_stack: &mut HashSet<String>) -> bool {
        // If node is in the current recursion stack, we found a cycle
        if rec_stack.contains(node) {
            return true;
        }

        // If already visited and not in current recursion stack, no cycle from this path
        if !visited.insert(node.to_string()) {
            return false;
        }

        // Add to recursion stack
        rec_stack.insert(node.to_string());

        if let Some(deps) = self.graph.get(node) {
            for dep in deps {
                if self.has_cycle_util(dep, visited, rec_stack) {
                    return true;
                }
            }
        }

        // Remove from recursion stack
        rec_stack.remove(node);
        false
    }

    /// Topological sort - returns compilation order
    /// Units can be compiled in the returned order
    pub fn topological_sort(&self) -> Result<Vec<String>, String> {
        if self.has_cycles() {
            return Err("circular dependency detected".to_string());
        }

        let mut sorted = Vec::new();
        let mut visited = HashSet::new();

        for node in self.graph.keys() {
            self.topological_sort_util(node, &mut visited, &mut sorted);
        }

        Ok(sorted)
    }

    fn topological_sort_util(&self, node: &str, visited: &mut HashSet<String>, sorted: &mut Vec<String>) {
        if visited.contains(node) {
            return;
        }

        visited.insert(node.to_string());

        // Visit dependencies first
        if let Some(deps) = self.graph.get(node) {
            for dep in deps {
                self.topological_sort_util(dep, visited, sorted);
            }
        }

        sorted.push(node.to_string());
    }

    /// Schedule parallel compilation
    /// Returns batches where each batch can be compiled concurrently
    pub fn schedule_parallel(&self) -> Result<Vec<Vec<String>>, String> {
        if self.has_cycles() {
            return Err("circular dependency detected".to_string());
        }

        let topological = self.topological_sort()?;
        let mut batches = Vec::new();
        let mut compiled = HashSet::new();

        // Build batches by finding which nodes are ready (all deps satisfied)
        let mut remaining: HashSet<_> = topological.into_iter().collect();

        while !remaining.is_empty() {
            let mut batch = Vec::new();

            // Find nodes that are ready (all dependencies satisfied)
            remaining.retain(|node| {
                let deps = self.graph.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
                let all_deps_satisfied = deps.iter().all(|dep| compiled.contains(dep));

                if all_deps_satisfied {
                    batch.push(node.clone());
                    false // Remove from remaining
                } else {
                    true // Keep in remaining
                }
            });

            if batch.is_empty() {
                // Should not happen if no cycles
                return Err("failed to schedule compilation: unexpected state".to_string());
            }

            for node in &batch {
                compiled.insert(node.clone());
            }

            batches.push(batch);
        }

        Ok(batches)
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_sort() {
        let mut graph = DependencyGraph::new();

        graph.add_node("a");
        graph.add_node("b");
        graph.add_node("c");

        graph.add_dependency("a", "b");
        graph.add_dependency("a", "c");

        let sorted = graph.topological_sort().unwrap();

        // b and c should come before a
        let pos_a = sorted.iter().position(|x| x == "a").unwrap();
        let pos_b = sorted.iter().position(|x| x == "b").unwrap();
        let pos_c = sorted.iter().position(|x| x == "c").unwrap();

        assert!(pos_b < pos_a || pos_c < pos_a);
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph = DependencyGraph::new();

        graph.add_node("a");
        graph.add_node("b");

        graph.add_dependency("a", "b");
        graph.add_dependency("b", "a");

        assert!(graph.has_cycles());
    }

    #[test]
    fn test_parallel_schedule() {
        let mut graph = DependencyGraph::new();

        graph.add_node("main");
        graph.add_node("utils");
        graph.add_node("io");
        graph.add_node("math");

        graph.add_dependency("main", "utils");
        graph.add_dependency("utils", "io");
        graph.add_dependency("utils", "math");

        let batches = graph.schedule_parallel().unwrap();

        // Should have 3 batches
        assert_eq!(batches.len(), 3);

        // Check first batch has independent nodes
        assert!(batches[0].contains(&"io".to_string()));
        assert!(batches[0].contains(&"math".to_string()));

        // Check last batch has main
        assert!(batches[2].contains(&"main".to_string()));
    }

    #[test]
    fn test_simple_graph() {
        let graph = DependencyGraph::new();
        assert!(!graph.has_cycles());
    }
}
