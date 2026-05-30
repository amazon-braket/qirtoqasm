// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! C ABI for the qirtoqasm translator.
//!
//! Designed for embedding in C / C++ projects that need to translate
//! QIR text to Braket-compatible OpenQASM 3 without a Python
//! interpreter. No Python on any call path; no LLVM dependency; no
//! hidden global state.
//!
//! # Options ABI stability
//!
//! All tunables flow through [`qirtoqasm_options_t`]. The struct
//! carries its own `struct_version` and `struct_size` so new fields
//! can be appended without invalidating binaries built against an
//! older header — the library size-gates reads against the caller's
//! declared size. Call [`qirtoqasm_options_init`] on a fresh struct
//! before setting fields, or pass `NULL` for all defaults.

#![deny(rust_2018_idioms)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Success code.
pub const QIRTOQASM_OK: c_int = 0;
/// QIR text could not be parsed.
pub const QIRTOQASM_ERR_SYNTAX: c_int = 1;
/// QIR contained an unsupported construct.
pub const QIRTOQASM_ERR_UNSUPPORTED: c_int = 2;
/// Entry-point CFG could not be reduced to structured OQ3.
pub const QIRTOQASM_ERR_UNSUPPORTED_CFG: c_int = 3;
/// Internal / unexpected failure.
pub const QIRTOQASM_ERR_INTERNAL: c_int = 4;

/// Current ABI version of [`qirtoqasm_options_t`]. Bumped when an
/// appended field's default-zero interpretation is not safe.
pub const QIRTOQASM_OPTIONS_VERSION: u32 = 1;

/// Tunables for [`qirtoqasm_translate`]. Fill via
/// [`qirtoqasm_options_init`] then set fields, or pass `NULL` for
/// defaults. Future fields are appended after the current tail.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct qirtoqasm_options_t {
    /// ABI version — set by [`qirtoqasm_options_init`].
    pub struct_version: u32,
    /// Struct size in bytes — set by [`qirtoqasm_options_init`].
    pub struct_size: u32,
    /// Upstream producer label (e.g. `"mylib 0.1.2"`); `NULL` or
    /// empty omits the field.
    pub producer: *const c_char,
}

impl Default for qirtoqasm_options_t {
    fn default() -> Self {
        Self {
            struct_version: QIRTOQASM_OPTIONS_VERSION,
            struct_size: std::mem::size_of::<Self>() as u32,
            producer: std::ptr::null(),
        }
    }
}

/// Initialize a caller-provided options struct to current defaults.
///
/// Sets `struct_version` / `struct_size` and clears every tunable.
/// Always call this once on a fresh struct before setting fields.
///
/// # Safety
/// - `opts` must point to a writable `qirtoqasm_options_t` slot.
/// - A `NULL` argument is a no-op.
#[no_mangle]
pub unsafe extern "C" fn qirtoqasm_options_init(opts: *mut qirtoqasm_options_t) {
    if opts.is_null() {
        return;
    }
    *opts = qirtoqasm_options_t::default();
}

/// Translate QIR text to Braket-compatible OpenQASM 3.
///
/// `opts` may be `NULL` (all defaults). If non-`NULL` the library
/// reads only fields whose byte offset is less than
/// `opts->struct_size`, so a caller built against an older header
/// keeps working against a newer library.
///
/// # Safety
/// - `qir` must point to a NUL-terminated UTF-8 string.
/// - `opts`, if non-`NULL`, must have been produced via
///   [`qirtoqasm_options_init`] (plus any subsequent per-field writes).
/// - `out` and `err` must point to writable `char*` slots.
/// - On success: `*out` is a heap-allocated NUL-terminated string;
///   `*err` is `NULL`. On failure: `*err` is a heap-allocated
///   NUL-terminated string; `*out` is `NULL`.
/// - The caller owns the returned strings and must free them with
///   [`qirtoqasm_free_string`].
#[no_mangle]
pub unsafe extern "C" fn qirtoqasm_translate(
    qir: *const c_char,
    opts: *const qirtoqasm_options_t,
    out: *mut *mut c_char,
    err: *mut *mut c_char,
) -> c_int {
    if out.is_null() || err.is_null() {
        return QIRTOQASM_ERR_INTERNAL;
    }
    *out = std::ptr::null_mut();
    *err = std::ptr::null_mut();
    if qir.is_null() {
        *err = rust_string_to_c("qirtoqasm_translate: qir pointer is NULL");
        return QIRTOQASM_ERR_INTERNAL;
    }
    let qir_cstr = CStr::from_ptr(qir);
    let qir_str = match qir_cstr.to_str() {
        Ok(s) => s,
        Err(_) => {
            *err = rust_string_to_c("qir input is not valid UTF-8");
            return QIRTOQASM_ERR_SYNTAX;
        }
    };
    let translate_options = match build_options(opts) {
        Ok(o) => o,
        Err(msg) => {
            *err = rust_string_to_c(&msg);
            return QIRTOQASM_ERR_INTERNAL;
        }
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        qirtoqasm_core::translate(qir_str, &translate_options)
    }));
    match result {
        Ok(Ok(qasm)) => {
            *out = rust_string_to_c(&qasm);
            QIRTOQASM_OK
        }
        Ok(Err(e)) => {
            let (code, msg) = match &e {
                qirtoqasm_core::QirToQasmError::Syntax(_) => (QIRTOQASM_ERR_SYNTAX, e.to_string()),
                qirtoqasm_core::QirToQasmError::Unsupported(_) => {
                    (QIRTOQASM_ERR_UNSUPPORTED, e.to_string())
                }
                qirtoqasm_core::QirToQasmError::UnsupportedCfg(_) => {
                    (QIRTOQASM_ERR_UNSUPPORTED_CFG, e.to_string())
                }
                qirtoqasm_core::QirToQasmError::Internal(_) => {
                    (QIRTOQASM_ERR_INTERNAL, e.to_string())
                }
            };
            *err = rust_string_to_c(&msg);
            code
        }
        Err(payload) => {
            let msg = panic_message(&payload);
            *err = rust_string_to_c(&format!("internal panic: {msg}"));
            QIRTOQASM_ERR_INTERNAL
        }
    }
}

/// Release a string previously returned by [`qirtoqasm_translate`].
/// A `NULL` pointer is a no-op.
///
/// # Safety
/// The pointer must have been produced by this library and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn qirtoqasm_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// Return the library version as a heap-allocated NUL-terminated string.
/// Caller owns it; free with [`qirtoqasm_free_string`].
#[no_mangle]
pub extern "C" fn qirtoqasm_version() -> *mut c_char {
    rust_string_to_c(qirtoqasm_core::VERSION)
}

/// Consume an optional NUL-terminated UTF-8 pointer. Empty → `None`.
unsafe fn read_optional_cstr(ptr: *const c_char) -> Result<Option<&'static str>, String> {
    if ptr.is_null() {
        return Ok(None);
    }
    match CStr::from_ptr(ptr).to_str() {
        Ok("") => Ok(None),
        Ok(s) => Ok(Some(s)),
        Err(_) => Err("options field is not valid UTF-8".to_string()),
    }
}

unsafe fn build_options(
    opts: *const qirtoqasm_options_t,
) -> Result<qirtoqasm_core::TranslateOptions, String> {
    if opts.is_null() {
        return Ok(qirtoqasm_core::TranslateOptions::default());
    }
    let v1_min = std::mem::size_of::<qirtoqasm_options_t>();
    let caller_size = std::ptr::read_unaligned(std::ptr::addr_of!((*opts).struct_size)) as usize;
    if caller_size < v1_min {
        return Err(format!(
            "qirtoqasm_options_t: struct_size {caller_size} is smaller than the V1 layout \
             ({v1_min} bytes); call qirtoqasm_options_init before use"
        ));
    }
    let caller_version = std::ptr::read_unaligned(std::ptr::addr_of!((*opts).struct_version));
    if caller_version == 0 {
        return Err(
            "qirtoqasm_options_t: struct_version is 0; call qirtoqasm_options_init before use"
                .to_string(),
        );
    }

    let producer_ptr = std::ptr::read_unaligned(std::ptr::addr_of!((*opts).producer));
    let producer = read_optional_cstr(producer_ptr)?;

    let mut out = qirtoqasm_core::TranslateOptions::default();
    if let Some(p) = producer {
        out = out.with_producer(p);
    }
    Ok(out)
}

fn rust_string_to_c(s: &str) -> *mut c_char {
    // Replace interior NULs with '?', which CString would otherwise refuse.
    let sanitised: String = s.chars().map(|c| if c == '\0' { '?' } else { c }).collect();
    CString::new(sanitised)
        .expect("no interior NUL after sanitisation")
        .into_raw()
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}
