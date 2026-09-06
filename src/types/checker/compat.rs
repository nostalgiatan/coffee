use super::*;
use super::super::definition::Type;
use crate::coffee_debug;
use crate::types::errors::TypeSystemError;

impl super::TypeChecker {
    /// Integer/float literals may be written without a width suffix; they fit
    /// an annotated type when the value is in range (including unary minus).
    pub(super) fn numeric_literal_fits(&self, expr: &crate::parser::expr::Expression, expected: &Type) -> bool {
        match expected {
            Type::Int { bits, signed } => {
                let Some(n) = Self::expr_as_i128(expr) else {
                    return false;
                };
                let bits = *bits as u32;
                if bits == 0 || bits > 128 {
                    return false;
                }
                if *signed {
                    let min = -(1i128 << (bits - 1));
                    let max = (1i128 << (bits - 1)) - 1;
                    n >= min && n <= max
                } else {
                    n >= 0 && n <= (1i128 << bits) - 1
                }
            }
            Type::Float { bits: 32 } => Self::expr_as_f64(expr)
                .map(|f| f.is_finite() && f.abs() <= f32::MAX as f64)
                .unwrap_or(false),
            Type::Float { bits: 64 } => Self::expr_as_f64(expr).map(|f| f.is_finite()).unwrap_or(false),
            _ => false,
        }
    }

    pub(super) fn expr_as_i128(expr: &crate::parser::expr::Expression) -> Option<i128> {
        match expr.kind() {
            crate::parser::expr::Expression::Literal(s) => s.parse().ok(),
            crate::parser::expr::Expression::Unary { op, operand } if op == "-" => {
                Self::expr_as_i128(operand).map(|n| -n)
            }
            _ => None,
        }
    }

    pub(super) fn expr_as_f64(expr: &crate::parser::expr::Expression) -> Option<f64> {
        match expr.kind() {
            crate::parser::expr::Expression::Literal(s) => s.parse().ok(),
            crate::parser::expr::Expression::Unary { op, operand } if op == "-" => {
                Self::expr_as_f64(operand).map(|n| -n)
            }
            _ => None,
        }
    }

    /// Check if types are compatible (`found` assigned to `expected`).
    pub(super) fn types_compatible(&self, found: &Type, expected: &Type) -> Result<bool, TypeSystemError> {
        coffee_debug!("[DEBUG] types_compatible: comparing found {:?} vs expected {:?}", found, expected);
        if found == expected {
            coffee_debug!("[DEBUG] types_compatible: types are equal, returning true");
            return Ok(true);
        }

        if let Ok(reg) = self.registry.read() {
            if reg.type_is_newtype(found) || reg.type_is_newtype(expected) {
                coffee_debug!("[DEBUG] types_compatible: newtype vs other, no implicit coerce");
                return Ok(false);
            }
            if let Type::NamedType { name } = expected {
                if reg.is_c_enum(name) && found.is_int() {
                    return Ok(true);
                }
            }
            if let Type::NamedType { name } = found {
                if reg.is_c_enum(name) && expected.is_int() {
                    return Ok(true);
                }
            }
        }

        // C `object`: in-params use handle-shaped values (not bool). Out-params
        // only to pointer-sized int, string, ref, or object — never int(4)+.
        if matches!(expected, Type::Variadic) {
            return Ok(found.is_c_handle_value());
        }
        if matches!(found, Type::Variadic) {
            return Ok(
                expected.is_pointer_sized_int()
                    || expected.is_buf()
                    || matches!(expected, Type::String | Type::Ref { .. } | Type::Variadic),
            );
        }

        if expected.can_coerce_from(found) {
            coffee_debug!("[DEBUG] types_compatible: expected can coerce from found, returning true");
            return Ok(true);
        }

        if self.mode == CheckingMode::Comprehensive {
            match self.registry.read() {
                Ok(reg) => {
                    let is_compatible = reg.is_compatible(expected, found);
                    coffee_debug!("[DEBUG] types_compatible: registry says {} (comprehensive mode)", is_compatible);
                    Ok(is_compatible)
                },
                Err(_) => {
                    coffee_debug!("[DEBUG] types_compatible: failed to read registry, returning false");
                    Ok(false)
                },
            }
        } else {
            coffee_debug!("[DEBUG] types_compatible: not comprehensive mode, returning false");
            Ok(false)
        }
    }

    /// Promote numeric types for arithmetic operations
    pub(super) fn promote_numeric_types(&self, left: &Type, right: &Type) -> Type {
        match (left, right) {
            (Type::Float { .. }, _) | (_, Type::Float { .. }) => Type::float(),
            (Type::Int { bits: b1, signed: s1 }, Type::Int { bits: b2, signed: s2 }) => {
                let bits = (*b1).max(*b2);
                let signed = *s1 || *s2;
                Type::Int { bits, signed }
            }
            _ => left.clone(),
        }
    }

    /// Parse `fn(T, U) => R` used in `let` annotations ([`type_from_str`], not a second scanner).
    pub(super) fn coffee_fn_type_from_str(s: &str) -> Option<Type> {
        match crate::types::definition::type_from_str(s.trim()) {
            Ok(ty @ Type::Function { .. }) => Some(ty),
            _ => None,
        }
    }

    /// Check value bounds (comprehensive mode only)
    pub(super) fn check_value_bounds(&self, ty: &Type, value: &str, location: Span) -> Result<(), TypeSystemError> {
        match ty {
            Type::Int { .. } => {
                if let Ok(val) = value.parse::<i64>() {
                    if val < self.bounds.int_min || val > self.bounds.int_max {
                        return Err(TypeSystemError::ConstraintViolation {
                            constraint: "integer bounds".to_string(),
                            reason: format!("Value {} exceeds bounds [{}, {}]", val, self.bounds.int_min, self.bounds.int_max),
                            span: location,
                        });
                    }
                }
            }
            Type::Float { .. } => {
                if let Ok(val) = value.parse::<f64>() {
                    if val < self.bounds.float_min || val > self.bounds.float_max {
                        return Err(TypeSystemError::ConstraintViolation {
                            constraint: "float bounds".to_string(),
                            reason: format!("Value {} exceeds bounds [{}, {}]", val, self.bounds.float_min, self.bounds.float_max),
                            span: location,
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::TypeChecker;
    use crate::parser::expr::Expression;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    #[test]
    fn expr_as_i128_peels_parsed_literal_and_unary_minus() {
        let checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let lit = crate::parser::expr::parse_expression("42").expect("parse 42");
        assert!(matches!(lit, Expression::Spanned { .. }));
        assert_eq!(TypeChecker::expr_as_i128(&lit), Some(42));
        assert_eq!(checker.numeric_literal_fits(&lit, &crate::types::definition::Type::int()), true);

        let neg = crate::parser::expr::parse_expression("-1").expect("parse -1");
        assert!(matches!(neg, Expression::Spanned { .. }));
        assert_eq!(TypeChecker::expr_as_i128(&neg), Some(-1));
    }
}
