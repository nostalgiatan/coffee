//! SSA MIR: unique assignment names and φ-nodes at CFG joins.
//!
//! Converted from statement [`MirFn`] via [`to_ssa`]. Not consumed by LLVM yet.

use std::collections::{BTreeSet, HashMap};

use super::expr::HirExpr;
use super::mir::{MirFn, MirStmt, NestedDecl};
use crate::parser::MemoryOp;

#[derive(Debug, Clone, PartialEq)]
pub struct SsaFn {
    pub name: String,
    pub blocks: Vec<SsaBlock>,
    pub complete: bool,
    pub param_names: Vec<String>,
    pub param_types: Vec<String>,
    pub return_type: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SsaBlock {
    pub stmts: Vec<SsaStmt>,
}

/// SSA statement list. Joins use [`SsaStmt::Phi`]; names on [`SsaStmt::Assign`] / phi dests are unique.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaStmt {
    Phi {
        dest: String,
        incomings: Vec<(usize, String)>,
    },
    Assign {
        name: String,
        value: HirExpr,
    },
    Expr(HirExpr),
    Goto(usize),
    Branch {
        cond: HirExpr,
        then_bb: usize,
        else_bb: usize,
    },
    Return(Option<HirExpr>),
    Raise(HirExpr),
    Nested(NestedDecl),
    ScopeEnter,
    ScopeExit,
    MemoryOp(MemoryOp),
}

pub fn to_ssa(mir: &MirFn) -> SsaFn {
    let n = mir.blocks.len();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, b) in mir.blocks.iter().enumerate() {
        for s in b.successors() {
            if s < n {
                preds[s].push(i);
            }
        }
    }

    let mut counters: HashMap<String, usize> = HashMap::new();
    let mut end_names: Vec<HashMap<String, String>> = vec![HashMap::new(); n];
    let mut blocks: Vec<SsaBlock> = vec![SsaBlock::default(); n];
    let entry_params: HashMap<String, String> = mir
        .param_names
        .iter()
        .map(|p| (p.clone(), p.clone()))
        .collect();

    for bb in mir.codegen_order() {
        let mut current = if bb == 0 && preds[bb].is_empty() {
            entry_params.clone()
        } else {
            merge_pred_names(&preds[bb], &end_names)
        };
        let mut stmts = Vec::new();
        for (orig, phi) in phis_at_join(&preds[bb], &end_names, &mut counters) {
            if let SsaStmt::Phi { dest, .. } = &phi {
                current.insert(orig, dest.clone());
            }
            stmts.push(phi);
        }
        for stmt in &mir.blocks[bb].stmts {
            stmts.push(rewrite_stmt(stmt, &mut current, &mut counters));
        }
        end_names[bb] = current;
        blocks[bb] = SsaBlock { stmts };
    }

    SsaFn {
        name: mir.name.clone(),
        blocks,
        complete: mir.complete,
        param_names: mir.param_names.clone(),
        param_types: mir.param_types.clone(),
        return_type: mir.return_type.clone(),
    }
}

fn fresh(counters: &mut HashMap<String, usize>, orig: &str) -> String {
    let n = counters.entry(orig.to_string()).or_insert(0);
    let name = format!("{orig}{n}");
    *n += 1;
    name
}

fn lookup(current: &HashMap<String, String>, orig: &str) -> String {
    current
        .get(orig)
        .cloned()
        .unwrap_or_else(|| orig.to_string())
}

fn merge_pred_names(
    preds: &[usize],
    end_names: &[HashMap<String, String>],
) -> HashMap<String, String> {
    if preds.is_empty() {
        return HashMap::new();
    }
    if preds.len() == 1 {
        return end_names[preds[0]].clone();
    }
    let mut origs = BTreeSet::new();
    for &p in preds {
        origs.extend(end_names[p].keys().cloned());
    }
    let mut out = HashMap::new();
    for orig in origs {
        let names: Vec<String> = preds
            .iter()
            .filter_map(|p| end_names[*p].get(&orig).cloned())
            .collect();
        if names.len() == preds.len() && names.windows(2).all(|w| w[0] == w[1]) {
            out.insert(orig, names[0].clone());
        }
    }
    out
}

fn phis_at_join(
    preds: &[usize],
    end_names: &[HashMap<String, String>],
    counters: &mut HashMap<String, usize>,
) -> Vec<(String, SsaStmt)> {
    if preds.len() < 2 {
        return Vec::new();
    }
    let mut origs: BTreeSet<String> = BTreeSet::new();
    for &p in preds {
        origs.extend(end_names[p].keys().cloned());
    }
    let mut phis = Vec::new();
    for orig in origs {
        let mut incomings = Vec::new();
        let mut all_same: Option<String> = None;
        let mut differ = false;
        let mut missing = false;
        for &p in preds {
            match end_names[p].get(&orig) {
                Some(name) => {
                    match &all_same {
                        None => all_same = Some(name.clone()),
                        Some(prev) if prev != name => differ = true,
                        _ => {}
                    }
                    incomings.push((p, name.clone()));
                }
                None => missing = true,
            }
        }
        if missing || !differ || incomings.len() != preds.len() {
            continue;
        }
        let dest = fresh(counters, &orig);
        phis.push((orig, SsaStmt::Phi { dest, incomings }));
    }
    phis
}

fn rewrite_stmt(
    stmt: &MirStmt,
    current: &mut HashMap<String, String>,
    counters: &mut HashMap<String, usize>,
) -> SsaStmt {
    match stmt {
        MirStmt::Assign { name, value } => {
            let value = rewrite_expr(value, current);
            let dest = fresh(counters, name);
            current.insert(name.clone(), dest.clone());
            SsaStmt::Assign { name: dest, value }
        }
        MirStmt::Expr(e) => SsaStmt::Expr(rewrite_expr(e, current)),
        MirStmt::Goto(t) => SsaStmt::Goto(*t),
        MirStmt::Branch {
            cond,
            then_bb,
            else_bb,
        } => SsaStmt::Branch {
            cond: rewrite_expr(cond, current),
            then_bb: *then_bb,
            else_bb: *else_bb,
        },
        MirStmt::Return(v) => SsaStmt::Return(v.as_ref().map(|e| rewrite_expr(e, current))),
        MirStmt::Raise(e) => SsaStmt::Raise(rewrite_expr(e, current)),
        MirStmt::Nested(d) => SsaStmt::Nested(d.clone()),
        MirStmt::ScopeEnter => SsaStmt::ScopeEnter,
        MirStmt::ScopeExit => SsaStmt::ScopeExit,
        MirStmt::MemoryOp(op) => SsaStmt::MemoryOp(rewrite_memory_op(op, current, counters)),
    }
}

fn rewrite_memory_op(
    op: &MemoryOp,
    current: &mut HashMap<String, String>,
    counters: &mut HashMap<String, usize>,
) -> MemoryOp {
    match op {
        MemoryOp::Clone { source, target } => {
            let source = lookup(current, source);
            let dest = fresh(counters, target);
            current.insert(target.clone(), dest.clone());
            MemoryOp::Clone {
                source,
                target: dest,
            }
        }
        MemoryOp::Copy { source, target } => {
            let source = lookup(current, source);
            let dest = fresh(counters, target);
            current.insert(target.clone(), dest.clone());
            MemoryOp::Copy {
                source,
                target: dest,
            }
        }
        MemoryOp::Move { source, target } => {
            let source = lookup(current, source);
            let dest = fresh(counters, target);
            current.insert(target.clone(), dest.clone());
            MemoryOp::Move {
                source,
                target: dest,
            }
        }
        MemoryOp::Remove { target } => MemoryOp::Remove {
            target: lookup(current, target),
        },
        MemoryOp::RemoveMultiple { targets } => MemoryOp::RemoveMultiple {
            targets: targets.iter().map(|t| lookup(current, t)).collect(),
        },
        MemoryOp::CleanOut {
            targets,
            except_mode,
        } => MemoryOp::CleanOut {
            targets: targets
                .as_ref()
                .map(|ts| ts.iter().map(|t| lookup(current, t)).collect()),
            except_mode: *except_mode,
        },
    }
}

fn rewrite_expr(expr: &HirExpr, current: &HashMap<String, String>) -> HirExpr {
    use super::expr::HirExprKind;
    let kind = match &expr.kind {
        HirExprKind::Variable(n) => HirExprKind::Variable(lookup(current, n)),
        HirExprKind::Literal(v) => HirExprKind::Literal(v.clone()),
        HirExprKind::FString {
            template,
            placeholders,
        } => HirExprKind::FString {
            template: template.clone(),
            placeholders: placeholders
                .iter()
                .map(|p| lookup(current, p))
                .collect(),
        },
        HirExprKind::Binary { left, op, right } => HirExprKind::Binary {
            left: Box::new(rewrite_expr(left, current)),
            op: op.clone(),
            right: Box::new(rewrite_expr(right, current)),
        },
        HirExprKind::Unary { op, operand } => HirExprKind::Unary {
            op: op.clone(),
            operand: Box::new(rewrite_expr(operand, current)),
        },
        HirExprKind::Call { function, args } => HirExprKind::Call {
            function: Box::new(rewrite_expr(function, current)),
            args: args.iter().map(|a| rewrite_expr(a, current)).collect(),
        },
        HirExprKind::ConstructorCall { class_name, args } => HirExprKind::ConstructorCall {
            class_name: class_name.clone(),
            args: args.iter().map(|a| rewrite_expr(a, current)).collect(),
        },
        HirExprKind::Member {
            object,
            field,
            args,
        } => HirExprKind::Member {
            object: Box::new(rewrite_expr(object, current)),
            field: field.clone(),
            args: args.iter().map(|a| rewrite_expr(a, current)).collect(),
        },
        HirExprKind::Index { array, index } => HirExprKind::Index {
            array: Box::new(rewrite_expr(array, current)),
            index: Box::new(rewrite_expr(index, current)),
        },
        HirExprKind::TupleField { tuple, index } => HirExprKind::TupleField {
            tuple: Box::new(rewrite_expr(tuple, current)),
            index: *index,
        },
        HirExprKind::EnumTag { value, enum_name } => HirExprKind::EnumTag {
            value: Box::new(rewrite_expr(value, current)),
            enum_name: enum_name.clone(),
        },
        HirExprKind::EnumPayload {
            value,
            enum_name,
            variant,
            index,
        } => HirExprKind::EnumPayload {
            value: Box::new(rewrite_expr(value, current)),
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            index: *index,
        },
        HirExprKind::Len { collection } => HirExprKind::Len {
            collection: Box::new(rewrite_expr(collection, current)),
        },
        HirExprKind::ArrayLiteral { elements } => HirExprKind::ArrayLiteral {
            elements: elements.iter().map(|e| rewrite_expr(e, current)).collect(),
        },
        HirExprKind::TupleLiteral { elements } => HirExprKind::TupleLiteral {
            elements: elements.iter().map(|e| rewrite_expr(e, current)).collect(),
        },
        HirExprKind::StructLiteral { struct_name, fields } => HirExprKind::StructLiteral {
            struct_name: struct_name.clone(),
            fields: fields
                .iter()
                .map(|(n, e)| (n.clone(), rewrite_expr(e, current)))
                .collect(),
        },
        HirExprKind::Assign {
            object,
            field_name,
            value,
        } => HirExprKind::Assign {
            object: Box::new(rewrite_expr(object, current)),
            field_name: field_name.clone(),
            value: Box::new(rewrite_expr(value, current)),
        },
        HirExprKind::TypeCast { target_type, value } => HirExprKind::TypeCast {
            target_type: target_type.clone(),
            value: Box::new(rewrite_expr(value, current)),
        },
        HirExprKind::AnonymousFunction { func } => HirExprKind::AnonymousFunction {
            func: func.clone(),
        },
    };
    HirExpr {
        ty: expr.ty.clone(),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::expr::{HirExpr, HirExprKind};
    use crate::hir::mir::{MirBlock, MirFn, MirStmt};
    use crate::types::Type;

    fn lit(n: &str) -> HirExpr {
        HirExpr {
            ty: Type::int(),
            kind: HirExprKind::Literal(n.into()),
        }
    }

    fn var(name: &str) -> HirExpr {
        HirExpr {
            ty: Type::int(),
            kind: HirExprKind::Variable(name.into()),
        }
    }

    /// Diamond: bb0 branches to bb1/bb2; both assign `x`; bb3 is the join.
    fn diamond_if_assign_x_both_sides() -> MirFn {
        MirFn {
            name: "f".into(),
            complete: true,
            param_names: vec!["c".into()],
            param_types: vec!["int".into()],
            return_type: "int".into(),
            blocks: vec![
                MirBlock {
                    stmts: vec![MirStmt::Branch {
                        cond: var("c"),
                        then_bb: 1,
                        else_bb: 2,
                    }],
                },
                MirBlock {
                    stmts: vec![
                        MirStmt::Assign {
                            name: "x".into(),
                            value: lit("1"),
                        },
                        MirStmt::Goto(3),
                    ],
                },
                MirBlock {
                    stmts: vec![
                        MirStmt::Assign {
                            name: "x".into(),
                            value: lit("0"),
                        },
                        MirStmt::Goto(3),
                    ],
                },
                MirBlock {
                    stmts: vec![MirStmt::Return(Some(var("x")))],
                },
            ],
        }
    }

    #[test]
    fn diamond_if_both_sides_assign_x_gets_one_phi() {
        let ssa = to_ssa(&diamond_if_assign_x_both_sides());
        let phis: Vec<_> = ssa
            .blocks
            .iter()
            .enumerate()
            .flat_map(|(bb, b)| {
                b.stmts.iter().filter_map(move |s| match s {
                    SsaStmt::Phi { dest, incomings } => Some((bb, dest.clone(), incomings.clone())),
                    _ => None,
                })
            })
            .collect();
        assert_eq!(
            phis.len(),
            1,
            "join of both-branch assigns must be one phi, got {:?}",
            ssa
        );
        let (join, dest, incomings) = &phis[0];
        assert_eq!(*join, 3, "phi belongs in the join block");
        assert_eq!(incomings.len(), 2);
        let preds: Vec<usize> = incomings.iter().map(|(bb, _)| *bb).collect();
        assert!(preds.contains(&1) && preds.contains(&2), "{:?}", incomings);
        assert_ne!(incomings[0].1, incomings[1].1, "incoming SSA names must differ");
        assert!(
            dest.starts_with('x'),
            "phi dest should be a unique rename of x, got {dest}"
        );

        let assign_names: Vec<&str> = ssa
            .blocks
            .iter()
            .flat_map(|b| b.stmts.iter())
            .filter_map(|s| match s {
                SsaStmt::Assign { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(assign_names.len(), 2);
        assert_ne!(assign_names[0], assign_names[1]);
        assert!(assign_names.iter().all(|n| n.starts_with('x')));
    }

    #[test]
    fn diamond_if_join_return_uses_phi_dest_not_original_x() {
        let ssa = to_ssa(&diamond_if_assign_x_both_sides());
        let join = &ssa.blocks[3];
        let dest = join
            .stmts
            .iter()
            .find_map(|s| match s {
                SsaStmt::Phi { dest, .. } => Some(dest.as_str()),
                _ => None,
            })
            .expect("join must have a phi for x");
        assert_ne!(dest, "x", "phi dest should be a rename of x, got {dest}");

        let ret = join
            .stmts
            .iter()
            .find_map(|s| match s {
                SsaStmt::Return(Some(e)) => Some(e),
                _ => None,
            })
            .expect("join must return a value");

        match &ret.kind {
            HirExprKind::Variable(n) => {
                assert_eq!(
                    n.as_str(),
                    dest,
                    "Return must use the phi dest after SSA rename, got {n} (phi dest {dest})"
                );
                assert_ne!(
                    n.as_str(),
                    "x",
                    "Return must not keep HirExprKind::Variable(\"x\") when x was renamed to {dest}"
                );
            }
            other => panic!("expected Return of Variable, got {other:?}"),
        }
    }
}
