//! `buf<T>`: an owned, length-carrying buffer of plain values.
//!
//! In the language it is a safe type (bounds-checked, zero-initialised, freed at scope end);
//! compiled, it is a `{ptr, i64 len}` pair with nothing but the pointer arithmetic left.

use crate::backend::codegen::CodeGenerator;
use crate::hir::HirExpr;
use crate::types::Type;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use inkwell::IntPredicate;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Stop the program with `reason` when `failed` (an `i1`) is true.
    fn emit_failure_if(&mut self, failed: IntValue<'ctx>, reason: &str) -> Result<(), String> {
        let builder = &self.backend.builder;
        let function = builder
            .get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or_else(|| self.error("buf", "no insert block"))?;
        let fail_block = self.backend.context.append_basic_block(function, "buf_fail");
        let ok_block = self.backend.context.append_basic_block(function, "buf_ok");
        builder
            .build_conditional_branch(failed, fail_block, ok_block)
            .map_err(|e| self.error("buf", e.to_string()))?;
        builder.position_at_end(fail_block);
        self.arithmetic_ctx
            .trap(builder, reason)
            .map_err(|e| self.error("buf", e.to_string()))?;
        builder.position_at_end(ok_block);
        Ok(())
    }

    fn buf_elem_llvm(&self, ty: &Type) -> Result<BasicTypeEnum<'ctx>, String> {
        let elem = ty
            .buf_elem_viewed()
            .ok_or_else(|| self.error("buf", format!("'{}' is not a buf<T>", ty)))?;
        self.coffee_type_to_llvm(&elem.to_string())
            .map_err(|e| self.error("buf", e))
    }

    fn elem_size(&self, elem: BasicTypeEnum<'ctx>) -> Result<IntValue<'ctx>, String> {
        use inkwell::types::BasicType;
        let size = elem
            .size_of()
            .ok_or_else(|| self.error("buf", "element type has no size"))?;
        Ok(size)
    }

    pub(crate) fn buf_split(&mut self, value: BasicValueEnum<'ctx>) -> Result<(PointerValue<'ctx>, IntValue<'ctx>), String> {
        // A variable reads as the address of its slot; a call result is the pair itself.
        let sv = match value {
            BasicValueEnum::StructValue(sv) => sv,
            BasicValueEnum::PointerValue(p) => self
                .backend
                .builder
                .build_load(self.type_mapper.slice_fat_type(), p, "buf_pair")
                .map_err(|e| self.error("buf", e.to_string()))?
                .into_struct_value(),
            _ => return Err(self.error("buf", "buf<T> value must be a {ptr, len} pair")),
        };
        let b = &self.backend.builder;
        let data = b.build_extract_value(sv, 0, "buf_data").map_err(|e| self.error("buf", e.to_string()))?;
        let len = b.build_extract_value(sv, 1, "buf_len").map_err(|e| self.error("buf", e.to_string()))?;
        Ok((data.into_pointer_value(), len.into_int_value()))
    }

    fn buf_pack(&mut self, data: PointerValue<'ctx>, len: IntValue<'ctx>) -> Result<BasicValueEnum<'ctx>, String> {
        let fat = self.type_mapper.slice_fat_type();
        let b = &self.backend.builder;
        let with_data = b
            .build_insert_value(fat.get_undef(), data, 0, "buf_with_data")
            .map_err(|e| self.error("buf", e.to_string()))?;
        let full = b
            .build_insert_value(with_data, len, 1, "buf_full")
            .map_err(|e| self.error("buf", e.to_string()))?;
        Ok(full.into_struct_value().into())
    }

    /// Zero-filled allocation of `n` elements; traps on a negative length, size overflow or OOM.
    fn buf_alloc_zeroed(&mut self, n: IntValue<'ctx>, elem: BasicTypeEnum<'ctx>) -> Result<PointerValue<'ctx>, String> {
        let i64_type = self.backend.context.i64_type();
        let negative = self
            .backend
            .builder
            .build_int_compare(IntPredicate::SLT, n, i64_type.const_zero(), "buf_negative")
            .map_err(|e| self.error("buf", e.to_string()))?;
        self.emit_failure_if(negative, "negative buffer length")?;
        let size = self.elem_size(elem)?;
        let bytes = self
            .arithmetic_ctx
            .build_int_mul(self.backend.context, &self.backend.builder, n, size, true)?;
        // `malloc(0)` may return null; ask for at least one byte so null always means failure.
        let one = i64_type.const_int(1, false);
        let nonzero = self
            .backend
            .builder
            .build_int_compare(IntPredicate::EQ, bytes, i64_type.const_zero(), "buf_empty")
            .map_err(|e| self.error("buf", e.to_string()))?;
        let request = self
            .backend
            .builder
            .build_select(nonzero, one, bytes, "buf_request")
            .map_err(|e| self.error("buf", e.to_string()))?
            .into_int_value();
        let data = self.emit_heap_alloc_dyn(request, "buf_alloc")?;
        let is_null = self
            .backend
            .builder
            .build_is_null(data, "buf_oom")
            .map_err(|e| self.error("buf", e.to_string()))?;
        self.emit_failure_if(is_null, "out of memory")?;
        self.backend
            .builder
            .build_memset(data, 1, self.backend.context.i8_type().const_zero(), request)
            .map_err(|e| self.error("buf", e.to_string()))?;
        Ok(data)
    }

    /// Checked address of `buf[index]` and the element's LLVM type.
    pub(crate) fn buf_element_ptr(
        &mut self,
        buf: &HirExpr,
        index: &HirExpr,
    ) -> Result<(PointerValue<'ctx>, BasicTypeEnum<'ctx>), String> {
        let elem = self.buf_elem_llvm(&buf.ty)?;
        let value = self.compile_hir_expr_typed(buf)?;
        let (data, len) = self.buf_split(value)?;
        let i64_type = self.backend.context.i64_type();
        let idx = self.compile_hir_expr_typed(index)?.into_int_value();
        let idx = self
            .backend
            .builder
            .build_int_s_extend_or_bit_cast(idx, i64_type, "buf_idx")
            .map_err(|e| self.error("buf", e.to_string()))?;
        // One unsigned compare covers both `idx < 0` and `idx >= len`.
        let out = self
            .backend
            .builder
            .build_int_compare(IntPredicate::UGE, idx, len, "buf_oob")
            .map_err(|e| self.error("buf", e.to_string()))?;
        self.emit_failure_if(out, "Array index out of bounds")?;
        // SAFETY: `idx` was just checked against the buffer length.
        let ptr = unsafe {
            self.backend
                .builder
                .build_in_bounds_gep(elem, data, &[idx], "buf_elem_ptr")
        }
        .map_err(|e| self.error("buf", e.to_string()))?;
        Ok((ptr, elem))
    }

    pub(crate) fn compile_hir_buf_index(
        &mut self,
        buf: &HirExpr,
        index: &HirExpr,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let (ptr, elem) = self.buf_element_ptr(buf, index)?;
        self.backend
            .builder
            .build_load(elem, ptr, "buf_elem")
            .map_err(|e| self.error("buf", e.to_string()))
    }

    /// `new_buf` / `buf_resize` / `buf_ptr`; `None` when `name` is not one of them.
    pub(crate) fn compile_buf_intrinsic(
        &mut self,
        name: &str,
        args: &[HirExpr],
        full: &HirExpr,
    ) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        let i64_type = self.backend.context.i64_type();
        match name {
            "new_buf" => {
                let elem = self.buf_elem_llvm(&full.ty)?;
                let n = self.compile_hir_expr_typed(&args[0])?.into_int_value();
                let n = self
                    .backend
                    .builder
                    .build_int_s_extend_or_bit_cast(n, i64_type, "buf_n")
                    .map_err(|e| self.error("buf", e.to_string()))?;
                let data = self.buf_alloc_zeroed(n, elem)?;
                Ok(Some(self.buf_pack(data, n)?))
            }
            "buf_resize" => {
                let elem = self.buf_elem_llvm(&full.ty)?;
                let old = self.compile_hir_expr_owned(&args[0])?;
                // The old buffer is consumed (freed below); the caller must not drop it again.
                let (old_data, old_len) = self.buf_split(old)?;
                self.mark_moved_owned_resource_var(&args[0]);
                let n = self.compile_hir_expr_typed(&args[1])?.into_int_value();
                let n = self
                    .backend
                    .builder
                    .build_int_s_extend_or_bit_cast(n, i64_type, "buf_n")
                    .map_err(|e| self.error("buf", e.to_string()))?;
                let data = self.buf_alloc_zeroed(n, elem)?;
                let shorter = self
                    .backend
                    .builder
                    .build_int_compare(IntPredicate::ULT, old_len, n, "buf_shorter")
                    .map_err(|e| self.error("buf", e.to_string()))?;
                let keep = self
                    .backend
                    .builder
                    .build_select(shorter, old_len, n, "buf_keep")
                    .map_err(|e| self.error("buf", e.to_string()))?
                    .into_int_value();
                let size = self.elem_size(elem)?;
                let keep_bytes = self
                    .backend
                    .builder
                    .build_int_mul(keep, size, "buf_keep_bytes")
                    .map_err(|e| self.error("buf", e.to_string()))?;
                self.backend
                    .builder
                    .build_memcpy(data, 1, old_data, 1, keep_bytes)
                    .map_err(|e| self.error("buf", e.to_string()))?;
                let free_fn = self
                    .functions
                    .get("free")
                    .copied()
                    .ok_or_else(|| self.error("buf", "free is not declared"))?;
                self.backend
                    .builder
                    .build_call(free_fn, &[old_data.into()], "buf_free_old")
                    .map_err(|e| self.error("buf", e.to_string()))?;
                // The consumed variable keeps a null data pointer, so the drop that precedes a
                // reassignment (`b = buf_resize(b, n)`) frees nothing.
                if let crate::hir::HirExprKind::Variable(var) = &args[0].kind {
                    if let Some(&(slot, _)) = self.variables.get(var) {
                        let fat = self.type_mapper.slice_fat_type();
                        let field = self
                            .backend
                            .builder
                            .build_struct_gep(fat, slot, 0, "buf_consumed")
                            .map_err(|e| self.error("buf", e.to_string()))?;
                        let null = self.backend.context.ptr_type(inkwell::AddressSpace::default()).const_null();
                        self.backend
                            .builder
                            .build_store(field, null)
                            .map_err(|e| self.error("buf", e.to_string()))?;
                    }
                }
                Ok(Some(self.buf_pack(data, n)?))
            }
            "subslice" => {
                let Type::Slice(elem_ty) = &full.ty else {
                    return Err(self.error("buf", "subslice must produce a slice"));
                };
                let elem = self
                    .coffee_type_to_llvm(&elem_ty.to_string())
                    .map_err(|e| self.error("buf", e))?;
                let source = self.compile_hir_expr_typed(&args[0])?;
                let (data, len) = self.buf_split(source)?;
                let widen = |this: &mut Self, v: IntValue<'ctx>| {
                    this.backend
                        .builder
                        .build_int_s_extend_or_bit_cast(v, i64_type, "slice_bound")
                        .map_err(|e| this.error("buf", e.to_string()))
                };
                let lo = self.compile_hir_expr_typed(&args[1])?.into_int_value();
                let lo = widen(self, lo)?;
                let hi = match args.get(2) {
                    Some(h) => {
                        let h = self.compile_hir_expr_typed(h)?.into_int_value();
                        widen(self, h)?
                    }
                    None => len,
                };
                // Unsigned compares also reject negative bounds.
                let b = &self.backend.builder;
                let lo_past_hi = b
                    .build_int_compare(IntPredicate::UGT, lo, hi, "slice_lo_hi")
                    .map_err(|e| self.error("buf", e.to_string()))?;
                let hi_past_len = b
                    .build_int_compare(IntPredicate::UGT, hi, len, "slice_hi_len")
                    .map_err(|e| self.error("buf", e.to_string()))?;
                let bad = b
                    .build_or(lo_past_hi, hi_past_len, "slice_bad")
                    .map_err(|e| self.error("buf", e.to_string()))?;
                self.emit_failure_if(bad, "Slice range out of bounds")?;
                // SAFETY: `lo <= hi <= len` was just checked.
                let start = unsafe {
                    self.backend.builder.build_in_bounds_gep(elem, data, &[lo], "slice_start")
                }
                .map_err(|e| self.error("buf", e.to_string()))?;
                let count = self
                    .backend
                    .builder
                    .build_int_sub(hi, lo, "slice_len")
                    .map_err(|e| self.error("buf", e.to_string()))?;
                Ok(Some(self.buf_pack(start, count)?))
            }
            "buf_ptr" => {
                let value = self.compile_hir_expr_typed(&args[0])?;
                let (data, _) = self.buf_split(value)?;
                Ok(Some(data.into()))
            }
            _ => Ok(None),
        }
    }
}
