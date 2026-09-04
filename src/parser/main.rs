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
use crate::parser::expr::{parse_expression, Expression};

/// Main entry point statement
#[derive(Debug, PartialEq, Clone)]
pub struct MainEntry {
    /// Name of the entry function
    pub entry_function: String,
    /// Arguments to pass to the entry function
    pub args: Vec<Expression>,
}

impl MainEntry {
    pub fn new(entry_function: String, args: Vec<Expression>) -> Self {
        Self {
            entry_function,
            args,
        }
    }
}

pub fn parse_main_entry(input: &str) -> IResult<&str, MainEntry> {
    coffee_debug!("DEBUG: parse_main_entry: input='{}'", input);

    if input.starts_with("main(") {
        let (input, _) = preceded(space0, tag("main(")).parse(input)?;
        let (input, _) = space0.parse(input)?;

        coffee_debug!("DEBUG: parse_main_entry: after parsing 'main(', input='{}'", input);

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

        let content = &input[..end_pos];
        let remaining = &input[end_pos + 1..];

        coffee_debug!("DEBUG: parse_main_entry: content='{}', remaining='{}'", content, remaining);

        if let Some(paren_pos) = content.find('(') {
            let func_name = content[..paren_pos].trim();

            if !is_valid_function_name(func_name) {
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Verify,
                )));
            }

            let args_str = &content[paren_pos + 1..];

            coffee_debug!("DEBUG: parse_main_entry: func_name='{}', args_str='{}'", func_name, args_str);

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

            let args = parse_main_args(actual_args)?;

            coffee_debug!("DEBUG: parse_main_entry: args={:?}", args);

            Ok((remaining, MainEntry::new(func_name.to_string(), args)))
        } else {
            let func_name = content.trim();

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
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    }
}

fn parse_main_args(actual_args: &str) -> Result<Vec<Expression>, nom::Err<nom::error::Error<&str>>> {
    if actual_args.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for ch in actual_args.chars() {
        if escape_next {
            current.push(ch);
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => {
                escape_next = true;
                current.push(ch);
            }
            '"' => {
                in_string = !in_string;
                current.push(ch);
            }
            '(' if !in_string => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_string => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_string && depth == 0 => {
                push_parsed_arg(actual_args, &mut args, &current)?;
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        push_parsed_arg(actual_args, &mut args, &current)?;
    }

    Ok(args)
}

fn push_parsed_arg<'a>(
    err_input: &'a str,
    args: &mut Vec<Expression>,
    raw: &str,
) -> Result<(), nom::Err<nom::error::Error<&'a str>>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    match parse_expression(trimmed) {
        Ok(expr) => {
            args.push(expr);
            Ok(())
        }
        Err(_) => Err(nom::Err::Error(nom::error::Error::new(
            err_input,
            nom::error::ErrorKind::Fail,
        ))),
    }
}

pub fn is_valid_function_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();

    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {},
        _ => return false,
    }

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
        assert_eq!(
            main_entry.args,
            vec![Expression::Literal("\"config.json\"".to_string())]
        );
    }

    #[test]
    fn test_parse_main_with_multiple_args() {
        let input = "main(my_program(42, \"hello\"))";
        let result = parse_main_entry(input);
        assert!(result.is_ok());
        let (remaining, main_entry) = result.unwrap();
        assert_eq!(remaining, "");
        assert_eq!(main_entry.entry_function, "my_program");
        assert_eq!(
            main_entry.args,
            vec![
                Expression::Literal("42".to_string()),
                Expression::Literal("\"hello\"".to_string()),
            ]
        );
    }
}
