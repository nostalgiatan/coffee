use super::*;
use super::super::definition::{Type, TypeDef};
use crate::types::errors::TypeSystemError;

impl super::TypeChecker {
    pub(super) fn check_constructor_call(
        &mut self,
        class_name: &str,
        args: &[crate::parser::expr::Expression],
    ) -> Result<Type, TypeSystemError> {
        if self.lookup_method(class_name, "new").is_some() {
            return self.check_method_call(class_name, "new", args);
        }
        if self.lookup_method(class_name, class_name).is_some() {
            return self.check_method_call(class_name, class_name, args);
        }
        if args.is_empty() {
            Ok(Type::NamedType { name: class_name.to_string() })
        } else {
            let error = TypeSystemError::arity_mismatch(
                0,
                args.len(),
                TypeSystemError::span_for_name(class_name),
            );
            self.add_error(error.clone());
            Err(error)
        }
    }
    pub(super) fn check_call_arguments(
        &mut self,
        param_types: &[Type],
        args: &[crate::parser::expr::Expression],
        c_variadic: bool,
        callee: &str,
    ) -> Result<(), TypeSystemError> {
        let location = TypeSystemError::span_for_name(callee);
        let trailing_variadic = matches!(param_types.last(), Some(Type::Variadic));
        let variadic = c_variadic || trailing_variadic;
        let fixed_len = if trailing_variadic {
            param_types.len().saturating_sub(1)
        } else {
            param_types.len()
        };

        if variadic {
            if args.len() < fixed_len {
                let error = TypeSystemError::arity_mismatch(fixed_len, args.len(), location);
                self.add_error(error.clone());
                return Err(error);
            }
        } else if args.len() != param_types.len() {
            let error = TypeSystemError::arity_mismatch(param_types.len(), args.len(), location);
            self.add_error(error.clone());
            return Err(error);
        }

        let typed_count = if variadic { fixed_len } else { param_types.len() };
        for i in 0..typed_count {
            let expected_type = &param_types[i];
            let arg_type = self.check_expression_hint(&args[i], Some(&expected_type))?;
            if !self.types_compatible(&arg_type, expected_type)? {
                let error = TypeSystemError::type_mismatch(
                    expected_type.clone(),
                    arg_type,
                    TypeSystemError::span_for_name(&args[i].to_string()),
                );
                self.add_error(error.clone());
                return Err(error);
            }
        }
        for arg in args.iter().skip(typed_count) {
            let arg_type = self.check_expression(arg)?;
            if c_variadic && !arg_type.is_c_handle_value() {
                let error = TypeSystemError::type_mismatch(
                    Type::Variadic,
                    arg_type,
                    TypeSystemError::span_for_name(&arg.to_string()),
                );
                self.add_error(error.clone());
                return Err(error);
            }
        }
        self.consume_last_use_call_args(param_types, args)?;
        Ok(())
    }

    pub(super) fn check_generic_fn_call(
        &mut self,
        name: &str,
        args: &[crate::parser::expr::Expression],
    ) -> Result<Type, TypeSystemError> {
        let location = TypeSystemError::span_for_name(name);
        if let Some(expected) = self.registry.read().ok().and_then(|r| r.generic_fn_value_arity(name)) {
            if args.len() != expected {
                let error = TypeSystemError::arity_mismatch(expected, args.len(), location);
                self.add_error(error.clone());
                return Err(error);
            }
        }
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.check_expression(arg)?);
        }
        let function_type = {
            let registry = self.registry.read().map_err(|_| TypeSystemError::ParseError {
                type_str: name.to_string(),
                reason: "Failed to access type registry".to_string(),
            })?;
            registry.infer_fn_instance(name, &arg_types)?
        };
        let (param_types, return_type) = match function_type {
            Type::Function { params, return_type } => (params, *return_type),
            other => {
                let error = TypeSystemError::not_callable(other, location);
                self.add_error(error.clone());
                return Err(error);
            }
        };
        for (i, expected) in param_types.iter().enumerate() {
            if i >= arg_types.len() {
                break;
            }
            if !self.types_compatible(&arg_types[i], expected)? {
                let error = TypeSystemError::type_mismatch(
                    expected.clone(),
                    arg_types[i].clone(),
                    TypeSystemError::span_for_name(&args[i].to_string()),
                );
                self.add_error(error.clone());
                return Err(error);
            }
        }
        self.consume_last_use_call_args(&param_types, args)?;
        self.extend_arg_loans_if_ref_return(&return_type, Some(name), args);
        Ok(return_type)
    }

    pub(super) fn check_method_call(
        &mut self,
        class_name: &str,
        method_name: &str,
        args: &[crate::parser::expr::Expression],
    ) -> Result<Type, TypeSystemError> {
        let method = self.lookup_method(class_name, method_name).ok_or_else(|| {
            TypeSystemError::MethodNotFound {
                type_name: class_name.to_string(),
                method_name: method_name.to_string(),
                span: TypeSystemError::span_for_name(method_name),
                similar: Vec::new(),
            }
        })?;

        let param_types: Vec<Type> = match method.params.first() {
            Some(Type::NamedType { name }) if name == class_name => method.params[1..].to_vec(),
            _ => method.params.clone(),
        };

        if args.len() != param_types.len() {
            let error = TypeSystemError::arity_mismatch(
                param_types.len(),
                args.len(),
                TypeSystemError::span_for_name(method_name),
            );
            self.add_error(error.clone());
            return Err(error);
        }

        for (arg, expected) in args.iter().zip(param_types.iter()) {
            let arg_type = self.check_expression(arg)?;
            if !self.types_compatible(&arg_type, expected)? {
                let error = TypeSystemError::type_mismatch(
                    expected.clone(),
                    arg_type,
                    TypeSystemError::span_for_name(method_name),
                );
                self.add_error(error.clone());
                return Err(error);
            }
        }

        self.consume_last_use_call_args(&param_types, args)?;
        self.extend_arg_loans_if_ref_return(&method.return_type, None, args);
        Ok(method.return_type)
    }

    pub(super) fn extend_arg_loans_if_ref_return(
        &mut self,
        return_type: &Type,
        callee: Option<&str>,
        args: &[crate::parser::expr::Expression],
    ) {
        if !matches!(return_type, Type::Ref { .. }) {
            return;
        }
        if let Some(name) = callee {
            if let Some(idx) = self.ref_return_alias.get(name).copied() {
                if let Some(arg) = args.get(idx) {
                    if let Some(place) = arg_borrow_place(arg) {
                        self.borrow.pin_unheld_temps_of(&place);
                        return;
                    }
                }
            }
        }
        self.borrow.pin_unheld_temps();
    }

    /// Check function call
    pub fn check_function_call(
        &mut self,
        name: &str,
        args: &[crate::parser::expr::Expression],
    ) -> Result<Type, TypeSystemError> {
        let location = Span::new(0, name.len());

        if matches!(name, "malloc" | "free" | "calloc" | "realloc") {
            let imported = self.analyzer.as_ref().and_then(|a| a.read().ok())
                .map(|a| a.is_explicit_c_import(name))
                .unwrap_or(false);
            if !imported {
                let error = TypeSystemError::ParseError {
                    type_str: name.to_string(),
                    reason: format!(
                        "{} is not a Coffee memory primitive; import it with 'use {} in libc of c'",
                        name, name
                    ),
                };
                self.add_error(error.clone());
                return Err(error);
            }
        }

        // Check if this is an enum variant: Enum::Variant(args)
        if name.contains("::") {
            let parts: Vec<&str> = name.split("::").collect();
            if parts.len() == 2 {
                let enum_name = parts[0].trim().to_string();
                let variant_name = parts[1].trim();
                
                // Check if this is a variant with parameters
                let variant_name_clean = if variant_name.contains('(') {
                    let paren_pos = variant_name.find('(').unwrap();
                    &variant_name[..paren_pos]
                } else {
                    variant_name
                };
                let variant_name_clean = variant_name_clean.to_string();
                
                // Try to resolve enum type
                let variant_info = {
                    let registry_result = self.registry.read();
                    match registry_result {
                        Ok(reg) => {
                            if let Some(TypeDef::Enum { variants, .. }) = reg.get_type(&enum_name) {
                                // Check if this variant exists
                                if let Some(variant) = variants.iter().find(|v| v.name == variant_name_clean) {
                                    Some((enum_name.clone(), variant.clone()))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        },
                        Err(_) => None
                    }
                };
                
                if let Some((enum_name, variant)) = variant_info {
                    // Check argument count
                    if args.len() != variant.fields.len() {
                        let error = TypeSystemError::arity_mismatch(variant.fields.len(), args.len(), location);
                        self.add_error(error.clone());
                        return Err(error);
                    }
                    
                    let payload_types = {
                        let Ok(reg) = self.registry.read() else {
                            let error = TypeSystemError::ParseError {
                                type_str: name.to_string(),
                                reason: "Failed to access registry".to_string(),
                            };
                            self.add_error(error.clone());
                            return Err(error);
                        };
                        let mut payload_types = Vec::with_capacity(variant.fields.len());
                        for field in &variant.fields {
                            let type_str = match field {
                                crate::types::definition::VariantField::Positional(t) => t.as_str(),
                                crate::types::definition::VariantField::Named { ty, .. } => ty.as_str(),
                            };
                            match reg.resolve_type(type_str) {
                                Ok(ty) => payload_types.push(ty),
                                Err(e) => {
                                    drop(reg);
                                    self.add_error(e.clone());
                                    return Err(e);
                                }
                            }
                        }
                        payload_types
                    };

                    for (i, (arg, expected_type)) in args.iter().zip(payload_types.iter()).enumerate() {
                        let arg_type = self.check_expression(arg)?;
                        if !self.types_compatible(&arg_type, expected_type)? {
                            let arg_location = Span::new(location.start + i * 10, location.start + (i + 1) * 10);
                            let error = TypeSystemError::type_mismatch(expected_type.clone(), arg_type, arg_location);
                            self.add_error(error.clone());
                            return Err(error);
                        }
                    }
                    
                    // Return enum type
                    return Ok(Type::NamedType { name: enum_name });
                }
                if self.lookup_method(&enum_name, &variant_name_clean).is_some() {
                    return self.check_method_call(&enum_name, &variant_name_clean, args);
                }
            }
        }
        
        // Get function type from registry or symbol space
        let function_type = {
            let registry_result = self.registry.read();
            match registry_result {
                Ok(reg) => {
                    // Check if it's a defined function in registry
                    if let Ok(func_type) = reg.resolve_type(name) {
                        Ok(func_type)
                    } else {
                        Err("not_found")
                    }
                }
                Err(_) => Err("registry_error")
            }
        };

        let mut c_variadic = false;
        let function_type = match function_type {
            Ok(ty) => ty,
            Err("not_found") => {
                let mut c_sig: Option<(Vec<String>, String, bool)> = None;
                if let Some(ref analyzer) = self.analyzer {
                    if let Ok(analyzer) = analyzer.read() {
                        if let Ok(cfc_symbols) = analyzer.get_cfc_symbols() {
                            for (_lib_name, symbol_table) in cfc_symbols.iter() {
                                if let Some(c_symbol) = symbol_table.symbols.get(name) {
                                    c_sig = Some((
                                        c_symbol
                                            .parameters
                                            .iter()
                                            .map(|p| p.param_type.clone())
                                            .collect(),
                                        c_symbol.return_type.clone(),
                                        c_symbol.is_variadic,
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
                if let Some((param_strs, ret_str, variadic)) = c_sig {
                    let lock = self.registry.read();
                    if lock.is_err() {
                        drop(lock);
                        let error = TypeSystemError::ParseError {
                            type_str: name.to_string(),
                            reason: "Failed to access registry".to_string(),
                        };
                        self.add_error(error.clone());
                        return Err(error);
                    }
                    let reg = lock.expect("registry lock was Ok");
                    let function_type = match Self::function_type_from_c_strings(&reg, &param_strs, &ret_str)
                    {
                        Ok(ty) => ty,
                        Err(e) => {
                            drop(reg);
                            self.add_error(e.clone());
                            return Err(e);
                        }
                    };
                    c_variadic = variadic;
                    function_type
                } else {
                    let error = TypeSystemError::undefined_function(name, location);
                    self.add_error(error.clone());
                    return Err(error);
                }
            }
            Err(_) => {
                let error = TypeSystemError::ParseError {
                    type_str: name.to_string(),
                    reason: "Failed to access registry".to_string(),
                };
                self.add_error(error.clone());
                return Err(error);
            }
        };

        let (param_types, return_type) = match function_type {
            Type::Function { params, return_type } => (params, *return_type),
            _ => {
                let error = TypeSystemError::not_callable(function_type, location);
                self.add_error(error.clone());
                return Err(error);
            }
        };

        if !c_variadic {
            c_variadic = self.c_symbol_is_variadic(name);
        }
        self.check_call_arguments(&param_types, args, c_variadic, name)?;
        self.extend_arg_loans_if_ref_return(&return_type, Some(name), args);
        Ok(return_type)
    }

    /// Function alias, type/enum name, or imported C symbol — not a local.
    pub fn type_of_global_name(&self, name: &str) -> Option<Type> {
        if let Ok(reg) = self.registry.read() {
            if let Some(ty) = reg.get_alias(name) {
                return Some(ty);
            }
            if reg.get_type(name).is_some() {
                return Some(Type::NamedType {
                    name: name.to_string(),
                });
            }
        }
        self.c_imported_function_type(name).ok().flatten()
    }

    fn function_type_from_c_strings(
        reg: &crate::types::registry::TypeRegistry,
        param_strs: &[String],
        ret_str: &str,
    ) -> Result<Type, TypeSystemError> {
        let mut param_types = Vec::new();
        for param_type in param_strs {
            param_types.push(reg.resolve_type(param_type)?);
        }
        let return_type = reg.resolve_type(ret_str)?;
        Ok(Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
        })
    }

    fn c_imported_function_type(&self, name: &str) -> Result<Option<Type>, TypeSystemError> {
        let (param_strs, ret_str) = {
            let Some(analyzer) = self.analyzer.as_ref() else {
                return Ok(None);
            };
            let Ok(analyzer) = analyzer.read() else {
                return Err(TypeSystemError::ParseError {
                    type_str: name.to_string(),
                    reason: "Failed to access analyzer".to_string(),
                });
            };
            let cfc_symbols = analyzer.get_cfc_symbols()?;
            let mut found = None;
            for (_lib_name, symbol_table) in cfc_symbols.iter() {
                if let Some(c_symbol) = symbol_table.symbols.get(name) {
                    found = Some((
                        c_symbol
                            .parameters
                            .iter()
                            .map(|p| p.param_type.clone())
                            .collect::<Vec<_>>(),
                        c_symbol.return_type.clone(),
                    ));
                    break;
                }
            }
            match found {
                Some(sig) => sig,
                None => return Ok(None),
            }
        };
        let Ok(reg) = self.registry.read() else {
            return Err(TypeSystemError::ParseError {
                type_str: name.to_string(),
                reason: "Failed to access registry".to_string(),
            });
        };
        Ok(Some(Self::function_type_from_c_strings(
            &reg,
            &param_strs,
            &ret_str,
        )?))
    }

    pub(super) fn c_symbol_is_variadic(&self, name: &str) -> bool {
        let Some(ref analyzer) = self.analyzer else {
            return false;
        };
        let Ok(analyzer) = analyzer.read() else {
            return false;
        };
        let Ok(cfc_symbols) = analyzer.get_cfc_symbols() else {
            return false;
        };
        cfc_symbols.iter().any(|(_lib, table)| {
            table.symbols.get(name).map(|s| s.is_variadic).unwrap_or(false)
        })
    }
}

fn arg_borrow_place(expr: &crate::parser::expr::Expression) -> Option<crate::types::borrow::Place> {
    use crate::parser::expr::Expression;
    match expr.kind() {
        Expression::Unary { op, operand } if op == "&" || op == "&mut" => expr_place(operand.as_ref()),
        Expression::Variable(name) => Some(crate::types::borrow::Place::var(name)),
        _ => expr_place(expr),
    }
}

fn expr_place(expr: &crate::parser::expr::Expression) -> Option<crate::types::borrow::Place> {
    use crate::parser::expr::Expression;
    match expr.kind() {
        Expression::Variable(name) => Some(crate::types::borrow::Place::var(name)),
        Expression::Member { object, field, args } if args.is_empty() => {
            Some(expr_place(object.as_ref())?.field(field.clone()))
        }
        Expression::Index { array, .. } => Some(expr_place(array.as_ref())?.index()),
        _ => None,
    }
}
