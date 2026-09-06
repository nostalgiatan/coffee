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
    /// Coffee uses `{` `}` in struct literals (`Point { x: 1 }`, `raise Err { field: v }`).
    /// Only treat braces as C-style *blocks* when the line looks like a brace-delimited body.
    fn looks_like_c_block_braces(trimmed: &str) -> bool {
        if trimmed == "{" || trimmed == "}" {
            return true;
        }
        if trimmed.ends_with('{') {
            return true;
        }
        if !trimmed.contains('{') && !trimmed.contains('}') {
            return false;
        }
        matches!(
            Self::leading_word(trimmed),
            "if" | "while" | "for" | "fn" | "else" | "elif"
        )
    }

    fn leading_word(trimmed: &str) -> &str {
        let raw = trimmed
            .split(|c: char| c.is_whitespace() || c == '(')
            .next()
            .unwrap_or("");
        raw.trim_end_matches(|c: char| matches!(c, ':' | '{' | '}' | ';' | ','))
    }

    fn is_foreign_keyword_error(err: &ParseError) -> bool {
        match err {
            ParseError::UnexpectedKeyword { keyword, .. } => matches!(
                keyword.as_str(),
                "import"
                    | "try"
                    | "except"
                    | "catch"
                    | "finally"
                    | "def"
                    | "function"
                    | "func"
                    | "var"
                    | "const"
                    | "#include"
                    | "package"
            ),
            ParseError::InvalidUseOfBraces { .. } | ParseError::WrongCommentSyntax { .. } => true,
            _ => false,
        }
    }

    /// Prefer a foreign-construct / C-brace diagnostic inside a failed `fn`/`if`/… block
    /// over blaming a complete-looking header (`InvalidFunctionSyntax` on `fn main() => int:`).
    pub fn detect_error_in_block(start_line: usize, lines: &[&str]) -> Self {
        let mut header: Option<ParseError> = None;
        for (i, raw) in lines.iter().enumerate() {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with("/#/") {
                continue;
            }
            let err = Self::detect_error(start_line + i, raw);
            if Self::is_foreign_keyword_error(&err) {
                return err;
            }
            // Prefer incomplete inner blocks (`if true:` with no indent) over a
            // generic error on the enclosing `fn` header.
            if matches!(err, ParseError::MissingBlockBody { .. }) {
                header = Some(err);
                continue;
            }
            if header.is_none() {
                header = Some(err);
            }
        }
        header.unwrap_or_else(|| {
            Self::detect_error(start_line, lines.first().copied().unwrap_or(""))
        })
    }

    /// Detect specific error patterns from a failed parse line
    pub fn detect_error(line_num: usize, line: &str) -> Self {
        let trimmed = line.trim();

        // C-style blocks only — not Coffee struct literals.
        if Self::looks_like_c_block_braces(trimmed) {
            let brace_type = if trimmed.contains('{') { "{" } else { "}" };
            return ParseError::InvalidUseOfBraces {
                line: line_num,
                context: trimmed.to_string(),
                brace_type: brace_type.to_string(),
            };
        }

        // `#include` is a C leftover, not a Coffee `#` comment.
        if trimmed.starts_with("#include") {
            return ParseError::UnexpectedKeyword {
                line: line_num,
                keyword: "#include".to_string(),
                expected: Self::known_keywords(),
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

        // Unknown keyword at the start of a failed statement (`try:` → `try`)
        let word = Self::leading_word(trimmed);
        if !word.is_empty()
            && (word == "#include"
                || word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_'))
            && !Self::known_keyword_slice().contains(&word)
        {
            return ParseError::UnexpectedKeyword {
                line: line_num,
                keyword: word.to_string(),
                expected: Self::known_keywords(),
            };
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

        // Complete header (`fn name(...) => Type:`) with no indented body
        if line.contains(')') && line.contains("=>") && line.trim_end().ends_with(':') {
            return ParseError::MissingBlockBody {
                line: line_num,
                statement_type: "fn".to_string(),
            };
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

    fn known_keyword_slice() -> &'static [&'static str] {
        &[
            "fn", "c", "if", "elif", "else", "while", "for", "match", "enum", "class",
            "packed", "let", "return", "raise", "break", "continue", "use", "main",
            "mv", "copy", "clone", "rm", "clean", "true", "false",
        ]
    }

    fn known_keywords() -> Vec<String> {
        Self::known_keyword_slice()
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    /// Coffee-specific hints for unknown/wrong keywords (Python/JS leftovers, etc.)
    fn unexpected_keyword_suggestions(keyword: &str, expected: &[String]) -> Vec<String> {
        let mut hints = Vec::new();
        match keyword {
            "def" | "function" | "func" => hints.push(
                "Coffee uses `fn` for functions, not `def`/`function`/`func`. Example: `fn foo() => int:`.".to_string(),
            ),
            "try" | "catch" | "except" | "finally" => hints.push(
                "Coffee has no try/catch. Use `raise` to throw; there is no catch block.".to_string(),
            ),
            "import" => hints.push(
                "Coffee imports with `use module` or `use name in lib of c`, not `import`.".to_string(),
            ),
            "var" | "const" => hints.push(
                "Coffee bindings use `let name: Type = value`, not `var` or `const`.".to_string(),
            ),
            "#include" => hints.push(
                "Coffee has no `#include`. Use `use name in lib of c` (and a `.cfc` file when needed).".to_string(),
            ),
            "package" => hints.push(
                "Coffee has no `package` keyword. Put sources under `src/` with `coffee.toml`.".to_string(),
            ),
            _ => {}
        }
        hints.push(format!(
            "Unexpected `{}`. Coffee keywords include: {}",
            keyword,
            expected.join(", ")
        ));
        hints.push(
            "Allocated values must be released with `rm` (Coffee has no implicit drop).".to_string(),
        );
        hints
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
                let hint = Self::unexpected_keyword_suggestions(keyword, expected)
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| format!("Expected one of: {}", expected.join(", ")));
                format!("line {}: Unexpected keyword '{}'. {}", line, keyword, hint)
            }
            ParseError::MissingBlockBody { line, statement_type } => {
                format!("line {}: Missing body for '{}'. Expected to end with ':'", line, statement_type)
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
            ParseError::GenericSyntaxError { line, .. } => *line,
            ParseError::WrongCommentSyntax { line, .. } => *line,
            ParseError::InvalidUseOfBraces { line, .. } => *line,
        }
    }

    /// Get suggestions for fixing the error
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            ParseError::MissingParameterName { hint, .. } => {
                vec![
                    hint.clone(),
                    "Coffee parameters need a name and a type, e.g. `fn f(x: int) => int:`.".to_string(),
                ]
            }
            ParseError::MissingClosingParen { context, .. } => {
                vec![
                    format!("Add a closing ')' to match the opening parenthesis in '{}'", context.trim()),
                    "Function headers look like `fn name(params: Types) => ReturnType:`.".to_string(),
                ]
            }
            ParseError::MissingReturnType { function_name, .. } => {
                vec![
                    format!("Add a return type after '=>', like: fn {}(...) => int:", function_name),
                    "Then start the indented body on the next line (Coffee uses indent, not braces).".to_string(),
                ]
            }
            ParseError::MissingColon { function_name, .. } => {
                vec![
                    format!("Add a colon ':' after the signature: fn {}(...) => Type:", function_name),
                    "Coffee blocks are indentation-based: `:` then an indented body, not `{}`.".to_string(),
                ]
            }
            ParseError::MissingTypeAnnotation { parameter_name, .. } => {
                vec![
                    format!("Add a type annotation: {}: Type", parameter_name),
                    "Example: `fn f(x: int) => int:` — types are required, not inferred from the name.".to_string(),
                ]
            }
            ParseError::InvalidFunctionSyntax { expected, .. } => {
                vec![
                    expected.clone(),
                    "Use `fn`, not `def`. End the header with `:` and indent the body.".to_string(),
                ]
            }
            ParseError::UnexpectedKeyword { keyword, expected, .. } => {
                Self::unexpected_keyword_suggestions(keyword, expected)
            }
            ParseError::MissingBlockBody { statement_type, .. } => {
                vec![
                    format!("End `{}` with ':' and indent the body (4 spaces).", statement_type),
                    "Coffee is indentation-based and does not use `{` `}` for blocks.".to_string(),
                ]
            }
            ParseError::GenericSyntaxError { hint, .. } => {
                vec![
                    hint.clone(),
                    "Check Coffee syntax: `fn` (not `def`), `/#/` comments (not `//`), indent blocks, and `rm` every `let`.".to_string(),
                ]
            }
            ParseError::WrongCommentSyntax { wrong_syntax, correct_syntax, .. } => {
                vec![
                    format!("Coffee comments use '{}', not '{}'.", correct_syntax, wrong_syntax),
                    format!("Single-line: {} This is a comment", correct_syntax),
                    "Multi-line: /#* This is a".to_string(),
                    "                   multi-line comment *#/".to_string(),
                    "`//`, `#`, and `/* */` are not Coffee comment syntax.".to_string(),
                ]
            }
            ParseError::InvalidUseOfBraces { .. } => {
                vec![
                    "Coffee is indentation-based: remove `{` `}` and start the block with `:`.".to_string(),
                    "Indent the body with 4 spaces per level.".to_string(),
                    "Example: `fn example() => int:` then indent `return 0`.".to_string(),
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
    fn test_missing_block_body_for_complete_fn_header() {
        let error = ParseError::detect_error(1, "fn main() => int:");
        assert!(
            matches!(error, ParseError::MissingBlockBody { ref statement_type, .. } if statement_type == "fn"),
            "{error:?}"
        );
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
        let joined = error.suggestions().join(" ");
        assert!(joined.contains("/#/"), "{joined}");
        assert!(joined.contains("//"), "{joined}");
    }

    #[test]
    fn test_def_suggestions_point_to_fn() {
        let error = ParseError::detect_error(1, "def foo():");
        let joined = error.suggestions().join(" ");
        assert!(joined.contains("`fn`"), "{joined}");
        assert!(joined.contains("`def`"), "{joined}");
        assert!(joined.contains("`rm`"), "{joined}");
    }

    #[test]
    fn test_try_suggestions_point_to_raise_not_catch() {
        let error = ParseError::detect_error(1, "try foo()");
        let joined = error.suggestions().join(" ");
        assert!(joined.contains("`raise`"), "{joined}");
        assert!(joined.contains("no try/catch"), "{joined}");
    }

    #[test]
    fn try_with_colon_is_unexpected_keyword() {
        let error = ParseError::detect_error(2, "    try:");
        assert!(matches!(error, ParseError::UnexpectedKeyword { ref keyword, .. } if keyword == "try"));
        assert!(error.to_message().contains("raise"), "{}", error.to_message());
    }

    #[test]
    fn import_suggests_use_module() {
        let error = ParseError::detect_error(1, "import math");
        let msg = error.to_message();
        let joined = error.suggestions().join(" ");
        assert!(msg.contains("use") || joined.contains("use"), "{msg} / {joined}");
        assert!(joined.contains("use name in lib of c") || joined.contains("`use module`"), "{joined}");
    }

    #[test]
    fn struct_literal_braces_are_not_c_blocks() {
        let point = ParseError::detect_error(1, "Point { x: 1 }");
        assert!(!matches!(point, ParseError::InvalidUseOfBraces { .. }), "{point:?}");
        let raise = ParseError::detect_error(1, "raise Err { field: v }");
        assert!(!matches!(raise, ParseError::InvalidUseOfBraces { .. }), "{raise:?}");
    }

    #[test]
    fn c_style_if_brace_is_c_block() {
        let error = ParseError::detect_error(2, "    if true {");
        assert!(matches!(error, ParseError::InvalidUseOfBraces { .. }), "{error:?}");
        assert!(error.to_message().to_lowercase().contains("indent"), "{}", error.to_message());
    }

    #[test]
    fn detect_error_in_block_prefers_try_in_fn_body() {
        let lines = ["fn main() => int:", "    try:", "        return 0"];
        let error = ParseError::detect_error_in_block(1, &lines);
        assert!(matches!(error, ParseError::UnexpectedKeyword { keyword, .. } if keyword == "try"));
    }
}
