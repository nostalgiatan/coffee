//! Control flow compilation for Coffee compiler
//! 
//! This module handles the compilation of control flow constructs in Coffee,
//! including if/else statements, loops, and break/continue operations. It
//! provides utilities for managing loop contexts, converting values to boolean
//! conditions, and generating appropriate LLVM IR for control flow constructs.
//! 
//! The module ensures proper handling of complex control flow scenarios and
//! maintains the necessary context for break and continue statements within
//! nested loops.

use inkwell::values::BasicValueEnum;

/// Loop context for break/continue handling
/// 
/// The LoopContext structure maintains the necessary information for handling
/// break and continue statements within loops. It stores references to the
/// appropriate basic blocks that should be jumped to when a break or continue
/// statement is encountered during code generation.
/// 
/// Each instance of LoopContext represents a single loop in the code and is
/// pushed onto the loop stack when entering a loop and popped when exiting.
/// This allows for proper handling of nested loops.
#[derive(Debug, Clone)]
pub struct LoopContext<'ctx> {
    /// Block to jump to for break
    pub break_block: inkwell::basic_block::BasicBlock<'ctx>,
    /// Block to jump to for continue
    pub continue_block: inkwell::basic_block::BasicBlock<'ctx>,
}

/// Convert a value to a boolean condition
/// 
/// This function converts a given LLVM value to a boolean condition suitable
/// for use in control flow operations like if statements and loops. It handles
/// different types of values by comparing them to their appropriate zero value:
/// 
/// - For integers: compares with 0 (non-zero values are true)
/// - For floats: compares with 0.0 (non-zero values are true)
/// - For pointers: compares with null pointer (non-null pointers are true)
/// 
/// The function returns an i1 integer value (LLVM's boolean type) that represents
/// the truthiness of the input value. This is essential for generating correct
/// conditional branches in the compiled code.
/// 
/// # Arguments
/// 
/// * `value` - The LLVM value to convert to a boolean condition
/// * `builder` - The LLVM builder to use for generating the comparison instruction
/// 
/// # Returns
/// 
/// * `Ok(IntValue)` - An i1 integer value representing the boolean condition
/// * `Err(String)` - If the value type cannot be converted to a boolean
pub fn value_to_bool<'ctx>(value: BasicValueEnum<'ctx>, builder: &inkwell::builder::Builder<'ctx>) -> Result<inkwell::values::IntValue<'ctx>, String> {
    use inkwell::IntPredicate;
    
    match value {
        BasicValueEnum::IntValue(i) => {
            let zero = i.get_type().const_zero();
            Ok(builder.build_int_compare(
                IntPredicate::NE,
                i,
                zero,
                "tobool"
            ).map_err(|e| format!("failed to build boolean conversion: {}", e))?)
        }
        BasicValueEnum::FloatValue(f) => {
            let zero = f.get_type().const_zero();
            let cmp = builder.build_float_compare(
                inkwell::FloatPredicate::ONE,
                f,
                zero,
                "toboolf"
            ).map_err(|e| format!("failed to build float boolean conversion: {}", e))?;
            Ok(cmp)
        }
        BasicValueEnum::PointerValue(p) => {
            let null_ptr = p.get_type().const_null();
            let i64_type = builder.get_insert_block().unwrap().get_context().i64_type();
            let int_ptr = builder.build_ptr_to_int(p, i64_type, "ptr_to_int")
                .map_err(|e| format!("failed to convert pointer to int: {}", e))?;
            let int_null = builder.build_ptr_to_int(null_ptr, i64_type, "null_to_int")
                .map_err(|e| format!("failed to convert null to int: {}", e))?;
            Ok(builder.build_int_compare(
                inkwell::IntPredicate::NE,
                int_ptr,
                int_null,
                "toptrbool"
            ).map_err(|e| format!("failed to build pointer boolean conversion: {}", e))?)
        }
        _ => Err(format!("cannot convert {:?} to boolean", value))
    }
}

/// Compile break statement
/// 
/// This function generates the LLVM IR for a break statement, which causes
/// an immediate exit from the innermost enclosing loop. It creates an
/// unconditional branch to the break block stored in the topmost LoopContext
/// in the loop stack.
/// 
/// The function first checks if there is a valid loop context in the stack
/// to ensure the break statement is used within a loop. If no loop context
/// exists, it returns an error indicating that break is used outside a loop.
/// 
/// # Arguments
/// 
/// * `builder` - The LLVM builder to use for generating the branch instruction
/// * `loop_stack` - The stack of loop contexts to determine the target block
/// 
/// # Returns
/// 
/// * `Ok(())` - If the break instruction was generated successfully
/// * `Err(String)` - If break is used outside a loop context
pub fn compile_break<'ctx>(
    builder: &inkwell::builder::Builder<'ctx>,
    loop_stack: &[LoopContext<'ctx>],
) -> Result<(), String> {
    if let Some(loop_ctx) = loop_stack.last() {
        builder.build_unconditional_branch(loop_ctx.break_block)
            .map_err(|e| format!("failed to build break: {}", e))?;
        Ok(())
    } else {
        Err("break outside loop".to_string())
    }
}

/// Compile continue statement
/// 
/// This function generates the LLVM IR for a continue statement, which causes
/// an immediate jump to the next iteration of the innermost enclosing loop.
/// It creates an unconditional branch to the continue block stored in the
/// topmost LoopContext in the loop stack.
/// 
/// The function first checks if there is a valid loop context in the stack
/// to ensure the continue statement is used within a loop. If no loop context
/// exists, it returns an error indicating that continue is used outside a loop.
/// 
/// # Arguments
/// 
/// * `builder` - The LLVM builder to use for generating the branch instruction
/// * `loop_stack` - The stack of loop contexts to determine the target block
/// 
/// # Returns
/// 
/// * `Ok(())` - If the continue instruction was generated successfully
/// * `Err(String)` - If continue is used outside a loop context
pub fn compile_continue<'ctx>(
    builder: &inkwell::builder::Builder<'ctx>,
    loop_stack: &[LoopContext<'ctx>],
) -> Result<(), String> {
    if let Some(loop_ctx) = loop_stack.last() {
        builder.build_unconditional_branch(loop_ctx.continue_block)
            .map_err(|e| format!("failed to build continue: {}", e))?;
        Ok(())
    } else {
        Err("continue outside loop".to_string())
    }
}