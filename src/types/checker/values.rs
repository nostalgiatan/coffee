use super::*;
use super::super::definition::{Span, Type};

/// Type bounds for primitive types
#[derive(Debug, Clone)]
pub struct TypeBounds {
    /// Integer minimum value
    pub int_min: i64,
    /// Integer maximum value
    pub int_max: i64,
    /// Float minimum value
    pub float_min: f64,
    /// Float maximum value
    pub float_max: f64,
}

impl Default for TypeBounds {
    fn default() -> Self {
        TypeBounds {
            int_min: i64::MIN,
            int_max: i64::MAX,
            float_min: f64::MIN,
            float_max: f64::MAX,
        }
    }
}

/// State of a value for ownership checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueState {
    /// Value is alive and owned
    Alive,
    /// Value has been moved
    Moved,
    /// Value has been dropped/freed
    Dropped { location: String },
    /// Value is borrowed
    Borrowed { immutable: bool },
}

/// Value ownership information
#[derive(Debug, Clone)]
pub struct ValueInfo {
    /// Type of the value
    pub ty: Type,
    /// Current state
    pub state: ValueState,
    /// Declaration location
    pub location: Span,
}

use crate::types::errors::TypeSystemError;

impl super::TypeChecker {
    pub(super) fn lookup_assignment_target(&mut self, name: &str) -> Result<Type, TypeSystemError> {
        let mut parts = name.split('.');
        let root = match parts.next() {
            Some(root) if !root.is_empty() => root,
            _ => {
                let error = TypeSystemError::undefined_variable(name, Span::new(0, name.len()));
                self.add_error(error.clone());
                return Err(error);
            }
        };
        let mut ty = match self.check_expression(&crate::parser::expr::Expression::Variable(root.to_string())) {
            Ok(ty) => ty,
            Err(e) => {
                self.add_error(e.clone());
                return Err(e);
            }
        };
        for field in parts {
            let class_name = match &ty {
                Type::NamedType { name } => name.clone(),
                _ => {
                    let error = TypeSystemError::ParseError {
                        type_str: "assignment".to_string(),
                        reason: format!("Cannot assign field '{}' on non-class type {:?}", field, ty),
                    };
                    self.add_error(error.clone());
                    return Err(error);
                }
            };
            ty = match self.lookup_field_type(&class_name, field) {
                Ok(field_ty) => field_ty,
                Err(e) => {
                    self.add_error(e.clone());
                    return Err(e);
                }
            };
        }
        Ok(ty)
    }

    pub(super) fn bind_alive(&mut self, name: &str, ty: Type) {
        self.values.insert(name.to_string(), ValueInfo {
            ty,
            state: ValueState::Alive,
            location: Span::new(0, name.len()),
        });
    }

    /// After `bind_function_locals`, end-of-body `rm` leaves locals `Dropped`.
    /// HIR lowering infers expressions out of program order, so revive every
    /// bound local to `Alive` while keeping types. Ownership of use-after-`rm`
    /// was already checked by `check_function`; this snapshot is for lowering only.
    pub fn revive_bound_locals_for_lowering(&mut self) {
        for info in self.values.values_mut() {
            info.state = ValueState::Alive;
        }
    }

    pub(super) fn similar_value_names(&self, name: &str) -> Vec<String> {
        let cands: Vec<String> = self.values.keys().cloned().collect();
        crate::diagnostics::find_similar_names(name, &cands, 2, 3)
    }

    pub(super) fn undefined_var_with_similar(&self, name: &str) -> TypeSystemError {
        TypeSystemError::undefined_variable(name.to_string(), Span::new(0, name.len()))
            .with_similar(self.similar_value_names(name))
    }
    pub(super) fn lookup_live_variable(&self, name: &str) -> Result<Type, TypeSystemError> {
        if self.capture_forbidden.contains(name) {
            return Err(TypeSystemError::ConstraintViolation {
                constraint: "capture".to_string(),
                reason: format!(
                    "anonymous function cannot capture enclosing local or parameter '{}'",
                    name
                ),
                span: Span::new(0, name.len()),
            });
        }
        if let Some(info) = self.values.get(name) {
            match &info.state {
                ValueState::Moved => {
                    return Err(TypeSystemError::OwnershipError {
                        reason: format!("use of moved value '{}'", name),
                        span: Span::new(0, name.len()),
                    });
                }
                ValueState::Dropped { location } => {
                    return Err(TypeSystemError::OwnershipError {
                        reason: format!("use of dropped value '{}' (dropped at {})", name, location),
                        span: Span::new(0, name.len()),
                    });
                }
                ValueState::Alive | ValueState::Borrowed { .. } => {
                    if let Err(e) = self.borrow.deny_use_while_mut(name) {
                        return Err(e);
                    }
                    return Ok(info.ty.clone());
                }
            }
        }
        Err(self.undefined_var_with_similar(name))
    }

    pub(super) fn lookup_place(&self, name: &str) -> Result<Type, TypeSystemError> {
        if let Some(info) = self.values.get(name) {
            match &info.state {
                ValueState::Moved => {
                    return Err(TypeSystemError::OwnershipError {
                        reason: format!("use of moved value '{}'", name),
                        span: Span::new(0, name.len()),
                    });
                }
                ValueState::Dropped { location } => {
                    return Err(TypeSystemError::OwnershipError {
                        reason: format!("use of dropped value '{}' (dropped at {})", name, location),
                        span: Span::new(0, name.len()),
                    });
                }
                ValueState::Alive | ValueState::Borrowed { .. } => return Ok(info.ty.clone()),
            }
        }
        Err(self.undefined_var_with_similar(name))
    }
    pub(super) fn claim_ref_binding(
        &mut self,
        name: &str,
        value: &crate::parser::expr::Expression,
    ) -> Result<(), TypeSystemError> {
        match value.kind() {
            crate::parser::expr::Expression::Unary { op, .. } if op == "&" || op == "&mut" => {
                self.borrow.claim_last_for(name);
            }
            crate::parser::expr::Expression::Call { .. } => {
                self.borrow.claim_unheld_for(name);
            }
            crate::parser::expr::Expression::Variable(src) => {
                if self.values.get(src).map(|v| matches!(v.ty, Type::Ref { .. })).unwrap_or(false) {
                    self.borrow.copy_loan_to(src, name)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn check_ref_escape(
        &self,
        expr: &crate::parser::expr::Expression,
        expected: &Type,
    ) -> Result<(), TypeSystemError> {
        if !matches!(expected, Type::Ref { .. }) {
            return Ok(());
        }
        let place = match expr.kind() {
            crate::parser::expr::Expression::Unary { op, operand } if op == "&" || op == "&mut" => {
                match operand.kind() {
                    crate::parser::expr::Expression::Variable(n) => Some(n.as_str()),
                    _ => None,
                }
            }
            crate::parser::expr::Expression::Variable(n) => self.borrow.place_of_holder(n),
            _ => None,
        };
        if let Some(place) = place {
            if !self.borrow.is_param(place) {
                return Err(TypeSystemError::LifetimeError {
                    reason: format!("returns reference to local variable '{}'", place),
                    span: Span::new(0, place.len()),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::TypeChecker;
    use crate::parser::expr::Expression;
    use crate::types::borrow::Place;
    use crate::types::definition::Type;
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    fn ref_int() -> Type {
        Type::Ref {
            elem: Box::new(Type::int()),
            mutable: false,
        }
    }

    #[test]
    fn check_ref_escape_peels_parsed_addr_of_local() {
        let checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let parsed = crate::parser::expr::parse_expression("&x").expect("parse &x");
        assert!(matches!(parsed, Expression::Spanned { .. }));
        let err = checker
            .check_ref_escape(&parsed, &ref_int())
            .expect_err("returning &local must fail");
        match err {
            TypeSystemError::LifetimeError { reason, .. } => {
                assert!(reason.contains("'x'"), "got {reason}");
            }
            other => panic!("expected LifetimeError, got {other:?}"),
        }
    }

    #[test]
    fn check_ref_escape_peels_parsed_variable_holder() {
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        checker
            .borrow
            .acquire_place(&Place::var("local"), false)
            .expect("loan local");
        checker.borrow.claim_last_for("x");
        let parsed = crate::parser::expr::parse_expression("x").expect("parse x");
        assert!(matches!(parsed, Expression::Spanned { .. }));
        let from_parsed = checker.check_ref_escape(&parsed, &ref_int());
        let from_bare = checker.check_ref_escape(&Expression::Variable("x".to_string()), &ref_int());
        assert!(from_parsed.is_err(), "Spanned Variable holder must escape-check");
        assert_eq!(format!("{from_parsed:?}"), format!("{from_bare:?}"));
    }
}

impl super::TypeChecker {
    pub(super) fn bind_memory_target(&mut self, target: &str, ty: Type) {
        if self.mode != CheckingMode::Comprehensive {
            return;
        }
        let location = Span::new(0, target.len());
        self.values.insert(
            target.to_string(),
            ValueInfo {
                ty,
                state: ValueState::Alive,
                location,
            },
        );
    }
}
