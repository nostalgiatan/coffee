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

use super::Backend;
use super::types::TypeMapper;

// Import modularized components
use super::arithmetic::ArithmeticContext;
use super::memory_ops::MemoryContext;
use super::memory::{LayoutCollector, safety::SafetyContext};

// Import C FFI support
use crate::c;

use inkwell::values::{FunctionValue, PointerValue};
use inkwell::types::BasicTypeEnum;

use std::collections::{HashMap, HashSet};

mod program;
mod types;
mod error;
mod layout;

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
/// // Compile a program (drivers pass frontend MIR; empty hir_fns is not an AST path)
/// let program = Program::new(vec![]);
/// let hir_fns = Vec::new();
/// codegen.compile_program_with_hir(&program, &[], std::collections::HashMap::new(), hir_fns).unwrap();
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
    /// Current function being compiled
    pub current_function: Option<FunctionValue<'ctx>>,
    /// String constants
    pub string_constants: HashMap<String, PointerValue<'ctx>>,
    /// Array length tracking (variable name -> length pointer)
    pub array_lengths: HashMap<String, PointerValue<'ctx>>,
    /// Array element type tracking (variable name -> element type)
    pub array_element_types: HashMap<String, BasicTypeEnum<'ctx>>,
    /// Array allocas tracking (variable name -> actual array alloca for GEP)
    pub array_allocas: HashMap<String, PointerValue<'ctx>>,
    /// Array sizes tracking (variable name -> size)
    pub array_sizes: HashMap<String, u32>,
    /// Pointee LLVM type for `&T` / `&mut T` locals (variable name -> pointee)
    pub ref_pointee_types: HashMap<String, BasicTypeEnum<'ctx>>,
    /// Main entry point (function name and args)
    pub main_entry: Option<(String, Vec<crate::parser::expr::Expression>)>,
    /// Current function's entry->body successor (for terminator restoration)
    pub entry_successor: Option<inkwell::basic_block::BasicBlock<'ctx>>,
    /// CRITICAL-5 FIX: Track total stack allocation size to prevent stack overflow
    pub current_stack_size: usize,
    /// HIGH-10 FIX: Track which variables are used to detect unused variables
    pub used_variables: std::collections::HashSet<String>,
    /// HIGH-13 FIX: Track expression nesting depth to prevent stack overflow
    pub expression_depth: usize,
    /// Maximum expression nesting depth (safety limit)
    pub max_expression_depth: usize,
    /// Current function's error handler name (if any)
    pub current_error_handler: Option<String>,
    /// Coffee name of the function being compiled (for listener-vs-self).
    pub current_function_name: Option<String>,
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
    /// `--enable-bitfields`: field name → GEP / bit-slice (must match LLVM `set_body`).
    pub packed_field_info: std::collections::HashMap<String, std::collections::HashMap<String, super::class_layout::PackedFieldInfo>>,
    /// Safety context for runtime checks
    pub safety_ctx: SafetyContext<'ctx>,
    /// Main function's argc (for arg1, arg2, ... access)
    pub main_argc: Option<inkwell::values::IntValue<'ctx>>,
    /// Main function's argv (for arg1, arg2, ... access)
    pub main_argv: Option<inkwell::values::PointerValue<'ctx>>,
    /// C functions that are actually used in the code (for lazy declaration)
    pub used_c_functions: std::collections::HashSet<String>,
    /// Class definitions (class name -> ClassDef)
    pub classes: std::collections::HashMap<String, crate::parser::class::ClassDef>,
    /// Enum definitions (enum name -> EnumDef)
    pub enums: std::collections::HashMap<String, crate::parser::class::EnumDef>,
    /// Complete function MIR from the frontend (omitted functions error: missing MIR).
    pub hir_fns: HashMap<String, crate::hir::MirFn>,
    /// Nested `fn` signatures keyed by `hir_fns` / LLVM name (body is empty; MIR compiles it).
    pub fn_asts: HashMap<String, crate::parser::function::Function>,
    /// Nested decl ASTs keyed by `NestedDecl.hir_key` (not stored in MIR).
    pub nested_asts: HashMap<String, crate::parser::Statement>,
    /// Remaining `Variable` / `rm` name uses in the MIR being compiled.
    /// Call args mark moved only when this hits zero (last use).
    pub(crate) mir_name_uses_left: HashMap<String, usize>,
    /// `.cfc` `c class` / `c union` / `c enum` names (memcpy, no Coffee drop).
    pub(crate) c_value_names: HashSet<String>,
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
            current_function: None,
            string_constants: HashMap::new(),
            array_lengths: HashMap::new(),
            array_element_types: HashMap::new(),
            array_allocas: HashMap::new(),
            array_sizes: HashMap::new(),
            ref_pointee_types: HashMap::new(),
            main_entry: None,
            entry_successor: None,
            current_stack_size: 0,
            used_variables: std::collections::HashSet::new(),
            expression_depth: 0,
            max_expression_depth: 1000,  // Safety limit to prevent stack overflow
            current_error_handler: None,
            current_function_name: None,
            current_function_params: Vec::new(),
            layout_collector: LayoutCollector::new(),
            c_imports: Vec::new(),
            cfc_symbols: crate::c::load_bundled_c_tables(),
            enable_bitfields: false,
            enable_safety: false,
            bit_field_layouts: std::collections::HashMap::new(),
            packed_field_info: std::collections::HashMap::new(),
            main_argc: None,
            main_argv: None,
            used_c_functions: std::collections::HashSet::new(),
            classes: std::collections::HashMap::new(),
            enums: std::collections::HashMap::new(),
            hir_fns: HashMap::new(),
            fn_asts: HashMap::new(),
            nested_asts: HashMap::new(),
            mir_name_uses_left: HashMap::new(),
            c_value_names: HashSet::new(),
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

    pub(crate) fn bitfield_storage_int_type(&self, bits: u8) -> inkwell::types::IntType<'ctx> {
        match bits {
            8 => self.backend.context.i8_type(),
            16 => self.backend.context.i16_type(),
            32 => self.backend.context.i32_type(),
            _ => self.backend.context.i64_type(),
        }
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
        // Note: panic function will be configured during compile_program_with_hir
        // after runtime functions are declared
    }

    pub(crate) fn ty_is_owned_resource(&self, ty: &crate::types::Type) -> bool {
        match ty {
            crate::types::Type::NamedType { name } if self.c_value_names.contains(name) => false,
            crate::types::Type::Array { elem, .. } => self.ty_is_owned_resource(elem),
            crate::types::Type::Tuple(elems) => elems.iter().any(|e| self.ty_is_owned_resource(e)),
            _ => crate::types::last_use::is_owned_resource(ty),
        }
    }
}
