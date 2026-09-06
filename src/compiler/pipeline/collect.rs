use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use crate::diagnostics::{Diagnostic, ErrorKind, Severity};
use crate::parser;
use crate::parser::expr::Expression;
use crate::parser::function::{unique_function_key, Function, FunctionBody};
use crate::parser::r#for::ForIterator;
use crate::types::{TypeChecker, TypeRegistry};
use super::CompilationPipeline;
use crate::compiler::CompilationStatistics;

impl CompilationPipeline {
    /// Register `type Name: object` from `.cfc` tables and from the program.
    /// Same name+source as an existing newtype is a no-op (libc `FILE` + user `type FILE`).
    pub(super) fn register_nominal_newtypes(&self, program: &parser::Program) {
        let type_checker = self.session.type_checker.write().unwrap();
        let registry = type_checker.type_registry();
        let reg = registry.write().unwrap();

        let tables = self.session.cfc_symbols.read().unwrap();
        let mut seen_types: HashMap<String, (String, crate::c::CTypeDef)> = HashMap::new();
        for (module, table) in tables.iter() {
            for def in table.type_defs.values() {
                let name = def.name().to_string();
                if let Some((prev_mod, prev)) = seen_types.get(&name) {
                    if prev != def {
                        self.session.emitter.emit(
                            Diagnostic::new(
                                Severity::Error,
                                ErrorKind::InvalidType {
                                    name: name.clone(),
                                    reason: format!(
                                        "c type '{name}' in library '{module}' disagrees with '{prev_mod}'"
                                    ),
                                },
                                format!(
                                    "c type '{name}' is defined differently in '{prev_mod}' and '{module}'"
                                ),
                            )
                            .with_suggestion(crate::diagnostics::Suggestion::new(
                                "keep one definition; regenerate both .cfc files from the same headers, or rename one type",
                            )),
                        );
                    }
                } else {
                    seen_types.insert(name, (module.clone(), def.clone()));
                }
            }
        }
        for table in tables.values() {
            for def in table.type_defs.values() {
                if let crate::c::CTypeDef::Newtype { name, source } = def {
                    Self::register_one_newtype(&reg, name, source, Some(&self.session.emitter));
                }
            }
            for def in table.type_defs.values() {
                if let crate::c::CTypeDef::Enum { name, variants } = def {
                    Self::register_c_enum(&reg, name, variants, Some(&self.session.emitter));
                }
            }
            for def in table.type_defs.values() {
                match def {
                    crate::c::CTypeDef::Class {
                        name,
                        fields,
                        size,
                        align,
                    } => {
                        Self::register_c_record(
                            &reg,
                            name,
                            fields,
                            *size,
                            *align,
                            crate::types::registry::CLayoutKind::Class,
                            Some(&self.session.emitter),
                        );
                    }
                    crate::c::CTypeDef::Union {
                        name,
                        fields,
                        size,
                        align,
                    } => {
                        Self::register_c_record(
                            &reg,
                            name,
                            fields,
                            *size,
                            *align,
                            crate::types::registry::CLayoutKind::Union,
                            Some(&self.session.emitter),
                        );
                    }
                    crate::c::CTypeDef::Newtype { .. } | crate::c::CTypeDef::Enum { .. } => {}
                }
            }
        }
        drop(tables);

        for statement in &program.statements {
            if let parser::Statement::TypeDecl(decl) = statement {
                Self::register_one_newtype(
                    &reg,
                    &decl.name,
                    &decl.source,
                    Some(&self.session.emitter),
                );
            }
        }
    }

    fn register_one_newtype(
        reg: &TypeRegistry,
        name: &str,
        source_str: &str,
        emitter: Option<&crate::diagnostics::DiagnosticEmitter>,
    ) {
        let source = match crate::types::Type::from_str(source_str) {
            Ok(ty) => ty,
            Err(reason) => {
                if let Some(emitter) = emitter {
                    emitter.emit(Diagnostic::new(
                        Severity::Error,
                        ErrorKind::InvalidType {
                            name: name.to_string(),
                            reason: reason.clone(),
                        },
                        format!("invalid newtype source for '{name}': {reason}"),
                    ));
                }
                return;
            }
        };
        if reg.is_newtype(name) {
            if reg.newtype_source(name).as_ref() != Some(&source) {
                if let Some(emitter) = emitter {
                    emitter.emit(Diagnostic::new(
                        Severity::Error,
                        ErrorKind::InvalidType {
                            name: name.to_string(),
                            reason: "newtype source mismatch".to_string(),
                        },
                        format!("type '{name}' already registered with a different source"),
                    ));
                }
            }
            return;
        }
        if let Err(e) = reg.register_newtype(name.to_string(), source) {
            if let Some(emitter) = emitter {
                emitter.emit(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::InvalidType {
                        name: name.to_string(),
                        reason: e.to_string(),
                    },
                    format!("failed to register type '{name}': {e}"),
                ));
            }
        }
    }

    fn register_c_record(
        reg: &TypeRegistry,
        name: &str,
        fields: &[(String, String)],
        size: u64,
        align: u64,
        kind: crate::types::registry::CLayoutKind,
        emitter: Option<&crate::diagnostics::DiagnosticEmitter>,
    ) {
        if reg.is_c_value_type(name) {
            return;
        }
        let class = crate::parser::class::ClassDef {
            name: name.to_string(),
            type_params: vec![],
            parent: None,
            fields: fields
                .iter()
                .map(|(n, t)| crate::parser::class::ClassField {
                    name: n.clone(),
                    field_type: t.clone(),
                    bit_width: None,
                })
                .collect(),
            methods: vec![],
            packed: false,
            has_constructor: false,
        };
        match reg.bind_class(&class) {
            Ok(td) => {
                if let Err(e) = reg.define_type(name.to_string(), td) {
                    if let Some(emitter) = emitter {
                        emitter.emit(Diagnostic::new(
                            Severity::Error,
                            ErrorKind::InvalidType {
                                name: name.to_string(),
                                reason: e.to_string(),
                            },
                            format!("failed to register c record '{name}': {e}"),
                        ));
                    }
                    return;
                }
            }
            Err(e) => {
                if let Some(emitter) = emitter {
                    emitter.emit(Diagnostic::new(
                        Severity::Error,
                        ErrorKind::InvalidType {
                            name: name.to_string(),
                            reason: e.to_string(),
                        },
                        format!("failed to bind c record '{name}': {e}"),
                    ));
                }
                return;
            }
        }
        reg.register_c_layout(
            name.to_string(),
            crate::types::registry::CLayout { size, align, kind },
        );
    }

    fn register_c_enum(
        reg: &TypeRegistry,
        name: &str,
        variants: &[(String, i64)],
        emitter: Option<&crate::diagnostics::DiagnosticEmitter>,
    ) {
        if reg.is_c_value_type(name) {
            return;
        }
        let def = crate::types::definition::TypeDef::Enum {
            name: name.to_string(),
            variants: variants
                .iter()
                .map(|(n, _)| crate::types::definition::EnumVariant {
                    name: n.clone(),
                    fields: vec![],
                })
                .collect(),
            generics: vec![],
        };
        if let Err(e) = reg.define_type(name.to_string(), def) {
            if let Some(emitter) = emitter {
                emitter.emit(Diagnostic::new(
                    Severity::Error,
                    ErrorKind::InvalidType {
                        name: name.to_string(),
                        reason: e.to_string(),
                    },
                    format!("failed to register c enum '{name}': {e}"),
                ));
            }
            return;
        }
        reg.register_c_layout(
            name.to_string(),
            crate::types::registry::CLayout {
                size: 4,
                align: 4,
                kind: crate::types::registry::CLayoutKind::Enum,
            },
        );
    }

    /// Collect statistics from a statement
    pub(super) fn collect_statistics(&self, statement: &parser::Statement, stats: &mut CompilationStatistics) {
        match statement {
            parser::Statement::Import(_) => stats.imports_processed += 1,
            parser::Statement::Function(_) => stats.functions_declared += 1,
            parser::Statement::Main(_) => {} // Main entry point - no specific stat needed
            parser::Statement::Class(_) => stats.classes_declared += 1,
            parser::Statement::Enum(_) => stats.enums_declared += 1,
            parser::Statement::TypeDecl(_) => {}
            parser::Statement::VariableDecl(_) => stats.variables_declared += 1,
            parser::Statement::Assignment(_, _) => stats.assignments_made += 1,
            parser::Statement::Return(_) => stats.return_statements += 1,
            parser::Statement::If(_) => stats.if_expressions += 1,
            parser::Statement::While(_) => stats.while_loops += 1,
            parser::Statement::For(_) => stats.for_loops += 1,
            parser::Statement::Match(_) => stats.match_expressions += 1,
            parser::Statement::Break(_) => stats.break_statements += 1,
            parser::Statement::Continue(_) => stats.continue_statements += 1,
            parser::Statement::MemoryOp(_) => stats.memory_operations += 1,
            parser::Statement::Raise(_) => stats.raise_statements += 1,
            parser::Statement::SingleLineComment(_) | parser::Statement::MultiLineComment(_) => {
                stats.comments_count += 1
            }
            parser::Statement::Expr(_) => {
                stats.expression_statements += 1
            }
        }
    }

    /// Extract exported symbols from statements
    pub(super) fn extract_exports(statements: &[parser::Statement]) -> Vec<String> {
        let mut exports = Vec::new();

        for statement in statements {
            match statement {
                parser::Statement::Function(func) => {
                    exports.push(func.name.clone());
                }
                parser::Statement::Class(class) => {
                    exports.push(class.name.clone());
                }
                parser::Statement::Enum(enum_def) => {
                    exports.push(enum_def.name.clone());
                }
                parser::Statement::VariableDecl(var_decl) => {
                    // Export all variables (could add visibility modifiers later)
                    exports.push(var_decl.name.clone());
                }
                _ => {}
            }
        }

        exports
    }

    /// Count program entry points: `main(...)` statements and functions named `main`.
    /// Both in one file (or two `fn main`) is an error.
    pub fn count_entry_points(program: &parser::Program) -> usize {
        program.statements.iter().filter(|stmt| match stmt {
            parser::Statement::Main(_) => true,
            parser::Statement::Function(f) => f.name == "main",
            _ => false,
        }).count()
    }

    /// Lower `func` to MIR under `hir_name` (LLVM / `hir_fns` map key).
    ///
    /// `lower_function` Err is **not** silent: a diagnostic is always emitted.
    /// Remaining Err cases (nested `class`/`fn` lower as `Nested` and stay
    /// complete; comment/import are skipped):
    /// - `check_expression` failed on `Call`/`Member`/`Assign` when the
    ///   name is not in the post-bind snapshot (undefined / not a local).
    /// - Simple `match` with no arms (`match has no arms`).
    /// - Match scrutinee `lower_expr` Err when no pattern type hint exists.
    /// - Internal: `leaf expression has no children` if a leaf used `rec`.
    ///
    /// Valid `let n = p.x` then `rm p` must lower: after a successful bind,
    /// locals are revived to `Alive` (types kept) so HIR infer does not
    /// re-fail on end-of-body `Dropped`. Use-after-`rm` in program order is
    /// still checked by `check_function` before bind.
    ///
    /// `lower_function` Err is a hard Error (`CompilationResult.success` false)
    /// so codegen never compiles that body from the parser AST.
    pub(super) fn lower_function_mir(
        &self,
        type_checker: &mut TypeChecker,
        type_registry: &Arc<RwLock<TypeRegistry>>,
        func: &Function,
        hir_name: String,
        taken: HashSet<String>,
    ) -> Option<crate::hir::MirFn> {
        let saved = type_checker.snapshot_values();
        if type_checker.bind_function_locals(func).is_ok() {
            type_checker.revive_bound_locals_for_lowering();
        }
        // Types from bind; states are Alive after revive so Member/Call infer.
        let bound = type_checker.snapshot_values();
        let mir = {
            let mut lower = crate::hir::HirLower::new(|expr| {
                match type_checker.check_expression(expr) {
                    Ok(ty) => Ok(ty),
                    Err(e) => match expr.kind() {
                        parser::expr::Expression::Variable(name) => bound
                            .get(name)
                            .map(|info| info.ty.clone())
                            .or_else(|| type_checker.type_of_global_name(name))
                            .ok_or_else(|| e.to_string()),
                        _ => Err(e.to_string()),
                    },
                }
            });
            lower.set_registry(type_registry.clone());
            match crate::hir::lower_function_keyed(func, &mut lower, &hir_name, taken) {
                Ok(mut m) => {
                    if m.param_names.is_empty() && !func.parameters.is_empty() {
                        m.param_names =
                            func.parameters.iter().map(|p| p.name.clone()).collect();
                        m.param_types = func
                            .parameters
                            .iter()
                            .map(|p| p.param_type.clone())
                            .collect();
                    }
                    if m.return_type.is_empty() {
                        m.return_type = func.return_type.clone();
                    }
                    Some(m)
                }
                Err(err) => {
                    self.session.emitter.emit(Diagnostic::new(
                        Severity::Error,
                        ErrorKind::CodeGeneration {
                            stage: "mir".to_string(),
                            details: format!("{}: {}", hir_name, err),
                        },
                        format!(
                            "failed to lower function '{}' to MIR: {}",
                            hir_name, err
                        ),
                    ));
                    None
                }
            }
        };
        type_checker.unbind_function_locals(saved);
        mir
    }

    pub(super) fn mir_name_taken(reserved: &HashSet<String>, hir_fns: &[crate::hir::MirFn]) -> HashSet<String> {
        let mut taken = reserved.clone();
        for f in hir_fns {
            taken.insert(f.name.clone());
        }
        taken
    }

    /// Nested `fn` bodies are MIR-complete; they must also appear in `hir_fns`
    /// with [`unique_function_key`] so two `helper`s in different parents do not collide.
    pub(super) fn collect_nested_functions(
        &self,
        type_checker: &mut TypeChecker,
        type_registry: &Arc<RwLock<TypeRegistry>>,
        stmts: &[parser::Statement],
        parent: &str,
        reserved: &HashSet<String>,
        hir_fns: &mut Vec<crate::hir::MirFn>,
    ) {
        for stmt in stmts {
            self.collect_anons_in_stmt(
                type_checker,
                type_registry,
                stmt,
                parent,
                reserved,
                hir_fns,
            );
            match stmt {
                parser::Statement::Function(nested) => {
                    let key = unique_function_key(&nested.name, Some(parent), |n| {
                        reserved.contains(n) || hir_fns.iter().any(|f| f.name == n)
                    });
                    if let Some(mir) = self.lower_function_mir(
                        type_checker,
                        type_registry,
                        nested,
                        key.clone(),
                        Self::mir_name_taken(reserved, hir_fns),
                    ) {
                        hir_fns.push(mir);
                    }
                    if let FunctionBody::Block(body) = &nested.body {
                        self.collect_nested_functions(
                            type_checker,
                            type_registry,
                            body,
                            &key,
                            reserved,
                            hir_fns,
                        );
                    }
                }
                parser::Statement::If(if_expr) => {
                    self.collect_nested_functions(
                        type_checker,
                        type_registry,
                        &if_expr.body,
                        parent,
                        reserved,
                        hir_fns,
                    );
                    for elif in &if_expr.elifs {
                        self.collect_nested_functions(
                            type_checker,
                            type_registry,
                            &elif.body,
                            parent,
                            reserved,
                            hir_fns,
                        );
                    }
                    if let Some(else_body) = &if_expr.else_body {
                        self.collect_nested_functions(
                            type_checker,
                            type_registry,
                            else_body,
                            parent,
                            reserved,
                            hir_fns,
                        );
                    }
                }
                parser::Statement::While(w) => {
                    self.collect_nested_functions(
                        type_checker,
                        type_registry,
                        &w.body,
                        parent,
                        reserved,
                        hir_fns,
                    );
                }
                parser::Statement::For(f) => {
                    self.collect_nested_functions(
                        type_checker,
                        type_registry,
                        &f.body,
                        parent,
                        reserved,
                        hir_fns,
                    );
                }
                parser::Statement::Match(m) => {
                    for arm in &m.arms {
                        self.collect_nested_functions(
                            type_checker,
                            type_registry,
                            &arm.body,
                            parent,
                            reserved,
                            hir_fns,
                        );
                    }
                }
                parser::Statement::Class(class) => {
                    for method in &class.methods {
                        let func = method.to_standalone_function(&class.name);
                        let key = unique_function_key(&func.name, Some(parent), |n| {
                            reserved.contains(n) || hir_fns.iter().any(|f| f.name == n)
                        });
                        if let Some(mir) = self.lower_function_mir(
                            type_checker,
                            type_registry,
                            &func,
                            key.clone(),
                            Self::mir_name_taken(reserved, hir_fns),
                        ) {
                            hir_fns.push(mir);
                        }
                        if let FunctionBody::Block(body) = &func.body {
                            self.collect_nested_functions(
                                type_checker,
                                type_registry,
                                body,
                                &key,
                                reserved,
                                hir_fns,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_anons_in_stmt(
        &self,
        type_checker: &mut TypeChecker,
        type_registry: &Arc<RwLock<TypeRegistry>>,
        stmt: &parser::Statement,
        parent: &str,
        reserved: &HashSet<String>,
        hir_fns: &mut Vec<crate::hir::MirFn>,
    ) {
        match stmt {
            parser::Statement::VariableDecl(d) => {
                self.collect_anons_in_expr(
                    type_checker,
                    type_registry,
                    &d.value,
                    parent,
                    reserved,
                    hir_fns,
                );
            }
            parser::Statement::Assignment(_, value) => {
                self.collect_anons_in_expr(
                    type_checker,
                    type_registry,
                    value,
                    parent,
                    reserved,
                    hir_fns,
                );
            }
            parser::Statement::Expr(value) => {
                self.collect_anons_in_expr(
                    type_checker,
                    type_registry,
                    value.as_ref(),
                    parent,
                    reserved,
                    hir_fns,
                );
            }
            parser::Statement::Return(r) => {
                if let Some(value) = &r.value {
                    self.collect_anons_in_expr(
                        type_checker,
                        type_registry,
                        value,
                        parent,
                        reserved,
                        hir_fns,
                    );
                }
            }
            parser::Statement::Raise(r) => {
                self.collect_anons_in_expr(
                    type_checker,
                    type_registry,
                    &r.error_expr,
                    parent,
                    reserved,
                    hir_fns,
                );
            }
            parser::Statement::If(if_expr) => {
                self.collect_anons_in_expr(
                    type_checker,
                    type_registry,
                    &if_expr.condition,
                    parent,
                    reserved,
                    hir_fns,
                );
                for elif in &if_expr.elifs {
                    self.collect_anons_in_expr(
                        type_checker,
                        type_registry,
                        &elif.condition,
                        parent,
                        reserved,
                        hir_fns,
                    );
                }
            }
            parser::Statement::While(w) => {
                self.collect_anons_in_expr(
                    type_checker,
                    type_registry,
                    &w.condition,
                    parent,
                    reserved,
                    hir_fns,
                );
            }
            parser::Statement::For(f) => match &f.iterator {
                ForIterator::Collection(e) => {
                    self.collect_anons_in_expr(
                        type_checker,
                        type_registry,
                        e,
                        parent,
                        reserved,
                        hir_fns,
                    );
                }
                ForIterator::Range { start, end } => {
                    self.collect_anons_in_expr(
                        type_checker,
                        type_registry,
                        start,
                        parent,
                        reserved,
                        hir_fns,
                    );
                    self.collect_anons_in_expr(
                        type_checker,
                        type_registry,
                        end,
                        parent,
                        reserved,
                        hir_fns,
                    );
                }
            },
            parser::Statement::Match(m) => {
                self.collect_anons_in_expr(
                    type_checker,
                    type_registry,
                    &m.value,
                    parent,
                    reserved,
                    hir_fns,
                );
                for arm in &m.arms {
                    if let Some(g) = &arm.guard {
                        self.collect_anons_in_expr(
                            type_checker,
                            type_registry,
                            g,
                            parent,
                            reserved,
                            hir_fns,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_anons_in_expr(
        &self,
        type_checker: &mut TypeChecker,
        type_registry: &Arc<RwLock<TypeRegistry>>,
        expr: &Expression,
        parent: &str,
        reserved: &HashSet<String>,
        hir_fns: &mut Vec<crate::hir::MirFn>,
    ) {
        match expr.kind() {
            Expression::AnonymousFunction { func } => {
                let key = unique_function_key(&func.name, Some(parent), |n| {
                    reserved.contains(n) || hir_fns.iter().any(|f| f.name == n)
                });
                let mut nested = func.as_ref().clone();
                nested.name = key.clone();
                if let Some(mir) = self.lower_function_mir(
                    type_checker,
                    type_registry,
                    &nested,
                    key.clone(),
                    Self::mir_name_taken(reserved, hir_fns),
                ) {
                    hir_fns.push(mir);
                }
                if let FunctionBody::Block(body) = &nested.body {
                    self.collect_nested_functions(
                        type_checker,
                        type_registry,
                        body,
                        &key,
                        reserved,
                        hir_fns,
                    );
                }
            }
            Expression::Binary { left, right, .. } => {
                self.collect_anons_in_expr(type_checker, type_registry, left, parent, reserved, hir_fns);
                self.collect_anons_in_expr(type_checker, type_registry, right, parent, reserved, hir_fns);
            }
            Expression::Unary { operand, .. }
            | Expression::Member { object: operand, .. }
            | Expression::TypeCast { value: operand, .. } => {
                self.collect_anons_in_expr(
                    type_checker,
                    type_registry,
                    operand,
                    parent,
                    reserved,
                    hir_fns,
                );
            }
            Expression::Call { function, args } => {
                self.collect_anons_in_expr(
                    type_checker,
                    type_registry,
                    function,
                    parent,
                    reserved,
                    hir_fns,
                );
                for a in args {
                    self.collect_anons_in_expr(type_checker, type_registry, a, parent, reserved, hir_fns);
                }
            }
            Expression::ConstructorCall { args, .. } => {
                for a in args {
                    self.collect_anons_in_expr(type_checker, type_registry, a, parent, reserved, hir_fns);
                }
            }
            Expression::Index { array, index } => {
                self.collect_anons_in_expr(type_checker, type_registry, array, parent, reserved, hir_fns);
                self.collect_anons_in_expr(type_checker, type_registry, index, parent, reserved, hir_fns);
            }
            Expression::ArrayLiteral { elements } | Expression::TupleLiteral { elements } => {
                for e in elements {
                    self.collect_anons_in_expr(type_checker, type_registry, e, parent, reserved, hir_fns);
                }
            }
            Expression::StructLiteral { fields, .. } => {
                for (_, e) in fields {
                    self.collect_anons_in_expr(type_checker, type_registry, e, parent, reserved, hir_fns);
                }
            }
            Expression::Assign { object, value, .. } => {
                self.collect_anons_in_expr(type_checker, type_registry, object, parent, reserved, hir_fns);
                self.collect_anons_in_expr(type_checker, type_registry, value, parent, reserved, hir_fns);
            }
            _ => {}
        }
    }
}
