use super::super::*;
use super::super::super::definition::Type;
use crate::types::errors::TypeSystemError;

impl super::super::TypeChecker {
    pub(in super::super) fn check_addr_of(
        &mut self,
        operand: &crate::parser::expr::Expression,
        mutable: bool,
    ) -> Result<Type, TypeSystemError> {
        self.check_index_exprs_in_place(operand)?;
        let place = match self.expr_to_place(operand) {
            Ok(place) => self.mark_c_union_overlay(place),
            Err(_) => {
                let op = if mutable { "&mut" } else { "&" };
                return Err(TypeSystemError::BorrowViolation {
                    variable: String::new(),
                    reason: format!("can only borrow a local variable with {}", op),
                    span: TypeSystemError::span_for_name(op),
                });
            }
        };
        let place_ty = self.lookup_place_type(&place)?;
        self.borrow.acquire_place(&place, mutable)?;
        let root = place.root_var().to_string();
        if let Some(info) = self.values.get_mut(&root) {
            if matches!(info.state, ValueState::Alive | ValueState::Borrowed { .. }) {
                info.state = ValueState::Borrowed { immutable: !mutable };
            }
        }
        Ok(Type::Ref {
            elem: Box::new(place_ty),
            mutable,
        })
    }

    fn mark_c_union_overlay(&self, place: crate::types::borrow::Place) -> crate::types::borrow::Place {
        use crate::types::borrow::Place;
        match place {
            Place::Field {
                base,
                field,
                union_overlay,
            } => {
                let base = self.mark_c_union_overlay(*base);
                let overlay = union_overlay
                    || matches!(
                        self.lookup_place_type(&base),
                        Ok(Type::NamedType { name }) if self
                            .registry
                            .read()
                            .ok()
                            .map(|r| r.is_c_union(&name))
                            .unwrap_or(false)
                    );
                if overlay {
                    base.union_field(field)
                } else {
                    base.field(field)
                }
            }
            Place::Index { base } => self.mark_c_union_overlay(*base).index(),
            other => other,
        }
    }

    fn expr_to_place(
        &self,
        expr: &crate::parser::expr::Expression,
    ) -> Result<crate::types::borrow::Place, TypeSystemError> {
        use crate::parser::expr::Expression;
        use crate::types::borrow::Place;
        match expr.kind() {
            Expression::Variable(name) => Ok(Place::var(name)),
            Expression::Member { object, field, args } if args.is_empty() => {
                let base = self.expr_to_place(object)?;
                let overlay = match self.lookup_place_type(&base) {
                    Ok(Type::NamedType { name }) => self
                        .registry
                        .read()
                        .ok()
                        .map(|r| r.is_c_union(&name))
                        .unwrap_or(false),
                    _ => false,
                };
                if overlay {
                    Ok(base.union_field(field.clone()))
                } else {
                    Ok(base.field(field.clone()))
                }
            }
            Expression::Index { array, .. } => Ok(self.expr_to_place(array)?.index()),
            _ => {
                let reason = "can only borrow a local variable";
                Err(TypeSystemError::BorrowViolation {
                    variable: String::new(),
                    reason: reason.to_string(),
                    span: TypeSystemError::span_for_name(reason),
                })
            }
        }
    }

    fn lookup_place_type(&self, place: &crate::types::borrow::Place) -> Result<Type, TypeSystemError> {
        use crate::types::borrow::Place;
        match place {
            Place::Var(name) => self.lookup_place(name),
            Place::Field { base, field, .. } => {
                let base_ty = self.lookup_place_type(base)?;
                let class_name = match &base_ty {
                    Type::NamedType { name } => name.clone(),
                    _ => {
                        let op = format!(".{}", field);
                        return Err(TypeSystemError::invalid_operation(
                            op.clone(),
                            base_ty.clone(),
                            base_ty,
                            TypeSystemError::span_for_name(&op),
                        ));
                    }
                };
                self.lookup_field_type(&class_name, field)
            }
            Place::Index { base } => {
                let base_ty = self.lookup_place_type(base)?;
                match base_ty {
                    Type::Array { elem, .. } => Ok(*elem),
                    Type::Slice(elem) => Ok(*elem),
                    other => Err(TypeSystemError::invalid_operation(
                        "[]",
                        other.clone(),
                        other,
                        TypeSystemError::span_for_name("[]"),
                    )),
                }
            }
        }
    }

    fn check_index_exprs_in_place(
        &mut self,
        expr: &crate::parser::expr::Expression,
    ) -> Result<(), TypeSystemError> {
        use crate::parser::expr::Expression;
        match expr.kind() {
            Expression::Index { array, index } => {
                self.check_index_exprs_in_place(array)?;
                let index_type = self.check_expression(index)?;
                if !index_type.is_int() {
                    return Err(TypeSystemError::TypeMismatch {
                        expected: Type::int(),
                        found: index_type.clone(),
                        span: TypeSystemError::span_for_name(&index_type.to_string()),
                    });
                }
                Ok(())
            }
            Expression::Member { object, args, .. } if args.is_empty() => {
                self.check_index_exprs_in_place(object)
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::TypeChecker;
    use crate::parser::expr::Expression;
    use crate::types::errors::TypeSystemError;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    #[test]
    fn expr_to_place_peels_parsed_variable() {
        let checker = TypeChecker::simple(Arc::new(RwLock::new(TypeRegistry::root())));
        let parsed = crate::parser::expr::parse_expression("x").expect("parse x");
        assert!(matches!(parsed, Expression::Spanned { .. }));
        let from_parsed = checker.expr_to_place(&parsed).expect("Spanned Variable is a place");
        let from_bare = checker
            .expr_to_place(&Expression::Variable("x".to_string()))
            .expect("bare Variable is a place");
        assert_eq!(from_parsed, from_bare);
        assert_eq!(from_parsed, crate::types::borrow::Place::var("x"));
    }

    #[test]
    fn expr_to_place_non_place_span_covers_reason() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let checker = TypeChecker::simple(registry);
        let err = checker
            .expr_to_place(&Expression::Literal("1".to_string()))
            .expect_err("literals are not borrow places");
        match err {
            TypeSystemError::BorrowViolation { reason, span, .. } => {
                assert_eq!(span, TypeSystemError::span_for_name(&reason));
            }
            other => panic!("expected BorrowViolation, got {other:?}"),
        }
    }

    fn bind_int(checker: &mut TypeChecker) {
        checker
            .check_variable_decl(&crate::parser::var::VariableDecl {
                name: "x".to_string(),
                var_type: "int".to_string(),
                value: Expression::Literal("1".to_string()),
            })
            .expect("let x:int = 1");
    }

    #[test]
    fn addr_of_field_on_int_reports_operand_type_on_both_sides() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        bind_int(&mut checker);
        let expr = Expression::Unary {
            op: "&".to_string(),
            operand: Box::new(Expression::Member {
                object: Box::new(Expression::Variable("x".to_string())),
                field: "foo".to_string(),
                args: vec![],
            }),
        };
        let err = checker.check_expression(&expr).unwrap_err();
        match err {
            TypeSystemError::InvalidOperation { op, left, right, .. } => {
                assert_eq!(op, ".foo");
                assert_eq!(left, crate::types::definition::Type::int());
                assert_eq!(right, crate::types::definition::Type::int());
            }
            other => panic!("expected InvalidOperation, got {other:?}"),
        }
    }

    #[test]
    fn addr_of_index_on_int_reports_operand_type_on_both_sides() {
        let registry = Arc::new(RwLock::new(TypeRegistry::root()));
        let mut checker = TypeChecker::comprehensive(registry);
        bind_int(&mut checker);
        let expr = Expression::Unary {
            op: "&".to_string(),
            operand: Box::new(Expression::Index {
                array: Box::new(Expression::Variable("x".to_string())),
                index: Box::new(Expression::Literal("0".to_string())),
            }),
        };
        let err = checker.check_expression(&expr).unwrap_err();
        match err {
            TypeSystemError::InvalidOperation { op, left, right, .. } => {
                assert_eq!(op, "[]");
                assert_eq!(left, crate::types::definition::Type::int());
                assert_eq!(right, crate::types::definition::Type::int());
            }
            other => panic!("expected InvalidOperation, got {other:?}"),
        }
    }
}
