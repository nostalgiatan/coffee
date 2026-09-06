//! Typed HIR expressions: parser `Expression` with a `Type` on every node.

use crate::types::Type;

/// Typed expression after a successful type query for this node.
#[derive(Debug, Clone, PartialEq)]
pub struct HirExpr {
    pub ty: Type,
    pub kind: HirExprKind,
}

/// Mirrors [`crate::parser::expr::Expression`], with nested [`HirExpr`] instead of untyped nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum HirExprKind {
    Literal(String),
    Variable(String),
    FString {
        template: String,
        placeholders: Vec<String>,
    },
    Binary {
        left: Box<HirExpr>,
        op: String,
        right: Box<HirExpr>,
    },
    Unary {
        op: String,
        operand: Box<HirExpr>,
    },
    Call {
        function: Box<HirExpr>,
        args: Vec<HirExpr>,
    },
    ConstructorCall {
        class_name: String,
        args: Vec<HirExpr>,
    },
    Member {
        object: Box<HirExpr>,
        field: String,
        args: Vec<HirExpr>,
    },
    Index {
        array: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    /// Tuple element `t.N` (not array index). Used when lowering tuple `match`.
    TupleField {
        tuple: Box<HirExpr>,
        index: usize,
    },
    /// Discriminant of a named enum (`Color.Red` / `Option.Some`). i64 tag.
    EnumTag {
        value: Box<HirExpr>,
        enum_name: String,
    },
    /// Payload field `index` of `enum_name.variant` (LLVM field `index + 1` after the tag).
    EnumPayload {
        value: Box<HirExpr>,
        enum_name: String,
        variant: String,
        index: usize,
    },
    /// Runtime collection length (`array_lengths` at codegen). Slice `for-in`.
    Len {
        collection: Box<HirExpr>,
    },
    ArrayLiteral {
        elements: Vec<HirExpr>,
    },
    TupleLiteral {
        elements: Vec<HirExpr>,
    },
    StructLiteral {
        struct_name: String,
        fields: Vec<(String, HirExpr)>,
    },
    Assign {
        object: Box<HirExpr>,
        field_name: String,
        value: Box<HirExpr>,
    },
    TypeCast {
        target_type: String,
        value: Box<HirExpr>,
    },
    AnonymousFunction {
        func: Box<crate::parser::function::Function>,
    },
}

/// Drop types and rebuild a parser [`Expression`] so existing AST codegen can run.
#[cfg(test)]
pub fn hir_expr_to_ast(expr: &HirExpr) -> crate::parser::expr::Expression {
    use crate::parser::expr::Expression;
    match &expr.kind {
        HirExprKind::Literal(v) => Expression::Literal(v.clone()),
        HirExprKind::Variable(v) => Expression::Variable(v.clone()),
        HirExprKind::FString {
            template,
            placeholders,
        } => Expression::FString {
            template: template.clone(),
            placeholders: placeholders.clone(),
        },
        HirExprKind::Binary { left, op, right } => Expression::Binary {
            left: Box::new(hir_expr_to_ast(left)),
            op: op.clone(),
            right: Box::new(hir_expr_to_ast(right)),
        },
        HirExprKind::Unary { op, operand } => Expression::Unary {
            op: op.clone(),
            operand: Box::new(hir_expr_to_ast(operand)),
        },
        HirExprKind::Call { function, args } => Expression::Call {
            function: Box::new(hir_expr_to_ast(function)),
            args: args.iter().map(hir_expr_to_ast).collect(),
        },
        HirExprKind::ConstructorCall { class_name, args } => Expression::ConstructorCall {
            class_name: class_name.clone(),
            args: args.iter().map(hir_expr_to_ast).collect(),
        },
        HirExprKind::Member { object, field, args } => Expression::Member {
            object: Box::new(hir_expr_to_ast(object)),
            field: field.clone(),
            args: args.iter().map(hir_expr_to_ast).collect(),
        },
        HirExprKind::Index { array, index } => Expression::Index {
            array: Box::new(hir_expr_to_ast(array)),
            index: Box::new(hir_expr_to_ast(index)),
        },
        HirExprKind::TupleField { tuple, index } => Expression::Index {
            array: Box::new(hir_expr_to_ast(tuple)),
            index: Box::new(Expression::Literal(index.to_string())),
        },
        HirExprKind::EnumTag { value, .. } => hir_expr_to_ast(value),
        HirExprKind::EnumPayload {
            value,
            index,
            ..
        } => Expression::Index {
            array: Box::new(hir_expr_to_ast(value)),
            index: Box::new(Expression::Literal((index + 1).to_string())),
        },
        HirExprKind::Len { collection } => Expression::Member {
            object: Box::new(hir_expr_to_ast(collection)),
            field: "len".into(),
            args: vec![],
        },
        HirExprKind::ArrayLiteral { elements } => Expression::ArrayLiteral {
            elements: elements.iter().map(hir_expr_to_ast).collect(),
        },
        HirExprKind::TupleLiteral { elements } => Expression::TupleLiteral {
            elements: elements.iter().map(hir_expr_to_ast).collect(),
        },
        HirExprKind::StructLiteral { struct_name, fields } => Expression::StructLiteral {
            struct_name: struct_name.clone(),
            fields: fields
                .iter()
                .map(|(n, e)| (n.clone(), hir_expr_to_ast(e)))
                .collect(),
        },
        HirExprKind::Assign {
            object,
            field_name,
            value,
        } => Expression::Assign {
            object: Box::new(hir_expr_to_ast(object)),
            field_name: field_name.clone(),
            value: Box::new(hir_expr_to_ast(value)),
        },
        HirExprKind::TypeCast { target_type, value } => Expression::TypeCast {
            target_type: target_type.clone(),
            value: Box::new(hir_expr_to_ast(value)),
        },
        HirExprKind::AnonymousFunction { func } => Expression::AnonymousFunction {
            func: func.clone(),
        },
    }
}
