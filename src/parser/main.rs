//! Main Entry Point Parser for Coffee Language
//! 
//! This module handles parsing of the main entry point syntax in Coffee programs.
//! The main entry point is specified using the `main` keyword followed by a function
//! call in parentheses. This defines which function serves as the starting point
//! for program execution.
//! 
//! The parser supports various forms of main entry point specifications:
//! - Simple function call: `main(run())`
//! - Function call with arguments: `main(start("config.json"))`
//! - Function call with multiple arguments: `main(main_function(arg1, arg2))`
//! 
//! The module includes validation to ensure that function names are valid
//! identifiers and that the syntax follows the expected pattern.

use nom::{
    bytes::complete::tag,
    character::complete::space0,
    sequence::preceded,
    IResult, Parser,
};

use crate::coffee_debug;

/// Main entry point statement
/// 
/// This structure represents the main entry point of a Coffee program, which specifies
/// which function serves as the starting point for program execution. The main entry
/// point is defined using the `main` keyword followed by a function call.
/// 
/// The entry point can call a function with or without arguments. Arguments are
/// stored as strings and will be processed during the compilation phase.
#[derive(Debug, PartialEq, Clone)]
pub struct MainEntry {
    /// Name of the entry function
    pub entry_function: String,
    /// Arguments to pass to the entry function (as strings)
    pub args: Vec<String>,
}

impl MainEntry {
    /// Create a new main entry point
    /// 
    /// This constructor creates a new MainEntry instance with the specified
    /// function name and arguments.
    /// 
    /// # Arguments
    /// 
    /// * `entry_function` - The name of the function to be used as the entry point
    /// * `args` - A vector of arguments to pass to the entry function
    /// 
    /// # Returns
    /// 
    /// A new MainEntry instance
    pub fn new(entry_function: String, args: Vec<String>) -> Self {
        Self {
            entry_function,
            args,
        }
    }
}

/// Parse a main entry point statement
/// 
/// This function parses the main entry point syntax in Coffee, which is specified
/// as `main(function_name(args))`. The function handles both forms:
/// - `main(function_name())` - function with no arguments
/// - `main(function_name(arg1, arg2, ...))` - function with arguments
/// 
/// The parser uses a parenthesis depth counter to correctly handle nested
/// parentheses in the function arguments. It also validates that the function
/// name is a valid identifier according to Coffee's naming rules.
/// 
/// # Arguments
/// 
/// * `input` - The input string containing the main entry point statement to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, MainEntry))` - Successfully parsed main entry and remaining input
/// * `Err(nom::Err)` - If the input does not match the main entry point pattern
pub fn parse_main_entry(input: &str) -> IResult<&str, MainEntry> {
    coffee_debug!("DEBUG: parse_main_entry: input='{}'", input);

    // Check if input is "main(" or just "main"
    if input.starts_with("main(") {
        // Parse "main(" ... ")"
        let (input, _) = preceded(space0, tag("main(")).parse(input)?;
        let (input, _) = space0.parse(input)?;

        coffee_debug!("DEBUG: parse_main_entry: after parsing 'main(', input='{}'", input);

        // Now we need to parse: function_name(arg1, arg2, ...))
        // Find the closing parenthesis for main( that matches the opening
        // The opening "main(" has already been consumed by tag("main(")
        // So we need to find the matching closing paren for the outer main()
        // Start depth at 0, and increment on '(' (inner function call)
        let mut depth = 0;
        let mut end_pos = 0;
        let chars: Vec<char> = input.chars().collect();

        for i in 0..chars.len() {
            match chars[i] {
                '(' => {
                    depth += 1;
                }
                ')' => {
                    if depth == 0 {
                        // This is the matching closing paren for the outer main()
                        end_pos = i;
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }

        coffee_debug!("DEBUG: parse_main_entry: end_pos={}, depth={}", end_pos, depth);

        if depth != 0 {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )));
        }

        // content is everything before the closing paren of main()
        // This includes the "main(" part, so we need to skip it
        let content = &input[..end_pos];
        let remaining = &input[end_pos + 1..];

        coffee_debug!("DEBUG: parse_main_entry: content='{}', remaining='{}'", content, remaining);

        // Parse content as function_name(args) or just function_name
        if let Some(paren_pos) = content.find('(') {
            let func_name = content[..paren_pos].trim();

            // Validate that func_name is a valid identifier
            if !is_valid_function_name(func_name) {
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Verify,
                )));
            }

            // Get everything between the ( and its matching )
            let args_str = &content[paren_pos + 1..];

            coffee_debug!("DEBUG: parse_main_entry: func_name='{}', args_str='{}'", func_name, args_str);

            // Find matching closing paren for the function call
            let mut arg_depth = 1;
            let mut arg_end = 0;
            let arg_chars: Vec<char> = args_str.chars().collect();

            for i in 0..arg_chars.len() {
                match arg_chars[i] {
                    '(' => arg_depth += 1,
                    ')' => {
                        arg_depth -= 1;
                        if arg_depth == 0 {
                            arg_end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            let actual_args = &args_str[..arg_end];
            coffee_debug!("DEBUG: parse_main_entry: actual_args='{}'", actual_args);

            // Parse arguments (if any)
            let args = if actual_args.trim().is_empty() {
                Vec::new()
            } else {
                actual_args
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            };

            coffee_debug!("DEBUG: parse_main_entry: args={:?}", args);

            Ok((remaining, MainEntry::new(func_name.to_string(), args)))
        } else {
            // No arguments, just function name (e.g., main(func))
            let func_name = content.trim();

            // Validate that func_name is a valid identifier
            if !is_valid_function_name(func_name) {
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Verify,
                )));
            }

            Ok((
                remaining,
                MainEntry::new(func_name.to_string(), Vec::new()),
            ))
        }
    } else {
        // Input is just "main" (should not happen in normal usage)
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    }
}

/// Check if a string is a valid function name
/// 
/// This function validates that a given string is a valid Coffee function name
/// according to the language's naming rules:
/// - Must not be empty
/// - Must start with a letter or underscore
/// - Must contain only letters, digits, and underscores
/// 
/// This validation is used during parsing to ensure that function names in main
/// entry point statements follow Coffee's identifier rules.
/// 
/// # Arguments
/// 
/// * `name` - The string to validate as a function name
/// 
/// # Returns
/// 
/// * `true` if the string is a valid Coffee function name
/// * `false` otherwise
pub fn is_valid_function_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();

    // First character must be letter or underscore
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {},
        _ => return false,
    }

    // Remaining characters must be alphanumeric or underscore
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_main() {
        let input = "main(run())";
        let result = parse_main_entry(input);
        assert!(result.is_ok());
        let (remaining, main_entry) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(main_entry.entry_function, "run");
        assert!(main_entry.args.is_empty());
    }

    #[test]
    fn test_parse_main_with_args() {
        let input = "main(app_start(\"config.json\"))";
        let result = parse_main_entry(input);
        assert!(result.is_ok());
        let (remaining, main_entry) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(main_entry.entry_function, "app_start");
        assert_eq!(main_entry.args, vec!["\"config.json\""]);
    }

    #[test]
    fn test_parse_main_with_multiple_args() {
        let input = "main(my_program(42, \"hello\"))";
        let result = parse_main_entry(input);
        assert!(result.is_ok());
        let (remaining, main_entry) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(main_entry.entry_function, "my_program");
        assert_eq!(main_entry.args, vec!["42", "\"hello\""]);
    }
}
