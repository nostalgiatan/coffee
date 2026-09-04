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
use super::memory_ops::MemoryContext;
use super::memory::{LayoutCollector, safety::SafetyContext};

// Import C FFI support
use crate::c;

use crate::parser::{Program, Statement};
use crate::parser::expr::Expression;
use crate::parser::function::Function;

use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};
use inkwell::types::BasicTypeEnum;
use inkwell::types::AnyTypeEnum;
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
    pub main_entry: Option<(String, Vec<crate::parser::expr::Expression>)>,
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
        for arg_expr in &entry_args {
            match arg_expr {
                Expression::Variable(name) => {
                    if let Some(arg_num) = Self::parse_arg_number(name) {
                        let arg_value = self.get_command_line_arg(argv, argc, arg_num)?;
                        compiled_args.push(arg_value);
                    } else if name == "argc" {
                        let i64_type = context.i64_type();
                        let argc_i64 = self.backend.builder.build_int_s_extend(argc, i64_type, "argc_i64")
                            .map_err(|e| self.error("generate_main", format!("failed to extend argc to i64: {}", e)))?;
                        compiled_args.push(argc_i64.into());
                    } else if name == "argv" {
                        compiled_args.push(argv.into());
                    } else {
                        let arg_value = self.compile_expr(arg_expr).map_err(|e| {
                            self.error("generate_main",
                                format!("failed to compile entry function argument '{}': {}", arg_expr, e))
                        })?;
                        compiled_args.push(arg_value);
                    }
                }
                _ => {
                    let arg_value = self.compile_expr(arg_expr).map_err(|e| {
                        self.error("generate_main",
                            format!("failed to compile entry function argument '{}': {}", arg_expr, e))
                    })?;
                    compiled_args.push(arg_value);
                }
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
