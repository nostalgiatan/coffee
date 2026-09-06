use super::CodeGenerator;

impl<'a, 'ctx> CodeGenerator<'a, 'ctx> {
    /// Enhanced error with context
    ///
    /// Creates a formatted error message with contextual information about the
    /// current compilation context. If there is a current function, the error
    /// message will include the function name for better debugging. The error
    /// follows the format "function_name: context: detail" when inside a function,
    /// or "context: detail" when not in a function context.
    ///
    /// # Arguments
    ///
    /// * `context` - A string describing the context where the error occurred
    /// * `detail` - An implementor of Display providing additional error details
    ///
    /// # Returns
    ///
    /// A formatted error message string with contextual information
    pub fn error(&self, context: &str, detail: impl std::fmt::Display) -> String {
        if let Some(func) = self.current_function {
            let func_name = func.get_name().to_str().unwrap_or("<unknown>");
            // 更清晰的错误格式：function_name: context: detail
            // 而不是扁平的链式结构
            format!("{}: {}: {}", func_name, context, detail)
        } else {
            format!("{}: {}", context, detail)
        }
    }
}
