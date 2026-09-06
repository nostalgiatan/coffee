use super::*;
use super::super::definition::Type;
use crate::types::errors::TypeSystemError;

mod addr;
mod lookups;
mod ops;
mod struct_lit;

fn type_of_simple_literal(value: &str) -> Option<Type> {
    let value = value.trim();
    if value == "true" || value == "false" {
        return Some(Type::bool());
    }
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return Some(Type::string());
    }
    if value.parse::<i64>().is_ok() {
        return Some(Type::int());
    }
    if value.parse::<f64>().is_ok() {
        return Some(Type::float());
    }
    None
}

impl super::TypeChecker {
    pub fn check_expression_hint(
        &mut self,
        expr: &crate::parser::expr::Expression,
        hint: Option<&Type>,
    ) -> Result<Type, TypeSystemError> {
        use crate::parser::expr::Expression;
        match expr.kind() {
            Expression::StructLiteral { struct_name, fields } => {
                let name = self.monomorphize_class_name(struct_name, hint)?;
                self.check_struct_literal(&name, fields)
            }
            Expression::ConstructorCall { class_name, args } => {
                let name = self.monomorphize_class_name(class_name, hint)?;
                self.check_constructor_call(&name, args)
            }
            _ => self.check_expression(expr),
        }
    }

    fn monomorphize_class_name(
        &self,
        name: &str,
        hint: Option<&Type>,
    ) -> Result<String, TypeSystemError> {
        let registry = self.registry.read().map_err(|_| {
            TypeSystemError::internal("type registry lock poisoned")
        })?;
        if !registry.is_generic_class(name) {
            return Ok(name.to_string());
        }
        if let Some(Type::NamedType { name: concrete }) = hint {
            if registry.template_name_of(concrete).as_deref() == Some(name) {
                return Ok(concrete.clone());
            }
        }
        Err(TypeSystemError::InstantiationError {
            type_name: name.to_string(),
            args: vec![],
            reason: format!("type '{name}' used without type arguments"),
            span: TypeSystemError::span_for_name(name),
        })
    }

    /// Check an expression
    pub fn check_expression(&mut self, expr: &crate::parser::expr::Expression) -> Result<Type, TypeSystemError> {
        use crate::parser::expr::Expression;
        let expr = expr.kind();
        match expr {
            Expression::Literal(value) => {
                match type_of_simple_literal(value) {
                    Some(ty) => {
                        if self.mode == CheckingMode::Comprehensive {
                            self.check_value_bounds(&ty, value, Span::new(0, value.len()))?;
                        }
                        Ok(ty)
                    }
                    None => Err(TypeSystemError::ParseError {
                        type_str: value.clone(),
                        reason: format!("Invalid literal: {}", value),
                    }),
                }
            },
            Expression::Variable(name) => {
                self.lookup_live_variable(name)
            },
            Expression::Binary { left, op, right } => {
                let left_type = self.check_expression(left)?;
                let right_type = self.check_expression(right)?;
                self.check_binary_operation(&left_type, op, &right_type)
            },
            Expression::Call { function, args } => {
                if let Expression::Variable(name) = function.kind() {
                    let pending_generic = self.registry.read().ok().map(|reg| {
                        reg.is_generic_fn(name) && reg.get_alias(name).is_none()
                    }).unwrap_or(false);
                    if pending_generic {
                        return self.check_generic_fn_call(name, args);
                    }
                }
                let function_name = match function.kind() {
                    Expression::Variable(name) => {
                        match self.lookup_live_variable(name) {
                            Ok(Type::Function { params, return_type }) => {
                                self.check_call_arguments(&params, args, false, name)?;
                                self.extend_arg_loans_if_ref_return(&return_type, None, args);
                                return Ok(*return_type);
                            }
                            Err(e) if self.capture_forbidden.contains(name) => {
                                return Err(e);
                            }
                            _ => name.clone(),
                        }
                    }
                    _ => {
                        match self.check_expression(function) {
                            Ok(Type::Function { params, return_type }) => {
                                self.check_call_arguments(&params, args, false, "fn")?;
                                self.extend_arg_loans_if_ref_return(&return_type, None, args);
                                return Ok(*return_type);
                            }
                            Ok(ty) => {
                                return Err(TypeSystemError::not_callable(
                                    ty.clone(),
                                    TypeSystemError::span_for_name(&ty.to_string()),
                                ));
                            }
                            Err(e) => return Err(e),
                        }
                    }
                };
                self.check_function_call(&function_name, args)
            },
            Expression::Member { object, field, args } => {
                if let Expression::Variable(type_name) = object.kind() {
                    let is_enum = self.registry.read().ok().and_then(|reg| {
                        match reg.get_type(type_name) {
                            Some(TypeDef::Enum { .. }) => Some(()),
                            _ => None,
                        }
                    }).is_some();
                    if is_enum {
                        if !self.enum_has_variant(type_name, field) {
                            return Err(TypeSystemError::VariantNotFound {
                                enum_name: type_name.clone(),
                                variant_name: field.clone(),
                                span: Span::new(0, field.len()),
                                similar: Vec::new(),
                            });
                        }
                        let payload_tys = self.variant_payload_types(type_name, field)?;
                        if args.len() != payload_tys.len() {
                            let error = TypeSystemError::arity_mismatch(
                                payload_tys.len(),
                                args.len(),
                                TypeSystemError::span_for_name(field),
                            );
                            self.add_error(error.clone());
                            return Err(error);
                        }
                        for (arg, expected) in args.iter().zip(payload_tys.iter()) {
                            let arg_type = self.check_expression(arg)?;
                            if !self.types_compatible(&arg_type, expected)? {
                                let error = TypeSystemError::type_mismatch(
                                    expected.clone(),
                                    arg_type,
                                    TypeSystemError::span_for_name(field),
                                );
                                self.add_error(error.clone());
                                return Err(error);
                            }
                        }
                        return Ok(Type::NamedType { name: type_name.clone() });
                    }
                }

                if field == "assign" {
                    return self.check_assign_call(object, args);
                }

                let object_type = self.check_expression(object)?;
                let class_name = match &object_type {
                    Type::NamedType { name } => name.clone(),
                    _ => {
                        let op = format!(".{}", field);
                        return Err(TypeSystemError::invalid_operation(
                            op.clone(),
                            object_type.clone(),
                            object_type,
                            TypeSystemError::span_for_name(&op),
                        ))
                    }
                };

                if !args.is_empty() {
                    return self.check_method_call(&class_name, field, args);
                }

                if self.lookup_method(&class_name, field).is_some() {
                    return self.check_method_call(&class_name, field, args);
                }

                self.lookup_field_type(&class_name, field)
            },
            Expression::StructLiteral { struct_name, fields } => {
                self.check_struct_literal(struct_name, fields)
            },
            Expression::Unary { op, operand } => {
                match op.as_str() {
                    "&" => self.check_addr_of(operand, false),
                    "&mut" => self.check_addr_of(operand, true),
                    "*" => {
                        let operand_type = self.check_expression(operand)?;
                        match operand_type {
                            Type::Ref { elem, .. } => Ok(*elem),
                            other => Err(TypeSystemError::invalid_operation(
                                "*",
                                other,
                                Type::NamedType { name: "reference".into() },
                                TypeSystemError::span_for_name("*"),
                            )),
                        }
                    }
                    _ => {
                        let operand_type = self.check_expression(operand)?;
                        self.check_unary_operation(op, &operand_type)
                    }
                }
            },
            Expression::FString { template: _, placeholders } => {
                // Validate that all placeholders exist in current scope
                for placeholder in placeholders {
                    if !self.values.contains_key(placeholder) {
                        return Err(self.undefined_var_with_similar(placeholder));
                    }

                    // Validate that the placeholder has a safe-to-format type
                    let value_info = &self.values[placeholder];
                    match &value_info.ty {
                        Type::Int { .. } | Type::Float { .. } | Type::Bool | Type::String => {
                            // These types are safe for string formatting
                        }
                        _ => {
                            return Err(TypeSystemError::ParseError {
                                type_str: format!("f-string placeholder '{}'", placeholder),
                                reason: format!("type {:?} cannot be formatted into string", value_info.ty),
                            });
                        }
                    }
                }
                Ok(Type::string())
            },
            Expression::ConstructorCall { class_name, args } => {
                self.check_constructor_call(class_name, args)
            },
            Expression::ArrayLiteral { elements } => {
                // Check all elements have the same type
                if elements.is_empty() {
                    return Err(TypeSystemError::ParseError {
                        type_str: "[]".to_string(),
                        reason: "cannot infer element type of empty array; annotate a slice such as [int]".to_string(),
                    });
                }

                // Infer type from first element
                let first_type = self.check_expression(&elements[0])?;

                // Check all other elements have the same type
                for element in &elements[1..] {
                    let elem_type = self.check_expression(element)?;
                    if elem_type != first_type {
                        return Err(TypeSystemError::TypeMismatch {
                            expected: first_type.clone(),
                            found: elem_type.clone(),
                            span: TypeSystemError::span_for_name(&elem_type.to_string()),
                        });
                    }
                }

                // Return array type
                Ok(Type::Array {
                    elem: Box::new(first_type),
                    size: elements.len(),
                })
            },
            Expression::TupleLiteral { elements } => {
                // Check all element expressions
                let mut elem_types = Vec::new();
                for element in elements {
                    let elem_type = self.check_expression(element)?;
                    elem_types.push(elem_type);
                }

                // Return tuple type
                Ok(Type::Tuple(elem_types))
            },
            Expression::Index { array, index } => {
                // Check array expression
                let array_type = self.check_expression(array)?;

                // Check index expression
                let index_type = self.check_expression(index)?;

                // Validate that index is an integer
                if !index_type.is_int() {
                    return Err(TypeSystemError::TypeMismatch {
                        expected: Type::int(),
                        found: index_type.clone(),
                        span: TypeSystemError::span_for_name(&index_type.to_string()),
                    });
                }

                // Return the element type of the array
                match array_type {
                    Type::Array { ref elem, .. } => Ok(*elem.clone()),
                    Type::Slice(ref elem) => Ok(*elem.clone()),
                    _ => Err(TypeSystemError::ParseError {
                        type_str: format!("array indexing"),
                        reason: format!("cannot index non-array type {:?}", array_type),
                    }),
                }
            },
            Expression::Assign { object, field_name, value } => {
                self.check_field_assign(object, field_name, value)
            },
            Expression::TypeCast { target_type, value } => {
                let value_type = self.check_expression(value)?;
                let resolved = match self.registry.read() {
                    Ok(reg) => reg.resolve_type(target_type).ok(),
                    Err(_) => None,
                };
                match resolved {
                    Some(ty) => {
                        if self.cast_allowed(&value_type, &ty) {
                            Ok(ty)
                        } else {
                            let error = TypeSystemError::type_mismatch(
                                ty,
                                value_type,
                                TypeSystemError::span_for_name(target_type),
                            );
                            self.add_error(error.clone());
                            Err(error)
                        }
                    }
                    None => Err(TypeSystemError::undefined_type(
                        target_type.clone(),
                        Span::new(0, target_type.len()),
                    )),
                }
            },
            Expression::AnonymousFunction { func } => {
                self.check_anonymous_function(func)
            },
            Expression::Spanned { .. } => unreachable!("kind() peels Spanned"),
        }
    }

    fn check_assign_call(
        &mut self,
        object: &crate::parser::expr::Expression,
        args: &[crate::parser::expr::Expression],
    ) -> Result<Type, TypeSystemError> {
        if args.len() != 2 {
            let error = TypeSystemError::arity_mismatch(
                2,
                args.len(),
                TypeSystemError::span_for_name("assign"),
            );
            self.add_error(error.clone());
            return Err(error);
        }
        let field_name = match args[0].kind() {
            crate::parser::expr::Expression::Literal(lit) => {
                let t = lit.trim();
                if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
                    t[1..t.len() - 1].to_string()
                } else {
                    let error = TypeSystemError::ParseError {
                        type_str: "assign".to_string(),
                        reason: "assign field name must be a string literal".to_string(),
                    };
                    self.add_error(error.clone());
                    return Err(error);
                }
            }
            _ => {
                let error = TypeSystemError::ParseError {
                    type_str: "assign".to_string(),
                    reason: "assign field name must be a string literal".to_string(),
                };
                self.add_error(error.clone());
                return Err(error);
            }
        };
        self.check_field_assign(object, &field_name, &args[1])
    }

    fn check_field_assign(
        &mut self,
        object: &crate::parser::expr::Expression,
        field_name: &str,
        value: &crate::parser::expr::Expression,
    ) -> Result<Type, TypeSystemError> {
        let object_type = self.check_expression(object)?;
        let class_name = match object_type {
            Type::NamedType { name } => name,
            other => {
                let op = format!(".{}", field_name);
                return Err(TypeSystemError::invalid_operation(
                    op.clone(),
                    other.clone(),
                    other,
                    TypeSystemError::span_for_name(&op),
                ));
            }
        };
        let field_type = self.lookup_field_type(&class_name, field_name)?;
        let value_type = self.check_expression(value)?;
        if !self.types_compatible(&value_type, &field_type)? {
            return Err(TypeSystemError::type_mismatch(
                field_type,
                value_type,
                TypeSystemError::span_for_name(field_name),
            ));
        }
        Ok(Type::void())
    }

    fn check_anonymous_function(
        &mut self,
        func: &crate::parser::function::Function,
    ) -> Result<Type, TypeSystemError> {
        let mut param_types = Vec::new();
        for param in &func.parameters {
            param_types.push(self.resolve_param_type_for(&param.param_type, func.is_c)?);
        }
        let return_type = self.resolve_type_str(&func.return_type)?;
        let fn_ty = Type::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
        };

        let prev_return = self.current_return_type.clone();
        let prev_values = self.values.clone();
        let prev_loop = self.loop_depth;
        let prev_borrow = self.borrow.clone();
        let prev_capture = self.capture_forbidden.clone();
        let prev_last_uses = std::mem::take(&mut self.last_uses);
        let prev_stmt_cursor = self.stmt_cursor;
        let prev_stmt_id = self.current_stmt_id;
        self.capture_forbidden.extend(
            prev_values
                .keys()
                .filter(|k| !self.module_value_names.contains(*k))
                .cloned(),
        );
        for param in &func.parameters {
            self.capture_forbidden.remove(&param.name);
        }
        self.values
            .retain(|k, _| self.module_value_names.contains(k));
        self.loop_depth = 0;
        self.borrow = BorrowCx::default();
        self.borrow.set_params(func.parameters.iter().map(|p| p.name.clone()));
        self.last_uses = crate::types::last_use::analyze_function_body(func);
        self.stmt_cursor = 0;
        self.current_return_type = Some(return_type);
        self.record_ref_return_alias(func);
        for (param, ty) in func.parameters.iter().zip(param_types.into_iter()) {
            self.values.insert(
                param.name.clone(),
                ValueInfo {
                    ty,
                    state: ValueState::Alive,
                    location: Span::new(0, param.name.len()),
                },
            );
        }
        let result = match &func.body {
            crate::parser::function::FunctionBody::Block(stmts) => self.check_stmt_list(stmts),
            crate::parser::function::FunctionBody::Expression(expr) => {
                self.current_stmt_id = 0;
                self.check_expr_stmt(expr)
            }
            crate::parser::function::FunctionBody::External => Ok(()),
        };
        self.current_return_type = prev_return;
        self.values = prev_values;
        self.loop_depth = prev_loop;
        self.borrow = prev_borrow;
        self.capture_forbidden = prev_capture;
        self.last_uses = prev_last_uses;
        self.stmt_cursor = prev_stmt_cursor;
        self.current_stmt_id = prev_stmt_id;
        result?;
        Ok(fn_ty)
    }
}

#[cfg(test)]
mod tests {
    use super::super::TypeChecker;
    use crate::parser::expr::Expression;
    use crate::types::definition::Type;
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    #[test]
    fn check_expression_parsed_int_literal_is_int() {
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let expr = crate::parser::expr::parse_expression("42").expect("parse 42");
        assert!(matches!(expr, Expression::Spanned { .. }));
        let ty = checker.check_expression(&expr).expect("Spanned literal must typecheck");
        assert_eq!(ty, Type::int());
    }

    #[test]
    fn deref_non_ref_expects_named_reference_type_and_star_span() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::simple(registry);
        let expr = Expression::Unary {
            op: "*".to_string(),
            operand: Box::new(Expression::Literal("1".to_string())),
        };
        let err = checker.check_expression(&expr).unwrap_err();
        match err {
            TypeSystemError::InvalidOperation {
                op,
                left,
                right,
                span,
            } => {
                assert_eq!(op, "*");
                assert_eq!(left, Type::int());
                assert_eq!(
                    right,
                    Type::NamedType {
                        name: "reference".into()
                    }
                );
                assert_eq!(span, TypeSystemError::span_for_name("*"));
            }
            other => panic!("expected InvalidOperation, got {other:?}"),
        }
    }

    #[test]
    fn member_on_int_clones_found_type_as_unused_operand() {
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let expr = Expression::Member {
            object: Box::new(Expression::Literal("1".to_string())),
            field: "x".to_string(),
            args: vec![],
        };
        let err = checker.check_expression(&expr).unwrap_err();
        match err {
            TypeSystemError::InvalidOperation { op, left, right, .. } => {
                assert_eq!(op, ".x");
                assert_eq!(left, Type::int());
                assert_eq!(right, Type::int());
            }
            other => panic!("expected InvalidOperation, got {other:?}"),
        }
    }

    #[test]
    fn field_assign_on_int_clones_found_type_as_unused_operand() {
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let expr = Expression::Assign {
            object: Box::new(Expression::Literal("1".to_string())),
            field_name: "x".to_string(),
            value: Box::new(Expression::Literal("2".to_string())),
        };
        let err = checker.check_expression(&expr).unwrap_err();
        match err {
            TypeSystemError::InvalidOperation { op, left, right, .. } => {
                assert_eq!(op, ".x");
                assert_eq!(left, Type::int());
                assert_eq!(right, Type::int());
            }
            other => panic!("expected InvalidOperation, got {other:?}"),
        }
    }
}
