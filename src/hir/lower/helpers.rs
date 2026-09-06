use crate::hir::expr::{HirExpr, HirExprKind};
use crate::types::Type;

pub(super) fn int_var(name: &str) -> HirExpr {
    HirExpr {
        ty: Type::int(),
        kind: HirExprKind::Variable(name.to_string()),
    }
}

pub(super) fn range_for_lt(var: &str, end: &HirExpr) -> HirExpr {
    HirExpr {
        ty: Type::bool(),
        kind: HirExprKind::Binary {
            left: Box::new(int_var(var)),
            op: "<".to_string(),
            right: Box::new(end.clone()),
        },
    }
}

pub(super) fn range_for_incr(var: &str) -> HirExpr {
    HirExpr {
        ty: Type::int(),
        kind: HirExprKind::Binary {
            left: Box::new(int_var(var)),
            op: "+".to_string(),
            right: Box::new(HirExpr {
                ty: Type::int(),
                kind: HirExprKind::Literal("1".into()),
            }),
        },
    }
}
