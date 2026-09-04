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
use inkwell::IntPredicate;

use super::codegen::CodeGenerator;

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

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile if expression
    pub fn compile_if(&mut self, if_expr: &crate::parser::IfExpr) -> Result<(), String> {
        let function = self.current_function
            .ok_or_else(|| self.error("compile_if", "if expression outside function context"))?;

        // Compile condition
        let cond_val = self.compile_expr(&if_expr.condition)?;
        let cond_bool = self.value_to_bool(cond_val)?;

        // Create blocks
        let then_block = self.backend.context.append_basic_block(function, "then");
        let merge_block = self.backend.context.append_basic_block(function, "ifcont");

        // Determine next block (elif, else, or merge)
        let has_elifs = !if_expr.elifs.is_empty();
        let has_else = if_expr.else_body.is_some();

        let else_block = if has_elifs || has_else {
            self.backend.context.append_basic_block(function, "else")
        } else {
            merge_block
        };

        self.backend.builder.build_conditional_branch(cond_bool, then_block, else_block)
            .map_err(|e| e.to_string())?;

        // Compile then branch (body is Vec<Statement>)
        self.backend.builder.position_at_end(then_block);
        // Enter scope for then branch
        self.memory_ctx.enter_scope();
        for stmt in &if_expr.body {
            self.compile_statement(stmt)?;
        }
        // Exit scope for then branch
        self.memory_ctx.exit_scope();
        // Check terminator of current block (builder may have moved during compilation)
        if let Some(current_block) = self.backend.builder.get_insert_block() {
            if current_block.get_terminator().is_none() {
                self.backend.builder.build_unconditional_branch(merge_block)
                    .map_err(|e| e.to_string())?;
            }
        }

        // Compile elif branches
        if has_elifs {
            let mut current_else = else_block;
            for (i, elif) in if_expr.elifs.iter().enumerate() {
                self.backend.builder.position_at_end(current_else);

                // Check if current block already has a terminator
                if current_else.get_terminator().is_some() {
                    // Block already terminated, skip this elif
                    break;
                }

                let elif_cond = self.compile_expr(&elif.condition)?;
                let elif_bool = self.value_to_bool(elif_cond)?;

                let elif_then = self.backend.context.append_basic_block(function, &format!("elif_then_{}", i));
                let next_block = if i + 1 < if_expr.elifs.len() || has_else {
                    self.backend.context.append_basic_block(function, &format!("elif_else_{}", i))
                } else {
                    merge_block
                };

                self.backend.builder.build_conditional_branch(elif_bool, elif_then, next_block)
                    .map_err(|e| e.to_string())?;

                // Compile elif body
                self.backend.builder.position_at_end(elif_then);
                // Enter scope for elif branch
                self.memory_ctx.enter_scope();
                for stmt in &elif.body {
                    self.compile_statement(stmt)?;
                }
                // Exit scope for elif branch
                self.memory_ctx.exit_scope();
                // Check terminator of current block (builder may have moved)
                if let Some(current_block) = self.backend.builder.get_insert_block() {
                    if current_block.get_terminator().is_none() {
                        self.backend.builder.build_unconditional_branch(merge_block)
                            .map_err(|e| e.to_string())?;
                    }
                }

                current_else = next_block;
            }

            // Compile else branch if exists
            if has_else {
                self.backend.builder.position_at_end(current_else);
                // Only compile if block doesn't have terminator
                if current_else.get_terminator().is_none() {
                    // Enter scope for else branch
                    self.memory_ctx.enter_scope();
                    if let Some(ref else_body) = if_expr.else_body {
                        for stmt in else_body {
                            self.compile_statement(stmt)?;
                        }
                    }
                    // Exit scope for else branch
                    self.memory_ctx.exit_scope();
                    // Check terminator of current block (builder may have moved)
                    if let Some(current_block) = self.backend.builder.get_insert_block() {
                        if current_block.get_terminator().is_none() {
                            self.backend.builder.build_unconditional_branch(merge_block)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
        } else if has_else {
            // No elifs, just else
            self.backend.builder.position_at_end(else_block);
            // Enter scope for else branch
            self.memory_ctx.enter_scope();
            if let Some(ref else_body) = if_expr.else_body {
                for stmt in else_body {
                    self.compile_statement(stmt)?;
                }
            }
            // Exit scope for else branch
            self.memory_ctx.exit_scope();
            // Check terminator of current block (builder may have moved)
            if let Some(current_block) = self.backend.builder.get_insert_block() {
                if current_block.get_terminator().is_none() {
                    self.backend.builder.build_unconditional_branch(merge_block)
                        .map_err(|e| e.to_string())?;
                }
            }
        }

        // Continue at merge block
        self.backend.builder.position_at_end(merge_block);
        Ok(())
    }

    /// Convert a value to a boolean condition
    /// 
    /// This internal method converts a given LLVM value to a boolean condition
    /// suitable for use in control flow operations like if statements and loops.
    /// It handles the conversion of different types to a boolean representation
    /// according to Coffee's type system rules.
    /// 
    /// # Arguments
    /// 
    /// * `value` - The LLVM value to convert to a boolean
    /// 
    /// # Returns
    /// 
    /// * `Ok(IntValue)` - The boolean representation as an LLVM i1 integer value
    /// * `Err(String)` - If there was an error during the conversion
    fn value_to_bool(&self, value: BasicValueEnum<'ctx>) -> Result<inkwell::values::IntValue<'ctx>, String> {
        crate::backend::control_flow::value_to_bool(value, &self.backend.builder)
    }

    /// Compile while loop
    /// 
    /// This method generates LLVM IR for a while loop, including proper control
    /// flow structure, loop condition evaluation, and loop body execution. The
    /// method includes several safety features:
    /// 
    /// - Loop nesting depth tracking to prevent stack overflow
    /// - Loop iteration counter to prevent infinite loops
    /// - Proper break/continue support through loop context management
    /// - Security checks to prevent resource exhaustion
    /// 
    /// The generated code follows the standard while loop pattern with separate
    /// blocks for condition checking, loop body execution, and post-loop continuation.
    /// 
    /// # Arguments
    /// 
    /// * `while_loop` - The parsed WhileLoop to compile
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the while loop was compiled successfully
    /// * `Err(String)` - If there was an error during compilation
    pub fn compile_while(&mut self, while_loop: &crate::parser::WhileLoop) -> Result<(), String> {
        // MEDIUM-3 FIX: Check loop nesting depth to prevent stack overflow
        const MAX_LOOP_NESTING: usize = 32;
        if self.loop_nesting_depth >= MAX_LOOP_NESTING {
            return Err(self.error("compile_while",
                format!("Loop nesting depth exceeds maximum\n  = note: current nesting depth: {}, maximum: {}\n  = help: reduce loop nesting to prevent stack overflow",
                    self.loop_nesting_depth, MAX_LOOP_NESTING)));
        }

        let function = self.current_function
            .ok_or_else(|| self.error("compile_while", "while loop statement outside function context"))?;

        // CRITICAL-7 FIX: Add loop iteration counter to prevent infinite loops
        const MAX_LOOP_ITERATIONS: u64 = 1_000_000; // 1 million iterations max

        let loop_cond_block = self.backend.context.append_basic_block(function, "loopcond");
        let loop_body_block = self.backend.context.append_basic_block(function, "loopbody");
        let after_block = self.backend.context.append_basic_block(function, "afterloop");
        let overflow_block = self.backend.context.append_basic_block(function, "loop_overflow");

        // Allocate loop counter in entry block (must dominate all uses)
        let i64_type = self.backend.context.i64_type();
        let counter_ptr = self.create_entry_alloca_preserving_terminator(i64_type, "loop_counter")
            .map_err(|e| self.error("compile_while",
                format!("failed to allocate loop counter: {}", e)))?;

        // Initialize counter to 0 before loop starts
        self.backend.builder.build_store(counter_ptr, i64_type.const_int(0, false))
            .map_err(|e| self.error("compile_while",
                format!("failed to initialize loop counter: {}", e)))?;

        // Push loop context for break/continue
        self.loop_stack.push(LoopContext {
            break_block: after_block,
            continue_block: loop_cond_block,
        });

        // MEDIUM-3 FIX: Increment loop nesting depth
        self.loop_nesting_depth += 1;
        self.memory_ctx.loop_nesting_depth = self.loop_nesting_depth;

        // Jump to loop condition
        self.backend.builder.build_unconditional_branch(loop_cond_block)
            .map_err(|e| self.error("compile_while", format!("failed to build branch to loop: {}", e)))?;

        // Loop condition block: check counter overflow AND user condition
        self.backend.builder.position_at_end(loop_cond_block);

        // Load and increment counter FIRST (before checking condition)
        let counter = self.backend.builder.build_load(i64_type, counter_ptr, "counter")
            .map_err(|e| self.error("compile_while",
                format!("failed to load loop counter: {}", e)))?
            .into_int_value();

        let new_counter = self.backend.builder.build_int_add(counter, i64_type.const_int(1, false), "new_counter")
            .map_err(|e| self.error("compile_while",
                format!("failed to increment loop counter: {}", e)))?;

        self.backend.builder.build_store(counter_ptr, new_counter)
            .map_err(|e| self.error("compile_while",
                format!("failed to store loop counter: {}", e)))?;

        // Check if counter exceeds maximum
        let is_overflow = self.backend.builder.build_int_compare(
            IntPredicate::UGT,
            new_counter,
            i64_type.const_int(MAX_LOOP_ITERATIONS, false),
            "loop_overflow"
        ).map_err(|e| self.error("compile_while",
            format!("failed to build overflow check: {}", e)))?;

        // Branch: if overflow -> panic, else -> check user condition
        let cond_check_block = self.backend.context.append_basic_block(function, "condcheck");
        self.backend.builder.build_conditional_branch(is_overflow, overflow_block, cond_check_block)
            .map_err(|e| self.error("compile_while",
                format!("failed to build overflow branch: {}", e)))?;

        // Overflow block: panic
        self.backend.builder.position_at_end(overflow_block);
        let panic_msg = self.backend.builder.build_global_string_ptr(
            "Loop iteration limit exceeded - possible infinite loop",
            "loop_panic_msg"
        ).map_err(|e| self.error("compile_while",
            format!("failed to build panic message: {}", e)))?;

        let puts_func = self.functions.get("puts").copied().unwrap();
        self.backend.builder.build_call(puts_func, &[panic_msg.as_pointer_value().into()], "puts_call")
            .map_err(|e| self.error("compile_while",
                format!("failed to build puts call: {}", e)))?;

        let exit_func = self.functions.get("exit").copied().unwrap();
        let exit_code = self.backend.context.i32_type().const_int(1, false);
        self.backend.builder.build_call(exit_func, &[exit_code.into()], "exit_call")
            .map_err(|e| self.error("compile_while",
                format!("failed to build exit call: {}", e)))?;

        self.backend.builder.build_unreachable()
            .map_err(|e| self.error("compile_while",
                format!("failed to build unreachable: {}", e)))?;

        // User condition check block
        self.backend.builder.position_at_end(cond_check_block);
        let cond_val = self.compile_expr(&while_loop.condition)
            .map_err(|e| self.error("compile_while", format!("failed to compile loop condition: {}", e)))?;
        let cond_bool = self.value_to_bool(cond_val)
            .map_err(|e| self.error("compile_while", format!("failed to convert condition to boolean: {}", e)))?;
        self.backend.builder.build_conditional_branch(cond_bool, loop_body_block, after_block)
            .map_err(|e| self.error("compile_while", format!("failed to build conditional branch: {}", e)))?;

        // Loop body (now Vec<Statement> instead of String)
        self.backend.builder.position_at_end(loop_body_block);
        
        // Enter loop body scope for each iteration
        self.memory_ctx.enter_scope();
        
        for stmt in &while_loop.body {
            self.compile_statement(stmt)?;
        }
        
        // Exit loop body scope after each iteration
        self.memory_ctx.exit_scope();
        
        // Check current block for terminator (not loop_body_block!)
        // After compiling statements with arithmetic, builder may be in a different block
        if let Some(current_block) = self.backend.builder.get_insert_block() {
            if current_block.get_terminator().is_none() {
                self.backend.builder.build_unconditional_branch(loop_cond_block)
                    .map_err(|e| self.error("compile_while", format!("failed to build loop back-edge: {}", e)))?;
            }
        }

        // Pop loop context
        self.loop_stack.pop();

        // MEDIUM-3 FIX: Decrement loop nesting depth
        self.loop_nesting_depth -= 1;
        self.memory_ctx.loop_nesting_depth = self.loop_nesting_depth;

        self.backend.builder.position_at_end(after_block);
        Ok(())
    }

    /// Compile for loop
    /// 
    /// This method generates LLVM IR for a for loop, supporting both range-based
    /// iteration (e.g., `for i in 0..10`) and collection-based iteration (e.g.,
    /// `for item in array`). The method handles proper loop variable initialization,
    /// condition checking, and incrementation, while including safety features:
    /// 
    /// - Loop nesting depth tracking to prevent stack overflow
    /// - Proper break/continue support through loop context management
    /// - Type checking to ensure proper integer bounds
    /// - Collection length validation to ensure safe iteration
    /// 
    /// The generated code follows the standard for loop pattern with separate
    /// blocks for condition checking, loop body execution, incrementation,
    /// and post-loop continuation.
    /// 
    /// # Arguments
    /// 
    /// * `for_loop` - The parsed ForLoop to compile
    /// 
    /// # Returns
    /// 
    /// * `Ok(())` - If the for loop was compiled successfully
    /// * `Err(String)` - If there was an error during compilation
    pub fn compile_for(&mut self, for_loop: &crate::parser::ForLoop) -> Result<(), String> {
        // MEDIUM-3 FIX: Check loop nesting depth to prevent stack overflow
        const MAX_LOOP_NESTING: usize = 32;
        if self.loop_nesting_depth >= MAX_LOOP_NESTING {
            return Err(self.error("compile_for",
                format!("Loop nesting depth exceeds maximum\n  = note: current nesting depth: {}, maximum: {}\n  = help: reduce loop nesting to prevent stack overflow",
                    self.loop_nesting_depth, MAX_LOOP_NESTING)));
        }

        let function = self.current_function
            .ok_or_else(|| self.error("compile_for", "for loop statement outside function context"))?;

        // Allocate loop variable
        let i64_type = self.backend.context.i64_type();
        let var_alloca = self.backend.builder.build_alloca(i64_type, &for_loop.variable)
            .map_err(|e| self.error("compile_for",
                format!("failed to allocate loop variable '{}': {}", for_loop.variable, e)))?;

        // Initialize based on iterator type
        let (start_val, end_val) = match &for_loop.iterator {
            crate::parser::ForIterator::Range { start, end } => {
                let start_v = self.compile_expr(start)
                    .map_err(|e| self.error("compile_for",
                        format!("failed to compile range start: {}", e)))?;
                let end_v = self.compile_expr(end)
                    .map_err(|e| self.error("compile_for",
                        format!("failed to compile range end: {}", e)))?;

                let start_int = match start_v {
                    BasicValueEnum::IntValue(i) => i,
                    _ => return Err(self.error("compile_for",
                        format!("range start must be an integer, got non-integer value\n  = note: for loop ranges require integer bounds"))),
                };
                let end_int = match end_v {
                    BasicValueEnum::IntValue(i) => i,
                    _ => return Err(self.error("compile_for",
                        format!("range end must be an integer, got non-integer value\n  = note: for loop ranges require integer bounds"))),
                };
                (start_int, end_int)
            }
            crate::parser::ForIterator::Collection(coll) => {
                let crate::parser::expr::Expression::Variable(name) = coll else {
                    if let Err(e) = self.compile_expr(coll) {
                        return Err(self.error("compile_for", format!(
                            "failed to compile for-in collection expression: {}\n  = note: for-in over arbitrary expressions is not yet supported\n  = help: use a simple collection name (`for x in items`) or an explicit range (`for i in 0..n`)",
                            e
                        )));
                    }
                    return Err(self.error("compile_for",
                        format!("for-in over arbitrary expressions is not yet supported\n  = note: collection is `{}`\n  = help: bind the collection to a variable first, or use an explicit range (`for i in 0..n`)", coll)));
                };

                let start_int = i64_type.const_int(0, false);

                let end_int = if let Some(&len_ptr) = self.array_lengths.get(name) {
                    self.backend.builder.build_load(i64_type, len_ptr, "arr_len")
                        .map_err(|e| self.error("compile_for",
                            format!("failed to load length of collection '{}': {}", name, e)))?
                        .into_int_value()
                } else if let Some(&(_var_ptr, _)) = self.variables.get(name) {
                    self.used_variables.insert(name.to_string());
                    return Err(self.error("compile_for",
                        format!("cannot iterate over collection '{}' - length information not available\n  = note: collection length must be tracked when the collection is created\n  = help: use explicit range instead: for i in 0..length", name)));
                } else {
                    return Err(self.error("compile_for",
                        format!("cannot find collection '{}' in this scope\n  = note: for-in loops require an existing collection or range", name)));
                };
                (start_int, end_int)
            }
        };

        self.backend.builder.build_store(var_alloca, start_val)
            .map_err(|e| self.error("compile_for",
                format!("failed to initialize loop variable '{}': {}", for_loop.variable, e)))?;
        self.variables.insert(for_loop.variable.clone(), (var_alloca, i64_type.into()));

        let loop_block = self.backend.context.append_basic_block(function, "forloop");
        let body_block = self.backend.context.append_basic_block(function, "forbody");
        let incr_block = self.backend.context.append_basic_block(function, "forincr");
        let after_block = self.backend.context.append_basic_block(function, "afterfor");

        // Push loop context (continue goes to increment block)
        self.loop_stack.push(LoopContext {
            break_block: after_block,
            continue_block: incr_block,
        });

        // MEDIUM-3 FIX: Increment loop nesting depth
        self.loop_nesting_depth += 1;

        self.backend.builder.build_unconditional_branch(loop_block)
            .map_err(|e| self.error("compile_for", format!("failed to build branch to loop: {}", e)))?;

        // Loop condition
        self.backend.builder.position_at_end(loop_block);
        let current = self.backend.builder.build_load(i64_type, var_alloca, "current")
            .map_err(|e| self.error("compile_for",
                format!("failed to load loop variable '{}': {}", for_loop.variable, e)))?;
        let cond = self.backend.builder.build_int_compare(
            IntPredicate::SLT,
            current.into_int_value(),
            end_val,
            "forcond"
        ).map_err(|e| self.error("compile_for", format!("failed to build loop condition: {}", e)))?;
        self.backend.builder.build_conditional_branch(cond, body_block, after_block)
            .map_err(|e| self.error("compile_for", format!("failed to build conditional branch: {}", e)))?;

        // Body (body is now Vec<Statement>)
        self.backend.builder.position_at_end(body_block);
        for stmt in &for_loop.body {
            // Check if this is a continue or break statement
            match stmt {
                crate::parser::Statement::Continue(_) => {
                    self.compile_statement(stmt)?;
                    // After continue, don't compile any more statements in this iteration
                    break;
                }
                crate::parser::Statement::Break(_) => {
                    self.compile_statement(stmt)?;
                    // After break, don't compile any more statements in this iteration
                    break;
                }
                _ => {
                    self.compile_statement(stmt)?;
                }
            }

            // Check if the current block now has a terminator (e.g., from continue/break inside if statement)
            if let Some(current_block) = self.backend.builder.get_insert_block() {
                if current_block.get_terminator().is_some() {
                    // Stop compiling more statements after a terminator
                    break;
                }
            }
        }

        // Check current block for terminator (not body_block!)
        // After compiling statements with arithmetic, builder may be in a different block
        if let Some(current_block) = self.backend.builder.get_insert_block() {
            if current_block.get_terminator().is_none() {
                self.backend.builder.build_unconditional_branch(incr_block)
                    .map_err(|e| self.error("compile_for", format!("failed to build branch to increment: {}", e)))?;
            }
        }

        // Increment block
        self.backend.builder.position_at_end(incr_block);
        let current = self.backend.builder.build_load(i64_type, var_alloca, "current")
            .map_err(|e| self.error("compile_for",
                format!("failed to load loop variable '{}' for increment: {}", for_loop.variable, e)))?;
        let next = self.backend.builder.build_int_add(
            current.into_int_value(),
            i64_type.const_int(1, false),
            "next"
        ).map_err(|e| self.error("compile_for", format!("failed to build increment operation: {}", e)))?;
        self.backend.builder.build_store(var_alloca, next)
            .map_err(|e| self.error("compile_for",
                format!("failed to store incremented loop variable '{}': {}", for_loop.variable, e)))?;
        self.backend.builder.build_unconditional_branch(loop_block)
            .map_err(|e| self.error("compile_for", format!("failed to build loop back-edge: {}", e)))?;

        // Pop loop context
        self.loop_stack.pop();

        // MEDIUM-3 FIX: Decrement loop nesting depth
        self.loop_nesting_depth -= 1;
        self.memory_ctx.loop_nesting_depth = self.loop_nesting_depth;

        self.backend.builder.position_at_end(after_block);
        Ok(())
    }
}
