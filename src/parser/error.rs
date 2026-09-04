// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License

//! Enhanced parser error diagnostics with intelligent error detection

use std::fmt;

/// Enhanced parser error with detailed context and suggestions
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// Function parameter is missing a name
    MissingParameterName {
        line: usize,
        function_name: String,
        hint: String,
    },

    /// Missing closing parenthesis
    MissingClosingParen {
        line: usize,
        context: String,
        opening_paren_pos: usize,
    },

    /// Missing return type annotation after `=>`
    MissingReturnType {
        line: usize,
        function_name: String,
    },

    /// Missing colon after function signature
    MissingColon {
        line: usize,
        function_name: String,
    },

    /// Missing type annotation for parameter
    MissingTypeAnnotation {
        line: usize,
        parameter_name: String,
    },

    /// Invalid function syntax
    InvalidFunctionSyntax {
        line: usize,
        context: String,
        expected: String,
    },

    /// Unknown keyword or identifier
    UnexpectedKeyword {
        line: usize,
        keyword: String,
        expected: Vec<String>,
    },

    /// Missing block body (function should have a body)
    MissingBlockBody {
        line: usize,
        statement_type: String,
    },

    /// Invalid type syntax
    InvalidTypeSyntax {
        line: usize,
        type_str: String,
        reason: String,
    },

    /// Generic syntax error with context
    GenericSyntaxError {
        line: usize,
        context: String,
        hint: String,
    },

    /// Wrong comment syntax (using // or # instead of /#/)
    WrongCommentSyntax {
        line: usize,
        wrong_syntax: String,
        correct_syntax: String,
    },

    /// Use of braces in an indentation-based language
    InvalidUseOfBraces {
        line: usize,
        context: String,
        brace_type: String,  // "{" or "}"
    },
}

impl ParseError {
    /// Detect specific error patterns from a failed parse line
    pub fn detect_error(line_num: usize, line: &str) -> Self {
        let trimmed = line.trim();

        // Check for braces FIRST - Coffee is indentation-based
        if trimmed.contains('{') || trimmed.contains('}') {
            let brace_type = if trimmed.contains('{') { "{" } else { "}" };
            return ParseError::InvalidUseOfBraces {
                line: line_num,
                context: trimmed.to_string(),
                brace_type: brace_type.to_string(),
            };
        }

        // Check for wrong comment syntax first (before other checks)
        // Coffee uses /#/ for single-line comments, not // or #
        if trimmed.starts_with("//") {
            return ParseError::WrongCommentSyntax {
                line: line_num,
                wrong_syntax: "//".to_string(),
                correct_syntax: "/#/".to_string(),
            };
        }

        if trimmed.starts_with("#") && !trimmed.starts_with("/#") {
            return ParseError::WrongCommentSyntax {
                line: line_num,
                wrong_syntax: "#".to_string(),
                correct_syntax: "/#/".to_string(),
            };
        }

        // Check for main() entry point errors
        if trimmed.starts_with("main(") {
            return Self::analyze_main_error(line_num, trimmed);
        }

        // Check for function definition errors
        if trimmed.starts_with("fn ") {
            return Self::analyze_function_error(line_num, trimmed);
        }

        // Incomplete / invalid block starters — prefer MissingBlockBody over a generic hint
        if trimmed.starts_with("if ")
            || trimmed == "if"
            || trimmed.starts_with("while ")
            || trimmed == "while"
            || trimmed.starts_with("for ")
            || trimmed == "for"
            || trimmed.starts_with("match ")
            || trimmed == "match"
            || trimmed.starts_with("enum ")
            || trimmed == "enum"
            || trimmed.starts_with("class ")
            || trimmed == "class"
            || trimmed.starts_with("packed class ")
        {
            let statement_type = if trimmed.starts_with("packed class ") || trimmed.starts_with("class ") || trimmed == "class" {
                "class"
            } else if trimmed.starts_with("while ") || trimmed == "while" {
                "while"
            } else if trimmed.starts_with("for ") || trimmed == "for" {
                "for"
            } else if trimmed.starts_with("match ") || trimmed == "match" {
                "match"
            } else if trimmed.starts_with("enum ") || trimmed == "enum" {
                "enum"
            } else {
                "if"
            };
            return ParseError::MissingBlockBody {
                line: line_num,
                statement_type: statement_type.to_string(),
            };
        }

        // Unknown keyword at the start of a failed statement
        if let Some(word) = trimmed.split(|c: char| c.is_whitespace() || c == '(').next() {
            const KNOWN: &[&str] = &[
                "fn", "c", "if", "elif", "else", "while", "for", "match", "enum", "class",
                "packed", "let", "return", "raise", "break", "continue", "use", "main",
                "mv", "copy", "clone", "rm", "clean", "true", "false",
            ];
            if !word.is_empty()
                && word.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                && !KNOWN.contains(&word)
            {
                return ParseError::UnexpectedKeyword {
                    line: line_num,
                    keyword: word.to_string(),
                    expected: KNOWN.iter().map(|s| (*s).to_string()).collect(),
                };
            }
        }

        // Check for missing colon patterns
        if trimmed.contains("=>") && !trimmed.contains(':') {
            // Found arrow but no colon
            if let Some(fn_name) = Self::extract_function_name(trimmed) {
                return ParseError::MissingColon {
                    line: line_num,
                    function_name: fn_name,
                };
            }
        }

        // Check for missing return type
        if trimmed.ends_with("=>") {
            if let Some(fn_name) = Self::extract_function_name(trimmed) {
                return ParseError::MissingReturnType {
                    line: line_num,
                    function_name: fn_name,
                };
            }
        }

        // Check for unclosed parentheses
        let open_count = trimmed.matches('(').count();
        let close_count = trimmed.matches(')').count();
        if open_count > close_count {
            return ParseError::MissingClosingParen {
                line: line_num,
                context: trimmed.to_string(),
                opening_paren_pos: trimmed.find('(').unwrap_or(0),
            };
        }

        // Generic syntax error with helpful hint
        ParseError::GenericSyntaxError {
            line: line_num,
            context: trimmed.to_string(),
            hint: Self::generate_hint(trimmed),
        }
    }

    /// Analyze function-specific errors
    fn analyze_function_error(line_num: usize, line: &str) -> Self {
        // Extract function name
        let fn_name = Self::extract_function_name(line)
            .unwrap_or_else(|| "unknown".to_string());

        // Check for missing parameter name pattern: "fn test1("
        if line.contains('(') {
            let after_paren = line.split('(').nth(1).unwrap_or("");
            let trimmed_after = after_paren.trim();

            // Empty parentheses with no closer - probably missing param name
            if trimmed_after.is_empty() {
                return ParseError::MissingParameterName {
                    line: line_num,
                    function_name: fn_name,
                    hint: "Parameters must have names and types, like 'fn test1(x: int)'".to_string(),
                };
            }

            // Check for parameter without type: "fn test1(x int)" — only look inside ( )
            if let Some(param_part) = line.split('(').nth(1) {
                let inside = param_part.split(')').next().unwrap_or(param_part);
                if !inside.trim().is_empty() && inside.contains(' ') && !inside.contains(':') {
                    if let Some(param_name) = inside.split_whitespace().next() {
                        return ParseError::MissingTypeAnnotation {
                            line: line_num,
                            parameter_name: param_name.trim_end_matches(',').to_string(),
                        };
                    }
                }
            }
        }

        // Check for missing closing paren
        let open_count = line.matches('(').count();
        let close_count = line.matches(')').count();
        if open_count > close_count {
            return ParseError::MissingClosingParen {
                line: line_num,
                context: line.to_string(),
                opening_paren_pos: line.find('(').unwrap_or(0),
            };
        }

        // Check for missing colon after signature
        if line.contains(')') && !line.contains(':') {
            return ParseError::MissingColon {
                line: line_num,
                function_name: fn_name,
            };
        }

        // Check for missing return type after =>
        if let Some(after_arrow) = line.split("=>").nth(1) {
            let trimmed_after = after_arrow.trim();
            if trimmed_after.is_empty() {
                return ParseError::MissingReturnType {
                    line: line_num,
                    function_name: fn_name,
                };
            }
        }

        // Generic function syntax error
        ParseError::InvalidFunctionSyntax {
            line: line_num,
            context: line.to_string(),
            expected: "fn name(param1: Type1, ...) => ReturnType:".to_string(),
        }
    }

    /// Analyze main() entry point errors
    fn analyze_main_error(line_num: usize, line: &str) -> Self {
        // Check if line ends with colon (invalid syntax)
        if line.ends_with(':') {
            return ParseError::GenericSyntaxError {
                line: line_num,
                context: line.to_string(),
                hint: "Main entry point syntax: main(function_name()). Do not add a colon at the end.".to_string(),
            };
        }

        // Check for unclosed parentheses
        let open_count = line.matches('(').count();
        let close_count = line.matches(')').count();
        if open_count > close_count {
            return ParseError::MissingClosingParen {
                line: line_num,
                context: line.to_string(),
                opening_paren_pos: line.find('(').unwrap_or(0),
            };
        }

        // Check if parentheses are empty: main()
        if line == "main()" || line.starts_with("main() ") {
            return ParseError::GenericSyntaxError {
                line: line_num,
                context: line.to_string(),
                hint: "Main entry point requires a function to call: main(function_name())".to_string(),
            };
        }

        // Extract the content between main( and the last )
        if let Some(start) = line.strip_prefix("main(") {
            if let Some(end_pos) = start.rfind(')') {
                let content = &start[..end_pos];

                // Check if content is a valid function name
                // Extract function name (before first paren if exists)
                let func_name = if let Some(paren_pos) = content.find('(') {
                    &content[..paren_pos]
                } else {
                    content
                }.trim();

                // Check if it's a valid identifier
                if !crate::parser::expr::is_valid_identifier(func_name) {
                    return ParseError::GenericSyntaxError {
                        line: line_num,
                        context: line.to_string(),
                        hint: format!("'{}' is not a valid function name. Function names must start with a letter or underscore and contain only letters, digits, and underscores.", func_name),
                    };
                }
            }
        }

        // Generic main syntax error
        ParseError::GenericSyntaxError {
            line: line_num,
            context: line.to_string(),
            hint: "Main entry point syntax: main(function_name()) - must call a function with no arguments".to_string(),
        }
    }

    /// Extract function name from a function declaration line
    fn extract_function_name(line: &str) -> Option<String> {
        if !line.starts_with("fn ") {
            return None;
        }

        // "fn name(params..." -> extract "name"
        let after_fn = line[3..].trim();
        after_fn
            .split(|c: char| c == '(' || c == ' ')
            .next()
            .map(|s| s.to_string())
    }

    /// Generate helpful hint based on line content
    fn generate_hint(line: &str) -> String {
        if line.contains("fn ") {
            "Function syntax: fn name(params: Types) => ReturnType:".to_string()
        } else if line.contains("class ") {
            "Class syntax: class Name:".to_string()
        } else if line.contains("enum ") {
            "Enum syntax: enum Name:".to_string()
        } else if line.contains("if ") {
            "If syntax: if condition:".to_string()
        } else if line.contains("while ") {
            "While syntax: while condition:".to_string()
        } else if line.contains("for ") {
            "For syntax: for var in iterable:".to_string()
        } else if line.starts_with("let ") {
            "Let syntax: let name: Type = value".to_string()
        } else {
            "Check syntax in language documentation".to_string()
        }
    }

    /// Convert to diagnostic message
    pub fn to_message(&self) -> String {
        match self {
            ParseError::MissingParameterName { line, function_name, hint } => {
                format!("line {}: Parameter name missing in function '{}'. {}", line, function_name, hint)
            }
            ParseError::MissingClosingParen { line, context, .. } => {
                format!("line {}: Missing closing parenthesis in '{}'", line, context.trim())
            }
            ParseError::MissingReturnType { line, function_name } => {
                format!("line {}: Missing return type for function '{}'. Expected syntax: fn {}(...) => ReturnType:", line, function_name, function_name)
            }
            ParseError::MissingColon { line, function_name } => {
                format!("line {}: Missing colon after function signature for '{}'. Expected syntax: fn {}(...) => Type:", line, function_name, function_name)
            }
            ParseError::MissingTypeAnnotation { line, parameter_name } => {
                format!("line {}: Missing type annotation for parameter '{}'. Expected syntax: param: Type", line, parameter_name)
            }
            ParseError::InvalidFunctionSyntax { line, context, expected } => {
                format!("line {}: Invalid function syntax in '{}'. Expected: {}", line, context.trim(), expected)
            }
            ParseError::UnexpectedKeyword { line, keyword, expected } => {
                format!("line {}: Unexpected keyword '{}'. Expected one of: {}", line, keyword, expected.join(", "))
            }
            ParseError::MissingBlockBody { line, statement_type } => {
                format!("line {}: Missing body for '{}'. Expected to end with ':'", line, statement_type)
            }
            ParseError::InvalidTypeSyntax { line, type_str, reason } => {
                format!("line {}: Invalid type syntax '{}': {}", line, type_str, reason)
            }
            ParseError::GenericSyntaxError { line, context, hint } => {
                format!("line {}: Invalid syntax '{}'. {}", line, context.trim(), hint)
            }
            ParseError::WrongCommentSyntax { line, wrong_syntax, correct_syntax } => {
                format!(
                    "line {}: Invalid comment syntax '{}'.\n  = help: Replace with '{}'\n  = help: Single-line: {} This is a comment\n  = help: Multi-line: /#* This is a\n                     multi-line comment *#/",
                    line, wrong_syntax, correct_syntax, correct_syntax
                )
            }
            ParseError::InvalidUseOfBraces { line, context, brace_type } => {
                format!(
                    "line {}: Invalid syntax '{}'. Coffee is an indentation-based language and does not use braces {{}}.\n  = help: Remove the '{}' character\n  = help: Use consistent indentation (4 spaces recommended) instead of braces\n  = help: Example:\n       fn example() => int:\n           if x > 0:\n               return 1\n           return 0\n  = note: To access command-line arguments, use arg1, arg2, ... in your main statement:\n       main(entry_function(arg1, arg2))\n     This will pass the first two command-line arguments to entry_function",
                    line, context.trim(), brace_type
                )
            }
        }
    }

    /// Get error location
    pub fn line(&self) -> usize {
        match self {
            ParseError::MissingParameterName { line, .. } => *line,
            ParseError::MissingClosingParen { line, .. } => *line,
            ParseError::MissingReturnType { line, .. } => *line,
            ParseError::MissingColon { line, .. } => *line,
            ParseError::MissingTypeAnnotation { line, .. } => *line,
            ParseError::InvalidFunctionSyntax { line, .. } => *line,
            ParseError::UnexpectedKeyword { line, .. } => *line,
            ParseError::MissingBlockBody { line, .. } => *line,
            ParseError::InvalidTypeSyntax { line, .. } => *line,
            ParseError::GenericSyntaxError { line, .. } => *line,
            ParseError::WrongCommentSyntax { line, .. } => *line,
            ParseError::InvalidUseOfBraces { line, .. } => *line,
        }
    }

    /// Get suggestions for fixing the error
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            ParseError::MissingParameterName { hint, .. } => {
                vec![hint.clone()]
            }
            ParseError::MissingClosingParen { context, .. } => {
                vec![format!("Add a closing ')' to match the opening parenthesis in '{}'", context.trim())]
            }
            ParseError::MissingReturnType { function_name, .. } => {
                vec![format!("Add return type after '=>', like: fn {}(...) => int:", function_name)]
            }
            ParseError::MissingColon { function_name, .. } => {
                vec![format!("Add a colon ':' at the end: fn {}(...) => Type:", function_name)]
            }
            ParseError::MissingTypeAnnotation { parameter_name, .. } => {
                vec![format!("Add type annotation: {}: Type", parameter_name)]
            }
            ParseError::InvalidFunctionSyntax { expected, .. } => {
                vec![expected.clone()]
            }
            ParseError::UnexpectedKeyword { expected, .. } => {
                vec![format!("Try using one of these keywords: {}", expected.join(", "))]
            }
            ParseError::MissingBlockBody { statement_type, .. } => {
                vec![format!("Add a body block with a colon: {} ...", statement_type)]
            }
            ParseError::InvalidTypeSyntax { type_str, .. } => {
                vec![format!("Check type syntax: '{}'. Valid types: int, float, str, bool, or custom types", type_str)]
            }
            ParseError::GenericSyntaxError { hint, .. } => {
                vec![hint.clone()]
            }
            ParseError::WrongCommentSyntax { wrong_syntax, correct_syntax, .. } => {
                vec![
                    format!("Replace '{}' with '{}'", wrong_syntax, correct_syntax),
                    format!("Single-line comment: {} This is a comment", correct_syntax),
                    "Multi-line comment: /#* This is a".to_string(),
                    "                   multi-line comment *#/".to_string(),
                    "Note: Comments in Coffee use /#/ delimiters, not // or /* */".to_string(),
                ]
            }
            ParseError::InvalidUseOfBraces { .. } => {
                vec![
                    "Remove all curly braces {} and use indentation instead".to_string(),
                    "Use 4 spaces for each indentation level".to_string(),
                    "Example: Instead of '{' use ':' to start a block and indent the body".to_string(),
                ]
            }
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_message())
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_parameter_name() {
        let error = ParseError::detect_error(1, "fn test1(");
        assert!(matches!(error, ParseError::MissingParameterName { .. }));
    }

    #[test]
    fn test_missing_closing_paren() {
        let error = ParseError::detect_error(4, "fn test2(x: int");
        assert!(matches!(error, ParseError::MissingClosingParen { .. }));
    }

    #[test]
    fn test_missing_return_type() {
        let error = ParseError::detect_error(7, "fn test3(x: int) =>");
        assert!(matches!(error, ParseError::MissingReturnType { .. }));
    }

    #[test]
    fn test_extract_function_name() {
        assert_eq!(ParseError::extract_function_name("fn test1("), Some("test1".to_string()));
        assert_eq!(ParseError::extract_function_name("fn my_function(x:"), Some("my_function".to_string()));
    }

    #[test]
    fn test_missing_block_body_for_if() {
        let error = ParseError::detect_error(1, "if");
        assert!(matches!(error, ParseError::MissingBlockBody { statement_type, .. } if statement_type == "if"));
    }

    #[test]
    fn test_unexpected_keyword() {
        let error = ParseError::detect_error(1, "def foo():");
        assert!(matches!(error, ParseError::UnexpectedKeyword { keyword, .. } if keyword == "def"));
    }

    #[test]
    fn test_wrong_comment_syntax() {
        let error = ParseError::detect_error(1, "// not coffee");
        assert!(matches!(error, ParseError::WrongCommentSyntax { .. }));
    }
}
