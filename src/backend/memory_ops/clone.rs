//! Type-driven deep clone for `clone` (nested `str` / class fields).
//!
//! After a shallow copy of a place, `clone_coffee_place` replaces resource
//! pointers. `object`, refs, and slices stay memcpy-only (no pointee clone).

use std::collections::HashMap;

use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{FunctionValue, PointerValue};
use crate::types::Type;

struct OwnedModuleGuard<'ctx>(std::mem::ManuallyDrop<inkwell::module::Module<'ctx>>);

impl<'ctx> std::ops::Deref for OwnedModuleGuard<'ctx> {
    type Target = inkwell::module::Module<'ctx>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn module_from_builder<'ctx>(
    builder: &inkwell::builder::Builder<'ctx>,
) -> Result<OwnedModuleGuard<'ctx>, String> {
    let block = builder
        .get_insert_block()
        .ok_or_else(|| "memory clone: builder has no insert block".to_string())?;
    let function = block
        .get_parent()
        .ok_or_else(|| "memory clone: insert block has no parent function".to_string())?;
    use inkwell::values::AsValueRef;
    let mref = unsafe { inkwell::llvm_sys::core::LLVMGetGlobalParent(function.as_value_ref()) };
    if mref.is_null() {
        return Err("memory clone: function has no parent module".to_string());
    }
    Ok(OwnedModuleGuard(std::mem::ManuallyDrop::new(unsafe {
        inkwell::module::Module::new(mref)
    })))
}

fn get_or_add_fn<'ctx>(
    module: &inkwell::module::Module<'ctx>,
    functions: Option<&HashMap<String, FunctionValue<'ctx>>>,
    name: &str,
    make_ty: impl FnOnce() -> inkwell::types::FunctionType<'ctx>,
) -> Result<FunctionValue<'ctx>, String> {
    if let Some(map) = functions {
        if let Some(&f) = map.get(name) {
            return Ok(f);
        }
    }
    if let Some(f) = module.get_function(name) {
        return Ok(f);
    }
    Ok(module.add_function(name, make_ty(), None))
}

fn clone_pointer_bytes<'ctx>(
    context: &'ctx Context,
    builder: &inkwell::builder::Builder<'ctx>,
    module: &inkwell::module::Module<'ctx>,
    functions: Option<&HashMap<String, FunctionValue<'ctx>>>,
    src_obj: PointerValue<'ctx>,
    byte_count: inkwell::values::IntValue<'ctx>,
) -> Result<PointerValue<'ctx>, String> {
    let malloc_fn = get_or_add_fn(module, functions, "malloc", || {
        crate::backend::functions::c_abi_fn_type(context, "object", &["int"], false)
            .expect("libc.cfc malloc is object(int)")
    })?;
    let dest_call = builder
        .build_call(malloc_fn, &[byte_count.into()], "clone_malloc")
        .map_err(|e| format!("memory clone: malloc: {}", e))?;
    let dest = match dest_call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(inkwell::values::BasicValueEnum::PointerValue(p)) => p,
        _ => return Err("memory clone: malloc did not return a pointer".to_string()),
    };
    builder
        .build_memcpy(dest, 1, src_obj, 1, byte_count)
        .map_err(|e| format!("memory clone: memcpy: {}", e))?;
    Ok(dest)
}

fn struct_nbytes<'ctx>(
    context: &'ctx Context,
    module: &inkwell::module::Module<'ctx>,
    st: inkwell::types::StructType<'ctx>,
) -> inkwell::values::IntValue<'ctx> {
    let dl = module.get_data_layout();
    let td = inkwell::targets::TargetData::create(dl.as_str().to_str().unwrap_or("e"));
    let size = td.get_store_size(&BasicTypeEnum::StructType(st));
    context.i64_type().const_int(size, false)
}

/// Deepen a shallow-copied place: clone nested `str` / class / array / tuple
/// resources. Slice, `object`, and refs are left as the memcpy'd pointers.
pub fn clone_coffee_place<'ctx>(
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
        | Type::Slice(_)
        | Type::App { .. } => Ok(()),
        Type::String => clone_string(context, builder, functions, ptr, llvm_ty),
        // `buf` has no length; clone is a type error. Class `__clone` memcpy-only.
        Type::NamedType { name } if name == "buf" => Ok(()),
        Type::NamedType { name } => clone_named(context, builder, functions, name, ptr, llvm_ty),
        Type::Tuple(elems) => clone_tuple(context, builder, functions, elems, ptr, llvm_ty),
        Type::Array { elem, size } => {
            clone_array(context, builder, functions, elem, *size, ptr, llvm_ty)
        }
    }
}

/// True when clone allocated a new heap buffer for the place (not slice/object alias).
pub fn clone_allocates_heap(ty: &Type) -> bool {
    match ty {
        Type::NamedType { name } if name == "buf" => false,
        Type::String | Type::NamedType { .. } => true,
        Type::Array { elem, .. } => elem.is_resource() && !matches!(
            elem.as_ref(),
            Type::Slice(_) | Type::Variadic | Type::Ref { .. }
        ) && !elem.is_buf(),
        Type::Tuple(elems) => elems.iter().any(clone_allocates_heap),
        _ => false,
    }
}

fn clone_string<'ctx>(
    context: &'ctx Context,
    builder: &inkwell::builder::Builder<'ctx>,
    functions: &HashMap<String, FunctionValue<'ctx>>,
    ptr: PointerValue<'ctx>,
    llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<(), String> {
    let loaded = builder
        .build_load(llvm_ty, ptr, "clone_str")
        .map_err(|e| format!("memory clone: failed to load str: {e}"))?;
    let src_obj = loaded.into_pointer_value();
    let module = module_from_builder(builder)?;
    let i64_ty = context.i64_type();
    let strlen_fn = get_or_add_fn(&module, Some(functions), "strlen", || {
        crate::backend::functions::c_abi_fn_type(context, "int(4)+", &["string"], false)
            .expect("libc.cfc strlen is int(4)+(string)")
    })?;
    let len_call = builder
        .build_call(strlen_fn, &[src_obj.into()], "clone_strlen")
        .map_err(|e| format!("memory clone: strlen: {}", e))?;
    let len = match len_call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(inkwell::values::BasicValueEnum::IntValue(v)) => v,
        _ => return Err("memory clone: strlen did not return an integer".to_string()),
    };
    let len64 = if len.get_type().get_bit_width() < 64 {
        builder
            .build_int_z_extend(len, i64_ty, "clone_strlen_i64")
            .map_err(|e| format!("memory clone: zext strlen: {e}"))?
    } else {
        len
    };
    let one = i64_ty.const_int(1, false);
    let nbytes = builder
        .build_int_add(len64, one, "clone_strlen_plus_1")
        .map_err(|e| format!("memory clone: strlen+1: {}", e))?;
    let dest = clone_pointer_bytes(context, builder, &module, Some(functions), src_obj, nbytes)?;
    builder
        .build_store(ptr, dest)
        .map_err(|e| format!("memory clone: failed to store cloned str: {e}"))?;
    Ok(())
}

fn clone_named<'ctx>(
    context: &'ctx Context,
    builder: &inkwell::builder::Builder<'ctx>,
    functions: &HashMap<String, FunctionValue<'ctx>>,
    name: &str,
    ptr: PointerValue<'ctx>,
    llvm_ty: BasicTypeEnum<'ctx>,
) -> Result<(), String> {
    let Some(st) = context.get_struct_type(name) else {
        return Ok(());
    };
    let clone_name = format!("{name}__clone");
    let clone_fn = functions.get(&clone_name).copied();
    match llvm_ty {
        BasicTypeEnum::PointerType(_) => {
            let loaded = builder
                .build_load(llvm_ty, ptr, "clone_named")
                .map_err(|e| format!("memory clone: failed to load '{name}': {e}"))?;
            let src_obj = loaded.into_pointer_value();
            let module = module_from_builder(builder)?;
            let nbytes = struct_nbytes(context, &module, st);
            let dest = clone_pointer_bytes(
                context,
                builder,
                &module,
                Some(functions),
                src_obj,
                nbytes,
            )?;
            if let Some(clone_fn) = clone_fn {
                builder
                    .build_call(clone_fn, &[dest.into()], "clone_named_call")
                    .map_err(|e| format!("memory clone: failed to call '{clone_name}': {e}"))?;
            }
            builder
                .build_store(ptr, dest)
                .map_err(|e| format!("memory clone: failed to store cloned '{name}': {e}"))?;
            Ok(())
        }
        BasicTypeEnum::StructType(_) => {
            if let Some(clone_fn) = clone_fn {
                builder
                    .build_call(clone_fn, &[ptr.into()], "clone_named_inplace")
                    .map_err(|e| format!("memory clone: failed to call '{clone_name}': {e}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn clone_tuple<'ctx>(
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
    for (i, elem) in elems.iter().enumerate() {
        if !elem.is_resource() {
            continue;
        }
        let idx = context.i32_type().const_int(i as u64, false);
        let field_ptr = unsafe {
            builder.build_in_bounds_gep(struct_ty, ptr, &[zero, idx], "clone_tuple_elem")
        }
        .map_err(|e| format!("clone: tuple GEP {i}: {e}"))?;
        let field_llvm = struct_ty.get_field_type_at_index(i as u32).ok_or_else(|| {
            format!("clone: missing LLVM tuple field {i}")
        })?;
        clone_coffee_place(context, builder, functions, elem, field_ptr, field_llvm)?;
    }
    Ok(())
}

fn llvm_array_of<'ctx>(elem: BasicTypeEnum<'ctx>, n: u32) -> inkwell::types::ArrayType<'ctx> {
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

fn clone_array<'ctx>(
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
                .build_load(llvm_ty, ptr, "clone_arr_base")
                .map_err(|e| format!("clone: failed to load array pointer: {e}"))?
                .into_pointer_value();
            (loaded, llvm_array_of(elem_llvm, size as u32))
        }
        _ => return Ok(()),
    };
    let zero = context.i32_type().const_int(0, false);
    for i in 0..size {
        let idx = context.i32_type().const_int(i as u64, false);
        let elem_ptr = unsafe {
            builder.build_in_bounds_gep(arr_ty, base, &[zero, idx], "clone_arr_elem")
        }
        .map_err(|e| format!("clone: array GEP {i}: {e}"))?;
        clone_coffee_place(context, builder, functions, elem, elem_ptr, elem_llvm)?;
    }
    Ok(())
}
