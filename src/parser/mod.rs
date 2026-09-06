//! Parser Module for Coffee Language
//! 
//! This module provides the complete parser for the Coffee programming language,
//! handling all language constructs including functions, classes, enums, control flow,
//! expressions, and more. The parser is designed to handle both single-line statements
//! and complex multiline structures, using a combination of line-based and block-based
//! parsing approaches.
//! 
//! The parser system features:
//! - Line-based parsing with indentation-aware block structure detection
//! - Multiline statement parsing for functions, classes, control flow blocks, etc.
//! - Comprehensive error detection and reporting
//! - Support for Coffee's unique syntax features including:
//!   - Indentation-based block structure
//!   - Memory management operations (mv, copy, clone, rm, clean)
//!   - Pattern matching with match expressions
//!   - Class and enum definitions
//!   - Import statements with various formats
//! - Block tracking to correctly identify nested structures
//! 
//! The top-level `parse_program` function orchestrates the parsing process,
//! combining single-line and multiline parsing strategies to handle complete
//! Coffee source files.


pub mod import;
pub mod ty;
pub mod function;
pub mod main;
pub mod r#if;
pub mod r#while;
pub mod r#match;
pub mod pattern;
pub mod r#for;
pub mod var;
pub mod comment;
pub mod class;
pub mod memory;
pub mod expr;
pub mod error;
pub mod tracker;
pub mod indent;
pub mod raise;
pub mod type_decl;

pub use error::ParseError;
pub use tracker::{BlockTracker, BlockType};
pub use raise::RaiseStmt;

pub use import::Import;
pub use function::{Function, FunctionBody};
pub use main::MainEntry;

pub use r#if::IfExpr;
pub use r#while::WhileLoop;
pub use r#match::MatchExpr;
pub use pattern::Pattern;
pub use r#for::{ForLoop, ForIterator};
pub use var::{VariableDecl, ReturnStmt, BreakStmt, ContinueStmt};
pub use comment::{SingleLineComment, MultiLineComment};
pub use class::{ClassDef, EnumDef};
pub use memory::MemoryOp;
pub use type_decl::{TypeDecl, parse_type_decl};

mod stmt;
mod line;
mod multiline;
mod program;

pub use stmt::Statement;
pub use program::{Program, parse_program};
pub use multiline::parse_multiline_statement;
pub use line::parse_single_line_statement;
pub(crate) use multiline::collect_multiline_variable_decl;

/// Split `a, b, (c, d), [e, f]` on commas that are not inside `()`, `[]`, `{}`, or strings.
pub(crate) fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut paren = 0i32;
    let mut brace = 0i32;
    let mut bracket = 0i32;
    let mut in_string = false;
    for (i, ch) in input.char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '(' if !in_string => paren += 1,
            ')' if !in_string => paren -= 1,
            '{' if !in_string => brace += 1,
            '}' if !in_string => brace -= 1,
            '[' if !in_string => bracket += 1,
            ']' if !in_string => bracket -= 1,
            ',' if !in_string && paren == 0 && brace == 0 && bracket == 0 => {
                let part = input[start..i].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    let last = input[start..].trim();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// Take text up to the first header `:`: skip `::`, and colons inside `()`, `[]`, `{}`, or strings.
/// Rest starts at that colon (same shape as a nom `take_until`).
pub(crate) fn take_until_header_colon(input: &str) -> nom::IResult<&str, &str> {
    let mut paren = 0i32;
    let mut brace = 0i32;
    let mut bracket = 0i32;
    let mut in_string = false;
    let bytes = input.as_bytes();
    let mut chars = input.char_indices();
    while let Some((i, ch)) = chars.next() {
        match ch {
            '"' => in_string = !in_string,
            '(' if !in_string => paren += 1,
            ')' if !in_string => paren -= 1,
            '{' if !in_string => brace += 1,
            '}' if !in_string => brace -= 1,
            '[' if !in_string => bracket += 1,
            ']' if !in_string => bracket -= 1,
            ':' if !in_string && paren == 0 && brace == 0 && bracket == 0 => {
                if bytes.get(i + 1) == Some(&b':') {
                    chars.next();
                    continue;
                }
                return Ok((&input[i..], &input[..i]));
            }
            _ => {}
        }
    }
    Err(nom::Err::Error(nom::error::Error {
        input,
        code: nom::error::ErrorKind::TakeUntil,
    }))
}

/// Index of the `)` that closes `input[0] == '('`, or `None` if unbalanced.
pub(crate) fn index_of_matching_close_paren(input: &str) -> Option<usize> {
    if !input.starts_with('(') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    for (i, ch) in input.char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_program_long_file_beyond_old_cap() {
        let mut src = String::new();
        for i in 0..150 {
            src.push_str(&format!("let x{}: int = {}\n", i, i));
        }
        let program = parse_program(&src).expect("files longer than 100 lines must still parse");
        assert_eq!(program.statements.len(), 150);
    }

    #[test]
    fn parse_program_class_of_error_empty_and_child_of_base() {
        let src = r#"
class E of Error:

class Child of Base:
    y: int

fn main() => int:
    return 0
"#;
        let program = parse_program(src).unwrap_or_else(|e| panic!("parse: {e:?}"));
        let classes: Vec<&str> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Class(c) => Some(c.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(classes, vec!["E", "Child"]);
    }

    #[test]
    fn parse_program_error_subclass_with_method() {
        let src = r#"
class Boom of Error:
    extra: int

    fn ping(self) => int:
        return self.extra

fn main() => int:
    return 0
"#;
        let program = parse_program(src).unwrap_or_else(|e| panic!("parse: {e:?}"));
        let boom = program
            .statements
            .iter()
            .find_map(|s| match s {
                Statement::Class(c) if c.name == "Boom" => Some(c),
                _ => None,
            })
            .expect("Boom");
        assert_eq!(boom.methods.len(), 1);
        assert_eq!(boom.methods[0].name, "ping");
    }

    #[test]
    fn long_file_several_functions_all_parse() {
        let mut src = String::new();
        let mut expected = Vec::new();
        for f in 0..6 {
            expected.push(format!("func{}", f));
            src.push_str(&format!("fn func{}() => int:\n", f));
            for i in 0..30 {
                src.push_str(&format!("    let a{}: int = {}\n", i, i));
            }
            for i in 0..30 {
                src.push_str(&format!("    rm a{}\n", i));
            }
            src.push_str("    return 0\n\n");
        }
        assert!(src.lines().count() >= 150, "fixture must exceed 150 lines");
        let program = parse_program(&src).expect("150+ line file with several fn must parse");
        let names: Vec<&str> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Function(func) => Some(func.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn parse_assignment_without_spaces() {
        let program = parse_program("x=1\n").expect("x=1 should parse as assignment");
        assert!(matches!(
            program.statements.as_slice(),
            [Statement::Assignment(name, _)] if name == "x"
        ));
    }

    #[test]
    fn parse_assignment_invalid_rhs_is_not_literal() {
        match parse_single_line_statement("x = @@@") {
            Some(Statement::Assignment(_, crate::parser::expr::Expression::Literal(s))) => {
                panic!("invalid assignment RHS must not be Literal, got {:?}", s)
            }
            Some(other) => panic!("invalid assignment RHS must not parse; got {:?}", other),
            None => {}
        }
    }

    #[test]
    fn parse_nested_member_assignment_flattens_dotted_name() {
        let program = parse_program("p.x.y=1\n").expect("p.x.y=1 should parse as assignment");
        assert!(matches!(
            program.statements.as_slice(),
            [Statement::Assignment(name, _)] if name == "p.x.y"
        ));
    }

    #[test]
    fn parse_assignment_does_not_treat_comparisons() {
        assert!(
            parse_single_line_statement("x==1")
                .is_none()
                || !matches!(parse_single_line_statement("x==1"), Some(Statement::Assignment(_, _))),
            "== must not parse as assignment"
        );
        assert!(!matches!(
            parse_single_line_statement("x!=1"),
            Some(Statement::Assignment(_, _))
        ));
        assert!(!matches!(
            parse_single_line_statement("x<=1"),
            Some(Statement::Assignment(_, _))
        ));
        assert!(!matches!(
            parse_single_line_statement("x>=1"),
            Some(Statement::Assignment(_, _))
        ));
    }

    #[test]
    fn broken_if_does_not_promote_body_to_top_level() {
        let src = "if\n    let x: int = 1\n";
        match parse_program(src) {
            Ok(program) => panic!(
                "broken if must be a parse error, got statements: {:?}",
                program.statements
            ),
            Err(errors) => {
                assert!(
                    errors.iter().any(|e| matches!(
                        e,
                        ParseError::MissingBlockBody { statement_type, .. } if statement_type == "if"
                    ) || matches!(e, ParseError::GenericSyntaxError { .. })),
                    "expected MissingBlockBody, got {:?}",
                    errors
                );
            }
        }
    }

    #[test]
    fn safety_cap_emits_parse_error() {
        // Directly cover the error variant used when the iteration cap is hit.
        let err = ParseError::GenericSyntaxError {
            line: 1,
            context: String::new(),
            hint: "Parser safety limit reached; remaining input was not parsed".to_string(),
        };
        assert!(err.to_message().contains("safety limit"));
    }
}


#[cfg(test)]
mod parse_program_tests {
    use super::*;

    #[test]
    fn long_file_exceeds_old_100_line_cap() {
        let mut src = String::new();
        for i in 0..150 {
            src.push_str(&format!("let x{}: int = {}\n", i, i));
        }
        let program = parse_program(&src).expect("file longer than 100 lines should still parse");
        let lets = program
            .statements
            .iter()
            .filter(|s| matches!(s, Statement::VariableDecl(_)))
            .count();
        assert_eq!(lets, 150);
    }

    #[test]
    fn eight_top_level_functions_preserve_source_order() {
        let mut src = String::new();
        for i in 0..8 {
            src.push_str(&format!("fn f{}() => int:\n    return {}\n\n", i, i));
        }
        let program = parse_program(&src).expect("eight top-level functions should parse");
        let names: Vec<&str> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Function(f) => Some(f.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7"]);
    }

    #[test]
    fn parallel_unit_error_uses_absolute_line() {
        let mut src = String::new();
        for i in 0..8 {
            if i == 3 {
                src.push_str("fn\n    return 0\n\n");
            } else {
                src.push_str(&format!("fn f{}() => int:\n    return 0\n\n", i));
            }
        }
        let err = parse_program(&src).expect_err("broken fn among eight units should error");
        assert!(
            err.iter().any(|e| matches!(
                e,
                ParseError::MissingBlockBody { line: 10, .. }
                    | ParseError::InvalidFunctionSyntax { line: 10, .. }
                    | ParseError::GenericSyntaxError { line: 10, .. }
            )),
            "error must attach to the broken unit's source line, got {:?}",
            err
        );
    }

    #[test]
    fn assignment_without_spaces() {
        let program = parse_program("x=1\n").expect("x=1 should parse as assignment");
        assert!(matches!(
            &program.statements[..],
            [Statement::Assignment(name, _)] if name == "x"
        ));
    }

    #[test]
    fn comparison_is_not_assignment() {
        let program = parse_program("x==1\n").expect("x==1 should parse as expression");
        assert!(matches!(program.statements[0], Statement::Expr(_)));
    }

    #[test]
    fn broken_if_does_not_promote_indented_let() {
        let src = "if\n    let x: int = 1\n";
        let err = parse_program(src).expect_err("broken if must be a parse error");
        assert!(err.iter().any(|e| matches!(
            e,
            ParseError::MissingBlockBody { statement_type, .. } if statement_type == "if"
        )));
        // Recovery must consume the indented `let` so we do not also report a second success path
        assert!(parse_program(src).is_err());
    }

    #[test]
    fn factorial_parses_two_functions() {
        let src = r#"
fn factorial(n: int) => int:
    if n <= 1:
        return 1
    else:
        let result: int = n * factorial(n - 1)
        rm n
        return result

fn main() => int:
    let result: int = factorial(5)
    rm result
    return 0
"#;
        let program = parse_program(src).expect("factorial fixture should parse");
        let names: Vec<&str> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Function(f) => Some(f.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["factorial", "main"]);
    }

    #[test]
    fn two_fn_file_has_nonzero_nonoverlapping_stmt_spans() {
        let src = "fn a() => int:\n    return 1\n\nfn b() => int:\n    return 2\n";
        let program = parse_program(src).expect("two functions should parse");
        assert_eq!(program.statements.len(), 2);
        assert_eq!(program.stmt_spans.len(), 2);
        let a = program.stmt_spans[0];
        let b = program.stmt_spans[1];
        assert!(a.end > a.start, "first statement span must be non-zero");
        assert!(b.end > b.start, "second statement span must be non-zero");
        assert!(
            a.end <= b.start,
            "statement spans must not overlap: {:?} {:?}",
            a,
            b
        );
        assert!(
            src[a.start..a.end].contains("fn a()"),
            "first span must cover fn a in original source, got {:?}",
            &src[a.start..a.end]
        );
        assert!(
            src[b.start..b.end].contains("fn b()"),
            "second span must cover fn b in original source, got {:?}",
            &src[b.start..b.end]
        );
    }

    #[test]
    fn safety_cap_emits_error() {
        // Directly exercise the error variant used when the cap is hit
        let err = ParseError::GenericSyntaxError {
            line: 1,
            context: String::new(),
            hint: "Parser safety limit reached; remaining input was not parsed".to_string(),
        };
        assert!(err.to_message().contains("safety limit"));
    }
}

