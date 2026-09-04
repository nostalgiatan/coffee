//! Project builder - coordinates compilation of Coffee projects
//!
//! The ProjectBuilder acts as a coordinator, delegating specialized tasks to:
//! - EntryPointManager: entry point selection and validation
//! - CompilationScheduler: compilation scheduling and batching
//! - SourceScanner: source file discovery and scanning
//! - CompilerFrontend: actual compilation of individual modules
//! - Linker: object file linking

use std::path::{Path, PathBuf};
use std::fs;
use std::sync::{Arc, Mutex};
use rayon::prelude::*;

use crate::parser;
use super::project::ProjectConfig;
use super::entry_point::EntryPointManager;
use super::scheduler::{CompilationScheduler, ProgressReporter};
use super::scanner::SourceScanner;
use super::linker::{Linker, LinkOptions};
use super::unit::{CompilationUnit, CompilationUnits, CompilationStatus};
use super::graph::DependencyGraph;
use super::{CompilerFrontend, EmitKind};

/// Project builder - coordinates compilation of Coffee projects
pub struct ProjectBuilder {
    /// Project configuration
    config: ProjectConfig,
    /// Entry point manager
    entry_manager: EntryPointManager,
    /// Compilation units storage
    pub units: CompilationUnits,
    /// Compiler frontend
    frontend: CompilerFrontend,
    /// Target triple for cross-compilation
    target_triple: Option<String>,
    /// Force static linking for all libraries
    force_static: bool,
    /// Requested output kind (IR skips clang link)
    emit: EmitKind,
    /// Optional `-o` path
    output_file: Option<PathBuf>,
}

impl ProjectBuilder {
    /// Create a new project builder
    pub fn new(config: ProjectConfig) -> Self {
        let entry_manager = EntryPointManager::from_config(config.clone());
        ProjectBuilder {
            config,
            entry_manager,
            units: CompilationUnits::new(),
            frontend: CompilerFrontend::new(),
            target_triple: None,
            force_static: false,
            emit: EmitKind::Binary,
            output_file: None,
        }
    }

    /// Set a custom entry file (overrides config.build.main)
    pub fn set_custom_entry(&mut self, entry: &str) {
        self.entry_manager.set_custom_entry(entry);
    }

    /// Set the target triple for cross-compilation
    pub fn set_target_triple(&mut self, target: &str) {
        self.target_triple = Some(target.to_string());
    }

    /// Force static linking for all libraries
    pub fn set_force_static(&mut self, force_static: bool) {
        self.force_static = force_static;
    }

    /// Set emit kind (LLVM IR / bitcode / assembly skip linking)
    pub fn set_emit(&mut self, emit: EmitKind) {
        self.emit = emit;
    }

    /// Set `-o` output path
    pub fn set_output_file(&mut self, output_file: Option<PathBuf>) {
        self.output_file = output_file;
    }

    /// Get the entry point manager
    #[allow(dead_code)]
    pub fn entry_manager(&self) -> &EntryPointManager {
        &self.entry_manager
    }

    /// Derive object file path from source file path
    /// Matches the logic in build_units
    #[allow(dead_code)]
    fn derive_object_path(source: &PathBuf, output_dir: &PathBuf) -> PathBuf {
        // Get file stem without extension
        let file_stem = source.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("module");

        // Get parent directory and convert to module path
        if let Some(parent) = source.parent() {
            let parent_str = parent.to_str().unwrap_or("");
            // Remove src/ prefix if present
            let rel_path = parent_str.trim_start_matches("src/").trim_start_matches("src\\");
            let module_path = if rel_path.is_empty() {
                file_stem.to_string()
            } else {
                format!("{}.{}", rel_path.replace('/', ".").replace('\\', "."), file_stem)
            };
            output_dir.join(format!("{}.o", module_path))
        } else {
            output_dir.join(format!("{}.o", file_stem))
        }
    }

    /// Scan project directory for .cf files
    pub fn scan_sources(&mut self) -> Result<Vec<PathBuf>, String> {
        let scanner = SourceScanner::new(&self.config.build.src_dir);
        scanner.scan()
    }

    /// Build compilation units from source files
    pub fn build_units(&mut self, sources: &[PathBuf]) -> Result<(), String> {
        for source in sources {
            // Calculate module name from file path
            let module_name = self.module_name_from_path(source)?;

            // Calculate object file path
            let rel_path = source.strip_prefix(&self.config.build.src_dir)
                .map_err(|_| format!("source file not in src_dir: {}", source.display()))?;
            let module_path = rel_path.to_str().unwrap_or("").replace('/', ".");
            let module_path = module_path.strip_suffix(".cf").unwrap_or(&module_path);

            let object_file = self.config.output_dir()
                .join(format!("{}.o", module_path));

            let mut unit = CompilationUnit::new(module_name, source.clone(), object_file);

            // Parse to extract dependencies and check for main
            let source_code = fs::read_to_string(source)
                .map_err(|e| format!("failed to read '{}': {}", source.display(), e))?;

            let statements = self.frontend.parse_source(&source_code, Some(source.to_str().unwrap_or("")))
                .map_err(|diagnostics| {
                    // Format each diagnostic error clearly
                    let mut error_msg = format!("parse error in '{}':\n", source.display());
                    for diag in &diagnostics {
                        error_msg.push_str(&format!("{}\n", diag.format()));
                    }
                    error_msg
                })?;

            // Check for multiple main() in single file (syntax rule - applies to all files)
            let main_count = CompilerFrontend::count_entry_points(&parser::Program { statements: statements.clone() });
            if main_count > 1 {
                return Err(format!("file '{}' contains {} main() statements (only 1 allowed per file)",
                    source.display(), main_count));
            }

            // Extract dependencies
            for stmt in &statements {
                if let parser::Statement::Import(import) = stmt {
                    let module_path = match import {
                        parser::Import::Simple { path } => path.clone(),
                        parser::Import::Aliased { path, .. } => path.clone(),
                        parser::Import::InModule { path, .. } => path.clone(),
                        parser::Import::InModuleWithLang { .. } => {
                            // C library imports don't add module dependencies
                            continue;
                        }
                    };
                    unit.add_dependency(module_path);
                }
            }

            // Calculate hash for dirty detection
            unit.calculate_hash()?;

            self.units.add(unit);
        }

        Ok(())
    }

    /// Convert file path to module name
    fn module_name_from_path(&self, path: &Path) -> Result<String, String> {
        let rel_path = path.strip_prefix(&self.config.build.src_dir)
            .map_err(|_| format!("source file not in src_dir: {}", path.display()))?;

        let mut parts: Vec<String> = Vec::new();
        for component in rel_path.components() {
            if let Some(name) = component.as_os_str().to_str() {
                if name.ends_with(".cf") {
                    parts.push(name.trim_end_matches(".cf").to_string());
                } else {
                    parts.push(name.to_string());
                }
            }
        }

        Ok(parts.join("."))
    }

    /// Validate project structure
    pub fn validate(&self) -> Result<(), String> {
        // Use EntryPointManager to validate entry point
        self.entry_manager.validate(
            |path| path.exists(),
            |path| {
                // Count main() statements in the file
                for (_name, unit) in self.units.all() {
                    if unit.source == *path {
                        if let Ok(source_code) = fs::read_to_string(&unit.source) {
                            if let Ok(statements) = self.frontend.parse_source(&source_code, Some(unit.source.to_str().unwrap_or(""))) {
                                return CompilerFrontend::count_entry_points(&parser::Program { statements });
                            }
                        }
                        break;
                    }
                }
                0
            }
        )?;

        // Check for circular dependencies
        let graph = DependencyGraph::from_units(self.units.all());
        if graph.has_cycles() {
            return Err("circular dependencies detected in project".to_string());
        }

        Ok(())
    }

    /// Compile all modules
    pub fn compile(&mut self) -> Result<PathBuf, String> {
        println!("Compiling project: {}", self.config.package.name);
        println!("  Version: {}", self.config.package.version);
        println!("  Opt level: O{}", self.config.target.opt_level);

        // 1. Scan sources
        println!("\nScanning sources...");
        let sources = self.scan_sources()?;
        println!("  Found {} source file(s)", sources.len());

        // 2. Build units
        println!("\nBuilding compilation units...");
        self.build_units(&sources)?;
        println!("  Created {} unit(s)", self.units.all().len());

        // 3. Validate
        println!("\nValidating project...");
        self.validate()?;

        // Intermediate artifacts: entry module only, no clang link
        if matches!(self.emit, EmitKind::LlvmIr | EmitKind::Bitcode | EmitKind::Assembly) {
            return self.emit_entry_artifact();
        }

        // 4. Create scheduler and get compilation schedule
        println!("\nBuilding dependency graph...");
        let mut scheduler = CompilationScheduler::new(self.units.all().to_owned());
        let schedule = scheduler.schedule()?;
        println!("  Planned {} batch(es)", schedule.len());

        // 5. Compile in batches with parallel execution
        println!("\nCompiling modules...");
        let entry_file = self.entry_manager.get_entry_file();

        for (batch_num, batch) in schedule.iter().enumerate() {
            let reporter = ProgressReporter::new(batch_num + 1, schedule.len());
            reporter.report_batch_start(batch.len())
                .map_err(|e| format!("I/O error: {}", e))?;

            // First pass: Check which modules need compilation and mark cached ones
            let mut modules_to_compile: Vec<(String, PathBuf, PathBuf, bool)> = Vec::new();
            let mut cached_modules: Vec<String> = Vec::new();

            for module_name in batch {
                if let Some(unit) = scheduler.get_unit_mut(module_name) {
                    let needs_recompile = match unit.check_hash_cache() {
                        Ok(cached) => !cached,
                        Err(_) => true,
                    };

                    if needs_recompile {
                        let is_entry = unit.source == entry_file;
                        modules_to_compile.push((unit.name.clone(), unit.source.clone(), unit.object.clone(), is_entry));
                    } else {
                        unit.status = CompilationStatus::Compiled(unit.object.clone());
                        cached_modules.push(unit.name.clone());
                    }
                }
            }

            // Report cached modules first
            for name in &cached_modules {
                println!("    - {} (cached)", name);
            }

            // Parallel compilation of modules that need recompilation
            if !modules_to_compile.is_empty() {
                let results: Arc<Mutex<Vec<(String, Result<(), String>)>>> =
                    Arc::new(Mutex::new(Vec::new()));

                modules_to_compile.par_iter().for_each(|(name, source, object_path, is_entry)| {
                    // Create independent compiler frontend for this thread
                    let mut frontend = CompilerFrontend::new();

                    // Create a temporary unit for compilation
                    let mut unit = CompilationUnit::new(name.clone(), source.clone(), object_path.clone());

                    // Calculate hash
                    if let Err(e) = unit.calculate_hash() {
                        let mut results = results.lock().unwrap();
                        results.push((name.clone(), Err(e)));
                        return;
                    }

                    // Compile the module
                    let result = frontend.compile_module_to_object(&mut unit, *is_entry);

                    // Store result
                    let mut results = results.lock().unwrap();
                    results.push((name.clone(), result.map_err(|e| format!("compilation error: {}", e))));
                });

                // Process results and update scheduler
                let results = results.lock().unwrap();
                for (name, result) in results.iter() {
                    reporter.report_module_start(name)
                        .map_err(|e| format!("I/O error: {}", e))?;

                    match result {
                        Ok(_) => {
                            // Update unit status in scheduler
                            if let Some(unit) = scheduler.get_unit_mut(name) {
                                unit.status = CompilationStatus::Compiled(unit.object.clone());
                            }
                            reporter.report_success()
                                .map_err(|e| format!("I/O error: {}", e))?;
                        }
                        Err(e) => {
                            reporter.report_failure(e)
                                .map_err(|io_err| format!("I/O error: {}", io_err))?;
                            return Err(format!("failed to compile {}", name));
                        }
                    }
                }
            }
        }

        // 6. Link
        println!("\nLinking...");
        let output_path = self.config.output_dir().join(self.config.executable_name());

        // Collect object files from scheduler
        let object_files = scheduler.object_files();

        // Check if we need to link
        if object_files.is_empty() && output_path.exists() {
            // All modules were cached and executable exists
            println!("  Up to date: {}", output_path.display());
            return Ok(output_path);
        }

        // Link using Linker with C library support
        let mut link_opts = LinkOptions::default();

        // Get C library link types (static/dynamic)
        let lib_link_types = self.config.get_c_library_link_types();

        // Separate static and dynamic libraries
        for (lib_name, is_static, static_path) in lib_link_types {
            if is_static {
                // Static library
                link_opts.static_libs.push((lib_name.clone(), static_path));
                println!("  Linking statically: {}", lib_name);
            } else {
                // Dynamic library
                link_opts.libraries.push(lib_name.clone());
            }
        }

        // Add include paths as library search paths
        for lib_path in self.config.get_c_include_paths() {
            link_opts.lib_paths.push(lib_path);
        }

        // Add linker flags
        for flag in self.config.get_c_link_flags() {
            link_opts.link_flags.push(flag);
        }

        // Set force_static flag if enabled
        link_opts.force_static = self.force_static;

        let linker = Linker::with_options(&output_path, link_opts);
        linker.link(&object_files)?;

        println!("  Done: {}", output_path.display());

        Ok(output_path)
    }

    /// Write IR/bitcode/assembly for the entry module only (v1; no clang link).
    fn emit_entry_artifact(&mut self) -> Result<PathBuf, String> {
        let entry_file = self.entry_manager.get_entry_file();
        let mut entry_unit = self.units
            .all()
            .values()
            .find(|u| u.source == entry_file)
            .cloned()
            .ok_or_else(|| format!("entry file '{}' has no compilation unit", entry_file.display()))?;

        let default_path = {
            let stem = entry_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("a");
            match self.emit {
                EmitKind::LlvmIr => PathBuf::from(format!("{}.ll", stem)),
                EmitKind::Bitcode => PathBuf::from(format!("{}.bc", stem)),
                EmitKind::Assembly => PathBuf::from(format!("{}.s", stem)),
                _ => entry_unit.object.clone(),
            }
        };
        let out = self.output_file.clone().unwrap_or(default_path);

        let mut frontend = CompilerFrontend::new();
        frontend.emit_module(&mut entry_unit, true, self.emit, Some(&out))?;

        println!("  Wrote: {}", out.display());
        Ok(out)
    }
}
