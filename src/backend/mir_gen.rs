//! Compile statement MIR to LLVM. Comment/import `Unsupported` is skipped in
//! lowering; nested class/fn is `MirStmt::Nested`. Missing or incomplete MIR
//! (`Ok(false)`) is a hard error in `compile_function`, not an AST body.

use super::codegen::CodeGenerator;
use crate::hir::{HirExpr, HirExprKind, MirFn, MirStmt};
use crate::parser::function::Function;
use crate::parser::memory::MemoryOp;
use inkwell::basic_block::BasicBlock;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::AddressSpace;
use std::collections::HashMap;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    pub(crate) fn compile_hir_expr(&mut self, expr: &HirExpr) -> Result<BasicValueEnum<'ctx>, String> {
        self.compile_hir_expr_typed(expr)
    }

    /// Compile MIR by name whenever `hir_fns` has the function. Moves the
    /// entry out of `hir_fns` so `compile_mir_fn` can take `&MirFn` without
    /// cloning; nested functions look up other names still in the map.
    /// `complete == false` means a non-comment/import `Unsupported` was
    /// dropped from the CFG — omit MIR so `compile_function` errors.
    pub(crate) fn compile_mir_for_function(&mut self, func: &Function) -> Result<bool, String> {
        let Some(key) = self.hir_fn_key(func) else {
            return Ok(false);
        };
        let Some(mir) = self.hir_fns.remove(&key) else {
            return Ok(false);
        };
        if !mir.complete {
            return Ok(false);
        }
        let result = self.compile_mir_fn(func, &mir);
        self.hir_fns.insert(key, mir);
        result.map(|()| true)
    }

    fn hir_fn_key(&self, func: &Function) -> Option<String> {
        if self.hir_fns.contains_key(&func.name) {
            return Some(func.name.clone());
        }
        if let Some(cur) = self.current_function {
            if let Ok(parent) = cur.get_name().to_str() {
                let keyed = format!("{}_{}", parent, func.name);
                if self.hir_fns.contains_key(&keyed) {
                    return Some(keyed);
                }
            }
        }
        None
    }

    pub(crate) fn compile_mir_fn(&mut self, _func: &Function, mir: &MirFn) -> Result<(), String> {
        let function = self.current_function.ok_or_else(|| {
            self.error("compile_mir_fn", "MIR body outside a function")
        })?;
        self.mir_name_uses_left = count_mir_name_uses(mir);

        let mut bbs: Vec<BasicBlock<'ctx>> = Vec::with_capacity(mir.blocks.len());
        for i in 0..mir.blocks.len() {
            bbs.push(
                self.backend
                    .context
                    .append_basic_block(function, &format!("mir{}", i)),
            );
        }
        if bbs.is_empty() {
            return Err(self.error("compile_mir_fn", "empty MIR"));
        }

        self.backend
            .builder
            .build_unconditional_branch(bbs[0])
            .map_err(|e| self.error("compile_mir_fn", format!("branch to MIR entry: {}", e)))?;
        self.entry_successor = Some(bbs[0]);

        // Per MIR scope region (not `memory_ctx.enter_scope`): then/else blocks
        // are not sequential, so stacking memory_ctx while scanning 0..n nests them.
        let mut mir_scopes: Vec<Vec<String>> = Vec::new();
        // Block index of each open `ScopeEnter` (codegen_order can leave an inner
        // enter on the stack while compiling an outer preheader/incr).
        let mut scope_enter_at: Vec<usize> = Vec::new();

        for i in mir.codegen_order() {
            let block = &mir.blocks[i];
            self.backend.builder.position_at_end(bbs[i]);
            self.memory_ctx.unmark_all_dropped();
            for stmt in &block.stmts {
                // Overflow/inttoptr lowering can `br` out of `bbs[i]` into a
                // merge block. Stopping because *the MIR header* already has a
                // terminator used to skip the rest of the block (IntBuf `push`
                // dropped `sys_put_int` after `cap * 2`). Keep emitting into
                // the current insert block when it is still open.
                let insert = self.backend.builder.get_insert_block();
                if insert.is_some_and(|b| b.get_terminator().is_some()) {
                    break;
                }
                if insert != Some(bbs[i]) && bbs[i].get_terminator().is_none() {
                    self.backend.builder.position_at_end(bbs[i]);
                }
                match stmt {
                    MirStmt::ScopeEnter => {
                        mir_scopes.push(Vec::new());
                        scope_enter_at.push(i);
                    }
                    MirStmt::ScopeExit => {
                        let names = mir_scopes.pop().unwrap_or_default();
                        scope_enter_at.pop();
                        self.emit_mir_scope_drops(&names)?;
                    }
                    MirStmt::Assign { name, value } => {
                        // Loop `i`/`j` are stored in the preheader first; a later
                        // Assign must not record them on an inner `ScopeEnter`.
                        // `is_dropped` is a *previous CFG arm*, not "already stored".
                        if !name.contains('.') && self.memory_ctx.is_dropped(name) {
                            self.memory_ctx.unmark_dropped(name);
                        }
                        let already_stored = !name.contains('.')
                            && self.variables.contains_key(name)
                            && !self.memory_ctx.is_moved(name);
                        let push_let = !already_stored
                            && !name.contains('.')
                            && scope_enter_at.last() == Some(&i);
                        self.compile_mir_assign(name, value)?;
                        if push_let {
                            if let Some(scope) = mir_scopes.last_mut() {
                                if !scope.iter().any(|n| n == name) {
                                    scope.push(name.clone());
                                }
                            }
                        }
                    }
                    MirStmt::Expr(e) => {
                        self.compile_hir_expr(e)?;
                    }
                    MirStmt::Goto(target) => {
                        self.backend
                            .builder
                            .build_unconditional_branch(bbs[*target])
                            .map_err(|e| self.error("compile_mir_fn", e.to_string()))?;
                    }
                    MirStmt::Branch {
                        cond,
                        then_bb,
                        else_bb,
                    } => {
                        let cond_val = self.compile_hir_expr(cond)?;
                        let cond_bool =
                            super::control_flow::value_to_bool(cond_val, &self.backend.builder)?;
                        self.backend
                            .builder
                            .build_conditional_branch(cond_bool, bbs[*then_bb], bbs[*else_bb])
                            .map_err(|e| self.error("compile_mir_fn", e.to_string()))?;
                    }
                    MirStmt::Return(v) => {
                        self.compile_mir_return(v.as_ref())?;
                    }
                    MirStmt::MemoryOp(op) => {
                        self.compile_memory_op(op)?;
                        Self::sync_mir_scopes_after_memory_op(&mut mir_scopes, op);
                    }
                    MirStmt::Raise(r) => {
                        self.compile_mir_raise(r)?;
                    }
                    MirStmt::Nested(n) => {
                        self.compile_mir_nested(n)?;
                    }
                }
            }
        }

        // MIR `bbs` are not the only LLVM blocks: `compile_mir_nested` / AST
        // helpers can append extras. Only walking `bbs` left those without a
        // terminator.
        for bb in function.get_basic_blocks() {
            if bb.get_terminator().is_some() {
                continue;
            }
            self.backend.builder.position_at_end(bb);
            self.emit_local_drops().map_err(|e| {
                self.error("compile_mir_fn", format!("auto-drop: {}", e))
            })?;
            super::functions::build_implicit_return(&self.backend.builder, function)
                .map_err(|e| self.error("compile_mir_fn", format!("default return: {}", e)))?;
        }

        if let Some(last) = bbs.last() {
            self.backend.builder.position_at_end(*last);
        }
        self.mir_name_uses_left.clear();
        Ok(())
    }

    fn mir_forget_name(mir_scopes: &mut Vec<Vec<String>>, name: &str) {
        for scope in mir_scopes.iter_mut() {
            scope.retain(|n| n != name);
        }
    }

    fn mir_record_name(mir_scopes: &mut [Vec<String>], name: &str) {
        if let Some(scope) = mir_scopes.last_mut() {
            if !scope.iter().any(|n| n == name) {
                scope.push(name.to_string());
            }
        }
    }

    fn sync_mir_scopes_after_memory_op(mir_scopes: &mut Vec<Vec<String>>, op: &MemoryOp) {
        match op {
            MemoryOp::Remove { target } => Self::mir_forget_name(mir_scopes, target),
            MemoryOp::RemoveMultiple { targets } => {
                for t in targets {
                    Self::mir_forget_name(mir_scopes, t);
                }
            }
            MemoryOp::Move { source, target } => {
                Self::mir_forget_name(mir_scopes, source);
                Self::mir_record_name(mir_scopes, target);
            }
            MemoryOp::Clone { target, .. } => Self::mir_record_name(mir_scopes, target),
            MemoryOp::Copy { .. } | MemoryOp::CleanOut { .. } => {}
        }
    }

    /// Drop lets recorded between matching `ScopeEnter`/`ScopeExit`. Same
    /// sequence as `emit_current_scope_drops` / then-body `compile_if`, but
    /// keyed by MIR names so then/else are not `memory_ctx` nested.
    fn emit_mir_scope_drops(&mut self, names: &[String]) -> Result<(), String> {
        if let Some(block) = self.backend.builder.get_insert_block() {
            if block.get_terminator().is_some() {
                return Ok(());
            }
        }
        for var_name in names {
            if self.memory_ctx.is_dropped(var_name) || self.memory_ctx.is_moved(var_name) {
                continue;
            }
            match self.memory_ctx.lifetimes.get(var_name) {
                Some(info)
                    if matches!(
                        info.state,
                        crate::backend::memory_ops::VariableState::Initialized
                    ) => {}
                _ => continue,
            }
            if self.array_allocas.contains_key(var_name) {
                self.drop_array_local(var_name)?;
                continue;
            }
            if !self.variables.contains_key(var_name) {
                continue;
            }
            // Value types are function-lifetime allocas (loop `i`/`j`). Removing
            // them at ScopeExit breaks later uses after continue/break joins.
            if let Some(&(_, ty)) = self.variables.get(var_name) {
                if matches!(
                    ty,
                    inkwell::types::BasicTypeEnum::IntType(_)
                        | inkwell::types::BasicTypeEnum::FloatType(_)
                ) {
                    continue;
                }
            }
            if let Some(tname) = self.variable_types.get(var_name) {
                if self.c_value_names.contains(tname) {
                    continue;
                }
            }
            super::memory_ops::compile_remove(
                self.backend.context,
                &self.backend.builder,
                &mut self.variables,
                &mut self.memory_ctx,
                &self.functions,
                var_name,
                false,
            )?;
            self.memory_ctx.mark_dropped(var_name.clone());
        }
        Ok(())
    }

    fn compile_mir_assign(&mut self, name: &str, value: &HirExpr) -> Result<(), String> {
        if name.contains('.') {
            return self.store_hir_field_assign(name, value);
        }
        if !self.variables.contains_key(name) && !self.array_allocas.contains_key(name) {
            let numeric = matches!(
                value.ty,
                crate::types::Type::Int { .. }
                    | crate::types::Type::Float { .. }
                    | crate::types::Type::Bool
            );
            if numeric {
                self.alloc_mir_local_slot(name, &value.ty)?;
            } else if matches!(
                value.ty,
                crate::types::Type::Array { .. } | crate::types::Type::Slice(_)
            ) {
                // `bind_array_ptr` fills array_allocas; a scalar alloca +
                // store cannot GEP later (`arr[i + 1]`).
                return self.bind_mir_array_local(name, value);
            } else {
                self.alloc_mir_local_slot(name, &value.ty)?;
            }
        }
        if let Some(src) = self.hir_owned_resource_var(value) {
            return self.compile_last_use_move(src, name);
        }
        self.store_hir_assign(name, value)
    }

    fn compile_last_use_move(&mut self, source: &str, target: &str) -> Result<(), String> {
        if let Some(class_name) = self.variable_types.get(source).cloned() {
            self.variable_types.insert(target.to_string(), class_name);
        }
        if let Some(ty) = self.memory_ctx.get_variable_type(source).cloned() {
            self.memory_ctx.set_variable_type(target.to_string(), ty);
        }
        super::memory_ops::compile_move(
            self.backend.context,
            &self.backend.builder,
            &mut self.variables,
            &mut self.memory_ctx,
            source,
            target,
        )
        .map_err(|e| self.error("compile_mir_assign", e))?;
        self.note_mir_name_use(source);
        self.memory_ctx.register_birth(target.to_string());
        self.memory_ctx.mark_initialized(target);
        Ok(())
    }

    /// Typed array/slice first bind into `array_allocas` (same maps as
    /// `compile_array_declaration`).
    fn bind_mir_array_local(&mut self, name: &str, value: &HirExpr) -> Result<(), String> {
        let compiled = self.compile_hir_expr_typed(value).map_err(|e| {
            self.error(
                "compile_mir_assign",
                format!("failed to compile array initializer for '{}': {}", name, e),
            )
        })?;
        if let crate::types::Type::Slice(elem) = &value.ty {
            let elem_llvm = self
                .coffee_type_to_llvm(&elem.to_string())
                .map_err(|e| {
                    self.error(
                        "compile_mir_assign",
                        format!("array element type '{}': {}", elem, e),
                    )
                })?;
            match (&value.kind, compiled) {
                (HirExprKind::ArrayLiteral { elements }, BasicValueEnum::PointerValue(p)) => {
                    let n = self.backend.context.i64_type().const_int(elements.len() as u64, false);
                    self.pack_slice_from_buffer(name, p, n, elem_llvm)?;
                }
                (_, fat) => {
                    self.bind_slice_fat(name, fat, elem_llvm, false)?;
                }
            }
            self.memory_ctx
                .set_variable_type(name.to_string(), value.ty.to_string());
            self.memory_ctx.register_birth(name.to_string());
            self.memory_ctx.mark_initialized(name);
            return Ok(());
        }
        let (size, elem_ty) = match &value.ty {
            crate::types::Type::Array { elem, size } => (*size as u32, elem.as_ref().clone()),
            _ => {
                return Err(self.error(
                    "compile_mir_assign",
                    format!("expected array or slice type for '{}'", name),
                ))
            }
        };
        let elem_llvm = self
            .coffee_type_to_llvm(&elem_ty.to_string())
            .map_err(|e| {
                self.error(
                    "compile_mir_assign",
                    format!("array element type '{}': {}", elem_ty, e),
                )
            })?;
        let ptr = match compiled {
            BasicValueEnum::PointerValue(p) => p,
            other => {
                let alloca = self
                    .create_entry_alloca_preserving_terminator(other.get_type(), name)
                    .map_err(|e| {
                        self.error(
                            "compile_mir_assign",
                            format!("failed to allocate array '{}': {}", name, e),
                        )
                    })?;
                self.backend.builder.build_store(alloca, other).map_err(|e| {
                    self.error(
                        "compile_mir_assign",
                        format!("failed to store array '{}': {}", name, e),
                    )
                })?;
                alloca
            }
        };
        self.bind_array_ptr(name, ptr, size, elem_llvm)?;
        self.memory_ctx
            .set_variable_type(name.to_string(), value.ty.to_string());
        self.memory_ctx.register_birth(name.to_string());
        self.memory_ctx.mark_initialized(name);
        Ok(())
    }

    /// Field store via GEP. Packed bitfields still RMW; the RHS is typed.
    fn store_hir_field_assign(&mut self, name: &str, value: &HirExpr) -> Result<(), String> {
        let parts: Vec<&str> = name.split('.').collect();
        if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
            return Err(self.error(
                "compile_mir_assign",
                format!("invalid field assignment '{}'", name),
            ));
        }

        if parts.len() == 2 {
            if let Some(info) = self.packed_bitfield_info(parts[0], parts[1]) {
                if info.bit_width > 0 {
                    let compiled = self.compile_hir_expr_typed(value).map_err(|e| {
                        self.error(
                            "compile_mir_assign",
                            format!("failed to compile value for field '{}': {}", name, e),
                        )
                    })?;
                    self.emit_field_store(parts[0], parts[1], compiled)?;
                    self.used_variables.insert(parts[0].to_string());
                    return Ok(());
                }
            }
        }

        let field_ptr = self.mir_field_assign_ptr(&parts)?;
        let compiled = self.compile_hir_expr_typed(value).map_err(|e| {
            self.error(
                "compile_mir_assign",
                format!("failed to compile value for field '{}': {}", name, e),
            )
        })?;
        self.backend
            .builder
            .build_store(field_ptr, compiled)
            .map_err(|e| {
                self.error(
                    "compile_mir_assign",
                    format!("failed to store field '{}': {}", name, e),
                )
            })?;
        self.used_variables.insert(parts[0].to_string());
        Ok(())
    }

    fn mir_field_assign_ptr(&mut self, parts: &[&str]) -> Result<PointerValue<'ctx>, String> {
        let root = parts[0];
        if parts.len() == 2 {
            return self.compile_field_access_ptr(root, parts[1]);
        }

        let mut class_name = self.variable_types.get(root).cloned().ok_or_else(|| {
            self.error(
                "compile_mir_assign",
                format!("cannot get class name for variable '{}'", root),
            )
        })?;

        let mut field_ptr = self.compile_field_access_ptr(root, parts[1]).map_err(|e| {
            self.error(
                "compile_mir_assign",
                format!("failed to get field pointer for '{}.{}': {}", root, parts[1], e),
            )
        })?;

        let first_field_ty = self
            .classes
            .get(&class_name)
            .and_then(|c| c.fields.iter().find(|f| f.name == parts[1]))
            .map(|f| f.field_type.clone())
            .ok_or_else(|| {
                self.error(
                    "compile_mir_assign",
                    format!("field '{}' not found on '{}'", parts[1], class_name),
                )
            })?;
        class_name = first_field_ty;

        for field_name in &parts[2..] {
            let llvm_ty = self.coffee_type_to_llvm(&class_name).map_err(|e| {
                self.error("compile_mir_assign", e)
            })?;
            let object_ptr = if matches!(llvm_ty, BasicTypeEnum::PointerType(_)) {
                let ptr_ty = self.backend.context.ptr_type(AddressSpace::default());
                self.backend
                    .builder
                    .build_load(ptr_ty, field_ptr, "nested_obj")
                    .map_err(|e| {
                        self.error(
                            "compile_mir_assign",
                            format!("failed to load nested object '{}': {}", class_name, e),
                        )
                    })?
                    .into_pointer_value()
            } else {
                field_ptr
            };

            let struct_type = *self.type_mapper.struct_types.get(&class_name).ok_or_else(|| {
                self.error(
                    "compile_mir_assign",
                    format!("class '{}' not found in struct_types cache", class_name),
                )
            })?;
            let field_index = self.get_field_index(&struct_type, field_name)?;
            let zero = self.backend.context.i64_type().const_int(0, false);
            let field_index_val = self
                .backend
                .context
                .i32_type()
                .const_int(field_index as u64, false);
            field_ptr = unsafe {
                self.backend.builder.build_in_bounds_gep(
                    struct_type,
                    object_ptr,
                    &[zero, field_index_val],
                    &format!("{}_{}_ptr", class_name, field_name),
                )
            }
            .map_err(|e| {
                self.error(
                    "compile_mir_assign",
                    format!(
                        "failed to build field GEP for '{}.{}': {}",
                        class_name, field_name, e
                    ),
                )
            })?;

            class_name = self
                .classes
                .get(&class_name)
                .and_then(|c| c.fields.iter().find(|f| f.name == *field_name))
                .map(|f| f.field_type.clone())
                .unwrap_or(class_name);
        }

        Ok(field_ptr)
    }

    pub(crate) fn note_mir_name_use(&mut self, name: &str) {
        if let Some(left) = self.mir_name_uses_left.get_mut(name) {
            *left = left.saturating_sub(1);
        }
    }

    fn hir_owned_resource_var<'e>(&self, expr: &'e HirExpr) -> Option<&'e str> {
        if !self.ty_is_owned_resource(&expr.ty) {
            return None;
        }
        match &expr.kind {
            HirExprKind::Variable(n) => Some(n.as_str()),
            _ => None,
        }
    }
}

fn count_mir_name_uses(mir: &MirFn) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for block in &mir.blocks {
        for stmt in &block.stmts {
            match stmt {
                MirStmt::Assign { value, .. } => count_hir_name_uses(value, &mut counts),
                MirStmt::Expr(e) | MirStmt::Raise(e) => count_hir_name_uses(e, &mut counts),
                MirStmt::Branch { cond, .. } => count_hir_name_uses(cond, &mut counts),
                MirStmt::Return(Some(e)) => count_hir_name_uses(e, &mut counts),
                MirStmt::MemoryOp(op) => count_memory_op_uses(op, &mut counts),
                _ => {}
            }
        }
    }
    counts
}

fn count_memory_op_uses(op: &MemoryOp, counts: &mut HashMap<String, usize>) {
    match op {
        MemoryOp::Move { source, .. }
        | MemoryOp::Clone { source, .. }
        | MemoryOp::Copy { source, .. }
        | MemoryOp::Remove { target: source } => {
            *counts.entry(source.clone()).or_insert(0) += 1;
        }
        MemoryOp::RemoveMultiple { targets } => {
            for t in targets {
                *counts.entry(t.clone()).or_insert(0) += 1;
            }
        }
        MemoryOp::CleanOut { targets, .. } => {
            if let Some(ts) = targets {
                for t in ts {
                    *counts.entry(t.clone()).or_insert(0) += 1;
                }
            }
        }
    }
}

fn count_hir_name_uses(expr: &HirExpr, counts: &mut HashMap<String, usize>) {
    match &expr.kind {
        HirExprKind::Variable(n) => {
            *counts.entry(n.clone()).or_insert(0) += 1;
        }
        HirExprKind::FString { placeholders, .. } => {
            for p in placeholders {
                *counts.entry(p.clone()).or_insert(0) += 1;
            }
        }
        HirExprKind::Binary { left, right, .. } => {
            count_hir_name_uses(left, counts);
            count_hir_name_uses(right, counts);
        }
        HirExprKind::Unary { operand, .. } => count_hir_name_uses(operand, counts),
        HirExprKind::Call { function, args } => {
            count_hir_name_uses(function, counts);
            for a in args {
                count_hir_name_uses(a, counts);
            }
        }
        HirExprKind::ConstructorCall { args, .. } => {
            for a in args {
                count_hir_name_uses(a, counts);
            }
        }
        HirExprKind::Member { object, args, .. } => {
            count_hir_name_uses(object, counts);
            for a in args {
                count_hir_name_uses(a, counts);
            }
        }
        HirExprKind::Index { array, index } => {
            count_hir_name_uses(array, counts);
            count_hir_name_uses(index, counts);
        }
        HirExprKind::TupleField { tuple, .. }
        | HirExprKind::EnumTag { value: tuple, .. }
        | HirExprKind::EnumPayload { value: tuple, .. }
        | HirExprKind::Len { collection: tuple } => count_hir_name_uses(tuple, counts),
        HirExprKind::ArrayLiteral { elements } | HirExprKind::TupleLiteral { elements } => {
            for e in elements {
                count_hir_name_uses(e, counts);
            }
        }
        HirExprKind::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                count_hir_name_uses(v, counts);
            }
        }
        HirExprKind::Assign { object, value, .. } => {
            count_hir_name_uses(object, counts);
            count_hir_name_uses(value, counts);
        }
        HirExprKind::TypeCast { value, .. } => count_hir_name_uses(value, counts),
        HirExprKind::Literal(_) | HirExprKind::AnonymousFunction { .. } => {}
    }
}
