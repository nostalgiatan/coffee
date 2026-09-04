//! Code Generator - Transforms AST to LLVM IR
//! 
//! This module contains the core code generation logic that transforms Coffee
//! abstract syntax trees (AST) into LLVM intermediate representation (IR).
//! The code generator handles all aspects of the translation process:
//! 
//! - Function declaration and definition
//! - Variable allocation and management
//! - Expression evaluation and code generation
//! - Control flow constructs (if, while, for loops)
//! - Memory operations and safety checks
//! - C library integration and FFI support
//! - Type conversion and mapping
//! - Runtime safety features
//! 
//! The code generator uses a modular design with dedicated contexts for different
//! aspects of code generation such as arithmetic, memory management, and control flow.

use crate::coffee_debug;
use super::Backend;
use super::types::TypeMapper;

// Import modularized components
use super::control_flow::LoopContext;
use super::arithmetic::ArithmeticContext;
use super::memory_ops::{MemoryContext, DiagSeverity};
use super::functions;
use super::memory::{LayoutCollector, safety::SafetyContext};

// Import C FFI support
use crate::c;

use crate::parser::{Program, Statement};
use crate::parser::function::{Function, FunctionBody};

use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};
use inkwell::types::BasicTypeEnum;
use inkwell::types::AnyTypeEnum;
use inkwell::IntPredicate;
use inkwell::FloatPredicate;
use inkwell::AddressSpace;

use std::collections::HashMap;

/// Code generator state
/// 
/// The CodeGenerator struct manages the entire code generation process, tracking
/// variables, functions, types, and various compilation contexts. It serves as
/// the main coordinator for transforming Coffee source code into LLVM IR.
/// 
/// The generator maintains several key pieces of state:
/// - Variable mappings from names to LLVM values
/// - Function definitions and declarations
/// - Type conversion mappings
/// - Loop and control flow contexts
/// - Memory management information
/// - Safety and security checks
/// - C library integration data
/// 
/// # Examples
/// 
/// ```rust
/// use inkwell::context::Context;
/// use crate::backend::{Backend, codegen::CodeGenerator};
/// use crate::parser::Program;
/// 
/// let context = Context::create();
/// let backend = Backend::new(&context, "example");
/// let mut codegen = CodeGenerator::new(&backend);
/// 
/// // Compile a program
/// let program = Program::empty();
/// codegen.compile_program(&program, &[], std::collections::HashMap::new()).unwrap();
/// ```
pub struct CodeGenerator<'a, 'ctx> {
    /// Backend instance for LLVM code generation
    pub backend: &'a Backend<'ctx>,
    /// Type mapper for Coffee to LLVM type conversion
    pub type_mapper: TypeMapper<'ctx>,
    /// Arithmetic context for safe arithmetic operations
    pub arithmetic_ctx: ArithmeticContext<'ctx>,
    /// Memory context for tracking memory operations
    pub memory_ctx: MemoryContext<'ctx>,
    /// Variable name -> (LLVM pointer, type) mapping
    pub variables: HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    /// Variable name -> class name mapping (for method calls)
    pub variable_types: HashMap<String, String>,
    /// Function name -> LLVM function mapping
    pub functions: HashMap<String, FunctionValue<'ctx>>,
    /// Function name -> error handler name mapping (for invoke path)
    pub function_error_handlers: HashMap<String, Option<String>>,
    /// Current function being compiled
    pub current_function: Option<FunctionValue<'ctx>>,
    /// String constants
    pub string_constants: HashMap<String, PointerValue<'ctx>>,
    /// Loop context stack for break/continue
    pub loop_stack: Vec<LoopContext<'ctx>>,
    /// Array length tracking (variable name -> length pointer)
    pub array_lengths: HashMap<String, PointerValue<'ctx>>,
    /// Array element type tracking (variable name -> element type)
    pub array_element_types: HashMap<String, BasicTypeEnum<'ctx>>,
    /// Array allocas tracking (variable name -> actual array alloca for GEP)
    pub array_allocas: HashMap<String, PointerValue<'ctx>>,
    /// Array sizes tracking (variable name -> size)
    pub array_sizes: HashMap<String, u32>,
    /// Main entry point (function name and args)
    pub main_entry: Option<(String, Vec<String>)>,
    /// Current function's entry->body successor (for terminator restoration)
    pub entry_successor: Option<inkwell::basic_block::BasicBlock<'ctx>>,
    /// CRITICAL-5 FIX: Track total stack allocation size to prevent stack overflow
    pub current_stack_size: usize,
    /// MEDIUM-3 FIX: Track loop nesting depth to prevent stack overflow from deeply nested loops
    pub loop_nesting_depth: usize,
    /// HIGH-10 FIX: Track which variables are used to detect unused variables
    pub used_variables: std::collections::HashSet<String>,
    /// HIGH-13 FIX: Track expression nesting depth to prevent stack overflow
    pub expression_depth: usize,
    /// Maximum expression nesting depth (safety limit)
    pub max_expression_depth: usize,
    /// Current function's error handler name (if any)
    pub current_error_handler: Option<String>,
    /// Current function's parameter names for error context
    pub current_function_params: Vec<String>,
    /// Memory layout collector for reporting
    pub layout_collector: LayoutCollector,
    /// C library imports (for external symbol resolution)
    pub c_imports: Vec<String>,
    /// C function symbol tables from .cfc files
    pub cfc_symbols: std::collections::HashMap<String, c::CSymbolTable>,
    /// Enable bit fields support
    pub enable_bitfields: bool,
    /// Enable runtime safety checks
    pub enable_safety: bool,
    /// Bit field layouts for classes (class name -> (field name -> bit field info))
    pub bit_field_layouts: std::collections::HashMap<String, std::collections::HashMap<String, (u8, u8, u8)>>,
    /// Safety context for runtime checks
    pub safety_ctx: SafetyContext<'ctx>,
    /// Main function's argc (for arg1, arg2, ... access)
    pub main_argc: Option<inkwell::values::IntValue<'ctx>>,
    /// Flag to track if Error class is needed (for raise statements and error handlers)
    pub needs_error_class: bool,
    /// Main function's argv (for arg1, arg2, ... access)
    pub main_argv: Option<inkwell::values::PointerValue<'ctx>>,
    /// C functions that are actually used in the code (for lazy declaration)
    pub used_c_functions: std::collections::HashSet<String>,
    /// F-string templates collected during compilation (template -> function name)
    pub fstring_templates: std::collections::HashMap<String, String>,
    /// Class definitions (class name -> ClassDef)
    pub classes: std::collections::HashMap<String, crate::parser::class::ClassDef>,
    /// Enum definitions (enum name -> EnumDef)
    pub enums: std::collections::HashMap<String, crate::parser::class::EnumDef>,
}

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Create a new code generator instance
    /// 
    /// This constructor initializes a code generator with the given backend instance
    /// and sets up all necessary contexts and mappings for code generation. The
    /// generator is ready to compile Coffee programs after initialization.
    /// 
    /// # Arguments
    /// 
    /// * `backend` - A reference to the LLVM backend instance to use for code generation
    /// 
    /// # Returns
    /// 
    /// A new CodeGenerator instance with all internal state initialized
    pub fn new(backend: &'a Backend<'ctx>) -> Self {
        let type_mapper = TypeMapper::new(backend.context);
        let arithmetic_ctx = ArithmeticContext::new();
        let memory_ctx = MemoryContext::new();
        let safety_ctx = SafetyContext::new();
        CodeGenerator {
            backend,
            type_mapper,
            arithmetic_ctx,
            memory_ctx,
            safety_ctx,
            variables: HashMap::new(),
            variable_types: HashMap::new(),
            functions: HashMap::new(),
            function_error_handlers: HashMap::new(),
            current_function: None,
            string_constants: HashMap::new(),
            loop_stack: Vec::new(),
            array_lengths: HashMap::new(),
            array_element_types: HashMap::new(),
            array_allocas: HashMap::new(),
            array_sizes: HashMap::new(),
            main_entry: None,
            entry_successor: None,
            current_stack_size: 0,
            loop_nesting_depth: 0,
            used_variables: std::collections::HashSet::new(),
            expression_depth: 0,
            max_expression_depth: 1000,  // Safety limit to prevent stack overflow
            current_error_handler: None,
            current_function_params: Vec::new(),
            layout_collector: LayoutCollector::new(),
            c_imports: Vec::new(),
            cfc_symbols: std::collections::HashMap::new(),
            enable_bitfields: false,
            enable_safety: false,
            bit_field_layouts: std::collections::HashMap::new(),
            main_argc: None,
            main_argv: None,
            needs_error_class: false,
            used_c_functions: std::collections::HashSet::new(),
            fstring_templates: std::collections::HashMap::new(),
            classes: std::collections::HashMap::new(),
            enums: std::collections::HashMap::new(),
        }
    }

    /// Enable or disable bit fields support
    /// 
    /// This method enables or disables support for bit fields in Coffee classes.
    /// When enabled, the code generator will handle packed classes with bit field
    /// specifications and generate appropriate LLVM IR for bit field operations.
    /// 
    /// # Arguments
    /// 
    /// * `enable` - Boolean indicating whether to enable bit fields support
    pub fn enable_bitfields(&mut self, enable: bool) {
        self.enable_bitfields = enable;
    }

    /// Enable or disable runtime safety checks
    /// 
    /// This method enables or disables runtime safety checks during code generation.
    /// When enabled, the code generator will insert additional safety checks into
    /// the generated LLVM IR to prevent common runtime errors like buffer overflows,
    /// null pointer dereferences, and arithmetic overflows. The panic function
    /// will be configured during the program compilation phase.
    /// 
    /// # Arguments
    /// 
    /// * `enable` - Boolean indicating whether to enable runtime safety checks
    pub fn enable_safety(&mut self, enable: bool) {
        self.enable_safety = enable;
        // Note: panic function will be configured during compile_program
        // after runtime functions are declared
    }

    /// Check if a function name is from a C library import
    /// 
    /// This method determines whether a given function name corresponds to a
    /// function that was imported from a C library. This is used during code
    /// generation to handle C library functions differently from Coffee functions,
    /// particularly for linking and calling convention purposes.
    /// 
    /// C imports are stored in the format "library:symbol" or just "symbol" when
    /// the library name is not specified.
    /// 
    /// # Arguments
    /// 
    /// * `func_name` - The name of the function to check
    /// 
    /// # Returns
    /// 
    /// * `true` if the function is a C library import
    /// * `false` otherwise
    pub fn is_c_library_function(&self, func_name: &str) -> bool {
        // Check if the function matches any C import
        // C imports are stored as "library:symbol" or just "symbol"
        for c_import in &self.c_imports {
            if c_import.contains(':') {
                // Format is "library:symbol"
                let parts: Vec<&str> = c_import.split(':').collect();
                if parts.len() == 2 && parts[1] == func_name {
                    return true;
                }
            } else if c_import == func_name {
                // Direct match
                return true;
            }
        }

        // Check if it's a built-in C function (from libc/libm)
        // This is used for lazy declaration - if the function is built-in,
        // we'll automatically declare it when it's used
        if self.is_builtin_c_function(func_name) {
            return true;
        }

        false
    }

    /// Check if a function is a built-in C function (from libc/libm)
    /// 
    /// This method checks if the function name matches any of the built-in
    /// C library functions that are available for use in Coffee programs.
    /// These functions are automatically declared when used, following the
    /// "declare on demand" principle similar to V language.
    /// 
    /// # Arguments
    /// 
    /// * `func_name` - The name of the function to check
    /// 
    /// # Returns
    /// 
    /// * `true` if the function is a built-in C library function
    /// * `false` otherwise
    pub fn is_builtin_c_function(&self, func_name: &str) -> bool {
        // Common libc functions
        const LIBC_FUNCTIONS: &[&str] = &[
            "printf", "fprintf", "sprintf", "snprintf",
            "puts", "putchar", "fputc", "fputs",
            "scanf", "fscanf", "sscanf",
            "malloc", "free", "calloc", "realloc",
            "memcpy", "memmove", "memcmp", "memset",
            "strlen", "strcmp", "strncmp", "strcpy", "strncpy", "strcat", "strncat",
            "exit", "_exit", "abort",
            "system", "getenv", "setenv",
            "rand", "srand", "time",
            "abs", "labs", "llabs", "atoi", "atol", "atoll",
            "fopen", "fclose", "fread", "fwrite", "fseek", "ftell", "rewind", "fflush",
            "fgets", "fputs", "fgetc", "fputc", "feof", "ferror", "clearerr",
            "execl", "execlp", "execle", "execv", "execvp", "execve",
            "wait", "waitpid",
        ];

        // Common libm functions
        const LIBM_FUNCTIONS: &[&str] = &[
            "sin", "cos", "tan",
            "asin", "acos", "atan", "atan2",
            "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
            "exp", "exp2", "expm1", "log", "log10", "log2", "log1p",
            "sqrt", "cbrt", "pow", "hypot",
            "fabs", "fmod", "remainder", "fmax", "fmin", "fdim",
            "floor", "ceil", "round", "trunc",
        ];

        LIBC_FUNCTIONS.contains(&func_name) || LIBM_FUNCTIONS.contains(&func_name)
    }

    /// Enhanced error with context
    /// 
    /// Creates a formatted error message with contextual information about the
    /// current compilation context. If there is a current function, the error
    /// message will include the function name for better debugging. The error
    /// follows the format "function_name: context: detail" when inside a function,
    /// or "context: detail" when not in a function context.
    /// 
    /// # Arguments
    /// 
    /// * `context` - A string describing the context where the error occurred
    /// * `detail` - An implementor of Display providing additional error details
    /// 
    /// # Returns
    /// 
    /// A formatted error message string with contextual information
    pub fn error(&self, context: &str, detail: impl std::fmt::Display) -> String {
        if let Some(func) = self.current_function {
            let func_name = func.get_name().to_str().unwrap_or("<unknown>");
            // 更清晰的错误格式：function_name: context: detail
            // 而不是扁平的链式结构
            format!("{}: {}: {}", func_name, context, detail)
        } else {
            format!("{}: {}", context, detail)
        }
    }

    /// Convert Coffee type to LLVM type
    /// 
    /// Converts a Coffee type string (e.g., "int", "string", "int(4)+") to its
    /// corresponding LLVM type representation. This method uses the TypeMapper
    /// for consistent type conversion across the code generator.
    /// 
    /// # Arguments
    /// 
    /// * `type_str` - A string representation of the Coffee type to convert
    /// 
    /// # Returns
    /// 
    /// * `Ok(BasicTypeEnum)` - The corresponding LLVM type
    /// * `Err(String)` - If the type conversion fails
    /// 
    /// Uses TypeMapper for consistent type conversion
    pub fn coffee_type_to_llvm(&self, type_str: &str) -> Result<BasicTypeEnum<'ctx>, String> {
        // Use TypeMapper for type conversion
        Ok(self.type_mapper.map_type(type_str))
    }

    /// Security: Insert NULL pointer check before dereferencing
    /// 
    /// Creates a basic block that checks if a pointer is NULL and panics if so.
    /// This security feature helps prevent null pointer dereference crashes by
    /// inserting runtime checks before pointer access. If the pointer is found
    /// to be NULL, the program prints an error message and exits.
    /// 
    /// # Arguments
    /// 
    /// * `builder` - The LLVM builder to use for code generation
    /// * `pointer` - The pointer value to check for NULL
    /// * `name` - A name to use in error messages and generated code labels
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the NULL check code was generated successfully
    /// * `Err(String)` - If there was an error generating the check code
    /// 
    /// Creates a basic block that checks if pointer is NULL and panics if so
    #[allow(dead_code)]
    fn check_pointer_not_null<'b>(
        &self,
        builder: &inkwell::builder::Builder<'ctx>,
        pointer: PointerValue<'ctx>,
        name: &str,
    ) -> Result<(), String> {
        // Convert pointer to int for comparison
        let ptr_as_int = builder.build_ptr_to_int(pointer, self.backend.context.i64_type(), format!("ptr_as_int_{}", name).as_str())
            .map_err(|e| format!("failed to convert pointer to int for '{}': {}", name, e))?;

        // Build NULL check (NULL is 0)
        let null_value = self.backend.context.i64_type().const_int(0, false);
        let is_null = builder.build_int_compare(
            inkwell::IntPredicate::EQ,
            ptr_as_int,
            null_value,
            format!("is_null_{}", name).as_str()
        ).map_err(|e| format!("failed to build NULL check for '{}': {}", name, e))?;

        // Create blocks for check and continuation
        let current_block = builder.get_insert_block().unwrap();
        let function = current_block.get_parent().unwrap();

        let panic_block = self.backend.context.append_basic_block(function, format!("panic_null_{}", name).as_str());
        let continue_block = self.backend.context.append_basic_block(function, format!("continue_{}", name).as_str());

        // Conditional branch
        builder.build_conditional_branch(is_null, panic_block, continue_block)
            .map_err(|e| format!("failed to build conditional branch for NULL check: {}", e))?;

        // Build panic block
        builder.position_at_end(panic_block);

        // Security: Don't leak variable name in error message
        let panic_msg = builder.build_global_string_ptr(
            "NULL pointer dereference",
            "panic_msg"
        ).map_err(|e| format!("failed to build panic message: {}", e))?;

        // Use puts to print the error message
        let i8_ptr_type = self.backend.context.ptr_type(AddressSpace::default());
        let i32_type = self.backend.context.i32_type();
        let puts_func = self.functions.get("puts").copied().unwrap_or_else(|| {
            let fn_type = i32_type.fn_type(&[i8_ptr_type.into()], false);
            self.backend.module.add_function("puts", fn_type, None)
        });

        builder.build_call(puts_func, &[panic_msg.as_pointer_value().into()], "puts_call")
            .map_err(|e| format!("failed to build puts call: {}", e))?;

        // Call exit(1) to terminate the program
        // exit function is pre-declared in runtime functions
        let i32_type = self.backend.context.i32_type();
        let exit_func = self.functions.get("exit").copied().unwrap();
        let exit_code = i32_type.const_int(1, false);
        builder.build_call(exit_func, &[exit_code.into()], "exit_call")
            .map_err(|e| format!("failed to build exit call: {}", e))?;

        builder.build_unreachable()
            .map_err(|e| format!("failed to build unreachable: {}", e))?;

        // Position builder at continue block
        builder.position_at_end(continue_block);

        Ok(())
    }

    /// Compile entire program
    /// 
    /// This method performs the complete compilation of a Coffee program into
    /// LLVM IR. It handles multiple passes of compilation:
    /// 
    /// 1. Function declaration pass - declares all functions in the program
    /// 2. Runtime function declaration - adds external functions like printf
    /// 3. Code generation pass - generates LLVM IR for all statements
    /// 4. Main function generation - creates the main() function if specified
    /// 
    /// The method also performs security checks to prevent DoS attacks through
    /// excessive function or statement counts, and integrates C library imports
    /// and .cfc symbol tables for FFI support.
    /// 
    /// # Arguments
    /// 
    /// * `program` - The parsed Coffee program to compile
    /// * `c_imports` - List of C functions imported by the program
    /// * `cfc_symbols` - HashMap of C function symbol tables from .cfc files
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the program was compiled successfully
    /// * `Err(String)` - If there was an error during compilation
    pub fn compile_program(&mut self, program: &Program, c_imports: &[String], cfc_symbols: std::collections::HashMap<String, c::CSymbolTable>) -> Result<(), String> {
        self.compile_program_with_imports(program, c_imports, cfc_symbols, &std::collections::HashMap::new(), &std::collections::HashMap::new())
    }

    /// Compile a program with imported modules
    ///
    /// This is the main entry point for compiling a Coffee program. It processes
    /// all statements in the program and generates LLVM IR code for them.
    ///
    /// # Arguments
    ///
    /// * `program` - The parsed Coffee program to compile
    /// * `c_imports` - List of C library imports
    /// * `cfc_symbols` - C function symbol tables from .cfc files
    /// * `imported_modules` - Map of module name to parsed module from imports
    /// * `imported_symbols` - Map of module name to list of imported symbols (empty = all)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the program was compiled successfully
    /// * `Err(String)` - If there was an error during compilation
    pub fn compile_program_with_imports(&mut self,
        program: &Program,
        c_imports: &[String],
        cfc_symbols: std::collections::HashMap<String, c::CSymbolTable>,
        imported_modules: &std::collections::HashMap<String, crate::compiler::import_resolver::ParsedModule>,
        imported_symbols: &std::collections::HashMap<String, Vec<String>>,
    ) -> Result<(), String> {
        // MEDIUM-5 FIX: Limit number of functions to prevent DoS via code bloat
        const MAX_FUNCTIONS: usize = 1000;
        let function_count = program.statements.iter()
            .filter(|s| matches!(s, Statement::Function(_)))
            .count();

        if function_count > MAX_FUNCTIONS {
            return Err(self.error("compile_program",
                format!("Program contains too many functions\n  = note: found {} functions, maximum: {}\n  = help: reduce number of functions or split into multiple modules",
                    function_count, MAX_FUNCTIONS)));
        }

        // MEDIUM-6 FIX: Limit total statements to prevent DoS
        const MAX_STATEMENTS: usize = 10000;
        if program.statements.len() > MAX_STATEMENTS {
            return Err(self.error("compile_program",
                format!("Program contains too many statements\n  = note: found {} statements, maximum: {}\n  = help: reduce program size or split into multiple modules",
                    program.statements.len(), MAX_STATEMENTS)));
        }

        // Store C library imports for symbol resolution
        self.c_imports = c_imports.to_vec();
        // Store C function symbol tables from .cfc files
        self.cfc_symbols = cfc_symbols;

        // Declare all imported C functions from c_imports
        let c_imports_to_declare = self.c_imports.clone();
        for c_import in &c_imports_to_declare {
            if c_import.contains(':') {
                // Format is "library:symbol"
                let parts: Vec<&str> = c_import.split(':').collect();
                if parts.len() == 2 {
                    let func_name = parts[1];
                    // Declare the external function
                    if let Err(e) = self.declare_external_function(func_name) {
                        eprintln!("Warning: Failed to declare external function '{}': {}", func_name, e);
                    }
                }
            } else {
                // Direct symbol name
                // Declare the external function
                if let Err(e) = self.declare_external_function(c_import) {
                    eprintln!("Warning: Failed to declare external function '{}': {}", c_import, e);
                }
            }
        }

        // First pass: compile class definitions to populate struct_types cache
        for stmt in &program.statements {
            if let Statement::Class(class) = stmt {
                self.compile_class(class)?;
            }
        }

        // First pass: declare functions from imported modules
        for (module_name, parsed_module) in imported_modules {
            // Get the list of symbols to import from this module
            // Empty vector means import all symbols
            let symbols_to_import = imported_symbols.get(module_name);

            for stmt in &parsed_module.statements {
                if let Statement::Function(func) = stmt {
                    // Check if we should import this function
                    let should_import = if let Some(symbols) = symbols_to_import {
                        // If symbols list is specified, only import if function is in the list
                        if symbols.is_empty() {
                            true // Import all
                        } else {
                            symbols.contains(&func.name)
                        }
                    } else {
                        // No explicit import list for this module - skip
                        false
                    };

                    if should_import {
                        // Declare function from imported module
                        self.declare_function(func)?;
                    }
                }
            }
        }

        // First pass: declare all functions from main program
        for (_idx, stmt) in program.statements.iter().enumerate() {
            if let Statement::Function(func) = stmt {
                self.declare_function(func)?;
            }
        }

        // Generate Error class definition if needed (for raise statements and error handlers)
        if self.needs_error_class {
            self.generate_error_class()?;
        }

        // Add external function declarations (printf, etc.)
        self.declare_runtime_functions();

        // Second pass: define functions and compile code
        coffee_debug!("DEBUG: compile_program: Second pass: compiling {} statements", program.statements.len());
        for (idx, stmt) in program.statements.iter().enumerate() {
            coffee_debug!("DEBUG: compile_program: compiling statement {}: {:?}", idx, stmt);
            // CRITICAL: Reset current_function before compiling top-level statements
            // This ensures that top-level statements (like main() calls) are not compiled
            // into the wrong function's basic block
            self.current_function = None;
            self.compile_statement(stmt)?;
        }

        // Third pass: generate the actual main() function if entry point is specified
        if self.main_entry.is_some() {
            self.generate_main_function()?;
        }

        // Fourth pass: lazy declaration of used C functions
        // This follows the "declare on demand" principle - only declare functions that are actually used
        self.declare_used_c_functions()?;

        Ok(())
    }

    /// Declare runtime functions (C library and Coffee stdlib)
    /// 
    /// This internal method declares all necessary runtime functions that are
    /// used by Coffee programs, including C library functions like printf and
    /// Coffee-specific runtime functions. These functions are declared in the
    /// LLVM module so they can be called from the generated code.
    /// 
    /// The method uses the functions module to declare standard runtime functions
    /// that are commonly used in Coffee programs.
    fn declare_runtime_functions(&mut self) {
        use super::functions;
        functions::declare_runtime_functions(
            self.backend.context,
            &self.backend.module,
            &mut self.functions,
        );
    }

    /// Declare used C functions (lazy declaration)
    /// 
    /// This method implements the "declare on demand" principle - it only declares
    /// C functions that are actually used in the code. This follows the practical
    /// design approach of V language, avoiding unnecessary function declarations.
    /// 
    /// The method iterates through the `used_c_functions` set and declares each
    /// function using the appropriate signature from the built-in C function table.
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If all functions were declared successfully
    /// * `Err(String)` - If there was an error during function declaration
    fn declare_used_c_functions(&mut self) -> Result<(), String> {
        use super::functions;

        // Declare each used C function
        for func_name in self.used_c_functions.iter() {
            // Only declare if not already declared
            if !self.functions.contains_key(func_name) {
                // Use the external function declaration mechanism
                // This will use the built-in function signatures
                functions::declare_builtin_c_function(
                    func_name,
                    self.backend.context,
                    &self.backend.module,
                    &mut self.functions,
                )?;
            }
        }

        Ok(())
    }

    /// Generate Error class definition if needed
    ///
    /// The Error class has three fields: code (int), note (string), e (any)
    /// This function is only called when the program uses raise statements or error handlers.
    fn generate_error_class(&mut self) -> Result<(), String> {
        crate::backend::error::generate_error_class(self.backend.context, &mut self.type_mapper)
    }

    /// Declare a function (without body)
    /// 
    /// This internal method declares a Coffee function in the LLVM module without
    /// generating its body. This is typically done in the first pass of compilation
    /// to establish function signatures before the functions are called from other
    /// parts of the code.
    /// 
    /// # Arguments
    /// 
    /// * `func` - The Coffee Function to declare
    /// 
    /// # Returns
    /// 
    /// * `Ok(FunctionValue)` - The declared LLVM function value
    /// * `Err(String)` - If there was an error during function declaration
    fn declare_function(&mut self, func: &Function) -> Result<FunctionValue<'ctx>, String> {
        use super::functions;
        functions::declare_function(
            func,
            &self.backend.module,
            self.backend.context,
            &self.type_mapper,
            &mut self.functions,
        )
    }

    /// Declare an external function (from imported module)
    /// 
    /// This method declares an external function that was imported from another
    /// module or C library. For imported functions, it uses a generic signature
    /// that will be properly linked during the linking phase. The function declaration
    /// allows the code generator to reference these functions before they are defined.
    /// 
    /// # Arguments
    /// 
    /// * `qualified_name` - The fully qualified name of the external function
    /// 
    /// # Returns
    /// 
    /// * `Ok(FunctionValue)` - The declared LLVM function value
    /// * `Err(String)` - If there was an error during function declaration
    /// 
    /// For imported functions, we use a generic signature that will be linked later
    pub fn declare_external_function(&mut self, qualified_name: &str) -> Result<FunctionValue<'ctx>, String> {
        use super::functions;
        functions::declare_external_function(
            qualified_name,
            self.backend.context,
            &self.backend.module,
            &mut self.functions,
            &self.cfc_symbols,
        )
    }

    /// Compile if expression
    pub fn compile_if(&mut self, if_expr: &crate::parser::IfExpr) -> Result<(), String> {
        let function = self.current_function
            .ok_or_else(|| self.error("compile_if", "if expression outside function context"))?;

        // Compile condition
        let cond_val = self.compile_expr(&if_expr.condition)?;
        let cond_bool = self.value_to_bool(cond_val)?;

        // Create blocks
        let then_block = self.backend.context.append_basic_block(function, "then");
        let merge_block = self.backend.context.append_basic_block(function, "ifcont");

        // Determine next block (elif, else, or merge)
        let has_elifs = !if_expr.elifs.is_empty();
        let has_else = if_expr.else_body.is_some();

        let else_block = if has_elifs || has_else {
            self.backend.context.append_basic_block(function, "else")
        } else {
            merge_block
        };

        self.backend.builder.build_conditional_branch(cond_bool, then_block, else_block)
            .map_err(|e| e.to_string())?;

        // Compile then branch (body is Vec<Statement>)
        self.backend.builder.position_at_end(then_block);
        // Enter scope for then branch
        self.memory_ctx.enter_scope();
        for stmt in &if_expr.body {
            self.compile_statement(stmt)?;
        }
        // Exit scope for then branch
        self.memory_ctx.exit_scope();
        // Check terminator of current block (builder may have moved during compilation)
        if let Some(current_block) = self.backend.builder.get_insert_block() {
            if current_block.get_terminator().is_none() {
                self.backend.builder.build_unconditional_branch(merge_block)
                    .map_err(|e| e.to_string())?;
            }
        }

        // Compile elif branches
        if has_elifs {
            let mut current_else = else_block;
            for (i, elif) in if_expr.elifs.iter().enumerate() {
                self.backend.builder.position_at_end(current_else);

                // Check if current block already has a terminator
                if current_else.get_terminator().is_some() {
                    // Block already terminated, skip this elif
                    break;
                }

                let elif_cond = self.compile_expr(&elif.condition)?;
                let elif_bool = self.value_to_bool(elif_cond)?;

                let elif_then = self.backend.context.append_basic_block(function, &format!("elif_then_{}", i));
                let next_block = if i + 1 < if_expr.elifs.len() || has_else {
                    self.backend.context.append_basic_block(function, &format!("elif_else_{}", i))
                } else {
                    merge_block
                };

                self.backend.builder.build_conditional_branch(elif_bool, elif_then, next_block)
                    .map_err(|e| e.to_string())?;

                // Compile elif body
                self.backend.builder.position_at_end(elif_then);
                // Enter scope for elif branch
                self.memory_ctx.enter_scope();
                for stmt in &elif.body {
                    self.compile_statement(stmt)?;
                }
                // Exit scope for elif branch
                self.memory_ctx.exit_scope();
                // Check terminator of current block (builder may have moved)
                if let Some(current_block) = self.backend.builder.get_insert_block() {
                    if current_block.get_terminator().is_none() {
                        self.backend.builder.build_unconditional_branch(merge_block)
                            .map_err(|e| e.to_string())?;
                    }
                }

                current_else = next_block;
            }

            // Compile else branch if exists
            if has_else {
                self.backend.builder.position_at_end(current_else);
                // Only compile if block doesn't have terminator
                if current_else.get_terminator().is_none() {
                    // Enter scope for else branch
                    self.memory_ctx.enter_scope();
                    if let Some(ref else_body) = if_expr.else_body {
                        for stmt in else_body {
                            self.compile_statement(stmt)?;
                        }
                    }
                    // Exit scope for else branch
                    self.memory_ctx.exit_scope();
                    // Check terminator of current block (builder may have moved)
                    if let Some(current_block) = self.backend.builder.get_insert_block() {
                        if current_block.get_terminator().is_none() {
                            self.backend.builder.build_unconditional_branch(merge_block)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
        } else if has_else {
            // No elifs, just else
            self.backend.builder.position_at_end(else_block);
            // Enter scope for else branch
            self.memory_ctx.enter_scope();
            if let Some(ref else_body) = if_expr.else_body {
                for stmt in else_body {
                    self.compile_statement(stmt)?;
                }
            }
            // Exit scope for else branch
            self.memory_ctx.exit_scope();
            // Check terminator of current block (builder may have moved)
            if let Some(current_block) = self.backend.builder.get_insert_block() {
                if current_block.get_terminator().is_none() {
                    self.backend.builder.build_unconditional_branch(merge_block)
                        .map_err(|e| e.to_string())?;
                }
            }
        }

        // Continue at merge block
        self.backend.builder.position_at_end(merge_block);
        Ok(())
    }

    /// Convert a value to a boolean condition
    /// 
    /// This internal method converts a given LLVM value to a boolean condition
    /// suitable for use in control flow operations like if statements and loops.
    /// It handles the conversion of different types to a boolean representation
    /// according to Coffee's type system rules.
    /// 
    /// # Arguments
    /// 
    /// * `value` - The LLVM value to convert to a boolean
    /// 
    /// # Returns
    /// 
    /// * `Ok(IntValue)` - The boolean representation as an LLVM i1 integer value
    /// * `Err(String)` - If there was an error during the conversion
    fn value_to_bool(&self, value: BasicValueEnum<'ctx>) -> Result<inkwell::values::IntValue<'ctx>, String> {
        use super::control_flow;
        control_flow::value_to_bool(value, &self.backend.builder)
    }

    /// Compile while loop
    /// 
    /// This method generates LLVM IR for a while loop, including proper control
    /// flow structure, loop condition evaluation, and loop body execution. The
    /// method includes several safety features:
    /// 
    /// - Loop nesting depth tracking to prevent stack overflow
    /// - Loop iteration counter to prevent infinite loops
    /// - Proper break/continue support through loop context management
    /// - Security checks to prevent resource exhaustion
    /// 
    /// The generated code follows the standard while loop pattern with separate
    /// blocks for condition checking, loop body execution, and post-loop continuation.
    /// 
    /// # Arguments
    /// 
    /// * `while_loop` - The parsed WhileLoop to compile
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the while loop was compiled successfully
    /// * `Err(String)` - If there was an error during compilation
    pub fn compile_while(&mut self, while_loop: &crate::parser::WhileLoop) -> Result<(), String> {
        // MEDIUM-3 FIX: Check loop nesting depth to prevent stack overflow
        const MAX_LOOP_NESTING: usize = 32;
        if self.loop_nesting_depth >= MAX_LOOP_NESTING {
            return Err(self.error("compile_while",
                format!("Loop nesting depth exceeds maximum\n  = note: current nesting depth: {}, maximum: {}\n  = help: reduce loop nesting to prevent stack overflow",
                    self.loop_nesting_depth, MAX_LOOP_NESTING)));
        }

        let function = self.current_function
            .ok_or_else(|| self.error("compile_while", "while loop statement outside function context"))?;

        // CRITICAL-7 FIX: Add loop iteration counter to prevent infinite loops
        const MAX_LOOP_ITERATIONS: u64 = 1_000_000; // 1 million iterations max

        let loop_cond_block = self.backend.context.append_basic_block(function, "loopcond");
        let loop_body_block = self.backend.context.append_basic_block(function, "loopbody");
        let after_block = self.backend.context.append_basic_block(function, "afterloop");
        let overflow_block = self.backend.context.append_basic_block(function, "loop_overflow");

        // Allocate loop counter in entry block (must dominate all uses)
        let i64_type = self.backend.context.i64_type();
        let counter_ptr = self.create_entry_alloca_preserving_terminator(i64_type, "loop_counter")
            .map_err(|e| self.error("compile_while",
                format!("failed to allocate loop counter: {}", e)))?;

        // Initialize counter to 0 before loop starts
        self.backend.builder.build_store(counter_ptr, i64_type.const_int(0, false))
            .map_err(|e| self.error("compile_while",
                format!("failed to initialize loop counter: {}", e)))?;

        // Push loop context for break/continue
        self.loop_stack.push(LoopContext {
            break_block: after_block,
            continue_block: loop_cond_block,
        });

        // MEDIUM-3 FIX: Increment loop nesting depth
        self.loop_nesting_depth += 1;
        self.memory_ctx.loop_nesting_depth = self.loop_nesting_depth;

        // Jump to loop condition
        self.backend.builder.build_unconditional_branch(loop_cond_block)
            .map_err(|e| self.error("compile_while", format!("failed to build branch to loop: {}", e)))?;

        // Loop condition block: check counter overflow AND user condition
        self.backend.builder.position_at_end(loop_cond_block);

        // Load and increment counter FIRST (before checking condition)
        let counter = self.backend.builder.build_load(i64_type, counter_ptr, "counter")
            .map_err(|e| self.error("compile_while",
                format!("failed to load loop counter: {}", e)))?
            .into_int_value();

        let new_counter = self.backend.builder.build_int_add(counter, i64_type.const_int(1, false), "new_counter")
            .map_err(|e| self.error("compile_while",
                format!("failed to increment loop counter: {}", e)))?;

        self.backend.builder.build_store(counter_ptr, new_counter)
            .map_err(|e| self.error("compile_while",
                format!("failed to store loop counter: {}", e)))?;

        // Check if counter exceeds maximum
        let is_overflow = self.backend.builder.build_int_compare(
            IntPredicate::UGT,
            new_counter,
            i64_type.const_int(MAX_LOOP_ITERATIONS, false),
            "loop_overflow"
        ).map_err(|e| self.error("compile_while",
            format!("failed to build overflow check: {}", e)))?;

        // Branch: if overflow -> panic, else -> check user condition
        let cond_check_block = self.backend.context.append_basic_block(function, "condcheck");
        self.backend.builder.build_conditional_branch(is_overflow, overflow_block, cond_check_block)
            .map_err(|e| self.error("compile_while",
                format!("failed to build overflow branch: {}", e)))?;

        // Overflow block: panic
        self.backend.builder.position_at_end(overflow_block);
        let panic_msg = self.backend.builder.build_global_string_ptr(
            "Loop iteration limit exceeded - possible infinite loop",
            "loop_panic_msg"
        ).map_err(|e| self.error("compile_while",
            format!("failed to build panic message: {}", e)))?;

        let puts_func = self.functions.get("puts").copied().unwrap();
        self.backend.builder.build_call(puts_func, &[panic_msg.as_pointer_value().into()], "puts_call")
            .map_err(|e| self.error("compile_while",
                format!("failed to build puts call: {}", e)))?;

        let exit_func = self.functions.get("exit").copied().unwrap();
        let exit_code = self.backend.context.i32_type().const_int(1, false);
        self.backend.builder.build_call(exit_func, &[exit_code.into()], "exit_call")
            .map_err(|e| self.error("compile_while",
                format!("failed to build exit call: {}", e)))?;

        self.backend.builder.build_unreachable()
            .map_err(|e| self.error("compile_while",
                format!("failed to build unreachable: {}", e)))?;

        // User condition check block
        self.backend.builder.position_at_end(cond_check_block);
        let cond_val = self.compile_expr(&while_loop.condition)
            .map_err(|e| self.error("compile_while", format!("failed to compile loop condition: {}", e)))?;
        let cond_bool = self.value_to_bool(cond_val)
            .map_err(|e| self.error("compile_while", format!("failed to convert condition to boolean: {}", e)))?;
        self.backend.builder.build_conditional_branch(cond_bool, loop_body_block, after_block)
            .map_err(|e| self.error("compile_while", format!("failed to build conditional branch: {}", e)))?;

        // Loop body (now Vec<Statement> instead of String)
        self.backend.builder.position_at_end(loop_body_block);
        
        // Enter loop body scope for each iteration
        self.memory_ctx.enter_scope();
        
        for stmt in &while_loop.body {
            self.compile_statement(stmt)?;
        }
        
        // Exit loop body scope after each iteration
        self.memory_ctx.exit_scope();
        
        // Check current block for terminator (not loop_body_block!)
        // After compiling statements with arithmetic, builder may be in a different block
        if let Some(current_block) = self.backend.builder.get_insert_block() {
            if current_block.get_terminator().is_none() {
                self.backend.builder.build_unconditional_branch(loop_cond_block)
                    .map_err(|e| self.error("compile_while", format!("failed to build loop back-edge: {}", e)))?;
            }
        }

        // Pop loop context
        self.loop_stack.pop();

        // MEDIUM-3 FIX: Decrement loop nesting depth
        self.loop_nesting_depth -= 1;
        self.memory_ctx.loop_nesting_depth = self.loop_nesting_depth;

        self.backend.builder.position_at_end(after_block);
        Ok(())
    }

    /// Compile for loop
    /// 
    /// This method generates LLVM IR for a for loop, supporting both range-based
    /// iteration (e.g., `for i in 0..10`) and collection-based iteration (e.g.,
    /// `for item in array`). The method handles proper loop variable initialization,
    /// condition checking, and incrementation, while including safety features:
    /// 
    /// - Loop nesting depth tracking to prevent stack overflow
    /// - Proper break/continue support through loop context management
    /// - Type checking to ensure proper integer bounds
    /// - Collection length validation to ensure safe iteration
    /// 
    /// The generated code follows the standard for loop pattern with separate
    /// blocks for condition checking, loop body execution, incrementation,
    /// and post-loop continuation.
    /// 
    /// # Arguments
    /// 
    /// * `for_loop` - The parsed ForLoop to compile
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the for loop was compiled successfully
    /// * `Err(String)` - If there was an error during compilation
    pub fn compile_for(&mut self, for_loop: &crate::parser::ForLoop) -> Result<(), String> {
        // MEDIUM-3 FIX: Check loop nesting depth to prevent stack overflow
        const MAX_LOOP_NESTING: usize = 32;
        if self.loop_nesting_depth >= MAX_LOOP_NESTING {
            return Err(self.error("compile_for",
                format!("Loop nesting depth exceeds maximum\n  = note: current nesting depth: {}, maximum: {}\n  = help: reduce loop nesting to prevent stack overflow",
                    self.loop_nesting_depth, MAX_LOOP_NESTING)));
        }

        let function = self.current_function
            .ok_or_else(|| self.error("compile_for", "for loop statement outside function context"))?;

        // Allocate loop variable
        let i64_type = self.backend.context.i64_type();
        let var_alloca = self.backend.builder.build_alloca(i64_type, &for_loop.variable)
            .map_err(|e| self.error("compile_for",
                format!("failed to allocate loop variable '{}': {}", for_loop.variable, e)))?;

        // Initialize based on iterator type
        let (start_val, end_val) = match &for_loop.iterator {
            crate::parser::ForIterator::Range { start, end } => {
                let start_v = self.compile_expr(start)
                    .map_err(|e| self.error("compile_for",
                        format!("failed to compile range start: {}", e)))?;
                let end_v = self.compile_expr(end)
                    .map_err(|e| self.error("compile_for",
                        format!("failed to compile range end: {}", e)))?;

                let start_int = match start_v {
                    BasicValueEnum::IntValue(i) => i,
                    _ => return Err(self.error("compile_for",
                        format!("range start must be an integer, got non-integer value\n  = note: for loop ranges require integer bounds"))),
                };
                let end_int = match end_v {
                    BasicValueEnum::IntValue(i) => i,
                    _ => return Err(self.error("compile_for",
                        format!("range end must be an integer, got non-integer value\n  = note: for loop ranges require integer bounds"))),
                };
                (start_int, end_int)
            }
            crate::parser::ForIterator::Collection(coll) => {
                // For collections, we iterate from 0 to len
                let start_int = i64_type.const_int(0, false);

                // Try to get array length from tracking
                let end_int = if let Some(&len_ptr) = self.array_lengths.get(coll) {
                    // Load the tracked length
                    self.backend.builder.build_load(i64_type, len_ptr, "arr_len")
                        .map_err(|e| self.error("compile_for",
                            format!("failed to load length of collection '{}': {}", coll, e)))?
                        .into_int_value()
                } else if let Some(&(_var_ptr, _)) = self.variables.get(coll) {
                    // HIGH-10 FIX: Mark collection variable as used
                    self.used_variables.insert(coll.to_string());
                    // Variable exists but no length tracked
                    return Err(self.error("compile_for",
                        format!("cannot iterate over collection '{}' - length information not available\n  = note: collection length must be tracked when the collection is created\n  = help: use explicit range instead: for i in 0..length", coll)));
                } else {
                    // Unknown collection
                    return Err(self.error("compile_for",
                        format!("cannot find collection '{}' in this scope\n  = note: for-in loops require an existing collection or range", coll)));
                };
                (start_int, end_int)
            }
        };

        self.backend.builder.build_store(var_alloca, start_val)
            .map_err(|e| self.error("compile_for",
                format!("failed to initialize loop variable '{}': {}", for_loop.variable, e)))?;
        self.variables.insert(for_loop.variable.clone(), (var_alloca, i64_type.into()));

        let loop_block = self.backend.context.append_basic_block(function, "forloop");
        let body_block = self.backend.context.append_basic_block(function, "forbody");
        let incr_block = self.backend.context.append_basic_block(function, "forincr");
        let after_block = self.backend.context.append_basic_block(function, "afterfor");

        // Push loop context (continue goes to increment block)
        self.loop_stack.push(LoopContext {
            break_block: after_block,
            continue_block: incr_block,
        });

        // MEDIUM-3 FIX: Increment loop nesting depth
        self.loop_nesting_depth += 1;

        self.backend.builder.build_unconditional_branch(loop_block)
            .map_err(|e| self.error("compile_for", format!("failed to build branch to loop: {}", e)))?;

        // Loop condition
        self.backend.builder.position_at_end(loop_block);
        let current = self.backend.builder.build_load(i64_type, var_alloca, "current")
            .map_err(|e| self.error("compile_for",
                format!("failed to load loop variable '{}': {}", for_loop.variable, e)))?;
        let cond = self.backend.builder.build_int_compare(
            IntPredicate::SLT,
            current.into_int_value(),
            end_val,
            "forcond"
        ).map_err(|e| self.error("compile_for", format!("failed to build loop condition: {}", e)))?;
        self.backend.builder.build_conditional_branch(cond, body_block, after_block)
            .map_err(|e| self.error("compile_for", format!("failed to build conditional branch: {}", e)))?;

        // Body (body is now Vec<Statement>)
        self.backend.builder.position_at_end(body_block);
        for stmt in &for_loop.body {
            // Check if this is a continue or break statement
            match stmt {
                crate::parser::Statement::Continue(_) => {
                    self.compile_statement(stmt)?;
                    // After continue, don't compile any more statements in this iteration
                    break;
                }
                crate::parser::Statement::Break(_) => {
                    self.compile_statement(stmt)?;
                    // After break, don't compile any more statements in this iteration
                    break;
                }
                _ => {
                    self.compile_statement(stmt)?;
                }
            }

            // Check if the current block now has a terminator (e.g., from continue/break inside if statement)
            if let Some(current_block) = self.backend.builder.get_insert_block() {
                if current_block.get_terminator().is_some() {
                    // Stop compiling more statements after a terminator
                    break;
                }
            }
        }

        // Check current block for terminator (not body_block!)
        // After compiling statements with arithmetic, builder may be in a different block
        if let Some(current_block) = self.backend.builder.get_insert_block() {
            if current_block.get_terminator().is_none() {
                self.backend.builder.build_unconditional_branch(incr_block)
                    .map_err(|e| self.error("compile_for", format!("failed to build branch to increment: {}", e)))?;
            }
        }

        // Increment block
        self.backend.builder.position_at_end(incr_block);
        let current = self.backend.builder.build_load(i64_type, var_alloca, "current")
            .map_err(|e| self.error("compile_for",
                format!("failed to load loop variable '{}' for increment: {}", for_loop.variable, e)))?;
        let next = self.backend.builder.build_int_add(
            current.into_int_value(),
            i64_type.const_int(1, false),
            "next"
        ).map_err(|e| self.error("compile_for", format!("failed to build increment operation: {}", e)))?;
        self.backend.builder.build_store(var_alloca, next)
            .map_err(|e| self.error("compile_for",
                format!("failed to store incremented loop variable '{}': {}", for_loop.variable, e)))?;
        self.backend.builder.build_unconditional_branch(loop_block)
            .map_err(|e| self.error("compile_for", format!("failed to build loop back-edge: {}", e)))?;

        // Pop loop context
        self.loop_stack.pop();

        // MEDIUM-3 FIX: Decrement loop nesting depth
        self.loop_nesting_depth -= 1;
        self.memory_ctx.loop_nesting_depth = self.loop_nesting_depth;

        self.backend.builder.position_at_end(after_block);
        Ok(())
    }

    /// Compile match expression
    /// 
    /// This method generates LLVM IR for a match expression, which is Coffee's
    /// pattern matching construct. The method handles multiple match arms with
    /// different patterns, creating appropriate comparison logic for each arm
    /// and generating the necessary control flow structure.
    /// 
    /// Each match arm is compiled as a conditional branch that compares the
    /// matched value with the arm's pattern. The method handles both explicit
    /// patterns (like literals) and wildcard patterns (underscore). The control
    /// flow ensures that only the first matching arm is executed, with proper
    /// fall-through behavior to prevent multiple matches.
    /// 
    /// # Arguments
    /// 
    /// * `match_expr` - The parsed MatchExpr to compile
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the match expression was compiled successfully
    /// Compile match expression
    /// 
    /// This method generates LLVM IR for a match expression, which is Coffee's
    /// pattern matching construct. The method handles multiple match arms with
    /// different patterns, creating appropriate comparison logic for each arm
    /// and generating the necessary control flow structure.
    /// 
    /// Each match arm is compiled as a conditional branch that compares the
    /// matched value with the arm's pattern. The method handles both explicit
    /// patterns (like literals) and wildcard patterns (underscore). The control
    /// flow ensures that only the first matching arm is executed, with proper
    /// fall-through behavior to prevent multiple matches.
    /// 
    /// # Arguments
    /// 
    /// * `match_expr` - The parsed MatchExpr to compile
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the match expression was compiled successfully
    /// * `Err(String)` - If there was an error during compilation
/// Compile match expression
    /// 
    /// This method generates LLVM IR for a match expression, which is Coffee's
    /// pattern matching construct. The method handles multiple match arms with
    /// different patterns, creating appropriate comparison logic for each arm
    /// and generating the necessary control flow structure.
    /// 
    /// Each match arm is compiled as a conditional branch that compares the
    /// matched value with the arm's pattern. The method handles both explicit
    /// patterns (like literals) and wildcard patterns (underscore). The control
    /// flow ensures that only the first matching arm is executed, with proper
    /// fall-through behavior to prevent multiple matches.
    /// 
    /// # Arguments
    
    /// Branch to `dest` if the current insert block has no terminator.
    fn branch_to_if_unterminated(
        &self,
        dest: inkwell::basic_block::BasicBlock,
    ) -> Result<(), String> {
        let Some(block) = self.backend.builder.get_insert_block() else {
            return Ok(());
        };
        if block.get_terminator().is_none() {
            self.backend.builder
                .build_unconditional_branch(dest)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Parse a field pair from a string like "field_name: var_name"
    fn parse_field_pair(field_pair: &str) -> (String, String) {
        let field_pair = field_pair.trim();
        if let Some(colon_pos) = field_pair.find(':') {
            let field_name = field_pair[..colon_pos].trim().to_string();
            let var_name = field_pair[colon_pos + 1..].trim().to_string();
            (field_name, var_name)
        } else {
            (field_pair.to_string(), "_".to_string())
        }
    }
    
    /// * `match_expr` - The parsed MatchExpr to compile
    /// 
    /// # Returns
        /// 
                /// * `Ok(())` - If the match expression was compiled successfully
                /// * `Err(String)` - If there was an error during compilation
            pub fn compile_match(&mut self, match_expr: &crate::parser::MatchExpr) -> Result<(), String> {
                let function = self.current_function
                    .ok_or("match outside function")?;
        
                let match_value_str = match &match_expr.value {
                    crate::parser::expr::Expression::Variable(n) => n.clone(),
                    other => other.to_string(),
                };
                let match_val = self.compile_expr(&match_expr.value)?;
                let merge_block = self.backend.context.append_basic_block(function, "matchend");
        
                // For each arm, create a comparison and branch
                let mut current_block = self.backend.builder.get_insert_block().unwrap();
        
                for (i, arm) in match_expr.arms.iter().enumerate() {
                    let arm_block = self.backend.context.append_basic_block(function, &format!("matcharm_{}", i));
                    let next_block = self.backend.context.append_basic_block(function, &format!("matchnext_{}", i));
        
                    self.backend.builder.position_at_end(current_block);
                    let pattern = match &arm.pattern {
                    crate::parser::expr::Expression::Variable(n) => n.clone(),
                    crate::parser::expr::Expression::Literal(s) => s.clone(),
                    other => other.to_string(),
                };
        
                    // Check if pattern is a tuple pattern (contains variables)
                    let is_tuple_pattern = pattern.starts_with('(') && pattern.contains(',') && pattern.ends_with(')');
                    coffee_debug!("DEBUG: compile_match: pattern='{}', is_tuple_pattern={}", pattern, is_tuple_pattern);
            // Match guard: binding + optional guard expression.
            if let Some(guard_expr) = &arm.guard {
                let base = pattern.trim();
                let is_binding = !base.is_empty() && base != "_"
                    && base.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
                    && base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if is_binding {
                    let bind_ty = match_val.get_type();
                    let alloca = self.backend.builder.build_alloca(bind_ty, base)
                        .map_err(|e| format!("match guard: failed to alloca '{}': {}", base, e))?;
                    self.backend.builder.build_store(alloca, match_val)
                        .map_err(|e| format!("match guard: failed to store '{}': {}", base, e))?;
                    self.variables.insert(base.to_string(), (alloca, bind_ty));
                }
                let guard_val = self.compile_expr(guard_expr)
                    .map_err(|e| format!("match guard: {}", e))?;
                let guard_bool = super::control_flow::value_to_bool(guard_val, &self.backend.builder)?;
                self.backend.builder.build_conditional_branch(guard_bool, arm_block, next_block)
                    .map_err(|e| e.to_string())?;
            } else if pattern != "_" {
                if is_tuple_pattern {
                    // Tuple pattern: extract variables and bind them
                    let inner = &pattern[1..pattern.len()-1].trim();
                    
                    // Parse the tuple pattern correctly, handling nested tuples
                    let mut vars: Vec<&str> = Vec::new();
                    let mut depth = 0;
                    let mut start = 0;
                    for (i, c) in inner.chars().enumerate() {
                        match c {
                            '(' => depth += 1,
                            ')' => depth -= 1,
                            ',' if depth == 0 => {
                                vars.push(&inner[start..i].trim());
                                start = i + 1;
                            }
                            _ => {}
                        }
                    }
                    if start < inner.len() {
                        vars.push(&inner[start..].trim());
                    }
                    
                    coffee_debug!("DEBUG: compile_match: parsed pattern '{}' into vars: {:?}", pattern, vars);

                    // Check if match_val is a pointer (needs to load) or already a value
                    let tuple_val = if let BasicValueEnum::PointerValue(ptr_val) = match_val {
                        // Load the struct value from the pointer
                        coffee_debug!("DEBUG: compile_match: match_val is PointerValue, loading struct value");
                        
                        // Try to get the type from the variable
                        let var_type = if let Some((_, var_type)) = self.variables.get(&match_value_str) {
                            Some(var_type)
                        } else {
                            None
                        };
                        
                        coffee_debug!("DEBUG: compile_match: var_type = {:?}", var_type);
                        
                        // Load the struct value from the pointer using the correct struct type
                        if let Some(BasicTypeEnum::StructType(struct_type)) = var_type {
                            coffee_debug!("DEBUG: compile_match: using struct type {:?}", struct_type);
                            // Use struct_type.clone() to create an owned StructType
                            self.backend.builder.build_load(BasicTypeEnum::StructType(struct_type.clone()), ptr_val, "tuple_val")
                                .map_err(|e| format!("failed to load tuple value: {}", e))?
                        } else {
                            // Fallback: try to load as i64 (for simple tuples)
                            coffee_debug!("DEBUG: compile_match: var_type is not StructType, using i64_type");
                            let i64_type = self.backend.context.i64_type();
                            self.backend.builder.build_load(BasicTypeEnum::IntType(i64_type), ptr_val, "tuple_val")
                                .map_err(|e| format!("failed to load tuple value: {}", e))?
                        }
                    } else {
                        // Already a value
                        coffee_debug!("DEBUG: compile_match: match_val is already a value: {:?}", match_val);
                        match_val
                    };

                    coffee_debug!("DEBUG: compile_match: loaded value is {:?}", tuple_val);

                    if let BasicValueEnum::StructValue(tuple_val) = tuple_val {
                        coffee_debug!("DEBUG: compile_match: tuple_val is StructValue");
                        coffee_debug!("DEBUG: compile_match: vars = {:?}", vars);
                        for (j, var_name) in vars.iter().enumerate() {
                            coffee_debug!("DEBUG: compile_match: processing var '{}' (index {})", var_name, j);
                            if *var_name != "_" {
                                // Extract field from tuple using build_extract_value
                                let field_val = self.backend.builder.build_extract_value(
                                    tuple_val,
                                    j as u32,
                                    var_name
                                ).map_err(|e| format!("failed to extract tuple field: {}", e))?;
                                
                                coffee_debug!("DEBUG: compile_match: extracted field '{}' (index {}), type: {:?}", var_name, j, field_val.get_type());

                                // Check if the field is itself a tuple pattern
                                if var_name.starts_with('(') && var_name.contains(',') && var_name.ends_with(')') {
                                    // Recursively handle nested tuple pattern
                                    coffee_debug!("DEBUG: compile_match: field '{}' is a nested tuple pattern", var_name);
                                    
                                    // Parse the nested tuple pattern
                                    let inner = &var_name[1..var_name.len()-1].trim();
                                    let mut nested_vars: Vec<&str> = Vec::new();
                                    let mut depth = 0;
                                    let mut start = 0;
                                    for (i, c) in inner.chars().enumerate() {
                                        match c {
                                            '(' => depth += 1,
                                            ')' => depth -= 1,
                                            ',' if depth == 0 => {
                                                nested_vars.push(&inner[start..i].trim());
                                                start = i + 1;
                                            }
                                            _ => {}
                                        }
                                    }
                                    if start < inner.len() {
                                        nested_vars.push(&inner[start..].trim());
                                    }
                                    
                                    coffee_debug!("DEBUG: compile_match: nested vars: {:?}", nested_vars);
                                    
                                    // Extract nested tuple fields directly from the field value
                                    // Don't compile the pattern as an expression
                                    if let BasicValueEnum::StructValue(nested_tuple) = field_val {
                                        for (k, nested_var_name) in nested_vars.iter().enumerate() {
                                            if *nested_var_name != "_" {
                                                let nested_field_val = self.backend.builder.build_extract_value(
                                                    nested_tuple,
                                                    k as u32,
                                                    nested_var_name
                                                ).map_err(|e| format!("failed to extract nested tuple field: {}", e))?;

                                                coffee_debug!("DEBUG: compile_match: extracted nested field '{}' (index {}), type: {:?}", nested_var_name, k, nested_field_val.get_type());

                                                // Store the variable in the variables map
                                                let var_type = nested_field_val.get_type();
                                                let var_alloca = self.backend.builder.build_alloca(var_type, nested_var_name)
                                                    .map_err(|e| format!("failed to allocate variable '{}': {}", nested_var_name, e))?;
                                                self.backend.builder.build_store(var_alloca, nested_field_val)
                                                    .map_err(|e| format!("failed to store variable '{}': {}", nested_var_name, e))?;

                                                // Add to variables map
                                                self.variables.insert(nested_var_name.to_string(), (var_alloca, var_type));
                                                coffee_debug!("DEBUG: compile_match: inserted variable '{}' into variables map", nested_var_name);
                                            }
                                        }
                                    } else {
                                        coffee_debug!("DEBUG: compile_match: field '{}' is not a StructValue, it's {:?}", var_name, field_val);
                                    }
                                } else {
                                    // Store the variable in the variables map
                                    let var_type = field_val.get_type();
                                    let var_alloca = self.backend.builder.build_alloca(var_type, var_name)
                                        .map_err(|e| format!("failed to allocate variable '{}': {}", var_name, e))?;
                                    self.backend.builder.build_store(var_alloca, field_val)
                                        .map_err(|e| format!("failed to store variable '{}': {}", var_name, e))?;

                                    // Add to variables map
                                    self.variables.insert(var_name.to_string(), (var_alloca, var_type));
                                    coffee_debug!("DEBUG: compile_match: inserted variable '{}' into variables map", var_name);
                                }
                            }
                        }

                        // Always match tuple patterns (for simplicity)
                        self.backend.builder.build_unconditional_branch(arm_block)
                            .map_err(|e| e.to_string())?;
                    } else {
                        coffee_debug!("DEBUG: compile_match: loaded value is not StructValue, it's {:?}", tuple_val);
                        // Not a struct, compare as before
                        let pattern_val = self.compile_expr(&arm.pattern)?;
                        let cond = match (match_val, pattern_val) {
                            (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) => {
                                self.backend.builder.build_int_compare(IntPredicate::EQ, a, b, "matchcmp")
                                    .map_err(|e| e.to_string())?
                            }
                            (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) => {
                                self.arithmetic_ctx.build_float_compare(&self.backend.builder, FloatPredicate::OEQ, a, b, "matchcmpf")
                                    .map_err(|e| e.to_string())?
                            }
                            _ => self.backend.context.bool_type().const_int(1, false),
                        };
                        self.backend.builder.build_conditional_branch(cond, arm_block, next_block)
                            .map_err(|e| e.to_string())?;
                    }
                } else {
                    // Check if pattern is a struct pattern (contains { and })
                    let is_struct_pattern = pattern.contains('{') && pattern.contains('}');
                    coffee_debug!("DEBUG: compile_match: pattern='{}', is_struct_pattern={}", pattern, is_struct_pattern);

                    if is_struct_pattern {
                        // Struct pattern: extract fields and bind them
                        // Parse the struct pattern: ClassName { field1: var1, field2: var2, ... }
                        let struct_name_end = pattern.find('{').unwrap();
                        let struct_name = pattern[..struct_name_end].trim();
                        let fields_str = &pattern[struct_name_end + 1..pattern.len() - 1].trim();

                        coffee_debug!("DEBUG: compile_match: struct_name='{}', fields_str='{}'", struct_name, fields_str);

                        // Parse fields: field1: var1, field2: var2, ...
                        let mut fields: Vec<(String, String)> = Vec::new();
                        let mut current_field = String::new();
                        let mut depth = 0;
                        for c in fields_str.chars() {
                            match c {
                                '{' => depth += 1,
                                '}' => depth -= 1,
                                ',' if depth == 0 => {
                                    fields.push(Self::parse_field_pair(&current_field));
                                    current_field.clear();
                                }
                                _ => current_field.push(c),
                            }
                        }
                        if !current_field.trim().is_empty() {
                            fields.push(Self::parse_field_pair(&current_field));
                        }

                        coffee_debug!("DEBUG: compile_match: parsed fields: {:?}", fields);

                        // Load the struct value
                        let struct_val = if let BasicValueEnum::PointerValue(ptr_val) = match_val {
                            // The struct type may be stored on the variable directly
                            // (StructType) OR — for class instances stored as heap
                            // pointers — the variable's type is PointerType. In the
                            // latter case, fall back to the class name in the pattern
                            // (struct_name) to look up the named struct type.
                            let struct_type = match self.variables.get(&match_value_str) {
                                Some((_, BasicTypeEnum::StructType(st))) => Some(*st),
                                _ => self.type_mapper.struct_types.get(struct_name).copied(),
                            };

                            if let Some(struct_type) = struct_type {
                                self.backend.builder.build_load(BasicTypeEnum::StructType(struct_type.clone()), ptr_val, "struct_val")
                                    .map_err(|e| format!("failed to load struct value: {}", e))?
                            } else {
                                return Err(format!("variable '{}' is not a struct (no struct type found for '{}')", match_value_str, struct_name));
                            }
                        } else {
                            match_val
                        };

                        if let BasicValueEnum::StructValue(struct_val) = struct_val {
                            // Extract fields and bind them to variables
                            for (field_name, var_name) in fields.iter() {
                                if *var_name != "_" {
                                    // Get field index
                                    let field_index = self.type_mapper.get_field_index(struct_name, field_name)
                                        .ok_or_else(|| format!("field '{}' not found in struct '{}'", field_name, struct_name))?;

                                    // Extract field value
                                    let field_val = self.backend.builder.build_extract_value(
                                        struct_val,
                                        field_index as u32,
                                        var_name
                                    ).map_err(|e| format!("failed to extract field '{}': {}", field_name, e))?;

                                    coffee_debug!("DEBUG: compile_match: extracted field '{}' (index {}), type: {:?}", var_name, field_index, field_val.get_type());

                                    // Store the variable in the variables map
                                    let var_type = field_val.get_type();
                                    let var_alloca = self.backend.builder.build_alloca(var_type, var_name)
                                        .map_err(|e| format!("failed to allocate variable '{}': {}", var_name, e))?;
                                    self.backend.builder.build_store(var_alloca, field_val)
                                        .map_err(|e| format!("failed to store variable '{}': {}", var_name, e))?;

                                    // Add to variables map
                                    self.variables.insert(var_name.to_string(), (var_alloca, var_type));
                                    coffee_debug!("DEBUG: compile_match: inserted variable '{}' into variables map", var_name);
                                }
                            }
                        } else {
                            return Err(format!("match value is not a struct"));
                        }

                        // Always match struct patterns (for simplicity)
                        self.backend.builder.build_unconditional_branch(arm_block)
                            .map_err(|e| e.to_string())?;
                    } else {
                        // Non-tuple, non-struct pattern: compare as before
                        let pattern_val = self.compile_expr(&arm.pattern)?;
                        let cond = match (match_val, pattern_val) {
                            (BasicValueEnum::IntValue(a), BasicValueEnum::IntValue(b)) => {
                                self.backend.builder.build_int_compare(IntPredicate::EQ, a, b, "matchcmp")
                                    .map_err(|e| e.to_string())?
                            }
                            (BasicValueEnum::FloatValue(a), BasicValueEnum::FloatValue(b)) => {
                                self.arithmetic_ctx.build_float_compare(&self.backend.builder, FloatPredicate::OEQ, a, b, "matchcmpf")
                                    .map_err(|e| e.to_string())?
                            }
                            _ => self.backend.context.bool_type().const_int(1, false),
                        };
                        self.backend.builder.build_conditional_branch(cond, arm_block, next_block)
                            .map_err(|e| e.to_string())?;
                    }
                }
            } else {
                // Default case (wildcard _)
                self.backend.builder.build_unconditional_branch(arm_block)
                    .map_err(|e| e.to_string())?;
            }

            // Compile arm body as statements (not a newline-joined string)
            self.backend.builder.position_at_end(arm_block);
            for stmt in &arm.body {
                self.compile_statement(stmt)?;
            }
            // CRITICAL: check the builder's CURRENT block for a terminator, not
            // arm_block. Overflow-check sub-blocks (add_merge, mul_merge, …) are
            // the insert point after a nested arithmetic expression.
            self.branch_to_if_unterminated(merge_block)?;

            current_block = next_block;
        }

        // Fall-through next_block of the last arm (may have no predecessors).
        self.backend.builder.position_at_end(current_block);
        self.branch_to_if_unterminated(merge_block)?;

        self.backend.builder.position_at_end(merge_block);
        
        // Print LLVM IR for debugging
        coffee_debug!("DEBUG: compile_match: LLVM IR:\n{}", self.backend.module.print_to_string().to_string());
        
        Ok(())
    }
    pub fn compile_function(&mut self, func: &Function) -> Result<(), String> {
        coffee_debug!("DEBUG: compile_function: START compiling function '{}', parameters: {:?}", func.name, func.parameters.iter().map(|p| &p.name).collect::<Vec<_>>());
        let function = *self.functions.get(&func.name)
            .ok_or_else(|| self.error("compile_function",
                format!("function '{}' not declared - this is an internal compiler error", func.name)))?;

        // For external declarations, don't generate any function body
        if matches!(func.body, FunctionBody::External) {
            return Ok(());
        }

        self.current_function = Some(function);

        // 进入新的函数作用域
        self.memory_ctx.enter_scope();

        // 初始化生命周期跟踪
        self.memory_ctx.set_function(func.name.clone());

        // Set error handler for this function
        self.current_error_handler = func.error_handler.clone();
        self.current_function_params = func.parameters.iter().map(|p| p.name.clone()).collect();

        // Mark that Error class is needed if function has error handler
        if func.error_handler.is_some() {
            self.needs_error_class = true;
        }

        // Create entry block
        let entry = self.backend.context.append_basic_block(function, "entry");
        self.backend.builder.position_at_end(entry);

        // Clear local variables and lifetime information for each function
        // This ensures that each function has its own scope and can use the same variable names
        self.variables.clear();
        self.memory_ctx.clear_all();

        // CRITICAL-5 FIX: Reset stack size tracking for each function
        self.current_stack_size = 0;

        // Allocate parameters
        for (i, param) in func.parameters.iter().enumerate() {
            let param_value = function.get_nth_param(i as u32)
                .ok_or_else(|| self.error("compile_function",
                    format!("missing parameter {} '{}' in function '{}' - internal error",
                        i, param.name, func.name)))?;

            let param_type = self.coffee_type_to_llvm(&param.param_type)
                .map_err(|e| self.error("compile_function",
                    format!("failed to convert type '{}' for parameter '{}': {}",
                        param.param_type, param.name, e)))?;
            let alloca = self.backend.builder.build_alloca(param_type, &param.name)
                .map_err(|e| self.error("compile_function",
                    format!("failed to allocate parameter '{}': {}", param.name, e)))?;

            self.backend.builder.build_store(alloca, param_value)
                .map_err(|e| self.error("compile_function",
                    format!("failed to store parameter '{}': {}", param.name, e)))?;

            coffee_debug!("DEBUG: compile_function: inserting parameter '{}' into variables", param.name);
            self.variables.insert(param.name.clone(), (alloca, param_type));
            coffee_debug!("DEBUG: compile_function: variables after inserting parameter '{}': {:?}", param.name, self.variables.keys().collect::<Vec<_>>());
        }

        // Create a body block where all actual code will be compiled
        // This keeps entry block clean with only allocas and a single branch
        let body_block = self.backend.context.append_basic_block(function, "body");
        self.backend.builder.build_unconditional_branch(body_block)
            .map_err(|e| self.error("compile_function",
                format!("failed to build branch from entry to body: {}", e)))?;
        self.backend.builder.position_at_end(body_block);

        // Track that we're now in body block
        self.entry_successor = Some(body_block);

        // Track if function contains raise statement
        let mut has_raise = false;
        let mut raise_line = 0;

        // Compile function body
        match &func.body {
            FunctionBody::External => {
                // External declaration - should have returned early
                return Err(self.error("compile_function",
                    "external function declaration should not reach body compilation"));
            }
            FunctionBody::Expression(expr) => {
                // Single expression - evaluate and return
                if !matches!(expr, crate::parser::expr::Expression::Literal(s) if s.is_empty()) {
                    let value = self.compile_expr(expr)
                        .map_err(|e| self.error("compile_function",
                            format!("failed to compile function body expression: {}", e)))?;
                    functions::build_return(&self.backend.builder, &func.return_type, self.backend.context, Some(value))
                        .map_err(|e| self.error("compile_function",
                            format!("failed to build return instruction: {}", e)))?;
                }
            }
            FunctionBody::Block(statements) => {
                // Multiple statements - compile each one
                let mut unreachable_warning = None;
                let mut has_terminal = false;

                for (i, stmt) in statements.iter().enumerate() {
                    // 检查是否是return或raise语句
                    let is_terminal = matches!(stmt,
                        Statement::Return(_) | Statement::Raise(_));

                    if has_terminal && !is_terminal {
                        // 在return/raise之后还有语句
                        // 如果是raise语句，报错；如果是return语句，警告
                        if has_raise {
                            return Err(self.error("compile_function",
                                format!("unreachable code after raise statement at line {} - raise must be the last statement in the function body", raise_line)));
                        } else {
                            unreachable_warning = Some(format!(
                                "unreachable code after return statement at line {}",
                                i + 1
                            ));
                            // 跳过不可达代码，不编译
                            continue;
                        }
                    }

                    // 如果已经遇到raise语句，不再编译后面的任何语句
                    if has_raise {
                        continue;
                    }

                    self.compile_statement(stmt)
                        .map_err(|e| self.error("compile_function",
                            format!("failed to compile statement {} in function body: {}", i + 1, e)))?;

                    if is_terminal {
                        has_terminal = true;
                        if matches!(stmt, Statement::Raise(_)) {
                            has_raise = true;
                            raise_line = i + 1;
                        }
                    }
                }

                // 如果有不可达代码警告，打印它（使用黄色警告）
                if let Some(warning) = unreachable_warning {
                    eprintln!("\x1b[1;33m[Warning]\x1b[0m: {}", warning);
                    eprintln!("    \x1b[0;36m| note: code after return will never execute\x1b[0m");
                }
            }
        }

        // Add default return if needed (but not if function has raise statement)
        let has_terminator = self.backend.builder.get_insert_block()
            .map(|b| b.get_terminator().is_some())
            .unwrap_or(false);
        if !has_raise && !has_terminator {
            functions::build_return(&self.backend.builder, &func.return_type, self.backend.context, None)
                .map_err(|e| self.error("compile_function",
                    format!("failed to build default return: {}", e)))?;
        }

        // Ensure entry block has a terminator before finishing
        let entry_block = function.get_first_basic_block().unwrap();
        if entry_block.get_terminator().is_none() {
            return Err(self.error("compile_function",
                "entry block lacks terminator - function body doesn't return or branch properly"));
        }

        // HIGH-10 FIX: Check for unused variables before function ends
        // Collect all declared variables (parameters + locals)
        let _all_vars: std::collections::HashSet<String> = self.variables.keys().cloned().collect();

        // 运行生命周期检查，并按 severity 分流：
        //   Error   (M006/M007，真正的 UB)        → 编译失败
        //   Warning (M001/M002/M005/M010/M011/M012/M013) → 黄色警告，不阻断编译
        //   Silent  (M003/M004/M009，已退役的 must-rm 规则) → 丢弃
        //   自动 drop 接管后，"必须显式 rm" 的前提已不复存在。
        let lifetime_diagnostics = self.memory_ctx.run_lifetime_checks()
            .map_err(|e| self.error("compile_function",
                format!("lifetime check failed: {}", e)))?;

        let mut error_messages = Vec::new();
        for diag in lifetime_diagnostics {
            // 显式使用 var_name 字段以确保它被使用
            let _var = &diag.var_name;
            match diag.severity() {
                DiagSeverity::Error => {
                    error_messages.push(format!(
                        "\x1b[1;35m[{}]\x1b[0m: {}\n    \x1b[0;36m| help: {}\x1b[0m",
                        diag.code, diag.message, diag.hint
                    ));
                }
                DiagSeverity::Warning => {
                    eprintln!("\x1b[1;33m[Warning {}]\x1b[0m: {}\n    \x1b[0;36m| help: {}\x1b[0m",
                        diag.code, diag.message, diag.hint);
                }
                DiagSeverity::Silent => {
                    // 已退役的诊断，丢弃
                }
            }
        }

        // 只有真正的内存安全 UB（M006 use-after-free / M007 use-after-move）
        // 才拒绝编译——这些无法被自动 drop 修复。
        if !error_messages.is_empty() {
            return Err(self.error("compile_function",
                format!("\x1b[1;35mMemory safety check failed\x1b[0m\n\n{}\n  \x1b[0;36m= note: use-after-free / use-after-move are undefined behavior and cannot be auto-fixed by drop\x1b[0m",
                    error_messages.join("\n\n"))));
        }

        // Clear used variables for next function
        self.used_variables.clear();

        // 退出函数作用域
        self.memory_ctx.exit_scope();

        self.current_function = None;
        Ok(())
    }

    /// Compile main entry point statement
    /// 
    /// This method processes the main entry point statement from the Coffee source,
    /// storing the entry point information for later generation of the actual C
    /// main() function. The main entry point specifies which function should be
    /// called when the program starts execution.
    /// 
    /// The method validates that only one main entry point is specified per program
    /// and stores the entry function name along with its arguments for use during
    /// the main function generation phase.
    /// 
    /// # Arguments
    /// 
    /// * `main_entry` - The parsed MainEntry to compile
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the main entry point was processed successfully
    /// * `Err(String)` - If there was an error during processing (e.g., duplicate main)
    /// 
    /// This stores the entry point info and generates the actual main() function later
    pub fn compile_main_entry(&mut self, main_entry: &crate::parser::MainEntry) -> Result<(), String> {
        // Check if main entry already specified
        if self.main_entry.is_some() {
            return Err(self.error("compile_main_entry",
                "multiple main() entry points specified\n  = note: only one main(entry_function()) is allowed per program"));
        }

        // Store the main entry point info
        self.main_entry = Some((
            main_entry.entry_function.clone(),
            main_entry.args.clone(),
        ));

        Ok(())
    }

    /// Generate the actual main() function that calls the entry function
    /// 
    /// This internal method creates the actual C-compatible main() function that
    /// serves as the entry point for the compiled program. The generated main()
    /// function follows the standard C signature (int main(int argc, char** argv))
    /// and calls the Coffee function specified as the program's entry point.
    /// 
    /// The method handles command-line argument passing to the Coffee entry function
    /// and manages the conversion between C-style arguments and Coffee's expected
    /// parameter format. It also ensures that the main function returns an
    /// appropriate exit code to the operating system.
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the main function was generated successfully
    /// * `Err(String)` - If there was an error during generation
    fn generate_main_function(&mut self) -> Result<(), String> {
        let (entry_func_name, entry_args) = self.main_entry.take().ok_or_else(|| {
            self.error("generate_main",
                "no main entry point specified\n  = note: use main(function_name()) to specify the program entry point")
        })?;

        let context = self.backend.context;
        let i32_type = context.i32_type();
        let i8_ptr_type = context.ptr_type(AddressSpace::default());

        // Create C-compatible main function: int main(int argc, char** argv)
        let main_type = i32_type.fn_type(&[i32_type.into(), i8_ptr_type.into()], false);
        let main_func = self.backend.module.add_function("main", main_type, None);

        let entry_block = context.append_basic_block(main_func, "entry");
        self.backend.builder.position_at_end(entry_block);

        // Get argc and argv parameters
        let params = main_func.get_params();
        if params.len() < 2 {
            return Err(self.error("generate_main", "main function requires 2 parameters (argc, argv)"));
        }

        let argc = params[0].into_int_value();
        let argv = params[1].into_pointer_value();

        // Store argc and argv for use in arg1, arg2, ... expressions
        self.main_argc = Some(argc);
        self.main_argv = Some(argv);

        // Check if the entry function exists
        let entry_func = *self.functions.get(&entry_func_name).ok_or_else(|| {
            self.error("generate_main",
                format!("entry function '{}' not found\n  = note: ensure the function is defined before main() statement", entry_func_name))
        })?;

        // Compile the entry function arguments
        let mut compiled_args = Vec::new();
        for arg_str in &entry_args {
            // Check if this is a special command-line argument (arg1, arg2, arg3, etc.)
            if let Some(arg_num) = Self::parse_arg_number(arg_str) {
                // Get command-line argument: argv[arg_num]
                let arg_value = self.get_command_line_arg(argv, argc, arg_num)?;
                compiled_args.push(arg_value);
            } else if arg_str == "argc" {
                // Special case: argc parameter
                // Convert i32 to i64 (Coffee's default int type)
                let i64_type = context.i64_type();
                let argc_i64 = self.backend.builder.build_int_s_extend(argc, i64_type, "argc_i64")
                    .map_err(|e| self.error("generate_main", format!("failed to extend argc to i64: {}", e)))?;
                compiled_args.push(argc_i64.into());
            } else if arg_str == "argv" {
                // Special case: argv parameter
                // argv is already a pointer (char**), which matches Coffee's string type
                compiled_args.push(argv.into());
            } else {
                // Regular expression - compile as usual
                let arg_value = self.compile_source_as_expr(arg_str).map_err(|e| {
                    self.error("generate_main",
                        format!("failed to compile entry function argument '{}': {}", arg_str, e))
                })?;
                compiled_args.push(arg_value);
            }
        }

        // Call the entry function
        let args_ref: Vec<_> = compiled_args.iter().map(|v| (*v).into()).collect();

        let call_site = self.backend.builder
            .build_call(entry_func, &args_ref, "entry_call")
            .map_err(|e| self.error("generate_main", format!("failed to build call to entry function: {}", e)))?;

        // Get return value and convert to i32
        let exit_code = match call_site.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => {
                // Convert the return value to i32
                match val {
                    BasicValueEnum::IntValue(int_val) => {
                        // Truncate or extend to i32 as needed
                        let bit_width = int_val.get_type().get_bit_width();
                        if bit_width == 64 {
                            // Truncate i64 to i32
                            self.backend.builder.build_int_truncate(int_val, i32_type, "trunc")
                                .map_err(|e| self.error("generate_main", format!("failed to truncate i64 to i32: {}", e)))?
                                .into()
                        } else if bit_width == 32 {
                            // Already i32
                            int_val.into()
                        } else if bit_width < 32 {
                            // Sign extend to i32
                            self.backend.builder.build_int_s_extend(int_val, i32_type, "sext")
                                .map_err(|e| self.error("generate_main", format!("failed to extend to i32: {}", e)))?
                                .into()
                        } else {
                            // Truncate larger types to i32
                            self.backend.builder.build_int_truncate(int_val, i32_type, "trunc")
                                .map_err(|e| self.error("generate_main", format!("failed to truncate to i32: {}", e)))?
                                .into()
                        }
                    }
                    BasicValueEnum::FloatValue(_) => {
                        // For float, return 0
                        i32_type.const_int(0, false)
                    }
                    _ => {
                        // For other types, return 0
                        i32_type.const_int(0, false)
                    }
                }
            }
            inkwell::values::ValueKind::Instruction(_) => {
                // Void return - exit with 0
                i32_type.const_int(0, false)
            }
        };

        // Return the exit code
        self.backend.builder.build_return(Some(&exit_code))
            .map_err(|e| self.error("generate_main", format!("failed to build return: {}", e)))?;

        Ok(())
    }

    /// Get memory layout report
    pub fn get_memory_layout_report(&self) -> String {
        self.layout_collector.generate_report()
    }

    /// Check if there are any structs to report
    pub fn has_structs(&self) -> bool {
        !self.layout_collector.structs.is_empty()
    }

    /// Split function arguments handling nested parentheses
    /// For example: "add(10, 20), subtract(30, 40)" -> ["add(10, 20)", "subtract(30, 40)"]
    pub fn split_function_args(&self, args_str: &str) -> Result<Vec<String>, String> {
        let mut args = Vec::new();
        let mut current_arg = String::new();
        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for ch in args_str.chars() {
            if escape_next {
                current_arg.push(ch);
                escape_next = false;
                continue;
            }

            match ch {
                '\\' => {
                    escape_next = true;
                    current_arg.push(ch);
                }
                '"' if !in_string => {
                    in_string = true;
                    current_arg.push(ch);
                }
                '"' if in_string => {
                    in_string = false;
                    current_arg.push(ch);
                }
                '(' if !in_string => {
                    depth += 1;
                    current_arg.push(ch);
                }
                ')' if !in_string => {
                    depth -= 1;
                    current_arg.push(ch);
                }
                ',' if !in_string && depth == 0 => {
                    // Top-level comma - this separates arguments
                    let arg = current_arg.trim().to_string();
                    if !arg.is_empty() {
                        args.push(arg);
                    }
                    current_arg = String::new();
                }
                _ => {
                    current_arg.push(ch);
                }
            }
        }

        // Don't forget the last argument
        let arg = current_arg.trim().to_string();
        if !arg.is_empty() {
            args.push(arg);
        }

        if depth != 0 {
            return Err(format!("unbalanced parentheses in argument list: '{}'", args_str));
        }

        if in_string {
            return Err(format!("unterminated string in argument list: '{}'", args_str));
        }

        Ok(args)
    }

    /// Parse arg1, arg2, arg3, etc. and return the argument number
    /// Returns Some(arg_num) for valid arg names, None otherwise
    pub fn parse_arg_number(arg_str: &str) -> Option<u32> {
        let arg_str = arg_str.trim();

        // Check if it starts with "arg" followed by a number
        if arg_str.starts_with("arg") {
            let num_str = &arg_str[3..]; // Skip "arg"
            if let Ok(num) = num_str.parse::<u32>() {
                if num >= 1 && num <= 127 { // Reasonable limit
                    return Some(num);
                }
            }
        }

        None
    }

    /// Get command-line argument from argv by index
    /// argv[0] is the program name, argv[1] is arg1, etc.
    pub fn get_command_line_arg(
        &self,
        argv: inkwell::values::PointerValue<'ctx>,
        argc: inkwell::values::IntValue<'ctx>,
        arg_num: u32,
    ) -> Result<inkwell::values::BasicValueEnum<'ctx>, String> {
        let builder = &self.backend.builder;
        let context = self.backend.context;

        // Check bounds: arg_num must be < argc
        let i32_type = context.i32_type();
        let arg_num_value = i32_type.const_int(arg_num as u64, false);

        // Build condition: if (arg_num >= argc) return default value
        let arg_num_ge_argc = builder
            .build_int_compare(inkwell::IntPredicate::UGE, arg_num_value, argc, "arg.ge.argc")
            .map_err(|e| format!("failed to compare arg_num with argc: {}", e))?;

        let current_block = builder.get_insert_block().ok_or("no insert block")?;
        let function = current_block.get_parent().ok_or("no parent function")?;

        let then_block = context.append_basic_block(function, "arg.bounds_fail");
        let else_block = context.append_basic_block(function, "arg.bounds_ok");
        let merge_block = context.append_basic_block(function, "arg.merge");

        builder
            .build_conditional_branch(arg_num_ge_argc, then_block, else_block)
            .map_err(|e| format!("failed to build conditional branch: {}", e))?;

        // Then block: return default empty string
        builder.position_at_end(then_block);
        let default_str = builder
            .build_global_string_ptr("default_empty_str", "")
            .map_err(|e| format!("failed to build default string: {}", e))?;
        builder.build_unconditional_branch(merge_block).map_err(|e| format!("failed to build branch: {}", e))?;

        // Else block: get argv[arg_num]
        builder.position_at_end(else_block);

        // Calculate argv[arg_num] pointer
        let argv_elem_ptr = unsafe {
            builder.build_in_bounds_gep(
                context.ptr_type(AddressSpace::default()),
                argv,
                &[arg_num_value],
                "argv_elem_ptr",
            )
        }.map_err(|e| format!("failed to build GEP: {}", e))?;

        // Load the pointer to the actual string
        let arg_ptr = builder
            .build_load(context.ptr_type(AddressSpace::default()), argv_elem_ptr, "arg_ptr")
            .map_err(|e| format!("failed to load argv element: {}", e))?
            .into_pointer_value();

        builder.build_unconditional_branch(merge_block).map_err(|e| format!("failed to build branch: {}", e))?;

        // Merge block
        builder.position_at_end(merge_block);

        let phi_node = builder
            .build_phi(context.ptr_type(AddressSpace::default()), "arg_value")
            .map_err(|e| format!("failed to build phi node: {}", e))?;

        phi_node.add_incoming(&[(&default_str, then_block), (&arg_ptr, else_block)]);

        // Convert PhiValue to PointerValue and then to BasicValueEnum
        Ok(inkwell::values::BasicValueEnum::PointerValue(
            phi_node.as_basic_value().into_pointer_value()
        ))
    }
}

/// Find the position of an operator outside of parentheses
/// Returns the first position where the operator appears outside any parentheses
fn find_operator_outside_parens(expr: &str, op: &str) -> Option<usize> {
    let mut depth = 0;
    let expr_bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();

    let mut i = 0;
    while i < expr_bytes.len() {
        let ch = expr_bytes[i] as char;

        if ch == '(' {
            depth += 1;
            i += 1;
        } else if ch == ')' {
            depth -= 1;
            i += 1;
        } else if depth == 0 && i + op_bytes.len() <= expr_bytes.len() {
            // Check if operator matches at this position
            if &expr_bytes[i..i + op_bytes.len()] == op_bytes {
                return Some(i);
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    None
}
