// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Compound-Boolean lowering: `icmp i1`, bitwise `and`/`or`/`xor i1`,
//! `select i1`, integer `add`/`sub`/`mul`, and short-circuit `phi i1`
//! merges.

use std::collections::HashMap;

use crate::error::{QirToQasmError, Result};
use crate::ir::{BinaryI1Op, IntArithOp, Operand, PhiIncoming};
use crate::oq3::ast::*;
use crate::symbols::SymbolTable;

/// Lower an `icmp <pred>` into an OQ3 `BinaryExpression` bound to the
/// instruction's SSA result. All ten LLVM integer predicates are
/// accepted; signed and unsigned families collapse to the same OQ3
/// operator because qirtoqasm operands are always classical bits or
/// small ints that agree under both interpretations.
pub fn lower_icmp_i1(
    result_key: &str,
    predicate: &str,
    lhs: &Operand,
    rhs: &Operand,
    symbols: &mut SymbolTable,
) -> Result<()> {
    let op = match predicate {
        "eq" => BinaryOp::Eq,
        "ne" => BinaryOp::Ne,
        "ult" | "slt" => BinaryOp::Lt,
        "ule" | "sle" => BinaryOp::Le,
        "ugt" | "sgt" => BinaryOp::Gt,
        "uge" | "sge" => BinaryOp::Ge,
        other => {
            return Err(QirToQasmError::unsupported(format!(
                "icmp predicate {:?} is not supported; supported predicates are \
                 [\"eq\", \"ne\", \"ult\", \"slt\", \"ule\", \"sle\", \"ugt\", \"sgt\", \
                 \"uge\", \"sge\"]",
                other
            )));
        }
    };
    let lhs_e = resolve_integer_operand(symbols, lhs)?;
    let rhs_e = resolve_integer_operand(symbols, rhs)?;
    symbols.record_ssa(result_key, bin(op, lhs_e, rhs_e));
    Ok(())
}

/// Lower an i1 binary operation (`xor` / `and` / `or`) to an OpenQASM
/// 3 Boolean expression.
///
/// `xor` recognizes three shapes:
///   * `xor i1 %x, true` / `xor i1 true, %x` — logical NOT, lowered
///     as `result = (x == 0)`.
///   * `xor i1 %x, false` / `xor i1 false, %x` — identity, `result = %x`.
///   * `xor i1 %a, %b` with two non-constant operands — integer XOR on
///     classical bits, semantically equivalent on Booleans to
///     inequality, lowered as `result = (a != b)`.
///
/// `and` and `or` always lower to `&&` / `||` over Boolean-normalized
/// operands.
pub fn lower_binary_i1(
    result_key: &str,
    op: BinaryI1Op,
    lhs: &Operand,
    rhs: &Operand,
    symbols: &mut SymbolTable,
) -> Result<()> {
    if op == BinaryI1Op::Xor {
        // Constant-fold both operand orders:
        //   xor x, true  / xor true, x   →  x == 0   (logical NOT)
        //   xor x, false / xor false, x  →  x        (identity)
        let lhs_const = constant_i1_value(lhs);
        let rhs_const = constant_i1_value(rhs);
        match (lhs_const, rhs_const) {
            (None, Some(true)) | (Some(true), None) => {
                let non_const = if lhs_const.is_none() { lhs } else { rhs };
                let e = resolve_i1_operand(symbols, non_const)?;
                symbols.record_ssa(result_key, bin(BinaryOp::Eq, e, int(0)));
                return Ok(());
            }
            (None, Some(false)) | (Some(false), None) => {
                let non_const = if lhs_const.is_none() { lhs } else { rhs };
                let e = resolve_i1_operand(symbols, non_const)?;
                symbols.record_ssa(result_key, e);
                return Ok(());
            }
            // Both operands constant or both non-constant — fall
            // through to the general path below.
            _ => {}
        }
    }
    let oq3_op = match op {
        BinaryI1Op::Xor => BinaryOp::Ne,
        BinaryI1Op::And => BinaryOp::And,
        BinaryI1Op::Or => BinaryOp::Or,
    };
    let lhs_e = resolve_i1_operand(symbols, lhs)?;
    let rhs_e = resolve_i1_operand(symbols, rhs)?;
    symbols.record_ssa(
        result_key,
        bin(
            oq3_op,
            as_boolean_expression(lhs_e),
            as_boolean_expression(rhs_e),
        ),
    );
    Ok(())
}

/// Lower integer `add` / `sub` / `mul` to an OQ3 arithmetic expression
/// bound to the result SSA. The binding is inlined at each use site, so
/// the operands must resolve to expressions OQ3 can represent (classical
/// register reads, integer constants, or previously bound expressions).
pub fn lower_int_arith(
    result_key: &str,
    op: IntArithOp,
    lhs: &Operand,
    rhs: &Operand,
    symbols: &mut SymbolTable,
) -> Result<()> {
    let oq3_op = match op {
        IntArithOp::Add => BinaryOp::Add,
        IntArithOp::Sub => BinaryOp::Sub,
        IntArithOp::Mul => BinaryOp::Mul,
    };
    let lhs_e = resolve_integer_operand(symbols, lhs)?;
    let rhs_e = resolve_integer_operand(symbols, rhs)?;
    symbols.record_ssa(result_key, bin(oq3_op, lhs_e, rhs_e));
    Ok(())
}

/// Lower `select i1 %cond, i1 %a, i1 %b` to a Boolean expression.
///
/// Six short-circuit / constant-fold shapes are recognized and reduced:
///   * `select %c, %rhs, false`  →  `c && rhs`
///   * `select %c, true,  %rhs`  →  `c || rhs`
///   * `select %c, false, %rhs`  →  `!c && rhs`
///   * `select %c, %rhs, true`   →  `!c || rhs`
///   * `select %c, true,  false` →  `c`
///   * `select %c, false, true`  →  `!c`
///
/// All other shapes fall through to the general
/// `(cond && t) || (!cond && f)` expansion. Non-`i1` value types are
/// rejected with a descriptive error since OpenQASM 3 has no classical
/// ternary.
pub fn lower_select_i1(
    result_key: &str,
    value_type: &str,
    cond: &Operand,
    true_value: &Operand,
    false_value: &Operand,
    symbols: &mut SymbolTable,
) -> Result<()> {
    if value_type == "i32" || value_type == "i64" {
        return lower_select_integer(result_key, cond, true_value, false_value, symbols);
    }
    if value_type != "i1" {
        return Err(QirToQasmError::unsupported(format!(
            "`select i1 %cond, {ty} ..., {ty} ...` is not supported: \
             OpenQASM 3 has no classical ternary expression for \
             value-typed selects. Apply each branch in its own `if` arm, \
             or precompute the value before the circuit.",
            ty = value_type
        )));
    }

    let cond_e = resolve_i1_operand(symbols, cond)?;
    let cond_bool = as_boolean_expression(cond_e);

    let t_const = constant_i1_value(true_value);
    let f_const = constant_i1_value(false_value);

    // Short-circuit / constant-fold shapes:
    //   cond && rhs   ← select %c, %rhs,  false
    //   cond || rhs   ← select %c, true,  %rhs
    //  !cond && rhs   ← select %c, false, %rhs
    //  !cond || rhs   ← select %c, %rhs,  true
    //   cond          ← select %c, true,  false
    //  !cond          ← select %c, false, true
    match (t_const, f_const) {
        (None, Some(false)) => {
            let t = resolve_i1_operand(symbols, true_value)?;
            symbols.record_ssa(
                result_key,
                bin(BinaryOp::And, cond_bool, as_boolean_expression(t)),
            );
            return Ok(());
        }
        (Some(true), None) => {
            let f = resolve_i1_operand(symbols, false_value)?;
            symbols.record_ssa(
                result_key,
                bin(BinaryOp::Or, cond_bool, as_boolean_expression(f)),
            );
            return Ok(());
        }
        (Some(false), None) => {
            let f = resolve_i1_operand(symbols, false_value)?;
            symbols.record_ssa(
                result_key,
                bin(BinaryOp::And, not(cond_bool), as_boolean_expression(f)),
            );
            return Ok(());
        }
        (None, Some(true)) => {
            let t = resolve_i1_operand(symbols, true_value)?;
            symbols.record_ssa(
                result_key,
                bin(BinaryOp::Or, not(cond_bool), as_boolean_expression(t)),
            );
            return Ok(());
        }
        (Some(true), Some(false)) => {
            symbols.record_ssa(result_key, cond_bool);
            return Ok(());
        }
        (Some(false), Some(true)) => {
            symbols.record_ssa(result_key, not(cond_bool));
            return Ok(());
        }
        // Both branches constant with matching values, or both non-
        // constant — fall through to the general expansion.
        _ => {}
    }

    // General shape: (cond && t) || (!cond && f).
    let t_e = resolve_i1_operand(symbols, true_value)?;
    let f_e = resolve_i1_operand(symbols, false_value)?;
    let t_bool = as_boolean_expression(t_e);
    let f_bool = as_boolean_expression(f_e);
    let then_branch = bin(BinaryOp::And, cond_bool.clone(), t_bool);
    let else_branch = bin(BinaryOp::And, not(cond_bool), f_bool);
    symbols.record_ssa(result_key, bin(BinaryOp::Or, then_branch, else_branch));
    Ok(())
}

/// Lower a two-incoming `phi i1` under the short-circuit pattern.
///
/// `pred_conditions` maps predecessor block name → the branching
/// condition of that predecessor's conditional `br`.
pub fn lower_phi_i1_short_circuit(
    result_key: &str,
    incomings: &[PhiIncoming],
    pred_conditions: &HashMap<String, Expression>,
    symbols: &mut SymbolTable,
) -> Result<()> {
    if incomings.len() != 2 {
        return Err(QirToQasmError::unsupported(format!(
            "phi i1 has {} incoming value(s); only the two-incoming \
             short-circuit pattern is supported",
            incomings.len()
        )));
    }

    let mut sc_pred: Option<(&str, bool)> = None;
    let mut rhs_pair: Option<&PhiIncoming> = None;
    for inc in incomings {
        if let Some(v) = constant_i1_value(&inc.value) {
            if sc_pred.is_some() {
                return Err(QirToQasmError::unsupported(
                    "phi i1 has two constant incomings; only one side \
                     of a short-circuit compound may be a constant",
                ));
            }
            sc_pred = Some((inc.pred.as_str(), v));
        } else {
            rhs_pair = Some(inc);
        }
    }
    let (sc_name, sc_value) = sc_pred.ok_or_else(|| {
        QirToQasmError::unsupported(
            "phi i1 does not match the short-circuit Boolean pattern: \
             expected exactly one constant incoming and one SSA incoming",
        )
    })?;
    let rhs = rhs_pair.ok_or_else(|| {
        QirToQasmError::unsupported(
            "phi i1 does not match the short-circuit Boolean pattern: \
             expected exactly one constant incoming and one SSA incoming",
        )
    })?;

    let lhs_expr = pred_conditions.get(sc_name).cloned().ok_or_else(|| {
        QirToQasmError::unsupported(format!(
            "phi i1 short-circuit predecessor {:?} did not record a \
             branching condition; expected the predecessor to end in a \
             conditional ``br`` on an SSA value",
            sc_name
        ))
    })?;

    let rhs_expr = match &rhs.value {
        Operand::Ssa(id) => symbols.lookup_ssa(id)?,
        _ => {
            return Err(QirToQasmError::unsupported(
                "phi i1 rhs incoming must be an SSA reference",
            ));
        }
    };

    let op = if sc_value {
        BinaryOp::Or
    } else {
        BinaryOp::And
    };
    symbols.record_ssa(
        result_key,
        bin(
            op,
            as_boolean_expression(lhs_expr),
            as_boolean_expression(rhs_expr),
        ),
    );
    Ok(())
}

fn resolve_i1_operand(symbols: &SymbolTable, op: &Operand) -> Result<Expression> {
    match op {
        Operand::ConstBool(b) => Ok(int(if *b { 1 } else { 0 })),
        Operand::ConstInt(n) => {
            if *n == 0 || *n == 1 {
                Ok(int(*n as i64))
            } else {
                Err(QirToQasmError::unsupported(format!(
                    "could not interpret i1 constant operand {n:?}"
                )))
            }
        }
        Operand::Ssa(id) => symbols.lookup_ssa(id),
        _ => Err(QirToQasmError::unsupported(format!(
            "could not interpret i1 constant operand {:?}",
            debug_operand(op)
        ))),
    }
}

/// Lower `select i1 %cond, iN A, iN B` to the inline arithmetic
/// expression `(cond_i) * A + (1 - cond_i) * B`, where `cond_i` is
/// the i1 condition viewed as an integer (0 or 1). OpenQASM 3 has no
/// ternary operator, but this arithmetic form is accepted by
/// Braket's local simulator in classical contexts.
///
/// Chains of selects (e.g. `%a = select i1 %x, i32 2, i32 1; %b =
/// select i1 %y, i32 %a, i32 %c`) compose naturally: `%a` binds to
/// `x*2 + (1-x)*1`, and substituting into `%b`'s formula gives the
/// nested expression that ultimately feeds `add`/`icmp`/etc.
///
/// This shape appears in adaptive-profile codegen as a shorter
/// alternative to the equivalent `phi i32` merges for the same source
/// pattern.
pub fn lower_select_integer(
    result_key: &str,
    cond: &Operand,
    true_value: &Operand,
    false_value: &Operand,
    symbols: &mut SymbolTable,
) -> Result<()> {
    // The condition must be an i1 SSA (or constant). `resolve_i1_operand`
    // returns either a boolean-producing comparison (which evaluates to
    // 0/1 in Braket's classical arithmetic context) or a bare 0/1
    // integer expression — both are valid operands for the arithmetic
    // encoding below.
    let cond_as_int = resolve_i1_operand(symbols, cond)?;
    let t_e = resolve_integer_operand(symbols, true_value)?;
    let f_e = resolve_integer_operand(symbols, false_value)?;

    // Build `(cond_as_int) * t + (1 - cond_as_int) * f`.
    let lhs = bin(BinaryOp::Mul, cond_as_int.clone(), t_e);
    let one_minus_cond = bin(BinaryOp::Sub, int(1), cond_as_int);
    let rhs = bin(BinaryOp::Mul, one_minus_cond, f_e);
    symbols.record_ssa(result_key, bin(BinaryOp::Add, lhs, rhs));
    Ok(())
}

/// Resolve an operand appearing in a general integer-typed context
/// (`icmp`, `add`/`sub`/`mul`). Accepts integer and Boolean constants
/// and SSA references; anything else errors.
fn resolve_integer_operand(symbols: &SymbolTable, op: &Operand) -> Result<Expression> {
    match op {
        Operand::ConstBool(b) => Ok(int(if *b { 1 } else { 0 })),
        Operand::ConstInt(n) => i64::try_from(*n).map(int).map_err(|_| {
            QirToQasmError::unsupported(format!(
                "integer constant {n} does not fit in i64; wider integer \
                     constants are not yet supported"
            ))
        }),
        Operand::Ssa(id) => symbols.lookup_ssa(id),
        _ => Err(QirToQasmError::unsupported(format!(
            "could not interpret integer operand {:?}",
            debug_operand(op)
        ))),
    }
}

fn constant_i1_value(op: &Operand) -> Option<bool> {
    match op {
        Operand::ConstBool(b) => Some(*b),
        Operand::ConstInt(1) => Some(true),
        Operand::ConstInt(0) => Some(false),
        _ => None,
    }
}

fn debug_operand(op: &Operand) -> String {
    format!("{op:?}")
}

/// Normalise an expression to a form Braket's simulator accepts as a
/// Boolean operand (wrap bare integer-typed values as `<expr> == 1`).
pub fn as_boolean_expression(expr: Expression) -> Expression {
    match &expr {
        Expression::Binary { op, .. } if op.is_boolean_producing() => expr,
        _ => bin(BinaryOp::Eq, expr, int(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    #[test]
    fn icmp_eq_binds_equality() {
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        lower_icmp_i1(
            "r",
            "eq",
            &Operand::Ssa("a".into()),
            &Operand::ConstBool(false),
            &mut s,
        )
        .unwrap();
        let e = s.lookup_ssa("r").unwrap();
        assert_eq!(
            e,
            Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(0)),
            }
        );
    }

    #[test]
    fn icmp_unsupported_predicate_errors_with_pinned_substring() {
        // With all 10 valid LLVM icmp predicates now accepted, only
        // malformed / unknown tokens should produce the "not supported"
        // message. (A caller that passes a garbage predicate is a bug;
        // we still want a readable error instead of a panic.)
        let mut s = SymbolTable::new();
        let err = lower_icmp_i1(
            "r",
            "bogus",
            &Operand::ConstBool(false),
            &Operand::ConstBool(true),
            &mut s,
        )
        .unwrap_err();
        assert!(err.to_string().contains("icmp predicate"));
        assert!(err.to_string().contains("is not supported"));
    }

    #[test]
    fn xor_true_binds_logical_not() {
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        lower_binary_i1(
            "r",
            BinaryI1Op::Xor,
            &Operand::Ssa("a".into()),
            &Operand::ConstBool(true),
            &mut s,
        )
        .unwrap();
        let e = s.lookup_ssa("r").unwrap();
        assert_eq!(
            e,
            Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(0)),
            }
        );
    }

    #[test]
    fn xor_false_rhs_folds_to_identity() {
        // `xor i1 %a, false` folds to just `a`.
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        lower_binary_i1(
            "r",
            BinaryI1Op::Xor,
            &Operand::Ssa("a".into()),
            &Operand::ConstBool(false),
            &mut s,
        )
        .unwrap();
        assert_eq!(s.lookup_ssa("r").unwrap(), index_expr("c", 0));
    }

    #[test]
    fn xor_true_lhs_folds_to_logical_not() {
        // `xor i1 true, %a` folds to logical NOT of `a`.
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        lower_binary_i1(
            "r",
            BinaryI1Op::Xor,
            &Operand::ConstBool(true),
            &Operand::Ssa("a".into()),
            &mut s,
        )
        .unwrap();
        assert_eq!(
            s.lookup_ssa("r").unwrap(),
            Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(0)),
            }
        );
    }

    #[test]
    fn xor_false_lhs_folds_to_identity() {
        // Swapped mirror of `xor i1 %a, false`; folds to just `a`.
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        lower_binary_i1(
            "r",
            BinaryI1Op::Xor,
            &Operand::ConstBool(false),
            &Operand::Ssa("a".into()),
            &mut s,
        )
        .unwrap();
        assert_eq!(s.lookup_ssa("r").unwrap(), index_expr("c", 0));
    }

    #[test]
    fn and_two_ssa_operands_lowers_as_boolean_and() {
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        s.record_ssa("b", index_expr("c", 1));
        lower_binary_i1(
            "r",
            BinaryI1Op::And,
            &Operand::Ssa("a".into()),
            &Operand::Ssa("b".into()),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        let wrap = |idx: i64| Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", idx)),
            rhs: Box::new(Expression::Integer(1)),
        };
        assert_eq!(
            got,
            Expression::Binary {
                op: BinaryOp::And,
                lhs: Box::new(wrap(0)),
                rhs: Box::new(wrap(1)),
            }
        );
    }

    #[test]
    fn or_two_ssa_operands_lowers_as_boolean_or() {
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        s.record_ssa("b", index_expr("c", 1));
        lower_binary_i1(
            "r",
            BinaryI1Op::Or,
            &Operand::Ssa("a".into()),
            &Operand::Ssa("b".into()),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        let wrap = |idx: i64| Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", idx)),
            rhs: Box::new(Expression::Integer(1)),
        };
        assert_eq!(
            got,
            Expression::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(wrap(0)),
                rhs: Box::new(wrap(1)),
            }
        );
    }

    #[test]
    fn select_i1_short_circuit_and_reduces_to_and() {
        // clang emits `cond && rhs` as `select i1 %cond, i1 %rhs, i1 false`.
        // The lowering should recognize that shape and emit `cond && rhs`
        // rather than the fully-general `(cond && t) || (!cond && f)`.
        let mut s = SymbolTable::new();
        s.record_ssa("cond", index_expr("c", 0));
        s.record_ssa("rhs", index_expr("c", 1));
        lower_select_i1(
            "r",
            "i1",
            &Operand::Ssa("cond".into()),
            &Operand::Ssa("rhs".into()),
            &Operand::ConstBool(false),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        let wrap = |idx: i64| Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", idx)),
            rhs: Box::new(Expression::Integer(1)),
        };
        assert_eq!(
            got,
            Expression::Binary {
                op: BinaryOp::And,
                lhs: Box::new(wrap(0)),
                rhs: Box::new(wrap(1)),
            }
        );
    }

    #[test]
    fn select_i1_short_circuit_or_reduces_to_or() {
        // `cond || rhs` → `select i1 %cond, i1 true, i1 %rhs`.
        let mut s = SymbolTable::new();
        s.record_ssa("cond", index_expr("c", 0));
        s.record_ssa("rhs", index_expr("c", 1));
        lower_select_i1(
            "r",
            "i1",
            &Operand::Ssa("cond".into()),
            &Operand::ConstBool(true),
            &Operand::Ssa("rhs".into()),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        let wrap = |idx: i64| Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", idx)),
            rhs: Box::new(Expression::Integer(1)),
        };
        assert_eq!(
            got,
            Expression::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(wrap(0)),
                rhs: Box::new(wrap(1)),
            }
        );
    }

    #[test]
    fn select_i1_inverse_short_circuit_and_reduces_to_not_cond_and_rhs() {
        // `!cond && rhs` → `select i1 %cond, i1 false, i1 %rhs`.
        let mut s = SymbolTable::new();
        s.record_ssa("cond", index_expr("c", 0));
        s.record_ssa("rhs", index_expr("c", 1));
        lower_select_i1(
            "r",
            "i1",
            &Operand::Ssa("cond".into()),
            &Operand::ConstBool(false),
            &Operand::Ssa("rhs".into()),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        let wrap_eq_1 = Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", 0)),
            rhs: Box::new(Expression::Integer(1)),
        };
        let not_cond = Expression::Unary {
            op: UnaryOp::Not,
            expr: Box::new(wrap_eq_1),
        };
        let rhs_wrap = Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", 1)),
            rhs: Box::new(Expression::Integer(1)),
        };
        assert_eq!(
            got,
            Expression::Binary {
                op: BinaryOp::And,
                lhs: Box::new(not_cond),
                rhs: Box::new(rhs_wrap),
            }
        );
    }

    #[test]
    fn select_i1_inverse_short_circuit_or_reduces_to_not_cond_or_rhs() {
        // `!cond || rhs` → `select i1 %cond, i1 %rhs, i1 true`.
        let mut s = SymbolTable::new();
        s.record_ssa("cond", index_expr("c", 0));
        s.record_ssa("rhs", index_expr("c", 1));
        lower_select_i1(
            "r",
            "i1",
            &Operand::Ssa("cond".into()),
            &Operand::Ssa("rhs".into()),
            &Operand::ConstBool(true),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        let wrap_eq_1 = Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", 0)),
            rhs: Box::new(Expression::Integer(1)),
        };
        let not_cond = Expression::Unary {
            op: UnaryOp::Not,
            expr: Box::new(wrap_eq_1),
        };
        let rhs_wrap = Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", 1)),
            rhs: Box::new(Expression::Integer(1)),
        };
        assert_eq!(
            got,
            Expression::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(not_cond),
                rhs: Box::new(rhs_wrap),
            }
        );
    }

    #[test]
    fn select_i1_general_ternary_lowers_to_or_of_and_pairs() {
        // Neither branch is constant — emit (cond && t) || (!cond && f).
        let mut s = SymbolTable::new();
        s.record_ssa("cond", index_expr("c", 0));
        s.record_ssa("t", index_expr("c", 1));
        s.record_ssa("f", index_expr("c", 2));
        lower_select_i1(
            "r",
            "i1",
            &Operand::Ssa("cond".into()),
            &Operand::Ssa("t".into()),
            &Operand::Ssa("f".into()),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        let wrap = |idx: i64| Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", idx)),
            rhs: Box::new(Expression::Integer(1)),
        };
        let not_cond = Expression::Unary {
            op: UnaryOp::Not,
            expr: Box::new(wrap(0)),
        };
        let expected = Expression::Binary {
            op: BinaryOp::Or,
            lhs: Box::new(Expression::Binary {
                op: BinaryOp::And,
                lhs: Box::new(wrap(0)),
                rhs: Box::new(wrap(1)),
            }),
            rhs: Box::new(Expression::Binary {
                op: BinaryOp::And,
                lhs: Box::new(not_cond),
                rhs: Box::new(wrap(2)),
            }),
        };
        assert_eq!(got, expected);
    }

    #[test]
    fn select_non_i1_value_type_errors_with_pinned_substring() {
        let mut s = SymbolTable::new();
        s.record_ssa("cond", index_expr("c", 0));
        let err = lower_select_i1(
            "r",
            "double",
            &Operand::Ssa("cond".into()),
            &Operand::ConstFloat(0.3),
            &Operand::ConstFloat(0.7),
            &mut s,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("is not supported"), "{msg}");
        assert!(msg.contains("OpenQASM 3 has no classical ternary"), "{msg}");
    }

    #[test]
    fn select_i32_const_arms_lowers_to_inline_arithmetic() {
        // `select i1 %cond, i32 2, i32 1` — the simplest shape producer
        // opt pipelines emit as part of the phi→select collapse for
        // integer accumulation.
        let mut s = SymbolTable::new();
        s.record_ssa("cond", index_expr("c", 0));
        lower_select_i1(
            "r",
            "i32",
            &Operand::Ssa("cond".into()),
            &Operand::ConstInt(2),
            &Operand::ConstInt(1),
            &mut s,
        )
        .unwrap();
        let expr = s.lookup_ssa("r").unwrap();
        // Expected: `(c[0]) * 2 + (1 - c[0]) * 1`
        let Expression::Binary { op, .. } = &expr else {
            panic!("expected Binary, got {expr:?}")
        };
        assert_eq!(*op, BinaryOp::Add);
    }

    #[test]
    fn select_i64_with_ssa_arms_resolves_recursively() {
        // The second select in a cascade has an SSA arm that itself
        // resolves to an earlier select's expression.
        let mut s = SymbolTable::new();
        s.record_ssa("c0", index_expr("c", 0));
        s.record_ssa("c1", index_expr("c", 1));
        // First select: %first = select %c0, i32 5, i32 3
        lower_select_i1(
            "first",
            "i32",
            &Operand::Ssa("c0".into()),
            &Operand::ConstInt(5),
            &Operand::ConstInt(3),
            &mut s,
        )
        .unwrap();
        // Second select: %second = select %c1, i32 %first, i32 0
        lower_select_i1(
            "second",
            "i32",
            &Operand::Ssa("c1".into()),
            &Operand::Ssa("first".into()),
            &Operand::ConstInt(0),
            &mut s,
        )
        .unwrap();
        // The %second expression must transitively reference c[0] and c[1].
        let text = format!("{:?}", s.lookup_ssa("second").unwrap());
        assert!(text.contains("\"c\""), "no c[] reference: {text}");
    }

    #[test]
    fn phi_and_pattern_binds_compound_with_eq_wrapping() {
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        s.record_ssa("b", index_expr("c", 1));
        let mut conds: HashMap<String, Expression> = HashMap::new();
        conds.insert("block_0".into(), index_expr("c", 0));
        lower_phi_i1_short_circuit(
            "combined",
            &[
                PhiIncoming {
                    value: Operand::ConstBool(false),
                    pred: "block_0".into(),
                },
                PhiIncoming {
                    value: Operand::Ssa("b".into()),
                    pred: "block_1".into(),
                },
            ],
            &conds,
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("combined").unwrap();
        let expected = Expression::Binary {
            op: BinaryOp::And,
            lhs: Box::new(Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(1)),
            }),
            rhs: Box::new(Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 1)),
                rhs: Box::new(Expression::Integer(1)),
            }),
        };
        assert_eq!(got, expected);
    }

    #[test]
    fn phi_or_pattern_uses_or_operator() {
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        s.record_ssa("b", index_expr("c", 1));
        let mut conds: HashMap<String, Expression> = HashMap::new();
        conds.insert("block_0".into(), index_expr("c", 0));
        lower_phi_i1_short_circuit(
            "combined",
            &[
                PhiIncoming {
                    value: Operand::ConstBool(true),
                    pred: "block_0".into(),
                },
                PhiIncoming {
                    value: Operand::Ssa("b".into()),
                    pred: "block_1".into(),
                },
            ],
            &conds,
            &mut s,
        )
        .unwrap();
        let Expression::Binary { op, .. } = s.lookup_ssa("combined").unwrap() else {
            panic!()
        };
        assert_eq!(op, BinaryOp::Or);
    }

    #[test]
    fn phi_requires_two_incomings() {
        let mut s = SymbolTable::new();
        let err = lower_phi_i1_short_circuit(
            "x",
            &[PhiIncoming {
                value: Operand::ConstBool(true),
                pred: "a".into(),
            }],
            &HashMap::new(),
            &mut s,
        )
        .unwrap_err();
        assert!(err.to_string().contains("phi i1 has 1 incoming"));
    }

    #[test]
    fn phi_rejects_two_constants() {
        let mut s = SymbolTable::new();
        let err = lower_phi_i1_short_circuit(
            "x",
            &[
                PhiIncoming {
                    value: Operand::ConstBool(true),
                    pred: "a".into(),
                },
                PhiIncoming {
                    value: Operand::ConstBool(false),
                    pred: "b".into(),
                },
            ],
            &HashMap::new(),
            &mut s,
        )
        .unwrap_err();
        assert!(err.to_string().contains("two constant incomings"));
    }

    #[test]
    fn int_arith_add_binds_addition() {
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        lower_int_arith(
            "sum",
            IntArithOp::Add,
            &Operand::Ssa("a".into()),
            &Operand::ConstInt(3),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("sum").unwrap();
        assert_eq!(
            got,
            Expression::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(3)),
            }
        );
    }

    #[test]
    fn int_arith_sub_mul_round_trip() {
        for (op, expected) in [
            (IntArithOp::Sub, BinaryOp::Sub),
            (IntArithOp::Mul, BinaryOp::Mul),
        ] {
            let mut s = SymbolTable::new();
            s.record_ssa("x", index_expr("c", 0));
            s.record_ssa("y", index_expr("c", 1));
            lower_int_arith(
                "r",
                op,
                &Operand::Ssa("x".into()),
                &Operand::Ssa("y".into()),
                &mut s,
            )
            .unwrap();
            let Expression::Binary { op: got, .. } = s.lookup_ssa("r").unwrap() else {
                panic!("{op:?} did not bind a binary expression")
            };
            assert_eq!(got, expected);
        }
    }

    #[test]
    fn int_arith_with_non_integer_operand_errors() {
        // `resolve_integer_operand` rejects pointer-like operands with a
        // pinned error.
        let mut s = SymbolTable::new();
        let err = lower_int_arith(
            "r",
            IntArithOp::Add,
            &qubit(0),
            &Operand::ConstInt(1),
            &mut s,
        )
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("could not interpret integer operand"));
    }

    #[test]
    fn int_arith_rejects_oversized_integer_constant() {
        // `resolve_integer_operand` overflow path: i128::MAX doesn't fit in i64.
        let mut s = SymbolTable::new();
        let err = lower_int_arith(
            "r",
            IntArithOp::Add,
            &Operand::ConstInt(i128::MAX),
            &Operand::ConstInt(0),
            &mut s,
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not fit in i64"));
    }

    #[test]
    fn resolve_i1_operand_rejects_out_of_range_int_via_icmp() {
        // `resolve_i1_operand` still rejects non-0/1 integer constants
        // when used in contexts that enforce Boolean semantics.
        // Covered via `lower_binary_i1(Xor, ...)`, which uses the i1 resolver.
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        let err = lower_binary_i1(
            "r",
            BinaryI1Op::Xor,
            &Operand::Ssa("a".into()),
            &Operand::ConstInt(17),
            &mut s,
        )
        .unwrap_err();
        assert!(err.to_string().contains("could not interpret i1"));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::oq3::ast::{index_expr, BinaryOp, Expression};

    #[test]
    fn phi_missing_predecessor_condition_errors() {
        let mut s = SymbolTable::new();
        s.record_ssa("b", index_expr("c", 1));
        let err = lower_phi_i1_short_circuit(
            "combined",
            &[
                PhiIncoming {
                    value: Operand::ConstBool(false),
                    pred: "missing".into(),
                },
                PhiIncoming {
                    value: Operand::Ssa("b".into()),
                    pred: "rhs".into(),
                },
            ],
            &std::collections::HashMap::new(),
            &mut s,
        )
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("did not record a branching condition"));
    }

    #[test]
    fn phi_rhs_non_ssa_errors() {
        let mut s = SymbolTable::new();
        let mut conds: std::collections::HashMap<String, Expression> =
            std::collections::HashMap::new();
        conds.insert("sc".into(), index_expr("c", 0));
        let err = lower_phi_i1_short_circuit(
            "combined",
            &[
                PhiIncoming {
                    value: Operand::ConstBool(false),
                    pred: "sc".into(),
                },
                PhiIncoming {
                    value: Operand::ConstInt(7),
                    pred: "rhs".into(),
                },
            ],
            &conds,
            &mut s,
        )
        .unwrap_err();
        // This is a synthetic case: the non-SSA rhs goes through
        // `constant_i1_value`, which returns None for 7, so the
        // analyser treats both incomings as non-constant and fails
        // with "short-circuit Boolean pattern" — accept either
        // observed error variant.
        let msg = err.to_string();
        assert!(
            msg.contains("short-circuit")
                || msg.contains("SSA reference")
                || msg.contains("two constant"),
            "{msg}"
        );
    }

    #[test]
    fn icmp_with_integer_operand_accepted() {
        let mut s = SymbolTable::new();
        lower_icmp_i1(
            "r",
            "ne",
            &Operand::ConstInt(0),
            &Operand::ConstInt(1),
            &mut s,
        )
        .unwrap();
        let Expression::Binary { op, .. } = s.lookup_ssa("r").unwrap() else {
            panic!()
        };
        assert_eq!(op, BinaryOp::Ne);
    }

    #[test]
    fn icmp_accepts_wide_integer_operand() {
        // icmp is resolved for all integer widths, so operands like
        // `icmp ult i32 %x, 999` must lower correctly.
        let mut s = SymbolTable::new();
        s.record_ssa("x", Expression::Identifier("k".into()));
        lower_icmp_i1(
            "r",
            "ult",
            &Operand::Ssa("x".into()),
            &Operand::ConstInt(999),
            &mut s,
        )
        .unwrap();
        let Expression::Binary { op, lhs, rhs } = s.lookup_ssa("r").unwrap() else {
            panic!()
        };
        assert_eq!(op, BinaryOp::Lt);
        assert_eq!(*lhs, Expression::Identifier("k".into()));
        assert_eq!(*rhs, Expression::Integer(999));
    }

    #[test]
    fn icmp_signed_and_unsigned_predicates_map_to_same_op() {
        // On Boolean / classical-register operands the signed and unsigned
        // LLVM `icmp` families agree; qirtoqasm maps both to the same OQ3
        // operator.
        for (pred, expected) in [
            ("ult", BinaryOp::Lt),
            ("slt", BinaryOp::Lt),
            ("ule", BinaryOp::Le),
            ("sle", BinaryOp::Le),
            ("ugt", BinaryOp::Gt),
            ("sgt", BinaryOp::Gt),
            ("uge", BinaryOp::Ge),
            ("sge", BinaryOp::Ge),
        ] {
            let mut s = SymbolTable::new();
            lower_icmp_i1(
                "r",
                pred,
                &Operand::ConstInt(3),
                &Operand::ConstInt(5),
                &mut s,
            )
            .unwrap();
            let Expression::Binary { op, .. } = s.lookup_ssa("r").unwrap() else {
                panic!("predicate {pred:?} did not bind a binary expression")
            };
            assert_eq!(op, expected, "predicate {pred:?} mapped incorrectly");
        }
    }

    #[test]
    fn xor_two_ssa_operands_lowers_as_ne() {
        // `xor i1 %a, %b` with two SSA operands lowers as inequality
        // between the Boolean-wrapped operands — that's `a != b`,
        // which is exactly XOR on Booleans.
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        s.record_ssa("b", index_expr("c", 1));
        lower_binary_i1(
            "r",
            BinaryI1Op::Xor,
            &Operand::Ssa("a".into()),
            &Operand::Ssa("b".into()),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        // Each operand is wrapped by `as_boolean_expression` because the
        // raw expression is an `Index` rather than a BinaryOp, so the
        // canonical form uses `x == 1` on each side.
        let wrap = |idx: i64| Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(index_expr("c", idx)),
            rhs: Box::new(Expression::Integer(1)),
        };
        let expected = Expression::Binary {
            op: BinaryOp::Ne,
            lhs: Box::new(wrap(0)),
            rhs: Box::new(wrap(1)),
        };
        assert_eq!(got, expected);
    }

    #[test]
    fn resolve_i1_operand_rejects_unsupported_form() {
        let s = SymbolTable::new();
        // A GlobalRef isn't a legal i1 operand shape.
        let err = super::resolve_i1_operand(&s, &Operand::GlobalRef("g".into())).unwrap_err();
        assert!(err.to_string().contains("could not interpret i1"));
    }

    #[test]
    fn as_boolean_expression_passes_through_comparison() {
        let e = Expression::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(Expression::Integer(1)),
            rhs: Box::new(Expression::Integer(1)),
        };
        let wrapped = as_boolean_expression(e.clone());
        assert_eq!(wrapped, e);
    }

    #[test]
    fn as_boolean_expression_wraps_raw_integer() {
        let wrapped = as_boolean_expression(Expression::Integer(7));
        let Expression::Binary { op, .. } = wrapped else {
            panic!()
        };
        assert_eq!(op, BinaryOp::Eq);
    }

    #[test]
    fn as_boolean_expression_wraps_arithmetic_binary() {
        let add = Expression::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(Expression::Integer(1)),
            rhs: Box::new(Expression::Integer(2)),
        };
        let wrapped = as_boolean_expression(add);
        let Expression::Binary { op, .. } = wrapped else {
            panic!()
        };
        assert_eq!(op, BinaryOp::Eq);
    }
}

#[cfg(test)]
mod final_coverage {
    use super::*;
    use crate::oq3::ast::index_expr;

    /// Force the `sc_pred.is_none()` branch: both incomings non-constant.
    #[test]
    fn phi_with_zero_constants_errors() {
        let mut s = SymbolTable::new();
        s.record_ssa("a", index_expr("c", 0));
        s.record_ssa("b", index_expr("c", 1));
        let err = lower_phi_i1_short_circuit(
            "r",
            &[
                PhiIncoming {
                    value: Operand::Ssa("a".into()),
                    pred: "p1".into(),
                },
                PhiIncoming {
                    value: Operand::Ssa("b".into()),
                    pred: "p2".into(),
                },
            ],
            &std::collections::HashMap::new(),
            &mut s,
        )
        .unwrap_err();
        assert!(err.to_string().contains("short-circuit Boolean pattern"));
    }

    #[test]
    fn constant_i1_value_on_const_int_0_and_1() {
        assert_eq!(constant_i1_value(&Operand::ConstInt(0)), Some(false));
        assert_eq!(constant_i1_value(&Operand::ConstInt(1)), Some(true));
        assert_eq!(constant_i1_value(&Operand::ConstInt(2)), None);
    }
}

#[cfg(test)]
mod rhs_pair_none_coverage {
    use super::*;
    use crate::oq3::ast::index_expr;

    /// Both incomings are constant — a previous test covers the
    /// "two constants" branch. What about exactly one constant with
    /// no rhs at all? That happens when `incomings.len() == 2` and
    /// both are constant AND equal — the first gets assigned to
    /// sc_pred, and after the "two constants" check triggers, we
    /// don't reach the rhs_pair check. Constructing a synthetic case
    /// where sc_pred is Some but rhs_pair is None requires a single-
    /// incoming list, which our length-check intercepts first. The
    /// rhs_pair-None branch is consequently unreachable on valid
    /// input. Keeping this regression test here as a tripwire.
    #[test]
    fn both_incomings_identified_correctly() {
        let mut s = SymbolTable::new();
        s.record_ssa("b", index_expr("c", 1));
        let mut conds = std::collections::HashMap::new();
        conds.insert("p1".into(), index_expr("c", 0));
        lower_phi_i1_short_circuit(
            "out",
            &[
                PhiIncoming {
                    value: Operand::ConstBool(false),
                    pred: "p1".into(),
                },
                PhiIncoming {
                    value: Operand::Ssa("b".into()),
                    pred: "p2".into(),
                },
            ],
            &conds,
            &mut s,
        )
        .unwrap();
        assert!(s.lookup_ssa("out").is_ok());
    }

    #[test]
    fn select_i1_true_false_folds_to_cond() {
        // `select i1 %c, i1 true, i1 false` is semantically just `%c`.
        // Bind the result SSA to the boolean form of `%c`.
        let mut s = SymbolTable::new();
        s.record_ssa("cond", index_expr("c", 0));
        lower_select_i1(
            "r",
            "i1",
            &Operand::Ssa("cond".into()),
            &Operand::ConstBool(true),
            &Operand::ConstBool(false),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        assert_eq!(
            got,
            Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(1)),
            }
        );
    }

    #[test]
    fn select_i1_false_true_folds_to_not_cond() {
        // `select i1 %c, i1 false, i1 true` is semantically `!%c`.
        // Bind the result SSA to `!(%c == 1)`.
        let mut s = SymbolTable::new();
        s.record_ssa("cond", index_expr("c", 0));
        lower_select_i1(
            "r",
            "i1",
            &Operand::Ssa("cond".into()),
            &Operand::ConstBool(false),
            &Operand::ConstBool(true),
            &mut s,
        )
        .unwrap();
        let got = s.lookup_ssa("r").unwrap();
        assert_eq!(
            got,
            Expression::Unary {
                op: UnaryOp::Not,
                expr: Box::new(Expression::Binary {
                    op: BinaryOp::Eq,
                    lhs: Box::new(index_expr("c", 0)),
                    rhs: Box::new(Expression::Integer(1)),
                }),
            }
        );
    }
}
