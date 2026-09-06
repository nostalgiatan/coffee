use std::collections::HashSet;

use crate::hir::expr::HirExpr;
use crate::hir::mir::{MirBlock, MirFn, MirStmt};
use crate::hir::stmt::HirStmt;

use super::helpers::{range_for_incr, range_for_lt};

pub fn hir_stmts_to_mir(name: &str, stmts: &[HirStmt]) -> MirFn {
    hir_stmts_to_mir_taken(name, stmts, HashSet::new())
}

pub fn hir_stmts_to_mir_taken(name: &str, stmts: &[HirStmt], _taken: HashSet<String>) -> MirFn {
    let mut b = MirBuilder::new();
    let entry = b.current;
    let _end = b.lower_stmts(stmts, entry);
    MirFn {
        name: name.to_string(),
        blocks: b.blocks,
        complete: b.complete,
        param_names: Vec::new(),
        param_types: Vec::new(),
        return_type: String::new(),
    }
}

struct LoopCtx {
    /// `continue` target: while header, or range-`for` increment block.
    cont: usize,
    exit: usize,
}

struct MirBuilder {
    blocks: Vec<MirBlock>,
    current: usize,
    loops: Vec<LoopCtx>,
    complete: bool,
}

impl MirBuilder {
    fn new() -> Self {
        MirBuilder {
            blocks: vec![MirBlock::default()],
            current: 0,
            loops: Vec::new(),
            complete: true,
        }
    }

    fn new_block(&mut self) -> usize {
        self.blocks.push(MirBlock::default());
        self.blocks.len() - 1
    }

    fn push(&mut self, bb: usize, stmt: MirStmt) {
        if self.blocks[bb].has_terminator() {
            return;
        }
        self.blocks[bb].stmts.push(stmt);
    }

    fn lower_stmts(&mut self, stmts: &[HirStmt], mut bb: usize) -> usize {
        for stmt in stmts {
            bb = self.lower_stmt(stmt, bb);
        }
        bb
    }

    fn lower_stmt(&mut self, stmt: &HirStmt, bb: usize) -> usize {
        match stmt {
            HirStmt::Let { name, value, .. } | HirStmt::Assign { name, value } => {
                self.push(
                    bb,
                    MirStmt::Assign {
                        name: name.clone(),
                        value: value.clone(),
                    },
                );
                bb
            }
            HirStmt::Expr(e) => {
                self.push(bb, MirStmt::Expr(e.clone()));
                bb
            }
            HirStmt::Return(v) => {
                self.push(bb, MirStmt::Return(v.clone()));
                self.new_block()
            }
            HirStmt::If {
                cond,
                then_body,
                elifs,
                else_body,
            } => self.lower_if(cond, then_body, elifs, else_body.as_deref(), bb),
            HirStmt::While { cond, body } => self.lower_while(cond, body, bb),
            HirStmt::ForRange {
                var,
                start,
                end,
                body,
            } => self.lower_for_range(var, start, end, body, bb),
            HirStmt::Break => {
                if let Some(ctx) = self.loops.last() {
                    let exit = ctx.exit;
                    self.push(bb, MirStmt::Goto(exit));
                    self.new_block()
                } else {
                    bb
                }
            }
            HirStmt::Continue => {
                if let Some(ctx) = self.loops.last() {
                    let cont = ctx.cont;
                    self.push(bb, MirStmt::Goto(cont));
                    self.new_block()
                } else {
                    bb
                }
            }
            HirStmt::Scope { body } => self.lower_stmts(body, bb),
            HirStmt::MemoryOp(op) => {
                self.push(bb, MirStmt::MemoryOp(op.clone()));
                bb
            }
            HirStmt::Raise(r) => {
                self.push(bb, MirStmt::Raise(r.clone()));
                bb
            }
            HirStmt::Nested(decl) => {
                if decl.name.is_empty() {
                    self.complete = false;
                } else {
                    self.push(bb, MirStmt::Nested(decl.clone()));
                }
                bb
            }
            // comment/import: skip. Nested class/fn is `HirStmt::Nested`,
            // already pushed above. Other kinds must not silently drop — they
            // are a lowering bug (`complete = false` so codegen errors).
            HirStmt::Unsupported { kind } => {
                if *kind != "comment" && *kind != "import" {
                    self.complete = false;
                }
                bb
            }
        }
    }

    fn lower_if(
        &mut self,
        cond: &HirExpr,
        then_body: &[HirStmt],
        elifs: &[(HirExpr, Vec<HirStmt>)],
        else_body: Option<&[HirStmt]>,
        start: usize,
    ) -> usize {
        let join = self.new_block();
        self.lower_if_chain(cond, then_body, elifs, else_body, start, join);
        join
    }

    fn lower_if_chain(
        &mut self,
        cond: &HirExpr,
        then_body: &[HirStmt],
        elifs: &[(HirExpr, Vec<HirStmt>)],
        else_body: Option<&[HirStmt]>,
        start: usize,
        join: usize,
    ) {
        let then_bb = self.new_block();
        let else_bb = if elifs.is_empty() && else_body.is_none() {
            join
        } else {
            self.new_block()
        };
        self.push(
            start,
            MirStmt::Branch {
                cond: cond.clone(),
                then_bb,
                else_bb,
            },
        );

        self.push(then_bb, MirStmt::ScopeEnter);
        let then_end = self.lower_stmts(then_body, then_bb);
        self.push(then_end, MirStmt::ScopeExit);
        self.push(then_end, MirStmt::Goto(join));

        if let Some(((elif_cond, elif_body), rest)) = elifs.split_first() {
            self.lower_if_chain(elif_cond, elif_body, rest, else_body, else_bb, join);
        } else if let Some(else_stmts) = else_body {
            self.push(else_bb, MirStmt::ScopeEnter);
            let else_end = self.lower_stmts(else_stmts, else_bb);
            self.push(else_end, MirStmt::ScopeExit);
            self.push(else_end, MirStmt::Goto(join));
        }
    }

    fn lower_while(&mut self, cond: &HirExpr, body: &[HirStmt], bb: usize) -> usize {
        let header = if self.blocks[bb].stmts.is_empty() && !self.blocks[bb].has_terminator() {
            bb
        } else {
            let h = self.new_block();
            self.push(bb, MirStmt::Goto(h));
            h
        };
        let body_bb = self.new_block();
        let exit = self.new_block();
        self.push(
            header,
            MirStmt::Branch {
                cond: cond.clone(),
                then_bb: body_bb,
                else_bb: exit,
            },
        );
        self.loops.push(LoopCtx { cont: header, exit });
        self.push(body_bb, MirStmt::ScopeEnter);
        let body_end = self.lower_stmts(body, body_bb);
        self.push(body_end, MirStmt::ScopeExit);
        self.push(body_end, MirStmt::Goto(header));
        self.loops.pop();
        exit
    }

    fn lower_for_range(
        &mut self,
        var: &str,
        start: &HirExpr,
        end: &HirExpr,
        body: &[HirStmt],
        bb: usize,
    ) -> usize {
        self.push(
            bb,
            MirStmt::Assign {
                name: var.to_string(),
                value: start.clone(),
            },
        );
        let header = self.new_block();
        self.push(bb, MirStmt::Goto(header));
        let body_bb = self.new_block();
        let incr = self.new_block();
        let join = self.new_block();
        self.push(
            header,
            MirStmt::Branch {
                cond: range_for_lt(var, end),
                then_bb: body_bb,
                else_bb: join,
            },
        );
        self.loops.push(LoopCtx {
            cont: incr,
            exit: join,
        });
        self.push(body_bb, MirStmt::ScopeEnter);
        let body_end = self.lower_stmts(body, body_bb);
        self.push(body_end, MirStmt::ScopeExit);
        self.push(body_end, MirStmt::Goto(incr));
        self.loops.pop();
        self.push(
            incr,
            MirStmt::Assign {
                name: var.to_string(),
                value: range_for_incr(var),
            },
        );
        self.push(incr, MirStmt::Goto(header));
        // Nested loops allocate blocks after this join; MIR codegen walks
        // blocks by index, so the join (and stmts after the for) must be last.
        self.move_block_to_end(join)
    }

    fn move_block_to_end(&mut self, bb: usize) -> usize {
        let end = self.new_block();
        if bb == end {
            return bb;
        }
        self.blocks.swap(bb, end);
        self.retarget_blocks(bb, end);
        end
    }

    fn retarget_blocks(&mut self, a: usize, b: usize) {
        for block in &mut self.blocks {
            for stmt in &mut block.stmts {
                match stmt {
                    MirStmt::Goto(t) => *t = swap_bb(*t, a, b),
                    MirStmt::Branch {
                        then_bb, else_bb, ..
                    } => {
                        *then_bb = swap_bb(*then_bb, a, b);
                        *else_bb = swap_bb(*else_bb, a, b);
                    }
                    _ => {}
                }
            }
        }
        for ctx in &mut self.loops {
            ctx.cont = swap_bb(ctx.cont, a, b);
            ctx.exit = swap_bb(ctx.exit, a, b);
        }
    }
}

fn swap_bb(t: usize, a: usize, b: usize) -> usize {
    if t == a {
        b
    } else if t == b {
        a
    } else {
        t
    }
}
