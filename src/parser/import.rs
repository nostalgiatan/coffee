//! Import Statement Parser for Coffee Language
//! 
//! This module handles parsing of import statements in the Coffee programming language.
//! Coffee supports various import syntaxes for importing modules, symbols, and foreign
//! language functions (particularly C functions). The parser recognizes and handles:
//! 
//! - Simple imports: `use module`
//! - Aliased imports: `use module as alias`
//! - Selective imports: `use symbol in module`
//! - Foreign language imports: `use symbol in module of lang`
//! 
//! The import system is crucial for Coffee's modularity and its ability to interoperate
//! with other languages, particularly C.

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{multispace0, multispace1, space0},
    combinator::opt,
    multi::separated_list1,
    sequence::delimited,
    sequence::preceded,
    IResult, Parser,
};

/// Represents different types of import statements in Coffee
/// 
/// This enum captures all the various ways imports can be expressed in Coffee,
/// from simple module imports to complex foreign language function imports.
/// Each variant corresponds to a different import syntax pattern.
#[derive(Debug, PartialEq, Clone)]
pub enum Import {
    /// Simple import (e.g., `use std.math`)
    /// Imports an entire module under its original name
    Simple { path: String },
    /// Aliased import (e.g., `use std.math as math`)
    /// Imports a module and gives it an alias
    Aliased { path: String, alias: String },
    /// Import specific symbol from module (e.g., `use sqrt in std.math`)
    /// Optionally with an alias (e.g., `use sqrt as root in std.math`)
    InModule { path: String, module: String, alias: Option<String> },
    /// Import symbol(s) from module with language specification
    /// - Single: `use printf in libc of c`
    /// - Multiple: `use printf, fprintf, exit in libc of c`
    /// - Module only: `use libc of c` (imports all common symbols)
    InModuleWithLang {
        paths: Vec<String>,  // Multiple symbols or single module name
        module: String,
        lang: String,
        alias: Option<String>,
    },
}

/// Parse an identifier for import statements
/// 
/// This internal function parses an identifier that can contain alphanumeric
/// characters, underscores, and dots. The dot is specifically allowed to support
/// module paths like "std.math" or "utils.string".
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, identifier))` - Successfully parsed identifier and remaining input
/// * `Err(nom::Err)` - If no valid identifier is found at the start of input
fn parse_identifier(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_' || c == '.')(input)
}

/// One selective-import item: a symbol name, or `*` for every exported function.
fn parse_import_item(input: &str) -> IResult<&str, &str> {
    alt((tag("*"), parse_identifier)).parse(input)
}

/// Parse an import statement with the 'in' syntax
/// 
/// This function handles import statements of the form:
/// - `use symbol in module` - imports specific symbol from module
/// - `use symbol as alias in module` - imports specific symbol with an alias
/// 
/// This syntax allows selective importing of specific functions, classes, or
/// other symbols from a module rather than importing the entire module.
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, Import::InModule))` - Successfully parsed import and remaining input
/// * `Err(nom::Err)` - If the input does not match the expected pattern
fn parse_import_in(input: &str) -> IResult<&str, Import> {
    let (input, _) = tag("use")(input)?;
    let (input, _) = multispace1(input)?;

    // Parse one or more identifiers separated by commas
    let (input, paths) = separated_list1(
        delimited(space0, tag(","), space0),
        parse_import_item,
    ).parse(input)?;

    let (input, _) = multispace0(input)?;

    // For backward compatibility, if only one symbol, use old InModule variant
    // If multiple symbols, we'll use InModuleWithLang with lang=""
    if paths.len() == 1 {
        let (input, alias) = opt(preceded(
            (tag("as"), multispace1),
            parse_identifier,
        ))
        .parse(input)?;

        let (input, _) = multispace0(input)?;
        let (input, _) = tag("in")(input)?;
        let (input, _) = multispace1(input)?;
        let (input, module) = parse_identifier(input)?;

        Ok((
            input,
            Import::InModule {
                path: paths[0].to_string(),
                module: module.to_string(),
                alias: alias.map(|s| s.to_string()),
            },
        ))
    } else {
        // Multiple symbols: use InModuleWithLang with empty language
        // This allows import resolver to handle it properly
        let (input, _) = tag("in")(input)?;
        let (input, _) = multispace1(input)?;
        let (input, module) = parse_identifier(input)?;

        Ok((
            input,
            Import::InModuleWithLang {
                paths: paths.iter().map(|s| s.to_string()).collect(),
                module: module.to_string(),
                lang: "".to_string(),
                alias: None,
            },
        ))
    }
}

/// Parse an aliased import statement
/// 
/// This function handles import statements of the form:
/// - `use module as alias` - imports a module and assigns it an alias
/// 
/// This syntax allows importing a module under a different name to avoid
/// naming conflicts or to provide shorter, more convenient names.
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, Import::Aliased))` - Successfully parsed import and remaining input
/// * `Err(nom::Err)` - If the input does not match the expected pattern
fn parse_import_as(input: &str) -> IResult<&str, Import> {
    let (input, _) = tag("use")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, path) = parse_identifier(input)?;
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("as")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, alias) = parse_identifier(input)?;

    Ok((
        input,
        Import::Aliased {
            path: path.to_string(),
            alias: alias.to_string(),
        },
    ))
}

/// Parse a simple import statement
/// 
/// This function handles import statements of the form:
/// - `use module` - imports an entire module
/// 
/// This is the most basic import syntax that brings in an entire module
/// under its original name.
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, Import::Simple))` - Successfully parsed import and remaining input
/// * `Err(nom::Err)` - If the input does not match the expected pattern
fn parse_import_simple(input: &str) -> IResult<&str, Import> {
    let (input, _) = tag("use")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, path) = parse_identifier(input)?;

    Ok((
        input,
        Import::Simple {
            path: path.to_string(),
        },
    ))
}

/// Parse import statements with language specification
/// 
/// This function handles import statements of the form:
/// - Single: `use printf in libc of c` - imports specific C function
/// - Multiple: `use printf, fprintf, exit in libc of c` - imports multiple C functions
/// - Empty: `use in libc of c` - imports all common symbols from C library
/// - Module only: `use libc of c` - imports all common symbols from C library
/// 
/// This syntax enables Coffee's foreign function interface (FFI), allowing
/// direct calls to functions from other languages (especially C). The 'of'
/// keyword specifies the target language for the import.
/// 
/// # Arguments
/// 
/// * `input` - The input string to parse
/// 
/// # Returns
/// 
/// * `Ok((remaining, Import::InModuleWithLang))` - Successfully parsed import and remaining input
/// * `Err(nom::Err)` - If the input does not match the expected pattern
fn parse_import_in_module_of_lang(input: &str) -> IResult<&str, Import> {
    let (input, _) = tag("use")(input)?;
    let (input, _) = multispace1(input)?;

    // Check if the next token is 'in' (empty path list)
    // If so, use empty paths
    // Otherwise, parse the path list
    let (input, paths) = if input.trim_start().starts_with("in") {
        (input, Vec::new())
    } else {
        let (input, paths) = separated_list1(
            delimited(space0, tag(","), space0),
            parse_identifier,
        ).parse(input)?;
        (input, paths)
    };

    let (input, _) = multispace0(input)?;

    let (input, _) = tag("in")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, module) = parse_identifier(input)?;
    let (input, _) = multispace0(input)?;

    let (input, _) = tag("of")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, lang) = parse_identifier(input)?;

    Ok((
        input,
        Import::InModuleWithLang {
            paths: paths.iter().map(|s| s.to_string()).collect(),
            module: module.to_string(),
            lang: lang.to_string(),
            alias: None,
        },
    ))
}

pub fn parse_import(input: &str) -> IResult<&str, Import> {
    // Order matters! More specific patterns must come first
    alt((
        parse_import_in_module_of_lang,  // use symbol in module of lang
        parse_import_in,                  // use symbol in module [as alias]
        parse_import_as,                  // use module as alias
        parse_import_simple,              // use module
    )).parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coffee_multi_import_is_not_c() {
        let (rest, import) = parse_import("use sys_malloc, sys_free in sys").unwrap();
        assert!(rest.trim().is_empty());
        match import {
            Import::InModuleWithLang { paths, module, lang, .. } => {
                assert_eq!(paths, vec!["sys_malloc", "sys_free"]);
                assert_eq!(module, "sys");
                assert!(lang.is_empty(), "Coffee multi-import must not look like of c");
            }
            other => panic!("expected InModuleWithLang, got {other:?}"),
        }
    }

    #[test]
    fn star_import_parses() {
        let (rest, import) = parse_import("use * in sys").unwrap();
        assert!(rest.trim().is_empty());
        match import {
            Import::InModule { path, module, alias } => {
                assert_eq!(path, "*");
                assert_eq!(module, "sys");
                assert!(alias.is_none());
            }
            other => panic!("expected InModule star, got {other:?}"),
        }
    }
}
