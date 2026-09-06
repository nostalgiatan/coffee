use super::*;
use super::super::definition::{Span, Type};
use crate::types::errors::TypeSystemError;

/// Import information for circular dependency checking
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// Module being imported
    pub module: String,
}

/// Current import stack for circular detection
#[derive(Debug, Clone)]
pub struct ImportStack {
    /// Stack of imports
    stack: Vec<ImportInfo>,
}

impl ImportStack {
    pub fn new() -> Self {
        ImportStack { stack: Vec::new() }
    }

    /// Push an import onto the stack
    pub fn push(&mut self, import: ImportInfo) -> Result<(), TypeSystemError> {
        // Check for circular imports
        for existing in &self.stack {
            if existing.module == import.module {
                let mut path: Vec<String> = self.stack.iter().map(|i| i.module.clone()).collect();
                path.push(import.module.clone());

                return Err(TypeSystemError::Cycle { path });
            }
        }
        self.stack.push(import);
        Ok(())
    }

    /// Get current path
    pub fn path(&self) -> Vec<String> {
        self.stack.iter().map(|i| i.module.clone()).collect()
    }
}

use crate::coffee_debug;
use crate::parser;

impl super::TypeChecker {
    /// Record an import for cycle detection. Module existence and `.cf` loading
    /// are the pipeline's job (`ProjectBuilder` / `CompilationPipeline`), not the
    /// type checker's: this function does not open files.
    pub fn check_import(&mut self, module: &str) -> Result<(), TypeSystemError> {
        if self.mode != CheckingMode::Comprehensive {
            return Ok(());
        }

        let import_info = ImportInfo {
            module: module.to_string(),
        };

        if let Ok(mut stack) = self.import_stack.write() {
            if !stack.path().iter().any(|m| m == module) {
                stack.push(import_info)?;
            }
        }

        Ok(())
    }

    /// Check main entry point arguments
    pub fn check_main_entry(&mut self, main_entry: &parser::main::MainEntry) -> Result<(), TypeSystemError> {
        coffee_debug!("[DEBUG] check_main_entry: checking main entry '{}'", main_entry.entry_function);
        
        // Find the function type
        let function_type = {
            let registry_result = self.registry.read();
            match registry_result {
                Ok(reg) => {
                    coffee_debug!("[DEBUG] check_main_entry: accessing registry");
                    if let Ok(func_type) = reg.resolve_type(&main_entry.entry_function) {
                        coffee_debug!("[DEBUG] check_main_entry: found function '{}' in registry", main_entry.entry_function);
                        Ok(func_type)
                    } else {
                        coffee_debug!("[DEBUG] check_main_entry: function '{}' not found in registry", main_entry.entry_function);
                        Err("not_found")
                    }
                }
                Err(_) => {
                    coffee_debug!("[DEBUG] check_main_entry: failed to access registry");
                    Err("registry_error")
                }
            }
        };

        let param_types = match function_type {
            Ok(Type::Function { params, return_type: _ }) => {
                coffee_debug!("[DEBUG] check_main_entry: function has {} parameters", params.len());
                Ok(params)
            },
            Ok(_) => {
                coffee_debug!("[DEBUG] check_main_entry: '{}' is not a function", main_entry.entry_function);
                let error = TypeSystemError::ParseError {
                    type_str: main_entry.entry_function.clone(),
                    reason: format!("'{}' is not a function", main_entry.entry_function),
                };
                self.add_error(error.clone());
                return Err(error);
            },
            Err("not_found") => {
                // Try to find function in semantic analyzer's C symbols
                coffee_debug!("[DEBUG] check_main_entry: trying to find '{}' in C symbols", main_entry.entry_function);
                let mut c_sig: Option<(Vec<String>, String)> = None;
                if let Some(ref analyzer) = self.analyzer {
                    if let Ok(analyzer) = analyzer.read() {
                        if let Ok(cfc_symbols) = analyzer.get_cfc_symbols() {
                            coffee_debug!("[DEBUG] check_main_entry: found {} C symbol tables", cfc_symbols.len());
                            for (lib_name, symbol_table) in cfc_symbols.iter() {
                                coffee_debug!("[DEBUG] check_main_entry: checking library '{}', {} symbols", lib_name, symbol_table.symbols.len());
                                if let Some(c_symbol) = symbol_table.symbols.get(&main_entry.entry_function) {
                                    c_sig = Some((
                                        c_symbol
                                            .parameters
                                            .iter()
                                            .map(|p| p.param_type.clone())
                                            .collect(),
                                        c_symbol.return_type.clone(),
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
                let mut c_param_types = None;
                if let Some((param_strs, ret_str)) = c_sig {
                    let mut param_types = Vec::new();
                    for param_type in &param_strs {
                        let resolved = {
                            let reg = self.registry.read().unwrap();
                            reg.resolve_type(param_type)
                        };
                        match resolved {
                            Ok(param_type) => param_types.push(param_type),
                            Err(e) => {
                                self.add_error(e.clone());
                                return Err(e);
                            }
                        }
                    }
                    let return_type = {
                        let reg = self.registry.read().unwrap();
                        reg.resolve_type(&ret_str)
                    };
                    let return_type = match return_type {
                        Ok(ty) => ty,
                        Err(e) => {
                            self.add_error(e.clone());
                            return Err(e);
                        }
                    };
                    coffee_debug!("[DEBUG] check_main_entry: found C function '{}' with {} parameters, return type {:?}", main_entry.entry_function, param_types.len(), return_type);
                    c_param_types = Some(param_types);
                }

                match c_param_types {
                    Some(param_types) => Ok(param_types),
                    None => {
                        coffee_debug!("[DEBUG] check_main_entry: function '{}' not found in C symbols", main_entry.entry_function);
                        let error = TypeSystemError::undefined_function(&main_entry.entry_function, Span::new(0, main_entry.entry_function.len()));
                        self.add_error(error.clone());
                        Err(error)
                    }
                }
            },
            Err(_) => {
                let error = TypeSystemError::ParseError {
                    type_str: main_entry.entry_function.clone(),
                    reason: "Failed to access registry".to_string(),
                };
                self.add_error(error.clone());
                Err(error)
            }
        };

        match param_types {
            Ok(param_types) => {
                coffee_debug!("[DEBUG] check_main_entry: found function with {} parameters", param_types.len());
                self.check_main_args(&main_entry.args, &param_types, &main_entry.entry_function)
            },
            Err(error) => Err(error)
        }
    }

    /// Check main entry point arguments against parameter types
    pub(super) fn check_main_args(&mut self, args: &[crate::parser::expr::Expression], param_types: &[Type], entry_function: &str) -> Result<(), TypeSystemError> {
        coffee_debug!("[DEBUG] check_main_args: checking {} args against {} param_types", args.len(), param_types.len());
        if args.len() != param_types.len() {
            let error = TypeSystemError::arity_mismatch(
                param_types.len(),
                args.len(),
                TypeSystemError::span_for_name(entry_function),
            );
            self.add_error(error.clone());
            return Err(error);
        }

        for (i, (arg, expected_type)) in args.iter().zip(param_types.iter()).enumerate() {
            coffee_debug!("[DEBUG] check_main_args: checking arg {} '{}' vs expected type {:?}", i, arg, expected_type);
            let arg_type = match self.check_expression(arg) {
                Ok(ty) => ty,
                Err(e) => {
                    self.add_error(e.clone());
                    return Err(e);
                }
            };
            coffee_debug!("[DEBUG] check_main_args: inferred arg type as {:?}", arg_type);
            if !self.types_compatible(&arg_type, expected_type)? {
                coffee_debug!("[DEBUG] check_main_args: argument type mismatch at position {}", i);
                let error = TypeSystemError::TypeMismatch {
                    expected: expected_type.clone(),
                    found: arg_type,
                    span: TypeSystemError::span_for_name(entry_function),
                };
                self.add_error(error.clone());
                return Err(error);
            }
        }

        coffee_debug!("[DEBUG] check_main_args: all arguments checked successfully");
        Ok(())
    }
}
