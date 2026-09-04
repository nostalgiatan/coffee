//! Type Inference Module for Coffee Compiler
//!
//! This module provides type inference capabilities for literal values and expressions.
//! It handles the conversion of literal values to appropriate types based on context,
//! such as variable declarations, function parameters, and function calls.
//!
//! The module implements a simple but effective type inference system that:
//! - Infers literal types from context (e.g., variable type in assignment)
//! - Performs safe type conversions (e.g., i64 to i32 when safe)
//! - Validates type compatibility for function calls
//! - Provides type coercion for common scenarios

use inkwell::values::BasicValueEnum;
use inkwell::types::BasicTypeEnum;
use inkwell::context::Context;
use inkwell::builder::Builder;

/// Process escape sequences in a string literal
fn process_escape_sequences(s: &str) -> Vec<u8> {
    let mut result = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => result.push(b'\n'),
                    't' => result.push(b'\t'),
                    'r' => result.push(b'\r'),
                    '\\' => result.push(b'\\'),
                    '"' => result.push(b'"'),
                    '\'' => result.push(b'\''),
                    '0' => result.push(b'\0'),
                    'x' => {
                        // Hex escape: \xHH
                        let mut hex_str = String::new();
                        for _ in 0..2 {
                            if let Some(&h) = chars.peek() {
                                if h.is_ascii_hexdigit() {
                                    hex_str.push(chars.next().unwrap());
                                }
                            }
                        }
                        if let Ok(byte) = u8::from_str_radix(&hex_str, 16) {
                            result.push(byte);
                        }
                    }
                    _ => {
                        // Unknown escape, keep as-is
                        result.push(b'\\');
                        result.push(next as u8);
                    }
                }
            } else {
                result.push(b'\\');
            }
        } else {
            result.push(c as u8);
        }
    }

    result
}

/// Type inference context
/// 
/// Provides information about the expected type in a given context,
/// such as the target type in an assignment or the parameter type in a function call.
pub struct TypeInferenceContext<'ctx> {
    /// The expected type in this context (if any)
    pub expected_type: Option<BasicTypeEnum<'ctx>>,
    /// Whether to allow implicit type conversions
    pub allow_implicit_conversion: bool,
}

impl<'ctx> TypeInferenceContext<'ctx> {
    /// Create a new type inference context without expected type
    pub fn new() -> Self {
        Self {
            expected_type: None,
            allow_implicit_conversion: true,
        }
    }

    /// Create a type inference context with an expected type
    pub fn with_expected_type(expected_type: BasicTypeEnum<'ctx>) -> Self {
        Self {
            expected_type: Some(expected_type),
            allow_implicit_conversion: true,
        }
    }

    /// Disable implicit type conversions
    pub fn disable_implicit_conversion(mut self) -> Self {
        self.allow_implicit_conversion = false;
        self
    }
}

impl<'ctx> Default for TypeInferenceContext<'ctx> {
    fn default() -> Self {
        Self::new()
    }
}

/// Compile a literal value with type inference
/// 
/// This function compiles a literal value (integer, float, boolean, or string)
/// and performs type inference based on the provided context. If an expected type
/// is specified, the literal will be converted to match that type when possible.
pub fn compile_literal_with_inference<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    expr_str: &str,
    inference_ctx: &TypeInferenceContext<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    // Try to parse as float literal first (to handle 0.0, 1.0, etc.)
    // Check if it looks like a float (contains '.' or 'e'/'E')
    if expr_str.contains('.') || expr_str.contains('e') || expr_str.contains('E') {
        if let Ok(f) = expr_str.parse::<f64>() {
            return compile_float_literal(context, f, inference_ctx);
        }
    }

    // Try to parse as integer literal
    if let Ok(i) = expr_str.parse::<i64>() {
        return compile_integer_literal(context, builder, i, inference_ctx);
    }

    // Boolean literals
    if expr_str == "true" {
        return Ok(context.i8_type().const_int(1, false).into());
    }
    if expr_str == "false" {
        return Ok(context.i8_type().const_int(0, false).into());
    }

    // String literal
    if expr_str.starts_with('"') && expr_str.ends_with('"') {
        let content = &expr_str[1..expr_str.len()-1];
        // Process escape sequences in the string literal
        let bytes = process_escape_sequences(content);
        // Convert bytes back to string for build_global_string_ptr
        let processed_content = String::from_utf8_lossy(&bytes);
        let global = builder.build_global_string_ptr(&processed_content, "str")
            .map_err(|e| format!("failed to build string constant: {}", e))?;
        return Ok(global.as_pointer_value().into());
    }

    Err(format!("invalid literal value: '{}'", expr_str))
}

/// Compile an integer literal with type inference
fn compile_integer_literal<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    value: i64,
    inference_ctx: &TypeInferenceContext<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let default_value = context.i64_type().const_int(value as u64, true);
    let default_int_value = default_value;

    if !inference_ctx.allow_implicit_conversion {
        return Ok(default_int_value.into());
    }

    if let Some(expected_type) = &inference_ctx.expected_type {
        if let BasicTypeEnum::IntType(expected_int_type) = expected_type {
            let expected_bit_width = expected_int_type.get_bit_width();
            let current_bit_width = 64;

            if expected_bit_width == current_bit_width {
                return Ok(default_int_value.into());
            }

            if expected_bit_width < current_bit_width {
                let max_value_for_type = match expected_bit_width {
                    8 => i8::MAX as i64,
                    16 => i16::MAX as i64,
                    32 => i32::MAX as i64,
                    _ => i64::MAX,
                };

                if value >= 0 && value <= max_value_for_type {
                    let truncated = builder.build_int_truncate(
                        default_int_value,
                        *expected_int_type,
                        "literal_trunc"
                    ).map_err(|e| format!("failed to truncate integer literal: {}", e))?;
                    return Ok(truncated.into());
                }
            }
        }
    }

    Ok(default_int_value.into())
}

/// Compile a float literal with type inference
fn compile_float_literal<'ctx>(
    context: &'ctx Context,
    value: f64,
    inference_ctx: &TypeInferenceContext<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let default_value = context.f64_type().const_float(value);

    if inference_ctx.allow_implicit_conversion {
        if let Some(expected_type) = &inference_ctx.expected_type {
            if let BasicTypeEnum::FloatType(expected_float_type) = expected_type {
                let expected_bit_width = expected_float_type.get_bit_width();
                let current_bit_width = 64;

                if expected_bit_width < current_bit_width {
                    let f32_value = value as f32;
                    if (f32_value as f64 - value).abs() < f64::EPSILON * 100.0 {
                        return Ok(expected_float_type.const_float(f32_value.into()).into());
                    }
                }
            }
        }
    }

    Ok(default_value.into())
}
