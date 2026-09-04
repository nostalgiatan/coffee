//! Statement compilation for Coffee compiler
//!
//! Handles compilation of various statement types including control flow,
//! memory operations, and return statements.

use crate::coffee_debug;
use super::codegen::CodeGenerator;
use crate::parser::expr::Expression;
use crate::parser::{Statement, MemoryOp};

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Compile a statement
    pub fn compile_statement(&mut self, stmt: &Statement) -> Result<(), String> {
        // Update current line for lifetime tracking (simple counter approach)
        self.memory_ctx.set_line(self.memory_ctx.current_line + 1);

        coffee_debug!("DEBUG: compile_statement: stmt={:?}", stmt);

        match stmt {
            Statement::Function(func) => {
                self.compile_function(func)
            }
            Statement::Main(main_entry) => self.compile_main_entry(main_entry),
            Statement::VariableDecl(var) => {
                // Check if we're inside a function
                if self.current_function.is_some() {
                    // Local variable - use alloca
                    self.compile_local_variable_decl(var)
                } else {
                    // Global variable
                    self.compile_variable_decl(var)
                }
            }
            Statement::Assignment(var_name, value_expr) => {
                self.compile_assignment_from_ast(var_name, value_expr)
            }
            Statement::Class(class) => {
                // Skip class compilation here - classes are already compiled in the first pass
                // in compile_program_with_imports to ensure struct types are available
                Ok(())
            }
            Statement::Enum(enum_def) => self.compile_enum(enum_def),
            Statement::Return(ret) => self.compile_return(ret.value.as_ref()),
            Statement::Import(_) => Ok(()), // Handled at frontend
            Statement::SingleLineComment(_) | Statement::MultiLineComment(_) => Ok(()),
            Statement::If(if_expr) => self.compile_if(if_expr),
            Statement::While(while_loop) => self.compile_while(while_loop),
            Statement::For(for_loop) => self.compile_for(for_loop),
            Statement::Match(match_expr) => self.compile_match(match_expr),
            Statement::Break(_) => self.compile_break(),
            Statement::Continue(_) => self.compile_continue(),
            Statement::MemoryOp(op) => self.compile_memory_op(op),
            Statement::Raise(raise_stmt) => self.compile_raise(raise_stmt),
            Statement::Expr(expr) => {
                self.compile_expr(expr)?;
                Ok(())
            }
        }
    }

    /// Compile a line of function body
    #[cfg(test)]
    pub fn compile_body_line(&mut self, line: &str) -> Result<(), String> {
        // Strip inline comments first
        let line = self.strip_inline_comments(line);
        let line = line.trim();

        if line.is_empty() || line.starts_with("/#") {
            return Ok(());
        }

        // Handle different statement types
        if line == "break" {
            self.compile_break()?;
        } else if line == "continue" {
            self.compile_continue()?;
        } else if line.starts_with("return ") {
            coffee_debug!("DEBUG: compile_body_line: return statement: {}", line);
            let expr_str = &line[7..];
            let value = self.compile_source_as_expr(expr_str)
                .map_err(|e| self.error("return_statement", format!("failed to compile return expression '{}': {}", expr_str, e)))?;
            self.backend.builder.build_return(Some(&value))
                .map_err(|e| self.error("return_statement", format!("failed to build return instruction: {}", e)))?;
        } else if line == "return" {
            coffee_debug!("DEBUG: compile_body_line: return statement (void)");
            self.backend.builder.build_return(None)
                .map_err(|e| self.error("return_statement", format!("failed to build void return: {}", e)))?;
        } else if line.starts_with("let ") {
            coffee_debug!("DEBUG: compile_body_line: let statement: {}", line);
            self.compile_let_statement(line)?;
        } else if line.contains(" = ") && !line.starts_with("if ") && !line.starts_with("for ") && !line.starts_with("while ") {
            self.compile_assignment(line)?;
        } else if line.contains("(") {
            // Function call (discard result)
            self.compile_source_as_expr(line)?;
        } else {
            // Note: This is not an error - it might be a comment or empty line after trimming
            // Control flow statements should be in the AST, not in string bodies
        }

        Ok(())
    }

    /// Compile return statement
    fn compile_return(&mut self, expr: Option<&Expression>) -> Result<(), String> {
        if let Some(expr) = expr {
            if let Expression::Variable(var_name) = expr {
                self.memory_ctx.record_use(var_name);
                if let Some(info) = self.memory_ctx.lifetimes.get_mut(var_name) {
                    info.properly_cleaned = true;
                }
            }

            let value = self.compile_expr(expr)?;

            // Convert return value to match function return type
            if let Some(current_fn) = self.current_function {
                let return_type = current_fn.get_type().get_return_type();
                if let Some(ret_type) = return_type {
                    let value_type = value.get_type();

                    // Check if types are compatible before conversion
                    if !self.are_types_compatible(value_type, ret_type) {
                        let value_type_str = self.type_to_string(value_type);
                        let ret_type_str = self.type_to_string(ret_type);
                        let fn_name = current_fn.get_name().to_str().unwrap_or("unknown");

                        return Err(self.error("return_statement",
                            format!("type mismatch in return statement of function '{}'\n  = note: expected return type '{}', found type '{}'\n  = note: these types are incompatible and cannot be implicitly converted\n  = help: ensure the return expression matches the function's declared return type",
                                fn_name, ret_type_str, value_type_str)));
                    }

                    let converted_value = self.convert_value_to_type(value, ret_type, "return_val")?;
                    self.backend.builder.build_return(Some(&converted_value))
                        .map_err(|e| e.to_string())?;
                } else {
                    self.backend.builder.build_return(Some(&value))
                        .map_err(|e| e.to_string())?;
                }
            } else {
                self.backend.builder.build_return(Some(&value))
                    .map_err(|e| e.to_string())?;
            }
        } else {
            self.backend.builder.build_return(None)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Compile raise statement - expands directly to C library calls
    /// `raise Error(...)` is compiled at compile-time, NOT using runtime functions
    /// CRITICAL: raise behaves like return - it's a terminating statement
    /// All variables MUST be cleaned up before panic to prevent memory leaks
    pub fn compile_raise(&mut self, raise_stmt: &crate::parser::RaiseStmt) -> Result<(), String> {
        // Mark that Error class is needed
        // This will trigger automatic generation of Error class definition
        self.needs_error_class = true;

        let parsed = raise_stmt.error_expr.clone();

        let (error_type, args) = Self::raise_ctor_parts(parsed);
        for arg in &args {
            let _ = self.compile_expr(arg)?;
        }

        // Format: "[E{code}]: e \n note:..." — literals for the static fprintf string;
        // non-literals still compile above, then panic via fprintf+exit (no try/catch).
        let error_msg = if args.is_empty() {
            format!("[E-1]: {}", error_type)
        } else {
            let mut error_code: i64 = -1;
            let mut error_extra = String::new();
            let mut error_note = String::new();

            if let Some(code) = Self::raise_int_literal(&args[0]) {
                error_code = code;
            }
            if args.len() >= 2 {
                error_note = Self::raise_display_text(&args[1]);
            }
            if args.len() >= 3 {
                error_extra = Self::raise_display_text(&args[2]);
            }

            if error_extra.is_empty() && error_note.is_empty() {
                format!("[E{}]: {}", error_code, error_type)
            } else if error_note.is_empty() {
                format!("[E{}]: {}", error_code, error_extra)
            } else if error_extra.is_empty() {
                format!("[E{}]: {}\nnote: {}", error_code, error_type, error_note)
            } else {
                format!("[E{}]: {}\nnote: {}", error_code, error_extra, error_note)
            }
        };

        // Get LLVM types
        let i8_ptr_type = self.backend.context.ptr_type(inkwell::AddressSpace::default());
        let i32_type = self.backend.context.i32_type();

        // Get fprintf and exit functions (using built-in libc signatures)
        let fprintf_func = *self.functions.get("fprintf")
            .ok_or("fprintf function not declared - use 'fprintf in libc of c'")?;

        let exit_func = *self.functions.get("exit")
            .ok_or("exit function not declared - use 'exit in libc of c'")?;

        // Create error message string constant
        let message_ptr = self.backend.builder.build_global_string_ptr(
            &error_msg,
            "error_msg"
        ).map_err(|e| self.error("compile_raise", format!("failed to build error message string: {}", e)))?
        .as_pointer_value();

        // Create format string constant
        let format_ptr = self.backend.builder.build_global_string_ptr(
            "%s\n",
            "format_str"
        ).map_err(|e| self.error("compile_raise", format!("failed to build format string: {}", e)))?
        .as_pointer_value();

        // Get or declare stderr global variable (FILE* stderr from libc)
        // stderr is a ptr to FILE*, so we need to load it first
        let stderr_global = match self.backend.module.get_global("stderr") {
            Some(g) => g.as_pointer_value(),
            None => {
                // Declare stderr as external global variable (ptr to FILE*)
                let stderr_type = i8_ptr_type;
                let stderr_global = self.backend.module.add_global(stderr_type, None, "stderr");
                stderr_global.set_linkage(inkwell::module::Linkage::External);
                stderr_global.as_pointer_value()
            }
        };

        // Load the actual FILE* pointer from stderr
        let stderr_file_ptr = self.backend.builder.build_load(
            i8_ptr_type,
            stderr_global,
            "stderr_file_ptr"
        ).map_err(|e| self.error("compile_raise", format!("failed to load stderr: {}", e)))?
        .into_pointer_value();

        // Call fprintf(stderr, "%s\n", msg) to output error message to stderr
        let _ = self.backend.builder.build_call(
            fprintf_func,
            &[stderr_file_ptr.into(), format_ptr.into(), message_ptr.into()],
            "fprintf_call"
        );

        // CRITICAL: Auto-cleanup all variables before raising error
        // This prevents memory leaks on the error path
        // IMPORTANT: Do NOT clean up function parameters, only local variables
        let vars_to_clean: Vec<String> = self.variables.keys()
            .filter(|name| !self.current_function_params.contains(name))
            .cloned()
            .collect();

        // Generate clean out for all local variables (not parameters)
        for var_name in vars_to_clean {
            // Only clean if variable exists and hasn't been moved/dropped
            if let Some(info) = self.memory_ctx.lifetimes.get(&var_name) {
                if matches!(info.state, crate::backend::memory_ops::VariableState::Initialized) {
                    use crate::parser::MemoryOp;
                    let clean_op = MemoryOp::Remove {
                        target: var_name.clone(),
                    };

                    // Emit cleanup code
                    self.compile_memory_op(&clean_op)?;
                }
            }
        }

        // Call exit(1) to terminate
        let exit_code = i32_type.const_int(1, false);
        let _ = self.backend.builder.build_call(
            exit_func,
            &[exit_code.into()],
            "exit_call"
        );

        // Build unreachable to indicate code after raise is unreachable
        let _ = self.backend.builder.build_unreachable();

        Ok(())
    }

    fn raise_ctor_parts(expr: Expression) -> (String, Vec<Expression>) {
        match expr {
            Expression::Call { function, args } => {
                let name = match function.as_ref() {
                    Expression::Variable(n) | Expression::Literal(n) => n.clone(),
                    _ => "Error".to_string(),
                };
                (name, args)
            }
            Expression::ConstructorCall { class_name, args } => (class_name, args),
            Expression::TypeCast { target_type, value } => (target_type, vec![*value]),
            Expression::Variable(n) | Expression::Literal(n) => {
                (if n.is_empty() { "Error".to_string() } else { n }, Vec::new())
            }
            _ => ("Error".to_string(), Vec::new()),
        }
    }

    fn raise_int_literal(expr: &Expression) -> Option<i64> {
        match expr {
            Expression::Literal(s) => {
                let t = s.trim();
                t.parse::<i64>().ok().or_else(|| t.parse::<i32>().ok().map(|n| n as i64))
            }
            Expression::Unary { op, operand } if op == "-" => {
                Self::raise_int_literal(operand).map(|n| -n)
            }
            _ => None,
        }
    }

    fn raise_display_text(expr: &Expression) -> String {
        match expr {
            Expression::Literal(s) => {
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
            Expression::Variable(n) => n.clone(),
            _ => String::new(),
        }
    }

    /// Compile memory operation
    pub fn compile_memory_op(&mut self, op: &MemoryOp) -> Result<(), String> {
        use super::memory_ops;
        memory_ops::compile_memory_op(
            op,
            self.backend.context,
            &self.backend.builder,
            &mut self.variables,
            &mut self.memory_ctx,
            &self.functions,
        )
    }

    /// Compile break statement
    pub fn compile_break(&mut self) -> Result<(), String> {
        use super::control_flow;
        control_flow::compile_break(&self.backend.builder, &self.loop_stack)
            .map_err(|e| self.error("compile_break", e))
    }

    /// Compile continue statement
    pub fn compile_continue(&mut self) -> Result<(), String> {
        use super::control_flow;
        control_flow::compile_continue(&self.backend.builder, &self.loop_stack)
            .map_err(|e| self.error("compile_continue", e))
    }

    /// Strip inline comments from a string
    /// Coffee comments: /#/ ... /#/ (with closing) or /#/ ... (to end of line)
    pub fn strip_inline_comments(&self, s: &str) -> String {
        coffee_debug!("DEBUG: strip_inline_comments: s='{}', len={}", s, s.len());
        
        // Find comment start: /#/
        if let Some(start_pos) = s.find("/#/") {
            // Check if it's inside a string literal
            let before = &s[..start_pos];
            let quote_count = before.matches('"').count() + before.matches('\'').count();
            // Odd number of quotes means we're inside a string
            if quote_count % 2 == 0 {
                // Not inside a string - this is a real comment
                // Look for closing /#/ after the opening
                let after_open = &s[start_pos + 3..];
                if let Some(end_pos) = after_open.find("/#/") {
                    // Found closing marker - remove comment and keep rest of line
                    let after_close = &after_open[end_pos + 3..];
                    let result = format!("{}{}", before, after_close).trim().to_string();
                    coffee_debug!("DEBUG: strip_inline_comments: found comment, result='{}', len={}", result, result.len());
                    return result;
                } else {
                    // No closing marker - comment extends to end of line
                    let result = before.trim().to_string();
                    coffee_debug!("DEBUG: strip_inline_comments: found unclosed comment, result='{}', len={}", result, result.len());
                    return result;
                }
            }
        }
        let result = s.trim().to_string();
        coffee_debug!("DEBUG: strip_inline_comments: no comment, result='{}', len={}", result, result.len());
        result
    }

    /// Calculate edit distance between two strings (for suggestion)
    pub fn edit_distance(a: &str, b: &str) -> usize {
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();
        let a_len = a_chars.len();
        let b_len = b_chars.len();

        if a_len == 0 { return b_len; }
        if b_len == 0 { return a_len; }

        let mut prev_row: Vec<usize> = (0..=b_len).collect();
        let mut curr_row = vec![0; b_len + 1];

        for i in 1..=a_len {
            curr_row[0] = i;
            for j in 1..=b_len {
                let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
                curr_row[j] = (prev_row[j] + 1)
                    .min(curr_row[j - 1] + 1)
                    .min(prev_row[j - 1] + cost);
            }
            std::mem::swap(&mut prev_row, &mut curr_row);
        }

        prev_row[b_len]
    }
}
