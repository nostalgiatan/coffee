//! Conservative last-use analysis for implicit resource moves.
//!
//! A name is last-use only when we can prove no later use on any remaining
//! path. False negatives keep today's `mv` / copy errors.

use crate::parser::expr::Expression;
use crate::parser::function::{Function, FunctionBody};
use crate::parser::memory::MemoryOp;
use crate::parser::{ForIterator, Statement};
use crate::types::definition::Type;
use std::collections::{HashMap, HashSet};

/// `(statement id, name)` sites that may implicit-move.
pub type LastUseSites = HashSet<(usize, String)>;

/// Owned resource (not a borrow). `Type::Ref` is excluded even though
/// [`Type::is_resource`] is true for it.
pub fn is_owned_resource(ty: &Type) -> bool {
    ty.is_resource() && !matches!(ty, Type::Ref { .. })
}

/// Innermost `Variable` name, if this expression is only a simple variable.
pub fn simple_var_name(expr: &Expression) -> Option<&str> {
    match expr.kind() {
        Expression::Variable(name) => Some(name.as_str()),
        _ => None,
    }
}

/// Last-use sites for a function body. Nested `fn` / `class` / anonymous `fn`
/// bodies are ignored (checked with their own analysis when those callables run).
pub fn analyze_function_body(func: &Function) -> LastUseSites {
    match &func.body {
        FunctionBody::Block(stmts) => analyze_stmts(stmts),
        FunctionBody::Expression(expr) => analyze_expr_body(expr),
        FunctionBody::External => LastUseSites::new(),
    }
}

pub fn analyze_stmts(stmts: &[Statement]) -> LastUseSites {
    let mut a = Analyzer {
        next_id: 0,
        sites: LastUseSites::new(),
    };
    a.walk_seq(stmts, &HashSet::new(), false);
    a.sites
}

fn analyze_expr_body(expr: &Expression) -> LastUseSites {
    let mut a = Analyzer {
        next_id: 0,
        sites: LastUseSites::new(),
    };
    let uses = expr_uses(expr);
    let mut counts = HashMap::new();
    count_simple_vars(expr, &mut counts);
    let mut cands = HashSet::new();
    collect_call_arg_simples(expr, &mut cands);
    if let Some(n) = simple_var_name(expr) {
        cands.insert(n.to_string());
    }
    record_candidates(0, &cands, &counts, &HashSet::new(), false, true, &mut a.sites);
    let _ = uses;
    a.sites
}

struct Analyzer {
    next_id: usize,
    sites: LastUseSites,
}

impl Analyzer {
    fn walk_seq(&mut self, stmts: &[Statement], used_after: &HashSet<String>, in_loop: bool) {
        let mut suffix = used_after.clone();
        let mut after_each = Vec::with_capacity(stmts.len());
        for stmt in stmts.iter().rev() {
            after_each.push(suffix.clone());
            suffix = union_owned(suffix, stmt_uses(stmt));
        }
        after_each.reverse();
        for (stmt, after) in stmts.iter().zip(after_each.into_iter()) {
            self.walk_stmt(stmt, &after, in_loop);
        }
    }

    fn walk_stmt(&mut self, stmt: &Statement, used_after: &HashSet<String>, in_loop: bool) {
        let id = self.next_id;
        self.next_id += 1;

        let header_loop = matches!(stmt, Statement::While(_) | Statement::For(_));
        let is_return = matches!(stmt, Statement::Return(_));
        self.record_stmt_sites(id, stmt, used_after, in_loop || header_loop, is_return);

        match stmt {
            Statement::If(if_expr) => {
                let else_uses = if_expr
                    .else_body
                    .as_ref()
                    .map(|b| stmts_uses(b))
                    .unwrap_or_default();
                let elif_uses: HashSet<String> = if_expr
                    .elifs
                    .iter()
                    .flat_map(|e| {
                        let mut u = expr_uses(&e.condition);
                        u.extend(stmts_uses(&e.body));
                        u
                    })
                    .collect();
                let then_after = union3(used_after, &else_uses, &elif_uses);
                self.walk_seq(&if_expr.body, &then_after, in_loop);

                let then_uses = stmts_uses(&if_expr.body);
                for (i, elif) in if_expr.elifs.iter().enumerate() {
                    let other_elifs: HashSet<String> = if_expr
                        .elifs
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .flat_map(|(_, e)| {
                            let mut u = expr_uses(&e.condition);
                            u.extend(stmts_uses(&e.body));
                            u
                        })
                        .collect();
                    let elif_after = union3(used_after, &then_uses, &else_uses);
                    let elif_after = union_owned(elif_after, other_elifs);
                    self.walk_seq(&elif.body, &elif_after, in_loop);
                }
                if let Some(else_body) = &if_expr.else_body {
                    let else_after = union_owned(union_ref(used_after, &then_uses), elif_uses);
                    self.walk_seq(else_body, &else_after, in_loop);
                }
            }
            Statement::While(w) => {
                self.walk_seq(&w.body, used_after, true);
            }
            Statement::For(f) => {
                self.walk_seq(&f.body, used_after, true);
            }
            Statement::Match(m) => {
                let arm_uses: Vec<HashSet<String>> = m
                    .arms
                    .iter()
                    .map(|arm| {
                        let mut u = stmts_uses(&arm.body);
                        if let Some(g) = &arm.guard {
                            u.extend(expr_uses(g));
                        }
                        u
                    })
                    .collect();
                for (i, arm) in m.arms.iter().enumerate() {
                    let mut after = used_after.clone();
                    for (j, u) in arm_uses.iter().enumerate() {
                        if i != j {
                            after.extend(u.iter().cloned());
                        }
                    }
                    self.walk_seq(&arm.body, &after, in_loop);
                }
            }
            Statement::Function(_)
            | Statement::Class(_)
            | Statement::Enum(_)
            | Statement::Main(_) => {}
            _ => {}
        }
    }

    fn record_stmt_sites(
        &mut self,
        id: usize,
        stmt: &Statement,
        used_after: &HashSet<String>,
        in_loop: bool,
        is_return: bool,
    ) {
        let mut counts = HashMap::new();
        count_stmt_simple_vars(stmt, &mut counts);
        let cands = stmt_move_candidates(stmt);
        record_candidates(
            id,
            &cands,
            &counts,
            used_after,
            in_loop,
            is_return,
            &mut self.sites,
        );
    }
}

fn record_candidates(
    id: usize,
    cands: &HashSet<String>,
    counts: &HashMap<String, usize>,
    used_after: &HashSet<String>,
    in_loop: bool,
    is_return: bool,
    sites: &mut LastUseSites,
) {
    if in_loop && !is_return {
        return;
    }
    for name in cands {
        if used_after.contains(name) {
            continue;
        }
        if counts.get(name).copied().unwrap_or(0) > 1 {
            continue;
        }
        sites.insert((id, name.clone()));
    }
}

fn union_ref(a: &HashSet<String>, b: &HashSet<String>) -> HashSet<String> {
    a.union(b).cloned().collect()
}

fn union3(a: &HashSet<String>, b: &HashSet<String>, c: &HashSet<String>) -> HashSet<String> {
    let mut o = union_ref(a, b);
    o.extend(c.iter().cloned());
    o
}

fn union_owned(mut a: HashSet<String>, b: HashSet<String>) -> HashSet<String> {
    a.extend(b);
    a
}

fn stmts_uses(stmts: &[Statement]) -> HashSet<String> {
    let mut u = HashSet::new();
    for s in stmts {
        u.extend(stmt_uses(s));
    }
    u
}

fn stmt_uses(stmt: &Statement) -> HashSet<String> {
    let mut u = HashSet::new();
    match stmt {
        Statement::VariableDecl(d) => {
            u.extend(expr_uses(&d.value));
        }
        Statement::Assignment(name, value) => {
            if let Some(root) = name.split('.').next() {
                u.insert(root.to_string());
            }
            u.extend(expr_uses(value));
        }
        Statement::Return(r) => {
            if let Some(v) = &r.value {
                u.extend(expr_uses(v));
            }
        }
        Statement::Expr(e) => u.extend(expr_uses(e)),
        Statement::Raise(r) => u.extend(expr_uses(&r.error_expr)),
        Statement::If(i) => {
            u.extend(expr_uses(&i.condition));
            u.extend(stmts_uses(&i.body));
            for e in &i.elifs {
                u.extend(expr_uses(&e.condition));
                u.extend(stmts_uses(&e.body));
            }
            if let Some(b) = &i.else_body {
                u.extend(stmts_uses(b));
            }
        }
        Statement::While(w) => {
            u.extend(expr_uses(&w.condition));
            u.extend(stmts_uses(&w.body));
        }
        Statement::For(f) => {
            match &f.iterator {
                ForIterator::Range { start, end } => {
                    u.extend(expr_uses(start));
                    u.extend(expr_uses(end));
                }
                ForIterator::Collection(e) => u.extend(expr_uses(e)),
            }
            u.extend(stmts_uses(&f.body));
        }
        Statement::Match(m) => {
            u.extend(expr_uses(&m.value));
            for arm in &m.arms {
                if let Some(g) = &arm.guard {
                    u.extend(expr_uses(g));
                }
                u.extend(stmts_uses(&arm.body));
            }
        }
        Statement::MemoryOp(op) => match op {
            MemoryOp::Move { source, .. }
            | MemoryOp::Clone { source, .. }
            | MemoryOp::Copy { source, .. } => {
                u.insert(source.clone());
            }
            MemoryOp::Remove { target } => {
                u.insert(target.clone());
            }
            MemoryOp::RemoveMultiple { targets } => {
                u.extend(targets.iter().cloned());
            }
            MemoryOp::CleanOut { targets, .. } => {
                if let Some(ts) = targets {
                    u.extend(ts.iter().cloned());
                }
            }
        },
        Statement::Function(f) => {
            // Nested function: conservative uses from its body (minus params).
            let mut inner = match &f.body {
                FunctionBody::Block(s) => stmts_uses(s),
                FunctionBody::Expression(e) => expr_uses(e),
                FunctionBody::External => HashSet::new(),
            };
            for p in &f.parameters {
                inner.remove(&p.name);
            }
            u.extend(inner);
        }
        Statement::Class(_)
        | Statement::Enum(_)
        | Statement::Main(_)
        | Statement::Import(_)
        | Statement::TypeDecl(_)
        | Statement::Break(_)
        | Statement::Continue(_)
        | Statement::SingleLineComment(_)
        | Statement::MultiLineComment(_) => {}
    }
    u
}

fn stmt_move_candidates(stmt: &Statement) -> HashSet<String> {
    let mut c = HashSet::new();
    match stmt {
        Statement::VariableDecl(d) => {
            if let Some(n) = simple_var_name(&d.value) {
                c.insert(n.to_string());
            }
            collect_call_arg_simples(&d.value, &mut c);
        }
        Statement::Assignment(_, value) => {
            if let Some(n) = simple_var_name(value) {
                c.insert(n.to_string());
            }
            collect_call_arg_simples(value, &mut c);
        }
        Statement::Return(r) => {
            if let Some(v) = &r.value {
                if let Some(n) = simple_var_name(v) {
                    c.insert(n.to_string());
                }
                collect_call_arg_simples(v, &mut c);
            }
        }
        Statement::Expr(e) => {
            collect_call_arg_simples(e, &mut c);
            if let Some(n) = simple_var_name(e) {
                c.insert(n.to_string());
            }
        }
        Statement::Raise(r) => {
            collect_call_arg_simples(&r.error_expr, &mut c);
            if let Some(n) = simple_var_name(&r.error_expr) {
                c.insert(n.to_string());
            }
        }
        Statement::If(i) => {
            collect_call_arg_simples(&i.condition, &mut c);
            for e in &i.elifs {
                collect_call_arg_simples(&e.condition, &mut c);
            }
        }
        Statement::While(w) => collect_call_arg_simples(&w.condition, &mut c),
        Statement::For(f) => match &f.iterator {
            ForIterator::Range { start, end } => {
                collect_call_arg_simples(start, &mut c);
                collect_call_arg_simples(end, &mut c);
            }
            ForIterator::Collection(e) => collect_call_arg_simples(e, &mut c),
        },
        Statement::Match(m) => collect_call_arg_simples(&m.value, &mut c),
        _ => {}
    }
    c
}

fn expr_uses(expr: &Expression) -> HashSet<String> {
    let mut u = HashSet::new();
    walk_expr_uses(expr, &mut u);
    u
}

fn walk_expr_uses(expr: &Expression, u: &mut HashSet<String>) {
    match expr.kind() {
        Expression::Variable(n) => {
            u.insert(n.clone());
        }
        Expression::FString { placeholders, .. } => {
            u.extend(placeholders.iter().cloned());
        }
        Expression::Binary { left, right, .. } => {
            walk_expr_uses(left, u);
            walk_expr_uses(right, u);
        }
        Expression::Unary { operand, .. } => walk_expr_uses(operand, u),
        Expression::Call { function, args } => {
            walk_expr_uses(function, u);
            for a in args {
                walk_expr_uses(a, u);
            }
        }
        Expression::ConstructorCall { args, .. } => {
            for a in args {
                walk_expr_uses(a, u);
            }
        }
        Expression::Member { object, args, .. } => {
            walk_expr_uses(object, u);
            for a in args {
                walk_expr_uses(a, u);
            }
        }
        Expression::Index { array, index } => {
            walk_expr_uses(array, u);
            walk_expr_uses(index, u);
        }
        Expression::ArrayLiteral { elements } | Expression::TupleLiteral { elements } => {
            for e in elements {
                walk_expr_uses(e, u);
            }
        }
        Expression::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                walk_expr_uses(v, u);
            }
        }
        Expression::Assign { object, value, .. } => {
            walk_expr_uses(object, u);
            walk_expr_uses(value, u);
        }
        Expression::TypeCast { value, .. } => walk_expr_uses(value, u),
        Expression::AnonymousFunction { .. } => {}
        Expression::Literal(_) => {}
        Expression::Spanned { .. } => unreachable!("kind() peels Spanned"),
    }
}

fn collect_call_arg_simples(expr: &Expression, c: &mut HashSet<String>) {
    match expr.kind() {
        Expression::Call { function, args } => {
            collect_call_arg_simples(function, c);
            for a in args {
                if let Some(n) = simple_var_name(a) {
                    c.insert(n.to_string());
                }
                collect_call_arg_simples(a, c);
            }
        }
        Expression::ConstructorCall { args, .. } => {
            for a in args {
                if let Some(n) = simple_var_name(a) {
                    c.insert(n.to_string());
                }
                collect_call_arg_simples(a, c);
            }
        }
        Expression::Member { object, args, .. } => {
            collect_call_arg_simples(object, c);
            for a in args {
                if let Some(n) = simple_var_name(a) {
                    c.insert(n.to_string());
                }
                collect_call_arg_simples(a, c);
            }
        }
        Expression::Binary { left, right, .. } => {
            collect_call_arg_simples(left, c);
            collect_call_arg_simples(right, c);
        }
        Expression::Unary { operand, .. } => collect_call_arg_simples(operand, c),
        Expression::Index { array, index } => {
            collect_call_arg_simples(array, c);
            collect_call_arg_simples(index, c);
        }
        Expression::ArrayLiteral { elements } | Expression::TupleLiteral { elements } => {
            for e in elements {
                collect_call_arg_simples(e, c);
            }
        }
        Expression::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                collect_call_arg_simples(v, c);
            }
        }
        Expression::Assign { object, value, .. } => {
            collect_call_arg_simples(object, c);
            collect_call_arg_simples(value, c);
        }
        Expression::TypeCast { value, .. } => collect_call_arg_simples(value, c),
        Expression::AnonymousFunction { .. } => {}
        _ => {}
    }
}

fn count_simple_vars(expr: &Expression, counts: &mut HashMap<String, usize>) {
    match expr.kind() {
        Expression::Variable(n) => {
            *counts.entry(n.clone()).or_insert(0) += 1;
        }
        Expression::FString { placeholders, .. } => {
            for p in placeholders {
                *counts.entry(p.clone()).or_insert(0) += 1;
            }
        }
        Expression::Binary { left, right, .. } => {
            count_simple_vars(left, counts);
            count_simple_vars(right, counts);
        }
        Expression::Unary { operand, .. } => count_simple_vars(operand, counts),
        Expression::Call { function, args } => {
            count_simple_vars(function, counts);
            for a in args {
                count_simple_vars(a, counts);
            }
        }
        Expression::ConstructorCall { args, .. } => {
            for a in args {
                count_simple_vars(a, counts);
            }
        }
        Expression::Member { object, args, .. } => {
            count_simple_vars(object, counts);
            for a in args {
                count_simple_vars(a, counts);
            }
        }
        Expression::Index { array, index } => {
            count_simple_vars(array, counts);
            count_simple_vars(index, counts);
        }
        Expression::ArrayLiteral { elements } | Expression::TupleLiteral { elements } => {
            for e in elements {
                count_simple_vars(e, counts);
            }
        }
        Expression::StructLiteral { fields, .. } => {
            for (_, v) in fields {
                count_simple_vars(v, counts);
            }
        }
        Expression::Assign { object, value, .. } => {
            count_simple_vars(object, counts);
            count_simple_vars(value, counts);
        }
        Expression::TypeCast { value, .. } => count_simple_vars(value, counts),
        Expression::AnonymousFunction { .. } => {}
        Expression::Literal(_) | Expression::Spanned { .. } => {}
    }
}

fn count_stmt_simple_vars(stmt: &Statement, counts: &mut HashMap<String, usize>) {
    match stmt {
        Statement::VariableDecl(d) => count_simple_vars(&d.value, counts),
        Statement::Assignment(_, v) => count_simple_vars(v, counts),
        Statement::Return(r) => {
            if let Some(v) = &r.value {
                count_simple_vars(v, counts);
            }
        }
        Statement::Expr(e) => count_simple_vars(e, counts),
        Statement::Raise(r) => count_simple_vars(&r.error_expr, counts),
        Statement::If(i) => {
            count_simple_vars(&i.condition, counts);
            for e in &i.elifs {
                count_simple_vars(&e.condition, counts);
            }
        }
        Statement::While(w) => count_simple_vars(&w.condition, counts),
        Statement::For(f) => match &f.iterator {
            ForIterator::Range { start, end } => {
                count_simple_vars(start, counts);
                count_simple_vars(end, counts);
            }
            ForIterator::Collection(e) => count_simple_vars(e, counts),
        },
        Statement::Match(m) => count_simple_vars(&m.value, counts),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_program, FunctionBody, Statement};

    fn fn_body(src: &str) -> Vec<Statement> {
        let program = parse_program(src).expect("parse");
        let mut last = None;
        for s in program.statements {
            if let Statement::Function(f) = s {
                if let FunctionBody::Block(b) = f.body {
                    last = Some(b);
                }
            }
        }
        last.expect("no fn block")
    }

    fn names_at(sites: &LastUseSites, name: &str) -> Vec<usize> {
        let mut ids: Vec<usize> = sites
            .iter()
            .filter(|(_, n)| n == name)
            .map(|(id, _)| *id)
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn let_simple_var_is_last_use_when_unused_after() {
        let body = fn_body(
            r#"
class Box:
    n: int

fn f() => int:
    let a: Box = Box { n: 1 }
    let b: Box = a
    return 0
"#,
        );
        let sites = analyze_stmts(&body);
        assert!(
            !names_at(&sites, "a").is_empty(),
            "expected last-use of a, got {sites:?}"
        );
    }

    #[test]
    fn used_after_is_not_last_use() {
        let body = fn_body(
            r#"
class Box:
    n: int

fn f() => int:
    let a: Box = Box { n: 1 }
    let b: Box = a
    rm a
    return 0
"#,
        );
        let sites = analyze_stmts(&body);
        assert!(
            names_at(&sites, "a").is_empty(),
            "a is used after let, got {sites:?}"
        );
    }

    #[test]
    fn then_branch_not_last_if_else_uses() {
        let body = fn_body(
            r#"
class Box:
    n: int

fn f(c: bool) => int:
    let a: Box = Box { n: 1 }
    if c:
        let b: Box = a
    else:
        rm a
    return 0
"#,
        );
        let sites = analyze_stmts(&body);
        assert!(
            names_at(&sites, "a").is_empty(),
            "else uses a, then let must not last-use, got {sites:?}"
        );
    }

    #[test]
    fn loop_body_let_is_not_last_use() {
        let body = fn_body(
            r#"
class Box:
    n: int

fn f() => int:
    let a: Box = Box { n: 1 }
    while true:
        let b: Box = a
    return 0
"#,
        );
        let sites = analyze_stmts(&body);
        assert!(
            names_at(&sites, "a").is_empty(),
            "loop let is not last-use, got {sites:?}"
        );
    }

    #[test]
    fn return_in_loop_can_be_last_use() {
        let body = fn_body(
            r#"
class Box:
    n: int

fn f() => Box:
    let a: Box = Box { n: 1 }
    while true:
        return a
"#,
        );
        let sites = analyze_stmts(&body);
        assert!(
            !names_at(&sites, "a").is_empty(),
            "return a in loop should last-use, got {sites:?}"
        );
    }

    #[test]
    fn call_arg_last_use() {
        let body = fn_body(
            r#"
class Box:
    n: int

fn take(p: Box) => int:
    return p.n

fn f() => int:
    let a: Box = Box { n: 1 }
    take(a)
    return 0
"#,
        );
        let sites = analyze_stmts(&body);
        assert!(
            !names_at(&sites, "a").is_empty(),
            "take(a) should last-use a, got {sites:?}"
        );
    }
}
