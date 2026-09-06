//! Lower parser AST → HIR (typed expr/stmt) → statement MIR.
//!
//! Type information comes from a callback so this module never calls `TypeChecker`.

mod expr;
mod helpers;
mod mir_builder;
mod stmts;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use crate::parser::expr::Expression;
use crate::parser::function::Function;
use crate::types::{Type, TypeRegistry};

pub use expr::lower_expr;
pub use mir_builder::{hir_stmts_to_mir, hir_stmts_to_mir_taken};

/// Lowers expressions/statements using a type-query callback (not `TypeChecker`).
pub struct HirLower<F> {
    pub(super) infer: F,
    /// Optional registry so enum match can type payload binds (`Option.Some(x)`).
    pub(super) registry: Option<Arc<RwLock<TypeRegistry>>>,
    /// Distinguishes `__forin_coll_*` temps when nested loops share a variable name.
    pub(super) for_in_temps: usize,
    /// Pattern-bind, loop-var, and body-`let` types while lowering a stmt list.
    pub(super) extra_types: HashMap<String, Type>,
    /// Top-level + already-lowered `hir_fns` names, for nested `fn` keys.
    pub reserved: HashSet<String>,
    /// Enclosing LLVM / `hir_fns` key while lowering a body (`unique_function_key`).
    pub(super) nested_parent: String,
    /// Names already used as nested `fn` keys in this lower pass.
    pub(super) nested_taken: HashSet<String>,
}

impl<F> HirLower<F>
where
    F: FnMut(&Expression) -> Result<Type, String>,
{
    pub fn new(infer: F) -> Self {
        HirLower {
            infer,
            registry: None,
            for_in_temps: 0,
            extra_types: HashMap::new(),
            reserved: HashSet::new(),
            nested_parent: String::new(),
            nested_taken: HashSet::new(),
        }
    }

    pub fn set_registry(&mut self, registry: Arc<RwLock<TypeRegistry>>) {
        self.registry = Some(registry);
    }
}

/// Lower a Coffee function to statement MIR (not SSA).
pub fn lower_function<F>(
    func: &Function,
    lower: &mut HirLower<F>,
) -> Result<crate::hir::mir::MirFn, String>
where
    F: FnMut(&Expression) -> Result<Type, String>,
{
    let taken = lower.reserved.clone();
    lower_function_keyed(func, lower, &func.name, taken)
}

/// Lower using the LLVM / `hir_fns` key as MIR parent so nested `fn` keys match collect.
pub fn lower_function_keyed<F>(
    func: &Function,
    lower: &mut HirLower<F>,
    hir_name: &str,
    taken: HashSet<String>,
) -> Result<crate::hir::mir::MirFn, String>
where
    F: FnMut(&Expression) -> Result<Type, String>,
{
    let saved_parent = std::mem::replace(&mut lower.nested_parent, hir_name.to_string());
    let saved_taken = std::mem::replace(&mut lower.nested_taken, taken.clone());
    let hir_body = lower.lower_function_body(func)?;
    lower.nested_parent = saved_parent;
    lower.nested_taken = saved_taken;
    let mut mir = hir_stmts_to_mir_taken(hir_name, &hir_body, taken);
    mir.param_names = func.parameters.iter().map(|p| p.name.clone()).collect();
    mir.param_types = func
        .parameters
        .iter()
        .map(|p| p.param_type.clone())
        .collect();
    mir.return_type = func.return_type.clone();
    if !mir.complete {
        return Err(format!(
            "incomplete MIR for '{}': unhandled statement",
            hir_name
        ));
    }
    Ok(mir)
}
