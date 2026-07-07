// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Test-only helpers shared across `#[cfg(test)] mod tests` blocks.
//!
//! Exists to eliminate the dozens of repeated `%Qubit = type opaque…
//! declare …__quantum__qis…__body… attributes #0 = { "entry_point" }`
//! preambles that otherwise dominate the test sections.

#![allow(dead_code)] // not every helper is used by every consumer module

use crate::ir::parser::parse_module;
use crate::ir::Operand;
use crate::translator::Exporter;

/// Standard set of QIS / RT declarations every wrapped-fixture body
/// might call. Kept as a fixed superset so `qir_entry` is parser-free.
pub const STANDARD_DECLARES: &str = "\
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__y__body(%Qubit*)
declare void @__quantum__qis__z__body(%Qubit*)
declare void @__quantum__qis__s__body(%Qubit*)
declare void @__quantum__qis__s__adj(%Qubit*)
declare void @__quantum__qis__t__body(%Qubit*)
declare void @__quantum__qis__t__adj(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__cy__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__cz__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__swap__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)
declare void @__quantum__qis__ccnot__body(%Qubit*, %Qubit*, %Qubit*)
declare void @__quantum__qis__rx__body(double, %Qubit*)
declare void @__quantum__qis__ry__body(double, %Qubit*)
declare void @__quantum__qis__rz__body(double, %Qubit*)
declare void @__quantum__qis__rxx__body(double, %Qubit*, %Qubit*)
declare void @__quantum__qis__ryy__body(double, %Qubit*, %Qubit*)
declare void @__quantum__qis__rzz__body(double, %Qubit*, %Qubit*)
declare void @__quantum__qis__phasedx__body(double, double, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1
declare void @__quantum__qis__reset__body(%Qubit*)
declare i1 @__quantum__qis__read_result__body(%Result*)
declare i1 @__quantum__rt__read_result(%Result*)
declare void @__quantum__rt__result_record_output(%Result*, i8*)
declare void @__quantum__rt__array_record_output(i64, i8*)
declare void @__quantum__rt__tuple_record_output(i64, i8*)
declare void @__quantum__rt__integer_record_output(i64, i8*)
declare void @__quantum__rt__int_record_output(i64, i8*)
declare void @__quantum__rt__bool_record_output(i1, i8*)
declare void @__quantum__rt__double_record_output(double, i8*)
declare void @__quantum__rt__initialize(i8*)
declare void @unknown_intrinsic(%Qubit*)";

/// Wrap a function body in a complete entry-point QIR module.
///
/// The output begins with the standard `%Qubit`/`%Result` opaque-type
/// declarations and ends with the standard `attributes #0` block
/// tagging the function as the entry point. The whole [`STANDARD_DECLARES`]
/// superset is appended unconditionally; tests don't care about
/// `declare` lines, only about the body.
pub fn qir_entry(body: &str) -> String {
    format!(
        "%Qubit = type opaque\n%Result = type opaque\n\
         define void @main() #0 {{\n{body}\n}}\n\
         {STANDARD_DECLARES}\n\
         attributes #0 = {{ \"entry_point\" }}\n\
         attributes #1 = {{ \"irreversible\" }}\n"
    )
}

/// Wrap `body`, parse, and translate to OQ3 source. Panics on any
/// parse / translate failure so tests can `assert!(out.contains(…))`
/// directly.
pub fn translate(body: &str) -> String {
    let qir = qir_entry(body);
    let module = parse_module(&qir).expect("parse_module failed in test_support::translate");
    Exporter::new()
        .dumps(&module)
        .expect("Exporter::dumps failed in test_support::translate")
}

/// Wrap `body`, parse, and translate; return the error string.
/// Panics if translation unexpectedly succeeds.
pub fn translate_err(body: &str) -> String {
    let qir = qir_entry(body);
    let module = parse_module(&qir).expect("parse_module failed in test_support::translate_err");
    Exporter::new()
        .dumps(&module)
        .expect_err("expected translate to fail")
        .to_string()
}

/// Translate raw QIR source (no body wrapping). Panics on failure.
pub fn translate_raw(qir: &str) -> String {
    let module = parse_module(qir).expect("parse_module failed in test_support::translate_raw");
    Exporter::new()
        .dumps(&module)
        .expect("Exporter::dumps failed in test_support::translate_raw")
}

/// Translate raw QIR source (no body wrapping); return the error string.
pub fn translate_raw_err(qir: &str) -> String {
    let module = parse_module(qir).expect("parse_module failed in test_support::translate_raw_err");
    Exporter::new()
        .dumps(&module)
        .expect_err("expected translate to fail")
        .to_string()
}

/// `Operand::PtrConst` typed as a `%Qubit*` pointer to qubit `i`.
pub fn qubit(i: i64) -> Operand {
    Operand::PtrConst {
        struct_name: Some("Qubit".into()),
        index: i,
    }
}

/// `Operand::PtrConst` typed as a `%Result*` pointer to result `i`.
pub fn result(i: i64) -> Operand {
    Operand::PtrConst {
        struct_name: Some("Result".into()),
        index: i,
    }
}

/// `Operand::PtrConst` typed as an opaque `ptr` (LLVM-15+ form),
/// at index `i`.
pub fn opaque(i: i64) -> Operand {
    Operand::PtrConst {
        struct_name: None,
        index: i,
    }
}
