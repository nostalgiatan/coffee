//! Project builder - coordinates compilation of Coffee projects
//!
//! The ProjectBuilder acts as a coordinator, delegating specialized tasks to:
//! - EntryPointManager: entry point selection and validation
//! - CompilationScheduler: compilation scheduling and batching
//! - SourceScanner: source file discovery and scanning
//! - CompilationPipeline: compilation of individual modules
//! - Linker: object file linking

use std::path::{Path, PathBuf};
use std::fs;
use std::sync::{Arc, Mutex};
use rayon::prelude::*;

use crate::parser;
use super::project::ProjectConfig;
use super::pkg;
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
    /// Compiler frontend (shared `CompilationPipeline` type)
    frontend: CompilerFrontend,
    /// Target triple for cross-compilation
    target_triple: Option<String>,
    /// Force static linking for all libraries
    force_static: bool,
    /// CLI `--static-lib` names; empty path so the linker uses `-Wl,-Bstatic -lname`
    static_lib_names: Vec<String>,
    /// Requested output kind (IR skips clang link)
    emit: EmitKind,
    /// Optional `-o` path
    output_file: Option<PathBuf>,
    /// Generate C headers (`c fn` / exportable fns) after a successful build
    have_c: bool,
    enable_bitfields: bool,
    enable_safety: bool,
    show_memory: bool,
    /// Package import roots (src/ or package root), prepended onto CompilerConfig.
    pkg_import_roots: Vec<PathBuf>,
}

/// Print `--show-memory` reports on the main thread (same header as single-file mode).
fn print_memory_layout_reports(reports: &[String]) {
    if reports.is_empty() {
        return;
    }
    println!("\n=== Memory Layout Report ===\n");
    for report in reports {
        println!("{}", report);
    }
}

impl ProjectBuilder {
    /// Create a new project builder
    pub fn new(config: ProjectConfig) -> Self {
        let entry_manager = EntryPointManager::from_config(config.clone());
        let have_c = config.build.have_c;
        ProjectBuilder {
            config,
            entry_manager,
            units: CompilationUnits::new(),
            frontend: CompilerFrontend::new(),
            target_triple: None,
            force_static: false,
            static_lib_names: Vec::new(),
            emit: EmitKind::Binary,
            output_file: None,
            have_c,
            enable_bitfields: false,
            enable_safety: false,
            show_memory: false,
            pkg_import_roots: Vec::new(),
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

    /// Request static linking for named libraries (`--static-lib`).
    ///
    /// Each name is stored as `(name, "")` in [`LinkOptions::static_libs`] so the
    /// linker emits `-Wl,-Bstatic -lname -Wl,-Bdynamic` when no `.a` path is found.
    pub fn set_static_lib_names(&mut self, names: &[String]) {
        self.static_lib_names = names
            .iter()
            .map(|n| n.trim())
            .filter(|n| !n.is_empty())
            .map(|n| n.to_string())
            .collect();
    }

    /// Set emit kind (LLVM IR / bitcode / assembly skip linking)
    pub fn set_emit(&mut self, emit: EmitKind) {
        self.emit = emit;
    }

    /// Set `-o` output path
    pub fn set_output_file(&mut self, output_file: Option<PathBuf>) {
        self.output_file = output_file;
    }

    /// CLI `-O0`..`-O3` overrides `coffee.toml` `[target] opt_level`.
    pub fn set_opt_level(&mut self, level: u8) {
        self.config.target.opt_level = level.min(3);
    }

    /// Enable C header emission (`--have-c`). Toml `[build] have_c` is OR'd in `new()`.
    pub fn set_have_c(&mut self, have_c: bool) {
        if have_c {
            self.have_c = true;
        }
    }

    pub fn set_enable_bitfields(&mut self, enable: bool) {
        self.enable_bitfields = enable;
    }

    pub fn set_enable_safety(&mut self, enable: bool) {
        self.enable_safety = enable;
    }

    pub fn set_show_memory(&mut self, enable: bool) {
        self.show_memory = enable;
    }

    fn prepend_pkg_import_paths(&self, config: &mut super::CompilerConfig) {
        let mut paths = self.pkg_import_roots.clone();
        paths.append(&mut config.import_paths);
        config.import_paths = paths;
    }

    /// Auto-generated `.cfc` dir: `<project root>/<target_dir>/cfc`.
    fn cfc_search_dir(&self) -> PathBuf {
        self.config
            .resolve_from_root(&self.config.build.target_dir)
            .join("cfc")
    }

    fn append_cfc_search_path(&self, config: &mut super::CompilerConfig) {
        let dir = self.cfc_search_dir();
        if !config.import_paths.iter().any(|p| p == &dir) {
            config.import_paths.push(dir);
        }
    }

    fn standard_c_include_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Ok(prefix) = std::env::var("PREFIX") {
            dirs.push(PathBuf::from(prefix).join("include"));
        }
        dirs.push(PathBuf::from("/usr/include"));
        dirs.push(PathBuf::from("/usr/local/include"));
        dirs
    }

    fn resolve_c_header(&self, header: &str, include_paths: &[String]) -> Option<PathBuf> {
        let given = Path::new(header);
        if given.is_absolute() {
            return given.exists().then(|| given.to_path_buf());
        }
        let from_root = self.config.resolve_from_root(given);
        if from_root.exists() {
            return Some(from_root);
        }
        for inc in include_paths {
            let candidate = Path::new(inc).join(header);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        for dir in Self::standard_c_include_dirs() {
            let candidate = dir.join(header);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    fn cfc_newer_than_headers(out: &Path, headers: &[PathBuf]) -> bool {
        let Ok(out_meta) = fs::metadata(out) else {
            return false;
        };
        let Ok(out_mtime) = out_meta.modified() else {
            return false;
        };
        if headers.is_empty() {
            return false;
        }
        for header in headers {
            let Ok(meta) = fs::metadata(header) else {
                return false;
            };
            let Ok(mtime) = meta.modified() else {
                return false;
            };
            if mtime > out_mtime {
                return false;
            }
        }
        true
    }

    /// Auto-generate `target_dir/cfc/lib<key>.cfc` for declared C libraries with headers.
    pub fn ensure_c_library_cfcs(&self) -> Result<(), String> {
        let entries: Vec<(String, super::project::CLibrary)> = self
            .config
            .dependencies
            .c_libraries
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        for (key, lib) in entries {
            if key == "libc" || key == "libm" {
                continue;
            }
            if lib.headers.is_empty() {
                continue;
            }

            let mut resolved = Vec::new();
            for header in &lib.headers {
                if let Some(path) = self.resolve_c_header(header, &lib.include_paths) {
                    resolved.push(path);
                }
            }
            let Some(root_header) = resolved.first().cloned() else {
                let first = lib.headers.first().map(|s| s.as_str()).unwrap_or("");
                let mut searched: Vec<String> = lib.include_paths.clone();
                searched.extend(
                    Self::standard_c_include_dirs()
                        .iter()
                        .map(|p| p.display().to_string()),
                );
                return Err(crate::c::diag::header_not_found(first, &searched).format_plain());
            };

            let out = self.cfc_search_dir().join(format!("lib{}.cfc", key));
            if Self::cfc_newer_than_headers(&out, &resolved) {
                continue;
            }

            fs::create_dir_all(self.cfc_search_dir())
                .map_err(|e| format!("failed to create {}: {}", self.cfc_search_dir().display(), e))?;

            crate::c::generator::generate_cfc(&root_header, Some(&out), &lib.include_paths)?;
        }
        Ok(())
    }

    fn codegen_frontend(&self) -> CompilerFrontend {
        let mut config = super::CompilerConfig::default();
        config.opt_level = self.config.target.opt_level.min(3);
        config.target_triple = self.target_triple.clone();
        config.enable_bitfields = self.enable_bitfields;
        config.enable_safety = self.enable_safety;
        config.show_memory = self.show_memory;
        self.prepend_pkg_import_paths(&mut config);
        self.append_cfc_search_path(&mut config);
        CompilerFrontend::with_config(config)
    }

    /// Scan project directory for .cf files
    pub fn scan_sources(&mut self) -> Result<Vec<PathBuf>, String> {
        let scanner = SourceScanner::new(self.config.src_dir_path());
        scanner.scan()
    }

    /// Build compilation units from source files
    pub fn build_units(&mut self, sources: &[PathBuf]) -> Result<(), String> {
        for source in sources {
            // Calculate module name from file path
            let module_name = self.module_name_from_path(source)?;

            // Calculate object file path
            let rel_path = source.strip_prefix(self.config.src_dir_path())
                .map_err(|_| format!("source file not in src_dir: {}", source.display()))?;
            let module_path = rel_path.to_str().unwrap_or("").replace('/', ".");
            let module_path = module_path.strip_suffix(".cf").unwrap_or(&module_path);

            let object_file = self.config.output_dir()
                .join(format!("{}.o", module_path));

            let mut unit = CompilationUnit::new(module_name, source.clone(), object_file);

            // Parse to extract dependencies and check for main
            let source_code = fs::read_to_string(source)
                .map_err(|e| format!("failed to read '{}': {}", source.display(), e))?;

            let program = self.frontend.parse_source(&source_code, Some(source.to_str().unwrap_or("")))
                .map_err(|diagnostics| {
                    // Format each diagnostic error clearly
                    let mut error_msg = format!("parse error in '{}':\n", source.display());
                    for diag in &diagnostics {
                        error_msg.push_str(&format!("{}\n", diag.format()));
                    }
                    error_msg
                })?;

            // Check for multiple main() in single file (syntax rule - applies to all files)
            let main_count = CompilerFrontend::count_entry_points(&program);
            if main_count > 1 {
                return Err(format!("file '{}' contains {} main() statements (only 1 allowed per file)",
                    source.display(), main_count));
            }

            // Extract dependencies
            for stmt in &program.statements {
                if let parser::Statement::Import(import) = stmt {
                    let module_path = match import {
                        parser::Import::Simple { path } => path.clone(),
                        parser::Import::Aliased { path, .. } => path.clone(),
                        parser::Import::InModule { path, .. } => path.clone(),
                        parser::Import::InModuleWithLang { module, lang, .. } => {
                            if lang == "c" {
                                continue;
                            }
                            module.clone()
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
        let rel_path = path.strip_prefix(self.config.src_dir_path())
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
                            if let Ok(program) = self.frontend.parse_source(&source_code, Some(unit.source.to_str().unwrap_or(""))) {
                                return CompilerFrontend::count_entry_points(&program);
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

        let resolved = pkg::resolve_packages(&self.config)?;
        self.pkg_import_roots = resolved.into_iter().map(|p| p.import_root).collect();
        if !self.config.dependencies.packages.contains_key("std") {
            self.pkg_import_roots.insert(0, pkg::std_import_root()?);
        }

        self.ensure_c_library_cfcs()?;

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
            let out = self.emit_entry_artifact()?;
            self.maybe_emit_c_headers()?;
            return Ok(out);
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
                let results: Arc<Mutex<Vec<(String, Result<Option<String>, String>)>>> =
                    Arc::new(Mutex::new(Vec::new()));
                let opt_level = self.config.target.opt_level.min(3);
                let target_triple = self.target_triple.clone();
                let enable_bitfields = self.enable_bitfields;
                let enable_safety = self.enable_safety;
                let show_memory = self.show_memory;
                let pkg_import_roots = self.pkg_import_roots.clone();
                let cfc_dir = self.cfc_search_dir();

                modules_to_compile.par_iter().for_each(|(name, source, object_path, is_entry)| {
                    let mut cfg = super::CompilerConfig::default();
                    cfg.opt_level = opt_level;
                    cfg.target_triple = target_triple.clone();
                    cfg.enable_bitfields = enable_bitfields;
                    cfg.enable_safety = enable_safety;
                    cfg.show_memory = show_memory;
                    let mut paths = pkg_import_roots.clone();
                    paths.append(&mut cfg.import_paths);
                    cfg.import_paths = paths;
                    if !cfg.import_paths.iter().any(|p| p == &cfc_dir) {
                        cfg.import_paths.push(cfc_dir.clone());
                    }
                    let mut frontend = CompilerFrontend::with_config(cfg);

                    // Create a temporary unit for compilation
                    let mut unit = CompilationUnit::new(name.clone(), source.clone(), object_path.clone());

                    // Calculate hash
                    if let Err(e) = unit.calculate_hash() {
                        let mut results = results.lock().unwrap();
                        results.push((name.clone(), Err(e)));
                        return;
                    }

                    // Compile the module (memory reports returned, not printed on workers)
                    let result = frontend.compile_module_to_object(&mut unit, *is_entry);

                    // Store result
                    let mut results = results.lock().unwrap();
                    results.push((name.clone(), result.map_err(|e| format!("compilation error: {}", e))));
                });

                // Process results and update scheduler
                let results = results.lock().unwrap();
                let mut memory_reports: Vec<String> = Vec::new();
                for (name, result) in results.iter() {
                    reporter.report_module_start(name)
                        .map_err(|e| format!("I/O error: {}", e))?;

                    match result {
                        Ok(report) => {
                            // Update unit status in scheduler
                            if let Some(unit) = scheduler.get_unit_mut(name) {
                                unit.status = CompilationStatus::Compiled(unit.object.clone());
                            }
                            if let Some(report) = report {
                                memory_reports.push(report.clone());
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
                print_memory_layout_reports(&memory_reports);
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
            self.maybe_emit_c_headers()?;
            return Ok(output_path);
        }

        let link_opts = self.collect_link_options();
        for (lib_name, _) in &link_opts.static_libs {
            println!("  Linking statically: {}", lib_name);
        }

        let linker = Linker::with_options(&output_path, link_opts);
        linker.link(&object_files)?;

        println!("  Done: {}", output_path.display());

        self.maybe_emit_c_headers()?;
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

        let mut frontend = self.codegen_frontend();
        let memory_report = frontend.emit_module(&mut entry_unit, true, self.emit, Some(&out))?;

        println!("  Wrote: {}", out.display());
        if let Some(report) = memory_report {
            print_memory_layout_reports(&[report]);
        }
        Ok(out)
    }

    /// Build [`LinkOptions`] from coffee.toml C deps plus CLI `--static` / `--static-lib`.
    fn collect_link_options(&self) -> LinkOptions {
        let mut link_opts = LinkOptions::default();

        for (lib_name, is_static, static_path) in self.config.get_c_library_link_types() {
            if is_static {
                link_opts.static_libs.push((lib_name, static_path));
            } else {
                link_opts.libraries.push(lib_name);
            }
        }

        // include_paths are `-I` for header generate only, not linker `-L`.

        for flag in self.config.get_c_link_flags() {
            link_opts.link_flags.push(flag);
        }

        for extra in self.needs_from_generated_cfc() {
            if extra == "c" || extra == "m" || extra == "libc" || extra == "libm" {
                continue;
            }
            if !link_opts.libraries.iter().any(|n| n == &extra)
                && !link_opts.static_libs.iter().any(|(n, _)| n == &extra)
            {
                link_opts.libraries.push(extra);
            }
        }

        link_opts.force_static = self.force_static;
        link_opts.opt_level = self.config.target.opt_level.min(3);
        link_opts.target_triple = self.target_triple.clone();

        for name in &self.static_lib_names {
            if link_opts.static_libs.iter().any(|(n, _)| n == name) {
                continue;
            }
            link_opts.static_libs.push((name.clone(), String::new()));
        }

        link_opts
    }

    /// Linker names listed in `// needs:` of auto-generated `target/cfc` tables.
    fn needs_from_generated_cfc(&self) -> Vec<String> {
        let dir = self.cfc_search_dir();
        let mut extra = Vec::new();
        for key in self.config.dependencies.c_libraries.keys() {
            let path = dir.join(format!("lib{key}.cfc"));
            if !path.exists() {
                continue;
            }
            let Ok(table) = crate::c::parse_cfc_file(&path) else {
                continue;
            };
            for need in table.needs {
                if !extra.iter().any(|e| e == &need) {
                    extra.push(need);
                }
            }
        }
        extra
    }

    /// Write `{output_dir}/include/{stem}.h` for each module when `--have-c` or toml `have_c`.
    fn maybe_emit_c_headers(&self) -> Result<(), String> {
        if !self.have_c {
            return Ok(());
        }

        let include_dir = self.config.output_dir().join("include");
        fs::create_dir_all(&include_dir).map_err(|e| {
            format!("failed to create header directory '{}': {}", include_dir.display(), e)
        })?;

        for (_name, unit) in self.units.all() {
            let source_code = fs::read_to_string(&unit.source)
                .map_err(|e| format!("failed to read '{}': {}", unit.source.display(), e))?;
            let program = self.frontend
                .parse_source(&source_code, Some(unit.source.to_str().unwrap_or("")))
                .map_err(|diagnostics| {
                    let mut error_msg = format!("parse error in '{}':\n", unit.source.display());
                    for diag in &diagnostics {
                        error_msg.push_str(&format!("{}\n", diag.format()));
                    }
                    error_msg
                })?;

            let functions: Vec<crate::parser::function::Function> = program.statements
                .iter()
                .filter_map(|stmt| {
                    if let parser::Statement::Function(f) = stmt {
                        Some(f.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let classes: Vec<crate::parser::class::ClassDef> = program.statements
                .iter()
                .filter_map(|stmt| {
                    if let parser::Statement::Class(c) = stmt {
                        Some(c.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let stem = unit
                .source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("coffee");
            let header_path = include_dir.join(format!("{}.h", stem));
            if let Err(e) = crate::c::header_gen::generate_header(
                &functions,
                &classes,
                &header_path,
                stem,
            ) {
                eprintln!("Warning: Failed to generate C header file: {}", e);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_config() -> ProjectConfig {
        toml::from_str(
            r#"
[package]
name = "static_lib_wire"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"
"#,
        )
        .expect("dummy coffee.toml")
    }

    #[test]
    fn parse_source_program_feeds_import_extract_and_entry_count() {
        let src = "use foo\nfn main() => int:\n    return 0\n";
        let frontend = CompilerFrontend::new();
        let program = frontend
            .parse_source(src, None)
            .expect("parse_source");
        assert_eq!(
            CompilerFrontend::count_entry_points(&program),
            1,
            "count_entry_points must accept parse_source Program without Program::new"
        );
        let mut import_paths = Vec::new();
        for stmt in &program.statements {
            if let parser::Statement::Import(import) = stmt {
                match import {
                    parser::Import::Simple { path } => import_paths.push(path.clone()),
                    parser::Import::Aliased { path, .. } => import_paths.push(path.clone()),
                    parser::Import::InModule { path, .. } => import_paths.push(path.clone()),
                    parser::Import::InModuleWithLang { .. } => {}
                }
            }
        }
        assert_eq!(import_paths, vec!["foo".to_string()]);
        assert_eq!(program.stmt_spans.len(), program.statements.len());
        assert!(
            program
                .stmt_spans
                .iter()
                .all(|s| *s != crate::types::definition::Span::new(0, 0)),
            "builder must not wrap parse_source with Program::new"
        );
    }

    #[test]
    fn set_static_lib_names_feeds_empty_path_link_options() {
        let mut builder = ProjectBuilder::new(dummy_config());
        builder.set_static_lib_names(&[
            "  m  ".to_string(),
            String::new(),
            "curl".to_string(),
        ]);
        let opts = builder.collect_link_options();
        assert_eq!(
            opts.static_libs,
            vec![
                ("m".to_string(), String::new()),
                ("curl".to_string(), String::new()),
            ]
        );
        assert!(!opts.force_static);
    }

    #[test]
    fn collect_link_options_does_not_copy_include_paths_to_lib_paths() {
        let config: ProjectConfig = toml::from_str(
            r#"
[package]
name = "inc_not_l"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"

[dependencies.c_libraries.z]
name = "z"
headers = ["zlib.h"]
include_paths = ["/usr/include"]
link_flags = ["-pthread"]
"#,
        )
        .expect("toml");
        let builder = ProjectBuilder::new(config);
        let opts = builder.collect_link_options();
        assert!(
            !opts.lib_paths.iter().any(|p| p == "/usr/include"),
            "include_paths are -I for header generate, not linker -L: {:?}",
            opts.lib_paths
        );
        assert!(opts.libraries.contains(&"z".to_string()));
        assert!(opts.link_flags.iter().any(|f| f == "-pthread"));
    }

    #[test]
    fn set_opt_level_overrides_toml_and_feeds_link_options() {
        let mut builder = ProjectBuilder::new(dummy_config());
        assert_eq!(builder.collect_link_options().opt_level, 0);
        builder.set_opt_level(3);
        assert_eq!(builder.collect_link_options().opt_level, 3);
    }

    #[test]
    fn have_c_from_toml_or_set_have_c() {
        let mut builder = ProjectBuilder::new(dummy_config());
        assert!(!builder.have_c);
        builder.set_have_c(true);
        assert!(builder.have_c);

        let with_toml = toml::from_str::<ProjectConfig>(
            r#"
[package]
name = "hdr"
version = "0.0.0"

[build]
src_dir = "src"
main = "src/main"
have_c = true
"#,
        )
        .expect("toml");
        let builder = ProjectBuilder::new(with_toml);
        assert!(builder.have_c);
    }

    #[test]
    fn set_enable_bitfields_and_safety_are_stored() {
        let mut builder = ProjectBuilder::new(dummy_config());
        assert!(!builder.enable_bitfields);
        assert!(!builder.enable_safety);
        assert!(!builder.show_memory);
        builder.set_enable_bitfields(true);
        builder.set_enable_safety(true);
        builder.set_show_memory(true);
        assert!(builder.enable_bitfields);
        assert!(builder.enable_safety);
        assert!(builder.show_memory);
    }
}
