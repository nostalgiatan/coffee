//! Intra-procedural borrow checker (variable, field, and index places).
//!
//! Rules (Coffee subset of Rust):
//! - Many shared (`&`) loans on a place, or one exclusive (`&mut`) loan, never both.
//! - Overlapping places (`p` vs `p.x` / `a` vs `a[i]`) conflict the same way (conservative).
//! - Move, `rm`, and assignment to a place are forbidden while any overlapping loan is live.
//! - Using a place as a value is forbidden while it is mutably borrowed.
//! - Named `&`/`&mut` loans expire after the last use of that binding in the remainder of
//!   the function (or at end of the `if`/`while` body that declared it).
//! - Temporary loans (`foo(&x)`) end at the end of the statement, unless the
//!   callee's return type is `Type::Ref`. If the callee is a known function
//!   whose body always `return`s one parameter (`return p` / `return &p` /
//!   `return &mut p`), only that argument's place is pinned. Otherwise every
//!   unheld `&`/`&mut` arg is pinned (opaque / C / fn-pointer / mixed returns).
//!   Pinned loans last until last use of the result binding, or function end if
//!   the call is discarded. No lifetime parameters `'a`.
//! - Returning a reference to a non-parameter local is a lifetime error.

use super::definition::Span;
use super::errors::TypeSystemError;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// A borrowable location: a local, a field chain (`p.x.y`), or an index (`a[i]`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Place {
    Var(String),
    Field {
        base: Box<Place>,
        field: String,
        /// All members of a `c union` overlay the same storage.
        union_overlay: bool,
    },
    Index { base: Box<Place> },
}

impl Place {
    pub fn var(name: impl Into<String>) -> Self {
        Place::Var(name.into())
    }

    pub fn field(self, field: impl Into<String>) -> Self {
        Place::Field {
            base: Box::new(self),
            field: field.into(),
            union_overlay: false,
        }
    }

    pub fn union_field(self, field: impl Into<String>) -> Self {
        Place::Field {
            base: Box::new(self),
            field: field.into(),
            union_overlay: true,
        }
    }

    pub fn index(self) -> Self {
        Place::Index {
            base: Box::new(self),
        }
    }

    pub fn root_var(&self) -> &str {
        match self {
            Place::Var(name) => name,
            Place::Field { base, .. } | Place::Index { base } => base.root_var(),
        }
    }

    pub fn components(&self) -> Vec<&str> {
        match self {
            Place::Var(name) => vec![name.as_str()],
            Place::Field { base, field, .. } => {
                let mut parts = base.components();
                parts.push(field.as_str());
                parts
            }
            Place::Index { base } => {
                let mut parts = base.components();
                parts.push("[]");
                parts
            }
        }
    }

    pub fn display(&self) -> String {
        match self {
            Place::Var(name) => name.clone(),
            Place::Field { base, field, .. } => format!("{}.{}", base.display(), field),
            Place::Index { base } => format!("{}[]", base.display()),
        }
    }

    fn has_union_overlay(&self) -> bool {
        match self {
            Place::Field {
                union_overlay: true,
                ..
            } => true,
            Place::Field { base, .. } | Place::Index { base } => base.has_union_overlay(),
            Place::Var(_) => false,
        }
    }

    /// True if one place is the other or a field path under it (`p` overlaps `p.x`).
    /// Sibling fields of a `c union` also overlap.
    pub fn overlaps(&self, other: &Place) -> bool {
        if self.root_var() == other.root_var()
            && self.has_union_overlay()
            && other.has_union_overlay()
        {
            return true;
        }
        let a = self.components();
        let b = other.components();
        a.starts_with(&b) || b.starts_with(&a)
    }
}

impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display())
    }
}

/// Parameter index uniquely returned as `&param` / `&mut param` / the param itself.
///
/// Walks `return` expressions (and an expression-body). `Some(i)` only when every
/// seen return names the same parameter. `None` if the body is missing, empty of
/// returns, returns a non-param, returns mixed params, or is otherwise opaque.
pub fn alias_param_index(func: &crate::parser::function::Function) -> Option<usize> {
    let names: Vec<&str> = func.parameters.iter().map(|p| p.name.as_str()).collect();
    let mut found: Option<usize> = None;
    match &func.body {
        crate::parser::function::FunctionBody::External => return None,
        crate::parser::function::FunctionBody::Expression(expr) => {
            note_returned_param(&names, expr, &mut found)?;
        }
        crate::parser::function::FunctionBody::Block(stmts) => {
            collect_returned_param(&names, stmts, &mut found)?;
        }
    }
    found
}

fn collect_returned_param(
    names: &[&str],
    stmts: &[crate::parser::Statement],
    found: &mut Option<usize>,
) -> Option<()> {
    for stmt in stmts {
        match stmt {
            crate::parser::Statement::Return(ret) => {
                let expr = ret.value.as_ref()?;
                note_returned_param(names, expr, found)?;
            }
            crate::parser::Statement::If(if_expr) => {
                collect_returned_param(names, &if_expr.body, found)?;
                for elif in &if_expr.elifs {
                    collect_returned_param(names, &elif.body, found)?;
                }
                if let Some(else_body) = &if_expr.else_body {
                    collect_returned_param(names, else_body, found)?;
                }
            }
            crate::parser::Statement::While(w) => {
                collect_returned_param(names, &w.body, found)?;
            }
            crate::parser::Statement::For(f) => {
                collect_returned_param(names, &f.body, found)?;
            }
            crate::parser::Statement::Match(m) => {
                for arm in &m.arms {
                    collect_returned_param(names, &arm.body, found)?;
                }
            }
            crate::parser::Statement::Function(_) => {}
            _ => {}
        }
    }
    Some(())
}

fn note_returned_param(
    names: &[&str],
    expr: &crate::parser::expr::Expression,
    found: &mut Option<usize>,
) -> Option<()> {
    let param = returned_param_name(expr)?;
    let idx = names.iter().position(|n| *n == param)?;
    match *found {
        None => *found = Some(idx),
        Some(i) if i == idx => {}
        Some(_) => return None,
    }
    Some(())
}

fn returned_param_name(expr: &crate::parser::expr::Expression) -> Option<&str> {
    use crate::parser::expr::Expression;
    match expr.kind() {
        Expression::Variable(n) => Some(n.as_str()),
        Expression::Unary { op, operand } if op == "&" || op == "&mut" => match operand.kind() {
            Expression::Variable(n) => Some(n.as_str()),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct Loan {
    place: Place,
    mutable: bool,
    holder: Option<String>,
    /// Unheld temp that must not expire at statement end (Ref-returning call).
    pinned: bool,
}

#[derive(Debug, Clone, Default)]
pub struct BorrowCx {
    loans: Vec<Loan>,
    params: HashSet<String>,
    scope_names: Vec<Vec<String>>,
    /// Names used as values during the current statement (`deny_use_while_mut`).
    used_this_stmt: RefCell<HashSet<String>>,
    /// Named loans that already expired by last-use (holder, borrowed place).
    ended_loans: Vec<(String, Place)>,
    /// Holders whose referent was written/exclusively re-borrowed after their loan ended.
    poisoned: RefCell<HashMap<String, String>>,
}

impl BorrowCx {
    pub fn set_params(&mut self, params: impl IntoIterator<Item = String>) {
        self.params = params.into_iter().collect();
        self.loans.clear();
        self.scope_names = vec![Vec::new()];
        self.ended_loans.clear();
        self.used_this_stmt.borrow_mut().clear();
        self.poisoned.borrow_mut().clear();
    }

    pub fn is_param(&self, name: &str) -> bool {
        self.params.contains(name)
    }

    pub fn enter_scope(&mut self) {
        self.scope_names.push(Vec::new());
    }

    pub fn note_binding(&mut self, name: &str) {
        if let Some(names) = self.scope_names.last_mut() {
            names.push(name.to_string());
        }
    }

    /// Drop loans held by names declared in the current scope. Returns those names.
    pub fn exit_scope(&mut self) -> Vec<String> {
        let names = self.scope_names.pop().unwrap_or_default();
        self.loans.retain(|loan| match &loan.holder {
            Some(h) => !names.iter().any(|n| n == h),
            None => true,
        });
        names
    }

    pub fn expire_temps(&mut self) {
        let used = self.used_this_stmt.borrow().clone();
        let mut ended = Vec::new();
        self.loans.retain(|loan| match &loan.holder {
            None => loan.pinned,
            Some(h) if used.contains(h) => {
                ended.push((h.clone(), loan.place.clone()));
                false
            }
            Some(_) => true,
        });
        self.ended_loans.extend(ended);
        self.used_this_stmt.borrow_mut().clear();
    }

    /// Keep unheld temps past this statement (callee returns `Ref`).
    pub fn pin_unheld_temps(&mut self) {
        for loan in &mut self.loans {
            if loan.holder.is_none() {
                loan.pinned = true;
            }
        }
    }

    /// Pin only unheld temps that overlap `place` (known callee returned that param).
    pub fn pin_unheld_temps_of(&mut self, place: &Place) {
        for loan in &mut self.loans {
            if loan.holder.is_none() && loan.place.overlaps(place) {
                loan.pinned = true;
            }
        }
    }

    /// Attach pinned unheld temps to a result binding (last-use like named loans).
    pub fn claim_unheld_for(&mut self, holder: &str) {
        for loan in &mut self.loans {
            if loan.holder.is_none() && loan.pinned {
                loan.holder = Some(holder.to_string());
                loan.pinned = false;
            }
        }
    }

    fn poison_ended_overlapping(&self, place: &Place, reason: &str) {
        let mut poisoned = self.poisoned.borrow_mut();
        for (holder, ended_place) in &self.ended_loans {
            if ended_place.overlaps(place) {
                poisoned.insert(holder.clone(), reason.to_string());
            }
        }
    }

    pub fn drop_holder(&mut self, holder: &str) {
        self.loans.retain(|loan| loan.holder.as_deref() != Some(holder));
    }

    pub fn acquire_place(&mut self, place: &Place, mutable: bool) -> Result<(), TypeSystemError> {
        for loan in &self.loans {
            if !loan.place.overlaps(place) {
                continue;
            }
            if mutable || loan.mutable || loan.place.has_union_overlay() || place.has_union_overlay() {
                let shown = place.display();
                return Err(TypeSystemError::BorrowConflict {
                    variable: shown.clone(),
                    reason: format!(
                        "cannot borrow '{}' {} because it is already borrowed",
                        shown,
                        if mutable { "mutably" } else { "immutably" }
                    ),
                    span: Span::new(0, shown.len()),
                });
            }
        }
        self.loans.push(Loan {
            place: place.clone(),
            mutable,
            holder: None,
            pinned: false,
        });
        if mutable {
            self.poison_ended_overlapping(place, "already borrowed");
        }
        Ok(())
    }

    pub fn claim_last_for(&mut self, holder: &str) {
        if let Some(loan) = self.loans.last_mut() {
            if loan.holder.is_none() {
                loan.holder = Some(holder.to_string());
            }
        }
    }

    pub fn copy_loan_to(&mut self, from_holder: &str, to_holder: &str) -> Result<(), TypeSystemError> {
        let src = self
            .loans
            .iter()
            .find(|l| l.holder.as_deref() == Some(from_holder))
            .cloned();
        if let Some(src) = src {
            self.acquire_place(&src.place, src.mutable)?;
            self.claim_last_for(to_holder);
        }
        Ok(())
    }

    /// Root local of the loan held by `holder` (for escape / param checks).
    pub fn place_of_holder(&self, holder: &str) -> Option<&str> {
        self.loans
            .iter()
            .find(|l| l.holder.as_deref() == Some(holder))
            .map(|l| l.place.root_var())
    }

    pub fn has_any_loan(&self, place: &str) -> bool {
        self.has_any_loan_place(&Place::var(place))
    }

    pub fn has_any_loan_place(&self, place: &Place) -> bool {
        self.loans.iter().any(|l| l.place.overlaps(place))
    }

    pub fn has_mut_loan(&self, place: &str) -> bool {
        self.has_mut_loan_place(&Place::var(place))
    }

    pub fn has_mut_loan_place(&self, place: &Place) -> bool {
        self.loans
            .iter()
            .any(|l| l.place.overlaps(place) && l.mutable)
    }

    pub fn deny_move(&self, place: &str) -> Result<(), TypeSystemError> {
        if self.has_any_loan(place) {
            return Err(TypeSystemError::BorrowViolation {
                variable: place.to_string(),
                reason: format!("cannot move '{}' while borrowed", place),
                span: Span::new(0, place.len()),
            });
        }
        Ok(())
    }

    pub fn deny_mutate(&self, place: &str) -> Result<(), TypeSystemError> {
        if self.has_any_loan(place) {
            return Err(TypeSystemError::BorrowViolation {
                variable: place.to_string(),
                reason: format!("cannot assign to '{}' while borrowed", place),
                span: Span::new(0, place.len()),
            });
        }
        self.poison_ended_overlapping(
            &Place::var(place),
            &format!("cannot assign to '{}' while borrowed", place),
        );
        Ok(())
    }

    pub fn deny_use_while_mut(&self, place: &str) -> Result<(), TypeSystemError> {
        self.used_this_stmt.borrow_mut().insert(place.to_string());
        if let Some(reason) = self.poisoned.borrow().get(place).cloned() {
            return Err(TypeSystemError::BorrowViolation {
                variable: place.to_string(),
                reason,
                span: Span::new(0, place.len()),
            });
        }
        if self.has_mut_loan(place) {
            return Err(TypeSystemError::BorrowConflict {
                variable: place.to_string(),
                reason: format!("cannot use '{}' while mutably borrowed", place),
                span: Span::new(0, place.len()),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_overlaps_prefix() {
        let p = Place::var("p");
        let px = Place::var("p").field("x");
        let py = Place::var("p").field("y");
        let q = Place::var("q");
        assert!(p.overlaps(&px));
        assert!(px.overlaps(&p));
        assert!(!px.overlaps(&py));
        assert!(!p.overlaps(&q));
    }

    #[test]
    fn shared_field_loans_ok_exclusive_conflicts() {
        let mut cx = BorrowCx::default();
        let px = Place::var("p").field("x");
        cx.acquire_place(&px, false).unwrap();
        cx.acquire_place(&px, false).unwrap();
        assert!(cx.acquire_place(&Place::var("p"), true).is_err());
        assert!(cx.acquire_place(&px, true).is_err());
    }

    #[test]
    fn field_loan_blocks_move_of_root() {
        let mut cx = BorrowCx::default();
        cx.acquire_place(&Place::var("p").field("x"), false).unwrap();
        assert!(cx.deny_move("p").is_err());
        assert!(cx.deny_mutate("p").is_err());
    }

    #[test]
    fn place_index_overlaps_array() {
        let a = Place::var("a");
        let ai = Place::var("a").index();
        let aj = Place::var("a").index();
        let b = Place::var("b").index();
        assert!(a.overlaps(&ai));
        assert!(ai.overlaps(&a));
        assert!(ai.overlaps(&aj));
        assert!(!ai.overlaps(&b));
    }

    #[test]
    fn pin_unheld_temps_survive_expire_until_claimed_last_use() {
        let mut cx = BorrowCx::default();
        cx.acquire_place(&Place::var("x"), false).unwrap();
        cx.pin_unheld_temps();
        cx.expire_temps();
        assert!(cx.deny_move("x").is_err());
        cx.claim_unheld_for("r");
        cx.deny_use_while_mut("r").unwrap();
        cx.expire_temps();
        assert!(cx.deny_move("x").is_ok());
    }

    #[test]
    fn pin_unheld_temps_of_only_that_place() {
        let mut cx = BorrowCx::default();
        cx.acquire_place(&Place::var("a"), false).unwrap();
        cx.acquire_place(&Place::var("b"), false).unwrap();
        cx.pin_unheld_temps_of(&Place::var("a"));
        cx.expire_temps();
        assert!(cx.deny_move("a").is_err());
        assert!(cx.deny_move("b").is_ok());
    }
}
