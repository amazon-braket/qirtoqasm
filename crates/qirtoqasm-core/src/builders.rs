// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Per-call lowering helpers.
//!
//! These functions consume a single QIR `call` instruction along with its
//! canonical signature and operands, and return the OpenQASM statements it
//! should lower to. Operand classification is signature-driven: we look up
//! the parameter type in the `FunctionSignature` rather than relying on the
//! operand's text form, so typed- and opaque-pointer QIR both work.

use crate::error::{QirToQasmError, Result};
use crate::ir::Operand;
use crate::oq3::ast::*;
use crate::profile::FunctionBuilder;
use crate::signatures::FunctionSignature;
use crate::symbols::{SymbolTable, QUBIT_REGISTER, RESULT_REGISTER};

/// Lower a call according to the builder's kind, returning any OQ3
/// statements that should be appended to the enclosing block.
///
/// `result_ssa` carries the SSA key assigned by the call (e.g. `"1"` for
/// a `%1 = call …` line). `ReadResult` binds this key to a `c[i]`
/// expression; other builders pass it through unchanged.
pub fn lower_call(
    builder: &FunctionBuilder,
    signature: &FunctionSignature,
    args: &[Operand],
    result_ssa: Option<&str>,
    symbols: &mut SymbolTable,
) -> Result<Vec<Statement>> {
    // Opaque-pointer QIR (LLVM 15+) arrives here with every `%Qubit*` /
    // `%Result*` declared parameter collapsed to bare `ptr`. Re-tag those
    // positions based on the builder's fixed calling convention before
    // the individual builders try to route operands by type.
    let signature = canonicalize_opaque_ptrs(signature, builder);
    match builder {
        FunctionBuilder::Gate { gate_name, adjoint } => {
            build_gate(gate_name, *adjoint, &signature, args, symbols)
        }
        FunctionBuilder::Measurement => build_measurement(&signature, args, symbols),
        FunctionBuilder::MeasureAndReset => build_measure_and_reset(&signature, args, symbols),
        FunctionBuilder::Reset => build_reset(&signature, args, symbols),
        FunctionBuilder::ReadResult => {
            let expr = build_read_result(&signature, args, symbols)?;
            if let Some(key) = result_ssa {
                symbols.record_ssa(key, expr);
            }
            Ok(Vec::new())
        }
        FunctionBuilder::RecordOutputNoop => Ok(Vec::new()),
        FunctionBuilder::GeneralizedControlled => {
            build_generalized_controlled(&signature, args, symbols)
        }
    }
}

/// Rewrite bare `"ptr"` param types to their semantic counterparts
/// (`"Qubit"` or `"Result"`) based on the builder's QIS calling
/// convention. Typed-pointer QIR is unaffected: its `param_types`
/// already contain the canonical struct names.
fn canonicalize_opaque_ptrs(
    sig: &FunctionSignature,
    builder: &FunctionBuilder,
) -> FunctionSignature {
    let replacement = |idx: usize| -> Option<&'static str> {
        match builder {
            // All QIS gate intrinsic pointer params are Qubit*.
            FunctionBuilder::Gate { .. } | FunctionBuilder::Reset => Some("Qubit"),
            // `mz`/`m`/`mresetz` all take (Qubit*, Result*).
            FunctionBuilder::Measurement | FunctionBuilder::MeasureAndReset => match idx {
                0 => Some("Qubit"),
                1 => Some("Result"),
                _ => None,
            },
            // `__quantum__qis__read_result__body(Result*)` /
            // `__quantum__rt__read_result(Result*)`.
            FunctionBuilder::ReadResult => Some("Result"),
            // Record-output intrinsics discard their operands, so
            // re-typing here is moot.
            FunctionBuilder::RecordOutputNoop => None,
            // The variadic generalized-invoke callee has no `ptr` params
            // in its fixed prefix; tail-operand qubits are handled inside
            // `build_generalized_controlled` itself.
            FunctionBuilder::GeneralizedControlled => None,
        }
    };
    let mut out = sig.clone();
    for (i, ty) in out.param_types.iter_mut().enumerate() {
        if ty == "ptr" {
            if let Some(replacement) = replacement(i) {
                *ty = replacement.to_string();
            }
        }
    }
    out
}

fn build_gate(
    gate_name: &str,
    adjoint: bool,
    signature: &FunctionSignature,
    args: &[Operand],
    symbols: &mut SymbolTable,
) -> Result<Vec<Statement>> {
    let mut arguments: Vec<Expression> = Vec::new();
    let mut qubits: Vec<IndexedIdentifier> = Vec::new();
    for (op, ty) in args.iter().zip(signature.param_types.iter()) {
        if ty == "Qubit" {
            qubits.push(resolve_qubit_operand(symbols, op, &signature.name)?);
        } else {
            arguments.push(resolve_scalar_operand(symbols, op, &signature.name)?);
        }
    }
    let modifiers = if adjoint {
        vec![GateModifier::Inv]
    } else {
        Vec::new()
    };
    Ok(vec![Statement::QuantumGate {
        modifiers,
        name: gate_name.to_string(),
        arguments,
        qubits,
    }])
}

/// Lower the `generalizedInvokeWithRotationsControlsTargets`
/// variadic multi-controlled dispatch into a single Braket-native
/// OpenQASM 3 gate application.
///
/// Operand layout (post-lowering, adaptive profile):
/// ```text
/// generalizedInvokeWithRotationsControlsTargets(
///     i64 numRotations,
///     i64 adjointFlag,
///     i64 numControls,
///     i64 numTargets,
///     i8* <innerFn>,        // bitcast (T* @__quantum__qis__<op>__ctl to i8*)
///     <numRotations doubles>,
///     <numControls + numTargets i8* qubit pointers>,
/// )
/// ```
///
/// We resolve `<op>` from the inner function name and the
/// `(numControls, numTargets, numRotations)` triple to a Braket-native
/// gate. Unsupported combinations (e.g. three-controlled X when
/// Braket has no `cccnot`) produce a descriptive error rather than an
/// incorrect translation.
fn build_generalized_controlled(
    signature: &FunctionSignature,
    args: &[Operand],
    symbols: &mut SymbolTable,
) -> Result<Vec<Statement>> {
    const CALLEE: &str = "generalizedInvokeWithRotationsControlsTargets";
    // Verify that this dispatch arm was actually reached for the right
    // callee. Every other builder guards this via `expect_signature`;
    // without an equivalent guard here, a misclassification upstream
    // (e.g. a plain `cnot` routed to `GeneralizedControlled` by mistake
    // in `profile.rs`) would surface as a confusing error about
    // `generalizedInvokeWithRotationsControlsTargets` operands the user
    // never passed, obscuring the real bug in the classifier.
    //
    // The 5th declared parameter carries the inner-function pointer.
    // In canonical form (see `signatures::canonicalize_type`), typed
    // pointers like `i8*` collapse to `i8` and LLVM 15+ opaque
    // pointers appear as `ptr`; accept either.
    const EXPECTED_LEADING: &[&str] = &["i64", "i64", "i64", "i64"];
    let leading_ok = signature.param_types.len() > EXPECTED_LEADING.len()
        && signature
            .param_types
            .iter()
            .zip(EXPECTED_LEADING.iter())
            .all(|(got, exp)| got == exp);
    let fifth_ok = signature
        .param_types
        .get(EXPECTED_LEADING.len())
        .is_some_and(|t| t == "i8" || t == "ptr");
    if signature.name != CALLEE || !signature.is_variadic || !leading_ok || !fifth_ok {
        return Err(QirToQasmError::unsupported(format!(
            "generalized-controlled intrinsic {:?} has unexpected signature \
             {:?} (variadic={}); expected callee {CALLEE:?} with parameter \
             prefix (\"i64\", \"i64\", \"i64\", \"i64\", \"i8\"|\"ptr\", ...) \
             and is_variadic=true",
            signature.name, signature.param_types, signature.is_variadic,
        )));
    }
    if args.len() < 5 {
        return Err(QirToQasmError::unsupported(format!(
            "{CALLEE} expects at least 5 operands (numRotations, adjoint, \
             numControls, numTargets, innerFn); got {} operand(s)",
            args.len()
        )));
    }
    let num_rotations = as_i64_operand(&args[0], CALLEE, "numRotations")?;
    let adjoint_flag = as_i64_operand(&args[1], CALLEE, "adjoint")?;
    let num_controls = as_i64_operand(&args[2], CALLEE, "numControls")?;
    let num_targets = as_i64_operand(&args[3], CALLEE, "numTargets")?;

    if num_rotations != 0 {
        return Err(QirToQasmError::unsupported(format!(
            "{CALLEE} with numRotations={num_rotations} is not yet supported; \
             only non-parametric controlled gates (numRotations == 0) are handled"
        )));
    }
    if num_controls < 0 || num_targets < 0 {
        return Err(QirToQasmError::unsupported(format!(
            "{CALLEE} had negative numControls={num_controls} or numTargets={num_targets}"
        )));
    }

    let inner_callee = match &args[4] {
        Operand::BitcastGlobal(name) => name.as_str(),
        // Some producers emit the inner function pointer as a bare
        // `@<name>` GlobalRef (without the explicit `bitcast` wrapper
        // that the typed-pointer form uses). Accept both spellings.
        Operand::GlobalRef(name) => name.as_str(),
        other => {
            return Err(QirToQasmError::unsupported(format!(
                "{CALLEE}: inner-function operand must be a bitcast of a named \
                 global (e.g. ``i8* bitcast (T* @__quantum__qis__<op>__ctl to \
                 i8*)``) or a bare global reference; got {other:?}"
            )));
        }
    };
    let op_name = inner_callee
        .strip_prefix("__quantum__qis__")
        .and_then(|rest| rest.strip_suffix("__ctl"))
        .ok_or_else(|| {
            QirToQasmError::unsupported(format!(
                "{CALLEE}: inner callee {inner_callee:?} does not match the \
                 expected `__quantum__qis__<op>__ctl` pattern"
            ))
        })?;

    // Variadic tail: numRotations doubles, then numControls + numTargets
    // qubit operands. We've validated numRotations == 0 above.
    let tail = &args[5..];
    let expected_tail = (num_controls + num_targets) as usize;
    if tail.len() != expected_tail {
        return Err(QirToQasmError::unsupported(format!(
            "{CALLEE}: expected {expected_tail} qubit operand(s) after the fixed \
             prefix (numControls={num_controls} + numTargets={num_targets}); \
             got {} trailing operand(s)",
            tail.len()
        )));
    }
    let mut qubits: Vec<IndexedIdentifier> = Vec::with_capacity(expected_tail);
    for op in tail {
        // Tail operands arrive typed `i8*`: the parser returns `I8Null`
        // for `i8* null` and `PtrConst { struct_name: None, ... }` for
        // `i8* inttoptr`. Normalise the former to qubit 0 so the shared
        // resolver (which only understands `PtrConst`) can handle both.
        let normalised = match op {
            Operand::I8Null => Operand::PtrConst {
                struct_name: None,
                index: 0,
            },
            other => other.clone(),
        };
        qubits.push(resolve_qubit_operand(symbols, &normalised, CALLEE)?);
    }

    let gate_name =
        resolve_controlled_gate_name(op_name, num_controls as usize, num_targets as usize)
            .ok_or_else(|| {
                QirToQasmError::unsupported(format!(
                    "{CALLEE}: no Braket-native gate for controlled ``{op_name}`` with \
             numControls={num_controls}, numTargets={num_targets}. Extend the \
             decomposition pipeline upstream, or add a gate mapping here."
                ))
            })?;

    let modifiers = if adjoint_flag != 0 {
        vec![GateModifier::Inv]
    } else {
        Vec::new()
    };
    Ok(vec![Statement::QuantumGate {
        modifiers,
        name: gate_name,
        arguments: Vec::new(),
        qubits,
    }])
}

/// Interpret one of the leading `i64` counts passed to the variadic
/// generalized-invoke intrinsic.
fn as_i64_operand(op: &Operand, callee: &str, field: &str) -> Result<i64> {
    match op {
        Operand::ConstInt(n) => i64::try_from(*n).map_err(|_| {
            QirToQasmError::unsupported(format!("{callee}: {field} = {n} does not fit in i64"))
        }),
        _ => Err(QirToQasmError::unsupported(format!(
            "{callee}: {field} must be an i64 constant, got {op:?}"
        ))),
    }
}

/// Map `(<op>__ctl, numControls, numTargets)` to a Braket-native OpenQASM 3
/// gate name. Returns `None` when no suitable built-in exists.
fn resolve_controlled_gate_name(
    op: &str,
    num_controls: usize,
    num_targets: usize,
) -> Option<String> {
    match (op, num_controls, num_targets) {
        // Controlled X: CNOT / CCNOT (Toffoli).
        ("x", 1, 1) => Some("cnot".into()),
        ("x", 2, 1) => Some("ccnot".into()),
        // Controlled Y / Z — only single-control forms are Braket-native.
        ("y", 1, 1) => Some("cy".into()),
        ("z", 1, 1) => Some("cz".into()),
        // Controlled-SWAP (Fredkin).
        ("swap", 1, 2) => Some("cswap".into()),
        // Controlled phase rotations emitted with their existing names.
        ("phaseshift", 1, 1) => Some("cphaseshift".into()),
        _ => None,
    }
}

/// Verify `signature.param_types` matches `expected`. Returns a
/// pinned-substring error keyed by `descriptor` (e.g. `"measurement"`)
/// when they differ — every callsite expects the substring
/// `"<descriptor> intrinsic"` and `"unexpected signature"` to appear.
fn expect_signature(
    signature: &FunctionSignature,
    expected: &[&str],
    descriptor: &str,
) -> Result<()> {
    if signature
        .param_types
        .iter()
        .map(String::as_str)
        .eq(expected.iter().copied())
    {
        return Ok(());
    }
    let pretty = match expected {
        [a] => format!("(\"{a}\",)"),
        [a, b] => format!("(\"{a}\", \"{b}\")"),
        _ => format!("{expected:?}"),
    };
    Err(QirToQasmError::unsupported(format!(
        "{descriptor} intrinsic {:?} has unexpected signature {:?}; expected {pretty}",
        signature.name, signature.param_types
    )))
}

fn build_measurement(
    signature: &FunctionSignature,
    args: &[Operand],
    symbols: &mut SymbolTable,
) -> Result<Vec<Statement>> {
    expect_signature(signature, &["Qubit", "Result"], "measurement")?;
    let qubit = resolve_qubit_operand(symbols, &args[0], &signature.name)?;
    let target = resolve_result_operand(symbols, &args[1], &signature.name)?;
    Ok(vec![Statement::QuantumMeasurementStatement {
        qubit,
        target,
    }])
}

fn build_reset(
    signature: &FunctionSignature,
    args: &[Operand],
    symbols: &mut SymbolTable,
) -> Result<Vec<Statement>> {
    expect_signature(signature, &["Qubit"], "reset")?;
    let q = resolve_qubit_operand(symbols, &args[0], &signature.name)?;
    Ok(vec![Statement::QuantumReset(q)])
}

/// Lower `mresetz(Qubit*, Result*)` to `measure q; reset q;`.
fn build_measure_and_reset(
    signature: &FunctionSignature,
    args: &[Operand],
    symbols: &mut SymbolTable,
) -> Result<Vec<Statement>> {
    expect_signature(signature, &["Qubit", "Result"], "measure-and-reset")?;
    let qubit = resolve_qubit_operand(symbols, &args[0], &signature.name)?;
    let target = resolve_result_operand(symbols, &args[1], &signature.name)?;
    Ok(vec![
        Statement::QuantumMeasurementStatement {
            qubit: qubit.clone(),
            target,
        },
        Statement::QuantumReset(qubit),
    ])
}

fn build_read_result(
    signature: &FunctionSignature,
    args: &[Operand],
    symbols: &mut SymbolTable,
) -> Result<Expression> {
    expect_signature(signature, &["Result"], "read_result")?;
    resolve_result_operand_expr(symbols, &args[0], &signature.name)
}

// ---------------------------------------------------------------------------
// Operand resolvers
// ---------------------------------------------------------------------------

/// Resolve a `%Qubit*` operand to `q[index]` (assignment target form).
pub fn resolve_qubit_operand(
    symbols: &mut SymbolTable,
    op: &Operand,
    callee: &str,
) -> Result<IndexedIdentifier> {
    match op {
        Operand::PtrConst { index, .. } => {
            symbols.record_qubit(*index);
            Ok(indexed_ident(QUBIT_REGISTER, *index))
        }
        Operand::Ssa(_) | Operand::GlobalRef(_) | Operand::GetElementPtr => {
            Err(QirToQasmError::unsupported(format!(
                "qubit operand for {callee:?} is not a compile-time constant; \
                 runtime qubit allocation is not supported"
            )))
        }
        _ => Err(QirToQasmError::unsupported(format!(
            "could not resolve qubit operand for {callee:?}"
        ))),
    }
}

/// Resolve a `%Result*` operand to `c[index]` as an assignment target.
pub fn resolve_result_operand(
    symbols: &mut SymbolTable,
    op: &Operand,
    callee: &str,
) -> Result<IndexedIdentifier> {
    match op {
        Operand::PtrConst { index, .. } => {
            symbols.record_result(*index);
            Ok(indexed_ident(RESULT_REGISTER, *index))
        }
        _ => Err(QirToQasmError::unsupported(format!(
            "result operand for {callee:?} is not a compile-time constant; \
             runtime result allocation is not supported"
        ))),
    }
}

/// Resolve a `%Result*` operand to `c[index]` as an expression.
pub fn resolve_result_operand_expr(
    symbols: &mut SymbolTable,
    op: &Operand,
    callee: &str,
) -> Result<Expression> {
    let id = resolve_result_operand(symbols, op, callee)?;
    // `resolve_result_operand` produces its `IndexedIdentifier` via
    // `indexed_ident(..)`, which always wraps the index in `Some(_)`,
    // so the `.expect(..)` here documents an upstream-enforced
    // invariant rather than a runtime fallibility.
    let index = id
        .index
        .expect("`resolve_result_operand` produces an indexed identifier");
    Ok(Expression::Index {
        collection: Box::new(Expression::Identifier(id.name)),
        index: Box::new(index),
    })
}

/// Resolve a scalar (non-pointer) operand to an OpenQASM expression.
pub fn resolve_scalar_operand(
    symbols: &SymbolTable,
    op: &Operand,
    callee: &str,
) -> Result<Expression> {
    match op {
        Operand::ConstInt(n) => Ok(Expression::Integer(i128_to_i64(*n)?)),
        Operand::ConstFloat(f) => Ok(Expression::Float(*f)),
        Operand::ConstBool(b) => Ok(Expression::Boolean(*b)),
        Operand::Ssa(id) => symbols.lookup_ssa(id),
        _ => Err(QirToQasmError::unsupported(format!(
            "unsupported scalar operand form for {callee:?}"
        ))),
    }
}

fn i128_to_i64(n: i128) -> Result<i64> {
    i64::try_from(n).map_err(|_| {
        QirToQasmError::unsupported(format!(
            "integer constant {n} does not fit in i64; larger widths are not supported"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn sig(name: &str, params: &[&str], ret: &str) -> FunctionSignature {
        FunctionSignature {
            name: name.into(),
            return_type: ret.into(),
            param_types: params.iter().map(|s| s.to_string()).collect(),
            is_variadic: false,
        }
    }

    #[test]
    fn gate_build_routes_qubit_vs_scalar_operands_by_signature() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::Gate {
            gate_name: "rx".into(),
            adjoint: false,
        };
        let signature = sig("__quantum__qis__rx__body", &["double", "Qubit"], "void");
        let args = vec![Operand::ConstFloat(0.5), qubit(1)];
        let stmts = lower_call(&b, &signature, &args, None, &mut s).unwrap();
        assert_eq!(stmts.len(), 1);
        let Statement::QuantumGate {
            name,
            arguments,
            qubits,
            ..
        } = &stmts[0]
        else {
            panic!("expected QuantumGate, got {:?}", stmts[0])
        };
        assert_eq!(name, "rx");
        assert_eq!(arguments, &vec![Expression::Float(0.5)]);
        assert_eq!(qubits, &vec![indexed_ident("q", 1)]);
        assert_eq!(s.max_qubit_index, 1);
    }

    #[test]
    fn measurement_build_requires_qubit_result_signature() {
        let mut s = SymbolTable::new();
        let args = vec![qubit(0), result(0)];
        let b = FunctionBuilder::Measurement;
        let signature = sig("__quantum__qis__mz__body", &["Qubit", "Result"], "void");
        let stmts = lower_call(&b, &signature, &args, None, &mut s).unwrap();
        assert_eq!(stmts.len(), 1);
        let Statement::QuantumMeasurementStatement { target, .. } = &stmts[0] else {
            panic!("expected QuantumMeasurementStatement, got {:?}", stmts[0])
        };
        assert_eq!(target.name, "c");

        // Wrong signature: (Result, Qubit) instead of (Qubit, Result).
        let signature = sig("__quantum__qis__mz__body", &["Result", "Qubit"], "void");
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err.to_string().contains("measurement intrinsic"));
        assert!(err.to_string().contains("unexpected signature"));
    }

    #[test]
    fn read_result_binds_ssa_and_emits_no_statements() {
        let mut s = SymbolTable::new();
        let args = vec![result(0)];
        let b = FunctionBuilder::ReadResult;
        let signature = sig("__quantum__qis__read_result__body", &["Result"], "i1");
        let stmts = lower_call(&b, &signature, &args, Some("cond"), &mut s).unwrap();
        assert!(stmts.is_empty());
        let expr = s.lookup_ssa("cond").unwrap();
        assert_eq!(expr, index_expr("c", 0));
    }

    #[test]
    fn record_output_noop_drops_call() {
        let mut s = SymbolTable::new();
        let args = vec![result(0), Operand::GetElementPtr];
        let b = FunctionBuilder::RecordOutputNoop;
        let signature = sig(
            "__quantum__rt__result_record_output",
            &["Result", "ptr"],
            "void",
        );
        let stmts = lower_call(&b, &signature, &args, None, &mut s).unwrap();
        assert!(stmts.is_empty());
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::oq3::ast::Expression;
    use crate::test_support::*;

    fn sig(name: &str, params: &[&str], ret: &str) -> FunctionSignature {
        FunctionSignature {
            name: name.into(),
            return_type: ret.into(),
            param_types: params.iter().map(|s| s.to_string()).collect(),
            is_variadic: false,
        }
    }

    #[test]
    fn measurement_with_bad_signature_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::Measurement;
        let signature = sig("mz", &["Qubit"], "void");
        let args = vec![qubit(0)];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err.to_string().contains("measurement intrinsic"));
        assert!(err.to_string().contains("unexpected signature"));
    }

    #[test]
    fn reset_with_bad_signature_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::Reset;
        let signature = sig("reset", &[], "void");
        let args = vec![];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err.to_string().contains("reset intrinsic"));
    }

    #[test]
    fn read_result_with_bad_signature_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::ReadResult;
        let signature = sig("rr", &["Qubit"], "i1");
        let args = vec![qubit(0)];
        let err = lower_call(&b, &signature, &args, Some("k"), &mut s).unwrap_err();
        assert!(err.to_string().contains("read_result intrinsic"));
    }

    #[test]
    fn resolve_qubit_operand_rejects_ssa() {
        let mut s = SymbolTable::new();
        let err = resolve_qubit_operand(&mut s, &Operand::Ssa("x".into()), "f").unwrap_err();
        assert!(err.to_string().contains("is not a compile-time constant"));
    }

    #[test]
    fn resolve_qubit_operand_rejects_other_forms() {
        let mut s = SymbolTable::new();
        let err = resolve_qubit_operand(&mut s, &Operand::ConstInt(0), "f").unwrap_err();
        assert!(err.to_string().contains("could not resolve qubit operand"));
    }

    #[test]
    fn resolve_result_operand_lvalue_rejects_non_pointer() {
        let mut s = SymbolTable::new();
        let err = resolve_result_operand(&mut s, &Operand::ConstInt(0), "f").unwrap_err();
        assert!(err.to_string().contains("is not a compile-time constant"));
    }

    #[test]
    fn resolve_result_operand_expr_rejects_non_pointer() {
        let mut s = SymbolTable::new();
        let err = resolve_result_operand_expr(&mut s, &Operand::ConstInt(0), "f").unwrap_err();
        assert!(err.to_string().contains("is not a compile-time constant"));
    }

    #[test]
    fn resolve_scalar_operand_supports_every_legal_shape() {
        let mut s = SymbolTable::new();
        s.record_ssa("x", Expression::Integer(5));
        assert_eq!(
            resolve_scalar_operand(&s, &Operand::ConstInt(3), "f").unwrap(),
            Expression::Integer(3)
        );
        assert_eq!(
            resolve_scalar_operand(&s, &Operand::ConstFloat(0.5), "f").unwrap(),
            Expression::Float(0.5)
        );
        assert_eq!(
            resolve_scalar_operand(&s, &Operand::ConstBool(true), "f").unwrap(),
            Expression::Boolean(true)
        );
        assert_eq!(
            resolve_scalar_operand(&s, &Operand::Ssa("x".into()), "f").unwrap(),
            Expression::Integer(5)
        );
    }

    #[test]
    fn resolve_scalar_operand_rejects_pointer_forms() {
        let s = SymbolTable::new();
        let err = resolve_scalar_operand(&s, &opaque(0), "f").unwrap_err();
        assert!(err.to_string().contains("unsupported scalar operand"));
    }

    #[test]
    fn const_int_too_large_for_i64_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::Gate {
            gate_name: "rx".into(),
            adjoint: false,
        };
        let signature = sig("rx", &["i64", "Qubit"], "void");
        let args = vec![Operand::ConstInt(i128::MAX), opaque(0)];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err.to_string().contains("does not fit in i64"));
    }

    #[test]
    fn adjoint_gate_builder_emits_inv_modifier() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::Gate {
            gate_name: "s".into(),
            adjoint: true,
        };
        let signature = sig("s_adj", &["Qubit"], "void");
        let args = vec![opaque(0)];
        let stmts = lower_call(&b, &signature, &args, None, &mut s).unwrap();
        let crate::oq3::ast::Statement::QuantumGate { modifiers, .. } = &stmts[0] else {
            panic!("expected QuantumGate, got {:?}", stmts[0])
        };
        assert_eq!(modifiers.len(), 1);
    }

    fn variadic_sig(name: &str) -> FunctionSignature {
        FunctionSignature {
            name: name.into(),
            return_type: "void".into(),
            param_types: vec![
                "i64".into(),
                "i64".into(),
                "i64".into(),
                "i64".into(),
                // Canonical form of `i8*` after `signatures::canonicalize_type`.
                "i8".into(),
            ],
            is_variadic: true,
        }
    }

    #[test]
    fn generalized_controlled_lowers_toffoli_to_ccnot() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(2),
            Operand::ConstInt(1),
            Operand::BitcastGlobal("__quantum__qis__x__ctl".into()),
            Operand::I8Null,
            opaque(1),
            opaque(2),
        ];
        let stmts = lower_call(&b, &signature, &args, None, &mut s).unwrap();
        assert_eq!(stmts.len(), 1);
        let crate::oq3::ast::Statement::QuantumGate {
            name,
            qubits,
            modifiers,
            ..
        } = &stmts[0]
        else {
            panic!("expected QuantumGate")
        };
        assert_eq!(name, "ccnot");
        assert_eq!(qubits.len(), 3);
        assert!(modifiers.is_empty());
    }

    #[test]
    fn generalized_controlled_lowers_fredkin_to_cswap() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(2),
            Operand::BitcastGlobal("__quantum__qis__swap__ctl".into()),
            Operand::I8Null,
            opaque(1),
            opaque(2),
        ];
        let stmts = lower_call(&b, &signature, &args, None, &mut s).unwrap();
        let crate::oq3::ast::Statement::QuantumGate { name, qubits, .. } = &stmts[0] else {
            panic!("expected QuantumGate")
        };
        assert_eq!(name, "cswap");
        assert_eq!(qubits.len(), 3);
    }

    #[test]
    fn generalized_controlled_with_adjoint_flag_emits_inv_modifier() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(1), // adjoint flag ON
            Operand::ConstInt(1),
            Operand::ConstInt(1),
            Operand::BitcastGlobal("__quantum__qis__y__ctl".into()),
            Operand::I8Null,
            opaque(1),
        ];
        let stmts = lower_call(&b, &signature, &args, None, &mut s).unwrap();
        let crate::oq3::ast::Statement::QuantumGate {
            name, modifiers, ..
        } = &stmts[0]
        else {
            panic!("expected QuantumGate")
        };
        assert_eq!(name, "cy");
        assert_eq!(modifiers.len(), 1);
    }

    #[test]
    fn generalized_controlled_rotations_not_yet_supported() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        let args = vec![
            Operand::ConstInt(1), // numRotations = 1 → reject
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(1),
            Operand::BitcastGlobal("__quantum__qis__rz__ctl".into()),
            Operand::ConstFloat(0.5),
            Operand::I8Null,
            opaque(1),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err.to_string().contains("numRotations=1"));
    }

    #[test]
    fn generalized_controlled_unknown_op_errors_descriptively() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(3), // three controls on x → no Braket-native
            Operand::ConstInt(1),
            Operand::BitcastGlobal("__quantum__qis__x__ctl".into()),
            Operand::I8Null,
            opaque(1),
            opaque(2),
            opaque(3),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no Braket-native gate"));
        assert!(msg.contains("numControls=3"));
    }

    #[test]
    fn generalized_controlled_inner_callee_without_ctl_suffix_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(1),
            // Missing the __ctl suffix → rejected.
            Operand::BitcastGlobal("__quantum__qis__x__body".into()),
            Operand::I8Null,
            opaque(1),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err
            .to_string()
            .contains("does not match the expected `__quantum__qis__<op>__ctl` pattern"));
    }

    #[test]
    fn generalized_controlled_too_few_operands_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        // Only 4 operands (missing the inner-fn pointer) - must reject.
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(1),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err.to_string().contains("expects at least 5 operands"));
    }

    #[test]
    fn generalized_controlled_negative_count_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(-1), // negative numControls
            Operand::ConstInt(1),
            Operand::GlobalRef("__quantum__qis__x__ctl".into()),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err.to_string().contains("negative numControls"));
    }

    #[test]
    fn generalized_controlled_non_bitcast_non_globalref_inner_fn_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(1),
            // PtrConst is not a valid inner-fn operand - must reject.
            opaque(0),
            Operand::I8Null,
            opaque(1),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err.to_string().contains("inner-function operand must be"));
    }

    #[test]
    fn generalized_controlled_wrong_tail_arity_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        // numControls=1 + numTargets=1 => expect 2 tail qubits, supply 3.
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(1),
            Operand::GlobalRef("__quantum__qis__x__ctl".into()),
            Operand::I8Null,
            opaque(1),
            opaque(2),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err.to_string().contains("expected 2 qubit operand(s)"));
    }

    #[test]
    fn generalized_controlled_count_field_non_constant_errors() {
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = variadic_sig("generalizedInvokeWithRotationsControlsTargets");
        // numRotations field is an SSA reference, not a constant - reject.
        let args = vec![
            Operand::Ssa("rotcount".into()),
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(1),
            Operand::GlobalRef("__quantum__qis__x__ctl".into()),
            Operand::I8Null,
            opaque(1),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        assert!(err
            .to_string()
            .contains("numRotations must be an i64 constant"));
    }

    #[test]
    fn generalized_controlled_wrong_callee_name_errors_at_dispatch() {
        // Regression test: if `profile.rs` ever misclassifies a callee as
        // `GeneralizedControlled`, the resulting error must point at the
        // misclassified callee, not at the generalized-invoke intrinsic's
        // internals (numRotations / numControls / etc.) which the user
        // never actually invoked.
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        // Signature shape matches the variadic prefix, but the name doesn't.
        let signature = variadic_sig("someOtherIntrinsic");
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(1),
            Operand::BitcastGlobal("__quantum__qis__x__ctl".into()),
            Operand::I8Null,
            opaque(1),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("generalized-controlled intrinsic")
                && msg.contains("someOtherIntrinsic")
                && msg.contains("unexpected signature"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn generalized_controlled_wrong_prefix_types_errors_at_dispatch() {
        // Same guard, tripped by a param-type-prefix mismatch rather than
        // a name mismatch. If a caller ever manages to reach this arm with
        // a signature whose fixed-arg prefix is not
        // (i64, i64, i64, i64, i8|ptr), we should reject early rather than
        // interpret garbage as rotation/control counts.
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = FunctionSignature {
            name: "generalizedInvokeWithRotationsControlsTargets".into(),
            return_type: "void".into(),
            param_types: vec![
                // Wrong: swap first i64 with i32.
                "i32".into(),
                "i64".into(),
                "i64".into(),
                "i64".into(),
                "i8".into(),
            ],
            is_variadic: true,
        };
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(1),
            Operand::BitcastGlobal("__quantum__qis__x__ctl".into()),
            Operand::I8Null,
            opaque(1),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("generalized-controlled intrinsic")
                && msg.contains("unexpected signature")
                && msg.contains("\"i32\""),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn generalized_controlled_non_variadic_errors_at_dispatch() {
        // Same guard, tripped by is_variadic=false. This shouldn't happen
        // through the normal parser+profile path, but guard against a
        // future refactor that constructs `FunctionSignature` values by
        // hand and forgets the flag.
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = FunctionSignature {
            name: "generalizedInvokeWithRotationsControlsTargets".into(),
            return_type: "void".into(),
            param_types: vec![
                "i64".into(),
                "i64".into(),
                "i64".into(),
                "i64".into(),
                "i8".into(),
            ],
            is_variadic: false, // wrong
        };
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(1),
            Operand::ConstInt(1),
            Operand::BitcastGlobal("__quantum__qis__x__ctl".into()),
            Operand::I8Null,
            opaque(1),
        ];
        let err = lower_call(&b, &signature, &args, None, &mut s).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("generalized-controlled intrinsic") && msg.contains("variadic=false"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn generalized_controlled_accepts_opaque_ptr_fifth_param() {
        // LLVM 15+ opaque-pointer variant: the 5th declared parameter is
        // canonicalized to `ptr` (vs. `i8` for the typed-pointer `i8*` form).
        // Both spellings appear in real producer output and both must be
        // routed to the same lowering path.
        let mut s = SymbolTable::new();
        let b = FunctionBuilder::GeneralizedControlled;
        let signature = FunctionSignature {
            name: "generalizedInvokeWithRotationsControlsTargets".into(),
            return_type: "void".into(),
            param_types: vec![
                "i64".into(),
                "i64".into(),
                "i64".into(),
                "i64".into(),
                "ptr".into(),
            ],
            is_variadic: true,
        };
        let args = vec![
            Operand::ConstInt(0),
            Operand::ConstInt(0),
            Operand::ConstInt(2),
            Operand::ConstInt(1),
            Operand::BitcastGlobal("__quantum__qis__x__ctl".into()),
            Operand::I8Null,
            opaque(1),
            opaque(2),
        ];
        let stmts = lower_call(&b, &signature, &args, None, &mut s).unwrap();
        assert_eq!(stmts.len(), 1);
        let crate::oq3::ast::Statement::QuantumGate { name, qubits, .. } = &stmts[0] else {
            panic!("expected QuantumGate")
        };
        assert_eq!(name, "ccnot");
        assert_eq!(qubits.len(), 3);
    }
}
