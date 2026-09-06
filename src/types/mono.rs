//! Generics v1: substitute type-param tokens and inject concrete classes.

use std::collections::HashMap;

use crate::parser::class::{ClassDef, MethodDef};
use crate::parser::expr::Expression;
use crate::parser::function::{Function, FunctionBody};
use crate::parser::Statement;

use super::definition::{type_from_str, Type};

/// Header `<T, U>` scraped because `ClassDef.type_params` may not exist yet.
#[derive(Debug, Clone, Default)]
pub struct HeaderGenerics {
    pub classes: HashMap<String, Vec<String>>,
    pub functions: HashMap<String, Vec<String>>,
}

/// Parser field when present; otherwise empty (pipeline uses [`peel_header_generics`]).
pub fn ast_class_type_params(class: &ClassDef) -> Vec<String> {
    class.type_params.clone()
}

/// Parser field when present; otherwise empty.
pub fn ast_fn_type_params(func: &Function) -> Vec<String> {
    func.type_params.clone()
}

/// Strip `class Box<T>:` / `fn id<T>(` so today's parser can accept the rest.
pub fn peel_header_generics(source: &str) -> (String, HeaderGenerics) {
    let mut headers = HeaderGenerics::default();
    let mut out = String::with_capacity(source.len());
    for (i, line) in source.split_inclusive('\n').enumerate() {
        let peeled = peel_decl_line(line);
        if let Some((is_class, name, params)) = peeled.1 {
            if is_class {
                headers.classes.insert(name, params);
            } else {
                headers.functions.insert(name, params);
            }
        }
        if i > 0 && !out.ends_with('\n') && source.contains('\n') {
            // split_inclusive keeps newlines
        }
        out.push_str(&peeled.0);
    }
    if source.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    (out, headers)
}

fn peel_decl_line(line: &str) -> (String, Option<(bool, String, Vec<String>)>) {
    let nl = if line.ends_with('\n') { "\n" } else { "" };
    let body = line.strip_suffix('\n').unwrap_or(line);
    let trimmed = body.trim_start();
    let indent_len = body.len() - trimmed.len();
    let indent = &body[..indent_len];

    if let Some(r) = trimmed.strip_prefix("packed ") {
        let r = r.trim_start();
        if let Some(r) = r.strip_prefix("class ") {
            return peel_named_header(indent, "packed class ", r.trim_start(), true, nl);
        }
        return (line.to_string(), None);
    }
    if let Some(r) = trimmed.strip_prefix("class ") {
        return peel_named_header(indent, "class ", r.trim_start(), true, nl);
    }
    if let Some(r) = trimmed.strip_prefix("c fn ") {
        return peel_named_header(indent, "c fn ", r.trim_start(), false, nl);
    }
    if let Some(r) = trimmed.strip_prefix("fn ") {
        return peel_named_header(indent, "fn ", r.trim_start(), false, nl);
    }
    (line.to_string(), None)
}

fn peel_named_header(
    indent: &str,
    kw: &str,
    after_kw: &str,
    is_class: bool,
    nl: &str,
) -> (String, Option<(bool, String, Vec<String>)>) {
    let (name, rest) = take_ident(after_kw);
    if name.is_empty() {
        return (format!("{indent}{kw}{after_kw}{nl}"), None);
    }
    let rest = rest.trim_start();
    if !rest.starts_with('<') {
        return (
            format!("{indent}{kw}{name}{}{nl}", &after_kw[name.len()..]),
            None,
        );
    }
    match take_angle_list(rest) {
        Some((params, after)) => {
            let rebuilt = format!("{indent}{kw}{name}{after}{nl}");
            Some((is_class, name.to_string(), params))
                .map(|info| (rebuilt, Some(info)))
                .unwrap()
        }
        None => (format!("{indent}{kw}{after_kw}{nl}"), None),
    }
}

fn take_ident(s: &str) -> (&str, &str) {
    let end = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_alphanumeric() && *c != '_')
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    if end == 0 {
        return ("", s);
    }
    let first = s.chars().next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return ("", s);
    }
    (&s[..end], &s[end..])
}

fn take_angle_list(s: &str) -> Option<(Vec<String>, &str)> {
    if !s.starts_with('<') {
        return None;
    }
    let mut depth = 0i32;
    let mut end = None;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let inner = s[1..end].trim();
    if inner.is_empty() {
        return None;
    }
    let mut params = Vec::new();
    for part in inner.split(',') {
        let p = part.trim();
        if p.is_empty() || !p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
        params.push(p.to_string());
    }
    Some((params, &s[end + 1..]))
}

pub fn subst_type_string(s: &str, map: &HashMap<String, Type>) -> Result<String, String> {
    let ty = type_from_str(s)?;
    Ok(ty.subst_params(map).to_string())
}

pub fn instantiate_class(
    class: &ClassDef,
    params: &[String],
    args: &[Type],
) -> Result<ClassDef, String> {
    if params.len() != args.len() {
        return Err(format!(
            "type '{}' expected {} generic arguments, found {}",
            class.name,
            params.len(),
            args.len()
        ));
    }
    let concrete = Type::App {
        name: class.name.clone(),
        args: args.to_vec(),
    }
    .mono_name();
    let map = subst_map(class.name.as_str(), &concrete, params, args);
    let mut out = class.clone();
    out.name = concrete;
    out.type_params.clear();
    for field in &mut out.fields {
        field.field_type = subst_type_string(&field.field_type, &map)?;
    }
    for method in &mut out.methods {
        subst_method(method, &map)?;
    }
    Ok(out)
}

pub fn instantiate_function(
    func: &Function,
    params: &[String],
    args: &[Type],
) -> Result<Function, String> {
    if params.len() != args.len() {
        return Err(format!(
            "cannot infer generic arguments for '{}'",
            func.name
        ));
    }
    let map = subst_map(func.name.as_str(), func.name.as_str(), params, args);
    let mut out = func.clone();
    out.type_params.clear();
    subst_function(&mut out, &map)?;
    Ok(out)
}

fn subst_map(
    template_name: &str,
    concrete_name: &str,
    params: &[String],
    args: &[Type],
) -> HashMap<String, Type> {
    let mut map = HashMap::new();
    for (p, a) in params.iter().zip(args.iter()) {
        map.insert(p.clone(), a.clone());
    }
    if template_name != concrete_name {
        map.insert(
            template_name.to_string(),
            Type::NamedType {
                name: concrete_name.to_string(),
            },
        );
    }
    map
}

fn subst_method(method: &mut MethodDef, map: &HashMap<String, Type>) -> Result<(), String> {
    for p in &mut method.parameters {
        if p.name == "self" && p.param_type.is_empty() {
            continue;
        }
        if !p.param_type.is_empty() {
            p.param_type = subst_type_string(&p.param_type, map)?;
        }
    }
    method.return_type = subst_type_string(&method.return_type, map)?;
    subst_stmts(&mut method.body, map)
}

fn subst_function(func: &mut Function, map: &HashMap<String, Type>) -> Result<(), String> {
    for p in &mut func.parameters {
        func_param_subst(&mut p.param_type, map)?;
    }
    func.return_type = subst_type_string(&func.return_type, map)?;
    match &mut func.body {
        FunctionBody::Block(stmts) => subst_stmts(stmts, map)?,
        FunctionBody::Expression(expr) => subst_expr(expr, map)?,
        FunctionBody::External => {}
    }
    Ok(())
}

fn func_param_subst(ty: &mut String, map: &HashMap<String, Type>) -> Result<(), String> {
    if ty.is_empty() {
        return Ok(());
    }
    *ty = subst_type_string(ty, map)?;
    Ok(())
}

fn subst_stmts(stmts: &mut [Statement], map: &HashMap<String, Type>) -> Result<(), String> {
    for stmt in stmts {
        subst_stmt(stmt, map)?;
    }
    Ok(())
}

fn subst_stmt(stmt: &mut Statement, map: &HashMap<String, Type>) -> Result<(), String> {
    match stmt {
        Statement::VariableDecl(decl) => {
            decl.var_type = subst_type_string(&decl.var_type, map)?;
            subst_expr(&mut decl.value, map)?;
        }
        Statement::Assignment(_, value) => subst_expr(value, map)?,
        Statement::Return(ret) => {
            if let Some(v) = &mut ret.value {
                subst_expr(v, map)?;
            }
        }
        Statement::Expr(expr) => subst_expr(expr, map)?,
        Statement::If(if_expr) => {
            subst_expr(&mut if_expr.condition, map)?;
            subst_stmts(&mut if_expr.body, map)?;
            for elif in &mut if_expr.elifs {
                subst_expr(&mut elif.condition, map)?;
                subst_stmts(&mut elif.body, map)?;
            }
            if let Some(else_body) = &mut if_expr.else_body {
                subst_stmts(else_body, map)?;
            }
        }
        Statement::While(w) => {
            subst_expr(&mut w.condition, map)?;
            subst_stmts(&mut w.body, map)?;
        }
        Statement::For(f) => {
            subst_stmts(&mut f.body, map)?;
        }
        Statement::Match(m) => {
            subst_expr(&mut m.value, map)?;
            for arm in &mut m.arms {
                subst_stmts(&mut arm.body, map)?;
            }
        }
        Statement::Function(func) => subst_function(func, map)?,
        Statement::Class(class) => {
            for field in &mut class.fields {
                field.field_type = subst_type_string(&field.field_type, map)?;
            }
            for method in &mut class.methods {
                subst_method(method, map)?;
            }
        }
        Statement::Raise(r) => subst_expr(&mut r.error_expr, map)?,
        _ => {}
    }
    Ok(())
}

fn subst_expr(expr: &mut Expression, map: &HashMap<String, Type>) -> Result<(), String> {
    match expr {
        Expression::Spanned { inner, .. } => subst_expr(inner, map)?,
        Expression::Binary { left, right, .. } => {
            subst_expr(left, map)?;
            subst_expr(right, map)?;
        }
        Expression::Unary { operand, .. } => subst_expr(operand, map)?,
        Expression::Call { function, args } => {
            subst_expr(function, map)?;
            for a in args {
                subst_expr(a, map)?;
            }
        }
        Expression::ConstructorCall { class_name, args } => {
            rewrite_class_name(class_name, map);
            for a in args {
                subst_expr(a, map)?;
            }
        }
        Expression::Member { object, args, .. } => {
            subst_expr(object, map)?;
            for a in args {
                subst_expr(a, map)?;
            }
        }
        Expression::Index { array, index } => {
            subst_expr(array, map)?;
            subst_expr(index, map)?;
        }
        Expression::ArrayLiteral { elements } | Expression::TupleLiteral { elements } => {
            for e in elements {
                subst_expr(e, map)?;
            }
        }
        Expression::StructLiteral { struct_name, fields } => {
            rewrite_class_name(struct_name, map);
            for (_, v) in fields {
                subst_expr(v, map)?;
            }
        }
        Expression::Assign { object, value, .. } => {
            subst_expr(object, map)?;
            subst_expr(value, map)?;
        }
        Expression::TypeCast { value, .. } => subst_expr(value, map)?,
        Expression::AnonymousFunction { func } => subst_function(func, map)?,
        Expression::Literal(_) | Expression::Variable(_) | Expression::FString { .. } => {}
    }
    Ok(())
}

fn rewrite_class_name(name: &mut String, map: &HashMap<String, Type>) {
    if let Some(Type::NamedType { name: n }) = map.get(name) {
        *name = n.clone();
    }
}

pub fn rewrite_program(program: &mut crate::parser::Program, reg: &super::registry::TypeRegistry) {
    for stmt in &mut program.statements {
        rewrite_stmt(stmt, reg, None);
    }
}

fn rewrite_type_str(s: &mut String, reg: &super::registry::TypeRegistry) {
    if s.is_empty() {
        return;
    }
    if let Ok(ty) = reg.resolve_type(s) {
        *s = ty.to_string();
    }
}

fn concrete_class_name(
    name: &str,
    hint: Option<&Type>,
    reg: &super::registry::TypeRegistry,
) -> String {
    if !reg.is_generic_class(name) {
        return name.to_string();
    }
    if let Some(Type::NamedType { name: concrete }) = hint {
        if reg.template_name_of(concrete).as_deref() == Some(name) {
            return concrete.clone();
        }
    }
    name.to_string()
}

fn rewrite_stmt(
    stmt: &mut Statement,
    reg: &super::registry::TypeRegistry,
    hint: Option<&Type>,
) {
    match stmt {
        Statement::VariableDecl(decl) => {
            rewrite_type_str(&mut decl.var_type, reg);
            let ty = reg.resolve_type(&decl.var_type).ok();
            rewrite_expr(&mut decl.value, ty.as_ref(), reg);
        }
        Statement::Assignment(_, value) => rewrite_expr(value, None, reg),
        Statement::Return(ret) => {
            if let Some(v) = &mut ret.value {
                rewrite_expr(v, hint, reg);
            }
        }
        Statement::Expr(expr) => rewrite_expr(expr, None, reg),
        Statement::If(if_expr) => {
            rewrite_expr(&mut if_expr.condition, None, reg);
            rewrite_stmts(&mut if_expr.body, reg, hint);
            for elif in &mut if_expr.elifs {
                rewrite_expr(&mut elif.condition, None, reg);
                rewrite_stmts(&mut elif.body, reg, hint);
            }
            if let Some(else_body) = &mut if_expr.else_body {
                rewrite_stmts(else_body, reg, hint);
            }
        }
        Statement::While(w) => {
            rewrite_expr(&mut w.condition, None, reg);
            rewrite_stmts(&mut w.body, reg, hint);
        }
        Statement::For(f) => rewrite_stmts(&mut f.body, reg, hint),
        Statement::Match(m) => {
            rewrite_expr(&mut m.value, None, reg);
            for arm in &mut m.arms {
                rewrite_stmts(&mut arm.body, reg, hint);
            }
        }
        Statement::Function(func) => {
            if func.type_params.is_empty() {
                for p in &mut func.parameters {
                    rewrite_type_str(&mut p.param_type, reg);
                }
                rewrite_type_str(&mut func.return_type, reg);
                let ret = reg.resolve_type(&func.return_type).ok();
                match &mut func.body {
                    FunctionBody::Block(stmts) => rewrite_stmts(stmts, reg, ret.as_ref()),
                    FunctionBody::Expression(expr) => rewrite_expr(expr, ret.as_ref(), reg),
                    FunctionBody::External => {}
                }
            }
        }
        Statement::Class(class) => {
            if class.type_params.is_empty() {
                for field in &mut class.fields {
                    rewrite_type_str(&mut field.field_type, reg);
                }
                for method in &mut class.methods {
                    for p in &mut method.parameters {
                        if !(p.name == "self" && p.param_type.is_empty()) {
                            rewrite_type_str(&mut p.param_type, reg);
                        }
                    }
                    rewrite_type_str(&mut method.return_type, reg);
                    let ret = reg.resolve_type(&method.return_type).ok();
                    rewrite_stmts(&mut method.body, reg, ret.as_ref());
                }
            }
        }
        Statement::Raise(r) => rewrite_expr(&mut r.error_expr, None, reg),
        _ => {}
    }
}

fn rewrite_stmts(
    stmts: &mut [Statement],
    reg: &super::registry::TypeRegistry,
    hint: Option<&Type>,
) {
    for stmt in stmts {
        rewrite_stmt(stmt, reg, hint);
    }
}

fn rewrite_expr(
    expr: &mut Expression,
    hint: Option<&Type>,
    reg: &super::registry::TypeRegistry,
) {
    match expr {
        Expression::Spanned { inner, .. } => rewrite_expr(inner, hint, reg),
        Expression::Binary { left, right, .. } => {
            rewrite_expr(left, None, reg);
            rewrite_expr(right, None, reg);
        }
        Expression::Unary { operand, .. } => rewrite_expr(operand, None, reg),
        Expression::Call { function, args } => {
            rewrite_expr(function, None, reg);
            let param_hints: Vec<Type> = match function.kind() {
                Expression::Variable(name) => match reg.get_alias(name) {
                    Some(Type::Function { params, .. }) => params,
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            };
            for (i, a) in args.iter_mut().enumerate() {
                rewrite_expr(a, param_hints.get(i), reg);
            }
        }
        Expression::ConstructorCall { class_name, args } => {
            *class_name = concrete_class_name(class_name, hint, reg);
            for a in args {
                rewrite_expr(a, None, reg);
            }
        }
        Expression::Member { object, args, .. } => {
            rewrite_expr(object, None, reg);
            for a in args {
                rewrite_expr(a, None, reg);
            }
        }
        Expression::Index { array, index } => {
            rewrite_expr(array, None, reg);
            rewrite_expr(index, None, reg);
        }
        Expression::ArrayLiteral { elements } | Expression::TupleLiteral { elements } => {
            for e in elements {
                rewrite_expr(e, None, reg);
            }
        }
        Expression::StructLiteral { struct_name, fields } => {
            *struct_name = concrete_class_name(struct_name, hint, reg);
            let field_tys: Vec<(String, Type)> = {
                use super::definition::TypeDef;
                match reg.get_type(struct_name) {
                    Some(TypeDef::Class { fields: defs, .. }) => defs
                        .iter()
                        .filter_map(|f| {
                            reg.resolve_type(&f.ty)
                                .ok()
                                .map(|ty| (f.name.clone(), ty))
                        })
                        .collect(),
                    _ => Vec::new(),
                }
            };
            for (fname, v) in fields {
                let fty = field_tys.iter().find(|(n, _)| n == fname).map(|(_, t)| t);
                rewrite_expr(v, fty, reg);
            }
        }
        Expression::Assign { object, value, .. } => {
            rewrite_expr(object, None, reg);
            rewrite_expr(value, None, reg);
        }
        Expression::TypeCast { value, .. } => rewrite_expr(value, None, reg),
        Expression::AnonymousFunction { func } => {
            let mut stmt = Statement::Function(func.as_ref().clone());
            rewrite_stmt(&mut stmt, reg, None);
            if let Statement::Function(f) = stmt {
                *func = Box::new(f);
            }
        }
        Expression::Literal(_) | Expression::Variable(_) | Expression::FString { .. } => {}
    }
}

pub fn infer_generic_args(
    params: &[String],
    param_type_strs: &[String],
    arg_types: &[Type],
) -> Result<Vec<Type>, String> {
    let mut found: HashMap<String, Type> = HashMap::new();
    let n = param_type_strs.len().min(arg_types.len());
    for i in 0..n {
        let pattern = type_from_str(&param_type_strs[i])?;
        unify_params_mut(&pattern, &arg_types[i], params, &mut found)?;
    }
    let mut args = Vec::with_capacity(params.len());
    for p in params {
        match found.get(p) {
            Some(ty) => args.push(ty.clone()),
            None => {
                return Err(format!("cannot infer {p}"));
            }
        }
    }
    Ok(args)
}

fn unify_params_mut(
    pattern: &Type,
    concrete: &Type,
    params: &[String],
    found: &mut HashMap<String, Type>,
) -> Result<(), String> {
    match pattern {
        Type::NamedType { name } if params.iter().any(|p| p == name) => {
            if let Some(prev) = found.get(name) {
                if prev != concrete {
                    return Err(format!("cannot infer {name}"));
                }
            } else {
                found.insert(name.clone(), concrete.clone());
            }
            Ok(())
        }
        Type::App {
            name: pn,
            args: pargs,
        } => match concrete {
            Type::App {
                name: cn,
                args: cargs,
            } if pn == cn && pargs.len() == cargs.len() => {
                for (p, c) in pargs.iter().zip(cargs.iter()) {
                    unify_params_mut(p, c, params, found)?;
                }
                Ok(())
            }
            Type::NamedType { .. } => {
                // After mono, uses are NamedType; cannot reconstruct args here.
                Ok(())
            }
            _ => Err(format!("cannot infer {pn}")),
        },
        Type::Ref { elem, mutable } => match concrete {
            Type::Ref {
                elem: c,
                mutable: m,
            } if m == mutable => unify_params_mut(elem, c, params, found),
            _ => Err("cannot infer type parameter".into()),
        },
        Type::Slice(elem) => match concrete {
            Type::Slice(c) => unify_params_mut(elem, c, params, found),
            _ => Err("cannot infer type parameter".into()),
        },
        Type::Array { elem, size } => match concrete {
            Type::Array { elem: c, size: s } if s == size => {
                unify_params_mut(elem, c, params, found)
            }
            _ => Err("cannot infer type parameter".into()),
        },
        Type::Tuple(elems) => match concrete {
            Type::Tuple(c) if c.len() == elems.len() => {
                for (p, x) in elems.iter().zip(c.iter()) {
                    unify_params_mut(p, x, params, found)?;
                }
                Ok(())
            }
            _ => Err("cannot infer type parameter".into()),
        },
        other => {
            if other == concrete {
                Ok(())
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::class::ClassField;

    #[test]
    fn peel_class_and_fn_headers() {
        let src = "class Box<T>:\n    v: T\nfn id<T>(x: T) => T:\n    return x\n";
        let (out, h) = peel_header_generics(src);
        assert!(out.contains("class Box:"), "{out}");
        assert!(out.contains("fn id(x: T)"), "{out}");
        assert_eq!(h.classes.get("Box").unwrap(), &vec!["T".to_string()]);
        assert_eq!(h.functions.get("id").unwrap(), &vec!["T".to_string()]);
        assert!(out.contains("v: T"));
    }

    #[test]
    fn instantiate_box_int() {
        let class = ClassDef {
            name: "Box".into(),
            type_params: vec!["T".into()],
            parent: None,
            fields: vec![ClassField {
                name: "v".into(),
                field_type: "T".into(),
                bit_width: None,
            }],
            methods: vec![],
            packed: false,
            has_constructor: false,
        };
        let inst = instantiate_class(&class, &["T".into()], &[Type::int()]).unwrap();
        assert_eq!(inst.name, "Box__int");
        assert_eq!(inst.fields[0].field_type, "int");
    }

    #[test]
    fn nested_app_mono_name() {
        let ty = Type::from_str("Box<Box<int>>").unwrap();
        assert_eq!(ty.mono_name(), "Box__Box__int");
    }
}
