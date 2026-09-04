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

// Modularized code generation components
pub mod arithmetic;
pub mod control_flow;
pub mod memory_ops;
pub mod memory;
pub mod functions;

// Additional modularized components
pub mod expressions;
pub mod error;
pub mod variables;
pub mod statements;
pub mod classes;
pub mod type_inference;

// Re-export memory types for convenience
// Note: These are used by codegen.rs but flagged as unused here
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
        }
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

    /// Optimize the module
    /// 
    /// Applies optimizations to the LLVM module. This implementation is a placeholder
    /// as in inkwell 0.7+, the pass manager API has changed and optimization is
    /// typically handled by the target machine during code generation.
    /// 
    /// # Arguments
    /// 
    /// * `_level` - The optimization level to apply (currently unused)
    /// 
    /// Note: inkwell 0.7+ uses new pass manager API.
    /// Optimization is handled by the target machine during code generation.
    pub fn optimize(&self, _level: OptimizationLevel) {
        // In inkwell 0.7+, the pass manager API has changed to use LLVM's new pass manager.
        // Manual pass configuration requires different setup than older versions.
        // The target machine applies optimizations based on the level when writing output files.

        // The new pass manager in LLVM 17+ requires different setup.
        // Basic compilation relies on the target machine's optimization level.
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
    pub fn write_object_file(&self, path: &Path) -> Result<(), String> {
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| e.to_string())?;

        // Use configured target triple or default
        let triple_str = self.get_target_triple();

        // Clean up the triple string for Android platforms
        // Android triples may have API level appended (e.g., "aarch64-linux-android24")
        // We need to remove the API level to get valid target triple
        let cleaned_triple = Self::clean_target_triple(&triple_str);

        let triple = TargetTriple::create(&cleaned_triple);
        let target = Target::from_triple(&triple)
            .map_err(|e| format!("Invalid target triple '{}': {}", cleaned_triple, e))?;
        let target_machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or("Failed to create target machine")?;

        target_machine
            .write_to_file(&self.module, FileType::Object, path)
            .map_err(|e| e.to_string())
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
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| e.to_string())?;

        // Use configured target triple or default
        let triple_str = self.get_target_triple();
        let cleaned_triple = Self::clean_target_triple(&triple_str);
        let triple = TargetTriple::create(&cleaned_triple);
        let target = Target::from_triple(&triple)
            .map_err(|e| format!("Invalid target triple '{}': {}", cleaned_triple, e))?;

        let target_machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or("Failed to create target machine")?;

        target_machine
            .write_to_file(&self.module, FileType::Assembly, path)
            .map_err(|e| e.to_string())
    }

    /// Write LLVM bitcode
    /// 
    /// Generates and writes an LLVM bitcode file (.bc) containing the compiled code.
    /// 
    /// # Arguments
    /// 
    /// * `path` - The path where the bitcode file should be written
    /// 
    /// # Returns
    /// 
    /// * `true` if the file was written successfully
    /// * `false` if there was an error writing the file
    pub fn write_bitcode(&self, path: &Path) -> bool {
        self.module.write_bitcode_to_path(path)
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
        self.module.print_to_file(path)
            .map_err(|e| e.to_string())
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
/// * `user_args` - Command-line arguments to pass to the executed program
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
    user_args: &[String],
) -> Result<i32, String> {
    use inkwell::targets::{InitializationConfig, Target};

    // Initialize native target for JIT
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| format!("Failed to initialize native target: {}", e))?;

    let context = Context::create();
    let backend = Backend::new(&context, "coffee_jit");

    // Generate code with C imports
    let mut codegen = codegen::CodeGenerator::new(&backend);
    codegen.compile_program(source, c_imports, cfc_symbols)?;

    // Verify
    backend.verify()?;

    // Get IR for debugging (available for debugging purposes)
    let _ir = backend.get_ir();

    // Create JIT execution engine
    let mut execution_engine = backend.module.create_jit_execution_engine(inkwell::OptimizationLevel::None)
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
            .map(|s| std::ffi::CString::new(s.as_bytes()).unwrap())
            .collect();

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
/// * `Ok(())` - Successfully mapped all available functions
/// * `Err(String)` - Error if a symbol name is invalid
/// 
/// This allows JIT-compiled Coffee code to call C functions
fn map_c_library_functions<'ctx>(
    execution_engine: &mut inkwell::execution_engine::ExecutionEngine<'ctx>,
    module: &inkwell::module::Module<'ctx>,
    c_imports: &[String],
) -> Result<(), String> {
    for function_name in c_imports {
        let function = module.get_function(function_name);

        if let Some(llvm_func) = function {
            // Try to get the function address from the process
            // This works for functions in standard libraries like libc, libm, etc.
            let func_name_cstr = std::ffi::CString::new(function_name.as_bytes())
                .map_err(|e| format!("Failed to create CString: {}", e))?;

            unsafe {
                // Get function address using dlsym with RTLD_DEFAULT
                // This searches in all loaded libraries including libc, libm, etc.
                let addr = libc::dlsym(libc::RTLD_DEFAULT, func_name_cstr.as_ptr());

                if !addr.is_null() {
                    // Successfully found the function, add mapping to JIT
                    let func_addr = addr as usize;
                    execution_engine.add_global_mapping(&llvm_func, func_addr);
                } else {
                    // Function not found - this will cause a runtime error if called
                    // Return a warning but don't fail compilation
                    eprintln!("Warning: C function '{}' not found in loaded libraries. This may cause runtime errors if called.", function_name);
                }
            }
        }
    }

    Ok(())
}
