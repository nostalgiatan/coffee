use crate::coffee_debug;
use super::borrow::BorrowCx;
use super::definition::*;
use super::registry::TypeRegistry;
use super::errors::TypeSystemError;
use crate::parser;
use crate::semantic::SemanticAnalyzer;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

mod values;
mod stmt;
mod expr;
mod call;
mod match_check;
mod memory;
mod import_main;
mod compat;

pub use values::{TypeBounds, ValueState, ValueInfo};
use import_main::ImportStack;

/// Checking mode for different levels of analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckingMode {
    /// Simple type checking only (like SimpleTypeChecker)
    #[cfg(test)]
    Simple,
    /// Comprehensive checking with ownership and lifetime analysis
    Comprehensive,
}

/// 统一类型检查器，合并 SimpleTypeChecker 和 space::TypeChecker 功能
pub struct TypeChecker {
    /// Type registry
    registry: Arc<RwLock<TypeRegistry>>,
    /// Semantic analyzer for symbol lookup
    analyzer: Option<Arc<RwLock<SemanticAnalyzer>>>,
    /// Checking mode
    mode: CheckingMode,
    /// Current return type (for return statement checking)
    current_return_type: Option<Type>,
    /// Type bounds
    bounds: TypeBounds,
    /// Value ownership tracking
    values: HashMap<String, ValueInfo>,
    /// Import stack for circular detection
    import_stack: Arc<RwLock<ImportStack>>,
    /// Error collection
    errors: Vec<TypeSystemError>,
    /// Nesting depth of `while` / `for` loops (`break`/`continue` require > 0)
    loop_depth: usize,
    /// Intra-procedural loans for `&` / `&mut`
    borrow: BorrowCx,
    /// Last-use implicit-move sites for the function currently being checked.
    last_uses: crate::types::last_use::LastUseSites,
    /// Next statement id (preorder, matches `last_use` numbering).
    stmt_cursor: usize,
    /// Id of the statement `check_statement` is currently visiting.
    current_stmt_id: usize,
    /// Known same-module functions: result of a `Ref` return aliases this param index.
    ref_return_alias: HashMap<String, usize>,
    /// Enclosing locals/params forbidden inside an anonymous `fn` body.
    capture_forbidden: HashSet<String>,
    /// Top-level `let` names. Anonymous `fn` may read these; not enclosing locals/params.
    module_value_names: HashSet<String>,
}


impl TypeChecker {
    /// Create new type checker with specified mode
    #[cfg(test)]
    pub fn new(registry: Arc<RwLock<TypeRegistry>>, mode: CheckingMode) -> Self {
        Self::with_analyzer(registry, mode, None)
    }

    /// Create new type checker with semantic analyzer
    pub fn with_analyzer(registry: Arc<RwLock<TypeRegistry>>, mode: CheckingMode, analyzer: Option<Arc<RwLock<SemanticAnalyzer>>>) -> Self {
        TypeChecker {
            registry,
            analyzer,
            mode,
            current_return_type: None,
            bounds: TypeBounds::default(),
            values: HashMap::new(),
            import_stack: Arc::new(RwLock::new(ImportStack::new())),
            errors: Vec::new(),
            loop_depth: 0,
            borrow: BorrowCx::default(),
            last_uses: crate::types::last_use::LastUseSites::new(),
            stmt_cursor: 0,
            current_stmt_id: 0,
            ref_return_alias: HashMap::new(),
            capture_forbidden: HashSet::new(),
            module_value_names: HashSet::new(),
        }
    }

    /// Create simple type checker (equivalent to SimpleTypeChecker)
    #[cfg(test)]
    pub fn simple(registry: Arc<RwLock<TypeRegistry>>) -> Self {
        Self::new(registry, CheckingMode::Simple)
    }

    /// Create comprehensive type checker (equivalent to space::TypeChecker)
    #[cfg(test)]
    pub fn comprehensive(registry: Arc<RwLock<TypeRegistry>>) -> Self {
        Self::new(registry, CheckingMode::Comprehensive)
    }

    /// Shared type registry (enum payload types for MIR match lowering).
    pub fn type_registry(&self) -> Arc<RwLock<TypeRegistry>> {
        Arc::clone(&self.registry)
    }

    pub(crate) fn ty_is_owned_resource(&self, ty: &Type) -> bool {
        if let Ok(reg) = self.registry.read() {
            if reg.type_is_c_value(ty) {
                return false;
            }
        }
        crate::types::last_use::is_owned_resource(ty)
    }

    /// Get collected errors
    pub fn errors(&self) -> &[TypeSystemError] {
        &self.errors
    }

    /// Clear errors
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }

    /// Add an error
    pub(crate) fn add_error(&mut self, error: TypeSystemError) {
        coffee_debug!("[DEBUG] add_error: adding error: {:?}", error);
        self.errors.push(error);
    }

    /// Record a check_statement `Err` that was not already pushed via `add_error`.
    pub fn record_check_error(&mut self, error: TypeSystemError) {
        self.add_error(error);
    }

    /// Register a function signature in the type registry for mutual recursion.
    /// Skips if semantic analysis already bound the name.
    pub fn declare_function_sig(&mut self, func: &parser::function::Function) -> Result<(), TypeSystemError> {
        if !func.type_params.is_empty() {
            if let Ok(reg) = self.registry.write() {
                reg.register_fn_template(func);
            }
            return Ok(());
        }
        self.record_ref_return_alias(func);
        let registry = self.registry.read().map_err(|_| TypeSystemError::ParseError {
            type_str: func.name.clone(),
            reason: "Failed to access type registry".to_string(),
        })?;
        if registry.get_alias(&func.name).is_some() {
            coffee_debug!("DEBUG: declare_function_sig: '{}' already bound, skipping", func.name);
            return Ok(());
        }
        let func_type = registry.bind_function(func)?;
        drop(registry);
        let registry = self.registry.write().map_err(|_| TypeSystemError::ParseError {
            type_str: func.name.clone(),
            reason: "Failed to access type registry".to_string(),
        })?;
        registry.define_alias(func.name.clone(), func_type)
    }

    pub(super) fn record_ref_return_alias(&mut self, func: &parser::function::Function) {
        match crate::types::borrow::alias_param_index(func) {
            Some(idx) => {
                self.ref_return_alias.insert(func.name.clone(), idx);
            }
            None => {
                self.ref_return_alias.remove(&func.name);
            }
        }
    }

    /// Set function return type from type string
    pub fn set_function_return_type(&mut self, type_str: &str) -> Result<(), TypeSystemError> {
        let return_type = self.registry
            .read()
            .map_err(|_| TypeSystemError::internal("type registry lock poisoned"))?
            .resolve_type(type_str)
            .map_err(|_| TypeSystemError::ParseError {
                type_str: type_str.to_string(),
                reason: "Failed to resolve return type".to_string(),
            })?;
        self.current_return_type = Some(return_type);
        Ok(())
    }

    /// Type-check a statement, recursing into function / if / while / match bodies
    /// so nested `let` and memory ops are checked. Compiler hookup: call this from
    /// `CompilationPipeline::type_check_statement`.
    pub fn check_statement(&mut self, statement: &parser::Statement) -> Result<(), TypeSystemError> {
        self.current_stmt_id = self.stmt_cursor;
        self.stmt_cursor += 1;
        match statement {
            parser::Statement::VariableDecl(decl) => self.check_variable_decl(decl),
            parser::Statement::MemoryOp(op) => self.check_memory_op(op).map_err(Into::into),
            parser::Statement::Function(func) => self.check_function(func),
            parser::Statement::If(if_expr) => self.check_if(if_expr),
            parser::Statement::While(while_loop) => self.check_while(while_loop),
            parser::Statement::Match(match_expr) => self.check_match(match_expr),
            parser::Statement::For(for_loop) => self.check_for(for_loop),
            parser::Statement::Expr(expr) => self.check_expr_stmt(expr),
            parser::Statement::Return(ret) => self.check_return_statement(ret),
            parser::Statement::Assignment(name, value) => self.check_assignment(name, value),
            parser::Statement::Raise(raise_stmt) => self.check_raise(&raise_stmt.error_expr),
            parser::Statement::Main(main_entry) => self.check_main_entry(main_entry),
            parser::Statement::Class(class) => self.check_class(class),
            parser::Statement::Enum(enum_def) => self.check_enum_field_types(enum_def),
            parser::Statement::Import(imp) => {
                let module = match imp {
                    parser::Import::Simple { path } => path.as_str(),
                    parser::Import::Aliased { path, .. } => path.as_str(),
                    parser::Import::InModule { module, .. } => module.as_str(),
                    parser::Import::InModuleWithLang { module, .. } => module.as_str(),
                };
                self.check_import(module)
            }
            parser::Statement::Break(_) => self.check_loop_control("break"),
            parser::Statement::Continue(_) => self.check_loop_control("continue"),
            parser::Statement::SingleLineComment(_)
            | parser::Statement::MultiLineComment(_) => Ok(()),
            parser::Statement::TypeDecl(_) => Ok(()),
        }
    }

    /// Report a diagnostic error (for compatibility)
    #[allow(dead_code)]
    pub fn report(&mut self, error: crate::diagnostics::Diagnostic) {
        // Convert diagnostic to TypeSystemError and add to errors
        self.errors.push(crate::types::TypeSystemError::from(error));
    }
}

#[cfg(test)]
mod tests;
