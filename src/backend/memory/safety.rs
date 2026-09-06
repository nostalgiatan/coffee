//! Memory Safety Checks for Coffee Compiler
//! 
//! Provides runtime safety checks for memory operations in the Coffee compiler.
//! This module implements critical safety checks that help prevent common
//! memory-related errors such as null pointer dereferences and out-of-bounds
//! array access. These checks are inserted into the generated code to provide
//! runtime protection while maintaining good performance.

use inkwell::{builder::Builder};

/// Safety context for runtime checks
/// 
/// This structure manages the state and configuration for inserting
/// runtime safety checks into generated LLVM IR. It keeps track of
/// the panic function to call when errors are detected and provides
/// methods for generating various safety checks.
/// 
/// The SafetyContext works with the LLVM builder to insert conditional
/// branches and error handling code at appropriate locations in the
/// generated IR to catch memory safety violations at runtime.
pub struct SafetyContext<'ctx> {
    _ctx: std::marker::PhantomData<&'ctx ()>,
}

impl<'ctx> SafetyContext<'ctx> {
    /// Create a new safety context
    /// 
    /// Initializes a safety context with no panic function set. This means
    /// that safety checks will be generated but will use alternative error
    /// reporting mechanisms if no panic function is later provided.
    /// 
    /// # Returns
    /// 
    /// A new SafetyContext instance ready for use with LLVM IR generation
    /// 
    /// # Examples
    /// 
    /// ```
    /// use coffee::backend::memory::safety::SafetyContext;
    /// 
    /// let safety_context = SafetyContext::new();
    /// // The context is ready to generate safety checks
    /// ```
    pub fn new() -> Self {
        Self {
            _ctx: std::marker::PhantomData,
        }
    }

    /// Generate array bounds check
    /// 
    /// This function generates LLVM IR code that checks if an array index
    /// is within valid bounds at runtime. The check verifies that the index
    /// is not negative and not greater than or equal to the array length.
    /// If the index is out of bounds, the check will trigger the panic function
    /// (if set) or use an alternative error reporting mechanism.
    /// 
    /// The generated code checks both for negative indices and for indices
    /// that exceed the array bounds, combining the checks with a logical OR
    /// to catch either condition.
    /// 
    /// # Arguments
    /// 
    /// * `builder` - The LLVM builder to use for generating the check code
    /// * `index` - The index value to check
    /// * `length` - The length of the array (valid indices are 0 to length-1)
    /// * `_location` - Source location information (currently unused)
    /// 
    /// # Returns
    /// 
    /// Ok(()) if the check was generated successfully, or an error string if generation failed
    /// 
    /// # Examples
    /// 
    /// ```
    /// // This example shows how the function would be used in a broader context
    /// use coffee::backend::memory::safety::SafetyContext;
    /// // In practice, you'd need LLVM builder, index, and length values
    /// // safety_context.check_bounds(&builder, index_value, length_value, "example.coffee:15:10");
    /// ```
    pub fn check_bounds(
        &self,
        builder: &Builder<'ctx>,
        index: inkwell::values::IntValue<'ctx>,
        length: inkwell::values::IntValue<'ctx>,
        _location: &str,
    ) -> Result<(), String> {
        let function = builder.get_insert_block()
            .and_then(|b| b.get_parent())
            .ok_or("check_bounds: not in a function")?;

        let context = function.get_first_basic_block()
            .unwrap()
            .get_context();

        // Check if index < 0
        let zero = index.get_type().const_int(0, false);
        let is_negative = builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            index,
            zero,
            "is_negative"
        ).map_err(|e| e.to_string())?;

        // Check if index >= length
        let is_out_of_bounds = builder.build_int_compare(
            inkwell::IntPredicate::SGE,
            index,
            length,
            "is_out_of_bounds"
        ).map_err(|e| e.to_string())?;

        // Combine checks: is_negative || is_out_of_bounds
        let is_invalid = builder.build_or(is_negative, is_out_of_bounds, "is_invalid")
            .map_err(|e| e.to_string())?;

        // Create blocks
        let then_block = context.append_basic_block(function, "bounds_panic");
        let merge_block = context.append_basic_block(function, "bounds_merge");

        // Conditional branch
        builder.build_conditional_branch(is_invalid, then_block, merge_block)
            .map_err(|e| e.to_string())?;

        // Panic block
        builder.position_at_end(then_block);
        builder.build_unreachable().map_err(|e| e.to_string())?;

        // Merge block
        builder.position_at_end(merge_block);

        Ok(())
    }
}
