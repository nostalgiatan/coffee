//! Control-flow facts the checker needs about a function body.

use crate::parser::expr::Expression;
use crate::parser::Statement;

/// True when every path through `stmts` ends in `return` or `raise`.
///
/// `match` is exhaustive by the time this runs (`check_match_exhaustiveness`), so a
/// `match` whose arms all end the path ends it too. `while true` with no `break` never
/// falls through.
pub(super) fn always_returns(stmts: &[Statement]) -> bool {
    stmts.iter().any(ends_path)
}

fn ends_path(stmt: &Statement) -> bool {
    match stmt {
        Statement::Return(_) | Statement::Raise(_) => true,
        Statement::If(if_expr) => {
            let Some(else_body) = &if_expr.else_body else {
                return false;
            };
            always_returns(&if_expr.body)
                && if_expr.elifs.iter().all(|elif| always_returns(&elif.body))
                && always_returns(else_body)
        }
        Statement::Match(match_expr) => {
            !match_expr.arms.is_empty()
                && match_expr.arms.iter().all(|arm| always_returns(&arm.body))
        }
        Statement::While(while_loop) => {
            is_literal_true(&while_loop.condition) && !contains_break(&while_loop.body)
        }
        _ => false,
    }
}

fn is_literal_true(expr: &Expression) -> bool {
    match expr {
        Expression::Spanned { inner, .. } => is_literal_true(inner),
        Expression::Literal(value) => value == "true",
        _ => false,
    }
}

/// Any `break` inside `stmts`, including nested loops (conservative).
fn contains_break(stmts: &[Statement]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Statement::Break(_) => true,
        Statement::If(if_expr) => {
            contains_break(&if_expr.body)
                || if_expr.elifs.iter().any(|elif| contains_break(&elif.body))
                || if_expr.else_body.as_deref().is_some_and(contains_break)
        }
        Statement::While(while_loop) => contains_break(&while_loop.body),
        Statement::For(for_loop) => contains_break(&for_loop.body),
        Statement::Match(match_expr) => match_expr.arms.iter().any(|arm| contains_break(&arm.body)),
        _ => false,
    })
}
