use super::SemanticAnalyzer;

/// Result of constant folding
#[derive(Debug, Clone, PartialEq)]
pub enum ConstantValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
}

impl ConstantValue {
    /// Get type name of the constant
    pub fn type_name(&self) -> &'static str {
        match self {
            ConstantValue::Int(_) => "int",
            ConstantValue::Float(_) => "float",
            ConstantValue::Bool(_) => "bool",
            ConstantValue::String(_) => "string",
        }
    }

    /// Convert to LLVM-style string representation
    pub fn to_llvm_literal(&self) -> String {
        match self {
            ConstantValue::Int(i) => i.to_string(),
            ConstantValue::Float(f) => f.to_string(),
            ConstantValue::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
            ConstantValue::String(s) => format!("\"{}\"", s),
        }
    }
}

impl SemanticAnalyzer {
    //=============================================================================
    // Constant Folding and Compile-Time Evaluation
    //=============================================================================

    /// Evaluate a constant expression at compile time
    /// Returns Some(value) if the expression can be evaluated as a constant,
    /// None if it cannot be evaluated at compile time
    pub fn eval_constant_expr(&self, expr: &str) -> Option<ConstantValue> {
        let expr = expr.trim();

        // Integer literal
        if let Ok(i) = expr.parse::<i64>() {
            return Some(ConstantValue::Int(i));
        }

        // Float literal
        if let Ok(f) = expr.parse::<f64>() {
            return Some(ConstantValue::Float(f));
        }

        // Boolean literal
        match expr {
            "true" => return Some(ConstantValue::Bool(true)),
            "false" => return Some(ConstantValue::Bool(false)),
            _ => {}
        }

        // String literal
        if expr.starts_with('"') && expr.ends_with('"') {
            let content = &expr[1..expr.len()-1];
            return Some(ConstantValue::String(content.to_string()));
        }

        // Try binary operations with constant folding
        for op in ["+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=", "&&", "||"] {
            if let Some(pos) = expr.find(op) {
                if pos > 0 {
                    let left_str = &expr[..pos].trim();
                    let right_str = &expr[pos + op.len()..].trim();

                    // Try to evaluate both sides recursively
                    if let Some(left) = self.eval_constant_expr(left_str) {
                        if let Some(right) = self.eval_constant_expr(right_str) {
                            return self.eval_constant_binary_op(op, &left, &right).ok();
                        }
                    }
                }
            }
        }

        // Unary operations
        if expr.starts_with('-') {
            let operand_str = &expr[1..].trim();
            if let Some(operand) = self.eval_constant_expr(operand_str) {
                return self.eval_constant_unary_op("-", &operand).ok();
            }
        }

        if expr.starts_with('!') {
            let operand_str = &expr[1..].trim();
            if let Some(operand) = self.eval_constant_expr(operand_str) {
                return self.eval_constant_unary_op("!", &operand).ok();
            }
        }

        // Cannot evaluate as constant
        None
    }

    /// Evaluate binary operation on constants (internal)
    fn eval_constant_binary_op(
        &self,
        op: &str,
        left: &ConstantValue,
        right: &ConstantValue,
    ) -> Result<ConstantValue, String> {
        match (left, right) {
            (ConstantValue::Int(l), ConstantValue::Int(r)) => {
                let result = match op {
                    "+" => l.checked_add(*r)
                        .ok_or_else(|| "Integer overflow in constant expression: addition".to_string())?,
                    "-" => l.checked_sub(*r)
                        .ok_or_else(|| "Integer overflow in constant expression: subtraction".to_string())?,
                    "*" => l.checked_mul(*r)
                        .ok_or_else(|| "Integer overflow in constant expression: multiplication".to_string())?,
                    "/" => {
                        if *r == 0 {
                            return Err("Division by zero in constant expression".to_string());
                        }
                        l / r
                    }
                    "%" => {
                        if *r == 0 {
                            return Err("Modulo by zero in constant expression".to_string());
                        }
                        l % r
                    }
                    "==" => return Ok(ConstantValue::Bool(l == r)),
                    "!=" => return Ok(ConstantValue::Bool(l != r)),
                    "<" => return Ok(ConstantValue::Bool(l < r)),
                    ">" => return Ok(ConstantValue::Bool(l > r)),
                    "<=" => return Ok(ConstantValue::Bool(l <= r)),
                    ">=" => return Ok(ConstantValue::Bool(l >= r)),
                    "&&" => return Ok(ConstantValue::Bool(*l != 0 && *r != 0)),
                    "||" => return Ok(ConstantValue::Bool(*l != 0 || *r != 0)),
                    _ => return Err(format!("Unsupported operator for integers: {}", op)),
                };
                Ok(ConstantValue::Int(result))
            }
            (ConstantValue::Float(l), ConstantValue::Float(r)) => {
                let result = match op {
                    "+" => {
                        let res = l + r;
                        // MEDIUM-2 FIX: Check for overflow/underflow in float addition
                        if res.is_infinite() && !l.is_infinite() && !r.is_infinite() {
                            return Err("Float overflow in constant expression: addition".to_string());
                        }
                        if res.is_nan() {
                            return Err("Float operation produced NaN in constant expression".to_string());
                        }
                        res
                    }
                    "-" => {
                        let res = l - r;
                        // MEDIUM-2 FIX: Check for overflow/underflow in float subtraction
                        if res.is_infinite() && !l.is_infinite() && !r.is_infinite() {
                            return Err("Float overflow in constant expression: subtraction".to_string());
                        }
                        if res.is_nan() {
                            return Err("Float operation produced NaN in constant expression".to_string());
                        }
                        res
                    }
                    "*" => {
                        let res = l * r;
                        // MEDIUM-2 FIX: Check for overflow in float multiplication
                        if res.is_infinite() && !l.is_infinite() && !r.is_infinite() {
                            return Err("Float overflow in constant expression: multiplication".to_string());
                        }
                        if res.is_nan() {
                            return Err("Float operation produced NaN in constant expression".to_string());
                        }
                        res
                    }
                    "/" => {
                        // MEDIUM-2 FIX: Check for division by zero in constant folding
                        if *r == 0.0 {
                            return Err("Division by zero in constant expression".to_string());
                        }
                        let res = l / r;
                        // Check for overflow (can happen with very small divisors)
                        if res.is_infinite() && !l.is_infinite() {
                            return Err("Float overflow in constant expression: division".to_string());
                        }
                        if res.is_nan() {
                            return Err("Float operation produced NaN in constant expression".to_string());
                        }
                        res
                    }
                    "==" => return Ok(ConstantValue::Bool((l - r).abs() < f64::EPSILON)),
                    "!=" => return Ok(ConstantValue::Bool((l - r).abs() >= f64::EPSILON)),
                    "<" => return Ok(ConstantValue::Bool(l < r)),
                    ">" => return Ok(ConstantValue::Bool(l > r)),
                    "<=" => return Ok(ConstantValue::Bool(l <= r)),
                    ">=" => return Ok(ConstantValue::Bool(l >= r)),
                    "&&" => return Ok(ConstantValue::Bool(*l != 0.0 && *r != 0.0)),
                    "||" => return Ok(ConstantValue::Bool(*l != 0.0 || *r != 0.0)),
                    _ => return Err(format!("Unsupported operator for floats: {}", op)),
                };
                Ok(ConstantValue::Float(result))
            }
            (ConstantValue::Bool(l), ConstantValue::Bool(r)) => {
                match op {
                    "==" => Ok(ConstantValue::Bool(l == r)),
                    "!=" => Ok(ConstantValue::Bool(l != r)),
                    "&&" => Ok(ConstantValue::Bool(*l && *r)),
                    "||" => Ok(ConstantValue::Bool(*l || *r)),
                    _ => Err(format!("Unsupported operator for booleans: {}", op)),
                }
            }
            _ => Err(format!("Type mismatch in constant expression: {} and {}",
                left.type_name(), right.type_name())),
        }
    }

    /// Evaluate unary operation on constant (internal)
    fn eval_constant_unary_op(
        &self,
        op: &str,
        operand: &ConstantValue,
    ) -> Result<ConstantValue, String> {
        match operand {
            ConstantValue::Int(i) => {
                match op {
                    "-" => {
                        if *i == i64::MIN {
                            // Negation overflow - return safe default (0)
                            Ok(ConstantValue::Int(0))
                        } else {
                            Ok(ConstantValue::Int(-i))
                        }
                    }
                    "!" => Ok(ConstantValue::Bool(*i == 0)),
                    _ => Err(format!("Unsupported unary operator for int: {}", op)),
                }
            }
            ConstantValue::Float(f) => {
                match op {
                    "-" => Ok(ConstantValue::Float(-f)),
                    "!" => Ok(ConstantValue::Bool(*f == 0.0)),
                    _ => Err(format!("Unsupported unary operator for float: {}", op)),
                }
            }
            ConstantValue::Bool(b) => {
                match op {
                    "!" => Ok(ConstantValue::Bool(!b)),
                    _ => Err(format!("Unsupported unary operator for bool: {}", op)),
                }
            }
            _ => Err(format!("Cannot apply unary operator to type: {}", operand.type_name())),
        }
    }
}
