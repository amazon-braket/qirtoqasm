// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! AST nodes emitted by the translator.
//!
//! This is intentionally a strict subset of OpenQASM 3: only the
//! constructs `qirtoqasm` produces are modelled. The types align
//! one-to-one with the `openqasm3.ast` classes the current Python
//! implementation uses so the printer can reproduce its bytes.

/// A complete program.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// Language version string (e.g. `"3.0"`).
    pub version: String,
    /// Top-level statements.
    pub statements: Vec<Statement>,
}

/// Top-level or nested block statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// `include "<path>";`
    Include(String),
    /// `qubit[<size>] <name>;`
    QubitDeclaration {
        /// Register width.
        size: u64,
        /// Register identifier (`q`).
        name: String,
    },
    /// `bit[<size>] <name>;`
    ClassicalDeclaration {
        /// Register width.
        bit_size: u64,
        /// Register identifier (`c`).
        name: String,
    },
    /// `int <name> = <init>;` — a scalar signed integer classical variable.
    IntDeclaration {
        /// Variable identifier.
        name: String,
        /// Initial value expression.
        init: Expression,
    },
    /// `<target> = <value>;` — assignment to a previously-declared classical
    /// variable or register element.
    Assignment {
        /// Target identifier (bare name, e.g. a classical int variable).
        target: String,
        /// Assigned value.
        value: Expression,
    },
    /// `output bit[<size>] <name>;`
    IODeclaration {
        /// IO direction.
        io_kind: IoKind,
        /// Bit register width.
        bit_size: u64,
        /// Register name.
        name: String,
    },
    /// Gate application: `[modifier @ ]<name>[(args)] <qubits>;`
    QuantumGate {
        /// Gate modifiers (only `inv @` is used today).
        modifiers: Vec<GateModifier>,
        /// Gate identifier.
        name: String,
        /// Parametric arguments.
        arguments: Vec<Expression>,
        /// Target qubits.
        qubits: Vec<IndexedIdentifier>,
    },
    /// `<target> = measure <qubit>;`
    QuantumMeasurementStatement {
        /// Qubit being measured.
        qubit: IndexedIdentifier,
        /// Classical bit that receives the outcome.
        target: IndexedIdentifier,
    },
    /// `reset <qubit>;`
    QuantumReset(IndexedIdentifier),
    /// `if (<cond>) { <if-body> } [else { <else-body> }]`
    BranchingStatement {
        /// Boolean condition.
        condition: Expression,
        /// Statements run on true.
        if_block: Vec<Statement>,
        /// Statements run on false (empty means no `else` clause).
        else_block: Vec<Statement>,
    },
    /// `while (<cond>) { <body> }`
    WhileLoop {
        /// Loop-continuation condition.
        condition: Expression,
        /// Loop body.
        body: Vec<Statement>,
    },
}

/// Expression tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    /// Bare identifier, e.g. `q` in `q[0]`.
    Identifier(String),
    /// Integer literal.
    Integer(i64),
    /// Float literal.
    Float(f64),
    /// Boolean literal.
    Boolean(bool),
    /// `<collection>[<index>]` in value (expression) position.
    Index {
        /// The collection being indexed.
        collection: Box<Expression>,
        /// The index expression.
        index: Box<Expression>,
    },
    /// Unary prefix operator applied to a subexpression.
    Unary {
        /// Unary operator.
        op: UnaryOp,
        /// Operand.
        expr: Box<Expression>,
    },
    /// Binary infix operator applied to a left and right subexpression.
    Binary {
        /// Binary operator.
        op: BinaryOp,
        /// Left operand.
        lhs: Box<Expression>,
        /// Right operand.
        rhs: Box<Expression>,
    },
}

impl Expression {
    /// Shortcut to wrap in a Box.
    pub fn boxed(self) -> Box<Expression> {
        Box::new(self)
    }
}

/// Construct an [`Expression::Binary`] without writing the
/// `Box::new(...)` wrappers at every call site.
pub fn bin(op: BinaryOp, lhs: Expression, rhs: Expression) -> Expression {
    Expression::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

/// Construct a logical-NOT [`Expression::Unary`].
pub fn not(expr: Expression) -> Expression {
    Expression::Unary {
        op: UnaryOp::Not,
        expr: Box::new(expr),
    }
}

/// Construct an [`Expression::Integer`].
pub fn int(n: i64) -> Expression {
    Expression::Integer(n)
}

/// Assignment-target / qubit-operand form of `<name>[<index>]`.
///
/// Distinct from [`Expression::Index`]: the OpenQASM 3 grammar uses
/// this node only in positions where a reference is required (gate
/// qubit operands, left-hand side of an assignment), never in
/// expression contexts.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedIdentifier {
    /// Register name.
    pub name: String,
    /// Index into the register.
    pub index: Expression,
}

/// Gate modifiers supported by the emitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateModifier {
    /// `inv @` — adjoint the following gate.
    Inv,
}

/// Input/output direction for an IO declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoKind {
    /// `output` storage class.
    Output,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Logical NOT.
    Not,
    /// Arithmetic negation.
    Neg,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Logical OR.
    Or,
    /// Logical AND.
    And,
    /// Bitwise OR.
    BitOr,
    /// Bitwise XOR.
    BitXor,
    /// Bitwise AND.
    BitAnd,
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less.
    Lt,
    /// Less or equal.
    Le,
    /// Greater.
    Gt,
    /// Greater or equal.
    Ge,
    /// Left shift.
    Shl,
    /// Right shift.
    Shr,
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Modulo.
    Mod,
    /// Power.
    Pow,
}

impl BinaryOp {
    /// Source-text spelling, matching the OpenQASM 3 grammar tokens.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Or => "||",
            Self::And => "&&",
            Self::BitOr => "|",
            Self::BitXor => "^",
            Self::BitAnd => "&",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Shl => "<<",
            Self::Shr => ">>",
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Mod => "%",
            Self::Pow => "**",
        }
    }

    /// Return `true` iff the operator's result type is boolean (suitable
    /// for feeding into `&&` / `||` without needing an `== 1` normalisation).
    pub fn is_boolean_producing(self) -> bool {
        matches!(
            self,
            Self::Eq | Self::Ne | Self::Lt | Self::Le | Self::Gt | Self::Ge | Self::And | Self::Or
        )
    }
}

impl UnaryOp {
    /// Source-text spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Not => "!",
            Self::Neg => "-",
        }
    }
}

/// Convenience constructor: `Expression::Index { collection: Identifier(name), index: Integer(idx) }`.
pub fn index_expr(name: impl Into<String>, idx: i64) -> Expression {
    Expression::Index {
        collection: Box::new(Expression::Identifier(name.into())),
        index: Box::new(Expression::Integer(idx)),
    }
}

/// Convenience constructor for a register-index target.
pub fn indexed_ident(name: impl Into<String>, idx: i64) -> IndexedIdentifier {
    IndexedIdentifier {
        name: name.into(),
        index: Expression::Integer(idx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_op_text_and_boolean_flags() {
        let all = [
            (BinaryOp::Or, "||", true),
            (BinaryOp::And, "&&", true),
            (BinaryOp::BitOr, "|", false),
            (BinaryOp::BitXor, "^", false),
            (BinaryOp::BitAnd, "&", false),
            (BinaryOp::Eq, "==", true),
            (BinaryOp::Ne, "!=", true),
            (BinaryOp::Lt, "<", true),
            (BinaryOp::Le, "<=", true),
            (BinaryOp::Gt, ">", true),
            (BinaryOp::Ge, ">=", true),
            (BinaryOp::Shl, "<<", false),
            (BinaryOp::Shr, ">>", false),
            (BinaryOp::Add, "+", false),
            (BinaryOp::Sub, "-", false),
            (BinaryOp::Mul, "*", false),
            (BinaryOp::Div, "/", false),
            (BinaryOp::Mod, "%", false),
            (BinaryOp::Pow, "**", false),
        ];
        for (op, text, is_bool) in all {
            assert_eq!(op.as_str(), text);
            assert_eq!(op.is_boolean_producing(), is_bool, "{op:?}");
        }
    }

    #[test]
    fn unary_op_text() {
        assert_eq!(UnaryOp::Not.as_str(), "!");
        assert_eq!(UnaryOp::Neg.as_str(), "-");
    }

    #[test]
    fn boxed_wraps_in_box() {
        let e = Expression::Integer(1).boxed();
        assert!(matches!(*e, Expression::Integer(1)));
    }

    #[test]
    fn index_expr_and_indexed_ident_helpers() {
        let e = index_expr("c", 7);
        assert!(matches!(e, Expression::Index { .. }));
        let id = indexed_ident("q", 3);
        assert_eq!(id.name, "q");
        assert_eq!(id.index, Expression::Integer(3));
    }

    #[test]
    fn debug_and_clone_round_trip() {
        let p = Program {
            version: "3.0".into(),
            statements: vec![
                Statement::Include("f.inc".into()),
                Statement::QubitDeclaration {
                    size: 1,
                    name: "q".into(),
                },
                Statement::ClassicalDeclaration {
                    bit_size: 1,
                    name: "c".into(),
                },
                Statement::IODeclaration {
                    io_kind: IoKind::Output,
                    bit_size: 1,
                    name: "c".into(),
                },
                Statement::QuantumGate {
                    modifiers: vec![GateModifier::Inv],
                    name: "s".into(),
                    arguments: vec![
                        Expression::Identifier("x".into()),
                        Expression::Float(1.5),
                        Expression::Boolean(true),
                        Expression::Unary {
                            op: UnaryOp::Neg,
                            expr: Box::new(Expression::Integer(2)),
                        },
                    ],
                    qubits: vec![indexed_ident("q", 0)],
                },
                Statement::QuantumMeasurementStatement {
                    qubit: indexed_ident("q", 0),
                    target: indexed_ident("c", 0),
                },
                Statement::QuantumReset(indexed_ident("q", 0)),
                Statement::BranchingStatement {
                    condition: Expression::Boolean(true),
                    if_block: vec![],
                    else_block: vec![],
                },
                Statement::WhileLoop {
                    condition: Expression::Boolean(false),
                    body: vec![],
                },
            ],
        };
        let cloned = p.clone();
        assert_eq!(p, cloned);
        let dbg = format!("{p:?}");
        assert!(dbg.contains("Program"));
    }
}
