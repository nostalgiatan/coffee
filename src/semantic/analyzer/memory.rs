use super::SemanticAnalyzer;
use crate::types::definition::*;
use crate::types::errors::TypeSystemError;

impl SemanticAnalyzer {
    pub(super) fn analyze_memory_op(&mut self, op: &crate::parser::MemoryOp) -> Result<(), TypeSystemError> {
        use crate::parser::expr::Expression;
        match op {
            crate::parser::MemoryOp::Clone { source, target }
            | crate::parser::MemoryOp::Move { source, target } => {
                self.analyze_expression(&Expression::Variable(source.clone()))?;
                let ty = self.infer_expression_type(&Expression::Variable(source.clone()))?;
                self.bind_memory_target(target, ty)
            }
            crate::parser::MemoryOp::Remove { target } => {
                self.analyze_expression(&Expression::Variable(target.clone()))
            }
            crate::parser::MemoryOp::RemoveMultiple { targets } => {
                for name in targets {
                    self.analyze_expression(&Expression::Variable(name.clone()))?;
                }
                Ok(())
            }
            crate::parser::MemoryOp::Copy { .. } => Err(TypeSystemError::ParseError {
                type_str: "copy".to_string(),
                reason: "copy was removed; use clone for an independent value or mv to move"
                    .to_string(),
            }),
            crate::parser::MemoryOp::CleanOut { .. } => Err(TypeSystemError::ParseError {
                type_str: "clean out".to_string(),
                reason: "clean out was removed; values drop at scope end, use rm only to drop early"
                    .to_string(),
            }),
        }
    }

    fn bind_memory_target(&mut self, name: &str, ty: Type) -> Result<(), TypeSystemError> {
        let span = Span::new(0, name.len());
        let symbol_name = if let Some(ref func_name) = self.current_function_name {
            format!("{}::{}", func_name, name)
        } else {
            name.to_string()
        };
        if let Ok(mut symbols) = self.symbol_space.write() {
            if symbols.lookup(&symbol_name).is_some() || symbols.lookup(name).is_some() {
                return Ok(());
            }
            let binding = Binding {
                name: symbol_name.clone(),
                entity: Entity::Variable {
                    ty,
                    initialized: true,
                },
                span,
                mutable: true,
                visibility: Visibility::Private,
            };
            symbols.declare(symbol_name, binding)?;
        }
        Ok(())
    }

    pub(super) fn bind_ephemeral_var(&mut self, name: &str) -> Result<(), TypeSystemError> {
        let symbol_name = if let Some(ref func_name) = self.current_function_name {
            format!("{}::{}", func_name, name)
        } else {
            name.to_string()
        };
        if let Ok(mut symbols) = self.symbol_space.write() {
            if symbols.lookup(&symbol_name).is_some() || symbols.lookup(name).is_some() {
                return Ok(());
            }
            let binding = Binding {
                name: symbol_name.clone(),
                entity: Entity::Variable {
                    ty: Type::int(),
                    initialized: true,
                },
                span: Span::new(0, name.len()),
                mutable: true,
                visibility: Visibility::Private,
            };
            symbols.declare(symbol_name, binding)?;
        }
        Ok(())
    }

    pub(super) fn unbind_ephemeral_var(&mut self, name: &str) {
        if let Ok(mut symbols) = self.symbol_space.write() {
            if let Some(ref func_name) = self.current_function_name {
                symbols.remove(&format!("{}::{}", func_name, name));
            }
            symbols.remove(name);
        }
    }

    pub(super) fn pattern_binding_names(pattern: &crate::parser::Pattern) -> Vec<String> {
        let mut names = Vec::new();
        Self::collect_pattern_binding_names(pattern, &mut names);
        names
    }

    fn collect_pattern_binding_names(pattern: &crate::parser::Pattern, names: &mut Vec<String>) {
        match pattern {
            crate::parser::Pattern::Ident(name) => names.push(name.clone()),
            crate::parser::Pattern::Tuple(elems) | crate::parser::Pattern::Or(elems) => {
                for el in elems {
                    Self::collect_pattern_binding_names(el, names);
                }
            }
            crate::parser::Pattern::Struct { fields, .. } => {
                for (_, pat) in fields {
                    Self::collect_pattern_binding_names(pat, names);
                }
            }
            crate::parser::Pattern::EnumVariant { args, .. } => {
                for arg in args {
                    Self::collect_pattern_binding_names(arg, names);
                }
            }
            crate::parser::Pattern::Wildcard | crate::parser::Pattern::Literal(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::SemanticAnalyzer;
    use crate::parser::MemoryOp;
    use crate::types::registry::TypeRegistry;
    use std::sync::{Arc, RwLock};

    fn analyzer() -> SemanticAnalyzer {
        SemanticAnalyzer::new(Arc::new(RwLock::new(TypeRegistry::root())))
    }

    #[test]
    fn copy_is_semantic_error() {
        let mut a = analyzer();
        let err = a
            .analyze_memory_op(&MemoryOp::Copy {
                source: "x".into(),
                target: "y".into(),
            })
            .expect_err("copy must not succeed in semantic analysis");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("copy") && (msg.contains("removed") || msg.contains("clone")),
            "unexpected copy error: {err}"
        );
    }

    #[test]
    fn clean_out_is_semantic_error() {
        let mut a = analyzer();
        for op in [
            MemoryOp::CleanOut {
                targets: None,
                except_mode: false,
            },
            MemoryOp::CleanOut {
                targets: Some(vec!["x".into()]),
                except_mode: true,
            },
            MemoryOp::CleanOut {
                targets: Some(vec!["x".into()]),
                except_mode: false,
            },
        ] {
            let err = a
                .analyze_memory_op(&op)
                .expect_err("clean out must not succeed in semantic analysis");
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains("clean out") || msg.contains("clean"),
                "unexpected clean out error: {err}"
            );
        }
    }
}
