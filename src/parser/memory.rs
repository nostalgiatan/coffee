use nom::{
    bytes::complete::tag,
    character::complete::{space0, space1},
    branch::alt,
    sequence::{terminated, separated_pair},
    multi::separated_list1,
    combinator::opt,
    IResult, Parser,
};

/// 内存操作语句
///
/// 支持以下操作：
/// - clone x y: 深拷贝，创建独立副本（新值，新生命周期）
/// - copy x y: 浅拷贝，共享引用（指向同一数据）
/// - mv x y: 移动，转移所有权（x失效，y接管）
/// - rm x: 释放，删除变量
/// - rm x, y, z: 批量释放多个变量
/// - clean out: 清理当前作用域所有变量
/// - clean out except x, y: 清理除x,y外的所有变量
/// - clean out x, y, z: 清理指定的多个变量

#[derive(Debug, PartialEq, Clone)]
pub enum MemoryOp {
    /// 深拷贝：clone x y
    /// 创建值的深拷贝，新值有独立的生命周期
    Clone {
        source: String,
        target: String,
    },
    /// 浅拷贝：copy x y
    /// 创建共享引用，两个变量指向同一数据
    Copy {
        source: String,
        target: String,
    },
    /// 移动：mv x y
    /// 转移所有权，源变量不再有效
    Move {
        source: String,
        target: String,
    },
    /// 释放：rm x
    /// 删除变量，释放资源
    Remove {
        target: String,
    },
    /// 批量释放：rm x, y, z
    RemoveMultiple {
        targets: Vec<String>,
    },
    /// 清理作用域：clean out
    CleanOut {
        /// 要清理的变量列表（None表示全部）
        targets: Option<Vec<String>>,
        /// 是否为except模式
        except_mode: bool,
    },
}

pub fn parse_memory_op(input: &str) -> IResult<&str, MemoryOp> {
    let (input, _) = space0(input)?;
    let (input, op) = {
        let mut parser = alt((
            parse_clone,
            parse_move,
            parse_copy,
            parse_remove_multiple,  // Try batch remove BEFORE single remove
            parse_remove,
            parse_clean_out,
        ));
        Parser::parse(&mut parser, input)?
    };
    let (input, _) = space0(input)?;
    Ok((input, op))
}

fn parse_clone(input: &str) -> IResult<&str, MemoryOp> {
    let mut parser1 = terminated(tag("clone"), space1);
    let mut parser2 = separated_pair(identifier, space1, identifier);
    let (input, _) = Parser::parse(&mut parser1, input)?;
    let (input, (source, target)) = Parser::parse(&mut parser2, input)?;
    Ok((input, MemoryOp::Clone {
        source: source.to_string(),
        target: target.to_string(),
    }))
}

fn parse_move(input: &str) -> IResult<&str, MemoryOp> {
    let mut parser1 = terminated(tag("mv"), space1);
    let mut parser2 = separated_pair(identifier, space1, identifier);
    let (input, _) = Parser::parse(&mut parser1, input)?;
    let (input, (source, target)) = Parser::parse(&mut parser2, input)?;
    Ok((input, MemoryOp::Move {
        source: source.to_string(),
        target: target.to_string(),
    }))
}

fn parse_copy(input: &str) -> IResult<&str, MemoryOp> {
    let mut parser1 = terminated(tag("copy"), space1);
    let mut parser2 = separated_pair(identifier, space1, identifier);
    let (input, _) = Parser::parse(&mut parser1, input)?;
    let (input, (source, target)) = Parser::parse(&mut parser2, input)?;
    Ok((input, MemoryOp::Copy {
        source: source.to_string(),
        target: target.to_string(),
    }))
}

fn parse_remove(input: &str) -> IResult<&str, MemoryOp> {
    let mut parser = terminated(tag("rm"), space1);
    let (input, _) = Parser::parse(&mut parser, input)?;
    let (input, target) = identifier(input)?;
    Ok((input, MemoryOp::Remove {
        target: target.to_string(),
    }))
}

/// 解析批量删除：rm x, y, z
fn parse_remove_multiple(input: &str) -> IResult<&str, MemoryOp> {
    let mut parser1 = terminated(tag("rm"), space1);
    let (input, _) = Parser::parse(&mut parser1, input)?;

    // Parse identifiers separated by commas, with optional spaces
    let mut parser2 = separated_list1(
        terminated(tag(","), space0),
        terminated(identifier, space0)
    );
    let (input, targets) = Parser::parse(&mut parser2, input)?;
    Ok((input, MemoryOp::RemoveMultiple {
        targets: targets.iter().map(|s| s.to_string()).collect(),
    }))
}

/// 解析清理作用域：clean out [vars...] 或 clean out except vars...
/// clean out = rm all (批量rm的语法糖)
fn parse_clean_out(input: &str) -> IResult<&str, MemoryOp> {
    let mut parser1 = terminated(tag("clean"), space1);
    let (input, _) = Parser::parse(&mut parser1, input)?;

    let mut parser2 = tag("out");
    let (input, _) = Parser::parse(&mut parser2, input)?;

    let (input, _) = space0(input)?;

    // 检查是否有except
    let mut except_parser = opt(tag("except"));
    let (input, has_except) = Parser::parse(&mut except_parser, input)?;

    let input = if has_except.is_some() {
        let mut parser3 = space1;
        let (input2, _) = Parser::parse(&mut parser3, input)?;
        input2
    } else {
        input
    };

    // 解析变量列表（如果有）
    let mut list_parser = opt(separated_list1(
        terminated(tag(","), space0),
        terminated(identifier, space0)
    ));
    let (input, identifiers) = Parser::parse(&mut list_parser, input)?;

    let targets = if let Some(idents) = identifiers {
        // Filter out empty strings
        let filtered: Vec<&str> = idents.iter().filter(|s| !s.is_empty()).copied().collect();
        if filtered.is_empty() {
            None
        } else {
            Some(filtered.iter().map(|s| s.to_string()).collect())
        }
    } else {
        None
    };

    let except_mode = has_except.is_some();

    // clean out 完全等价于 RemoveMultiple
    // clean out -> RemoveMultiple(all)
    // clean out except x, y -> RemoveMultiple(all - {x, y})
    // clean out x, y, z -> RemoveMultiple([x, y, z])
    // 这里先返回CleanOut，在codegen阶段转换为RemoveMultiple
    Ok((input, MemoryOp::CleanOut {
        targets,
        except_mode,
    }))
}

fn identifier(input: &str) -> IResult<&str, &str> {
    let (input, ident) = take_until_space_or_end(input)?;
    Ok((input, ident))
}

fn take_until_space_or_end(input: &str) -> IResult<&str, &str> {
    for (i, c) in input.char_indices() {
        if c.is_whitespace() || c == ',' {
            return Ok((&input[i..], &input[..i]));
        }
    }
    Ok(("", input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_clone() {
        let result = parse_memory_op("clone x y");
        assert_eq!(result, Ok(("", MemoryOp::Clone {
            source: "x".to_string(),
            target: "y".to_string(),
        })));
    }

    #[test]
    fn test_parse_move() {
        let result = parse_memory_op("mv x y");
        assert_eq!(result, Ok(("", MemoryOp::Move {
            source: "x".to_string(),
            target: "y".to_string(),
        })));
    }

    #[test]
    fn test_parse_copy() {
        let result = parse_memory_op("copy x y");
        assert_eq!(result, Ok(("", MemoryOp::Copy {
            source: "x".to_string(),
            target: "y".to_string(),
        })));
    }

    #[test]
    fn test_parse_remove() {
        let result = parse_memory_op("rm x");
        assert_eq!(result, Ok(("", MemoryOp::RemoveMultiple {
            targets: vec!["x".to_string()],
        })));
    }

    #[test]
    fn test_parse_clone_with_whitespace() {
        let result = parse_memory_op("  clone   x   y  ");
        assert_eq!(result, Ok(("", MemoryOp::Clone {
            source: "x".to_string(),
            target: "y".to_string(),
        })));
    }

    #[test]
    fn test_parse_complex_names() {
        let result = parse_memory_op("clone original_var cloned_var");
        assert_eq!(result, Ok(("", MemoryOp::Clone {
            source: "original_var".to_string(),
            target: "cloned_var".to_string(),
        })));

        let result = parse_memory_op("mv source_ptr target_ptr");
        assert_eq!(result, Ok(("", MemoryOp::Move {
            source: "source_ptr".to_string(),
            target: "target_ptr".to_string(),
        })));
    }
}
