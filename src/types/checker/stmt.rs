use super::*;
use super::super::definition::{Span, Type};
use crate::coffee_debug;
use crate::parser;
use crate::types::errors::TypeSystemError;
use std::collections::{HashMap, HashSet};

impl super::TypeChecker {
    /// Check a return statement
    pub fn check_return_statement(&mut self, return_stmt: &parser::var::ReturnStmt) -> Result<(), TypeSystemError> {
        coffee_debug!("[DEBUG] check_return_statement: checking return statement");
        if let Some(ref expr) = return_stmt.value {
            coffee_debug!("[DEBUG] check_return_statement: checking return expression");
            let hint = self.current_return_type.clone();
            let expr_type = self.check_expression_hint(expr, hint.as_ref())?;
            coffee_debug!("[DEBUG] check_return_statement: inferred return value type as {:?}", expr_type);
            
            if let Some(ref expected_type) = self.current_return_type {
                coffee_debug!("[DEBUG] check_return_statement: expected return type is {:?}", expected_type);
                if !self.types_compatible(&expr_type, expected_type)? {
                    coffee_debug!("[DEBUG] check_return_statement: return type mismatch");
                    let error = TypeSystemError::type_mismatch(
                        expected_type.clone(),
                        expr_type,
                        Span::new(0, expr.to_string().len())
                    );
                    self.add_error(error.clone());
                    return Err(error);
                }
                if let Err(e) = self.check_ref_escape(expr, expected_type) {
                    self.add_error(e.clone());
                    return Err(e);
                }
                if self.ty_is_owned_resource(expected_type) {
                    if let Some(src) = crate::types::last_use::simple_var_name(expr) {
                        if self.is_last_use(src) {
                            self.implicit_move_source(src)?;
                        } else {
                            let error = TypeSystemError::OwnershipError {
                                reason: format!(
                                    "cannot copy resource '{}' with return; use mv or clone",
                                    src
                                ),
                                span: Span::new(0, src.len()),
                            };
                            self.add_error(error.clone());
                            return Err(error);
                        }
                    }
                }
            } else {
                let error = TypeSystemError::ParseError {
                    type_str: "return".to_string(),
                    reason: "return used outside of a function".to_string(),
                };
                self.add_error(error.clone());
                return Err(error);
            }
        }
        Ok(())
    }

    /// Check a variable declaration
    pub fn check_variable_decl(&mut self, decl: &parser::var::VariableDecl) -> Result<(), TypeSystemError> {
        let location = Span::new(0, decl.name.len());

        coffee_debug!("[DEBUG] check_variable_decl: checking variable '{}' with type '{}', value '{}'", decl.name, decl.var_type, decl.value);

        // Check if variable already exists (comprehensive mode only)
        if self.mode == CheckingMode::Comprehensive {
            if let Some(existing) = self.values.get(&decl.name) {
                if existing.state == ValueState::Alive {
                    let error = TypeSystemError::Duplicate {
                        name: decl.name.clone(),
                        existing: existing.location,
                        new: location,
                    };
                    self.add_error(error.clone());
                    return Err(error);
                }
            }
        }

        // Resolve type
        let ty = if let Some(fn_ty) = Self::coffee_fn_type_from_str(&decl.var_type) {
            fn_ty
        } else {
            let registry_result = self.registry.read();
            match registry_result {
                Ok(reg) => reg.resolve_type(&decl.var_type),
                Err(_) => {
                    drop(registry_result); // Explicitly drop the registry lock
                    let error = TypeSystemError::ParseError {
                        type_str: decl.var_type.clone(),
                        reason: "Failed to access registry".to_string(),
                    };
                    self.add_error(error.clone());
                    return Err(error);
                }
            }?
        };

        coffee_debug!("[DEBUG] check_variable_decl: resolved type to {:?}", ty);

        let value_type = match decl.value.kind() {
            crate::parser::expr::Expression::ArrayLiteral { elements }
                if elements.is_empty() =>
            {
                match &ty {
                    Type::Slice(elem) => Type::Slice(elem.clone()),
                    Type::Array { elem, size: 0 } => Type::Array {
                        elem: elem.clone(),
                        size: 0,
                    },
                    _ => match self.check_expression_hint(&decl.value, Some(&ty)) {
                        Ok(t) => t,
                        Err(e) => {
                            self.add_error(e.clone());
                            return Err(e);
                        }
                    },
                }
            }
            _ => match self.check_expression_hint(&decl.value, Some(&ty)) {
                Ok(t) => t,
                Err(e) => {
                    self.add_error(e.clone());
                    return Err(e);
                }
            },
        };
        coffee_debug!("[DEBUG] check_variable_decl: inferred value type as {:?}", value_type);
        if !self.types_compatible(&value_type, &ty)? && !self.numeric_literal_fits(&decl.value, &ty) {
            coffee_debug!("[DEBUG] check_variable_decl: types NOT compatible: {:?} vs {:?}", value_type, ty);
            let error = TypeSystemError::TypeMismatch {
                expected: ty.clone(),
                found: value_type.clone(),
                span: Span::new(0, decl.value.to_string().len()),
            };
            coffee_debug!("[DEBUG] check_variable_decl: creating TypeMismatch error: expected {:?}, found {:?}", ty, value_type);
            self.add_error(error.clone());
            return Err(error);
        }

        self.borrow.note_binding(&decl.name);
        self.claim_ref_binding(&decl.name, &decl.value)?;

        if self.ty_is_owned_resource(&ty) {
            if let Some(src) = crate::types::last_use::simple_var_name(&decl.value) {
                if self.is_last_use(src) {
                    self.implicit_move_source(src)?;
                } else {
                    let error = TypeSystemError::OwnershipError {
                        reason: format!(
                            "cannot copy resource '{}' with let; use mv or clone",
                            decl.name
                        ),
                        span: location,
                    };
                    self.add_error(error.clone());
                    return Err(error);
                }
            }
        }

        if self.mode == CheckingMode::Comprehensive {
            if let crate::parser::expr::Expression::Literal(lit) = decl.value.kind() {
                self.check_value_bounds(&ty, lit, location)?;
            }
        }

        // Track the value
        if self.mode == CheckingMode::Comprehensive {
            self.values.insert(decl.name.clone(), ValueInfo {
                ty,
                state: ValueState::Alive,
                location,
            });
            if self.current_return_type.is_none() {
                self.module_value_names.insert(decl.name.clone());
            }
        }

        Ok(())
    }
    pub(super) fn check_assignment(
        &mut self,
        name: &str,
        value: &crate::parser::expr::Expression,
    ) -> Result<(), TypeSystemError> {
        let target_type = self.lookup_assignment_target(name)?;

        let value_type = match self.check_expression_hint(value, Some(&target_type)) {
            Ok(ty) => ty,
            Err(e) => {
                self.add_error(e.clone());
                return Err(e);
            }
        };

        if !self.types_compatible(&value_type, &target_type)? && !self.numeric_literal_fits(value, &target_type) {
            let error = TypeSystemError::type_mismatch(target_type, value_type, Span::new(0, name.len()));
            self.add_error(error.clone());
            return Err(error);
        }

        if let Some(root) = name.split('.').next() {
            if self.values.get(root).map(|v| matches!(v.ty, Type::Ref { .. })).unwrap_or(false) {
                self.borrow.drop_holder(root);
                self.claim_ref_binding(root, value)?;
            } else if !name.contains('.') {
                if let Err(e) = self.borrow.deny_mutate(name) {
                    self.add_error(e.clone());
                    return Err(e);
                }
            }
        }

        if self.ty_is_owned_resource(&target_type) {
            if let Some(src) = crate::types::last_use::simple_var_name(value) {
                if src != name && self.is_last_use(src) {
                    self.implicit_move_source(src)?;
                } else {
                    let error = TypeSystemError::OwnershipError {
                        reason: format!(
                            "cannot copy resource '{}' with '='; use mv or clone",
                            name
                        ),
                        span: Span::new(0, name.len()),
                    };
                    self.add_error(error.clone());
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub(super) fn is_last_use(&self, name: &str) -> bool {
        self.last_uses
            .contains(&(self.current_stmt_id, name.to_string()))
    }

    /// Transfer ownership like `mv` after a last-use copy site.
    pub(super) fn implicit_move_source(&mut self, name: &str) -> Result<(), TypeSystemError> {
        if let Err(e) = self.borrow.deny_move(name) {
            self.add_error(e.clone());
            return Err(e);
        }
        match self.check_memory_operation(name, "move") {
            Ok(()) => Ok(()),
            Err(e) => {
                self.add_error(e.clone());
                Err(e)
            }
        }
    }

    pub(super) fn consume_last_use_call_args(
        &mut self,
        param_types: &[Type],
        args: &[crate::parser::expr::Expression],
    ) -> Result<(), TypeSystemError> {
        let n = param_types.len().min(args.len());
        for i in 0..n {
            if !self.ty_is_owned_resource(&param_types[i]) {
                continue;
            }
            let Some(src) = crate::types::last_use::simple_var_name(&args[i]) else {
                continue;
            };
            if self.is_last_use(src) {
                self.implicit_move_source(src)?;
            }
        }
        Ok(())
    }
    pub fn check_class(&mut self, class: &parser::class::ClassDef) -> Result<(), TypeSystemError> {
        if !class.type_params.is_empty() {
            if let Ok(reg) = self.registry.write() {
                reg.register_class_template(class);
            }
            return Ok(());
        }
        if class.name == "Error" {
            let error = TypeSystemError::duplicate(
                "Error",
                Span::new(0, 5),
                Span::new(0, 5),
            );
            self.add_error(error.clone());
            return Err(error);
        }
        for field in &class.fields {
            if let Err(e) = self.resolve_type_str(&field.field_type) {
                return Err(e);
            }
        }
        let class_ty = Type::NamedType { name: class.name.clone() };
        let mut first_err = None;
        for method in &class.methods {
            let prev_return = self.current_return_type.clone();
            let prev_values = self.values.clone();
            if let Err(e) = self.set_function_return_type(&method.return_type) {
                self.add_error(e.clone());
                self.current_return_type = prev_return;
                if first_err.is_none() {
                    first_err = Some(e);
                }
                continue;
            }
            if method.parameters.iter().any(|p| p.name == "self") {
                self.bind_alive("self", class_ty.clone());
            }
            for param in &method.parameters {
                let ty = if param.name == "self" {
                    class_ty.clone()
                } else {
                    match self.resolve_param_type(&param.param_type) {
                        Ok(ty) => ty,
                        Err(e) => {
                            if first_err.is_none() {
                                first_err = Some(e);
                            }
                            continue;
                        }
                    }
                };
                self.bind_alive(&param.name, ty);
            }
            let prev_last_uses = std::mem::take(&mut self.last_uses);
            let prev_stmt_cursor = self.stmt_cursor;
            let prev_stmt_id = self.current_stmt_id;
            let prev_borrow = self.borrow.clone();
            self.last_uses = crate::types::last_use::analyze_stmts(&method.body);
            self.stmt_cursor = 0;
            self.borrow.set_params(
                method
                    .parameters
                    .iter()
                    .map(|p| p.name.clone()),
            );
            if let Err(e) = self.check_stmt_list(&method.body) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            self.last_uses = prev_last_uses;
            self.stmt_cursor = prev_stmt_cursor;
            self.current_stmt_id = prev_stmt_id;
            self.borrow = prev_borrow;
            self.current_return_type = prev_return;
            self.values = prev_values;
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    pub(super) fn check_enum_field_types(&mut self, enum_def: &parser::class::EnumDef) -> Result<(), TypeSystemError> {
        let mut seen_variants: HashSet<&str> = HashSet::new();
        for variant in &enum_def.variants {
            if !seen_variants.insert(variant.name.as_str()) {
                let error = TypeSystemError::duplicate(
                    variant.name.clone(),
                    Span::new(0, variant.name.len()),
                    Span::new(0, variant.name.len()),
                );
                self.add_error(error.clone());
                return Err(error);
            }
        }
        for variant in &enum_def.variants {
            for field in &variant.fields {
                let type_str = match field {
                    parser::class::VariantField::Type(ty) => ty.as_str(),
                    parser::class::VariantField::Named { field_type, .. } => field_type.as_str(),
                };
                let resolved = self.registry.read().ok().and_then(|reg| reg.resolve_type(type_str).ok());
                if resolved.is_none() {
                    let error = TypeSystemError::undefined_type(
                        type_str.to_string(),
                        Span::new(0, type_str.len()),
                    );
                    self.add_error(error.clone());
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_stmt_list(&mut self, statements: &[parser::Statement]) -> Result<(), TypeSystemError> {
        let mut first_err = None;
        for stmt in statements {
            if let Err(e) = self.check_statement(stmt) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            self.borrow.expire_temps();
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    pub(super) fn check_expr_stmt(&mut self, expr: &crate::parser::expr::Expression) -> Result<(), TypeSystemError> {
        match self.check_expression(expr) {
            Ok(_) => Ok(()),
            Err(e) => {
                self.add_error(e.clone());
                Err(e)
            }
        }
    }

    pub fn check_function(&mut self, func: &parser::Function) -> Result<(), TypeSystemError> {
        if !func.type_params.is_empty() {
            if let Ok(reg) = self.registry.write() {
                reg.register_fn_template(func);
            }
            return Ok(());
        }
        self.record_ref_return_alias(func);
        self.apply_function_check(func, true)
    }

    /// Re-apply param + body checking so `values` holds this function's locals.
    /// Does not restore `values`; caller must `unbind_function_locals`.
    /// Does not append diagnostics already recorded by a prior `check_function`.
    pub fn bind_function_locals(&mut self, func: &parser::Function) -> Result<(), TypeSystemError> {
        self.apply_function_check(func, false)
    }

    /// Restore `values` saved before `bind_function_locals`.
    pub fn unbind_function_locals(&mut self, saved: HashMap<String, ValueInfo>) {
        self.values = saved;
    }

    /// Snapshot of `values` for restore after HIR lowering.
    pub fn snapshot_values(&self) -> HashMap<String, ValueInfo> {
        self.values.clone()
    }

    fn apply_function_check(
        &mut self,
        func: &parser::Function,
        restore_values: bool,
    ) -> Result<(), TypeSystemError> {
        let prev_return = self.current_return_type.clone();
        let prev_values = self.values.clone();
        let err_len = self.errors.len();
        if let Err(e) = self.set_function_return_type(&func.return_type) {
            self.add_error(e.clone());
            if !restore_values {
                self.errors.truncate(err_len);
            }
            self.current_return_type = prev_return;
            return Err(e);
        }
        if let Err(e) = self.check_error_listener(func) {
            self.current_return_type = prev_return;
            if restore_values {
                self.values = prev_values;
            } else {
                self.errors.truncate(err_len);
            }
            return Err(e);
        }
        let prev_loop = self.loop_depth;
        self.loop_depth = 0;
        let prev_borrow = self.borrow.clone();
        let prev_last_uses = std::mem::take(&mut self.last_uses);
        let prev_stmt_cursor = self.stmt_cursor;
        let prev_stmt_id = self.current_stmt_id;
        self.last_uses = crate::types::last_use::analyze_function_body(func);
        self.stmt_cursor = 0;
        self.borrow.set_params(func.parameters.iter().map(|p| p.name.clone()));
        if func.is_c {
            if let Some(Type::Slice(_)) = &self.current_return_type {
                let error = TypeSystemError::ParseError {
                    type_str: func.return_type.clone(),
                    reason: "slices cannot be C parameters".to_string(),
                };
                self.add_error(error.clone());
                self.current_return_type = prev_return;
                self.loop_depth = prev_loop;
                self.borrow = prev_borrow;
                self.last_uses = prev_last_uses;
                self.stmt_cursor = prev_stmt_cursor;
                self.current_stmt_id = prev_stmt_id;
                if restore_values {
                    self.values = prev_values;
                } else {
                    self.errors.truncate(err_len);
                }
                return Err(error);
            }
        }
        for param in &func.parameters {
            let ty = match self.resolve_param_type_for(&param.param_type, func.is_c) {
                Ok(ty) => ty,
                Err(e) => {
                    self.current_return_type = prev_return;
                    self.values = prev_values;
                    self.loop_depth = prev_loop;
                    self.borrow = prev_borrow;
                    self.last_uses = prev_last_uses;
                    self.stmt_cursor = prev_stmt_cursor;
                    self.current_stmt_id = prev_stmt_id;
                    if !restore_values {
                        self.errors.truncate(err_len);
                    }
                    return Err(e);
                }
            };
            self.values.insert(param.name.clone(), ValueInfo {
                ty,
                state: ValueState::Alive,
                location: Span::new(0, param.name.len()),
            });
        }
        let result = match &func.body {
            parser::FunctionBody::Block(stmts) => self.check_stmt_list(stmts),
            parser::FunctionBody::Expression(expr) => {
                self.current_stmt_id = 0;
                self.check_expr_stmt(expr)
            }
            parser::FunctionBody::External => Ok(()),
        };
        self.current_return_type = prev_return;
        self.loop_depth = prev_loop;
        self.borrow = prev_borrow;
        self.last_uses = prev_last_uses;
        self.stmt_cursor = prev_stmt_cursor;
        self.current_stmt_id = prev_stmt_id;
        if restore_values {
            self.values = prev_values;
        }
        if !restore_values {
            self.errors.truncate(err_len);
        }
        result
    }

    /// `#on_err` names a declared `fn on_err(err: Error) => R` with R equal to `func`'s return.
    fn check_error_listener(&mut self, func: &parser::Function) -> Result<(), TypeSystemError> {
        let Some(handler_name) = func.error_handler.as_deref() else {
            return Ok(());
        };
        let span = Span::new(0, handler_name.len());
        if handler_name == func.name {
            let error = TypeSystemError::ConstraintViolation {
                constraint: "error listener".to_string(),
                reason: format!(
                    "error listener '{}' must have a different name from function '{}'",
                    handler_name, func.name
                ),
                span,
            };
            self.add_error(error.clone());
            return Err(error);
        }

        let handler_ty = match self.registry.write() {
            Ok(reg) => {
                reg.ensure_error_type();
                reg.get_alias(handler_name)
            }
            Err(_) => None,
        };

        let Some(handler_ty) = handler_ty else {
            let error = TypeSystemError::undefined_function(handler_name, span);
            self.add_error(error.clone());
            return Err(error);
        };

        let Type::Function {
            params,
            return_type,
        } = handler_ty
        else {
            let error = TypeSystemError::not_callable(handler_ty, span);
            self.add_error(error.clone());
            return Err(error);
        };

        if params.len() != 1 {
            let error = TypeSystemError::arity_mismatch(1, params.len(), span);
            self.add_error(error.clone());
            return Err(error);
        }

        let expected_err = Type::NamedType {
            name: "Error".to_string(),
        };
        if !matches!(&params[0], Type::NamedType { name } if name == "Error")
            && !self.types_compatible(&params[0], &expected_err)?
        {
            let error = TypeSystemError::type_mismatch(expected_err, params[0].clone(), span);
            self.add_error(error.clone());
            return Err(error);
        }

        let expected_ret = self.current_return_type.clone().ok_or_else(|| {
            TypeSystemError::internal("error listener checked without a function return type")
        })?;
        if *return_type != expected_ret && !self.types_compatible(&return_type, &expected_ret)? {
            let error = TypeSystemError::type_mismatch(expected_ret, *return_type, span);
            self.add_error(error.clone());
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn resolve_type_str(&mut self, type_str: &str) -> Result<Type, TypeSystemError> {
        let resolved = match self.registry.read() {
            Ok(reg) => reg.resolve_type(type_str).ok(),
            Err(_) => None,
        };
        match resolved {
            Some(ty) => Ok(ty),
            None => {
                let error = TypeSystemError::undefined_type(
                    type_str.to_string(),
                    Span::new(0, type_str.len()),
                );
                self.add_error(error.clone());
                Err(error)
            }
        }
    }

    pub(super) fn resolve_param_type(&mut self, type_str: &str) -> Result<Type, TypeSystemError> {
        self.resolve_param_type_for(type_str, false)
    }

    pub(super) fn resolve_param_type_for(
        &mut self,
        type_str: &str,
        is_c: bool,
    ) -> Result<Type, TypeSystemError> {
        let ty = self.resolve_type_str(type_str)?;
        if is_c && matches!(ty, Type::Slice(_)) {
            let error = TypeSystemError::ParseError {
                type_str: type_str.to_string(),
                reason: "slices cannot be C parameters".to_string(),
            };
            self.add_error(error.clone());
            return Err(error);
        }
        Ok(ty)
    }

    pub(super) fn require_bool_condition(&mut self, expr: &crate::parser::expr::Expression) -> Result<(), TypeSystemError> {
        let ty = match self.check_expression(expr) {
            Ok(ty) => ty,
            Err(e) => {
                self.add_error(e.clone());
                return Err(e);
            }
        };
        if ty != Type::bool() {
            let error = TypeSystemError::type_mismatch(
                Type::bool(),
                ty,
                TypeSystemError::span_for_name("bool"),
            );
            self.add_error(error.clone());
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn require_int_expr(&mut self, expr: &crate::parser::expr::Expression) -> Result<(), TypeSystemError> {
        let ty = match self.check_expression(expr) {
            Ok(ty) => ty,
            Err(e) => {
                self.add_error(e.clone());
                return Err(e);
            }
        };
        if !ty.is_int() {
            let error = TypeSystemError::type_mismatch(
                Type::int(),
                ty,
                TypeSystemError::span_for_name("int"),
            );
            self.add_error(error.clone());
            return Err(error);
        }
        Ok(())
    }
    pub(super) fn check_scoped_stmts(
        &mut self,
        statements: &[parser::Statement],
    ) -> Result<(), TypeSystemError> {
        self.borrow.enter_scope();
        let result = self.check_stmt_list(statements);
        let names = self.borrow.exit_scope();
        for n in names {
            self.values.remove(&n);
        }
        result
    }

    pub(super) fn check_loop_control(&mut self, keyword: &str) -> Result<(), TypeSystemError> {
        if self.loop_depth == 0 {
            let error = TypeSystemError::ParseError {
                type_str: keyword.to_string(),
                reason: format!("{} used outside of a loop", keyword),
            };
            self.add_error(error.clone());
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn check_raise(&mut self, expr: &crate::parser::expr::Expression) -> Result<(), TypeSystemError> {
        match self.check_raise_expression(expr) {
            Ok(Type::NamedType { name }) if self.is_exception_type(&name) => Ok(()),
            Ok(ty) => {
                let error = TypeSystemError::type_mismatch(
                    Type::NamedType { name: "Error".to_string() },
                    ty,
                    TypeSystemError::span_for_name("Error"),
                );
                self.add_error(error.clone());
                Err(error)
            }
            Err(e) => {
                self.add_error(e.clone());
                Err(e)
            }
        }
    }

    pub(super) fn check_raise_expression(&mut self, expr: &crate::parser::expr::Expression) -> Result<Type, TypeSystemError> {
        match expr.kind() {
            crate::parser::expr::Expression::Call { function, args } => {
                if let crate::parser::expr::Expression::Variable(name) = function.kind() {
                    if self.is_exception_type(name) && self.is_registered_type(name) {
                        for arg in args {
                            self.check_expression(arg)?;
                        }
                        return Ok(Type::NamedType { name: name.clone() });
                    }
                }
                self.check_expression(expr)
            }
            crate::parser::expr::Expression::StructLiteral { struct_name, fields } => {
                if self.is_exception_type(struct_name) && self.is_registered_type(struct_name) {
                    return self.check_struct_literal(struct_name, fields);
                }
                self.check_expression(expr)
            }
            _ => self.check_expression(expr),
        }
    }

    pub(super) fn is_exception_type(&self, name: &str) -> bool {
        self.registry
            .read()
            .ok()
            .is_some_and(|reg| reg.is_exception_type(name))
    }

    pub(super) fn is_registered_type(&self, name: &str) -> bool {
        self.registry.read().ok().and_then(|reg| reg.get_type(name)).is_some()
    }

    pub(super) fn check_if(&mut self, if_expr: &parser::IfExpr) -> Result<(), TypeSystemError> {
        self.require_bool_condition(&if_expr.condition)?;
        self.check_scoped_stmts(&if_expr.body)?;
        for elif in &if_expr.elifs {
            self.require_bool_condition(&elif.condition)?;
            self.check_scoped_stmts(&elif.body)?;
        }
        if let Some(else_body) = &if_expr.else_body {
            self.check_scoped_stmts(else_body)?;
        }
        Ok(())
    }

    pub(super) fn check_while(&mut self, while_loop: &parser::WhileLoop) -> Result<(), TypeSystemError> {
        self.require_bool_condition(&while_loop.condition)?;
        self.loop_depth += 1;
        let result = self.check_scoped_stmts(&while_loop.body);
        self.loop_depth -= 1;
        result
    }

    pub(super) fn check_for(&mut self, for_loop: &parser::ForLoop) -> Result<(), TypeSystemError> {
        let loop_ty = match &for_loop.iterator {
            parser::ForIterator::Range { start, end } => {
                self.require_int_expr(start)?;
                self.require_int_expr(end)?;
                Type::int()
            }
            parser::ForIterator::Collection(expr) => {
                let col_ty = match self.check_expression(expr) {
                    Ok(ty) => ty,
                    Err(e) => {
                        self.add_error(e.clone());
                        return Err(e);
                    }
                };
                match col_ty {
                    Type::Array { elem, .. } => *elem,
                    Type::Slice(elem) => *elem,
                    Type::Tuple(elems) => {
                        let Some(first) = elems.first() else {
                            let error = TypeSystemError::ParseError {
                                type_str: "()".to_string(),
                                reason: "cannot infer element type of empty tuple".to_string(),
                            };
                            self.add_error(error.clone());
                            return Err(error);
                        };
                        for elem in elems.iter().skip(1) {
                            if elem != first {
                                let error = TypeSystemError::type_mismatch(
                                    first.clone(),
                                    elem.clone(),
                                    TypeSystemError::span_for_name(&elem.to_string()),
                                );
                                self.add_error(error.clone());
                                return Err(error);
                            }
                        }
                        first.clone()
                    }
                    other => {
                        let error = TypeSystemError::type_mismatch(
                            Type::NamedType {
                                name: "collection".to_string(),
                            },
                            other.clone(),
                            TypeSystemError::span_for_name(&other.to_string()),
                        );
                        self.add_error(error.clone());
                        return Err(error);
                    }
                }
            }
        };
        let prev = self.values.remove(&for_loop.variable);
        self.values.insert(for_loop.variable.clone(), ValueInfo {
            ty: loop_ty,
            state: ValueState::Alive,
            location: Span::new(0, for_loop.variable.len()),
        });
        self.loop_depth += 1;
        let result = self.check_scoped_stmts(&for_loop.body);
        self.loop_depth -= 1;
        match prev {
            Some(info) => {
                self.values.insert(for_loop.variable.clone(), info);
            }
            None => {
                self.values.remove(&for_loop.variable);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::TypeChecker;
    use crate::parser::expr::{parse_expression, Expression};
    use crate::parser::var::VariableDecl;
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    #[test]
    fn check_variable_decl_bounds_peels_parsed_literal() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        // Default int bounds are i64; tighten so 999 is out of range once peeled.
        checker.bounds.int_max = 100;

        let value = parse_expression("999").expect("parse 999");
        assert!(
            matches!(value, Expression::Spanned { .. }),
            "parser wraps literals in Spanned; matching Literal on the outer node skips bounds"
        );

        let decl = VariableDecl {
            name: "x".to_string(),
            var_type: "int(1)+".to_string(),
            value,
        };

        let err = checker
            .check_variable_decl(&decl)
            .expect_err("999 does not fit signed 1-byte int or tightened bounds");
        match err {
            TypeSystemError::ConstraintViolation { constraint, .. } => {
                assert_eq!(constraint, "integer bounds");
            }
            other => panic!("expected integer bounds ConstraintViolation, got {other:?}"),
        }
    }
}
