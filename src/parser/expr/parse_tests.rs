use super::*;
use super::parse_expression;
use crate::parser::expr::Expression;
use crate::parser::parse_program;
use crate::parser::Statement;
use crate::types::definition::Span;

#[test]
fn test_literal_expression() {
    let expr = Expression::literal("42");
    assert!(matches!(expr, Expression::Literal(_)));
}

#[test]
fn test_variable_expression() {
    let expr = Expression::var("x");
    assert!(matches!(expr, Expression::Variable(_)));
}

#[test]
fn test_binary_expression() {
    let expr = Expression::binary(
        Expression::var("x"),
        "+",
        Expression::literal("10"),
    );
    assert!(matches!(expr, Expression::Binary { .. }));
}

#[test]
fn test_parse_simple_expr() {
    let result = Expression::parse("x + 10");
    assert!(result.is_ok());
    if let Ok(expr) = result {
        if let Expression::Binary { op, .. } = expr.kind() {
            assert_eq!(op, "+");
        } else {
            panic!("Expected Binary expression");
        }
    }
}

#[test]
fn test_parse_clone_expr() {
    match parse_expression("clone a").expect("parse").kind() {
        Expression::Unary { op, operand } => {
            assert_eq!(op, "clone");
            assert_eq!(**operand, Expression::Variable("a".to_string()));
        }
        other => panic!("expected unary clone, got {:?}", other),
    }
}

#[test]
fn test_parse_addr_of_and_deref() {
    match parse_expression("&x").expect("parse").kind() {
        Expression::Unary { op, operand } => {
            assert_eq!(op, "&");
            assert_eq!(**operand, Expression::Variable("x".to_string()));
        }
        other => panic!("expected &, got {:?}", other),
    }
    match parse_expression("&mut x").expect("parse").kind() {
        Expression::Unary { op, operand } => {
            assert_eq!(op, "&mut");
            assert_eq!(**operand, Expression::Variable("x".to_string()));
        }
        other => panic!("expected &mut, got {:?}", other),
    }
    match parse_expression("*y").expect("parse").kind() {
        Expression::Unary { op, operand } => {
            assert_eq!(op, "*");
            assert_eq!(**operand, Expression::Variable("y".to_string()));
        }
        other => panic!("expected deref, got {:?}", other),
    }
}

#[test]
fn nested_enum_construction_keeps_outer_dot() {
    match parse_expression("Result.Success(Option.Some(42))").expect("parse").kind() {
        Expression::Member { field, args, .. } => {
            assert_eq!(field, "Success");
            assert_eq!(args.len(), 1);
            match args[0].kind() {
                Expression::Member { field, args, .. } => {
                    assert_eq!(field, "Some");
                    assert_eq!(args.len(), 1);
                }
                other => panic!("inner: {:?}", other),
            }
        }
        other => panic!("outer: {:?}", other),
    }
}

#[test]
fn test_parse_complex_expr() {
    let result = Expression::parse("x + y * z");
    assert!(result.is_ok());
}

fn bin(expr: Expression) -> (Expression, String, Expression) {
    match expr.kind() {
        Expression::Binary { left, op, right } => (left.kind().clone(), op.clone(), right.kind().clone()),
        other => panic!("expected Binary, got {:?}", other),
    }
}

#[test]
fn or_binds_looser_than_and() {
    let (left, op, right) = bin(parse_expression("a || b && c").expect("parse"));
    assert_eq!(op, "||");
    assert_eq!(left, Expression::Variable("a".into()));
    let (rl, rop, rr) = bin(right);
    assert_eq!(rop, "&&");
    assert_eq!(rl, Expression::Variable("b".into()));
    assert_eq!(rr, Expression::Variable("c".into()));
}

#[test]
fn mul_binds_tighter_than_add() {
    let (left, op, right) = bin(parse_expression("1+2*3").expect("parse"));
    assert_eq!(op, "+");
    assert_eq!(left, Expression::Literal("1".into()));
    let (rl, rop, rr) = bin(right);
    assert_eq!(rop, "*");
    assert_eq!(rl, Expression::Literal("2".into()));
    assert_eq!(rr, Expression::Literal("3".into()));
}

#[test]
fn same_prec_splits_rightmost_left_assoc() {
    let (left, op, right) = bin(parse_expression("1-2-3").expect("parse"));
    assert_eq!(op, "-");
    assert_eq!(right, Expression::Literal("3".into()));
    let (ll, lop, lr) = bin(left);
    assert_eq!(lop, "-");
    assert_eq!(ll, Expression::Literal("1".into()));
    assert_eq!(lr, Expression::Literal("2".into()));
}

#[test]
fn bitwise_or_looser_than_xor_and_and() {
    let (left, op, right) = bin(parse_expression("a | b ^ c & d").expect("parse"));
    assert_eq!(op, "|");
    assert_eq!(left, Expression::Variable("a".into()));
    let (xl, xop, xr) = bin(right);
    assert_eq!(xop, "^");
    assert_eq!(xl, Expression::Variable("b".into()));
    let (al, aop, ar) = bin(xr);
    assert_eq!(aop, "&");
    assert_eq!(al, Expression::Variable("c".into()));
    assert_eq!(ar, Expression::Variable("d".into()));
}

#[test]
fn eq_looser_than_comparison() {
    let (left, op, right) = bin(parse_expression("a == b < c").expect("parse"));
    assert_eq!(op, "==");
    assert_eq!(left, Expression::Variable("a".into()));
    let (_, cop, _) = bin(right);
    assert_eq!(cop, "<");
}

#[test]
fn comparison_looser_than_shift() {
    let (left, op, right) = bin(parse_expression("a < b << c").expect("parse"));
    assert_eq!(op, "<");
    assert_eq!(left, Expression::Variable("a".into()));
    let (_, sop, _) = bin(right);
    assert_eq!(sop, "<<");
}

#[test]
fn plus_does_not_steal_unary_minus_or_addr() {
    let (left, op, right) = bin(parse_expression("1 + -2").expect("parse"));
    assert_eq!(op, "+");
    assert_eq!(left, Expression::Literal("1".into()));
    match right.kind() {
        Expression::Unary { op, operand } => {
            assert_eq!(op, "-");
            assert_eq!(**operand, Expression::Literal("2".into()));
        }
        other => panic!("expected unary minus, got {:?}", other),
    }
    let (left, op, right) = bin(parse_expression("a + &b").expect("parse"));
    assert_eq!(op, "+");
    assert_eq!(left, Expression::Variable("a".into()));
    match right.kind() {
        Expression::Unary { op, operand } => {
            assert_eq!(op, "&");
            assert_eq!(**operand, Expression::Variable("b".into()));
        }
        other => panic!("expected unary &, got {:?}", other),
    }
}

#[test]
fn parse_expression_plus_bitwise_not_is_unary() {
    let (left, op, right) = bin(parse_expression("1+~2").expect("parse"));
    assert_eq!(op, "+");
    assert_eq!(left, Expression::Literal("1".into()));
    match right.kind() {
        Expression::Unary { op, operand } => {
            assert_eq!(op, "~");
            assert_eq!(**operand, Expression::Literal("2".into()));
        }
        other => panic!("expected unary ~, got {:?}", other),
    }
}

#[test]
fn parse_expression_and_logical_not_is_unary() {
    let (left, op, right) = bin(parse_expression("a&&!b").expect("parse"));
    assert_eq!(op, "&&");
    assert_eq!(left, Expression::Variable("a".into()));
    match right.kind() {
        Expression::Unary { op, operand } => {
            assert_eq!(op, "!");
            assert_eq!(**operand, Expression::Variable("b".into()));
        }
        other => panic!("expected unary !, got {:?}", other),
    }
}

#[test]
fn paren_group_blocks_split() {
    let (left, op, right) = bin(parse_expression("(a || b) && c").expect("parse"));
    assert_eq!(op, "&&");
    assert_eq!(right, Expression::Variable("c".into()));
    let (_, iop, _) = bin(left);
    assert_eq!(iop, "||");
}

#[test]
fn test_parse_unary() {
    let result = Expression::parse("!true");
    assert!(result.is_ok());
}

#[test]
fn test_parse_function_call() {
    let result = Expression::parse("f(x, y)");
    assert!(result.is_ok());
}

#[test]
fn test_parse_array_index() {
    let result = Expression::parse("arr[0]");
    assert!(result.is_ok());
    if let Ok(expr) = result {
        if let Expression::Index { array, index } = expr.kind() {
            assert!(matches!(array.kind(), Expression::Variable(_)));
            assert!(matches!(index.kind(), Expression::Literal(_)));
        } else {
            panic!("Expected Index expression");
        }
    }
}

#[test]
fn test_parse_array_index_with_variable() {
    let result = Expression::parse("arr[i]");
    assert!(result.is_ok());
    if let Ok(expr) = result {
        if let Expression::Index { array, index } = expr.kind() {
            assert!(matches!(array.kind(), Expression::Variable(_)));
            assert!(matches!(index.kind(), Expression::Variable(_)));
        } else {
            panic!("Expected Index expression");
        }
    }
}

#[test]
fn test_parse_array_index_with_expression() {
    let result = Expression::parse("arr[i + 1]");
    assert!(result.is_ok());
    if let Ok(expr) = result {
        if let Expression::Index { index, .. } = expr.kind() {
            assert!(matches!(index.kind(), Expression::Binary { .. }));
        } else {
            panic!("Expected Index expression with binary index");
        }
    }
}

#[test]
fn parse_index_then_outer_plus_not_inner_index_binop() {
    match parse_expression("foo[i + 1] + 2").expect("parse").kind() {
        Expression::Binary { op, left, right } => {
            assert_eq!(op, "+");
            match left.kind() {
                Expression::Index { array, index } => {
                    assert!(matches!(array.kind(), Expression::Variable(n) if n == "foo"));
                    match index.kind() {
                        Expression::Binary { op, .. } => assert_eq!(op, "+"),
                        other => panic!("index should be i + 1, got {:?}", other),
                    }
                }
                other => panic!("left should be Index, got {:?}", other),
            }
            assert!(matches!(right.kind(), Expression::Literal(_)));
        }
        other => panic!("expected outer Binary +, got {:?}", other),
    }
}

#[test]
fn path_name_parses_as_variable() {
    match parse_expression("Foo::bar").expect("parse").kind() {
        Expression::Variable(name) => assert_eq!(name, "Foo::bar"),
        other => panic!("expected Variable Foo::bar, got {:?}", other),
    }
}

#[test]
fn constructor_call_stays_constructor() {
    match parse_expression("Foo::new(1, 2)").expect("parse").kind() {
        Expression::ConstructorCall { class_name, args } => {
            assert_eq!(class_name, "Foo");
            assert_eq!(args.len(), 2);
        }
        other => panic!("expected ConstructorCall, got {:?}", other),
    }
}

#[test]
fn enum_variant_call_stays_call() {
    match parse_expression("Result::Ok(x)").expect("parse").kind() {
        Expression::Call { function, args } => {
            assert_eq!(**function, Expression::Variable("Result::Ok".to_string()));
            assert_eq!(args.len(), 1);
            assert_eq!(args[0], Expression::Variable("x".to_string()));
        }
        other => panic!("expected Call, got {:?}", other),
    }
}

#[test]
fn path_function_call_keeps_path_variable() {
    match parse_expression("Foo::bar()").expect("parse").kind() {
        Expression::Call { function, args } => {
            assert_eq!(**function, Expression::Variable("Foo::bar".to_string()));
            assert!(args.is_empty());
        }
        other => panic!("expected Call with Variable Foo::bar, got {:?}", other),
    }
}

#[test]
fn constructor_args_are_expressions() {
    let result = parse_expression("Point::new(1, 2)").unwrap();
    match result.kind() {
        Expression::ConstructorCall { args, .. } => {
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0].kind(), Expression::Literal(_)));
            assert!(matches!(args[1].kind(), Expression::Literal(_)));
        }
        other => panic!("{:?}", other),
    }
}

#[test]
fn enum_variant_args_are_expressions() {
    let result = parse_expression("Result::Ok(Option::Some(42), true)").unwrap();
    match result.kind() {
        Expression::Call { function, args } => {
            assert_eq!(format!("{}", function), "Result::Ok");
            assert_eq!(args.len(), 2);
            match args[0].kind() {
                Expression::Call { function, args } => {
                    assert_eq!(format!("{}", function), "Option::Some");
                    assert_eq!(args.len(), 1);
                    assert!(matches!(args[0].kind(), Expression::Literal(_)));
                }
                other => panic!("inner: {:?}", other),
            }
            assert!(matches!(args[1].kind(), Expression::Literal(_)));
        }
        other => panic!("outer: {:?}", other),
    }
}

#[test]
fn call_args_parse_nested_calls_immediately() {
    let result = parse_expression("f(g(1, 2), h())").unwrap();
    match result.kind() {
        Expression::Call { args, .. } => {
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0].kind(), Expression::Call { .. }));
            assert!(matches!(args[1].kind(), Expression::Call { .. }));
        }
        other => panic!("{:?}", other),
    }
}

#[test]
fn call_args_keep_commas_inside_array_literal() {
    match parse_expression("from_array([1, 2, 3, 4, 5])").expect("parse").kind() {
        Expression::Call { args, .. } => {
            assert_eq!(args.len(), 1, "args: {:?}", args);
            match args[0].kind() {
                Expression::ArrayLiteral { elements } => assert_eq!(elements.len(), 5),
                other => panic!("expected array arg, got {:?}", other),
            }
        }
        other => panic!("{:?}", other),
    }
}

#[test]
fn assignment_rhs_invalid_expr_is_error_not_literal() {
    match assignment_rhs("@@@") {
        Err(_) => {}
        Ok(expr) => panic!(
            "invalid assignment RHS must not parse; got {:?}",
            expr
        ),
    }
}

#[test]
fn assignment_rhs_valid_literal() {
    let expr = assignment_rhs("42").expect("valid RHS");
    assert!(matches!(expr.kind(), Expression::Literal(_)));
}

#[test]
fn struct_literal_keeps_commas_inside_array_field() {
    match parse_expression("Matrix { data: [1, 2, 3], rows: 3 }").expect("parse").kind() {
        Expression::StructLiteral { struct_name, fields } => {
            assert_eq!(struct_name, "Matrix");
            assert_eq!(fields.len(), 2, "fields: {:?}", fields);
            assert_eq!(fields[0].0, "data");
            match fields[0].1.kind() {
                Expression::ArrayLiteral { elements } => assert_eq!(elements.len(), 3),
                other => panic!("expected array field, got {:?}", other),
            }
            assert_eq!(fields[1].0, "rows");
        }
        other => panic!("{:?}", other),
    }
}

#[test]
fn struct_literal_keeps_commas_inside_tuple_field() {
    match parse_expression("Pair { values: (10, 20), sum: 30 }").expect("parse").kind() {
        Expression::StructLiteral { struct_name, fields } => {
            assert_eq!(struct_name, "Pair");
            assert_eq!(fields.len(), 2, "fields: {:?}", fields);
            match fields[0].1.kind() {
                Expression::TupleLiteral { elements } => assert_eq!(elements.len(), 2),
                other => panic!("expected tuple field, got {:?}", other),
            }
        }
        other => panic!("{:?}", other),
    }
}

fn anon_fn(params: Vec<(&str, &str)>, return_type: &str) -> Expression {
    use crate::parser::function::{Function, FunctionBody, Parameter};
    Expression::AnonymousFunction {
        func: Box::new(Function {
            type_params: vec![],
            name: "__anon_test".into(),
            parameters: params
                .into_iter()
                .map(|(name, param_type)| Parameter {
                    name: name.into(),
                    param_type: param_type.into(),
                    is_variadic: false,
                })
                .collect(),
            return_type: return_type.into(),
            error_handler: None,
            body: FunctionBody::External,
            is_c: false,
        }),
    }
}

#[test]
fn infer_type_typecast_unknown_target_is_named_not_int() {
    let expr = Expression::TypeCast {
        target_type: "nope".into(),
        value: Box::new(Expression::literal("1")),
    };
    assert_eq!(
        expr.infer_type(),
        Some(crate::types::Type::NamedType {
            name: "nope".into()
        }),
        "unknown TypeCast must not silently become int"
    );
}

#[test]
fn infer_type_typecast_from_str_err_keeps_target_name() {
    let expr = Expression::TypeCast {
        target_type: "List<".into(),
        value: Box::new(Expression::literal("1")),
    };
    assert_eq!(
        expr.infer_type(),
        Some(crate::types::Type::NamedType {
            name: "List<".into()
        }),
        "Type::from_str Err must not fall back to int"
    );
}

#[test]
fn infer_type_anon_fn_from_str_err_is_named_not_int() {
    let ty = anon_fn(vec![("x", "List<")], "Foo<").infer_type();
    match ty {
        Some(crate::types::Type::Function {
            params,
            return_type,
        }) => {
            assert_eq!(
                params,
                vec![crate::types::Type::NamedType {
                    name: "List<".into()
                }]
            );
            assert_eq!(
                *return_type,
                crate::types::Type::NamedType {
                    name: "Foo<".into()
                }
            );
        }
        other => panic!("expected Function type, got {:?}", other),
    }
}

#[test]
fn infer_type_literals_from_syntax() {
    assert_eq!(
        Expression::literal("42").infer_type(),
        Some(crate::types::Type::int())
    );
    assert_eq!(
        Expression::literal("3.14").infer_type(),
        Some(crate::types::Type::float())
    );
    assert_eq!(
        Expression::literal("true").infer_type(),
        Some(crate::types::Type::bool())
    );
    assert_eq!(
        Expression::literal("\"hi\"").infer_type(),
        Some(crate::types::Type::string())
    );
}

#[test]
fn infer_type_unknown_literal_is_none() {
    assert_eq!(
        Expression::literal("not_a_literal_form").infer_type(),
        None,
        "unknown Literal must not invent int"
    );
}

#[test]
fn infer_type_cannot_guess_without_registry() {
    assert_eq!(Expression::var("x").infer_type(), None);
    assert_eq!(
        Expression::Call {
            function: Box::new(Expression::var("f")),
            args: vec![],
        }
        .infer_type(),
        None
    );
    assert_eq!(
        Expression::Member {
            object: Box::new(Expression::var("o")),
            field: "x".into(),
            args: vec![],
        }
        .infer_type(),
        None
    );
    assert_eq!(
        Expression::Index {
            array: Box::new(Expression::var("a")),
            index: Box::new(Expression::literal("0")),
        }
        .infer_type(),
        None
    );
    assert_eq!(
        Expression::ArrayLiteral {
            elements: vec![Expression::literal("1")],
        }
        .infer_type(),
        None
    );
    assert_eq!(
        Expression::TupleLiteral {
            elements: vec![Expression::literal("1")],
        }
        .infer_type(),
        None
    );
}

#[test]
fn infer_type_unary_without_operand_type_is_none() {
    let expr = Expression::Unary {
        op: "-".into(),
        operand: Box::new(Expression::var("x")),
    };
    assert_eq!(expr.infer_type(), None);
}

#[test]
fn infer_type_binary_arithmetic_is_none() {
    let expr = Expression::binary(
        Expression::literal("1"),
        "+",
        Expression::literal("2"),
    );
    assert_eq!(
        expr.infer_type(),
        None,
        "arithmetic must not assume int"
    );
}

#[test]
fn infer_type_compare_and_logic_are_bool() {
    for op in ["==", "!=", "<", "<=", ">", ">=", "&&", "||"] {
        let expr = Expression::binary(Expression::var("a"), op, Expression::var("b"));
        assert_eq!(
            expr.infer_type(),
            Some(crate::types::Type::bool()),
            "{op}"
        );
    }
}

#[test]
fn literal_and_var_helpers_are_unspanned() {
    let lit = Expression::literal("42");
    assert_eq!(lit.span(), Span::new(0, 0));
    assert!(matches!(lit, Expression::Literal(_)));
    assert!(matches!(lit.kind(), Expression::Literal(_)));
    let var = Expression::var("x");
    assert_eq!(var.span(), Span::new(0, 0));
    assert!(matches!(var, Expression::Variable(_)));
}

#[test]
fn parse_expression_wraps_trimmed_slice_span() {
    let expr = parse_expression("42").expect("parse");
    assert_eq!(expr.span(), Span::new(0, 2));
    assert!(matches!(expr.kind(), Expression::Literal(s) if s == "42"));
    let padded = parse_expression("  42  ").expect("parse");
    assert_eq!(padded.span(), Span::new(0, 2));
    assert!(matches!(padded.kind(), Expression::Literal(s) if s == "42"));
}

#[test]
fn parse_expression_at_adds_base_offset() {
    let expr = parse_expression_at("42", 10).expect("parse");
    assert_eq!(expr.span(), Span::new(10, 12));
}

#[test]
fn parse_binary_subexprs_use_slice_offsets() {
    let expr = parse_expression("1+2").expect("parse");
    assert_eq!(expr.span(), Span::new(0, 3));
    match expr.kind() {
        Expression::Binary { left, right, op } => {
            assert_eq!(op, "+");
            assert_eq!(left.span(), Span::new(0, 1));
            assert_eq!(right.span(), Span::new(2, 3));
            assert!(matches!(left.kind(), Expression::Literal(s) if s == "1"));
            assert!(matches!(right.kind(), Expression::Literal(s) if s == "2"));
        }
        other => panic!("expected Binary, got {:?}", other),
    }
}

#[test]
fn parse_program_threads_line_byte_start_into_expr_span() {
    let src = "  foo()\n";
    let program = parse_program(src).expect("program");
    match &program.statements[..] {
        [Statement::Expr(e)] => {
            assert_eq!(e.span(), Span::new(2, 7));
            assert!(matches!(e.kind(), Expression::Call { .. }));
        }
        other => panic!("expected expr stmt, got {:?}", other),
    }
}

fn file_offset_of(src: &str, marker: &str, token: &str) -> usize {
    let start = src.find(marker).expect("marker");
    let rel = marker.find(token).expect("token in marker");
    start + rel
}

#[test]
fn parse_program_if_condition_span_uses_file_offset() {
    let src = "fn f() => int:\n    if 1:\n        return 0\n";
    let program = parse_program(src).expect("program");
    let one = file_offset_of(src, "if 1", "1");
    let cond = match &program.statements[..] {
        [Statement::Function(func)] => match &func.body {
            crate::parser::FunctionBody::Block(body) => match &body[..] {
                [Statement::If(ife)] => ife.condition.span(),
                other => panic!("expected If in fn body, got {:?}", other),
            },
            other => panic!("expected block body, got {:?}", other),
        },
        other => panic!("expected function, got {:?}", other),
    };
    assert_ne!(cond, Span::new(0, 1));
    assert_eq!(cond, Span::new(one, one + 1));
}

#[test]
fn parse_program_let_value_span_uses_file_offset() {
    let src = "fn f() => int:\n    let x: int = 42\n    return x\n";
    let program = parse_program(src).expect("program");
    let forty_two = file_offset_of(src, "= 42", "42");
    let value = match &program.statements[..] {
        [Statement::Function(func)] => match &func.body {
            crate::parser::FunctionBody::Block(body) => match &body[..] {
                [Statement::VariableDecl(decl), Statement::Return(_)] => decl.value.span(),
                other => panic!("expected let+return in fn body, got {:?}", other),
            },
            other => panic!("expected block body, got {:?}", other),
        },
        other => panic!("expected function, got {:?}", other),
    };
    assert_eq!(value, Span::new(forty_two, forty_two + 2));
}

#[test]
fn parse_program_while_for_match_raise_spans_use_file_offset() {
    let src = concat!(
        "fn f() => int:\n",
        "    while 1:\n",
        "        break\n",
        "    for i in 0..3:\n",
        "        break\n",
        "    match 1:\n",
        "        _ => 0\n",
        "    raise E(1)\n",
        "    return 0\n",
    );
    let program = parse_program(src).expect("program");
    let while_one = file_offset_of(src, "while 1", "1");
    let range_zero = file_offset_of(src, "0..3", "0");
    let match_one = file_offset_of(src, "match 1", "1");
    let raise_e = src.find("E(1)").expect("raise expr");
    match &program.statements[..] {
        [Statement::Function(func)] => match &func.body {
            crate::parser::FunctionBody::Block(body) => {
                assert_eq!(body.len(), 5, "got {:?}", body);
                match &body[0] {
                    Statement::While(w) => {
                        assert_eq!(w.condition.span(), Span::new(while_one, while_one + 1));
                    }
                    other => panic!("expected while, got {:?}", other),
                }
                match &body[1] {
                    Statement::For(f) => match &f.iterator {
                        crate::parser::ForIterator::Range { start, .. } => {
                            assert_eq!(start.span(), Span::new(range_zero, range_zero + 1));
                        }
                        other => panic!("expected range, got {:?}", other),
                    },
                    other => panic!("expected for, got {:?}", other),
                }
                match &body[2] {
                    Statement::Match(m) => {
                        assert_eq!(m.value.span(), Span::new(match_one, match_one + 1));
                    }
                    other => panic!("expected match, got {:?}", other),
                }
                match &body[3] {
                    Statement::Raise(r) => {
                        assert_eq!(r.error_expr.span(), Span::new(raise_e, raise_e + 4));
                    }
                    other => panic!("expected raise, got {:?}", other),
                }
            }
            other => panic!("expected block body, got {:?}", other),
        },
        other => panic!("expected function, got {:?}", other),
    }
}

#[test]
fn infer_type_fstring_struct_ctor_assign() {
    let fs = Expression::fstring("hello {x}").expect("fstring");
    assert_eq!(fs.infer_type(), Some(crate::types::Type::string()));
    assert_eq!(
        Expression::StructLiteral {
            struct_name: "Point".into(),
            fields: vec![],
        }
        .infer_type(),
        Some(crate::types::Type::NamedType {
            name: "Point".into()
        })
    );
    assert_eq!(
        Expression::ConstructorCall {
            class_name: "Point".into(),
            args: vec![],
        }
        .infer_type(),
        Some(crate::types::Type::NamedType {
            name: "Point".into()
        })
    );
    assert_eq!(
        Expression::Assign {
            object: Box::new(Expression::var("self")),
            field_name: "x".into(),
            value: Box::new(Expression::literal("1")),
        }
        .infer_type(),
        Some(crate::types::Type::void())
    );
}

#[test]
fn parse_value_as_type() {
    let expr = parse_expression("f as object").expect("as cast");
    match expr.kind() {
        Expression::TypeCast { target_type, value } => {
            assert_eq!(target_type, "object");
            assert!(matches!(value.kind(), Expression::Variable(n) if n == "f"));
        }
        other => panic!("expected TypeCast, got {other:?}"),
    }
}
