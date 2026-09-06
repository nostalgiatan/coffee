use super::scope::ScopeSpace;
#[cfg(test)]
use super::lifetime::LifetimeSpace;
use super::symbols::SymbolSpace;
use crate::types::registry::TypeRegistry;
use crate::types::errors::TypeSystemError;
use crate::c::CSymbolTable;
use std::sync::{Arc, RwLock};
use std::collections::{HashMap, HashSet};

mod decls;
mod expr;
mod memory;
mod const_eval;
mod report;

#[allow(unused_imports)] // public path `semantic::analyzer::ConstantValue`
pub use const_eval::ConstantValue;
pub use report::{AnalysisReport, ScopeInfo, SymbolInfo};

//=============================================================================
// Semantic Analyzer
//=============================================================================

/// 语义分析器：协调所有语义分析空间
pub struct SemanticAnalyzer {
    /// 作用域管理
    pub(super) scope_space: Arc<RwLock<ScopeSpace>>,
    /// Reserved for future `'a` params. Intra-procedural borrows: `src/types/borrow.rs`.
    #[cfg(test)]
    pub(super) lifetime_space: Arc<RwLock<LifetimeSpace>>,
    /// 符号管理
    pub(super) symbol_space: Arc<RwLock<SymbolSpace>>,
    /// 类型注册器
    pub(super) type_registry: Arc<RwLock<TypeRegistry>>,
    /// 分析错误
    pub(super) errors: Vec<TypeSystemError>,
    /// C库导入（格式："library:symbol"，这些符号不需要在语义分析阶段报错）
    pub(super) c_imports: Arc<RwLock<Vec<String>>>,
    /// C函数符号表（用于类型检查C函数调用）
    pub(super) cfc_symbols: Arc<RwLock<HashMap<String, CSymbolTable>>>,
    /// 当前函数的返回类型（用于检查 void 函数的 return 语句）
    pub(super) current_function_return_type: Option<String>,
    /// 当前函数的名字（用于更好的错误提示）
    pub(super) current_function_name: Option<String>,
}


impl SemanticAnalyzer {
    pub fn new(type_registry: Arc<RwLock<TypeRegistry>>) -> Self {
        Self::with_cfc_symbols(
            type_registry,
            Arc::new(RwLock::new(crate::c::load_bundled_c_tables())),
            Arc::new(RwLock::new(Vec::new())),
        )
    }

    /// Share the session C tables (same `Arc` as `Session`).
    pub fn with_cfc_symbols(
        type_registry: Arc<RwLock<TypeRegistry>>,
        cfc_symbols: Arc<RwLock<HashMap<String, CSymbolTable>>>,
        c_imports: Arc<RwLock<Vec<String>>>,
    ) -> Self {
        SemanticAnalyzer {
            scope_space: Arc::new(RwLock::new(ScopeSpace::root())),
            #[cfg(test)]
            lifetime_space: Arc::new(RwLock::new(LifetimeSpace::new("global"))),
            symbol_space: Arc::new(RwLock::new(SymbolSpace::new("global"))),
            type_registry,
            errors: Vec::new(),
            c_imports,
            cfc_symbols,
            current_function_return_type: None,
            current_function_name: None,
        }
    }

    /// 获取作用域空间
    pub fn scope_space(&self) -> Arc<RwLock<ScopeSpace>> {
        Arc::clone(&self.scope_space)
    }

    /// Reserved for future `'a`. Borrow checking is `src/types/borrow.rs`.
    #[cfg(test)]
    pub fn lifetime_space(&self) -> Arc<RwLock<LifetimeSpace>> {
        Arc::clone(&self.lifetime_space)
    }

    /// 获取符号空间
    pub fn symbol_space(&self) -> Arc<RwLock<SymbolSpace>> {
        Arc::clone(&self.symbol_space)
    }

    /// 获取类型注册器
    pub fn type_registry(&self) -> Arc<RwLock<TypeRegistry>> {
        Arc::clone(&self.type_registry)
    }

    /// 进入新的作用域
    pub fn enter_scope(&self, name: &str) -> Arc<RwLock<ScopeSpace>> {
        if let Ok(scope) = self.scope_space.read() {
            let child = scope.child(name);
            Arc::new(RwLock::new((*child).clone()))
        } else {
            Arc::new(RwLock::new(ScopeSpace::new(name)))
        }
    }

    /// 获取分析错误
    pub fn errors(&self) -> &[TypeSystemError] {
        &self.errors
    }

    /// 添加错误
    pub fn add_error(&mut self, error: TypeSystemError) {
        self.errors.push(error);
    }

    /// 清空错误
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }

    /// 设置C库导入列表
    pub fn set_c_imports(&self, imports: Vec<String>) {
        if let Ok(mut c_imports) = self.c_imports.write() {
            *c_imports = imports;
        }
    }

    /// 设置C函数符号表（会与内置符号合并）
    pub fn set_cfc_symbols(&self, symbols: HashMap<String, CSymbolTable>) {
        if let Ok(mut cfc_symbols) = self.cfc_symbols.write() {
            // 合并符号表，而不是覆盖内置符号
            for (library, symbol_table) in symbols.iter() {
                cfc_symbols.insert(library.clone(), symbol_table.clone());
            }
        }
    }

    /// 获取C函数符号表
    pub fn get_cfc_symbols(&self) -> Result<HashMap<String, CSymbolTable>, TypeSystemError> {
        self.cfc_symbols.read()
            .map(|symbols| symbols.clone())
            .map_err(|_| TypeSystemError::ParseError {
                type_str: "cfc_symbols".to_string(),
                reason: "Failed to access cfc_symbols".to_string(),
            })
    }

    /// 获取C函数符号
    pub(super) fn get_c_symbol(&self, symbol_name: &str) -> Option<crate::c::CSymbol> {
        if let Ok(cfc_symbols) = self.cfc_symbols.read() {
            for (_library, symbol_table) in cfc_symbols.iter() {
                if let Some(symbol) = symbol_table.symbols.get(symbol_name) {
                    return Some(symbol.clone());
                }
            }
        }
        None
    }

    /// True only if the source listed `use <name> in … of c` (not a language primitive).
    pub fn is_explicit_c_import(&self, symbol_name: &str) -> bool {
        if let Ok(c_imports) = self.c_imports.read() {
            for c_import in c_imports.iter() {
                if c_import.contains(':') {
                    let parts: Vec<&str> = c_import.split(':').collect();
                    if parts.len() == 2 && parts[1] == symbol_name {
                        return true;
                    }
                } else if c_import == symbol_name {
                    return true;
                }
            }
        }
        false
    }

    /// 检查符号是否是C库导入
    pub(super) fn is_c_import(&self, symbol_name: &str) -> bool {
        if self.is_explicit_c_import(symbol_name) {
            return true;
        }

        // 然后检查是否是内置的C函数（libc/libm）
        if self.get_c_symbol(symbol_name).is_some() {
            return true;
        }

        false
    }

    /// 检查符号是否是命令行参数 (arg1, arg2, etc.)
    pub(super) fn is_command_line_arg(&self, symbol_name: &str) -> bool {
        let symbol_name = symbol_name.trim();

        // Check if it matches arg1, arg2, ..., argN pattern
        if symbol_name.starts_with("arg") {
            let num_str = &symbol_name[3..]; // Skip "arg"
            if let Ok(num) = num_str.parse::<u32>() {
                // Valid if it's arg1 through arg127
                return num >= 1 && num <= 127;
            }
        }

        false
    }
    /// 执行完整的语义分析
    pub fn analyze(&mut self, program: &crate::parser::Program) -> Result<(), Vec<TypeSystemError>> {
        self.clear_errors();

        // 分析所有顶级声明
        for stmt in &program.statements {
            if let Err(error) = self.analyze_statement(stmt) {
                self.add_error(error);
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors.clone())
        }
    }

    /// 分析语句
    pub fn analyze_statement(&mut self, stmt: &crate::parser::Statement) -> Result<(), TypeSystemError> {
        match stmt {
            crate::parser::Statement::VariableDecl(decl) => {
                self.analyze_variable_decl(decl)
            }
            crate::parser::Statement::Function(func) => {
                // Check for import statements inside function body
                if let crate::parser::function::FunctionBody::Block(statements) = &func.body {
                    for stmt in statements {
                        if matches!(stmt, crate::parser::Statement::Import(_)) {
                            return Err(TypeSystemError::ParseError {
                                type_str: "import statement".to_string(),
                                reason: format!("import statements are not allowed inside function '{}'\n  = note: move the import statement to the top of the file, before any function definitions", func.name),
                            });
                        }
                    }
                }
                self.analyze_function_decl(func)
            }
            crate::parser::Statement::Class(class) => {
                // 先获取 type_def，这会在 block 结束时释放读锁
                let type_def = match self.type_registry.read() {
                    Ok(reg) => reg.bind_class(class)?,
                    Err(_) => return Err(TypeSystemError::ParseError {
                        type_str: class.name.clone(),
                        reason: "Failed to access type registry".to_string(),
                    }),
                };
                // 读锁已释放，现在可以安全调用 analyze_type_def（它会尝试获取写锁）
                self.analyze_type_def(&class.name, type_def)
            }
            crate::parser::Statement::Enum(enum_def) => {
                let type_def = match self.type_registry.read() {
                    Ok(reg) => reg.bind_enum(enum_def)?,
                    Err(_) => return Err(TypeSystemError::ParseError {
                        type_str: enum_def.name.clone(),
                        reason: "Failed to access type registry".to_string(),
                    }),
                };
                self.analyze_type_def(&enum_def.name, type_def)
            }
            crate::parser::Statement::Expr(expr) => {
                self.analyze_expression(expr)
            }
            crate::parser::Statement::Return(ret_stmt) => {
                if let Some(ref return_type) = self.current_function_return_type {
                    let is_void = return_type == "void" || return_type == "()";

                    if is_void {
                        if let Some(ref ret_value) = ret_stmt.value {
                            let func_name = self.current_function_name.as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or("<unknown>");

                            return Err(TypeSystemError::ParseError {
                                type_str: return_type.clone(),
                                reason: format!(
                                    "function '{}' is declared to return void, but returns a value: {}",
                                    func_name, ret_value
                                ),
                            });
                        }
                    }
                }

                if let Some(ref ret_value) = ret_stmt.value {
                    self.analyze_expression(ret_value)?;
                }
                Ok(())
            }
            crate::parser::Statement::If(if_expr) => {
                self.analyze_expression(&if_expr.condition)?;
                self.analyze_block_scope(&if_expr.body)?;
                for elif in &if_expr.elifs {
                    self.analyze_expression(&elif.condition)?;
                    self.analyze_block_scope(&elif.body)?;
                }
                if let Some(else_body) = &if_expr.else_body {
                    self.analyze_block_scope(else_body)?;
                }
                Ok(())
            }
            crate::parser::Statement::While(while_loop) => {
                self.analyze_expression(&while_loop.condition)?;
                self.analyze_block_scope(&while_loop.body)
            }
            crate::parser::Statement::For(for_loop) => {
                match &for_loop.iterator {
                    crate::parser::ForIterator::Range { start, end } => {
                        self.analyze_expression(start)?;
                        self.analyze_expression(end)?;
                    }
                    crate::parser::ForIterator::Collection(expr) => {
                        self.analyze_expression(expr)?;
                    }
                }
                self.bind_ephemeral_var(&for_loop.variable)?;
                let result = self.analyze_block_scope(&for_loop.body);
                self.unbind_ephemeral_var(&for_loop.variable);
                result
            }
            crate::parser::Statement::Assignment(name, value) => {
                self.analyze_assignment_lhs(name)?;
                self.analyze_expression(value)
            }
            crate::parser::Statement::Main(main_entry) => {
                for arg in &main_entry.args {
                    self.analyze_expression(arg)?;
                }
                Ok(())
            }
            crate::parser::Statement::Match(match_expr) => {
                self.analyze_expression(&match_expr.value)?;
                for arm in &match_expr.arms {
                    let bound = Self::pattern_binding_names(&arm.pattern);
                    for name in &bound {
                        self.bind_ephemeral_var(name)?;
                    }
                    if let Some(guard) = &arm.guard {
                        self.analyze_expression(guard)?;
                    }
                    self.analyze_block_scope(&arm.body)?;
                    for name in bound.iter().rev() {
                        self.unbind_ephemeral_var(name);
                    }
                }
                Ok(())
            }
            crate::parser::Statement::MemoryOp(op) => self.analyze_memory_op(op),
            _ => Ok(())
        }
    }

    /// Walk a dotted assignment target (`p`, `p.x`, `p.x.y`) as a Member chain.
    fn analyze_assignment_lhs(&self, name: &str) -> Result<(), TypeSystemError> {
        use crate::parser::expr::Expression;
        let mut parts = name.split('.');
        let Some(first) = parts.next() else {
            return Ok(());
        };
        let mut expr = Expression::Variable(first.to_string());
        self.analyze_expression(&expr)?;
        for field in parts {
            expr = Expression::Member {
                object: Box::new(expr),
                field: field.to_string(),
                args: Vec::new(),
            };
            self.analyze_expression(&expr)?;
        }
        Ok(())
    }

    fn analyze_stmt_list(&mut self, stmts: &[crate::parser::Statement]) -> Result<(), TypeSystemError> {
        for stmt in stmts {
            self.analyze_statement(stmt)?;
        }
        Ok(())
    }

    /// Nested block: names declared here are dropped afterward so if/elif/else
    /// can reuse the same `let` identifier.
    fn analyze_block_scope(&mut self, stmts: &[crate::parser::Statement]) -> Result<(), TypeSystemError> {
        let before: HashSet<String> = if let Ok(symbols) = self.symbol_space.read() {
            symbols.declared_symbols().into_iter().collect()
        } else {
            HashSet::new()
        };
        let result = self.analyze_stmt_list(stmts);
        if let Ok(mut symbols) = self.symbol_space.write() {
            let extra: Vec<String> = symbols
                .declared_symbols()
                .into_iter()
                .filter(|name| !before.contains(name))
                .collect();
            for name in extra {
                symbols.remove(&name);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_program;
    use crate::types::registry::TypeRegistry;

    #[test]
    fn function_name_is_not_registered_as_lifetime_param() {
        let src = "fn foo() => int:\n    return 0\n";
        let program = parse_program(src).expect("parse");
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut analyzer = SemanticAnalyzer::new(registry);
        analyzer.analyze(&program).expect("analyze");
        let space = analyzer.lifetime_space.read().expect("lifetime_space");
        assert!(
            space.get_param("foo").is_none(),
            "function names must not be LifetimeParam; borrow checking is types::borrow"
        );
    }
}
