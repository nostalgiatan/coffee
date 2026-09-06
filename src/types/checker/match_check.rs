use super::*;
use super::super::definition::{Span, Type, TypeDef};
use crate::parser;
use crate::types::errors::TypeSystemError;
use std::collections::HashSet;

impl super::TypeChecker {
    pub(super) fn check_match(&mut self, match_expr: &parser::MatchExpr) -> Result<(), TypeSystemError> {
        let scrutinee_ty = match self.check_expression(&match_expr.value) {
            Ok(ty) => ty,
            Err(e) => {
                self.add_error(e.clone());
                return Err(e);
            }
        };
        for arm in &match_expr.arms {
            let bound = self.bind_pattern_vars(&arm.pattern, &scrutinee_ty)?;
            if let Some(guard) = &arm.guard {
                self.require_bool_condition(guard)?;
            }
            self.check_scoped_stmts(&arm.body)?;
            self.unbind_pattern_vars(bound);
        }
        self.check_match_exhaustiveness(&scrutinee_ty, &match_expr.arms)
    }

    pub(super) fn check_match_exhaustiveness(
        &mut self,
        scrutinee_ty: &Type,
        arms: &[parser::r#match::MatchArm],
    ) -> Result<(), TypeSystemError> {
        if matches!(
            scrutinee_ty,
            Type::Function { .. } | Type::Void | Type::Variadic
        ) {
            let catch_all_only = !arms.is_empty()
                && arms
                    .iter()
                    .all(|a| Self::pattern_is_catch_all(&a.pattern));
            if catch_all_only {
                return Ok(());
            }
            let error = TypeSystemError::ConstraintViolation {
                constraint: "exhaustive match".to_string(),
                reason: format!(
                    "match on {} requires catch-all only (ident or _)",
                    scrutinee_ty
                ),
                span: TypeSystemError::span_for_name(&scrutinee_ty.to_string()),
            };
            self.add_error(error.clone());
            return Err(error);
        }
        if matches!(scrutinee_ty, Type::Int { .. } | Type::Float { .. } | Type::String) {
            let catch_all = arms.iter().any(Self::arm_is_irrefutable);
            if catch_all {
                return Ok(());
            }
            let error = TypeSystemError::ConstraintViolation {
                constraint: "exhaustive match".to_string(),
                reason: format!("non-exhaustive match on {}", scrutinee_ty),
                span: TypeSystemError::span_for_name(&scrutinee_ty.to_string()),
            };
            self.add_error(error.clone());
            return Err(error);
        }
        if *scrutinee_ty == Type::bool() {
            let mut saw_true = false;
            let mut saw_false = false;
            let mut catch_all = false;
            for arm in arms {
                if Self::arm_is_irrefutable(arm) {
                    catch_all = true;
                    break;
                }
                if arm.guard.is_some() {
                    continue;
                }
                if let crate::parser::Pattern::Literal(lit) = &arm.pattern {
                    match lit.as_str() {
                        "true" => saw_true = true,
                        "false" => saw_false = true,
                        _ => {}
                    }
                }
            }
            if catch_all || (saw_true && saw_false) {
                return Ok(());
            }
            let error = TypeSystemError::ConstraintViolation {
                constraint: "exhaustive match".to_string(),
                reason: "non-exhaustive match on bool".to_string(),
                span: TypeSystemError::span_for_name(&scrutinee_ty.to_string()),
            };
            self.add_error(error.clone());
            return Err(error);
        }
        if matches!(scrutinee_ty, Type::Tuple(_)) {
            let catch_all = arms.iter().any(Self::arm_is_irrefutable);
            if catch_all {
                return Ok(());
            }
            let error = TypeSystemError::ConstraintViolation {
                constraint: "exhaustive match".to_string(),
                reason: format!("non-exhaustive match on {}", scrutinee_ty),
                span: TypeSystemError::span_for_name(&scrutinee_ty.to_string()),
            };
            self.add_error(error.clone());
            return Err(error);
        }
        let Type::NamedType { name } = scrutinee_ty else {
            return Ok(());
        };
        let variants: Vec<String> = {
            let Ok(reg) = self.registry.read() else {
                return Ok(());
            };
            match reg.get_type(name) {
                Some(TypeDef::Enum { variants, .. }) => {
                    variants.iter().map(|v| v.name.clone()).collect()
                }
                _ => return Ok(()),
            }
        };
        if variants.is_empty() {
            return Ok(());
        }
        let mut covered: HashSet<String> = HashSet::new();
        let mut catch_all = false;
        for arm in arms {
            if Self::arm_is_irrefutable(arm) {
                catch_all = true;
                break;
            }
            Self::collect_covered_enum_variants(&arm.pattern, name, &mut covered);
        }
        if catch_all {
            return Ok(());
        }
        let missing: Vec<String> = variants
            .into_iter()
            .filter(|v| !covered.contains(v))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        let error = TypeSystemError::ConstraintViolation {
            constraint: "exhaustive match".to_string(),
            reason: format!("missing variants: {}", missing.join(", ")),
            span: TypeSystemError::span_for_name(&scrutinee_ty.to_string()),
        };
        self.add_error(error.clone());
        Err(error)
    }

    pub(super) fn arm_is_irrefutable(arm: &parser::r#match::MatchArm) -> bool {
        arm.guard.is_none() && Self::pattern_is_catch_all(&arm.pattern)
    }

    pub(super) fn pattern_is_catch_all(pattern: &crate::parser::Pattern) -> bool {
        match pattern {
            crate::parser::Pattern::Wildcard | crate::parser::Pattern::Ident(_) => true,
            crate::parser::Pattern::Tuple(elements) => {
                elements.iter().all(Self::pattern_is_catch_all)
            }
            crate::parser::Pattern::Struct { fields, .. } => {
                fields.iter().all(|(_, p)| Self::pattern_is_catch_all(p))
            }
            crate::parser::Pattern::Or(elements) => elements.iter().any(Self::pattern_is_catch_all),
            _ => false,
        }
    }

    pub(super) fn collect_covered_enum_variants(
        pattern: &crate::parser::Pattern,
        enum_name: &str,
        covered: &mut HashSet<String>,
    ) {
        match pattern {
            crate::parser::Pattern::EnumVariant { enum_name: en, variant, .. } => {
                if en.is_empty() || en == enum_name {
                    covered.insert(variant.clone());
                }
            }
            crate::parser::Pattern::Or(elements) => {
                for el in elements {
                    Self::collect_covered_enum_variants(el, enum_name, covered);
                }
            }
            _ => {}
        }
    }

    pub(super) fn bind_pattern_vars(
        &mut self,
        pattern: &crate::parser::Pattern,
        expected: &Type,
    ) -> Result<Vec<(String, Option<ValueInfo>)>, TypeSystemError> {
        let mut bound = Vec::new();
        self.collect_pattern_bindings(pattern, expected, &mut bound)?;
        Ok(bound)
    }

    fn tuple_pattern_found_type(arity: usize) -> Type {
        Type::Tuple(vec![
            Type::NamedType {
                name: "_".to_string(),
            };
            arity
        ])
    }

    pub(super) fn variant_payload_types(
        &self,
        enum_name: &str,
        variant: &str,
    ) -> Result<Vec<Type>, TypeSystemError> {
        let Ok(reg) = self.registry.read() else {
            return Ok(Vec::new());
        };
        let Some(TypeDef::Enum { variants, .. }) = reg.get_type(enum_name) else {
            return Ok(Vec::new());
        };
        let Some(v) = variants.iter().find(|v| v.name == variant) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::with_capacity(v.fields.len());
        for field in &v.fields {
            let type_str = match field {
                crate::types::definition::VariantField::Positional(t) => t.as_str(),
                crate::types::definition::VariantField::Named { ty, .. } => ty.as_str(),
            };
            match reg.resolve_type(type_str) {
                Ok(ty) => out.push(ty),
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    pub(super) fn collect_pattern_bindings(
        &mut self,
        pattern: &crate::parser::Pattern,
        expected: &Type,
        bound: &mut Vec<(String, Option<ValueInfo>)>,
    ) -> Result<(), TypeSystemError> {
        match pattern {
            crate::parser::Pattern::Ident(name) => {
                let prev = self.values.remove(name);
                self.values.insert(name.clone(), ValueInfo {
                    ty: expected.clone(),
                    state: ValueState::Alive,
                    location: Span::new(0, name.len()),
                });
                bound.push((name.clone(), prev));
                Ok(())
            }
            crate::parser::Pattern::Tuple(elements) => {
                match expected {
                    Type::Tuple(tys) => {
                        if elements.len() != tys.len() {
                            let error = TypeSystemError::type_mismatch(
                                expected.clone(),
                                Self::tuple_pattern_found_type(elements.len()),
                                TypeSystemError::span_for_name(&expected.to_string()),
                            );
                            self.add_error(error.clone());
                            return Err(error);
                        }
                        for (el, ty) in elements.iter().zip(tys.iter()) {
                            self.collect_pattern_bindings(el, ty, bound)?;
                        }
                        Ok(())
                    }
                    _ => {
                        let error = TypeSystemError::type_mismatch(
                            expected.clone(),
                            Self::tuple_pattern_found_type(elements.len()),
                            TypeSystemError::span_for_name(&expected.to_string()),
                        );
                        self.add_error(error.clone());
                        Err(error)
                    }
                }
            }
            crate::parser::Pattern::Or(elements) => {
                for el in elements {
                    self.collect_pattern_bindings(el, expected, bound)?;
                }
                Ok(())
            }
            crate::parser::Pattern::Struct { name, fields } => {
                match expected {
                    Type::NamedType { name: expected_name } if expected_name == name || name.is_empty() => {
                        let type_name = if name.is_empty() { expected_name.clone() } else { name.clone() };
                        for (field, value) in fields {
                            let ty = match self.lookup_field_type(&type_name, field) {
                                Ok(ty) => ty,
                                Err(e) => {
                                    self.add_error(e.clone());
                                    return Err(e);
                                }
                            };
                            self.collect_pattern_bindings(value, &ty, bound)?;
                        }
                        Ok(())
                    }
                    _ => {
                        let error = TypeSystemError::type_mismatch(
                            expected.clone(),
                            Type::NamedType { name: name.clone() },
                            TypeSystemError::span_for_name(name),
                        );
                        self.add_error(error.clone());
                        Err(error)
                    }
                }
            }
            crate::parser::Pattern::EnumVariant { enum_name, variant, args } => {
                let type_name = if !enum_name.is_empty() {
                    enum_name.clone()
                } else if let Type::NamedType { name } = expected {
                    name.clone()
                } else {
                    String::new()
                };
                let payload_tys = match self.variant_payload_types(&type_name, variant) {
                    Ok(tys) => tys,
                    Err(e) => {
                        self.add_error(e.clone());
                        return Err(e);
                    }
                };
                for (i, arg) in args.iter().enumerate() {
                    let Some(ty) = payload_tys.get(i).cloned() else {
                        let error = TypeSystemError::ConstraintViolation {
                            constraint: "match payload".to_string(),
                            reason: format!(
                                "incomplete/unknown payload for {type_name}.{variant} at index {i}"
                            ),
                            span: TypeSystemError::span_for_name(variant),
                        };
                        self.add_error(error.clone());
                        return Err(error);
                    };
                    self.collect_pattern_bindings(arg, &ty, bound)?;
                }
                Ok(())
            }
            crate::parser::Pattern::Wildcard | crate::parser::Pattern::Literal(_) => Ok(()),
        }
    }

    pub(super) fn unbind_pattern_vars(&mut self, bound: Vec<(String, Option<ValueInfo>)>) {
        for (name, prev) in bound.into_iter().rev() {
            match prev {
                Some(info) => {
                    self.values.insert(name, info);
                }
                None => {
                    self.values.remove(&name);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use crate::types::definition::Type;
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    fn hole_tuple(arity: usize) -> Type {
        Type::Tuple(vec![
            Type::NamedType {
                name: "_".to_string(),
            };
            arity
        ])
    }

    fn found_has_invented_int(found: &Type) -> bool {
        match found {
            Type::Int { .. } => true,
            Type::Tuple(els) => els.iter().any(found_has_invented_int),
            _ => false,
        }
    }

    fn assert_honest_tuple_pattern_found(err: TypeSystemError, arity: usize) {
        match err {
            TypeSystemError::TypeMismatch { found, .. } => {
                assert!(
                    !found_has_invented_int(&found),
                    "tuple pattern mismatch must not invent int as found, got {found:?}"
                );
                assert!(
                    found == hole_tuple(arity)
                        || found
                            == Type::NamedType {
                                name: "tuple pattern".to_string(),
                            },
                    "expected hole-tuple of arity {arity} or NamedType \"tuple pattern\", got {found:?}"
                );
            }
            TypeSystemError::ParseError { type_str, reason } => {
                let blob = format!("{type_str} {reason}").to_lowercase();
                assert!(
                    !blob.contains("int"),
                    "ParseError must not invent int, got type_str={type_str:?} reason={reason:?}"
                );
                assert!(
                    blob.contains("tuple") || blob.contains("pattern") || blob.contains("arity"),
                    "ParseError should describe tuple pattern mismatch, got {reason:?}"
                );
            }
            other => panic!(
                "expected TypeMismatch without invented int or ParseError, got {other:?}"
            ),
        }
    }

    #[test]
    fn tuple_pattern_arity_mismatch_does_not_invent_int_found() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let expected = Type::Tuple(vec![Type::int(), Type::int()]);
        let err = checker
            .bind_pattern_vars(
                &parser::pattern::Pattern::Tuple(vec![parser::pattern::Pattern::Ident(
                    "x".to_string(),
                )]),
                &expected,
            )
            .expect_err("arity mismatch should fail");
        assert_honest_tuple_pattern_found(err, 1);
    }

    #[test]
    fn tuple_pattern_on_non_tuple_does_not_invent_int_found() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        let err = checker
            .bind_pattern_vars(
                &parser::pattern::Pattern::Tuple(vec![
                    parser::pattern::Pattern::Ident("a".to_string()),
                    parser::pattern::Pattern::Ident("b".to_string()),
                ]),
                &Type::bool(),
            )
            .expect_err("tuple pattern on bool should fail");
        assert_honest_tuple_pattern_found(err, 2);
    }
}
