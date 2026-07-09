//! C-ABI smoke tests exercised from Rust. Lines starting with
//! `// generated-by:` are stripped before byte-comparing against
//! the expected output.

use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::ptr;

use qirtoqasm::{qirtoqasm_options_t, QIRTOQASM_OK, QIRTOQASM_OPTIONS_VERSION};

const BELL_LL: &str = "; Bell state. Two qubits, prepared in |\u{03A6}+\u{27E9} and measured.
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cnot__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1

attributes #0 = { \"entry_point\" \"qir_profiles\"=\"base_profile\" \"requiredQubits\"=\"2\" \"requiredResults\"=\"2\" }
attributes #1 = { \"irreversible\" }
";

const BELL_QASM: &str = "OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cnot q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];
";

fn strip_generated_by(s: &str) -> String {
    let had_trailing_newline = s.ends_with('\n');
    let mut out = s
        .lines()
        .filter(|l| !l.starts_with("// generated-by:"))
        .collect::<Vec<_>>()
        .join("\n");
    if had_trailing_newline {
        out.push('\n');
    }
    out
}

fn translate_default(qir: &str) -> Result<String, String> {
    let c_qir = CString::new(qir).unwrap();
    let mut out: *mut std::os::raw::c_char = ptr::null_mut();
    let mut err: *mut std::os::raw::c_char = ptr::null_mut();
    let rc =
        unsafe { qirtoqasm::qirtoqasm_translate(c_qir.as_ptr(), ptr::null(), &mut out, &mut err) };
    collect(rc, out, err)
}

fn translate_with_producer(qir: &str, producer: Option<&str>) -> Result<String, String> {
    let c_qir = CString::new(qir).unwrap();
    let c_producer = producer.map(|s| CString::new(s).unwrap());

    let mut opts_uninit = MaybeUninit::<qirtoqasm_options_t>::uninit();
    unsafe { qirtoqasm::qirtoqasm_options_init(opts_uninit.as_mut_ptr()) };
    let mut opts = unsafe { opts_uninit.assume_init() };
    opts.producer = c_producer.as_ref().map_or(ptr::null(), |c| c.as_ptr());

    let mut out: *mut std::os::raw::c_char = ptr::null_mut();
    let mut err: *mut std::os::raw::c_char = ptr::null_mut();
    let rc = unsafe { qirtoqasm::qirtoqasm_translate(c_qir.as_ptr(), &opts, &mut out, &mut err) };
    collect(rc, out, err)
}

fn collect(
    rc: std::os::raw::c_int,
    out: *mut std::os::raw::c_char,
    err: *mut std::os::raw::c_char,
) -> Result<String, String> {
    if rc == QIRTOQASM_OK {
        let got = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_string();
        unsafe { qirtoqasm::qirtoqasm_free_string(out) };
        Ok(got)
    } else {
        let msg = unsafe { CStr::from_ptr(err) }.to_str().unwrap().to_string();
        unsafe { qirtoqasm::qirtoqasm_free_string(err) };
        Err(msg)
    }
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[test]
fn translates_bell_state_with_null_options() {
    let got = translate_default(BELL_LL).unwrap();
    assert_eq!(strip_generated_by(&got), strip_generated_by(BELL_QASM));
}

#[test]
fn translates_bell_state_with_initialized_options_no_producer() {
    let got = translate_with_producer(BELL_LL, None).unwrap();
    assert_eq!(strip_generated_by(&got), strip_generated_by(BELL_QASM));
}

#[test]
fn translate_emits_generated_by_line_by_default() {
    let got = translate_default(BELL_LL).unwrap();
    let last = got.lines().rfind(|l| !l.is_empty()).unwrap();
    assert!(
        last.starts_with("// generated-by: {\"name\":\"qirtoqasm\","),
        "{last}"
    );
    assert!(!last.contains("\"producer\""), "{last}");
}

#[test]
fn translate_with_producer_surfaces_caller_string() {
    let got = translate_with_producer(BELL_LL, Some("mylib 0.1.2")).unwrap();
    let last = got.lines().rfind(|l| !l.is_empty()).unwrap();
    assert!(last.contains(r#""producer":"mylib 0.1.2""#), "{last}");
}

#[test]
fn translate_with_empty_producer_omits_field() {
    let got = translate_with_producer(BELL_LL, Some("")).unwrap();
    let last = got.lines().rfind(|l| !l.is_empty()).unwrap();
    assert!(!last.contains("\"producer\""), "{last}");
}

#[test]
fn translate_with_null_producer_ptr_omits_field() {
    let got = translate_with_producer(BELL_LL, None).unwrap();
    let last = got.lines().rfind(|l| !l.is_empty()).unwrap();
    assert!(!last.contains("\"producer\""), "{last}");
}

// ---------------------------------------------------------------------------
// Error paths
// ---------------------------------------------------------------------------

#[test]
fn reports_syntax_error_through_c_abi() {
    let err = translate_default("{").unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn reports_error_when_options_struct_size_is_zero() {
    let c_qir = CString::new(BELL_LL).unwrap();
    let opts = qirtoqasm_options_t {
        struct_version: 0,
        struct_size: 0,
        producer: ptr::null(),
    };
    let mut out: *mut std::os::raw::c_char = ptr::null_mut();
    let mut err: *mut std::os::raw::c_char = ptr::null_mut();
    let rc = unsafe { qirtoqasm::qirtoqasm_translate(c_qir.as_ptr(), &opts, &mut out, &mut err) };
    assert_ne!(rc, QIRTOQASM_OK);
    assert!(out.is_null());
    let msg = unsafe { CStr::from_ptr(err) }.to_str().unwrap().to_string();
    assert!(msg.contains("struct_size"), "{msg}");
    unsafe { qirtoqasm::qirtoqasm_free_string(err) };
}

#[test]
fn reports_error_when_options_struct_version_is_zero() {
    let c_qir = CString::new(BELL_LL).unwrap();
    let opts = qirtoqasm_options_t {
        struct_version: 0,
        struct_size: std::mem::size_of::<qirtoqasm_options_t>() as u32,
        producer: ptr::null(),
    };
    let mut out: *mut std::os::raw::c_char = ptr::null_mut();
    let mut err: *mut std::os::raw::c_char = ptr::null_mut();
    let rc = unsafe { qirtoqasm::qirtoqasm_translate(c_qir.as_ptr(), &opts, &mut out, &mut err) };
    assert_ne!(rc, QIRTOQASM_OK);
    let msg = unsafe { CStr::from_ptr(err) }.to_str().unwrap().to_string();
    assert!(msg.contains("struct_version"), "{msg}");
    unsafe { qirtoqasm::qirtoqasm_free_string(err) };
}

#[test]
fn reports_error_when_qir_pointer_is_null() {
    let mut out: *mut std::os::raw::c_char = ptr::null_mut();
    let mut err: *mut std::os::raw::c_char = ptr::null_mut();
    let rc =
        unsafe { qirtoqasm::qirtoqasm_translate(ptr::null(), ptr::null(), &mut out, &mut err) };
    assert_ne!(rc, QIRTOQASM_OK);
    assert!(out.is_null());
    let msg = unsafe { CStr::from_ptr(err) }.to_str().unwrap().to_string();
    assert!(msg.contains("NULL"), "{msg}");
    unsafe { qirtoqasm::qirtoqasm_free_string(err) };
}

// ---------------------------------------------------------------------------
// Housekeeping endpoints
// ---------------------------------------------------------------------------

#[test]
fn free_string_tolerates_null() {
    unsafe { qirtoqasm::qirtoqasm_free_string(ptr::null_mut()) };
}

#[test]
fn options_init_tolerates_null() {
    unsafe { qirtoqasm::qirtoqasm_options_init(ptr::null_mut()) };
}

#[test]
fn options_init_sets_version_and_size() {
    let mut opts_uninit = MaybeUninit::<qirtoqasm_options_t>::uninit();
    unsafe { qirtoqasm::qirtoqasm_options_init(opts_uninit.as_mut_ptr()) };
    let opts = unsafe { opts_uninit.assume_init() };
    assert_eq!(opts.struct_version, QIRTOQASM_OPTIONS_VERSION);
    assert_eq!(
        opts.struct_size as usize,
        std::mem::size_of::<qirtoqasm_options_t>()
    );
    assert!(opts.producer.is_null());
}

#[test]
fn version_is_a_non_empty_string() {
    let ptr = qirtoqasm::qirtoqasm_version();
    assert!(!ptr.is_null());
    let v = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    assert!(!v.is_empty());
    unsafe { qirtoqasm::qirtoqasm_free_string(ptr) };
}
