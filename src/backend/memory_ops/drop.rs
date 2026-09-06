//! Type-driven resource drop for class `__drop` and `compile_remove`.

use std::collections::HashMap;

use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{FunctionValue, PointerValue};
use inkwell::AddressSpace;

use crate::types::Type;

pub fn drop_coffee_place<'ctx>(
    context: &'ctx Context,
    builder: &inkwell::builder::Builder<'ctx>,
    functions: &HashMap<String, FunctionValue<'ctx>>,
    ty: &Type,
    ptr: PointerValue<'ctx>,
    llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<(), String> {
    match ty {
        Type::Int { .. }
        | Type::Float { .. }
        | Type::Bool
        | Type::Unit
        | Type::Void
        | Type::Function { .. }
        | Type::Variadic
        | Type::Ref { .. }
        | Type::App { .. } => Ok(()),
        Type::Slice(elem) => drop_slice(context, builder, functions, elem, ptr, llvm_ty),
        Type::String => drop_string(context, builder, functions, ptr, llvm_ty),
        Type::NamedType { name } if name == "buf" => {
            drop_string(context, builder, functions, ptr, llvm_ty)
        }
        Type::NamedType { name } => drop_named(builder, functions, name, ptr, llvm_ty),
        Type::Tuple(elems) => drop_tuple(context, builder, functions, elems, ptr, llvm_ty),
        Type::Array { elem, size } => {
            drop_array(context, builder, functions, elem, *size, ptr, llvm_ty)
        }
    }
}

fn drop_string<'ctx>(
    context: &'ctx Context,
    builder: &inkwell::builder::Builder<'ctx>,
    functions: &HashMap<String, FunctionValue<'ctx>>,
    ptr: PointerValue<'ctx>,
    llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<(), String> {
    let Some(&free_fn) = functions.get("free") else {
        return Ok(());
    };
    let loaded = builder
        .build_load(llvm_ty, ptr, "drop_str")
        .map_err(|e| format!("drop: failed to load str: {e}"))?;
    let i8_ptr = context.ptr_type(AddressSpace::default());
    let casted = builder
        .build_bit_cast(loaded.into_pointer_value(), i8_ptr, "drop_str_cast")
        .map_err(|e| format!("drop: failed to cast str: {e}"))?;
    builder
        .build_call(free_fn, &[casted.into()], "drop_str_free")
        .map_err(|e| format!("drop: failed to free str: {e}"))?;
    Ok(())
}

fn drop_named<'ctx>(
    builder: &inkwell::builder::Builder<'ctx>,
    functions: &HashMap<String, FunctionValue<'ctx>>,
    name: &str,
    ptr: PointerValue<'ctx>,
    llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<(), String> {
    let drop_name = format!("{name}__drop");
    let Some(&drop_fn) = functions.get(&drop_name) else {
        return Ok(());
    };
    let arg: inkwell::values::BasicMetadataValueEnum = if matches!(llvm_ty, BasicTypeEnum::PointerType(_))
    {
        builder
            .build_load(llvm_ty, ptr, "drop_named")
            .map_err(|e| format!("drop: failed to load '{name}': {e}"))?
            .into()
    } else {
        ptr.into()
    };
    builder
        .build_call(drop_fn, &[arg], "drop_named_call")
        .map_err(|e| format!("drop: failed to call '{drop_name}': {e}"))?;
    Ok(())
}

fn drop_tuple<'ctx>(
    context: &'ctx Context,
    builder: &inkwell::builder::Builder<'ctx>,
    functions: &HashMap<String, FunctionValue<'ctx>>,
    elems: &[Type],
    ptr: PointerValue<'ctx>,
    llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<(), String> {
    let struct_ty = match llvm_ty {
        BasicTypeEnum::StructType(st) => st,
        _ => return Ok(()),
    };
    let zero = context.i32_type().const_int(0, false);
    for (i, elem) in elems.iter().enumerate().rev() {
        let idx = context.i32_type().const_int(i as u64, false);
        let field_ptr = unsafe {
            builder.build_in_bounds_gep(struct_ty, ptr, &[zero, idx], "drop_tuple_elem")
        }
        .map_err(|e| format!("drop: tuple GEP {i}: {e}"))?;
        let field_llvm = struct_ty.get_field_type_at_index(i as u32).ok_or_else(|| {
            format!("drop: missing LLVM tuple field {i}")
        })?;
        drop_coffee_place(context, builder, functions, elem, field_ptr, field_llvm)?;
    }
    Ok(())
}

fn llvm_array_of<'ctx>(
    elem: BasicTypeEnum<'ctx>,
    n: u32,
) -> inkwell::types::ArrayType<'ctx> {
    match elem {
        BasicTypeEnum::IntType(t) => t.array_type(n),
        BasicTypeEnum::FloatType(t) => t.array_type(n),
        BasicTypeEnum::PointerType(t) => t.array_type(n),
        BasicTypeEnum::StructType(t) => t.array_type(n),
        BasicTypeEnum::ArrayType(t) => t.array_type(n),
        BasicTypeEnum::VectorType(t) => t.array_type(n),
        BasicTypeEnum::ScalableVectorType(t) => t.array_type(n),
    }
}

fn drop_array<'ctx>(
    context: &'ctx Context,
    builder: &inkwell::builder::Builder<'ctx>,
    functions: &HashMap<String, FunctionValue<'ctx>>,
    elem: &Type,
    size: usize,
    ptr: PointerValue<'ctx>,
    llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<(), String> {
    if size == 0 || !elem.is_resource() {
        return Ok(());
    }
    let elem_llvm = crate::backend::types::TypeMapper::llvm_abi_of(context, elem);
    let (base, arr_ty) = match llvm_ty {
        BasicTypeEnum::ArrayType(at) => (ptr, at),
        BasicTypeEnum::PointerType(_) => {
            let loaded = builder
                .build_load(llvm_ty, ptr, "drop_arr_base")
                .map_err(|e| format!("drop: failed to load array pointer: {e}"))?
                .into_pointer_value();
            (loaded, llvm_array_of(elem_llvm, size as u32))
        }
        _ => return Ok(()),
    };
    let zero = context.i32_type().const_int(0, false);
    for i in (0..size).rev() {
        let idx = context.i32_type().const_int(i as u64, false);
        let elem_ptr = unsafe {
            builder.build_in_bounds_gep(arr_ty, base, &[zero, idx], "drop_arr_elem")
        }
        .map_err(|e| format!("drop: array GEP {i}: {e}"))?;
        drop_coffee_place(context, builder, functions, elem, elem_ptr, elem_llvm)?;
    }
    Ok(())
}

fn drop_slice<'ctx>(
    context: &'ctx Context,
    builder: &inkwell::builder::Builder<'ctx>,
    functions: &HashMap<String, FunctionValue<'ctx>>,
    elem: &Type,
    ptr: PointerValue<'ctx>,
    llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<(), String> {
    if !elem.is_resource() {
        return Ok(());
    }
    let struct_ty = match llvm_ty {
        BasicTypeEnum::StructType(st) => st,
        _ => return Ok(()),
    };
    let Some(block) = builder.get_insert_block() else {
        return Ok(());
    };
    let Some(function) = block.get_parent() else {
        return Ok(());
    };
    let i32_ty = context.i32_type();
    let i64_ty = context.i64_type();
    let zero32 = i32_ty.const_int(0, false);
    let one32 = i32_ty.const_int(1, false);
    let zero64 = i64_ty.const_int(0, false);
    let one64 = i64_ty.const_int(1, false);
    let data_field = unsafe {
        builder.build_in_bounds_gep(struct_ty, ptr, &[zero32, zero32], "slice_data_field")
    }
    .map_err(|e| format!("drop: slice data GEP: {e}"))?;
    let len_field = unsafe {
        builder.build_in_bounds_gep(struct_ty, ptr, &[zero32, one32], "slice_len_field")
    }
    .map_err(|e| format!("drop: slice len GEP: {e}"))?;
    let data_llvm = struct_ty
        .get_field_type_at_index(0)
        .ok_or_else(|| "drop: slice missing ptr field".to_string())?;
    let len_llvm = struct_ty
        .get_field_type_at_index(1)
        .ok_or_else(|| "drop: slice missing len field".to_string())?;
    let data = builder
        .build_load(data_llvm, data_field, "slice_data")
        .map_err(|e| format!("drop: load slice data: {e}"))?
        .into_pointer_value();
    let len = builder
        .build_load(len_llvm, len_field, "slice_len")
        .map_err(|e| format!("drop: load slice len: {e}"))?
        .into_int_value();
    let elem_llvm = crate::backend::types::TypeMapper::llvm_abi_of(context, elem);
    let idx_alloca = builder
        .build_alloca(i64_ty, "slice_drop_i")
        .map_err(|e| format!("drop: slice idx alloca: {e}"))?;
    builder
        .build_store(idx_alloca, len)
        .map_err(|e| format!("drop: store slice idx: {e}"))?;
    let loop_bb = context.append_basic_block(function, "slice_drop_loop");
    let body_bb = context.append_basic_block(function, "slice_drop_body");
    let done_bb = context.append_basic_block(function, "slice_drop_done");
    builder
        .build_unconditional_branch(loop_bb)
        .map_err(|e| format!("drop: slice br loop: {e}"))?;
    builder.position_at_end(loop_bb);
    let cur = builder
        .build_load(i64_ty, idx_alloca, "slice_drop_cur")
        .map_err(|e| format!("drop: load slice idx: {e}"))?
        .into_int_value();
    let done = builder
        .build_int_compare(inkwell::IntPredicate::EQ, cur, zero64, "slice_drop_done_cmp")
        .map_err(|e| format!("drop: slice done cmp: {e}"))?;
    builder
        .build_conditional_branch(done, done_bb, body_bb)
        .map_err(|e| format!("drop: slice cond br: {e}"))?;
    builder.position_at_end(body_bb);
    let next = builder
        .build_int_sub(cur, one64, "slice_drop_next")
        .map_err(|e| format!("drop: slice idx sub: {e}"))?;
    builder
        .build_store(idx_alloca, next)
        .map_err(|e| format!("drop: store slice next: {e}"))?;
    let elem_ptr = unsafe {
        builder.build_in_bounds_gep(elem_llvm, data, &[next], "slice_drop_elem")
    }
    .map_err(|e| format!("drop: slice elem GEP: {e}"))?;
    drop_coffee_place(context, builder, functions, elem, elem_ptr, elem_llvm)?;
    builder
        .build_unconditional_branch(loop_bb)
        .map_err(|e| format!("drop: slice br back: {e}"))?;
    builder.position_at_end(done_bb);
    Ok(())
}
