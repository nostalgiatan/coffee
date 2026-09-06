use super::*;
use super::super::definition::Type;
use crate::types::errors::TypeSystemError;

impl super::TypeChecker {
    /// Check memory operation (comprehensive mode only)
    ///
    /// Coffee language memory operations:
    /// - clone x y: Deep copy - creates independent copy with new lifetime
    /// - copy x y: Shallow copy - creates shared reference to same data
    /// - mv x y: Move - transfers ownership (x becomes invalid, y takes over)
    /// - rm x: Remove - deletes variable and frees resources
    pub fn check_memory_operation(&mut self, var_name: &str, operation: &str) -> Result<(), TypeSystemError> {
        if self.mode != CheckingMode::Comprehensive {
            return Ok(());
        }

        let location = Span::new(0, var_name.len());

        match operation {
            "move" | "mv" => {
                self.borrow.deny_move(var_name)?;
                if self.values.get(var_name).map(|v| matches!(v.ty, Type::Variadic)).unwrap_or(false) {
                    return Ok(());
                }
                // Move operation: source becomes invalid
                if let Some(value_info) = self.values.get_mut(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot move '{}' - value already moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot move '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive | ValueState::Borrowed { .. } => {
                            value_info.state = ValueState::Moved;
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "clone" => {
                // Clone operation: source remains valid, creates deep copy
                // Only check that source exists and is accessible
                if let Some(value_info) = self.values.get(var_name) {
                    if value_info.ty.is_buf() {
                        return Err(TypeSystemError::OwnershipError {
                            reason: "clone buf needs a size; use a slice".to_string(),
                            span: location,
                        });
                    }
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot clone '{}' - value was moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot clone '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive | ValueState::Borrowed { .. } => {
                            // Clone is always safe - source remains valid
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "copy" => {
                // Copy operation: creates shared reference (shallow copy)
                // Source remains valid, both variables point to same data
                if let Some(value_info) = self.values.get(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot copy '{}' - value was moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot copy '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive | ValueState::Borrowed { .. } => {
                            // Copy is safe - creates shared reference
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "remove" | "rm" => {
                self.borrow.deny_move(var_name)?;
                if self.values.get(var_name).map(|v| matches!(v.ty, Type::Variadic)).unwrap_or(false) {
                    return Ok(());
                }
                // Remove operation: frees the variable
                if let Some(value_info) = self.values.get_mut(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot remove '{}' - value was already moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { .. } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot remove '{}' - value was already dropped", var_name),
                                span: location,
                            });
                        }
                        ValueState::Alive | ValueState::Borrowed { .. } => {
                            value_info.state = ValueState::Dropped {
                                location: "memory operation".to_string(),
                            };
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "remove_multiple" => {
                if self.values.get(var_name).map(|v| matches!(v.ty, Type::Variadic)).unwrap_or(false) {
                    return Ok(());
                }
                // Remove multiple operation: frees multiple variables
                // This is handled by the backend, we just need to check that the first variable exists
                // The actual removal will be done by the backend
                if !var_name.is_empty() {
                    if let Some(value_info) = self.values.get_mut(var_name) {
                        match value_info.state {
                            ValueState::Moved => {
                                return Err(TypeSystemError::OwnershipError {
                                    reason: format!("Cannot remove '{}' - value was already moved", var_name),
                                    span: location,
                                });
                            }
                            ValueState::Dropped { .. } => {
                                return Err(TypeSystemError::OwnershipError {
                                    reason: format!("Cannot remove '{}' - value was already dropped", var_name),
                                    span: location,
                                });
                            }
                            ValueState::Alive | ValueState::Borrowed { .. } => {
                                value_info.state = ValueState::Dropped {
                                    location: "memory operation".to_string(),
                                };
                            }
                        }
                    } else {
                        return Err(TypeSystemError::undefined_variable(var_name, location));
                    }
                }
            }

            "borrow" => {
                if let Some(value_info) = self.values.get_mut(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot borrow '{}' - value was moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot borrow '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive => {
                            value_info.state = ValueState::Borrowed { immutable: true };
                        }
                        ValueState::Borrowed { .. } => {
                            // Already borrowed
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            "borrow_mut" => {
                if let Some(value_info) = self.values.get_mut(var_name) {
                    match value_info.state {
                        ValueState::Moved => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot mutably borrow '{}' - value was moved", var_name),
                                span: location,
                            });
                        }
                        ValueState::Dropped { ref location } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot mutably borrow '{}' - value was dropped at {}", var_name, location),
                                span: Span::new(0, var_name.len()),
                            });
                        }
                        ValueState::Alive => {
                            value_info.state = ValueState::Borrowed { immutable: false };
                        }
                        ValueState::Borrowed { .. } => {
                            return Err(TypeSystemError::OwnershipError {
                                reason: format!("Cannot mutably borrow '{}' - already borrowed", var_name),
                                span: location,
                            });
                        }
                    }
                } else {
                    return Err(TypeSystemError::undefined_variable(var_name, location));
                }
            }

            _ => {
                return Err(TypeSystemError::OwnershipError {
                    reason: format!("Unknown memory operation: '{}'", operation),
                    span: location,
                });
            }
        }

        Ok(())
    }
    /// Check memory operation (alias for check_memory_operation for compatibility)
    pub fn check_memory_op(&mut self, op: &crate::parser::memory::MemoryOp) -> Result<(), crate::diagnostics::Diagnostic> {
        match op {
            crate::parser::memory::MemoryOp::Copy { .. } => {
                let error = TypeSystemError::ParseError {
                    type_str: "copy".to_string(),
                    reason: "copy was removed; use clone for an independent value or mv to move".to_string(),
                };
                self.add_error(error.clone());
                return Err(error.into());
            }
            crate::parser::memory::MemoryOp::CleanOut { .. } => {
                let error = TypeSystemError::ParseError {
                    type_str: "clean out".to_string(),
                    reason: "clean out was removed; values drop at scope end, use rm only to drop early".to_string(),
                };
                self.add_error(error.clone());
                return Err(error.into());
            }
            crate::parser::memory::MemoryOp::Clone { source, target } => {
                self.check_memory_operation(source, "clone")
                    .map_err(|e| -> crate::diagnostics::Diagnostic { e.into() })?;
                if let Some(ty) = self.values.get(source).map(|v| v.ty.clone()) {
                    self.bind_memory_target(target, ty);
                }
                Ok(())
            }
            crate::parser::memory::MemoryOp::Move { source, target } => {
                let ty = self.values.get(source).map(|v| v.ty.clone());
                self.check_memory_operation(source, "move")
                    .map_err(|e| -> crate::diagnostics::Diagnostic { e.into() })?;
                if let Some(ty) = ty {
                    self.bind_memory_target(target, ty);
                }
                Ok(())
            }
            crate::parser::memory::MemoryOp::Remove { target } => {
                self.check_memory_operation(target, "remove")
                    .map_err(|e| -> crate::diagnostics::Diagnostic { e.into() })
            }
            crate::parser::memory::MemoryOp::RemoveMultiple { targets } => {
                for name in targets {
                    self.check_memory_operation(name, "remove")
                        .map_err(|e| -> crate::diagnostics::Diagnostic { e.into() })?;
                }
                Ok(())
            }
        }
    }
}
