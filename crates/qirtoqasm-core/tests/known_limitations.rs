//! Known-limitation negative-path integration tests.
//!
//! Each fixture here is a `.ll` with no sibling `.qasm`; translation must
//! fail with [`QirToQasmError::Unsupported`] (surfaced as the FFI's
//! `QIRTOQASM_ERR_UNSUPPORTED` code and the Python shim's `QirToQasmError`
//! with `code == QIRTOQASM_ERR_UNSUPPORTED`), and the message must contain
//! a pinned substring.

use std::fs;
use std::path::PathBuf;

use qirtoqasm_core::QirToQasmError;

fn fixture(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("test")
        .join("integ_tests")
        .join("fixtures_qir")
        .join(name)
}

fn translate_fixture(name: &str) -> Result<String, QirToQasmError> {
    let src = fs::read_to_string(fixture(name)).unwrap();
    qirtoqasm_core::translate(&src, &qirtoqasm_core::TranslateOptions::default())
}

#[test]
fn unsupported_controlled_h_names_the_inner_callee() {
    // Controlled-H has no Braket-native single-gate spelling, so the
    // error should name both the op and the unsupported (controls,
    // targets) counts.
    let err = translate_fixture("cudaq_unsupported_ctrl_h.ll").unwrap_err();
    let msg = err.to_string();
    assert!(matches!(err, QirToQasmError::Unsupported(_)));
    assert!(msg.contains("no Braket-native gate"), "{msg}");
    assert!(msg.contains("``h``"), "{msg}");
    assert!(msg.contains("numControls=1"), "{msg}");
    assert!(msg.contains("numTargets=1"), "{msg}");
}
