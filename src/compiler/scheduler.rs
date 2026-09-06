//! Compilation Scheduling and Batch Management
//! 
//! This module handles scheduling of compilation units into batches for parallel
//! compilation. The scheduler analyzes dependencies between compilation units
//! and groups them into batches that can be compiled concurrently while respecting
//! dependency relationships.
//! 
//! The scheduler uses the dependency graph to determine which modules can be
//! compiled in parallel and which must wait for their dependencies to complete.
//! This enables efficient utilization of multiple CPU cores during the compilation
//! process, significantly reducing build times for large projects.
//! 
//! The module also includes a progress reporting system to provide feedback on
//! compilation status to the user.

use std::collections::HashMap;
use std::path::PathBuf;
use std::io::{self, Write};
use super::unit::{CompilationUnit, CompilationStatus};
use super::graph::DependencyGraph;

/// Compilation scheduler
/// 
/// Schedules compilation units into batches based on dependencies for efficient
/// parallel compilation. The scheduler uses dependency information to determine
/// which modules can be compiled concurrently and which must wait for their
/// dependencies to complete.
/// 
/// The scheduler is responsible for creating a compilation schedule that:
/// - Respects dependency relationships between modules
/// - Maximizes parallelism where possible
/// - Detects and reports circular dependencies
/// - Groups modules into batches that can be compiled together
/// 
/// # Examples
/// 
/// ```rust
/// use std::collections::HashMap;
/// use crate::compiler::scheduler::CompilationScheduler;
/// use crate::compiler::unit::CompilationUnit;
/// 
/// let mut units = HashMap::new();
/// // Add units to the map...
/// 
/// let scheduler = CompilationScheduler::new(units);
/// let schedule = scheduler.schedule().unwrap();
/// 
/// for (batch_num, batch) in schedule.iter().enumerate() {
///     println!("Batch {}: {:?}", batch_num + 1, batch);
/// }
/// ```
pub struct CompilationScheduler {
    /// All compilation units
    /// The collection of all compilation units in the project
    units: HashMap<String, CompilationUnit>,
    /// Dependency graph
    /// Used to analyze dependencies and schedule units into batches
    graph: DependencyGraph,
}

impl CompilationScheduler {
    /// Create a new scheduler from compilation units
    /// 
    /// Initializes a scheduler with the given compilation units. The scheduler
    /// will build a dependency graph from these units to determine the compilation
    /// schedule.
    /// 
    /// # Arguments
    /// 
    /// * `units` - A HashMap containing compilation units keyed by module name
    /// 
    /// # Returns
    /// 
    /// A new CompilationScheduler instance
    pub fn new(units: HashMap<String, CompilationUnit>) -> Self {
        let graph = DependencyGraph::from_units(&units);
        CompilationScheduler { units, graph }
    }

    /// Schedule compilation into parallel batches
    /// 
    /// Creates a compilation schedule by grouping units into batches where
    /// all units in a batch can be compiled in parallel since they don't
    /// depend on each other and all their dependencies have been satisfied
    /// by previous batches.
    /// 
    /// # Returns
    /// 
    /// * `Ok(Vec<Vec<String>>)` - A vector of batches, each containing module names to compile in parallel
    /// * `Err(String)` - Error message if scheduling fails (e.g., circular dependencies)
    pub fn schedule(&self) -> Result<Vec<Vec<String>>, String> {
        self.schedule_parallel()
    }

    /// Schedule using parallel batch algorithm
    /// 
    /// Internal method that implements the parallel batch scheduling algorithm
    /// using the dependency graph.
    /// 
    /// # Returns
    /// 
    /// * `Ok(Vec<Vec<String>>)` - A vector of batches for parallel compilation
    /// * `Err(String)` - Error message if scheduling fails
    fn schedule_parallel(&self) -> Result<Vec<Vec<String>>, String> {
        // Check for cycles first
        if self.graph.has_cycles() {
            return Err("circular dependencies detected in project".to_string());
        }

        // Use the dependency graph's scheduling
        self.graph.schedule_parallel()
    }

    /// Get mutable reference to a compilation unit
    /// 
    /// Retrieves a mutable reference to a specific compilation unit by its name.
    /// This is useful for updating unit status during compilation.
    /// 
    /// # Arguments
    /// 
    /// * `name` - The module name of the unit to retrieve
    /// 
    /// # Returns
    /// 
    /// * `Some(&mut CompilationUnit)` - Mutable reference to the unit if found
    /// * `None` - If no unit with the given name exists
    pub fn get_unit_mut(&mut self, name: &str) -> Option<&mut CompilationUnit> {
        self.units.get_mut(name)
    }

    /// Collect all object file paths
    /// 
    /// Gathers paths to all object files that have been successfully compiled
    /// (i.e., units with status CompilationStatus::Compiled).
    /// 
    /// # Returns
    /// 
    /// A vector of paths to all compiled object files
    pub fn object_files(&self) -> Vec<PathBuf> {
        self.units.values()
            .filter_map(|unit| {
                if let CompilationStatus::Compiled(obj) = &unit.status {
                    Some(obj.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if there are no units
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

/// Progress reporter for compilation
/// 
/// Provides progress reporting for the compilation process, showing batch
/// and module compilation status to the user. The reporter handles output
/// formatting and ensures that all output is properly flushed to maintain
/// real-time visibility of compilation progress.
/// 
/// The progress reporter is used during the compilation process to provide
/// feedback to users about which modules are being compiled and their status.
pub struct ProgressReporter {
    /// Current batch number
    /// The sequence number of the currently processing batch (1-indexed)
    batch_num: usize,
    /// Total batches
    /// The total number of batches in the compilation schedule
    total_batches: usize,
}

impl ProgressReporter {
    /// Create a new progress reporter
    /// 
    /// Initializes a progress reporter for a specific batch in the compilation schedule.
    /// 
    /// # Arguments
    /// 
    /// * `batch_num` - The number of the current batch (1-indexed)
    /// * `total_batches` - The total number of batches in the schedule
    /// 
    /// # Returns
    /// 
    /// A new ProgressReporter instance
    pub fn new(batch_num: usize, total_batches: usize) -> Self {
        ProgressReporter { batch_num, total_batches }
    }

    /// Report start of a batch
    /// 
    /// Outputs information about the start of a compilation batch, including
    /// the batch number and the number of modules in the batch.
    /// 
    /// # Arguments
    /// 
    /// * `module_count` - The number of modules in this batch
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - Successfully reported the batch start
    /// * `Err(io::Error)` - Error if writing to stdout failed
    pub fn report_batch_start(&self, module_count: usize) -> io::Result<()> {
        println!("  Batch {}/{} ({} module(s)):", self.batch_num, self.total_batches, module_count);
        io::stdout().flush()
    }

    /// Report compilation of a single module
    /// 
    /// Outputs information about starting compilation of a specific module,
    /// showing the module name and indicating that compilation is in progress.
    /// 
    /// # Arguments
    /// 
    /// * `module_name` - The name of the module being compiled
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - Successfully reported the module start
    /// * `Err(io::Error)` - Error if writing to stdout failed
    pub fn report_module_start(&self, module_name: &str) -> io::Result<()> {
        print!("    - {} ... ", module_name);
        io::stdout().flush()
    }

    /// Report successful compilation
    /// 
    /// Outputs a success indicator for a completed module compilation and
    /// ensures the output is flushed.
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - Successfully reported the success
    /// * `Err(io::Error)` - Error if writing to stdout failed
    pub fn report_success(&self) -> io::Result<()> {
        println!("OK");
        io::stdout().flush()
    }

    /// Report compilation failure
    /// 
    /// Outputs a failure indicator for a module compilation, including the
    /// error message, and ensures the output is flushed.
    /// 
    /// # Arguments
    /// 
    /// * `error` - The error message describing the compilation failure
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - Successfully reported the failure
    /// * `Err(io::Error)` - Error if writing to stdout failed
    pub fn report_failure(&self, error: &str) -> io::Result<()> {
        println!("FAIL: {}", error);
        io::stdout().flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_creation() {
        let units = HashMap::new();
        let scheduler = CompilationScheduler::new(units);
        assert!(scheduler.is_empty());
    }
}
