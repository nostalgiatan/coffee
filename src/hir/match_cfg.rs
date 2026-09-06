//! Lower `match` to [`HirStmt::If`] when arms can be expressed as equality
//! tests and field binds.
//!
//! Covers int / bool / float / `str` (including class-field scrutinees), tuple
//! patterns, class `Struct` patterns, shallow enum variants (`Color.Red`,
//! `Option.Some(x)`), and nested enum payloads (`Result.Success(Option.Some(x))`)
//! via [`HirExprKind::EnumTag`] / [`HirExprKind::EnumPayload`]. Ident / wildcard
//! (and Or of those) lower on any scrutinee type, including Function / Void /
//! Variadic. If `match_is_simple` is false (Function / Void / Variadic with
//! mixed literal / enum / struct / tuple arms, or a pattern that does not fit
//! the scrutinee type), lowering returns `Err` so the function is omitted from
//! `hir_fns` (codegen error: missing MIR). `compile_statement` Match already
//! errors if hit. Complete MIR never contains leftover match.

use std::sync::{Arc, RwLock};

use crate::parser::pattern::Pattern;
use crate::parser::r#match::MatchExpr;
use crate::types::definition::{TypeDef, VariantField};
use crate::types::{Type, TypeRegistry};

use super::expr::{HirExpr, HirExprKind};
use super::stmt::HirStmt;

fn is_integer_literal(s: &str) -> bool {
    s.parse::<i64>().is_ok() || s.parse::<u64>().is_ok()
}

fn literal_fits_ty(lit: &str, ty: &Type) -> bool {
    if ty.is_int() {
        is_integer_literal(lit)
    } else if *ty == Type::bool() {
        lit == "true" || lit == "false"
    } else if ty.is_float() {
        lit.parse::<f64>().is_ok()
    } else if *ty == Type::string() {
        lit.len() >= 2 && lit.starts_with('"') && lit.ends_with('"')
    } else {
        false
    }
}

fn literal_ty(lit: &str) -> Option<Type> {
    if lit == "true" || lit == "false" {
        Some(Type::bool())
    } else if lit.len() >= 2 && lit.starts_with('"') && lit.ends_with('"') {
        Some(Type::string())
    } else if is_integer_literal(lit) {
        Some(Type::int())
    } else if lit.parse::<f64>().is_ok() {
        Some(Type::float())
    } else {
        None
    }
}

fn pattern_is_catchall(p: &Pattern) -> bool {
    match p {
        Pattern::Wildcard | Pattern::Ident(_) => true,
        Pattern::Or(alts) => !alts.is_empty() && alts.iter().all(pattern_is_catchall),
        _ => false,
    }
}

fn pattern_is_lowerable(p: &Pattern, ty: &Type) -> bool {
    match p {
        Pattern::Wildcard | Pattern::Ident(_) => true,
        Pattern::Literal(lit) => literal_fits_ty(lit, ty),
        Pattern::Or(alts) => !alts.is_empty() && alts.iter().all(|a| pattern_is_lowerable(a, ty)),
        Pattern::Tuple(elems) => match ty {
            Type::Tuple(tys) if tys.len() == elems.len() => elems
                .iter()
                .zip(tys.iter())
                .all(|(e, t)| pattern_is_lowerable(e, t)),
            _ => false,
        },
        Pattern::Struct { fields, .. } => match ty {
            Type::NamedType { .. } => fields
                .iter()
                .all(|(_, fp)| pattern_is_lowerable_aggregate(fp)),
            _ => false,
        },
        Pattern::EnumVariant { args, .. } => match ty {
            Type::NamedType { .. } => args.iter().all(pattern_is_lowerable_aggregate),
            _ => false,
        },
    }
}

fn pattern_is_lowerable_aggregate(p: &Pattern) -> bool {
    match p {
        Pattern::Wildcard | Pattern::Ident(_) | Pattern::Literal(_) => true,
        Pattern::Or(alts) => {
            !alts.is_empty() && alts.iter().all(pattern_is_lowerable_aggregate)
        }
        Pattern::Tuple(elems) => elems.iter().all(pattern_is_lowerable_aggregate),
        Pattern::Struct { fields, .. } => fields
            .iter()
            .all(|(_, fp)| pattern_is_lowerable_aggregate(fp)),
        Pattern::EnumVariant { args, .. } => args.iter().all(pattern_is_lowerable_aggregate),
    }
}

fn scrutinee_ty_is_lowerable(ty: &Type) -> bool {
    ty.is_int()
        || *ty == Type::bool()
        || ty.is_float()
        || *ty == Type::string()
        || *ty == Type::unit()
        || matches!(
            ty,
            Type::Tuple(_)
                | Type::NamedType { .. }
                | Type::Array { .. }
                | Type::Slice(_)
                | Type::Ref { .. }
        )
}

/// Literal / ident / wildcard / Or on int-bool-float-`str` (and ident/wildcard
/// on array/slice/ref/unit), plus tuple, class-struct, and enum-variant
/// patterns (including nested enum payloads). Catch-all arms (ident /
/// wildcard / Or of those) are simple on any type.
pub fn match_is_simple(m: &MatchExpr, scrutinee_ty: &Type) -> bool {
    if !m.arms.is_empty() && m.arms.iter().all(|a| pattern_is_catchall(&a.pattern)) {
        return true;
    }
    if !scrutinee_ty_is_lowerable(scrutinee_ty) {
        return false;
    }
    m.arms
        .iter()
        .all(|a| pattern_is_lowerable(&a.pattern, scrutinee_ty))
}

fn pattern_ty_hint(p: &Pattern) -> Option<Type> {
    match p {
        Pattern::Literal(lit) => literal_ty(lit),
        Pattern::Tuple(elems) => {
            let mut tys = Vec::with_capacity(elems.len());
            for e in elems {
                tys.push(field_scrut_ty(e).ok()?);
            }
            Some(Type::Tuple(tys))
        }
        Pattern::Struct { name, .. } => Some(Type::NamedType { name: name.clone() }),
        Pattern::EnumVariant { enum_name, .. } => Some(Type::NamedType {
            name: enum_name.clone(),
        }),
        Pattern::Or(alts) => alts.iter().find_map(|a| {
            if pattern_is_catchall(a) {
                None
            } else {
                pattern_ty_hint(a)
            }
        }),
        Pattern::Wildcard | Pattern::Ident(_) => None,
    }
}

/// Type implied by non-catchall arms when the infer callback is missing
/// (syntactic `Variable` → `int` would leave enum/tuple/struct `Match`).
pub fn scrutinee_ty_from_patterns(m: &MatchExpr) -> Option<Type> {
    let mut hint: Option<Type> = None;
    for arm in &m.arms {
        if pattern_is_catchall(&arm.pattern) {
            continue;
        }
        let Some(ty) = pattern_ty_hint(&arm.pattern) else {
            continue;
        };
        match &hint {
            None => hint = Some(ty),
            Some(prev) if prev == &ty => {}
            Some(Type::NamedType { name: n }) => {
                if let Type::NamedType { name } = &ty {
                    if name != n {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    hint
}

/// Ident bindings introduced by `pattern` at `scrut_ty` (match-arm locals).
/// Unresolvable enum payload types are `Err` (do not invent `int`). Nested
/// `Result.Ok(Option.Some(x))` still walks the inner enum from the pattern;
/// `x` is bound only when its payload type is resolvable (registry), not as `int`.
pub fn pattern_binding_types(
    pattern: &Pattern,
    scrut_ty: &Type,
    registry: Option<&Arc<RwLock<TypeRegistry>>>,
) -> Result<Vec<(String, Type)>, String> {
    let mut out = Vec::new();
    collect_pattern_binds(pattern, scrut_ty, registry, &mut out)?;
    Ok(out)
}

fn collect_pattern_binds(
    pattern: &Pattern,
    scrut_ty: &Type,
    registry: Option<&Arc<RwLock<TypeRegistry>>>,
    out: &mut Vec<(String, Type)>,
) -> Result<(), String> {
    match pattern {
        Pattern::Ident(name) => {
            out.push((name.clone(), scrut_ty.clone()));
            Ok(())
        }
        Pattern::Or(alts) => {
            if let Some(first) = alts.first() {
                collect_pattern_binds(first, scrut_ty, registry, out)?;
            }
            Ok(())
        }
        Pattern::Tuple(elems) => {
            let Type::Tuple(tys) = scrut_ty else {
                return Err("tuple pattern on non-tuple scrutinee".into());
            };
            if tys.len() != elems.len() {
                return Err(format!(
                    "tuple pattern length {} does not match type length {}",
                    elems.len(),
                    tys.len()
                ));
            }
            for (e, ty) in elems.iter().zip(tys.iter()) {
                collect_pattern_binds(e, ty, registry, out)?;
            }
            Ok(())
        }
        Pattern::Struct { name, fields } => {
            for (fname, fp) in fields {
                let ty = class_field_type(registry, name, fname)?;
                collect_pattern_binds(fp, &ty, registry, out)?;
            }
            Ok(())
        }
        Pattern::EnumVariant {
            enum_name,
            variant,
            args,
        } => {
            let en = enum_name_of(enum_name, scrut_ty);
            if args.is_empty() {
                return Ok(());
            }
            match payload_types(registry, &en, variant) {
                Err(e) => {
                    // Outer payload unresolvable: still walk nested enum/struct/tuple
                    // so `Result.Ok(Option.Some(x))` can bind `x`. Ident at this
                    // slot must not become invented `int`.
                    for arg in args {
                        match arg {
                            Pattern::Ident(_) => return Err(e.clone()),
                            Pattern::Wildcard | Pattern::Literal(_) => {}
                            nested => {
                                collect_pattern_binds(
                                    nested,
                                    &field_scrut_ty(nested)?,
                                    registry,
                                    out,
                                )?;
                            }
                        }
                    }
                    Ok(())
                }
                Ok(tys) => {
                    for (i, arg) in args.iter().enumerate() {
                        let ty = tys.get(i).cloned().ok_or_else(|| {
                            format!("missing payload type for {en}.{variant}[{i}]")
                        })?;
                        collect_pattern_binds(arg, &ty, registry, out)?;
                    }
                    Ok(())
                }
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => Ok(()),
    }
}

pub fn match_arms_to_if(
    value: HirExpr,
    arms: Vec<(Pattern, Option<HirExpr>, Vec<HirStmt>)>,
) -> Result<HirStmt, String> {
    match_arms_to_if_with_registry(value, arms, None)
}

pub fn match_arms_to_if_with_registry(
    value: HirExpr,
    arms: Vec<(Pattern, Option<HirExpr>, Vec<HirStmt>)>,
    registry: Option<Arc<RwLock<TypeRegistry>>>,
) -> Result<HirStmt, String> {
    if arms.is_empty() {
        return Err("match has no arms".into());
    }

    // Copy the scrutinee so elif conditions do not load a variable that
    // `rm` in the if-join (walked by MIR block index before elif blocks).
    let scrut = match &value.kind {
        HirExprKind::Variable(n) => format!("__match_{}", n),
        _ => "__match_s".to_string(),
    };
    let scrut_e = HirExpr {
        ty: value.ty.clone(),
        kind: HirExprKind::Variable(scrut.clone()),
    };

    let mut then_cond: Option<HirExpr> = None;
    let mut then_body: Option<Vec<HirStmt>> = None;
    let mut elifs: Vec<(HirExpr, Vec<HirStmt>)> = Vec::new();
    let mut else_body: Option<Vec<HirStmt>> = None;

    for (pattern, guard, body) in arms {
        let body = bind_pattern(&pattern, &scrut_e, body, registry.as_ref())?;
        let cond = pattern_cond(&pattern, &scrut_e, registry.as_ref())?;
        match cond {
            None => {
                else_body = Some(wrap_guard(guard, body));
            }
            Some(mut cond) => {
                if let Some(g) = guard {
                    cond = and_bool(cond, g);
                }
                if then_cond.is_none() {
                    then_cond = Some(cond);
                    then_body = Some(body);
                } else {
                    elifs.push((cond, body));
                }
            }
        }
    }

    let Some(cond) = then_cond else {
        return Ok(HirStmt::Scope {
            body: vec![
                HirStmt::Let {
                    name: scrut,
                    ty: value.ty.clone(),
                    value,
                },
                HirStmt::If {
                    cond: HirExpr {
                        ty: Type::bool(),
                        kind: HirExprKind::Literal("true".into()),
                    },
                    then_body: else_body.unwrap_or_default(),
                    elifs: Vec::new(),
                    else_body: None,
                },
            ],
        });
    };
    Ok(HirStmt::Scope {
        body: vec![
            HirStmt::Let {
                name: scrut,
                ty: value.ty.clone(),
                value,
            },
            HirStmt::If {
                cond,
                then_body: then_body.unwrap_or_default(),
                elifs,
                else_body,
            },
        ],
    })
}

/// `None` = always matches (wildcard / ident / or containing those). Match
/// lowers to `HirStmt::If`; codegen does not walk parser `Match` AST.
fn pattern_cond(
    pattern: &Pattern,
    value: &HirExpr,
    registry: Option<&Arc<RwLock<TypeRegistry>>>,
) -> Result<Option<HirExpr>, String> {
    match pattern {
        Pattern::Wildcard | Pattern::Ident(_) => Ok(None),
        Pattern::Literal(lit) => Ok(Some(eq_lit(value, lit))),
        Pattern::Or(alts) => {
            let mut acc: Option<HirExpr> = None;
            for alt in alts {
                match pattern_cond(alt, value, registry)? {
                    None => return Ok(None),
                    Some(c) => {
                        acc = Some(match acc {
                            None => c,
                            Some(prev) => or_bool(prev, c),
                        });
                    }
                }
            }
            Ok(acc.or_else(|| {
                Some(HirExpr {
                    ty: Type::bool(),
                    kind: HirExprKind::Literal("true".into()),
                })
            }))
        }
        Pattern::Tuple(elems) => {
            let mut acc: Option<HirExpr> = None;
            for (i, elem) in elems.iter().enumerate() {
                match pattern_cond(elem, &tuple_field(value, i)?, registry)? {
                    None => {}
                    Some(c) => {
                        acc = Some(match acc {
                            None => c,
                            Some(prev) => and_bool(prev, c),
                        });
                    }
                }
            }
            Ok(acc)
        }
        Pattern::Struct { name, fields } => {
            let mut acc: Option<HirExpr> = None;
            for (fname, fpat) in fields {
                if matches!(fpat, Pattern::Wildcard) {
                    continue;
                }
                let field_e = struct_field(
                    value,
                    name,
                    fname,
                    class_field_type(registry, name, fname)?,
                );
                match pattern_cond(fpat, &field_e, registry)? {
                    None => {}
                    Some(c) => {
                        acc = Some(match acc {
                            None => c,
                            Some(prev) => and_bool(prev, c),
                        });
                    }
                }
            }
            Ok(acc)
        }
        Pattern::EnumVariant {
            enum_name,
            variant,
            args,
        } => {
            let en = enum_name_of(enum_name, &value.ty);
            let mut cond = eq_values(
                enum_tag(value, &en),
                enum_variant_tag_const(&en, variant),
            );
            if !args.is_empty() {
                let tys = payload_types(registry, &en, variant)?;
                for (i, arg) in args.iter().enumerate() {
                    if matches!(arg, Pattern::Wildcard) {
                        continue;
                    }
                    let ty = tys.get(i).cloned().ok_or_else(|| {
                        format!("missing payload type for {en}.{variant}[{i}]")
                    })?;
                    let payload = enum_payload(value, &en, variant, i, ty);
                    if let Some(c) = pattern_cond(arg, &payload, registry)? {
                        cond = and_bool(cond, c);
                    }
                }
            }
            Ok(Some(cond))
        }
    }
}

fn bind_pattern(
    pattern: &Pattern,
    value: &HirExpr,
    body: Vec<HirStmt>,
    registry: Option<&Arc<RwLock<TypeRegistry>>>,
) -> Result<Vec<HirStmt>, String> {
    match pattern {
        Pattern::Ident(name) => Ok(prepend_let(name, value, body)),
        Pattern::Or(alts) => {
            // AST match binds the first alternative only.
            match alts.first() {
                Some(first) => bind_pattern(first, value, body, registry),
                None => Ok(body),
            }
        }
        Pattern::Tuple(elems) => {
            let mut body = body;
            for (i, elem) in elems.iter().enumerate().rev() {
                body = bind_pattern(elem, &tuple_field(value, i)?, body, registry)?;
            }
            Ok(body)
        }
        Pattern::Struct { name, fields } => {
            let mut body = body;
            for (fname, fpat) in fields.iter().rev() {
                if matches!(fpat, Pattern::Wildcard | Pattern::Literal(_)) {
                    continue;
                }
                let field_e = struct_field(
                    value,
                    name,
                    fname,
                    class_field_type(registry, name, fname)?,
                );
                body = bind_pattern(fpat, &field_e, body, registry)?;
            }
            Ok(body)
        }
        Pattern::EnumVariant {
            enum_name,
            variant,
            args,
        } => {
            let en = enum_name_of(enum_name, &value.ty);
            if args.is_empty() {
                return Ok(body);
            }
            let tys = payload_types(registry, &en, variant)?;
            let mut body = body;
            for (i, arg) in args.iter().enumerate().rev() {
                if matches!(arg, Pattern::Wildcard | Pattern::Literal(_)) {
                    continue;
                }
                let ty = tys.get(i).cloned().ok_or_else(|| {
                    format!("missing payload type for {en}.{variant}[{i}]")
                })?;
                let payload = enum_payload(value, &en, variant, i, ty);
                body = bind_pattern(arg, &payload, body, registry)?;
            }
            Ok(body)
        }
        _ => Ok(body),
    }
}

fn enum_name_of(pattern_name: &str, ty: &Type) -> String {
    if !pattern_name.is_empty() {
        return pattern_name.to_string();
    }
    match ty {
        Type::NamedType { name } => name.clone(),
        _ => String::new(),
    }
}

fn payload_types(
    registry: Option<&Arc<RwLock<TypeRegistry>>>,
    enum_name: &str,
    variant: &str,
) -> Result<Vec<Type>, String> {
    let Some(reg) = registry else {
        return Err("no type registry for enum payload types".into());
    };
    let Ok(reg) = reg.read() else {
        return Err("type registry poisoned".into());
    };
    let Some(TypeDef::Enum { variants, .. }) = reg.get_type(enum_name) else {
        return Err(format!("unknown enum `{enum_name}` for match payload"));
    };
    let Some(v) = variants.iter().find(|v| v.name == variant) else {
        return Err(format!("unknown variant `{enum_name}.{variant}`"));
    };
    let mut out = Vec::with_capacity(v.fields.len());
    for field in &v.fields {
        let type_str = match field {
            VariantField::Positional(t) => t.as_str(),
            VariantField::Named { ty, .. } => ty.as_str(),
        };
        match reg.resolve_type(type_str) {
            Ok(ty) => out.push(ty),
            Err(_) => {
                return Err(format!(
                    "cannot resolve match payload type `{type_str}` for {enum_name}.{variant}"
                ));
            }
        }
    }
    Ok(out)
}

pub(crate) fn class_field_type(
    registry: Option<&Arc<RwLock<TypeRegistry>>>,
    class_name: &str,
    field: &str,
) -> Result<Type, String> {
    let Some(reg) = registry else {
        return Err(format!(
            "no type registry for class field {class_name}.{field}"
        ));
    };
    let Ok(reg) = reg.read() else {
        return Err("type registry poisoned".into());
    };
    let mut current = Some(class_name.to_string());
    while let Some(cname) = current {
        match reg.get_type(&cname) {
            Some(TypeDef::Class { fields, parent, .. }) => {
                if let Some(f) = fields.iter().find(|f| f.name == field) {
                    return reg.resolve_type(&f.ty).map_err(|e| e.to_string());
                }
                current = parent;
            }
            _ => {
                return Err(format!(
                    "unknown class `{class_name}` for field `{field}`"
                ));
            }
        }
    }
    Err(format!("unknown field `{class_name}.{field}`"))
}

fn enum_tag(value: &HirExpr, enum_name: &str) -> HirExpr {
    HirExpr {
        ty: Type::int(),
        kind: HirExprKind::EnumTag {
            value: Box::new(value.clone()),
            enum_name: enum_name.to_string(),
        },
    }
}

fn enum_variant_tag_const(enum_name: &str, variant: &str) -> HirExpr {
    HirExpr {
        ty: Type::int(),
        kind: HirExprKind::Member {
            object: Box::new(HirExpr {
                ty: Type::NamedType {
                    name: enum_name.to_string(),
                },
                kind: HirExprKind::Variable(enum_name.to_string()),
            }),
            field: variant.to_string(),
            args: Vec::new(),
        },
    }
}

fn enum_payload(
    value: &HirExpr,
    enum_name: &str,
    variant: &str,
    index: usize,
    ty: Type,
) -> HirExpr {
    HirExpr {
        ty,
        kind: HirExprKind::EnumPayload {
            value: Box::new(value.clone()),
            enum_name: enum_name.to_string(),
            variant: variant.to_string(),
            index,
        },
    }
}

fn eq_values(left: HirExpr, right: HirExpr) -> HirExpr {
    HirExpr {
        ty: Type::bool(),
        kind: HirExprKind::Binary {
            left: Box::new(left),
            op: "==".into(),
            right: Box::new(right),
        },
    }
}

fn field_scrut_ty(pat: &Pattern) -> Result<Type, String> {
    match pat {
        Pattern::Literal(lit) => literal_ty(lit).ok_or_else(|| {
            format!("cannot determine type of match literal `{lit}`")
        }),
        Pattern::Struct { name, .. } => Ok(Type::NamedType { name: name.clone() }),
        Pattern::EnumVariant { enum_name, .. } => Ok(Type::NamedType {
            name: enum_name.clone(),
        }),
        Pattern::Tuple(elems) => {
            let mut tys = Vec::with_capacity(elems.len());
            for e in elems {
                tys.push(field_scrut_ty(e)?);
            }
            Ok(Type::Tuple(tys))
        }
        Pattern::Or(alts) => alts
            .iter()
            .find_map(|a| field_scrut_ty(a).ok())
            .ok_or_else(|| "cannot determine type of or-pattern".into()),
        Pattern::Wildcard | Pattern::Ident(_) => {
            Err("cannot determine field type from wildcard or ident pattern".into())
        }
    }
}

fn tuple_field(value: &HirExpr, index: usize) -> Result<HirExpr, String> {
    let ty = match &value.ty {
        Type::Tuple(ts) => ts.get(index).cloned().ok_or_else(|| {
            format!("tuple pattern index {index} out of range for {:?}", value.ty)
        })?,
        other => {
            return Err(format!(
                "tuple pattern field {index} requires a tuple scrutinee, got {other}"
            ))
        }
    };
    Ok(HirExpr {
        ty,
        kind: HirExprKind::TupleField {
            tuple: Box::new(value.clone()),
            index,
        },
    })
}

fn struct_field(value: &HirExpr, _struct_name: &str, field: &str, ty: Type) -> HirExpr {
    HirExpr {
        ty,
        kind: HirExprKind::Member {
            object: Box::new(value.clone()),
            field: field.to_string(),
            args: Vec::new(),
        },
    }
}

fn prepend_let(name: &str, value: &HirExpr, mut body: Vec<HirStmt>) -> Vec<HirStmt> {
    let mut out = vec![HirStmt::Let {
        name: name.to_string(),
        ty: value.ty.clone(),
        value: value.clone(),
    }];
    out.append(&mut body);
    out
}

fn eq_lit(value: &HirExpr, lit: &str) -> HirExpr {
    HirExpr {
        ty: Type::bool(),
        kind: HirExprKind::Binary {
            left: Box::new(value.clone()),
            op: "==".into(),
            right: Box::new(HirExpr {
                ty: value.ty.clone(),
                kind: HirExprKind::Literal(lit.to_string()),
            }),
        },
    }
}

fn and_bool(a: HirExpr, b: HirExpr) -> HirExpr {
    HirExpr {
        ty: Type::bool(),
        kind: HirExprKind::Binary {
            left: Box::new(a),
            op: "&&".into(),
            right: Box::new(b),
        },
    }
}

fn or_bool(a: HirExpr, b: HirExpr) -> HirExpr {
    HirExpr {
        ty: Type::bool(),
        kind: HirExprKind::Binary {
            left: Box::new(a),
            op: "||".into(),
            right: Box::new(b),
        },
    }
}

fn wrap_guard(guard: Option<HirExpr>, body: Vec<HirStmt>) -> Vec<HirStmt> {
    match guard {
        Some(cond) => vec![HirStmt::If {
            cond,
            then_body: body,
            elifs: Vec::new(),
            else_body: None,
        }],
        None => body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::expr::Expression;
    use crate::parser::pattern::Pattern;
    use crate::parser::r#match::{MatchArm, MatchExpr};
    use crate::types::definition::{EnumVariant as DefEnumVariant, TypeDef, VariantField};

    fn int_var(name: &str) -> HirExpr {
        HirExpr {
            ty: Type::int(),
            kind: HirExprKind::Variable(name.into()),
        }
    }

    fn match_only(pattern: Pattern) -> MatchExpr {
        MatchExpr {
            value: Expression::var("s"),
            arms: vec![MatchArm {
                pattern,
                guard: None,
                body: vec![],
            }],
        }
    }

    fn weird_registry_unresolvable() -> Arc<RwLock<TypeRegistry>> {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Weird",
                TypeDef::Enum {
                    name: "Weird".into(),
                    variants: vec![DefEnumVariant {
                        name: "Some".into(),
                        fields: vec![VariantField::Positional("not_a_real_type".into())],
                    }],
                    generics: vec![],
                },
            )
            .expect("Weird");
        Arc::new(RwLock::new(registry))
    }

    fn weird_scrut() -> HirExpr {
        HirExpr {
            ty: Type::NamedType {
                name: "Weird".into(),
            },
            kind: HirExprKind::Variable("w".into()),
        }
    }

    #[test]
    fn enum_payload_unresolvable_type_is_err_not_int() {
        let err = match_arms_to_if_with_registry(
            weird_scrut(),
            vec![
                (
                    Pattern::EnumVariant {
                        enum_name: "Weird".into(),
                        variant: "Some".into(),
                        args: vec![Pattern::Ident("x".into())],
                    },
                    None,
                    vec![],
                ),
                (Pattern::Wildcard, None, vec![]),
            ],
            Some(weird_registry_unresolvable()),
        )
        .expect_err("unresolvable payload type must not lower as int");
        assert!(
            err.contains("not_a_real_type"),
            "expected unresolved type name in error, got {err:?}"
        );
    }

    #[test]
    fn enum_payload_unresolvable_named_field_is_err_not_int() {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Boxy",
                TypeDef::Enum {
                    name: "Boxy".into(),
                    variants: vec![DefEnumVariant {
                        name: "Wrap".into(),
                        fields: vec![VariantField::Named {
                            name: "inner".into(),
                            ty: "not_a_real_type".into(),
                        }],
                    }],
                    generics: vec![],
                },
            )
            .expect("Boxy");
        let err = match_arms_to_if_with_registry(
            HirExpr {
                ty: Type::NamedType {
                    name: "Boxy".into(),
                },
                kind: HirExprKind::Variable("b".into()),
            },
            vec![(
                Pattern::EnumVariant {
                    enum_name: "Boxy".into(),
                    variant: "Wrap".into(),
                    args: vec![Pattern::Ident("x".into())],
                },
                None,
                vec![],
            )],
            Some(Arc::new(RwLock::new(registry))),
        )
        .expect_err("unresolvable named payload type must not lower as int");
        assert!(
            err.contains("not_a_real_type"),
            "expected unresolved type name in error, got {err:?}"
        );
    }

    fn result_ok_option_some_x() -> Pattern {
        Pattern::EnumVariant {
            enum_name: "Result".into(),
            variant: "Ok".into(),
            args: vec![Pattern::EnumVariant {
                enum_name: "Option".into(),
                variant: "Some".into(),
                args: vec![Pattern::Ident("x".into())],
            }],
        }
    }

    #[test]
    fn nested_result_ok_inner_ident_without_payload_type_is_err() {
        let err = pattern_binding_types(
            &result_ok_option_some_x(),
            &Type::NamedType {
                name: "Result".into(),
            },
            None,
        )
        .expect_err("inner x has no payload type without registry");
        assert!(
            err.contains("ident")
                || err.contains("wildcard")
                || err.contains("field type")
                || err.contains("registry")
                || err.contains("payload"),
            "expected missing payload/registry error, not a typed-as-int bind, got {err:?}"
        );
    }

    #[test]
    fn nested_result_ok_binds_inner_x_from_registry_payload() {
        let registry = TypeRegistry::root();
        registry
            .define_type(
                "Option",
                TypeDef::Enum {
                    name: "Option".into(),
                    variants: vec![
                        DefEnumVariant {
                            name: "Some".into(),
                            fields: vec![VariantField::Positional("int".into())],
                        },
                        DefEnumVariant {
                            name: "None".into(),
                            fields: vec![],
                        },
                    ],
                    generics: vec![],
                },
            )
            .expect("Option");
        registry
            .define_type(
                "Result",
                TypeDef::Enum {
                    name: "Result".into(),
                    variants: vec![DefEnumVariant {
                        name: "Ok".into(),
                        fields: vec![VariantField::Positional("Option".into())],
                    }],
                    generics: vec![],
                },
            )
            .expect("Result");
        let registry = Arc::new(RwLock::new(registry));
        let binds = pattern_binding_types(
            &result_ok_option_some_x(),
            &Type::NamedType {
                name: "Result".into(),
            },
            Some(&registry),
        )
        .expect("registry payload types");
        let x_ty = binds
            .iter()
            .find(|(n, _)| n == "x")
            .map(|(_, t)| t.clone())
            .expect("inner x bound");
        assert_eq!(x_ty, Type::int());
    }

    #[test]
    fn unresolvable_payload_pattern_binding_types_is_err_not_int() {
        let registry = weird_registry_unresolvable();
        let err = pattern_binding_types(
            &Pattern::EnumVariant {
                enum_name: "Weird".into(),
                variant: "Some".into(),
                args: vec![Pattern::Ident("x".into())],
            },
            &Type::NamedType {
                name: "Weird".into(),
            },
            Some(&registry),
        )
        .expect_err("unresolvable payload must not bind x as int");
        assert!(
            err.contains("not_a_real_type"),
            "expected unresolved type name in error, got {err:?}"
        );
    }

    #[test]
    fn tuple_pattern_on_non_tuple_scrutinee_is_err_not_int() {
        let err = match_arms_to_if(
            int_var("t"),
            vec![(
                Pattern::Tuple(vec![Pattern::Ident("a".into()), Pattern::Ident("b".into())]),
                None,
                vec![],
            )],
        )
        .expect_err("tuple field on int must not be typed as int");
        assert!(
            err.contains("tuple") || err.contains("non-tuple"),
            "expected tuple-type error, got {err:?}"
        );
    }

    #[test]
    fn tuple_field_oob_index_is_err_not_int() {
        let err = match_arms_to_if(
            HirExpr {
                ty: Type::Tuple(vec![Type::int()]),
                kind: HirExprKind::Variable("t".into()),
            },
            vec![(
                Pattern::Tuple(vec![Pattern::Ident("a".into()), Pattern::Ident("b".into())]),
                None,
                vec![],
            )],
        )
        .expect_err("OOB tuple field must not be typed as int");
        assert!(
            err.contains("tuple") || err.contains("index") || err.contains("field"),
            "expected OOB tuple error, got {err:?}"
        );
    }

    #[test]
    fn pattern_binding_types_tuple_on_non_tuple_is_err_not_int() {
        let err = pattern_binding_types(
            &Pattern::Tuple(vec![Pattern::Ident("a".into())]),
            &Type::int(),
            None,
        )
        .expect_err("tuple binds on int must not invent int fields");
        assert!(
            err.contains("tuple") || err.contains("non-tuple"),
            "expected tuple-type error, got {err:?}"
        );
    }

    #[test]
    fn wildcard_tuple_pattern_does_not_hint_int_fields() {
        let hint = scrutinee_ty_from_patterns(&match_only(Pattern::Tuple(vec![
            Pattern::Wildcard,
            Pattern::Wildcard,
        ])));
        assert!(
            hint.is_none(),
            "(_, _) must not invent Tuple([int, int]), got {hint:?}"
        );
    }

    #[test]
    fn nested_tuple_wildcards_do_not_hint_int_tuple() {
        let hint = scrutinee_ty_from_patterns(&match_only(Pattern::Tuple(vec![
            Pattern::Tuple(vec![Pattern::Wildcard, Pattern::Wildcard]),
            Pattern::Literal("0".into()),
        ])));
        assert!(
            hint.is_none(),
            "nested (_, _) must not invent Tuple([int, int]), got {hint:?}"
        );
    }

    #[test]
    fn garbage_literal_does_not_hint_as_int() {
        let hint = scrutinee_ty_from_patterns(&match_only(Pattern::Literal("not_a_value".into())));
        assert!(
            hint.is_none(),
            "unknown literal must not become int, got {hint:?}"
        );
    }
}
