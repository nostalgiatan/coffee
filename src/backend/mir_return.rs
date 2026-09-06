//! MIR `return` from [`HirExpr`], without `Statement::Return`.

use crate::backend::codegen::CodeGenerator;
use crate::hir::{HirExpr, HirExprKind};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn compile_mir_return(&mut self, value: Option<&HirExpr>) -> Result<(), String> {
        if let Some(expr) = value {
            if let HirExprKind::Variable(var_name) = &expr.kind {
                self.memory_ctx.record_use(var_name);
                if let Some(info) = self.memory_ctx.lifetimes.get_mut(var_name) {
                    info.properly_cleaned = true;
                }
            }

            let value = self.compile_hir_expr(expr)?;

            if let HirExprKind::Variable(var_name) = &expr.kind {
                if self.ty_is_owned_resource(&expr.ty) {
                    self.memory_ctx.mark_moved(var_name.clone());
                }
            }

            if let Some(current_fn) = self.current_function {
                let return_type = current_fn.get_type().get_return_type();
                if let Some(ret_type) = return_type {
                    let value_type = value.get_type();
                    if !self.are_types_compatible(value_type, ret_type) {
                        let value_type_str = self.type_to_string(value_type);
                        let ret_type_str = self.type_to_string(ret_type);
                        let fn_name = current_fn.get_name().to_str().unwrap_or("unknown");
                        return Err(self.error("return_statement",
                            format!("type mismatch in return statement of function '{}'\n  = note: expected return type '{}', found type '{}'\n  = note: these types are incompatible and cannot be implicitly converted\n  = help: ensure the return expression matches the function's declared return type",
                                fn_name, ret_type_str, value_type_str)));
                    }
                    let converted_value = self.convert_value_to_type(value, ret_type, "return_val")?;
                    self.emit_local_drops()?;
                    self.backend.builder.build_return(Some(&converted_value))
                        .map_err(|e| e.to_string())?;
                } else {
                    self.emit_local_drops()?;
                    self.backend.builder.build_return(Some(&value))
                        .map_err(|e| e.to_string())?;
                }
            } else {
                self.emit_local_drops()?;
                self.backend.builder.build_return(Some(&value))
                    .map_err(|e| e.to_string())?;
            }
        } else {
            self.emit_local_drops()?;
            self.backend.builder.build_return(None)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
