use super::{
    BreakStmt, ClassDef, ContinueStmt, EnumDef, ForLoop, Function, IfExpr, Import, MainEntry,
    MatchExpr, MemoryOp, MultiLineComment, RaiseStmt, ReturnStmt, SingleLineComment, TypeDecl,
    VariableDecl, WhileLoop,
};

#[derive(Debug, PartialEq, Clone)]
/// Represents a statement in the Coffee programming language
/// 
/// This enumeration contains all possible statement types that can appear in
/// Coffee source code. Each variant corresponds to a different syntactic
/// construct in the language, from simple expressions to complex control flow
/// structures. The AST (Abstract Syntax Tree) of a parsed Coffee program is
/// composed of these statement nodes.
/// 
/// The Statement enum provides a unified representation of all Coffee constructs,
/// allowing the compiler to process different language elements in a consistent
/// way during semantic analysis, type checking, and code generation phases.
pub enum Statement {
    /// Import statement (e.g., `use printf in libc of c`)
    Import(Import),
    /// Function definition (e.g., `fn add(x: int, y: int) => int:`)
    Function(Function),
    /// Main entry point (e.g., `main(add(1, 2))`)
    Main(MainEntry),
    /// If expression/block (e.g., `if condition: ... elif condition: ... else: ...`)
    If(IfExpr),
    /// While loop (e.g., `while condition: ...`)
    While(WhileLoop),
    /// Match expression (e.g., `match value: pattern1 => result1, pattern2 => result2`)
    Match(MatchExpr),
    /// For loop (e.g., `for item in collection: ...`)
    For(ForLoop),
    /// Variable declaration (e.g., `let x: int = 5`)
    VariableDecl(VariableDecl),
    /// Assignment statement (e.g., `x = x + 1`)
    Assignment(String, crate::parser::expr::Expression),  // (variable_name, value_expression)
    /// Return statement (e.g., `return value`)
    Return(ReturnStmt),
    /// Break statement (e.g., `break`)
    Break(BreakStmt),
    /// Continue statement (e.g., `continue`)
    Continue(ContinueStmt),
    /// Single-line comment (e.g., `/#/ This is a comment`)
    SingleLineComment(SingleLineComment),
    /// Multi-line comment (e.g., `/#* This is a comment *#/`)
    MultiLineComment(MultiLineComment),
    /// Class definition (e.g., `class MyClass: ...`)
    Class(ClassDef),
    /// Enum definition (e.g., `enum Color: Red, Green, Blue`)
    Enum(EnumDef),
    /// Nominal newtype (e.g., `type FILE: object`)
    TypeDecl(TypeDecl),
    /// Memory operation (e.g., `mv source target`, `clone source target`, `rm var1, var2`)
    MemoryOp(MemoryOp),
    /// Raise statement (e.g., `raise ErrorType(arguments)`)
    Raise(RaiseStmt),
    /// Expression statement (e.g., standalone function call)
    Expr(Box<crate::parser::expr::Expression>),
}
