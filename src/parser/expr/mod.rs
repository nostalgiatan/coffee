// Copyright 2024 Coffee Language Contributors
// Licensed under the MIT License
//
// Expression types for the Coffee language

mod ast;
mod binary;
mod ident;
mod parse;
mod postfix;
mod primary;
mod scan;
mod unary;

pub use ast::Expression;
pub use parse::{is_valid_identifier, parse_expression, parse_expression_at};
pub(crate) use parse::assignment_rhs;
