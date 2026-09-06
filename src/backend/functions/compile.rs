use crate::coffee_debug;
use crate::parser::Function;
use crate::parser::function::FunctionBody;
use crate::backend::memory_ops::DiagSeverity;
use crate::backend::codegen::CodeGenerator;
use crate::types::Type;
use inkwell::values::BasicValueEnum;
use super::build_implicit_return;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub fn compile_function(&mut self, func: &Function) -> Result<(), String> {
        coffee_debug!("DEBUG: compile_function: START compiling function '{}', parameters: {:?}", func.name, func.parameters.iter().map(|p| &p.name).collect::<Vec<_>>());
        let function = *self.functions.get(&func.name)
            .ok_or_else(|| self.error("compile_function",
                format!("function '{}' not declared - this is an internal compiler error", func.name)))?;

        // For external declarations, don't generate any function body
        if matches!(func.body, FunctionBody::External) {
            return Ok(());
        }

        let parent_fn = self.current_function;
        let parent_block = self.backend.builder.get_insert_block();
        // Nested `fn` needs a fresh locals map; `take` is O(1) vs cloning every HashMap.
        let parent_vars = std::mem::take(&mut self.variables);
        let parent_vtypes = std::mem::take(&mut self.variable_types);
        // Parent used-set stays until body compile (not cleared at function start).
        let parent_used = self.used_variables.clone();
        let parent_array_allocas = std::mem::take(&mut self.array_allocas);
        let parent_array_sizes = std::mem::take(&mut self.array_sizes);
        let parent_array_lengths = std::mem::take(&mut self.array_lengths);
        let parent_array_elem_types = std::mem::take(&mut self.array_element_types);
        let parent_stack = self.current_stack_size;
        let parent_entry = self.entry_successor;
        let parent_errh = std::mem::take(&mut self.current_error_handler);
        let parent_fn_name = std::mem::take(&mut self.current_function_name);
        let parent_params = std::mem::take(&mut self.current_function_params);

        let result = (|| -> Result<(), String> {
        self.current_function = Some(function);
        if let FunctionBody::Block(body) = &func.body {
            self.register_nested_asts(body, &func.name);
        }

        // 进入新的函数作用域
        self.memory_ctx.enter_scope();

        // 初始化生命周期跟踪
        self.memory_ctx.set_function(func.name.clone());

        // Set error handler for this function
        self.current_error_handler = func.error_handler.clone();
        self.current_function_name = Some(func.name.clone());
        self.current_function_params = func.parameters.iter().map(|p| p.name.clone()).collect();

        // Create entry block
        let entry = self.backend.context.append_basic_block(function, "entry");
        self.backend.builder.position_at_end(entry);

        // Locals maps were `take`n above (empty). Reset lifetime tracking.
        self.memory_ctx.clear_all();

        // CRITICAL-5 FIX: Reset stack size tracking for each function
        self.current_stack_size = 0;

        // Allocate parameters
        for (i, param) in func.parameters.iter().enumerate() {
            let param_value = function.get_nth_param(i as u32)
                .ok_or_else(|| self.error("compile_function",
                    format!("missing parameter {} '{}' in function '{}' - internal error",
                        i, param.name, func.name)))?;

            let param_type = self.coffee_type_to_llvm(&param.param_type)
                .map_err(|e| self.error("compile_function",
                    format!("failed to convert type '{}' for parameter '{}': {}",
                        param.param_type, param.name, e)))?;
            let alloca = self.backend.builder.build_alloca(param_type, &param.name)
                .map_err(|e| self.error("compile_function",
                    format!("failed to allocate parameter '{}': {}", param.name, e)))?;

            self.backend.builder.build_store(alloca, param_value)
                .map_err(|e| self.error("compile_function",
                    format!("failed to store parameter '{}': {}", param.name, e)))?;

            coffee_debug!("DEBUG: compile_function: inserting parameter '{}' into variables", param.name);
            self.variables.insert(param.name.clone(), (alloca, param_type));
            coffee_debug!("DEBUG: compile_function: variables after inserting parameter '{}': {:?}", param.name, self.variables.keys().collect::<Vec<_>>());
            if self.classes.contains_key(&param.param_type) {
                self.variable_types.insert(param.name.clone(), param.param_type.clone());
            }
            match crate::types::type_from_str(&param.param_type) {
                Ok(Type::Array { elem, size }) => {
                    let elem_llvm = self.coffee_type_to_llvm(&elem.to_string())?;
                    let BasicValueEnum::PointerValue(array_ptr) = param_value else {
                        return Err(self.error("compile_function",
                            format!("array parameter '{}' must be a pointer", param.name)));
                    };
                    self.array_allocas.insert(param.name.clone(), array_ptr);
                    self.array_sizes.insert(param.name.clone(), size as u32);
                    self.array_element_types.insert(param.name.clone(), elem_llvm);
                }
                Ok(Type::Slice(elem)) => {
                    let elem_llvm = self.coffee_type_to_llvm(&elem.to_string())?;
                    self.bind_slice_fat(&param.name, param_value, elem_llvm, true)?;
                    self.memory_ctx
                        .set_variable_type(param.name.clone(), param.param_type.clone());
                }
                _ => {}
            }
        }

        if !self.compile_mir_for_function(func)? {
            return Err(self.error(
                "compile_function",
                format!("missing MIR for function '{}'", func.name),
            ));
        }

        // Add default return if the MIR body left the insert block open.
        let has_terminator = self.backend.builder.get_insert_block()
            .map(|b| b.get_terminator().is_some())
            .unwrap_or(false);
        if !has_terminator {
            self.emit_local_drops()
                .map_err(|e| self.error("compile_function",
                    format!("failed to auto-drop locals: {}", e)))?;
            build_implicit_return(&self.backend.builder, function)
                .map_err(|e| self.error("compile_function",
                    format!("failed to build default return: {}", e)))?;
        }

        // Ensure entry block has a terminator before finishing
        let entry_block = function.get_first_basic_block().unwrap();
        if entry_block.get_terminator().is_none() {
            return Err(self.error("compile_function",
                "entry block lacks terminator - function body doesn't return or branch properly"));
        }

        // HIGH-10 FIX: Check for unused variables before function ends
        // Collect all declared variables (parameters + locals)
        let _all_vars: std::collections::HashSet<String> = self.variables.keys().cloned().collect();

        // 运行生命周期检查，并按 severity 分流：
        //   Error   (M006/M007，真正的 UB)        → 编译失败
        //   Warning (M001/M002/M010/M011/M012/M013) → 黄色警告，不阻断编译
        //   Silent  (M003/M004/M009，已退役的 must-rm 规则) → 丢弃
        //   自动 drop 接管后，"必须显式 rm" 的前提已不复存在。
        let lifetime_diagnostics = self.memory_ctx.run_lifetime_checks()
            .map_err(|e| self.error("compile_function",
                format!("lifetime check failed: {}", e)))?;

        let mut error_messages = Vec::new();
        for diag in lifetime_diagnostics {
            // 显式使用 var_name 字段以确保它被使用
            let _var = &diag.var_name;
            match diag.severity() {
                DiagSeverity::Error => {
                    error_messages.push(format!(
                        "\x1b[1;35m[{}]\x1b[0m: {}\n    \x1b[0;36m| help: {}\x1b[0m",
                        diag.code, diag.message, diag.hint
                    ));
                }
                DiagSeverity::Warning => {
                    eprintln!("\x1b[1;33m[Warning {}]\x1b[0m: {}\n    \x1b[0;36m| help: {}\x1b[0m",
                        diag.code, diag.message, diag.hint);
                }
                DiagSeverity::Silent => {
                    // 已退役的诊断，丢弃
                }
            }
        }

        // 只有真正的内存安全 UB（M006 use-after-free / M007 use-after-move）
        // 才拒绝编译——这些无法被自动 drop 修复。
        if !error_messages.is_empty() {
            return Err(self.error("compile_function",
                format!("\x1b[1;35mMemory safety check failed\x1b[0m\n\n{}\n  \x1b[0;36m= note: use-after-free / use-after-move are undefined behavior and cannot be auto-fixed by drop\x1b[0m",
                    error_messages.join("\n\n"))));
        }

        // Clear used variables for next function
        self.used_variables.clear();
        Ok(())
        })();

        self.memory_ctx.exit_scope();
        self.current_function = parent_fn;
        if let Some(bb) = parent_block {
            self.backend.builder.position_at_end(bb);
        }
        self.variables = parent_vars;
        self.variable_types = parent_vtypes;
        self.used_variables = parent_used;
        self.array_allocas = parent_array_allocas;
        self.array_sizes = parent_array_sizes;
        self.array_lengths = parent_array_lengths;
        self.array_element_types = parent_array_elem_types;
        self.current_stack_size = parent_stack;
        self.entry_successor = parent_entry;
        self.current_error_handler = parent_errh;
        self.current_function_name = parent_fn_name;
        self.current_function_params = parent_params;
        result
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::{codegen::CodeGenerator, Backend};
    use crate::parser::expr::Expression;
    use crate::parser::function::{Function, FunctionBody};
    use crate::parser::var::ReturnStmt;
    use crate::parser::Statement;
    use inkwell::context::Context;

    fn int_main() -> Function {
        Function {
            type_params: vec![],
            name: "main".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![Statement::Return(ReturnStmt {
                value: Some(Expression::literal("0")),
            })]),
            is_c: false,
        }
    }

    #[test]
    fn compile_function_without_mir_is_error() {
        let context = Context::create();
        let backend = Backend::new(&context, "missing_mir");
        let mut cg = CodeGenerator::new(&backend);
        let func = int_main();
        cg.declare_function(&func).unwrap();
        let err = cg.compile_function(&func).expect_err("AST body fallback is forbidden");
        assert!(
            err.contains("missing MIR") || err.contains("MIR"),
            "expected missing-MIR error, got: {}",
            err
        );
    }

    #[test]
    fn compile_external_function_without_mir_is_ok() {
        let context = Context::create();
        let backend = Backend::new(&context, "ext_mir");
        let mut cg = CodeGenerator::new(&backend);
        let func = Function {
            type_params: vec![],
            name: "puts".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::External,
            is_c: true,
        };
        cg.declare_function(&func).unwrap();
        cg.compile_function(&func).expect("external fn skips MIR body");
    }
}
