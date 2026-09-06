//! Statement compilation for Coffee compiler
//!
//! Handles compilation of various statement types including control flow,
//! memory operations, and return statements.

use crate::coffee_debug;
use super::codegen::CodeGenerator;
use crate::parser::{Statement, MemoryOp};
#[cfg(test)]
use crate::parser::expr::Expression;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile a statement
    pub fn compile_statement(&mut self, stmt: &Statement) -> Result<(), String> {
        // Update current line for lifetime tracking (simple counter approach)
        self.memory_ctx.set_line(self.memory_ctx.current_line + 1);

        coffee_debug!("DEBUG: compile_statement: stmt={:?}", stmt);

        match stmt {
            Statement::Function(func) => {
                if self.current_function.is_some() {
                    self.compile_nested_function(func)
                } else {
                    self.ensure_coffee_function_declared(func)?;
                    self.compile_function(func)
                }
            }
            Statement::Main(main_entry) => self.compile_main_entry(main_entry),
            Statement::VariableDecl(var) => {
                if self.current_function.is_some() {
                    return Err(self.error(
                        "compile_statement",
                        "let must come from MIR, not the parser AST",
                    ));
                }
                self.compile_variable_decl(var)
            }
            Statement::Assignment(_, _) => {
                self.require_current_function("assignment")?;
                Err(self.error(
                    "compile_statement",
                    "assignment must come from MIR, not the parser AST",
                ))
            }
            Statement::Class(class) => {
                // Top-level classes are compiled in the first pass of
                // `compile_program_with_imports`. Nested classes in a function
                // body are not in that list; compile them when already inside a fn.
                if self.current_function.is_some() {
                    self.compile_class(class)
                } else {
                    Ok(())
                }
            }
            Statement::Enum(enum_def) => self.compile_enum(enum_def),
            Statement::Return(_) => {
                self.require_current_function("return")?;
                Err(self.error(
                    "compile_statement",
                    "return must come from MIR, not the parser AST",
                ))
            }
            Statement::Import(_) => Ok(()), // Handled at frontend
            Statement::TypeDecl(_) => Ok(()), // Registered at frontend (Task 4)
            Statement::SingleLineComment(_) | Statement::MultiLineComment(_) => Ok(()),
            Statement::If(_) => Err(self.error(
                "compile_statement",
                "if must come from MIR, not the parser AST",
            )),
            Statement::While(_) => Err(self.error(
                "compile_statement",
                "while must come from MIR, not the parser AST",
            )),
            Statement::For(_) => Err(self.error(
                "compile_statement",
                "for must come from MIR, not the parser AST",
            )),
            Statement::Match(_) => Err(self.error(
                "compile_statement",
                "match must come from MIR, not the parser AST",
            )),
            Statement::Break(_) => {
                self.require_current_function("break")?;
                Err(self.error(
                    "compile_statement",
                    "break must come from MIR, not the parser AST",
                ))
            }
            Statement::Continue(_) => {
                self.require_current_function("continue")?;
                Err(self.error(
                    "compile_statement",
                    "continue must come from MIR, not the parser AST",
                ))
            }
            Statement::MemoryOp(_) => {
                self.require_current_function("memory operation")?;
                Err(self.error(
                    "compile_statement",
                    "memory operation must come from MIR, not the parser AST",
                ))
            }
            Statement::Raise(_) => Err(self.error(
                "compile_statement",
                "raise must come from MIR, not the parser AST",
            )),
            Statement::Expr(_) => {
                self.require_current_function("expression statement")?;
                Err(self.error(
                    "compile_statement",
                    "expression statement must come from MIR, not the parser AST",
                ))
            }
        }
    }

    fn require_current_function(&self, what: &str) -> Result<(), String> {
        if self.current_function.is_none() {
            return Err(self.error(
                "compile_statement",
                format!(
                    "{what} is not allowed at module level; executable statements belong in a function"
                ),
            ));
        }
        Ok(())
    }

    /// AST `return` is forbidden; function bodies use [`Self::compile_mir_return`].
    #[cfg(test)]
    pub(crate) fn compile_return(&mut self, _expr: Option<&Expression>) -> Result<(), String> {
        Err(self.error(
            "compile_return",
            "return must come from MIR, not the parser AST",
        ))
    }

    /// Drop remaining locals at a terminator. Skips parameters and `self`.
    pub(crate) fn emit_local_drops(&mut self) -> Result<(), String> {
        let vars_to_clean: Vec<String> = self.variables.keys()
            .filter(|name| {
                let n = name.as_str();
                n != "self"
                    && !self.current_function_params.iter().any(|p| p == n)
                    && self.memory_ctx.is_in_live_scope(n)
            })
            .cloned()
            .collect();

        for var_name in vars_to_clean {
            if let Some(info) = self.memory_ctx.lifetimes.get(&var_name) {
                if matches!(info.state, crate::backend::memory_ops::VariableState::Initialized) {
                    super::memory_ops::compile_remove(
                        self.backend.context,
                        &self.backend.builder,
                        &mut self.variables,
                        &mut self.memory_ctx,
                        &self.functions,
                        &var_name,
                        false,
                    )?;
                }
            }
        }

        let arrays: Vec<String> = self
            .array_allocas
            .keys()
            .filter(|name| {
                let n = name.as_str();
                n != "self"
                    && !self.current_function_params.iter().any(|p| p == n)
                    && self.memory_ctx.is_in_live_scope(n)
                    && !self.memory_ctx.is_dropped(n)
                    && !self.memory_ctx.is_moved(n)
            })
            .cloned()
            .collect();
        for var_name in arrays {
            if let Some(info) = self.memory_ctx.lifetimes.get(&var_name) {
                if matches!(info.state, crate::backend::memory_ops::VariableState::Initialized) {
                    self.drop_array_local(&var_name)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn drop_array_local(&mut self, name: &str) -> Result<(), String> {
        if self.memory_ctx.is_dropped(name) || self.memory_ctx.is_moved(name) {
            return Ok(());
        }
        let Some(&ptr) = self.array_allocas.get(name) else {
            return Ok(());
        };
        let Some(type_str) = self.memory_ctx.get_variable_type(name).cloned() else {
            return Ok(());
        };
        let ty = crate::types::type_from_str(&type_str).unwrap_or(crate::types::Type::NamedType {
            name: type_str.clone(),
        });
        let drop_ty = match &ty {
            crate::types::Type::Array { .. } => ty.clone(),
            crate::types::Type::Slice(_) => {
                if let Some(&(fat_ptr, fat_llvm)) = self.variables.get(name) {
                    super::memory_ops::drop_coffee_place(
                        self.backend.context,
                        &self.backend.builder,
                        &self.functions,
                        &ty,
                        fat_ptr,
                        fat_llvm,
                    )?;
                    if self
                        .memory_ctx
                        .lifetimes
                        .get(name)
                        .map(|info| info.is_heap_allocated)
                        .unwrap_or(false)
                    {
                        if let (Some(&free_fn), Some(&data)) =
                            (self.functions.get("free"), self.array_allocas.get(name))
                        {
                            let i8_ptr = self.backend.context.ptr_type(inkwell::AddressSpace::default());
                            let casted = self
                                .backend
                                .builder
                                .build_bit_cast(data, i8_ptr, "slice_buf_cast")
                                .map_err(|e| format!("drop slice: cast buffer: {e}"))?;
                            self.backend
                                .builder
                                .build_call(free_fn, &[casted.into()], "slice_buf_free")
                                .map_err(|e| format!("drop slice: free buffer: {e}"))?;
                        }
                    }
                    self.memory_ctx.mark_dropped(name.to_string());
                    return Ok(());
                }
                ty.clone()
            }
            _ => return Ok(()),
        };
        let Some(&elem_llvm) = self.array_element_types.get(name) else {
            return Ok(());
        };
        let n = match &drop_ty {
            crate::types::Type::Array { size, .. } => *size as u32,
            _ => return Ok(()),
        };
        let arr_ty: inkwell::types::BasicTypeEnum = match elem_llvm {
            inkwell::types::BasicTypeEnum::IntType(t) => t.array_type(n).into(),
            inkwell::types::BasicTypeEnum::FloatType(t) => t.array_type(n).into(),
            inkwell::types::BasicTypeEnum::PointerType(t) => t.array_type(n).into(),
            inkwell::types::BasicTypeEnum::StructType(t) => t.array_type(n).into(),
            inkwell::types::BasicTypeEnum::ArrayType(t) => t.array_type(n).into(),
            inkwell::types::BasicTypeEnum::VectorType(t) => t.array_type(n).into(),
            inkwell::types::BasicTypeEnum::ScalableVectorType(t) => t.array_type(n).into(),
        };
        super::memory_ops::drop_coffee_place(
            self.backend.context,
            &self.backend.builder,
            &self.functions,
            &drop_ty,
            ptr,
            arr_ty,
        )?;
        self.memory_ctx.mark_dropped(name.to_string());
        Ok(())
    }

    /// Compile memory operation
    pub fn compile_memory_op(&mut self, op: &MemoryOp) -> Result<(), String> {
        use super::memory_ops;
        match op {
            MemoryOp::Remove { target } if self.array_allocas.contains_key(target) => {
                self.drop_array_local(target)
            }
            MemoryOp::RemoveMultiple { targets } => {
                for t in targets {
                    if self.array_allocas.contains_key(t) {
                        self.drop_array_local(t)?;
                    } else {
                        memory_ops::compile_remove(
                            self.backend.context,
                            &self.backend.builder,
                            &mut self.variables,
                            &mut self.memory_ctx,
                            &self.functions,
                            t,
                            true,
                        )?;
                    }
                }
                Ok(())
            }
            _ => memory_ops::compile_memory_op(
                op,
                self.backend.context,
                &self.backend.builder,
                &mut self.variables,
                &mut self.memory_ctx,
                &self.functions,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Backend;
    use crate::parser::function::{Function, FunctionBody};
    use crate::parser::r#match::{MatchArm, MatchExpr};
    use crate::parser::pattern::Pattern;
    use crate::parser::{ForIterator, ForLoop, IfExpr, RaiseStmt, WhileLoop};
    use inkwell::context::Context;

    fn cg_in_fn<'a, 'ctx>(backend: &'a Backend<'ctx>) -> CodeGenerator<'a, 'ctx> {
        let mut cg = CodeGenerator::new(backend);
        let func = Function {
            type_params: vec![],
            name: "main".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![]),
            is_c: false,
        };
        cg.declare_function(&func).unwrap();
        let f = *cg.functions.get("main").unwrap();
        cg.current_function = Some(f);
        let entry = backend.context.append_basic_block(f, "entry");
        backend.builder.position_at_end(entry);
        cg
    }

    fn from_mir_err(err: &str) -> bool {
        err.contains("MIR") || err.contains("must come from")
    }

    #[test]
    fn compile_statement_match_must_come_from_mir() {
        let context = Context::create();
        let backend = Backend::new(&context, "stmt_match");
        let mut cg = cg_in_fn(&backend);
        let stmt = Statement::Match(MatchExpr {
            value: Expression::literal("1"),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard,
                guard: None,
                body: vec![],
            }],
        });
        let err = cg.compile_statement(&stmt).expect_err("AST match forbidden");
        assert!(from_mir_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_for_must_come_from_mir() {
        let context = Context::create();
        let backend = Backend::new(&context, "stmt_for");
        let mut cg = cg_in_fn(&backend);
        let stmt = Statement::For(ForLoop {
            variable: "i".into(),
            iterator: ForIterator::Range {
                start: Expression::literal("0"),
                end: Expression::literal("1"),
            },
            body: vec![],
        });
        let err = cg.compile_statement(&stmt).expect_err("AST for forbidden");
        assert!(from_mir_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_if_must_come_from_mir() {
        let context = Context::create();
        let backend = Backend::new(&context, "stmt_if");
        let mut cg = cg_in_fn(&backend);
        let stmt = Statement::If(IfExpr {
            condition: Expression::literal("true"),
            body: vec![],
            elifs: vec![],
            else_body: None,
        });
        let err = cg.compile_statement(&stmt).expect_err("AST if forbidden");
        assert!(from_mir_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_while_must_come_from_mir() {
        let context = Context::create();
        let backend = Backend::new(&context, "stmt_while");
        let mut cg = cg_in_fn(&backend);
        let stmt = Statement::While(WhileLoop {
            condition: Expression::literal("true"),
            body: vec![],
        });
        let err = cg.compile_statement(&stmt).expect_err("AST while forbidden");
        assert!(from_mir_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_raise_must_come_from_mir() {
        let context = Context::create();
        let backend = Backend::new(&context, "stmt_raise");
        let mut cg = cg_in_fn(&backend);
        let stmt = Statement::Raise(RaiseStmt {
            error_expr: Expression::literal("1"),
        });
        let err = cg.compile_statement(&stmt).expect_err("AST raise forbidden");
        assert!(from_mir_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_executable_ast_must_come_from_mir() {
        let context = Context::create();
        let backend = Backend::new(&context, "stmt_exec");
        let mut cg = cg_in_fn(&backend);
        let stmts = [
            Statement::Assignment("x".into(), Expression::literal("1")),
            Statement::Return(crate::parser::var::ReturnStmt {
                value: Some(Expression::literal("0")),
            }),
            Statement::Expr(Box::new(Expression::literal("1"))),
            Statement::VariableDecl(crate::parser::VariableDecl {
                name: "x".into(),
                var_type: "int".into(),
                value: Expression::literal("1"),
            }),
            Statement::MemoryOp(MemoryOp::Remove {
                target: "x".into(),
            }),
            Statement::Break(crate::parser::var::BreakStmt),
            Statement::Continue(crate::parser::var::ContinueStmt),
        ];
        for stmt in &stmts {
            let err = cg
                .compile_statement(stmt)
                .expect_err(&format!("AST {stmt:?} forbidden"));
            assert!(
                from_mir_err(&err),
                "expected MIR error for {:?}, got: {}",
                stmt,
                err
            );
        }
    }

    #[test]
    fn mir_gen_and_function_compile_must_not_call_compile_expr() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
        for rel in ["mir_gen.rs", "functions/compile.rs"] {
            let text = std::fs::read_to_string(root.join(rel)).unwrap();
            let mut offenders = Vec::new();
            for (i, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if code.contains("compile_expr") {
                    offenders.push(format!("{}:{}: {}", rel, i + 1, line.trim()));
                }
            }
            assert!(
                offenders.is_empty(),
                "{} must not call compile_expr:\n{}",
                rel,
                offenders.join("\n")
            );
        }
    }

    fn top_level_cg<'a, 'ctx>(backend: &'a Backend<'ctx>) -> CodeGenerator<'a, 'ctx> {
        CodeGenerator::new(backend)
    }

    fn executable_outside_fn_err(err: &str) -> bool {
        err.contains("module level")
    }

    #[test]
    fn compile_statement_assignment_outside_function_is_error() {
        let context = Context::create();
        let backend = Backend::new(&context, "tl_assign");
        let mut cg = top_level_cg(&backend);
        let err = cg
            .compile_statement(&Statement::Assignment(
                "x".into(),
                Expression::literal("1"),
            ))
            .expect_err("module-level assignment is not a fake body");
        assert!(executable_outside_fn_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_return_must_come_from_mir_without_compile_expr() {
        let context = Context::create();
        let backend = Backend::new(&context, "ast_compile_return");
        let mut cg = cg_in_fn(&backend);
        let err = cg
            .compile_return(Some(&Expression::literal("1")))
            .expect_err("AST compile_return forbidden");
        assert!(from_mir_err(&err), "got: {}", err);

        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend/statements.rs"),
        )
        .unwrap();
        let start = text
            .find("fn compile_return")
            .expect("compile_return");
        let rest = &text[start..];
        let end = rest.find("\n    pub(crate) fn emit_local_drops").unwrap_or(rest.len());
        let body = &rest[..end];
        let code = body
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !code.contains("compile_expr"),
            "compile_return must not call compile_expr"
        );
    }

    #[test]
    fn compile_statement_return_outside_function_is_error() {
        let context = Context::create();
        let backend = Backend::new(&context, "tl_return");
        let mut cg = top_level_cg(&backend);
        let err = cg
            .compile_statement(&Statement::Return(crate::parser::var::ReturnStmt {
                value: Some(Expression::literal("0")),
            }))
            .expect_err("module-level return is not a fake body");
        assert!(executable_outside_fn_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_break_outside_function_is_error() {
        let context = Context::create();
        let backend = Backend::new(&context, "tl_break");
        let mut cg = top_level_cg(&backend);
        let err = cg
            .compile_statement(&Statement::Break(crate::parser::var::BreakStmt))
            .expect_err("module-level break is not a fake body");
        assert!(executable_outside_fn_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_continue_outside_function_is_error() {
        let context = Context::create();
        let backend = Backend::new(&context, "tl_continue");
        let mut cg = top_level_cg(&backend);
        let err = cg
            .compile_statement(&Statement::Continue(crate::parser::var::ContinueStmt))
            .expect_err("module-level continue is not a fake body");
        assert!(executable_outside_fn_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_memory_op_outside_function_is_error() {
        let context = Context::create();
        let backend = Backend::new(&context, "tl_mem");
        let mut cg = top_level_cg(&backend);
        let err = cg
            .compile_statement(&Statement::MemoryOp(MemoryOp::Remove {
                target: "x".into(),
            }))
            .expect_err("module-level memory op is not a fake body");
        assert!(executable_outside_fn_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_expr_outside_function_is_error() {
        let context = Context::create();
        let backend = Backend::new(&context, "tl_expr");
        let mut cg = top_level_cg(&backend);
        let err = cg
            .compile_statement(&Statement::Expr(Box::new(Expression::literal("1"))))
            .expect_err("module-level expr is not a fake body");
        assert!(executable_outside_fn_err(&err), "got: {}", err);
    }

    #[test]
    fn compile_statement_top_level_function_is_missing_mir_not_module_junk() {
        let context = Context::create();
        let backend = Backend::new(&context, "tl_fn");
        let mut cg = top_level_cg(&backend);
        let func = Function {
            type_params: vec![],
            name: "main".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![]),
            is_c: false,
        };
        let err = cg
            .compile_statement(&Statement::Function(func))
            .expect_err("function bodies still need MIR");
        assert!(
            err.contains("missing MIR"),
            "top-level Function must still compile via compile_function; got: {}",
            err
        );
        assert!(
            !executable_outside_fn_err(&err),
            "Function is a declaration, not executable junk; got: {}",
            err
        );
    }

    #[test]
    fn compile_statement_top_level_import_main_class_enum_ok() {
        let context = Context::create();
        let backend = Backend::new(&context, "tl_decls");
        let mut cg = top_level_cg(&backend);
        cg.compile_statement(&Statement::Import(crate::parser::Import::Simple {
            path: "m".into(),
        }))
        .expect("Import");
        cg.compile_statement(&Statement::Main(crate::parser::MainEntry::new(
            "entry".into(),
            vec![],
        )))
        .expect("Main");
        cg.compile_statement(&Statement::Class(crate::parser::class::ClassDef {
            type_params: vec![],
            name: "C".into(),
            parent: None,
            fields: vec![],
            methods: vec![],
            packed: false,
            has_constructor: false,
        }))
        .expect("Class");
        cg.compile_statement(&Statement::Enum(crate::parser::class::EnumDef {
            name: "E".into(),
            variants: vec![],
        }))
        .expect("Enum");
        cg.compile_statement(&Statement::VariableDecl(crate::parser::VariableDecl {
            name: "g".into(),
            var_type: "int".into(),
            value: Expression::literal("1"),
        }))
        .expect("global VariableDecl");
    }
}
