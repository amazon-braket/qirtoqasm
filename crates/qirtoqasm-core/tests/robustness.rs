//! Robustness tests: malformed inputs must surface a clean
//! `QirToQasmError` naming the offending shape rather than
//! panicking, silently miscompiling, or returning a misleading
//! error.

use qirtoqasm_core::{translate, QirToQasmError, TranslateOptions};

/// Wrap a QIR module body in the standard `%Qubit`/`%Result` opaque
/// declarations and an `attributes #0 = { "entry_point" }` block, so
/// individual tests can stay compact.
fn wrap(body: &str) -> String {
    format!(
        "%Qubit = type opaque\n%Result = type opaque\n\
         define void @main() #0 {{\n{body}\n}}\n\
         declare void @__quantum__qis__h__body(%Qubit*)\n\
         declare void @__quantum__qis__x__body(%Qubit*)\n\
         declare void @__quantum__qis__mz__body(%Qubit*, %Result*)\n\
         attributes #0 = {{ \"entry_point\" \"qir_profiles\"=\"base_profile\" }}\n"
    )
}

fn translate_wrapped(body: &str) -> Result<String, QirToQasmError> {
    translate(&wrap(body), &TranslateOptions::default())
}

#[test]
fn branch_to_undefined_block_label_surfaces_clear_error() {
    let body = "  call void @__quantum__qis__h__body(%Qubit* null)\n\
                br label %ghost";
    let err = translate_wrapped(body).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("branch target"), "{msg}");
    assert!(msg.contains("%ghost"), "{msg}");
}

#[test]
fn duplicate_block_label_surfaces_clear_error() {
    let ll = r#"
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
entry:
  br label %dup
dup:
  call void @__quantum__qis__h__body(%Qubit* null)
  br label %tail
dup:
  call void @__quantum__qis__x__body(%Qubit* null)
  br label %tail
tail:
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
attributes #0 = { "entry_point" "qir_profiles"="base_profile" }
"#;
    let err = translate(ll, &TranslateOptions::default()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("duplicate block label"), "{msg}");
    assert!(msg.contains("%dup"), "{msg}");
}

#[test]
fn qubit_index_overflow_surfaces_clear_error() {
    // `inttoptr (i64 9223372036854775807 ...)` uses i64::MAX as a
    // qubit index; register-size arithmetic must not wrap.
    let body = "  call void @__quantum__qis__h__body(\
                %Qubit* inttoptr (i64 9223372036854775807 to %Qubit*))\n\
                ret void";
    let err = translate_wrapped(body).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("qubit index too large"), "{msg}");
}

#[test]
fn unbalanced_closing_bracket_in_call_args_surfaces_syntax_error() {
    let body = "  call void @__quantum__qis__h__body(%Qubit* null }, %Qubit* null)\n\
                ret void";
    let err = translate_wrapped(body).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unbalanced closing bracket"), "{msg}");
}
