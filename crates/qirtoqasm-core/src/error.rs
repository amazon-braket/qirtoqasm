// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Error type used by every module in the core.

use thiserror::Error;

/// Every error the translator may surface.
///
/// The variant determines which Python exception class the bindings
/// layer raises and which C ABI code the FFI layer returns.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QirToQasmError {
    /// Parse-time failure on QIR source text.
    #[error("{0}")]
    Syntax(String),
    /// A QIR construct with no supported lowering.
    #[error("{0}")]
    Unsupported(String),
    /// Entry-point control-flow graph could not be reduced to structured OQ3.
    #[error("{0}")]
    UnsupportedCfg(String),
    /// Unexpected internal failure.
    #[error("internal error: {0}")]
    Internal(String),
}

impl QirToQasmError {
    /// Construct a syntax error.
    pub fn syntax(msg: impl Into<String>) -> Self {
        Self::Syntax(msg.into())
    }
    /// Construct an unsupported-construct error.
    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::Unsupported(msg.into())
    }
    /// Construct an unsupported-CFG error.
    pub fn unsupported_cfg(msg: impl Into<String>) -> Self {
        Self::UnsupportedCfg(msg.into())
    }
    /// Construct an internal error.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

/// `Result<T>` specialized to [`QirToQasmError`].
pub type Result<T> = std::result::Result<T, QirToQasmError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_strips_variant_prefix_except_internal() {
        assert_eq!(QirToQasmError::Syntax("oops".into()).to_string(), "oops");
        assert_eq!(
            QirToQasmError::Unsupported("not yet".into()).to_string(),
            "not yet"
        );
        assert_eq!(
            QirToQasmError::UnsupportedCfg("weird cfg".into()).to_string(),
            "weird cfg"
        );
        assert_eq!(
            QirToQasmError::Internal("bug".into()).to_string(),
            "internal error: bug"
        );
    }

    #[test]
    fn constructors_wrap_strings() {
        assert_eq!(
            QirToQasmError::syntax("x"),
            QirToQasmError::Syntax("x".into())
        );
        assert_eq!(
            QirToQasmError::unsupported("x"),
            QirToQasmError::Unsupported("x".into())
        );
        assert_eq!(
            QirToQasmError::unsupported_cfg("x"),
            QirToQasmError::UnsupportedCfg("x".into())
        );
        assert_eq!(
            QirToQasmError::internal("x"),
            QirToQasmError::Internal("x".into())
        );
    }
}
