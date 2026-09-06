//! MIR `raise` codegen from [`HirExpr`]: abort (`fprintf`+`exit`) or call `#handler`.

use crate::backend::codegen::CodeGenerator;
use crate::hir::{HirExpr, HirExprKind};
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::AddressSpace;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn compile_mir_raise(&mut self, expr: &HirExpr) -> Result<(), String> {
        if raise_should_call_listener(
            self.current_error_handler.as_deref(),
            self.current_function_name.as_deref(),
        ) {
            return self.compile_raise_listen(expr);
        }

        let (error_type, args) = raise_ctor_parts(expr);
        for arg in &args {
            let _ = self.compile_hir_expr(arg)?;
        }
        self.compile_raise_abort(&error_type, &args)
    }

    fn compile_raise_abort(&mut self, error_type: &str, args: &[HirExpr]) -> Result<(), String> {
        let error_msg = abort_error_message(error_type, args);

        let i8_ptr_type = self.backend.context.ptr_type(AddressSpace::default());

        let fprintf_func = *self.functions.get("fprintf")
            .ok_or("fprintf function not declared - use 'fprintf in libc of c'")?;
        let exit_func = *self.functions.get("exit")
            .ok_or("exit function not declared - use 'exit in libc of c'")?;

        let message_ptr = self.backend.builder.build_global_string_ptr(&error_msg, "error_msg")
            .map_err(|e| self.error("compile_raise", format!("failed to build error message string: {}", e)))?
            .as_pointer_value();
        let format_ptr = self.backend.builder.build_global_string_ptr("%s\n", "format_str")
            .map_err(|e| self.error("compile_raise", format!("failed to build format string: {}", e)))?
            .as_pointer_value();

        let stderr_global = match self.backend.module.get_global("stderr") {
            Some(g) => g.as_pointer_value(),
            None => {
                let stderr_global = self.backend.module.add_global(i8_ptr_type, None, "stderr");
                stderr_global.set_linkage(inkwell::module::Linkage::External);
                stderr_global.as_pointer_value()
            }
        };
        let stderr_file_ptr = self.backend.builder.build_load(i8_ptr_type, stderr_global, "stderr_file_ptr")
            .map_err(|e| self.error("compile_raise", format!("failed to load stderr: {}", e)))?
            .into_pointer_value();

        let _ = self.backend.builder.build_call(
            fprintf_func,
            &[stderr_file_ptr.into(), format_ptr.into(), message_ptr.into()],
            "fprintf_call",
        );
        self.emit_local_drops()?;
        let exit_code = crate::backend::functions::const_exit_status_one(
            exit_func,
            self.backend.context,
        );
        let _ = self.backend.builder.build_call(exit_func, &[exit_code.into()], "exit_call");
        let _ = self.backend.builder.build_unreachable();
        Ok(())
    }

    fn compile_raise_listen(&mut self, expr: &HirExpr) -> Result<(), String> {
        let (error_type, args) = raise_ctor_parts(expr);
        crate::backend::error::generate_error_class(self.backend.context, &mut self.type_mapper)?;
        self.ensure_exception_llvm_type(&error_type)?;

        let handler_name = self.current_error_handler.clone().ok_or_else(|| {
            self.error("compile_raise", "listen path requires current_error_handler")
        })?;
        let handler_fn = *self.functions.get(&handler_name).ok_or_else(|| {
            self.error(
                "compile_raise",
                format!("error listener '{}' is not declared", handler_name),
            )
        })?;

        let err_ptr = if raise_listen_uses_instance(expr) {
            match self.compile_hir_expr(expr) {
                Ok(val) => self.raise_value_as_ptr(val)?,
                Err(_) => self.pack_listen_error_ptr(&error_type, &args)?,
            }
        } else {
            self.pack_listen_error_ptr(&error_type, &args)?
        };

        let call = self.backend.builder.build_call(
            handler_fn,
            &[err_ptr.into()],
            "error_listener",
        ).map_err(|e| self.error("compile_raise", format!("failed to call error listener '{}': {}", handler_name, e)))?;

        let ret_value = match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(val) => Some(val),
            inkwell::values::ValueKind::Instruction(_) => None,
        };
        self.return_listener_value(ret_value)
    }

    fn ensure_exception_llvm_type(&mut self, class_name: &str) -> Result<(), String> {
        crate::backend::error::generate_error_class(self.backend.context, &mut self.type_mapper)?;
        if class_name.is_empty() || class_name == "Error" {
            return Ok(());
        }
        if self.type_mapper.struct_types.contains_key(class_name) {
            return Ok(());
        }
        if let Some(class) = self.classes.get(class_name).cloned() {
            self.type_mapper
                .get_or_create_struct_type_from_class(class_name, &class, &self.classes)
                .map_err(|e| self.error("compile_raise", e))?;
        }
        Ok(())
    }

    fn pack_listen_error_ptr(
        &mut self,
        error_type: &str,
        args: &[HirExpr],
    ) -> Result<PointerValue<'ctx>, String> {
        self.ensure_exception_llvm_type(error_type)?;
        let error_struct = listen_pack_struct_type(&self.type_mapper, error_type).ok_or_else(|| {
            self.error("compile_raise", "Error class was not generated")
        })?;

        let pack = raise_pack_fields(error_type, args);
        let err_ptr = self.backend.builder.build_alloca(error_struct, "raised_error")
            .map_err(|e| self.error("compile_raise", format!("failed to allocate Error: {}", e)))?;

        let i64_type = self.backend.context.i64_type();
        let code_val = i64_type.const_int(pack.code as u64, true);
        self.store_error_field(error_struct, err_ptr, 0, "code", code_val.into())?;

        let note_ptr = self.raise_field_string_ptr(&pack.note, "err_note")?;
        self.store_error_field(error_struct, err_ptr, 1, "note", note_ptr.into())?;

        let extra_ptr = self.raise_field_string_ptr(&pack.extra, "err_extra")?;
        self.store_error_field(error_struct, err_ptr, 2, "e", extra_ptr.into())?;
        Ok(err_ptr)
    }

    fn raise_value_as_ptr(
        &mut self,
        value: BasicValueEnum<'ctx>,
    ) -> Result<PointerValue<'ctx>, String> {
        match value {
            BasicValueEnum::PointerValue(ptr) => Ok(ptr),
            other => {
                let alloca = self
                    .backend
                    .builder
                    .build_alloca(other.get_type(), "raised_error")
                    .map_err(|e| {
                        self.error(
                            "compile_raise",
                            format!("failed to allocate raised exception: {}", e),
                        )
                    })?;
                self.backend
                    .builder
                    .build_store(alloca, other)
                    .map_err(|e| {
                        self.error(
                            "compile_raise",
                            format!("failed to store raised exception: {}", e),
                        )
                    })?;
                Ok(alloca)
            }
        }
    }

    fn store_error_field(
        &self,
        error_struct: inkwell::types::StructType<'ctx>,
        err_ptr: PointerValue<'ctx>,
        index: u64,
        field: &str,
        value: BasicValueEnum<'ctx>,
    ) -> Result<(), String> {
        let zero = self.backend.context.i32_type().const_int(0, false);
        let idx = self.backend.context.i32_type().const_int(index, false);
        let field_ptr = unsafe {
            self.backend.builder.build_in_bounds_gep(
                error_struct,
                err_ptr,
                &[zero, idx],
                &format!("error_{}_ptr", field),
            )
        }
        .map_err(|e| self.error("compile_raise", format!("failed to GEP Error.{}: {}", field, e)))?;
        self.backend.builder.build_store(field_ptr, value)
            .map_err(|e| self.error("compile_raise", format!("failed to store Error.{}: {}", field, e)))?;
        Ok(())
    }

    fn raise_field_string_ptr(&self, text: &str, name: &str) -> Result<PointerValue<'ctx>, String> {
        let i8_ptr_type = self.backend.context.ptr_type(AddressSpace::default());
        if text.is_empty() {
            return Ok(i8_ptr_type.const_null());
        }
        Ok(self.backend.builder.build_global_string_ptr(text, name)
            .map_err(|e| self.error("compile_raise", format!("failed to build Error string '{}': {}", name, e)))?
            .as_pointer_value())
    }

    fn return_listener_value(&mut self, value: Option<BasicValueEnum<'ctx>>) -> Result<(), String> {
        if let Some(value) = value {
            if let Some(current_fn) = self.current_function {
                let return_type = current_fn.get_type().get_return_type();
                if let Some(ret_type) = return_type {
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

/// Abort unless `#handler` names a *different* function than the one being compiled.
/// `None` handler, missing current name, or listener compiling itself → abort.
fn raise_should_call_listener(handler: Option<&str>, current_fn: Option<&str>) -> bool {
    match (handler, current_fn) {
        (Some(h), Some(cur)) if h != cur => true,
        _ => false,
    }
}

fn raise_listen_uses_instance(expr: &HirExpr) -> bool {
    matches!(
        &expr.kind,
        HirExprKind::StructLiteral { .. }
            | HirExprKind::ConstructorCall { .. }
            | HirExprKind::Variable(_)
    )
}

/// Call-form packing needs the `Error` prefix (code/note/e). Use the subclass
/// type only when that layout already starts with at least those three fields.
fn listen_pack_struct_type<'ctx>(
    type_mapper: &crate::backend::types::TypeMapper<'ctx>,
    error_type: &str,
) -> Option<inkwell::types::StructType<'ctx>> {
    let error = type_mapper.struct_types.get("Error").copied();
    if error_type != "Error" {
        if let Some(st) = type_mapper.struct_types.get(error_type).copied() {
            if st.count_fields() >= 3 {
                return Some(st);
            }
        }
    }
    error
}

fn abort_error_message(error_type: &str, args: &[HirExpr]) -> String {
    if args.is_empty() {
        return format!("[E-1]: {}", error_type);
    }
    let pack = raise_pack_fields(error_type, args);
    if pack.extra.is_empty() && pack.note.is_empty() {
        format!("[E{}]: {}", pack.code, error_type)
    } else if pack.note.is_empty() {
        format!("[E{}]: {}", pack.code, pack.extra)
    } else if pack.extra.is_empty() {
        format!("[E{}]: {}\nnote: {}", pack.code, error_type, pack.note)
    } else {
        format!("[E{}]: {}\nnote: {}", pack.code, pack.extra, pack.note)
    }
}

struct RaisePack {
    code: i64,
    note: String,
    extra: String,
}

/// Same packing as the abort `fprintf` message: int literal → `code`, strings → `note` / `e`.
fn raise_pack_fields(_error_type: &str, args: &[HirExpr]) -> RaisePack {
    let mut code: i64 = -1;
    let mut note = String::new();
    let mut extra = String::new();
    if let Some(first) = args.first() {
        if let Some(c) = raise_int_literal(first) {
            code = c;
        }
    }
    if args.len() >= 2 {
        note = raise_display_text(&args[1]);
    }
    if args.len() >= 3 {
        extra = raise_display_text(&args[2]);
    }
    RaisePack { code, note, extra }
}

fn raise_ctor_parts(expr: &HirExpr) -> (String, Vec<HirExpr>) {
    match &expr.kind {
        HirExprKind::Call { function, args } => {
            let name = match &function.kind {
                HirExprKind::Variable(n) | HirExprKind::Literal(n) => n.clone(),
                _ => "Error".to_string(),
            };
            (name, args.clone())
        }
        HirExprKind::ConstructorCall { class_name, args } => (class_name.clone(), args.clone()),
        HirExprKind::StructLiteral { struct_name, fields } => {
            let name = if struct_name.is_empty() {
                "Error".to_string()
            } else {
                struct_name.clone()
            };
            (name, fields.iter().map(|(_, e)| e.clone()).collect())
        }
        HirExprKind::TypeCast { target_type, value } => (target_type.clone(), vec![(**value).clone()]),
        HirExprKind::Variable(n) | HirExprKind::Literal(n) => {
            (if n.is_empty() { "Error".to_string() } else { n.clone() }, Vec::new())
        }
        _ => ("Error".to_string(), Vec::new()),
    }
}

fn raise_int_literal(expr: &HirExpr) -> Option<i64> {
    match &expr.kind {
        HirExprKind::Literal(s) => {
            let t = s.trim();
            t.parse::<i64>().ok().or_else(|| t.parse::<i32>().ok().map(|n| n as i64))
        }
        HirExprKind::Unary { op, operand } if op == "-" => {
            raise_int_literal(operand).map(|n| -n)
        }
        _ => None,
    }
}

fn raise_display_text(expr: &HirExpr) -> String {
    match &expr.kind {
        HirExprKind::Literal(s) => {
            let t = s.trim();
            if t.len() >= 2
                && ((t.starts_with('"') && t.ends_with('"'))
                    || (t.starts_with('\'') && t.ends_with('\'')))
            {
                t[1..t.len() - 1].to_string()
            } else {
                t.to_string()
            }
        }
        HirExprKind::Variable(n) => n.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{codegen::CodeGenerator, Backend};
    use crate::hir::{HirExpr, HirExprKind};
    use crate::types::Type;
    use inkwell::context::Context;

    fn hir(kind: HirExprKind) -> HirExpr {
        HirExpr {
            ty: Type::int(),
            kind,
        }
    }

    fn lit(s: &str) -> HirExpr {
        hir(HirExprKind::Literal(s.to_string()))
    }

    fn str_lit(s: &str) -> HirExpr {
        HirExpr {
            ty: Type::string(),
            kind: HirExprKind::Literal(format!("\"{}\"", s)),
        }
    }

    fn raise_error_expr(code: &str, note: &str, extra: Option<&str>) -> HirExpr {
        let mut args = vec![lit(code), str_lit(note)];
        if let Some(e) = extra {
            args.push(str_lit(e));
        }
        hir(HirExprKind::ConstructorCall {
            class_name: "Error".to_string(),
            args,
        })
    }

    #[test]
    fn raise_ctor_parts_struct_literal_uses_type_name_and_field_exprs() {
        let expr = hir(HirExprKind::StructLiteral {
            struct_name: "CustomError".to_string(),
            fields: vec![
                ("code".to_string(), lit("404")),
                ("message".to_string(), lit("\"Not found\"")),
                ("details".to_string(), lit("\"missing\"")),
            ],
        });
        let (name, args) = raise_ctor_parts(&expr);
        assert_eq!(name, "CustomError");
        assert_eq!(args.len(), 3);
        assert!(matches!(&args[0].kind, HirExprKind::Literal(s) if s == "404"));
        assert!(matches!(&args[1].kind, HirExprKind::Literal(s) if s == "\"Not found\""));
        assert!(matches!(&args[2].kind, HirExprKind::Literal(s) if s == "\"missing\""));
    }

    #[test]
    fn raise_ctor_parts_empty_struct_literal_still_uses_type_name() {
        let expr = hir(HirExprKind::StructLiteral {
            struct_name: "EmptyError".to_string(),
            fields: vec![],
        });
        let (name, args) = raise_ctor_parts(&expr);
        assert_eq!(name, "EmptyError");
        assert!(args.is_empty());
    }

    #[test]
    fn raise_should_call_listener_only_when_handler_is_other_function() {
        assert!(!raise_should_call_listener(None, Some("f")));
        assert!(!raise_should_call_listener(Some("on_err"), None));
        assert!(!raise_should_call_listener(Some("on_err"), Some("on_err")));
        assert!(!raise_should_call_listener(None, None));
        assert!(raise_should_call_listener(Some("on_err"), Some("f")));
    }

    #[test]
    fn raise_pack_fields_maps_int_and_strings_like_abort_message() {
        let args = vec![lit("404"), lit("\"Not found\""), lit("\"missing\"")];
        let pack = raise_pack_fields("CustomError", &args);
        assert_eq!(pack.code, 404);
        assert_eq!(pack.note, "Not found");
        assert_eq!(pack.extra, "missing");
        assert_eq!(
            abort_error_message("CustomError", &args),
            "[E404]: missing\nnote: Not found"
        );
    }

    fn compile_raise_module(
        current: &str,
        handler: Option<&str>,
        expr: &HirExpr,
    ) -> Result<String, String> {
        let context = Context::create();
        let backend = Backend::new(&context, "mir_raise_test");
        let mut cg = CodeGenerator::new(&backend);
        cg.declare_runtime_functions();

        let i8p = context.ptr_type(AddressSpace::default());
        let i32 = context.i32_type();
        let i64 = context.i64_type();
        let fprintf_ty = i32.fn_type(&[i8p.into(), i8p.into()], true);
        let fprintf = backend.module.add_function("fprintf", fprintf_ty, None);
        cg.functions.insert("fprintf".to_string(), fprintf);

        let fn_ty = i64.fn_type(&[], false);
        let f = backend.module.add_function(current, fn_ty, None);
        cg.functions.insert(current.to_string(), f);
        cg.current_function = Some(f);
        cg.current_function_name = Some(current.to_string());
        cg.current_error_handler = handler.map(|s| s.to_string());

        if let Some(h) = handler {
            if h != current && !cg.functions.contains_key(h) {
                let handler_ty = i64.fn_type(&[i8p.into()], false);
                let hf = backend.module.add_function(h, handler_ty, None);
                cg.functions.insert(h.to_string(), hf);
            }
        }

        let entry = context.append_basic_block(f, "entry");
        backend.builder.position_at_end(entry);
        cg.compile_mir_raise(expr)?;
        Ok(backend.module.print_to_string().to_string())
    }

    #[test]
    fn compile_mir_raise_without_handler_emits_fprintf_and_exit() {
        let ir = compile_raise_module("main", None, &raise_error_expr("1", "boom", None)).unwrap();
        assert!(ir.contains("fprintf"), "abort IR should call fprintf:\n{}", ir);
        assert!(ir.contains("exit"), "abort IR should call exit:\n{}", ir);
        assert!(ir.contains("unreachable"), "abort IR should be unreachable after exit:\n{}", ir);
        assert!(!ir.contains("error_listener"), "abort IR must not call a listener:\n{}", ir);
    }

    #[test]
    fn compile_mir_raise_in_listener_itself_still_aborts() {
        let ir = compile_raise_module("on_err", Some("on_err"), &raise_error_expr("1", "boom", None))
            .unwrap();
        assert!(ir.contains("exit"), "listener self-raise must abort:\n{}", ir);
        assert!(!ir.contains("call i64 @on_err"), "must not recurse into self as listener:\n{}", ir);
    }

    #[test]
    fn compile_mir_raise_with_other_handler_calls_and_returns() {
        let ir = compile_raise_module("f", Some("on_err"), &raise_error_expr("404", "Not found", Some("missing")))
            .unwrap();
        assert!(
            ir.contains("call i64 @on_err"),
            "listen IR should call on_err:\n{}",
            ir
        );
        assert!(ir.contains("ret i64"), "listen IR should return the handler value:\n{}", ir);
        assert!(
            !ir.contains("exit_call") && !ir.contains("call i32 @exit"),
            "listen IR must not call exit:\n{}",
            ir
        );
        assert!(
            ir.contains("404") || ir.contains("i64 404"),
            "Error.code should be packed from the int literal:\n{}",
            ir
        );
        assert!(
            ir.contains("Not found"),
            "Error.note should be packed from the string:\n{}",
            ir
        );
        assert!(
            ir.contains("missing"),
            "Error.e should be packed from the extra string:\n{}",
            ir
        );
    }

    fn error_class_def() -> crate::parser::class::ClassDef {
        crate::parser::class::ClassDef {
            type_params: vec![],
            name: "Error".to_string(),
            parent: None,
            fields: vec![
                crate::parser::class::ClassField {
                    name: "code".to_string(),
                    field_type: "int".to_string(),
                    bit_width: None,
                },
                crate::parser::class::ClassField {
                    name: "note".to_string(),
                    field_type: "str".to_string(),
                    bit_width: None,
                },
                crate::parser::class::ClassField {
                    name: "e".to_string(),
                    field_type: "object".to_string(),
                    bit_width: None,
                },
            ],
            methods: vec![],
            packed: false,
            has_constructor: false,
        }
    }

    fn app_error_class_def() -> crate::parser::class::ClassDef {
        crate::parser::class::ClassDef {
            type_params: vec![],
            name: "AppError".to_string(),
            parent: Some("Error".to_string()),
            fields: vec![crate::parser::class::ClassField {
                name: "kind".to_string(),
                field_type: "int".to_string(),
                bit_width: None,
            }],
            methods: vec![],
            packed: false,
            has_constructor: false,
        }
    }

    fn compile_raise_module_with_app_error(
        current: &str,
        handler: Option<&str>,
        expr: &HirExpr,
    ) -> Result<String, String> {
        let context = Context::create();
        let backend = Backend::new(&context, "mir_raise_test");
        let mut cg = CodeGenerator::new(&backend);
        cg.declare_runtime_functions();
        cg.classes.insert("Error".to_string(), error_class_def());
        cg.classes.insert("AppError".to_string(), app_error_class_def());
        crate::backend::error::generate_error_class(cg.backend.context, &mut cg.type_mapper)?;
        let app = cg.classes.get("AppError").cloned().unwrap();
        cg.type_mapper
            .get_or_create_struct_type_from_class("AppError", &app, &cg.classes)?;

        let i8p = context.ptr_type(AddressSpace::default());
        let i32 = context.i32_type();
        let i64 = context.i64_type();
        let fprintf_ty = i32.fn_type(&[i8p.into(), i8p.into()], true);
        let fprintf = backend.module.add_function("fprintf", fprintf_ty, None);
        cg.functions.insert("fprintf".to_string(), fprintf);

        let fn_ty = i64.fn_type(&[], false);
        let f = backend.module.add_function(current, fn_ty, None);
        cg.functions.insert(current.to_string(), f);
        cg.current_function = Some(f);
        cg.current_function_name = Some(current.to_string());
        cg.current_error_handler = handler.map(|s| s.to_string());

        if let Some(h) = handler {
            if h != current && !cg.functions.contains_key(h) {
                let handler_ty = i64.fn_type(&[i8p.into()], false);
                let hf = backend.module.add_function(h, handler_ty, None);
                cg.functions.insert(h.to_string(), hf);
            }
        }

        let ctor_ty = i8p.fn_type(&[], false);
        let ctor = backend.module.add_function("AppError_new", ctor_ty, None);
        cg.functions.insert("AppError_new".to_string(), ctor);

        let entry = context.append_basic_block(f, "entry");
        backend.builder.position_at_end(entry);
        cg.compile_mir_raise(expr)?;
        Ok(backend.module.print_to_string().to_string())
    }

    #[test]
    fn compile_mir_raise_listen_app_error_extra_field_is_not_only_error_pack() {
        let expr = HirExpr {
            ty: Type::NamedType {
                name: "AppError".to_string(),
            },
            kind: HirExprKind::StructLiteral {
                struct_name: "AppError".to_string(),
                fields: vec![
                    ("kind".to_string(), lit("7")),
                    ("code".to_string(), lit("1")),
                ],
            },
        };
        let ir = compile_raise_module_with_app_error("f", Some("on_err"), &expr).unwrap();
        assert!(
            ir.contains("%AppError") || ir.contains("AppError"),
            "listen IR should materialize the AppError instance, not only %Error packing:\n{}",
            ir
        );
        let only_three_field_error_pack = ir.contains("%Error = type { i64, ptr, ptr }")
            && (ir.contains("alloca %Error") || ir.contains("raised_error"))
            && !ir.contains("%AppError");
        assert!(
            !only_three_field_error_pack,
            "listen IR must not only alloca/pack a 3-field %Error when raising AppError with extra fields:\n{}",
            ir
        );
        assert!(
            ir.contains("call i64 @on_err"),
            "listen IR should still call on_err:\n{}",
            ir
        );
    }

    #[test]
    fn raise_listen_uses_instance_for_struct_ctor_and_variable() {
        assert!(raise_listen_uses_instance(&hir(HirExprKind::StructLiteral {
            struct_name: "AppError".to_string(),
            fields: vec![],
        })));
        assert!(raise_listen_uses_instance(&hir(HirExprKind::ConstructorCall {
            class_name: "AppError".to_string(),
            args: vec![],
        })));
        assert!(raise_listen_uses_instance(&hir(HirExprKind::Variable(
            "err".to_string()
        ))));
        assert!(!raise_listen_uses_instance(&hir(HirExprKind::Call {
            function: Box::new(hir(HirExprKind::Variable("AppError".to_string()))),
            args: vec![],
        })));
    }

    #[test]
    fn compile_mir_raise_listen_constructor_call_passes_class_pointer() {
        let expr = HirExpr {
            ty: Type::NamedType {
                name: "AppError".to_string(),
            },
            kind: HirExprKind::ConstructorCall {
                class_name: "AppError".to_string(),
                args: vec![],
            },
        };
        let ir = compile_raise_module_with_app_error("f", Some("on_err"), &expr).unwrap();
        assert!(
            ir.contains("AppError_new") || ir.contains("%AppError"),
            "listen ConstructorCall should compile a class instance, not only pack %Error:\n{}",
            ir
        );
        assert!(
            ir.contains("call i64 @on_err"),
            "listen IR should call on_err:\n{}",
            ir
        );
        let only_three_field_error_pack = ir.contains("alloca %Error") && !ir.contains("%AppError");
        assert!(
            !only_three_field_error_pack,
            "ConstructorCall listen must not fall back to packing a 3-field %Error:\n{}",
            ir
        );
    }

    #[test]
    fn compile_mir_raise_listen_variable_passes_existing_instance_pointer() {
        let expr = HirExpr {
            ty: Type::NamedType {
                name: "AppError".to_string(),
            },
            kind: HirExprKind::Variable("err".to_string()),
        };
        let ir = compile_raise_listen_with_app_error_var("err", &expr).unwrap();
        assert!(
            ir.contains("%AppError"),
            "listen Variable should pass the AppError instance pointer:\n{}",
            ir
        );
        assert!(
            ir.contains("call i64 @on_err"),
            "listen IR should call on_err:\n{}",
            ir
        );
        assert!(
            !ir.contains("alloca %Error"),
            "listen Variable must not alloca a packed 3-field %Error:\n{}",
            ir
        );
    }

    fn compile_raise_listen_with_app_error_var(
        var: &str,
        expr: &HirExpr,
    ) -> Result<String, String> {
        let context = Context::create();
        let backend = Backend::new(&context, "mir_raise_test");
        let mut cg = CodeGenerator::new(&backend);
        cg.declare_runtime_functions();
        cg.classes.insert("Error".to_string(), error_class_def());
        cg.classes.insert("AppError".to_string(), app_error_class_def());
        crate::backend::error::generate_error_class(cg.backend.context, &mut cg.type_mapper)?;
        let app = cg.classes.get("AppError").cloned().unwrap();
        let app_ty = cg
            .type_mapper
            .get_or_create_struct_type_from_class("AppError", &app, &cg.classes)?;

        let i8p = context.ptr_type(AddressSpace::default());
        let i64 = context.i64_type();
        let fn_ty = i64.fn_type(&[], false);
        let f = backend.module.add_function("f", fn_ty, None);
        cg.functions.insert("f".to_string(), f);
        cg.current_function = Some(f);
        cg.current_function_name = Some("f".to_string());
        cg.current_error_handler = Some("on_err".to_string());
        let handler_ty = i64.fn_type(&[i8p.into()], false);
        let hf = backend.module.add_function("on_err", handler_ty, None);
        cg.functions.insert("on_err".to_string(), hf);

        let ctor_ty = i8p.fn_type(&[], false);
        let ctor = backend.module.add_function("AppError_new", ctor_ty, None);
        cg.functions.insert("AppError_new".to_string(), ctor);

        let entry = context.append_basic_block(f, "entry");
        backend.builder.position_at_end(entry);
        let alloca = backend
            .builder
            .build_alloca(app_ty, var)
            .map_err(|e| e.to_string())?;
        cg.variables
            .insert(var.to_string(), (alloca, app_ty.into()));
        cg.compile_mir_raise(expr)?;
        Ok(backend.module.print_to_string().to_string())
    }
}
