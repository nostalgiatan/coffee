use super::SemanticAnalyzer;
use crate::types::definition::*;
use crate::types::errors::TypeSystemError;

fn similar_local_symbols(name: &str, declared: &[String]) -> Vec<String> {
    let cands: Vec<String> = declared
        .iter()
        .filter(|s| !s.starts_with('_'))
        .map(|s| s.rsplit("::").next().unwrap_or(s).to_string())
        .collect();
    crate::diagnostics::find_similar_names(name, &cands, 2, 3)
}

impl SemanticAnalyzer {
    fn infer_literal_type(lit: &str) -> Result<Type, TypeSystemError> {
        let lit = lit.trim();
        if lit == "true" || lit == "false" {
            return Ok(Type::bool());
        }
        if lit.len() >= 2 && lit.starts_with('"') && lit.ends_with('"') {
            return Ok(Type::string());
        }
        if lit.parse::<i64>().is_ok() {
            return Ok(Type::int());
        }
        if lit.parse::<f64>().is_ok() {
            return Ok(Type::float());
        }
        Err(TypeSystemError::ParseError {
            type_str: lit.to_string(),
            reason: "cannot infer type here; unknown literal".to_string(),
        })
    }

    fn lookup_variable_type(&self, name: &str) -> Result<Type, TypeSystemError> {
        let symbols = self.symbol_space.read()
            .map_err(|_| TypeSystemError::ParseError {
                type_str: name.to_string(),
                reason: "Failed to access symbol table".to_string(),
            })?;

        let binding = if let Some(b) = symbols.lookup(name) {
            Some(b)
        } else if let Some(ref func_name) = self.current_function_name {
            let qualified_name = format!("{}::{}", func_name, name);
            symbols.lookup(&qualified_name)
        } else {
            None
        };

        if let Some(binding) = binding {
            match &binding.entity {
                crate::types::definition::Entity::Variable { ty, .. } => Ok(ty.clone()),
                crate::types::definition::Entity::Function { .. } => {
                    Err(TypeSystemError::ParseError {
                        type_str: name.to_string(),
                        reason: "Cannot use function as value".to_string(),
                    })
                }
                _ => Err(TypeSystemError::ParseError {
                    type_str: name.to_string(),
                    reason: "cannot infer type here; type checking owns non-variable entities"
                        .to_string(),
                }),
            }
        } else {
            let similar = similar_local_symbols(name, &symbols.declared_symbols());
            Err(TypeSystemError::undefined_variable(
                name.to_string(),
                crate::types::definition::Span::new(0, name.len()),
            )
            .with_similar(similar))
        }
    }

    /// Thin literal/variable typing. Call/member/C-symbol typing lives in TypeChecker.
    pub(super) fn infer_expression_type(&self, expr: &crate::parser::expr::Expression) -> Result<Type, TypeSystemError> {
        use crate::parser::expr::Expression;

        let expr = expr.kind();
        match expr {
            Expression::Literal(lit) => Self::infer_literal_type(lit),
            Expression::Variable(name) => self.lookup_variable_type(name),
            Expression::Unary { op, operand } => {
                let inner = self.infer_expression_type(operand)?;
                match op.as_str() {
                    "&" => Ok(Type::Ref {
                        elem: Box::new(inner),
                        mutable: false,
                    }),
                    "&mut" => Ok(Type::Ref {
                        elem: Box::new(inner),
                        mutable: true,
                    }),
                    "*" => match inner {
                        Type::Ref { elem, .. } => Ok(*elem),
                        other => Ok(other),
                    },
                    _ => Ok(inner),
                }
            }
            Expression::FString { .. } => Ok(Type::string()),
            Expression::StructLiteral { struct_name, .. } => {
                Ok(Type::NamedType { name: struct_name.clone() })
            }
            Expression::ConstructorCall { class_name, .. } => {
                Ok(Type::NamedType { name: class_name.clone() })
            }
            Expression::TypeCast { target_type, .. } => match target_type.as_str() {
                "int" => Ok(Type::int()),
                "float" => Ok(Type::float()),
                "bool" => Ok(Type::bool()),
                _ => Err(TypeSystemError::ParseError {
                    type_str: target_type.clone(),
                    reason: "cannot infer type here; type checking owns TypeCast targets other than int/float/bool".to_string(),
                }),
            },
            Expression::ArrayLiteral { elements } => {
                if elements.is_empty() {
                    Err(TypeSystemError::ParseError {
                        type_str: "[]".to_string(),
                        reason: "cannot infer empty array element type; type checking owns this"
                            .to_string(),
                    })
                } else {
                    let first_type = self.infer_expression_type(&elements[0])?;
                    Ok(Type::Array { elem: Box::new(first_type), size: elements.len() })
                }
            }
            Expression::TupleLiteral { elements } => {
                let mut elem_types = Vec::new();
                for element in elements {
                    elem_types.push(self.infer_expression_type(element)?);
                }
                Ok(Type::Tuple(elem_types))
            }
            Expression::Assign { .. } => Ok(Type::void()),
            Expression::Binary { .. }
            | Expression::Call { .. }
            | Expression::Member { .. }
            | Expression::Index { .. } => Err(TypeSystemError::ParseError {
                type_str: "expr".to_string(),
                reason: "cannot infer type here; type checking owns Call/Member/Index/Binary"
                    .to_string(),
            }),
            Expression::AnonymousFunction { func } => {
                let mut params = Vec::new();
                for p in &func.parameters {
                    params.push(Type::from_str(&p.param_type).map_err(|reason| {
                        TypeSystemError::ParseError {
                            type_str: p.param_type.clone(),
                            reason,
                        }
                    })?);
                }
                let return_type = Type::from_str(&func.return_type).map_err(|reason| {
                    TypeSystemError::ParseError {
                        type_str: func.return_type.clone(),
                        reason,
                    }
                })?;
                Ok(Type::Function {
                    params,
                    return_type: Box::new(return_type),
                })
            }
            Expression::Spanned { .. } => unreachable!("kind() peels Spanned"),
        }
    }

    /// Analyze an expression and check for undefined symbols
    pub(super) fn analyze_expression(&self, expr: &crate::parser::expr::Expression) -> Result<(), TypeSystemError> {
        use crate::parser::expr::Expression;

        let expr = expr.kind();
        match expr {
            Expression::Literal(_) => Ok(()),
            Expression::Variable(name) => {
                // Check if it's an enum variant: Enum::Variant or Enum::Variant(args)
                if name.contains("::") {
                    let parts: Vec<&str> = name.split("::").collect();
                    if parts.len() == 2 {
                        let enum_name = parts[0].trim();
                        let variant_part = parts[1].trim();
                        
                        // Check if this is a variant with parameters
                        let variant_name = if variant_part.contains('(') {
                            let paren_pos = variant_part.find('(').unwrap();
                            &variant_part[..paren_pos]
                        } else {
                            variant_part
                        };
                        
                        // Try to resolve enum type
                        match self.type_registry.read() {
                            Ok(reg) => {
                                if let Some(TypeDef::Enum { variants, .. }) = reg.get_type(enum_name) {
                                    // Check if this variant exists
                                    if variants.iter().any(|v| v.name == variant_name) {
                                        // Enum variant exists - it's valid
                                        return Ok(());
                                    }
                                }
                            },
                            Err(_) => {}
                        }
                    }
                }
                
                // Check if it's a C library import - these are external symbols
                if self.is_c_import(name) {
                    return Ok(());
                }

                // Check if it's a command-line argument (arg1, arg2, etc.)
                if self.is_command_line_arg(name) {
                    return Ok(());
                }

                // Check if variable or function exists in symbol space
                let symbols = self.symbol_space.read()
                    .map_err(|_| TypeSystemError::ParseError {
                        type_str: name.clone(),
                        reason: "Failed to access symbol table".to_string(),
                    })?;

                // First, try to find the symbol directly
                if symbols.lookup(name).is_some() {
                    return Ok(());
                }

                // If not found, try to find it in the current function's scope
                // by checking for function::parameter pattern
                if let Some(ref func_name) = self.current_function_name {
                    let qualified_name = format!("{}::{}", func_name, name);
                    if symbols.lookup(&qualified_name).is_some() {
                        return Ok(());
                    }
                }

                let similar = similar_local_symbols(name, &symbols.declared_symbols());
                Err(TypeSystemError::undefined_variable(
                    name.clone(),
                    Span::new(0, name.len()),
                )
                .with_similar(similar))
            }
            Expression::FString { template: _, placeholders } => {
                // Check all placeholder variables
                for placeholder in placeholders {
                    self.analyze_expression(&Expression::Variable(placeholder.clone()))?;
                }
                Ok(())
            }
            Expression::StructLiteral { struct_name, fields } => {
                // Check that struct exists
                let symbols = self.symbol_space.read()
                    .map_err(|_| TypeSystemError::ParseError {
                        type_str: struct_name.clone(),
                        reason: "Failed to access symbol table".to_string(),
                    })?;

                if symbols.lookup(struct_name).is_none() {
                    return Err(TypeSystemError::ParseError {
                        type_str: struct_name.clone(),
                        reason: format!("undefined struct '{}'", struct_name),
                    });
                }

                // Check field expressions
                for (_, field_value) in fields {
                    self.analyze_expression(field_value)?;
                }
                Ok(())
            }
            Expression::Binary { left, op: _, right } => {
                self.analyze_expression(left)?;
                self.analyze_expression(right)
            }
            Expression::Unary { op: _, operand } => {
                self.analyze_expression(operand)
            }
            Expression::Call { function, args } => {
                if let Expression::Variable(func_name) = function.kind() {
                    if func_name.contains("::") {
                        let parts: Vec<&str> = func_name.split("::").collect();
                        if parts.len() == 2 {
                            let enum_name = parts[0].trim();
                            let variant_name = parts[1].trim();
                            let variant_name_clean = if let Some(paren_pos) = variant_name.find('(') {
                                &variant_name[..paren_pos]
                            } else {
                                variant_name
                            };
                            if let Ok(reg) = self.type_registry.read() {
                                if let Some(TypeDef::Enum { variants, .. }) = reg.get_type(enum_name) {
                                    if variants.iter().any(|v| v.name == variant_name_clean) {
                                        for arg in args {
                                            self.analyze_expression(arg)?;
                                        }
                                        return Ok(());
                                    }
                                }
                                if let Some(TypeDef::Class { methods, .. }) = reg.get_type(enum_name) {
                                    if methods.iter().any(|m| m.name == variant_name_clean) {
                                        for arg in args {
                                            self.analyze_expression(arg)?;
                                        }
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }

                self.analyze_expression(function)?;

                if let Expression::Variable(func_name) = function.kind() {
                    if self.is_c_import(func_name) {
                        if let Some(c_symbol) = self.get_c_symbol(func_name) {
                            if !c_symbol.is_variadic && args.len() != c_symbol.parameters.len() {
                                return Err(TypeSystemError::ArityMismatch {
                                    expected: c_symbol.parameters.len(),
                                    found: args.len(),
                                    span: TypeSystemError::span_for_name(func_name),
                                });
                            }
                            for (i, arg) in args.iter().enumerate() {
                                if c_symbol.is_variadic && i >= c_symbol.parameters.len() {
                                    self.analyze_expression(arg)?;
                                    continue;
                                }
                                self.analyze_expression(arg)?;
                                let Ok(arg_type) = self.infer_expression_type(arg) else {
                                    continue;
                                };
                                let param_type_str = &c_symbol.parameters[i].param_type;
                                let param_type = self.type_registry.read()
                                    .map_err(|_| TypeSystemError::ParseError {
                                        type_str: param_type_str.clone(),
                                        reason: "Failed to access type registry".to_string(),
                                    })?
                                    .resolve_type(param_type_str)?;
                                let newtype_involved = self
                                    .type_registry
                                    .read()
                                    .ok()
                                    .map(|reg| {
                                        reg.type_is_newtype(&param_type)
                                            || reg.type_is_newtype(&arg_type)
                                    })
                                    .unwrap_or(false);
                                let types_match = if newtype_involved {
                                    param_type == arg_type
                                } else {
                                    param_type.can_coerce_from(&arg_type)
                                        || matches!(
                                            (&arg_type, &param_type),
                                            (Type::Int { .. }, Type::Int { .. })
                                        )
                                };
                                if !types_match {
                                    return Err(TypeSystemError::TypeMismatch {
                                        expected: param_type,
                                        found: arg_type,
                                        span: TypeSystemError::span_for_name(func_name),
                                    });
                                }
                            }
                            return Ok(());
                        }
                    }

                    let symbols = self.symbol_space.read()
                        .map_err(|_| TypeSystemError::ParseError {
                            type_str: func_name.clone(),
                            reason: "Failed to access symbol table".to_string(),
                        })?;

                    let binding = if let Some(b) = symbols.lookup(func_name) {
                        Some(b)
                    } else if let Some(ref func) = self.current_function_name {
                        symbols.lookup(&format!("{}::{}", func, func_name))
                    } else {
                        None
                    };
                    if let Some(binding) = binding {
                        if let crate::types::definition::Entity::Variable {
                            ty: Type::Function { .. },
                            ..
                        } = &binding.entity
                        {
                            for arg in args {
                                self.analyze_expression(arg)?;
                            }
                            return Ok(());
                        }
                        if let crate::types::definition::Entity::Function { params, .. } = &binding.entity {
                            if args.len() != params.len() {
                                return Err(TypeSystemError::ArityMismatch {
                                    expected: params.len(),
                                    found: args.len(),
                                    span: TypeSystemError::span_for_name(func_name),
                                });
                            }
                            for (i, arg) in args.iter().enumerate() {
                                self.analyze_expression(arg)?;
                                let Ok(arg_type) = self.infer_expression_type(arg) else {
                                    continue;
                                };
                                let param_type = &params[i];
                                let types_match = match (&arg_type, param_type) {
                                    (Type::Int { .. }, Type::Int { .. }) => true,
                                    _ => arg_type == *param_type,
                                };
                                if !types_match {
                                    return Err(TypeSystemError::TypeMismatch {
                                        expected: param_type.clone(),
                                        found: arg_type,
                                        span: TypeSystemError::span_for_name(func_name),
                                    });
                                }
                            }
                            return Ok(());
                        }
                        let ty = match &binding.entity {
                            crate::types::definition::Entity::Variable { ty, .. } => ty.clone(),
                            _ => Type::NamedType {
                                name: func_name.clone(),
                            },
                        };
                        return Err(TypeSystemError::NotCallable {
                            ty,
                            span: TypeSystemError::span_for_name(func_name),
                        });
                    }
                    return Err(TypeSystemError::NotFound {
                        name: func_name.clone(),
                        kind: crate::types::definition::SpaceKind::Symbol,
                    });
                }

                for arg in args {
                    self.analyze_expression(arg)?;
                }
                Ok(())
            }
            Expression::Member { object, field: _, args } => {
                self.analyze_expression(object)?;
                
                // If args is not empty, this is a method call
                if !args.is_empty() {
                    for arg in args {
                        self.analyze_expression(arg)?;
                    }
                }
                Ok(())
            }
            Expression::ConstructorCall { class_name, args } => {
                // Check that class exists
                let symbols = self.symbol_space.read()
                    .map_err(|_| TypeSystemError::ParseError {
                        type_str: class_name.clone(),
                        reason: "Failed to access symbol table".to_string(),
                    })?;

                if symbols.lookup(class_name).is_none() {
                    return Err(TypeSystemError::ParseError {
                        type_str: class_name.clone(),
                        reason: format!("undefined class '{}'", class_name),
                    });
                }

                // Check argument expressions
                for arg in args {
                    self.analyze_expression(arg)?;
                }
                Ok(())
            }
            Expression::Index { array, index } => {
                self.analyze_expression(array)?;
                self.analyze_expression(index)
            }
            Expression::Assign { object, field_name: _, value } => {
                self.analyze_expression(object)?;
                self.analyze_expression(value)
            }
            Expression::ArrayLiteral { elements } => {
                // Check all element expressions
                for element in elements {
                    self.analyze_expression(element)?;
                }
                Ok(())
            }
            Expression::TupleLiteral { elements } => {
                // Check all element expressions
                for element in elements {
                    self.analyze_expression(element)?;
                }
                Ok(())
            }
            Expression::TypeCast { value, .. } => {
                // Check the value expression
                self.analyze_expression(value)
            }
            Expression::AnonymousFunction { .. } => Ok(()),
            Expression::Spanned { .. } => unreachable!("kind() peels Spanned"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::SemanticAnalyzer;
    use crate::parser::expr::Expression;
    use crate::parser::function::{Function, FunctionBody, Parameter};
    use crate::parser::MemoryOp;
    use crate::types::definition::*;
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    fn analyzer() -> SemanticAnalyzer {
        SemanticAnalyzer::new(Arc::new(RwLock::new(TypeRegistry::root())))
    }

    fn declare(analyzer: &SemanticAnalyzer, name: &str, entity: Entity) {
        let binding = Binding {
            name: name.to_string(),
            entity,
            span: Span::new(0, name.len()),
            mutable: true,
            visibility: Visibility::Private,
        };
        analyzer
            .symbol_space
            .write()
            .unwrap()
            .declare(name.to_string(), binding)
            .unwrap();
    }

    fn err_text(err: &TypeSystemError) -> String {
        err.to_string().to_lowercase()
    }

    fn assert_checker_owns(result: Result<Type, TypeSystemError>) {
        let err = result.expect_err("semantic must not invent a type");
        let msg = err_text(&err);
        assert!(
            msg.contains("type checking") || msg.contains("cannot infer"),
            "error should say type checking owns this / cannot infer here, got: {err}"
        );
    }

    #[test]
    fn infer_parsed_int_literal_is_int() {
        let a = analyzer();
        let expr = crate::parser::expr::parse_expression("42").expect("parse 42");
        assert!(matches!(expr, Expression::Spanned { .. }));
        assert_eq!(a.infer_expression_type(&expr).expect("Spanned literal"), Type::int());
    }

    #[test]
    fn infer_refuses_call_member_index_binary() {
        let a = analyzer();
        let lit = || Box::new(Expression::Literal("1".into()));
        assert_checker_owns(a.infer_expression_type(&Expression::Call {
            function: Box::new(Expression::Variable("foo".into())),
            args: vec![],
        }));
        assert_checker_owns(a.infer_expression_type(&Expression::Member {
            object: lit(),
            field: "x".into(),
            args: vec![],
        }));
        assert_checker_owns(a.infer_expression_type(&Expression::Index {
            array: Box::new(Expression::Variable("xs".into())),
            index: lit(),
        }));
        assert_checker_owns(a.infer_expression_type(&Expression::Binary {
            left: lit(),
            op: "+".into(),
            right: lit(),
        }));
    }

    #[test]
    fn infer_refuses_unknown_type_cast_target() {
        let a = analyzer();
        assert!(a
            .infer_expression_type(&Expression::TypeCast {
                target_type: "str".into(),
                value: Box::new(Expression::Literal("1".into())),
            })
            .is_err());
        assert_eq!(
            a.infer_expression_type(&Expression::TypeCast {
                target_type: "int".into(),
                value: Box::new(Expression::Literal("1".into())),
            })
            .unwrap(),
            Type::int()
        );
        assert_eq!(
            a.infer_expression_type(&Expression::TypeCast {
                target_type: "float".into(),
                value: Box::new(Expression::Literal("1".into())),
            })
            .unwrap(),
            Type::float()
        );
        assert_eq!(
            a.infer_expression_type(&Expression::TypeCast {
                target_type: "bool".into(),
                value: Box::new(Expression::Literal("1".into())),
            })
            .unwrap(),
            Type::bool()
        );
    }

    #[test]
    fn infer_refuses_unknown_entity_kind() {
        let a = analyzer();
        declare(
            &a,
            "Point",
            Entity::TypeDef {
                def: TypeDef::Class {
                    name: "Point".into(),
                    fields: vec![],
                    methods: vec![],
                    generics: vec![],
                    parent: None,
                },
            },
        );
        let err = a
            .infer_expression_type(&Expression::Variable("Point".into()))
            .expect_err("TypeDef must not become int");
        assert_ne!(format!("{err}"), "");
    }

    #[test]
    fn infer_refuses_empty_array_literal() {
        let a = analyzer();
        assert!(a
            .infer_expression_type(&Expression::ArrayLiteral { elements: vec![] })
            .is_err());
        assert_eq!(
            a.infer_expression_type(&Expression::ArrayLiteral {
                elements: vec![Expression::Literal("1".into())],
            })
            .unwrap(),
            Type::Array {
                elem: Box::new(Type::int()),
                size: 1
            }
        );
    }

    #[test]
    fn infer_refuses_anonymous_fn_when_from_str_fails() {
        let a = analyzer();
        let bad = Function {
            type_params: vec![],
            name: "anon".into(),
            parameters: vec![Parameter {
                name: "x".into(),
                param_type: "Foo<T>".into(),
                is_variadic: false,
            }],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::External,
            is_c: false,
        };
        assert!(a
            .infer_expression_type(&Expression::AnonymousFunction {
                func: Box::new(bad),
            })
            .is_err());

        let ok = Function {
            type_params: vec![],
            name: "anon".into(),
            parameters: vec![Parameter {
                name: "x".into(),
                param_type: "int".into(),
                is_variadic: false,
            }],
            return_type: "str".into(),
            error_handler: None,
            body: FunctionBody::External,
            is_c: false,
        };
        assert_eq!(
            a.infer_expression_type(&Expression::AnonymousFunction {
                func: Box::new(ok),
            })
            .unwrap(),
            Type::Function {
                params: vec![Type::int()],
                return_type: Box::new(Type::string()),
            }
        );
    }

    #[test]
    fn infer_keeps_literals_vars_tuples_fstring_named_assign() {
        let a = analyzer();
        declare(
            &a,
            "s",
            Entity::Variable {
                ty: Type::string(),
                initialized: true,
            },
        );
        assert_eq!(
            a.infer_expression_type(&Expression::Literal("true".into()))
                .unwrap(),
            Type::bool()
        );
        assert_eq!(
            a.infer_expression_type(&Expression::Variable("s".into()))
                .unwrap(),
            Type::string()
        );
        assert_eq!(
            a.infer_expression_type(&Expression::FString {
                template: "hi {s}".into(),
                placeholders: vec!["s".into()],
            })
            .unwrap(),
            Type::string()
        );
        assert_eq!(
            a.infer_expression_type(&Expression::TupleLiteral {
                elements: vec![
                    Expression::Literal("1".into()),
                    Expression::Literal("true".into())
                ],
            })
            .unwrap(),
            Type::Tuple(vec![Type::int(), Type::bool()])
        );
        assert_eq!(
            a.infer_expression_type(&Expression::StructLiteral {
                struct_name: "Point".into(),
                fields: vec![],
            })
            .unwrap(),
            Type::NamedType {
                name: "Point".into()
            }
        );
        assert_eq!(
            a.infer_expression_type(&Expression::ConstructorCall {
                class_name: "Point".into(),
                args: vec![],
            })
            .unwrap(),
            Type::NamedType {
                name: "Point".into()
            }
        );
        assert_eq!(
            a.infer_expression_type(&Expression::Assign {
                object: Box::new(Expression::Variable("self".into())),
                field_name: "x".into(),
                value: Box::new(Expression::Literal("1".into())),
            })
            .unwrap(),
            Type::void()
        );
    }

    #[test]
    fn analyze_call_still_reports_undefined_and_does_not_infer_int() {
        let a = analyzer();
        let call = Expression::Call {
            function: Box::new(Expression::Variable("missing".into())),
            args: vec![],
        };
        assert!(a.analyze_expression(&call).is_err());
        assert_checker_owns(a.infer_expression_type(&call));
    }

    #[test]
    fn clone_and_mv_propagate_infer_err_instead_of_defaulting_int() {
        let mut a = analyzer();
        declare(
            &a,
            "Point",
            Entity::TypeDef {
                def: TypeDef::Enum {
                    name: "Point".into(),
                    variants: vec![],
                    generics: vec![],
                },
            },
        );
        assert!(a
            .analyze_memory_op(&MemoryOp::Clone {
                source: "Point".into(),
                target: "p".into(),
            })
            .is_err());
        assert!(a
            .analyze_memory_op(&MemoryOp::Move {
                source: "Point".into(),
                target: "q".into(),
            })
            .is_err());
    }

    #[test]
    fn infer_unknown_literal_is_error_not_int() {
        let a = analyzer();
        let err = a
            .infer_expression_type(&Expression::Literal("not_a_literal".into()))
            .expect_err("unknown literal must not become int");
        assert!(
            !matches!(err, TypeSystemError::TypeMismatch { found: Type::Int { .. }, .. }),
            "unknown literal must not be reported as int, got: {err}"
        );
        let msg = err_text(&err);
        assert!(
            msg.contains("cannot infer") || msg.contains("unknown") || msg.contains("literal"),
            "error should reject the unknown literal, got: {err}"
        );
    }

    #[test]
    fn analyze_call_not_callable_uses_binding_type() {
        let a = analyzer();
        declare(
            &a,
            "x",
            Entity::Variable {
                ty: Type::string(),
                initialized: true,
            },
        );
        let err = a
            .analyze_expression(&Expression::Call {
                function: Box::new(Expression::Variable("x".into())),
                args: vec![],
            })
            .expect_err("calling a non-function binding must be NotCallable");
        match err {
            TypeSystemError::NotCallable { ty, .. } => {
                assert_eq!(ty, Type::string(), "must use the binding's type, not fake int");
            }
            other => panic!("expected NotCallable, got {other:?}"),
        }
    }

    #[test]
    fn analyze_call_not_callable_typedef_uses_named_type() {
        let a = analyzer();
        declare(
            &a,
            "Point",
            Entity::TypeDef {
                def: TypeDef::Class {
                    name: "Point".into(),
                    fields: vec![],
                    methods: vec![],
                    generics: vec![],
                    parent: None,
                },
            },
        );
        let err = a
            .analyze_expression(&Expression::Call {
                function: Box::new(Expression::Variable("Point".into())),
                args: vec![],
            })
            .expect_err("calling a type name must be NotCallable");
        match err {
            TypeSystemError::NotCallable { ty, .. } => {
                assert_eq!(
                    ty,
                    Type::NamedType {
                        name: "Point".into()
                    },
                    "must name the binding, not fake int"
                );
            }
            other => panic!("expected NotCallable, got {other:?}"),
        }
    }
}
