//! Nested `fn` / `class` / `enum` / `main` inside a MIR body.
//!
//! MIR stores [`crate::hir::NestedDecl`], not a cloned parser `Statement`.
//! Nested `fn` compiles through `hir_fns` + [`Self::compile_nested_function_from_mir`]
//! using the stored key and `MirFn` signature. Nested class/enum/main may use
//! [`CodeGenerator::nested_asts`].

use std::collections::HashSet;

use crate::backend::codegen::CodeGenerator;
use crate::hir::{NestedDecl, NestedKind};
use crate::parser::function::{unique_function_key, Function, FunctionBody, Parameter};
use crate::parser::{Program, Statement};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Declare and compile a nested `fn`. LLVM / `hir_fns` keys use
    /// [`unique_function_key`] with the enclosing function name when `func.name`
    /// is already in `self.functions` (same rule as `declare_function`'s map key).
    pub(crate) fn compile_nested_function(&mut self, func: &Function) -> Result<(), String> {
        let source_name = func.name.clone();
        let parent = self
            .current_function
            .and_then(|f| f.get_name().to_str().ok().map(str::to_string))
            .unwrap_or_default();
        let key = unique_function_key(&source_name, Some(parent.as_str()), |n| {
            self.functions.contains_key(n)
        });
        self.compile_nested_function_keyed(&source_name, &key, func)
    }

    /// Declare from [`crate::hir::MirFn`] signature and compile the body in
    /// `hir_fns[hir_key]`. Does not look up `nested_asts` / `fn_asts`.
    pub(crate) fn compile_nested_function_from_mir(
        &mut self,
        nested: &NestedDecl,
    ) -> Result<(), String> {
        let mir = self.hir_fns.get(&nested.hir_key).ok_or_else(|| {
            self.error(
                "compile_mir_nested",
                format!(
                    "no MIR for nested '{}' (hir key '{}')",
                    nested.name, nested.hir_key
                ),
            )
        })?;
        let func = Function {
            type_params: vec![],
            name: nested.hir_key.clone(),
            parameters: mir
                .param_names
                .iter()
                .zip(mir.param_types.iter())
                .map(|(name, param_type)| Parameter {
                    name: name.clone(),
                    param_type: param_type.clone(),
                    is_variadic: param_type == "...",
                })
                .collect(),
            return_type: mir.return_type.clone(),
            error_handler: None,
            body: FunctionBody::Block(vec![]),
            is_c: false,
        };
        self.compile_nested_function_keyed(&nested.name, &nested.hir_key, &func)
    }

    pub(crate) fn compile_nested_function_keyed(
        &mut self,
        source_name: &str,
        key: &str,
        func: &Function,
    ) -> Result<(), String> {
        let mut nested = func.clone();
        nested.name = key.to_string();
        if matches!(nested.body, FunctionBody::External) {
            nested.body = FunctionBody::Block(vec![]);
        }
        if !self.functions.contains_key(key) {
            self.declare_function(&nested)?;
        }
        if key != source_name {
            if let Some(&fv) = self.functions.get(key) {
                self.functions.insert(source_name.to_string(), fv);
            }
        }
        self.compile_function(&nested)
    }

    pub(crate) fn compile_mir_nested(&mut self, nested: &NestedDecl) -> Result<(), String> {
        // `compile_class` drop fns (and similar) move the builder; the outer MIR
        // block must resume at the same insert point.
        let parent_block = self.backend.builder.get_insert_block();
        let result = match nested.kind {
            NestedKind::Function => self.compile_nested_function_from_mir(nested),
            NestedKind::Class => {
                let class = self.classes.get(&nested.name).cloned().ok_or_else(|| {
                    self.error(
                        "compile_mir_nested",
                        format!("no class '{}' in program/classes map", nested.name),
                    )
                })?;
                self.compile_class(&class)
            }
            NestedKind::Enum => {
                let enum_def = self.enums.get(&nested.name).cloned().ok_or_else(|| {
                    self.error(
                        "compile_mir_nested",
                        format!("no enum '{}' in program/enums map", nested.name),
                    )
                })?;
                self.compile_enum(&enum_def)
            }
            NestedKind::Main => self.compile_main_entry(&crate::parser::MainEntry {
                entry_function: nested.name.clone(),
                args: vec![],
            }),
        };
        if let Some(bb) = parent_block {
            self.backend.builder.position_at_end(bb);
        }
        result
    }

    /// Index nested decls from a function body (top-level `compile_function` still has AST).
    pub(crate) fn register_nested_asts(&mut self, body: &[Statement], parent: &str) {
        let reserved: HashSet<String> = self.functions.keys().cloned().collect();
        let mut taken = reserved.clone();
        taken.extend(self.fn_asts.keys().cloned());
        self.index_nested_in_stmts(body, parent, &reserved, &mut taken);
    }

    /// Index nested `fn` stubs (no body) and nested class/enum defs for MIR lookup.
    pub(crate) fn index_nested_decls(&mut self, program: &Program) {
        let mut reserved = HashSet::new();
        for stmt in &program.statements {
            if let Statement::Function(func) = stmt {
                reserved.insert(func.name.clone());
            }
            if let Statement::Class(class) = stmt {
                for method in &class.methods {
                    reserved.insert(method.to_standalone_function(&class.name).name);
                }
            }
        }
        let mut taken = reserved.clone();
        for stmt in &program.statements {
            if let Statement::Class(class) = stmt {
                for method in &class.methods {
                    let func = method.to_standalone_function(&class.name);
                    if let FunctionBody::Block(body) = &func.body {
                        self.index_nested_in_stmts(body, &func.name, &reserved, &mut taken);
                    }
                }
            }
        }
        for stmt in &program.statements {
            if let Statement::Function(func) = stmt {
                if let FunctionBody::Block(body) = &func.body {
                    self.index_nested_in_stmts(body, &func.name, &reserved, &mut taken);
                }
            }
        }
    }

    fn index_nested_in_stmts(
        &mut self,
        stmts: &[Statement],
        parent: &str,
        reserved: &HashSet<String>,
        taken: &mut HashSet<String>,
    ) {
        for stmt in stmts {
            self.index_nested_in_stmt(stmt, parent, reserved, taken);
        }
    }

    fn index_nested_in_stmt(
        &mut self,
        stmt: &Statement,
        parent: &str,
        reserved: &HashSet<String>,
        taken: &mut HashSet<String>,
    ) {
        match stmt {
            Statement::Function(nested) => {
                let key = unique_function_key(&nested.name, Some(parent), |n| {
                    reserved.contains(n) || taken.contains(n)
                });
                taken.insert(key.clone());
                if let FunctionBody::Block(body) = &nested.body {
                    self.index_nested_in_stmts(body, &key, reserved, taken);
                }
            }
            Statement::Class(class) => {
                self.classes.insert(class.name.clone(), class.clone());
                self.nested_asts
                    .insert(class.name.clone(), stmt.clone());
                for method in &class.methods {
                    let func = method.to_standalone_function(&class.name);
                    let key = unique_function_key(&func.name, Some(parent), |n| {
                        reserved.contains(n) || taken.contains(n)
                    });
                    taken.insert(key.clone());
                    if let FunctionBody::Block(body) = &func.body {
                        self.index_nested_in_stmts(body, &key, reserved, taken);
                    }
                }
            }
            Statement::Enum(enum_def) => {
                self.enums.insert(enum_def.name.clone(), enum_def.clone());
                self.nested_asts
                    .insert(enum_def.name.clone(), stmt.clone());
            }
            Statement::If(if_expr) => {
                self.index_nested_in_stmts(&if_expr.body, parent, reserved, taken);
                for elif in &if_expr.elifs {
                    self.index_nested_in_stmts(&elif.body, parent, reserved, taken);
                }
                if let Some(else_body) = &if_expr.else_body {
                    self.index_nested_in_stmts(else_body, parent, reserved, taken);
                }
            }
            Statement::While(w) => {
                self.index_nested_in_stmts(&w.body, parent, reserved, taken);
            }
            Statement::For(f) => {
                self.index_nested_in_stmts(&f.body, parent, reserved, taken);
            }
            Statement::Match(m) => {
                for arm in &m.arms {
                    self.index_nested_in_stmts(&arm.body, parent, reserved, taken);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::{codegen::CodeGenerator, Backend};
    use crate::hir::{HirExpr, HirExprKind, MirBlock, MirFn, MirStmt, NestedDecl, NestedKind};
    use crate::parser::function::{Function, FunctionBody};
    use crate::parser::{Program, Statement};
    use crate::types::Type;
    use inkwell::context::Context;

    fn int_lit(n: &str) -> HirExpr {
        HirExpr {
            ty: Type::int(),
            kind: HirExprKind::Literal(n.to_string()),
        }
    }

    fn nested_int_fn(name: &str, params: Vec<(&str, &str)>, ret: &str) -> MirFn {
        MirFn {
            name: name.to_string(),
            param_names: params.iter().map(|(n, _)| n.to_string()).collect(),
            param_types: params.iter().map(|(_, t)| t.to_string()).collect(),
            return_type: ret.to_string(),
            complete: true,
            blocks: vec![MirBlock {
                stmts: vec![MirStmt::Return(Some(int_lit("1")))],
            }],
        }
    }

    #[test]
    fn compile_nested_function_uses_mir_signature_not_nested_asts() {
        let context = Context::create();
        let backend = Backend::new(&context, "nested_fn_sig");
        let mut cg = CodeGenerator::new(&backend);
        let outer = Function {
            type_params: vec![],
            name: "outer".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![]),
            is_c: false,
        };
        cg.declare_function(&outer).unwrap();
        let llvm_outer = *cg.functions.get("outer").unwrap();
        let entry = context.append_basic_block(llvm_outer, "entry");
        cg.backend.builder.position_at_end(entry);
        cg.current_function = Some(llvm_outer);

        cg.hir_fns
            .insert("inner".into(), nested_int_fn("inner", vec![("x", "int")], "int"));
        assert!(cg.nested_asts.is_empty());
        assert!(cg.fn_asts.is_empty());

        cg.compile_mir_nested(&NestedDecl {
            kind: NestedKind::Function,
            name: "inner".into(),
            hir_key: "inner".into(),
        })
        .expect("nested fn from MirFn signature");

        assert!(
            cg.functions.contains_key("inner"),
            "declare_function from MirFn signature"
        );
        assert!(
            !cg.nested_asts.values().any(|s| matches!(s, Statement::Function(_))),
            "Function must not be stored in nested_asts"
        );
    }

    #[test]
    fn index_nested_decls_does_not_put_function_in_nested_asts() {
        let context = Context::create();
        let backend = Backend::new(&context, "nested_asts_fn");
        let mut cg = CodeGenerator::new(&backend);
        let inner = Function {
            type_params: vec![],
            name: "helper".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![]),
            is_c: false,
        };
        let outer = Function {
            type_params: vec![],
            name: "outer".into(),
            parameters: vec![],
            return_type: "int".into(),
            error_handler: None,
            body: FunctionBody::Block(vec![Statement::Function(inner)]),
            is_c: false,
        };
        let program = Program::new(vec![Statement::Function(outer)]);
        cg.index_nested_decls(&program);
        assert!(
            !cg.nested_asts
                .values()
                .any(|s| matches!(s, Statement::Function(_))),
            "Function still in nested_asts: {:?}",
            cg.nested_asts.keys().collect::<Vec<_>>()
        );
    }
}
