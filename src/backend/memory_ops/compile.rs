//! LLVM IR generation for clone, move, and remove.

use crate::parser::MemoryOp;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{FunctionValue, PointerValue};

use super::context::MemoryContext;

/// Compile a clone operation (deep copy)
/// 
/// Generates LLVM IR for a clone operation, which creates a deep copy of
/// a variable's value. The clone operation duplicates the value from the
/// source variable to the target variable, ensuring that both variables
/// have independent copies of the data.
/// 
/// In Coffee's ownership system, cloning creates a new, independent copy
/// of the data, allowing both the source and target to be used independently
/// without affecting each other.
/// 
/// # Arguments
/// 
/// * `_context` - The LLVM context (not used directly in this implementation)
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `source` - The name of the source variable to clone from
/// * `target` - The name of the target variable to clone to
/// 
/// # Returns
/// 
/// * `Ok(())` - If the clone operation was compiled successfully
/// * `Err(String)` - If there was an error during compilation
pub fn compile_clone<'ctx>(
    context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    source: &str,
    target: &str,
) -> Result<(), String> {
    compile_clone_impl(context, builder, variables, None, None, source, target)
}

fn compile_clone_impl<'ctx>(
    context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    memory_ctx: Option<&mut MemoryContext<'ctx>>,
    functions: Option<&std::collections::HashMap<String, FunctionValue<'ctx>>>,
    source: &str,
    target: &str,
) -> Result<(), String> {
    let Some(&(src_ptr, src_type)) = variables.get(source) else {
        return Err(format!("memory clone: source variable '{}' not found", source));
    };

    let value = builder
        .build_load(src_type, src_ptr, "clone_val")
        .map_err(|e| format!("memory clone: failed to load from '{}': {}", source, e))?;

    let type_name = memory_ctx
        .as_ref()
        .and_then(|ctx| ctx.get_variable_type(source).cloned());
    let parsed = type_name
        .as_ref()
        .and_then(|s| crate::types::type_from_str(s).ok());

    let dst_ptr = if let Some(&(existing, _)) = variables.get(target) {
        existing
    } else {
        let alloca = builder
            .build_alloca(src_type, target)
            .map_err(|e| format!("memory clone: failed to allocate '{}': {}", target, e))?;
        variables.insert(target.to_string(), (alloca, src_type));
        alloca
    };

    builder
        .build_store(dst_ptr, value)
        .map_err(|e| format!("memory clone: failed to store to '{}': {}", target, e))?;

    let empty = std::collections::HashMap::new();
    let fns = functions.unwrap_or(&empty);
    if let Some(ref ty) = parsed {
        super::clone::clone_coffee_place(context, builder, fns, ty, dst_ptr, src_type)?;
    }

    let did_heap_clone = parsed
        .as_ref()
        .map(super::clone::clone_allocates_heap)
        .unwrap_or(false);
    if did_heap_clone {
        if let Some(ctx) = memory_ctx {
            ctx.mark_heap_allocated(target);
            if let Some(t) = type_name {
                ctx.set_variable_type(target.to_string(), t);
            }
        }
    }
    Ok(())
}

/// Compile a move operation (transfer ownership)
/// 
/// Generates LLVM IR for a move operation, which transfers ownership of
/// a variable's value from the source to the target. The move operation
/// invalidates the source variable after the transfer, preventing further
/// use without re-initialization.
/// 
/// In Coffee's ownership system, moving transfers ownership from one variable
/// to another, ensuring that there is only one owner of any data at a time.
/// After a move, the source variable is zeroed out to prevent information
/// leaks and marked as moved in the memory context.
/// 
/// # Arguments
/// 
/// * `_context` - The LLVM context (not used directly in this implementation)
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `memory_ctx` - The memory context to track ownership changes
/// * `source` - The name of the source variable to move from
/// * `target` - The name of the target variable to move to
/// 
/// # Returns
/// 
/// * `Ok(())` - If the move operation was compiled successfully
/// * `Err(String)` - If there was an error during compilation
pub fn compile_move<'ctx>(
    _context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    memory_ctx: &mut MemoryContext<'ctx>,
    source: &str,
    target: &str,
) -> Result<(), String> {
    if let Some(&(src_ptr, src_type)) = variables.get(source) {
        let value = builder.build_load(src_type, src_ptr, "move_val")
            .map_err(|e| format!("memory move: failed to load from '{}': {}", source, e))?;

        let dst_ptr = if let Some(&(existing, _)) = variables.get(target) {
            existing
        } else {
            let alloca = builder.build_alloca(src_type, target)
                .map_err(|e| format!("memory move: failed to allocate '{}': {}", target, e))?;
            variables.insert(target.to_string(), (alloca, src_type));
            alloca
        };

        builder.build_store(dst_ptr, value)
            .map_err(|e| format!("memory move: failed to store to '{}': {}", target, e))?;

        // CRITICAL-9 FIX: Zero the source after move to prevent information leaks
        // Store zeros to invalidate the source data
        
        match src_type {
            BasicTypeEnum::IntType(t) => {
                let zero = t.const_zero();
                builder.build_store(src_ptr, zero)
                    .map_err(|e| format!("memory move: failed to invalidate '{}': {}", source, e))?;
            }
            BasicTypeEnum::FloatType(t) => {
                let zero = t.const_zero();
                builder.build_store(src_ptr, zero)
                    .map_err(|e| format!("memory move: failed to invalidate '{}': {}", source, e))?;
            }
            BasicTypeEnum::PointerType(t) => {
                let zero = t.const_zero();
                builder.build_store(src_ptr, zero)
                    .map_err(|e| format!("memory move: failed to invalidate '{}': {}", source, e))?;
            }
            _ => {
                // For other types, try to zero them generically
                // This is a best-effort approach
            }
        }

        // Mark source as moved
        memory_ctx.mark_moved(source.to_string());
    } else {
        return Err(format!("memory move: source variable '{}' not found", source));
    }
    Ok(())
}

/// Compile a remove operation (deallocate/invalidate)
/// 
/// Generates LLVM IR for a remove operation, which invalidates a variable
/// and prevents its further use. The remove operation zeros out the variable's
/// value to prevent information leaks and marks it as dropped in the memory
/// context to prevent double-free errors.
/// 
/// In Coffee's explicit resource management system, variables must be
/// explicitly removed when no longer needed. This operation handles the
/// safe invalidation of stack-allocated variables. For heap-allocated
/// objects, explicit free() calls should be used before remove.
/// 
/// # Arguments
/// 
/// * `_context` - The LLVM context (not used directly in this implementation)
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `memory_ctx` - The memory context to track the removal
/// * `target` - The name of the variable to remove
/// 
/// # Returns
/// 
/// * `Ok(())` - If the remove operation was compiled successfully
/// * `Err(String)` - If there was an error during compilation (e.g., double-free)
pub fn compile_remove<'ctx>(
    context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    memory_ctx: &mut MemoryContext<'ctx>,
    functions: &std::collections::HashMap<String, inkwell::values::FunctionValue<'ctx>>,
    target: &str,
    track_lifetime: bool,
) -> Result<(), String> {
    // Path-local auto-drop must not poison sibling CFG branches.
    if memory_ctx.is_dropped(target) {
        if track_lifetime {
            return Err(format!("memory remove: double-free detected - variable '{}' was already removed", target));
        }
        return Ok(());
    }

    if let Some(&(ptr, var_type)) = variables.get(target) {
        let parsed = memory_ctx
            .get_variable_type(target)
            .and_then(|s| crate::types::type_from_str(s).ok());
        let composite = matches!(
            parsed,
            Some(crate::types::Type::Tuple(_))
                | Some(crate::types::Type::Array { .. })
                | Some(crate::types::Type::Slice(_))
        );
        let is_buf = parsed.as_ref().is_some_and(|t| t.is_buf());

        if is_buf {
            if let Some(ty) = parsed.as_ref() {
                super::drop::drop_coffee_place(context, builder, functions, ty, ptr, var_type)?;
            }
        } else if composite {
            if let Some(ty) = parsed.as_ref() {
                super::drop::drop_coffee_place(context, builder, functions, ty, ptr, var_type)?;
            }
        }

        // Check if this is a class instance (struct type)
        let is_class = matches!(var_type, BasicTypeEnum::StructType(_)) && !composite;
        
        // For class instances, call the drop function first
        if is_class {
            let class_name = memory_ctx.get_variable_type(target);
            if let Some(class_name) = class_name {
                if memory_ctx.c_value_names.contains(class_name) {
                    // C struct/union: bitwise copy, no Coffee `__drop`.
                } else {
                let drop_func_name = format!("{}__drop", class_name);
                if let Some(&drop_func) = functions.get(&drop_func_name) {
                    builder.build_call(drop_func, &[ptr.into()], &format!("{}_drop_call", target))
                        .map_err(|e| format!("memory remove: failed to call drop function: {}", e))?;
                } else {
                    return Err(format!("memory remove: drop function '{}' not found for class instance '{}'\n  = note: this indicates the class was not properly initialized or the drop function was not generated\n  = help: ensure the class has a constructor (fn new()) defined", drop_func_name, target));
                }
                }
            }
        }
        
        // For pointer types, drop the class (if known) then free heap memory.
        // `buf` already called `drop_coffee_place` (`free`); skip class/`is_heap_allocated`.
        if !is_buf {
        if let BasicTypeEnum::PointerType(_) = var_type {
            let class_name = memory_ctx.get_variable_type(target).cloned();
            let drop_func = class_name
                .as_ref()
                .and_then(|class_name| functions.get(&format!("{}__drop", class_name)).copied());
            // Only free if the pointer is heap-allocated
            let should_free = memory_ctx.lifetimes.get(target)
                .map(|info| info.is_heap_allocated)
                .unwrap_or(false);
            
            if drop_func.is_some() || should_free {
                // Load the pointer value
                let loaded_ptr = builder.build_load(var_type, ptr, &format!("{}_load", target))
                    .map_err(|e| format!("memory remove: failed to load pointer '{}': {}", target, e))?;

                if let Some(drop_func) = drop_func {
                    builder.build_call(
                        drop_func,
                        &[loaded_ptr.into()],
                        &format!("{}_drop_call", target),
                    )
                    .map_err(|e| format!("memory remove: failed to call drop function: {}", e))?;
                }
                
                // Only free if free function is available (means user used malloc)
                if should_free {
                    if let Some(&free_fn) = functions.get("free") {
                        let i8_ptr_type = context.ptr_type(inkwell::AddressSpace::default());
                        let casted_ptr = builder.build_bit_cast(
                            loaded_ptr.into_pointer_value(),
                            i8_ptr_type,
                            &format!("{}_cast", target)
                        ).map_err(|e| format!("memory remove: failed to cast pointer: {}", e))?;

                        builder.build_call(free_fn, &[casted_ptr.into()], &format!("{}_free_call", target))
                            .map_err(|e| format!("memory remove: failed to call free: {}", e))?;
                    }
                }
            }
        }
        }
        
        // CRITICAL-9 FIX: Zero the allocation to prevent information leaks
        match var_type {
            BasicTypeEnum::IntType(t) => {
                let zero = t.const_zero();
                builder.build_store(ptr, zero)
                    .map_err(|e| format!("memory remove: failed to invalidate '{}': {}", target, e))?;
            }
            BasicTypeEnum::FloatType(t) => {
                let zero = t.const_zero();
                builder.build_store(ptr, zero)
                    .map_err(|e| format!("memory remove: failed to invalidate '{}': {}", target, e))?;
            }
            BasicTypeEnum::PointerType(t) => {
                let zero = t.const_zero();
                builder.build_store(ptr, zero)
                    .map_err(|e| format!("memory remove: failed to invalidate '{}': {}", target, e))?;
            }
            BasicTypeEnum::StructType(_) => {
                // For structs, we've already called the drop function if available
                // Just mark as dropped
            }
            _ => {
                // For other types, best effort
            }
        }

        if track_lifetime {
            memory_ctx.mark_dropped(target.to_string());
            // Keep the alloca in `variables` so other CFG blocks compiled later
            // can still name it. Path-local use-after-rm is `is_dropped`.
        }
    }
    Ok(())
}

/// Compile any memory operation
/// 
/// Dispatches and compiles the appropriate memory operation based on the
/// provided MemoryOp enum. This function serves as a central entry point
/// for compiling all types of memory operations in the Coffee language,
/// including clone, copy, move, remove, and clean operations.
/// 
/// The function handles the execution of the specific memory operation
/// and updates the memory context accordingly, ensuring proper lifetime
/// tracking and ownership management.
/// 
/// # Arguments
/// 
/// * `op` - The memory operation to compile
/// * `context` - The LLVM context
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `memory_ctx` - The memory context to track the operation's effects
/// 
/// # Returns
/// 
/// * `Ok(())` - If the memory operation was compiled successfully
/// * `Err(String)` - If there was an error during compilation
pub fn compile_memory_op<'ctx>(
    op: &MemoryOp,
    context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    memory_ctx: &mut MemoryContext<'ctx>,
    functions: &std::collections::HashMap<String, inkwell::values::FunctionValue<'ctx>>,
) -> Result<(), String> {
    match op {
        MemoryOp::Clone { source, target } => {
            compile_clone_impl(
                context,
                builder,
                variables,
                Some(memory_ctx),
                Some(functions),
                source,
                target,
            )
        }
        MemoryOp::Copy { source: _, target: _ } => {
            Err("copy is a type error; the copy keyword was removed — use clone or mv".to_string())
        }
        MemoryOp::Move { source, target } => {
            compile_move(context, builder, variables, memory_ctx, source, target)
        }
        MemoryOp::Remove { target } => {
            compile_remove(context, builder, variables, memory_ctx, functions, target, true)
        }
        MemoryOp::RemoveMultiple { targets } => {
            compile_remove_multiple(context, builder, variables, memory_ctx, functions, targets)
        }
        MemoryOp::CleanOut { .. } => {
            Err("memory clean out: clean out was removed; values drop at scope end".to_string())
        }
    }
}

/// Compile batch remove operation
/// 
/// Generates LLVM IR for removing multiple variables in a single operation.
/// This function iterates through the list of target variables and performs
/// a remove operation on each one, invalidating them and preventing further use.
/// 
/// The batch operation is commonly used in 'clean out' operations that
/// remove multiple variables at once, such as cleaning all variables in
/// a scope or cleaning specific variables as directed by the source code.
/// 
/// # Arguments
/// 
/// * `_context` - The LLVM context (not used directly in this implementation)
/// * `builder` - The LLVM builder to use for instruction generation
/// * `variables` - The map of variable names to their LLVM values and types
/// * `memory_ctx` - The memory context to track the removals
/// * `targets` - A slice of variable names to remove
/// 
/// # Returns
/// 
/// * `Ok(())` - If all remove operations were compiled successfully
/// * `Err(String)` - If there was an error during compilation of any removal
pub fn compile_remove_multiple<'ctx>(
    context: &'ctx inkwell::context::Context,
    builder: &inkwell::builder::Builder<'ctx>,
    variables: &mut std::collections::HashMap<String, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>,
    memory_ctx: &mut MemoryContext<'ctx>,
    functions: &std::collections::HashMap<String, inkwell::values::FunctionValue<'ctx>>,
    targets: &[String],
) -> Result<(), String> {
    for target in targets {
        compile_remove(context, builder, variables, memory_ctx, functions, target, true)?;
    }
    Ok(())
}
