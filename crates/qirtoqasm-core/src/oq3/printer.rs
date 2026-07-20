// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Byte-exact OpenQASM 3 pretty-printer.
//!
//! Matches `openqasm3.printer.Printer` (defaults: `indent="  "`,
//! `chain_else_if=True`, `old_measurement=False`) for the emit
//! subset used by the translator. Fixture-parity byte comparison
//! (Requirement 4) hinges on this module.

use std::fmt::Write as _;

use super::ast::*;

/// Two-space indent step; matches `Printer(indent="  ")`.
const INDENT: &str = "  ";

/// Pretty-print a [`Program`] to a string.
pub fn print(program: &Program) -> String {
    let mut out = String::new();
    print_into(&mut out, program);
    out
}

/// Pretty-print a program, appending to an existing buffer.
pub fn print_into(out: &mut String, program: &Program) {
    let mut p = Printer { out, depth: 0 };
    p.program(program);
}

struct Printer<'a> {
    out: &'a mut String,
    depth: usize,
}

impl Printer<'_> {
    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str(INDENT);
        }
    }

    fn write(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn end_statement(&mut self) {
        self.out.push_str(";\n");
    }

    fn end_line(&mut self) {
        self.out.push('\n');
    }

    fn write_statement_line(&mut self, line: &str) {
        self.indent();
        self.out.push_str(line);
        self.end_statement();
    }

    fn program(&mut self, node: &Program) {
        if !node.version.is_empty() {
            self.write_statement_line(&format!("OPENQASM {}", node.version));
        }
        for stmt in &node.statements {
            self.statement(stmt);
        }
    }

    fn statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Include(path) => {
                self.write_statement_line(&format!("include \"{path}\""));
            }
            Statement::QubitDeclaration { size, name } => {
                self.indent();
                self.write(&format!("qubit[{size}] {name}"));
                self.end_statement();
            }
            Statement::ClassicalDeclaration { bit_size, name } => {
                self.indent();
                self.write(&format!("bit[{bit_size}] {name}"));
                self.end_statement();
            }
            Statement::IntDeclaration { name, init } => {
                self.indent();
                self.write(&format!("int {name} = "));
                self.expression(init);
                self.end_statement();
            }
            Statement::Assignment { target, value } => {
                self.indent();
                self.indexed_ident(target);
                self.write(" = ");
                self.expression(value);
                self.end_statement();
            }
            Statement::IODeclaration {
                io_kind: IoKind::Output,
                bit_size,
                name,
            } => {
                self.indent();
                self.write(&format!("output bit[{bit_size}] {name}"));
                self.end_statement();
            }
            Statement::QuantumGate {
                modifiers,
                name,
                arguments,
                qubits,
            } => {
                self.indent();
                for m in modifiers {
                    match m {
                        GateModifier::Inv => self.write("inv @ "),
                    }
                }
                self.write(name);
                if !arguments.is_empty() {
                    self.write("(");
                    for (i, arg) in arguments.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.expression(arg);
                    }
                    self.write(")");
                }
                self.write(" ");
                for (i, q) in qubits.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.indexed_ident(q);
                }
                self.end_statement();
            }
            Statement::QuantumMeasurementStatement { qubit, target } => {
                self.indent();
                self.indexed_ident(target);
                self.write(" = measure ");
                self.indexed_ident(qubit);
                self.end_statement();
            }
            Statement::QuantumReset(q) => {
                self.indent();
                self.write("reset ");
                self.indexed_ident(q);
                self.end_statement();
            }
            Statement::BranchingStatement {
                condition,
                if_block,
                else_block,
            } => {
                self.indent();
                self.write("if (");
                self.expression(condition);
                self.write(") {");
                self.end_line();
                self.depth += 1;
                for s in if_block {
                    self.statement(s);
                }
                self.depth -= 1;
                self.indent();
                self.write("}");
                if !else_block.is_empty() {
                    self.write(" else {");
                    self.end_line();
                    self.depth += 1;
                    for s in else_block {
                        self.statement(s);
                    }
                    self.depth -= 1;
                    self.indent();
                    self.write("}");
                }
                self.end_line();
            }
            Statement::WhileLoop { condition, body } => {
                self.indent();
                self.write("while (");
                self.expression(condition);
                self.write(") {");
                self.end_line();
                self.depth += 1;
                for s in body {
                    self.statement(s);
                }
                self.depth -= 1;
                self.indent();
                self.write("}");
                self.end_line();
            }
        }
    }

    fn indexed_ident(&mut self, id: &IndexedIdentifier) {
        self.write(&id.name);
        if let Some(idx) = &id.index {
            self.write("[");
            self.expression(idx);
            self.write("]");
        }
    }

    fn expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Identifier(name) => self.write(name),
            Expression::Integer(v) => {
                let mut tmp = String::new();
                let _ = write!(tmp, "{v}");
                self.write(&tmp);
            }
            Expression::Boolean(v) => self.write(if *v { "true" } else { "false" }),
            Expression::Float(v) => self.write(&format_float(*v)),
            Expression::Index { collection, index } => {
                // openqasm3 printer: paren the collection when its precedence
                // is strictly less than the IndexExpression's precedence.
                let our_prec = precedence(expr);
                let coll_prec = precedence(collection);
                if coll_prec < our_prec {
                    self.write("(");
                    self.expression(collection);
                    self.write(")");
                } else {
                    self.expression(collection);
                }
                self.write("[");
                self.expression(index);
                self.write("]");
            }
            Expression::Unary { op, expr: inner } => {
                // openqasm3 prints unary ops by writing the operator name directly.
                // The operator's "name" attribute for Not is "!" -> that's a problem
                // since openqasm3 uses `ast.UnaryOperator["!"]` whose `.name` attribute
                // is actually "!". We match that output: the spelling of the op.
                self.write(op.as_str());
                let our_prec = unary_precedence();
                let inner_prec = precedence(inner);
                if our_prec >= inner_prec {
                    self.write("(");
                    self.expression(inner);
                    self.write(")");
                } else {
                    self.expression(inner);
                }
            }
            Expression::Binary { op, lhs, rhs } => {
                let our_prec = binary_precedence(*op);
                // Parenthesise the lhs when its precedence is strictly
                // lower than ours, or equal-precedence but a different
                // operator (e.g. `(a == b) != c` for non-associative
                // `==` / `!=`). Same-op left-associativity needs no parens.
                let lhs_prec = precedence(lhs);
                let lhs_same_op =
                    matches!(lhs.as_ref(), Expression::Binary { op: lop, .. } if lop == op);
                let lhs_needs_paren = lhs_prec < our_prec || (lhs_prec == our_prec && !lhs_same_op);
                if lhs_needs_paren {
                    self.write("(");
                    self.expression(lhs);
                    self.write(")");
                } else {
                    self.expression(lhs);
                }
                self.write(" ");
                self.write(op.as_str());
                self.write(" ");
                let rhs_prec = precedence(rhs);
                if rhs_prec <= our_prec {
                    self.write("(");
                    self.expression(rhs);
                    self.write(")");
                } else {
                    self.expression(rhs);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Precedence table (mirrors openqasm3/properties.py::_BINARY_PRECEDENCE_TABLE)
// ---------------------------------------------------------------------------

fn precedence(expr: &Expression) -> u32 {
    match expr {
        Expression::Identifier(_)
        | Expression::Integer(_)
        | Expression::Float(_)
        | Expression::Boolean(_) => 14,
        Expression::Index { .. } => 13,
        Expression::Unary { .. } => unary_precedence(),
        Expression::Binary { op, .. } => binary_precedence(*op),
    }
}

fn unary_precedence() -> u32 {
    11
}

fn binary_precedence(op: BinaryOp) -> u32 {
    match op {
        BinaryOp::Or => 1,
        BinaryOp::And => 2,
        BinaryOp::BitOr => 3,
        BinaryOp::BitXor => 4,
        BinaryOp::BitAnd => 5,
        BinaryOp::Eq | BinaryOp::Ne => 6,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 7,
        BinaryOp::Shl | BinaryOp::Shr => 8,
        BinaryOp::Add | BinaryOp::Sub => 9,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 10,
        BinaryOp::Pow => 12,
    }
}

/// Format an `f64` so the output matches Python's `str(float)` byte-for-byte
/// for every value the translator emits.
///
/// Python's `str(float)` (aka `repr` for floats) produces the shortest
/// decimal that round-trips. Rust's `f64::to_string` implementation
/// (since 1.55) uses the same Grisu/Ryu algorithm and produces the
/// same bytes for all finite values **except** whole-number values,
/// where Rust emits `"0"` and Python emits `"0.0"`, `"1"` vs `"1.0"`,
/// etc. We post-fix that single divergence here.
pub fn format_float(value: f64) -> String {
    if value.is_nan() {
        // Python prints `nan` for `str(float("nan"))`.
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf".to_string()
        } else {
            "inf".to_string()
        };
    }
    let s = value.to_string();
    // If the Rust formatter produced a pure integer representation (no '.',
    // no 'e', no 'E'), append ".0" so we match Python's `str(float)` which
    // always includes the decimal point for finite floats.
    if !s.bytes().any(|b| b == b'.' || b == b'e' || b == b'E') {
        format!("{s}.0")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printed(program: Program) -> String {
        print(&program)
    }

    fn bell_program(include_files: Vec<String>) -> Program {
        Program {
            version: "3.0".into(),
            statements: vec![
                Statement::QubitDeclaration {
                    size: 2,
                    name: "q".into(),
                },
                Statement::ClassicalDeclaration {
                    bit_size: 2,
                    name: "c".into(),
                },
                Statement::QuantumGate {
                    modifiers: vec![],
                    name: "h".into(),
                    arguments: vec![],
                    qubits: vec![indexed_ident("q", 0)],
                },
                Statement::QuantumGate {
                    modifiers: vec![],
                    name: "cnot".into(),
                    arguments: vec![],
                    qubits: vec![indexed_ident("q", 0), indexed_ident("q", 1)],
                },
                Statement::QuantumMeasurementStatement {
                    qubit: indexed_ident("q", 0),
                    target: indexed_ident("c", 0),
                },
                Statement::QuantumMeasurementStatement {
                    qubit: indexed_ident("q", 1),
                    target: indexed_ident("c", 1),
                },
            ]
            .into_iter()
            .chain(include_files.into_iter().map(Statement::Include))
            .collect(),
        }
    }

    #[test]
    fn prints_bell_state_fixture_byte_for_byte() {
        let prog = bell_program(vec![]);
        let expected = "\
OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cnot q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];
";
        assert_eq!(printed(prog), expected);
    }

    #[test]
    fn include_directive_prints_before_statements() {
        let prog = Program {
            version: "3.0".into(),
            statements: vec![
                Statement::Include("stdgates.inc".into()),
                Statement::QubitDeclaration {
                    size: 1,
                    name: "q".into(),
                },
            ],
        };
        let expected = "OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[1] q;\n";
        assert_eq!(printed(prog), expected);
    }

    #[test]
    fn output_io_declaration_prefixes_output_keyword() {
        let prog = Program {
            version: "3.0".into(),
            statements: vec![Statement::IODeclaration {
                io_kind: IoKind::Output,
                bit_size: 3,
                name: "c".into(),
            }],
        };
        assert_eq!(printed(prog), "OPENQASM 3.0;\noutput bit[3] c;\n");
    }

    #[test]
    fn gate_with_arguments_and_multiple_qubits_prints_with_commas() {
        let prog = Program {
            version: "3.0".into(),
            statements: vec![Statement::QuantumGate {
                modifiers: vec![],
                name: "rxx".into(),
                arguments: vec![Expression::Float(0.5)],
                qubits: vec![indexed_ident("q", 0), indexed_ident("q", 1)],
            }],
        };
        assert_eq!(printed(prog), "OPENQASM 3.0;\nrxx(0.5) q[0], q[1];\n");
    }

    #[test]
    fn inv_modifier_prefixes_gate_name() {
        let prog = Program {
            version: "3.0".into(),
            statements: vec![Statement::QuantumGate {
                modifiers: vec![GateModifier::Inv],
                name: "s".into(),
                arguments: vec![],
                qubits: vec![indexed_ident("q", 0)],
            }],
        };
        assert_eq!(printed(prog), "OPENQASM 3.0;\ninv @ s q[0];\n");
    }

    #[test]
    fn reset_prints_bare() {
        let prog = Program {
            version: "3.0".into(),
            statements: vec![Statement::QuantumReset(indexed_ident("q", 0))],
        };
        assert_eq!(printed(prog), "OPENQASM 3.0;\nreset q[0];\n");
    }

    #[test]
    fn assignment_with_bare_name_target_prints_without_brackets() {
        // Bare-name lvalue: `IndexedIdentifier { index: None, .. }`
        // should render as `k` rather than `k[…]`.
        let prog = Program {
            version: "3.0".into(),
            statements: vec![
                Statement::IntDeclaration {
                    name: "k".into(),
                    init: Expression::Integer(0),
                },
                Statement::Assignment {
                    target: ident("k"),
                    value: Expression::Integer(1),
                },
            ],
        };
        let expected = "OPENQASM 3.0;\nint k = 0;\nk = 1;\n";
        assert_eq!(printed(prog), expected);
    }

    #[test]
    fn assignment_with_indexed_target_prints_with_brackets() {
        // Indexed lvalue: `IndexedIdentifier { index: Some(_), .. }`
        // should render as `c[0]`.
        let prog = Program {
            version: "3.0".into(),
            statements: vec![
                Statement::ClassicalDeclaration {
                    bit_size: 1,
                    name: "c".into(),
                },
                Statement::Assignment {
                    target: indexed_ident("c", 0),
                    value: Expression::Boolean(true),
                },
            ],
        };
        let expected = "OPENQASM 3.0;\nbit[1] c;\nc[0] = true;\n";
        assert_eq!(printed(prog), expected);
    }

    #[test]
    fn if_then_block_has_no_else() {
        let prog = Program {
            version: "3.0".into(),
            statements: vec![Statement::BranchingStatement {
                condition: index_expr("c", 0),
                if_block: vec![Statement::QuantumGate {
                    modifiers: vec![],
                    name: "x".into(),
                    arguments: vec![],
                    qubits: vec![indexed_ident("q", 1)],
                }],
                else_block: vec![],
            }],
        };
        let expected = "OPENQASM 3.0;\nif (c[0]) {\n  x q[1];\n}\n";
        assert_eq!(printed(prog), expected);
    }

    #[test]
    fn if_else_block_prints_else_brace_on_same_line() {
        let prog = Program {
            version: "3.0".into(),
            statements: vec![Statement::BranchingStatement {
                condition: index_expr("c", 0),
                if_block: vec![Statement::QuantumGate {
                    modifiers: vec![],
                    name: "x".into(),
                    arguments: vec![],
                    qubits: vec![indexed_ident("q", 1)],
                }],
                else_block: vec![Statement::QuantumGate {
                    modifiers: vec![],
                    name: "z".into(),
                    arguments: vec![],
                    qubits: vec![indexed_ident("q", 1)],
                }],
            }],
        };
        let expected = "OPENQASM 3.0;\nif (c[0]) {\n  x q[1];\n} else {\n  z q[1];\n}\n";
        assert_eq!(printed(prog), expected);
    }

    #[test]
    fn compound_and_condition_renders_with_expected_parens() {
        // Matches qsharp_and.qasm: `if (c[0] == 1 && c[1] == 1) {`
        let cond = Expression::Binary {
            op: BinaryOp::And,
            lhs: Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(1)),
            }
            .boxed(),
            rhs: Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 1)),
                rhs: Box::new(Expression::Integer(1)),
            }
            .boxed(),
        };
        let prog = Program {
            version: "3.0".into(),
            statements: vec![Statement::BranchingStatement {
                condition: cond,
                if_block: vec![],
                else_block: vec![],
            }],
        };
        // An empty if-block still gets its braces.
        let expected = "OPENQASM 3.0;\nif (c[0] == 1 && c[1] == 1) {\n}\n";
        assert_eq!(printed(prog), expected);
    }

    #[test]
    fn left_associative_chain_ands_without_extra_parens() {
        // (c[0] == 1 && c[1] == 1) && c[2] == 1
        let first = Expression::Binary {
            op: BinaryOp::And,
            lhs: Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(1)),
            }
            .boxed(),
            rhs: Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 1)),
                rhs: Box::new(Expression::Integer(1)),
            }
            .boxed(),
        };
        let chained = Expression::Binary {
            op: BinaryOp::And,
            lhs: Box::new(first),
            rhs: Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 2)),
                rhs: Box::new(Expression::Integer(1)),
            }
            .boxed(),
        };
        let prog = Program {
            version: "3.0".into(),
            statements: vec![Statement::BranchingStatement {
                condition: chained,
                if_block: vec![],
                else_block: vec![],
            }],
        };
        let expected = "OPENQASM 3.0;\nif (c[0] == 1 && c[1] == 1 && c[2] == 1) {\n}\n";
        assert_eq!(printed(prog), expected);
    }

    #[test]
    fn while_loop_prints_with_body() {
        let prog = Program {
            version: "3.0".into(),
            statements: vec![Statement::WhileLoop {
                condition: Expression::Boolean(true),
                body: vec![Statement::QuantumGate {
                    modifiers: vec![],
                    name: "h".into(),
                    arguments: vec![],
                    qubits: vec![indexed_ident("q", 0)],
                }],
            }],
        };
        let expected = "OPENQASM 3.0;\nwhile (true) {\n  h q[0];\n}\n";
        assert_eq!(printed(prog), expected);
    }

    #[test]
    fn float_integer_valued_gets_trailing_decimal() {
        assert_eq!(format_float(0.0), "0.0");
        assert_eq!(format_float(-0.0), "-0.0");
        assert_eq!(format_float(1.0), "1.0");
        assert_eq!(format_float(-1.0), "-1.0");
    }

    #[test]
    #[allow(clippy::approx_constant)]
    // The literals below are the byte-exact decimals we must
    // reproduce to match Python's `float.__repr__`; they happen to
    // equal π/2 and π but we aren't doing math with them.
    fn float_fractional_matches_python_repr() {
        assert_eq!(format_float(0.1), "0.1");
        assert_eq!(format_float(1.5707963267948966), "1.5707963267948966");
        assert_eq!(format_float(-1.5707963267948966), "-1.5707963267948966");
        assert_eq!(format_float(3.141592653589793), "3.141592653589793");
    }

    #[test]
    fn float_nonfinite_matches_python_repr() {
        assert_eq!(format_float(f64::NAN), "nan");
        assert_eq!(format_float(f64::INFINITY), "inf");
        assert_eq!(format_float(f64::NEG_INFINITY), "-inf");
    }

    #[test]
    fn unary_not_bool_literal_needs_parens_on_same_precedence() {
        // `!true` — bool literal has higher precedence than unary, so no parens.
        let expr = Expression::Unary {
            op: UnaryOp::Not,
            expr: Box::new(Expression::Boolean(true)),
        };
        let prog = Program {
            version: "".into(),
            statements: vec![Statement::BranchingStatement {
                condition: expr,
                if_block: vec![],
                else_block: vec![],
            }],
        };
        assert_eq!(print(&prog), "if (!true) {\n}\n");
    }

    #[test]
    fn index_of_parenthesised_binary_paren_only_when_lower_precedence() {
        // `(c[0] + 1)` wrapped as an index-collection: IndexExpression has prec 13,
        // binary Add has prec 9, so the printer MUST parenthesise the collection.
        let expr = Expression::Index {
            collection: Expression::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(1)),
            }
            .boxed(),
            index: Box::new(Expression::Integer(0)),
        };
        // Embed in a condition so the printer is exercised via BranchingStatement.
        let prog = Program {
            version: "".into(),
            statements: vec![Statement::BranchingStatement {
                condition: expr,
                if_block: vec![],
                else_block: vec![],
            }],
        };
        assert_eq!(print(&prog), "if ((c[0] + 1)[0]) {\n}\n");
    }
}

#[cfg(test)]
mod final_coverage {
    use super::*;

    fn wrap_cond(cond: Expression) -> String {
        let prog = Program {
            version: "".into(),
            statements: vec![Statement::BranchingStatement {
                condition: cond,
                if_block: vec![],
                else_block: vec![],
            }],
        };
        print(&prog)
    }

    #[test]
    fn every_binary_operator_has_precedence() {
        use BinaryOp::*;
        let ops = [
            Or, And, BitOr, BitXor, BitAnd, Eq, Ne, Lt, Le, Gt, Ge, Shl, Shr, Add, Sub, Mul, Div,
            Mod, Pow,
        ];
        for op in ops {
            let p = binary_precedence(op);
            assert!((1..=12).contains(&p), "{op:?}");
        }
    }

    #[test]
    fn printer_wraps_unary_with_parens_when_inner_has_lower_precedence() {
        // `!(a && b)` — unary has higher precedence than &&, so &&
        // must be parenthesised.
        let expr = Expression::Unary {
            op: UnaryOp::Not,
            expr: Box::new(Expression::Binary {
                op: BinaryOp::And,
                lhs: Box::new(Expression::Boolean(true)),
                rhs: Box::new(Expression::Boolean(false)),
            }),
        };
        let out = wrap_cond(expr);
        assert!(out.contains("!(true && false)"), "{out}");
    }

    #[test]
    fn printer_wraps_binary_with_parens_when_lhs_has_lower_prec() {
        // `(a + b) * c`
        let expr = Expression::Binary {
            op: BinaryOp::Mul,
            lhs: Box::new(Expression::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(Expression::Integer(1)),
                rhs: Box::new(Expression::Integer(2)),
            }),
            rhs: Box::new(Expression::Integer(3)),
        };
        let out = wrap_cond(expr);
        assert!(out.contains("(1 + 2) * 3"), "{out}");
    }

    #[test]
    fn printer_wraps_binary_with_parens_when_rhs_has_equal_prec() {
        // `a + (b + c)` — left-associative, rhs of equal prec gets parens.
        let expr = Expression::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(Expression::Integer(1)),
            rhs: Box::new(Expression::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(Expression::Integer(2)),
                rhs: Box::new(Expression::Integer(3)),
            }),
        };
        let out = wrap_cond(expr);
        assert!(out.contains("1 + (2 + 3)"), "{out}");
    }

    #[test]
    fn every_binary_op_renders_with_expected_spelling() {
        let pairs = [
            (BinaryOp::Or, "||"),
            (BinaryOp::And, "&&"),
            (BinaryOp::BitOr, "|"),
            (BinaryOp::BitXor, "^"),
            (BinaryOp::BitAnd, "&"),
            (BinaryOp::Eq, "=="),
            (BinaryOp::Ne, "!="),
            (BinaryOp::Lt, "<"),
            (BinaryOp::Le, "<="),
            (BinaryOp::Gt, ">"),
            (BinaryOp::Ge, ">="),
            (BinaryOp::Shl, "<<"),
            (BinaryOp::Shr, ">>"),
            (BinaryOp::Add, "+"),
            (BinaryOp::Sub, "-"),
            (BinaryOp::Mul, "*"),
            (BinaryOp::Div, "/"),
            (BinaryOp::Mod, "%"),
            (BinaryOp::Pow, "**"),
        ];
        for (op, spelling) in pairs {
            let out = wrap_cond(Expression::Binary {
                op,
                lhs: Box::new(Expression::Identifier("a".into())),
                rhs: Box::new(Expression::Identifier("b".into())),
            });
            assert!(out.contains(&format!(" {spelling} ")), "{op:?} -> {out:?}");
        }
    }

    #[test]
    fn precedence_of_unary_subexpr() {
        // Unary inside an index [...]: Unary has prec 11, IndexExpression 13.
        // Since 11 < 13, the collection (the unary) must be parenthesised.
        let expr = Expression::Index {
            collection: Box::new(Expression::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(Expression::Integer(1)),
            }),
            index: Box::new(Expression::Integer(0)),
        };
        let out = wrap_cond(expr);
        assert!(out.contains("(-1)[0]"), "{out}");
    }
}
