// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! PyO3 bindings backing the public `qirtoqasm` Python package.
//!
//! Public surface:
//! - [`translate(qir_text, *, producer=None) -> str`](translate) — one-stage
//!   translation. Every tunable is keyword-only with a default, so new
//!   options can be added without breaking existing callers.
//! - `QirToQasmError` — raised on any translation failure.
//! - `__version__` — module-level version string.

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

use qirtoqasm_core as core;

create_exception!(_qirtoqasm_native, QirToQasmError, PyException);

fn map_err(err: core::QirToQasmError) -> PyErr {
    QirToQasmError::new_err(err.to_string())
}

/// Translate QIR text to Braket-compatible OpenQASM 3.
///
/// ``producer``, if given and non-empty, is surfaced as the
/// ``"producer"`` field in the trailing ``// generated-by:`` comment
/// (e.g. ``"mylib 0.1.2"``). ``None`` or empty omits the field.
///
/// Raises ``QirToQasmError`` on any translation failure.
#[pyfunction]
#[pyo3(signature = (qir_text, *, producer=None))]
fn translate(qir_text: &str, producer: Option<&str>) -> PyResult<String> {
    let mut options = core::TranslateOptions::default();
    if let Some(p) = producer {
        options = options.with_producer(p);
    }
    core::translate(qir_text, &options).map_err(map_err)
}

#[pymodule]
#[pyo3(name = "_qirtoqasm_native")]
fn module_init(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add("__version__", core::VERSION)?;
    m.add("QirToQasmError", py.get_type::<QirToQasmError>())?;
    m.add_function(wrap_pyfunction!(translate, m)?)?;
    Ok(())
}
