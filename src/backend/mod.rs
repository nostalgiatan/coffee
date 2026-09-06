//! Coffee Compiler Backend - LLVM Code Generation
//! 
//! This module uses inkwell (LLVM bindings) to generate native code. The backend
//! is responsible for translating Coffee's intermediate representation into
//! LLVM IR and then to native machine code. The backend provides functionality
//! for:
//! 
//! - Code generation for all Coffee language constructs
//! - LLVM IR generation and optimization
//! - Target-specific code generation for cross-compilation
//! - JIT execution for immediate code testing
//! - Object and assembly file generation
//! - Integration with C libraries and foreign function interface
//! 
//! The backend uses a modular design with separate modules for different aspects
//! of code generation such as arithmetic operations, control flow, memory management,
//! and function generation.

pub mod codegen;
pub mod context;
pub mod types;
pub mod class_layout;

// Modularized code generation components
pub mod arithmetic;
pub mod control_flow;
pub mod memory_ops;
pub mod memory;
pub mod functions;
pub mod mir_gen;
pub mod mir_expr;
pub mod mir_raise;
pub mod mir_return;
pub mod mir_nested;

// Additional modularized components
pub mod expressions;
pub mod error;
pub mod variables;
pub mod statements;
pub mod classes;
pub mod type_inference;
pub mod opt_passes;

// Re-export memory types for convenience
// Note: These are used by codegen/ but flagged as unused here
#[allow(unused_imports)]
pub use memory::{Layout, LayoutCollector, StructLayout};


use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::builder::Builder;
use inkwell::targets::{InitializationConfig, Target, TargetMachine, TargetTriple, RelocMode, CodeModel, FileType};
use inkwell::OptimizationLevel;
use std::path::Path;

/// Compiler backend using LLVM
/// 
/// The Backend struct manages the LLVM context, module, and builder for code generation.
/// It provides methods for generating different output formats (object files, assembly,
/// bitcode, IR) and supports cross-compilation through target triple configuration.
/// 
/// # Examples
/// 
/// ```rust
/// use inkwell::context::Context;
/// use crate::backend::Backend;
/// 
/// let context = Context::create();
/// let backend = Backend::new(&context, "my_module");
/// 
/// // Generate code using the backend...
/// 
/// // Write object file
/// use std::path::Path;
/// backend.write_object_file(Path::new("output.o")).unwrap();
/// ```
pub struct Backend<'ctx> {
    /// The LLVM context for this backend
    pub context: &'ctx Context,
    /// The LLVM module being generated
    pub module: Module<'ctx>,
    /// The LLVM builder for creating instructions
    pub builder: Builder<'ctx>,
    /// Target triple for cross-compilation
    target_triple: Option<String>,
    /// Codegen opt level passed to `Target::create_target_machine`
    opt_level: OptimizationLevel,
    /// CPU name for the target machine (LLVM default `"generic"`)
    cpu: String,
}

impl<'ctx> Backend<'ctx> {
    /// Create a new backend with the given context and module name
    /// 
    /// This constructor creates a backend that uses the default target triple for the current platform.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM context to use for code generation
    /// * `module_name` - The name to assign to the LLVM module being generated
    /// 
    /// # Returns
    /// 
    /// A new Backend instance
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        Self::with_target(context, module_name, None)
    }

    /// Create a new backend with a specific target triple for cross-compilation
    /// 
    /// This constructor allows specifying a target triple for cross-compilation scenarios.
    /// If no target triple is provided, the default target triple for the current platform is used.
    /// 
    /// # Arguments
    /// 
    /// * `context` - The LLVM context to use for code generation
    /// * `module_name` - The name to assign to the LLVM module being generated
    /// * `target_triple` - Optional target triple for cross-compilation (e.g., "x86_64-unknown-linux-gnu")
    /// 
    /// # Returns
    /// 
    /// A new Backend instance
    pub fn with_target(context: &'ctx Context, module_name: &str, target_triple: Option<String>) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        // Set target triple on the module if specified
        if let Some(ref triple_str) = target_triple {
            let triple = TargetTriple::create(triple_str);
            module.set_triple(&triple);
        }

        Backend {
            context,
            module,
            builder,
            target_triple,
            opt_level: OptimizationLevel::None,
            cpu: "generic".to_string(),
        }
    }

    /// Map Coffee `-O0`..`-O3` to LLVM `OptimizationLevel`.
    pub fn optimization_level(level: u8) -> OptimizationLevel {
        match level {
            0 => OptimizationLevel::None,
            1 => OptimizationLevel::Less,
            2 => OptimizationLevel::Default,
            _ => OptimizationLevel::Aggressive,
        }
    }

    /// Set the TargetMachine optimization level used when writing object/asm.
    pub fn set_opt_level(&mut self, level: OptimizationLevel) {
        self.opt_level = level;
    }

    /// Get the target triple being used
    /// 
    /// Returns the target triple configured for this backend. If no specific target triple
    /// was set, returns the default target triple for the current platform.
    /// 
    /// # Returns
    /// 
    /// The target triple as a string
    pub fn get_target_triple(&self) -> String {
        if let Some(ref triple) = self.target_triple {
            triple.clone()
        } else {
            // Get the actual triple string from the default target triple
            TargetMachine::get_default_triple()
                .as_str()
                .to_string_lossy()
                .to_string()
        }
    }

    /// Get LLVM IR as string
    /// 
    /// Returns the LLVM IR representation of the module as a string. This is useful
    /// for debugging and inspection of the generated code.
    /// 
    /// # Returns
    /// 
    /// The LLVM IR as a string
    pub fn get_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }

    /// Clean up target triple string for platform-specific quirks
    /// 
    /// This internal function handles platform-specific quirks in target triple strings.
    /// For example, Android may report "aarch64-linux-android24" which should be "aarch64-linux-android".
    /// 
    /// # Arguments
    /// 
    /// * `triple` - The target triple string to clean
    /// 
    /// # Returns
    /// 
    /// A cleaned version of the target triple
    fn clean_target_triple(triple: &str) -> String {
        // Remove Android API level if present (e.g., aarch64-linux-android24 -> aarch64-linux-android)
        if triple.contains("-android") {
            if let Some(pos) = triple.find("-android") {
                let after_android = &triple[pos + 8..];  // Skip "-android"
                if !after_android.is_empty() && after_android.chars().all(|c| c.is_ascii_digit()) {
                    return format!("{}-android", &triple[..pos]);
                }
            }
        }

        triple.to_string()
    }

    /// Verify the module
    /// 
    /// Performs verification on the LLVM module to ensure it is well-formed and
    /// follows LLVM's type system and instruction semantics rules.
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if the module is valid
    /// * `Err(String)` if the module fails verification
    pub fn verify(&self) -> Result<(), String> {
        self.module.verify()
            .map_err(|e| e.to_string())
    }

    /// Optimize the module with LLVM's new pass manager (`default<On>`).
    ///
    /// `OptimizationLevel::None` is a no-op. Target-machine or pass errors
    /// fail compilation so `-O1`/`-O2`/`-O3` never silently skip LLVM.
    pub fn optimize(&self, level: OptimizationLevel) -> Result<(), String> {
        let machine = self.create_target_machine().map_err(|err| {
            format!("optimize: failed to create target machine: {}", err)
        })?;
        opt_passes::apply(&self.module, &machine, level).map_err(|err| {
            format!("optimize: pass pipeline failed: {}", err)
        })
    }

    /// Write object file
    /// 
    /// Generates and writes an object file (.o) containing the compiled code.
    /// The target triple is used to generate platform-appropriate code.
    /// 
    /// # Arguments
    /// 
    /// * `path` - The path where the object file should be written
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if the file was written successfully
    /// * `Err(String)` if there was an error writing the file
    fn create_target_machine(&self) -> Result<TargetMachine, String> {
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| e.to_string())?;

        let triple_str = self.get_target_triple();
        let cleaned_triple = Self::clean_target_triple(&triple_str);
        let native_cleaned = Self::clean_target_triple(
            &TargetMachine::get_default_triple()
                .as_str()
                .to_string_lossy(),
        );
        // Explicit `--target` is cross-compile: never use host CPU/features.
        let is_native = self.target_triple.is_none() && cleaned_triple == native_cleaned;

        let host_cpu;
        let host_features;
        let (cpu, features): (&str, &str) = if self.cpu != "generic" {
            (self.cpu.as_str(), "")
        } else if is_native {
            host_cpu = TargetMachine::get_host_cpu_name().to_string();
            host_features = TargetMachine::get_host_cpu_features().to_string();
            (&host_cpu, &host_features)
        } else {
            ("generic", "")
        };

        let reloc = if is_native && cleaned_triple.contains("-android") {
            RelocMode::PIC
        } else {
            RelocMode::Default
        };

        let triple = TargetTriple::create(&cleaned_triple);
        let target = Target::from_triple(&triple)
            .map_err(|e| format!("Invalid target triple '{}': {}", cleaned_triple, e))?;
        target
            .create_target_machine(
                &triple,
                cpu,
                features,
                self.opt_level,
                reloc,
                CodeModel::Default,
            )
            .ok_or_else(|| format!(
                "failed to create LLVM target machine for '{}'\n  = help: check --target <triple> is a triple LLVM knows",
                cleaned_triple
            ))
    }

    fn llvm_write_err(kind: &str, path: &Path, detail: impl std::fmt::Display) -> String {
        format!(
            "failed to write {} '{}': {}\n  = help: check the output path is writable and the disk is not full",
            kind,
            path.display(),
            detail
        )
    }

    pub fn write_object_file(&self, path: &Path) -> Result<(), String> {
        self.create_target_machine()?
            .write_to_file(&self.module, FileType::Object, path)
            .map_err(|e| Self::llvm_write_err("object file", path, e))
    }

    /// Write assembly file
    /// 
    /// Generates and writes an assembly file (.s) containing the compiled code.
    /// The target triple is used to generate platform-appropriate assembly.
    /// 
    /// # Arguments
    /// 
    /// * `path` - The path where the assembly file should be written
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if the file was written successfully
    /// * `Err(String)` if there was an error writing the file
    pub fn write_assembly_file(&self, path: &Path) -> Result<(), String> {
        self.create_target_machine()?
            .write_to_file(&self.module, FileType::Assembly, path)
            .map_err(|e| Self::llvm_write_err("assembly", path, e))
    }

    /// Write LLVM bitcode (.bc).
    pub fn write_bitcode(&self, path: &Path) -> Result<(), String> {
        if self.module.write_bitcode_to_path(path) {
            Ok(())
        } else {
            Err(Self::llvm_write_err("LLVM bitcode", path, "LLVM write_bitcode_to_path returned false"))
        }
    }

    /// Write LLVM IR to file
    /// 
    /// Generates and writes an LLVM IR file (.ll) containing the human-readable
    /// representation of the compiled code.
    /// 
    /// # Arguments
    /// 
    /// * `path` - The path where the IR file should be written
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` if the file was written successfully
    /// * `Err(String)` if there was an error writing the file
    pub fn write_ir(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.get_ir())
            .map_err(|e| Self::llvm_write_err("LLVM IR", path, e))
    }
}

/// Compile and execute using JIT
/// 
/// This function compiles the provided Coffee program to native code using
/// LLVM's JIT compilation and then executes it immediately. This allows for
/// instant testing of Coffee programs without generating intermediate files.
/// The JIT execution also supports dynamic linking with C library functions
/// through the provided import list.
/// 
/// # Arguments
/// 
/// * `source` - The parsed Coffee program to compile and execute
/// * `c_imports` - List of C functions that are imported by the Coffee program
/// * `cfc_symbols` - HashMap of C function signature tables from .cfc files
/// * `hir_fns` - Frontend MIR functions (same handoff as AOT `compile_program_with_hir`)
/// * `user_args` - Command-line arguments to pass to the executed program
/// * `opt_level` - Same LLVM level AOT uses: IR `Backend::optimize` then the JIT engine
/// 
/// # Returns
/// 
/// * `Ok(i32)` - The exit code returned by the executed program
/// * `Err(String)` - Error message if compilation or execution failed
/// 
/// This allows using C library functions through dynamic linking
pub fn compile_and_run(
    source: &crate::parser::Program,
    c_imports: &[String],
    cfc_symbols: std::collections::HashMap<String, crate::c::CSymbolTable>,
    hir_fns: Vec<crate::hir::MirFn>,
    user_args: &[String],
    opt_level: OptimizationLevel,
) -> Result<i32, String> {
    use inkwell::targets::{InitializationConfig, Target};

    // Initialize native target for JIT
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| format!("Failed to initialize native target: {}", e))?;

    let context = Context::create();
    let mut backend = Backend::new(&context, "coffee_jit");
    backend.set_opt_level(opt_level);

    // Generate code with C imports and frontend MIR (same path as AOT)
    let mut codegen = codegen::CodeGenerator::new(&backend);
    codegen.compile_program_with_hir(source, c_imports, cfc_symbols, hir_fns)?;

    // Verify
    backend.verify()?;

    // Same IR pass pipeline as AOT (`OptimizationLevel::None` is a no-op).
    backend.optimize(opt_level)?;

    // Create JIT execution engine at the same opt level as AOT TargetMachine.
    let mut execution_engine = backend.module.create_jit_execution_engine(opt_level)
        .map_err(|e| format!("Failed to create JIT execution engine: {:?}", e))?;

    // Map external C functions to their actual addresses
    map_c_library_functions(&mut execution_engine, &backend.module, c_imports)?;

    // Find and run the main function
    let _main_fn = backend.module.get_function("main")
        .ok_or("No main() entry point found in this module.\n  = note: This module cannot be executed directly.\n  = note: Add 'main(entry_function())' to specify the program entry point.\n  = example: main(my_function())\n  = example with args: main(my_function(arg1, arg2))")?;

    unsafe {
        // SAFETY: This unsafe block is safe because:
        // 1. We ensure main function exists before calling it
        // 2. We properly convert Rust strings to C-compatible format using CString
        // 3. We correctly construct the argv array with proper null-termination
        // 4. The function signature matches the expected C calling convention (i32, *const *const i8) -> i32
        // 5. We properly manage memory allocation and ensure strings remain valid during execution
        
        // Get the function pointer
        let main_fn_ptr = execution_engine.get_function_address("main")
            .map_err(|e| format!("Failed to get main function address: {:?}", e))?;

        // Cast to function pointer
        type MainFn = unsafe extern "C" fn(i32, *const *const i8) -> i32;
        let main_fn: MainFn = std::mem::transmute(main_fn_ptr);

        // Build argv: [program_name, user_arg1, user_arg2, ...]
        let mut filtered_args = vec![std::env::args().next().unwrap_or_else(|| "coffee".to_string())];
        filtered_args.extend(user_args.iter().cloned());

        let argc = filtered_args.len() as i32;

        // Convert args to C-style strings
        let c_args: Vec<std::ffi::CString> = filtered_args
            .iter()
            .map(|s| {
                std::ffi::CString::new(s.as_bytes()).map_err(|e| {
                    format!(
                        "invalid NUL byte in JIT argv\n  = note: {}\n  = help: command-line arguments cannot contain interior NUL bytes",
                        e
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Create array of pointers
        let arg_ptrs: Vec<*const i8> = c_args
            .iter()
            .map(|s| s.as_ptr() as *const i8)
            .collect();

        let argv = arg_ptrs.as_ptr();

        // Execute
        let result = main_fn(argc, argv);

        Ok(result)
    }
}

/// Map C library functions to their actual addresses in the JIT execution engine
/// 
/// This function resolves C library function symbols and maps them to their actual
/// memory addresses in the JIT execution engine. This allows JIT-compiled Coffee code
/// to call C functions that are linked into the system. The function handles both
/// specific library:symbol imports and just symbol imports.
/// 
/// # Arguments
/// 
/// * `execution_engine` - The JIT execution engine to map functions to
/// * `module` - The LLVM module containing function declarations
/// * `c_imports` - List of C function imports in the format "library:symbol" or just "symbol"
/// 
/// # Returns
/// 
/// * `Ok(())` - Successfully mapped all functions
/// * `Err(String)` - Invalid symbol name or `dlsym` miss
/// 
/// This allows JIT-compiled Coffee code to call C functions
fn map_c_library_functions<'ctx>(
    execution_engine: &mut inkwell::execution_engine::ExecutionEngine<'ctx>,
    module: &inkwell::module::Module<'ctx>,
    c_imports: &[String],
) -> Result<(), String> {
    for c_import in c_imports {
        // Imports are `"library:symbol"` (or a bare symbol). LLVM decls use the symbol.
        let (library, symbol) = match c_import.split_once(':') {
            Some((lib, name)) => (lib, name),
            None => ("libc", c_import.as_str()),
        };
        let Some(llvm_func) = module.get_function(symbol) else {
            continue;
        };

        let func_name_cstr = std::ffi::CString::new(symbol.as_bytes())
            .map_err(|e| format!("Failed to create CString for C symbol '{}': {}", symbol, e))?;

        unsafe {
            // Get function address using dlsym with RTLD_DEFAULT
            // This searches in all loaded libraries including libc, libm, etc.
            let addr = libc::dlsym(libc::RTLD_DEFAULT, func_name_cstr.as_ptr());

            if addr.is_null() {
                return Err(format!(
                    "C function '{}' not found in loaded libraries\n  = note: JIT resolves C symbols with dlsym(RTLD_DEFAULT); library '{}' must already be loaded in this process\n  = help: import it with `use {} in {} of c`\n  = help: libc and libm are loaded by default; other libraries must be loaded before `--jit` (JIT does not link `-l` like `--bin`)",
                    symbol, library, symbol, library
                ));
            }

            execution_engine.add_global_mapping(&llvm_func, addr as usize);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    fn tiny_module(context: &Context) -> Backend<'_> {
        let backend = Backend::new(context, "get_ir_smoke");
        let i32_ty = context.i32_type();
        let fn_ty = i32_ty.fn_type(&[], false);
        let function = backend.module.add_function("answer", fn_ty, None);
        let entry = context.append_basic_block(function, "entry");
        backend.builder.position_at_end(entry);
        backend
            .builder
            .build_return(Some(&i32_ty.const_int(42, false)))
            .unwrap();
        backend
    }

    #[test]
    fn get_ir_returns_nonempty_llvm_ir() {
        let context = Context::create();
        let backend = tiny_module(&context);
        let ir = backend.get_ir();
        assert!(!ir.is_empty(), "LLVM IR string should not be empty");
        assert!(
            ir.contains("answer"),
            "IR should include the compiled function: {ir}"
        );
    }

    #[test]
    fn write_ir_persists_get_ir_text() {
        let context = Context::create();
        let backend = tiny_module(&context);
        let dir = std::env::temp_dir();
        let path = dir.join("coffee_backend_get_ir_smoke.ll");
        backend.write_ir(&path).expect("write_ir");
        let on_disk = std::fs::read_to_string(&path).expect("read .ll");
        let _ = std::fs::remove_file(&path);
        assert_eq!(on_disk, backend.get_ir());
    }
}
