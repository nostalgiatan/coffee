use super::super::super::definition::Type;
use crate::types::errors::TypeSystemError;
use crate::types::registry::TypeRegistry;

fn newtype_one_hop(reg: &TypeRegistry, from: &Type, to: &Type) -> bool {
    if let Type::NamedType { name } = from {
        if let Some(src) = reg.newtype_source(name) {
            return to == &src;
        }
    }
    if let Type::NamedType { name } = to {
        if let Some(src) = reg.newtype_source(name) {
            return from == &src;
        }
    }
    false
}

impl super::super::TypeChecker {
    pub(in super::super) fn cast_allowed(&self, from: &Type, to: &Type) -> bool {
        if from == to {
            return true;
        }
        if let Ok(reg) = self.registry.read() {
            let from_nt = reg.type_is_newtype(from);
            let to_nt = reg.type_is_newtype(to);
            if from_nt || to_nt {
                return newtype_one_hop(&reg, from, to);
            }
        }
        if to.can_coerce_from(from) {
            return true;
        }
        if from.is_numeric() && to.is_numeric() {
            return true;
        }
        matches!(
            (from, to),
            (Type::Int { .. }, Type::Bool) | (Type::Float { .. }, Type::Bool)
        )
    }

    #[cfg(test)]
    pub(crate) fn cast_allowed_for_test(&self, from: &Type, to: &Type) -> bool {
        self.cast_allowed(from, to)
    }
    pub(in super::super) fn check_unary_operation(&self, op: &str, operand_type: &Type) -> Result<Type, TypeSystemError> {
        let location = TypeSystemError::span_for_name(op);
        match op {
            "!" => {
                if *operand_type == Type::bool() {
                    Ok(Type::bool())
                } else {
                    Err(TypeSystemError::invalid_operation(op, operand_type.clone(), operand_type.clone(), location))
                }
            }
            "-" => {
                if operand_type.is_numeric() {
                    Ok(operand_type.clone())
                } else {
                    Err(TypeSystemError::invalid_operation(op, operand_type.clone(), operand_type.clone(), location))
                }
            }
            "~" => {
                if operand_type.is_int() {
                    Ok(operand_type.clone())
                } else {
                    Err(TypeSystemError::invalid_operation(op, operand_type.clone(), operand_type.clone(), location))
                }
            }
            "clone" => {
                if operand_type.is_buf() {
                    Err(TypeSystemError::OwnershipError {
                        reason: "clone buf needs a size; use a slice".to_string(),
                        span: location,
                    })
                } else {
                    Ok(operand_type.clone())
                }
            }
            _ => Err(TypeSystemError::invalid_operation(op, operand_type.clone(), operand_type.clone(), location)),
        }
    }

    /// Check binary operation with proper types
    pub(in super::super) fn check_binary_operation(&self, left_type: &Type, op: &str, right_type: &Type) -> Result<Type, TypeSystemError> {
        let location = TypeSystemError::span_for_name(op);

        match op {
            "+" | "-" | "*" | "/" | "%" => {
                if left_type.is_numeric() && right_type.is_numeric() {
                    Ok(self.promote_numeric_types(left_type, right_type))
                } else if op == "+"
                    && left_type.is_ptr_offset_base()
                    && right_type.is_int()
                {
                    Ok(left_type.clone())
                } else if op == "+"
                    && left_type.is_int()
                    && right_type.is_ptr_offset_base()
                {
                    Ok(right_type.clone())
                } else {
                    Err(TypeSystemError::invalid_operation(op, left_type.clone(), right_type.clone(), location))
                }
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                if self.types_compatible(left_type, right_type)? {
                    Ok(Type::bool())
                } else {
                    Err(TypeSystemError::invalid_operation(op, left_type.clone(), right_type.clone(), location))
                }
            }
            "&&" | "||" => {
                if *left_type == Type::bool() && *right_type == Type::bool() {
                    Ok(Type::bool())
                } else {
                    Err(TypeSystemError::invalid_operation(op, left_type.clone(), right_type.clone(), location))
                }
            }
            "&" | "|" | "^" | "<<" | ">>" => {
                if left_type.is_int() && right_type.is_int() {
                    Ok(self.promote_numeric_types(left_type, right_type))
                } else {
                    Err(TypeSystemError::invalid_operation(op, left_type.clone(), right_type.clone(), location))
                }
            }
            _ => {
                Err(TypeSystemError::invalid_operation(op, left_type.clone(), right_type.clone(), location))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::TypeChecker;
    use crate::parser::expr::Expression;
    use crate::types::definition::Type;
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    fn unary(op: &str, operand: &str) -> Expression {
        Expression::Unary {
            op: op.to_string(),
            operand: Box::new(Expression::Literal(operand.to_string())),
        }
    }

    fn assert_unary_invalid_rhs_is_operand(err: TypeSystemError, expected_op: &str, expected_operand: Type) {
        match err {
            TypeSystemError::InvalidOperation { op, left, right, .. } => {
                assert_eq!(op, expected_op);
                assert_eq!(left, expected_operand);
                assert_eq!(right, expected_operand);
                assert!(
                    !matches!(&right, Type::NamedType { name } if name == "_" || name == "()"),
                    "unary InvalidOperation must not invent a dummy RHS type, got {right:?}"
                );
            }
            other => panic!("expected InvalidOperation, got {other:?}"),
        }
    }

    #[test]
    fn bang_on_int_reports_operand_as_both_sides() {
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let err = checker.check_expression(&unary("!", "1")).unwrap_err();
        assert_unary_invalid_rhs_is_operand(err, "!", Type::int());
    }

    #[test]
    fn negate_on_bool_reports_operand_as_both_sides() {
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let err = checker.check_expression(&unary("-", "true")).unwrap_err();
        assert_unary_invalid_rhs_is_operand(err, "-", Type::bool());
    }

    #[test]
    fn bitnot_on_float_reports_operand_as_both_sides() {
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let err = checker.check_expression(&unary("~", "1.5")).unwrap_err();
        assert_unary_invalid_rhs_is_operand(err, "~", Type::float());
    }

    #[test]
    fn unknown_unary_after_clone_reports_operand_as_both_sides() {
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let err = checker.check_expression(&unary("not-an-op", "1")).unwrap_err();
        assert_unary_invalid_rhs_is_operand(err, "not-an-op", Type::int());
    }

    #[test]
    fn clone_unary_still_returns_operand_type() {
        let mut checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let ty = checker
            .check_expression(&unary("clone", "1"))
            .expect("clone is a successful unary");
        assert_eq!(ty, Type::int());
    }
}
