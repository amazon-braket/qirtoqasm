// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! qirtoqasm-core: pure-Rust QIR → Braket-compatible OpenQASM 3 translator.
//!
//! The public surface is [`translate`], [`TranslateOptions`],
//! [`QirToQasmError`] / [`Result`], and [`VERSION`].
//!
//! # Scaffolding stub
//!
//! This is the initial scaffolding. The public API shape is final but
//! [`translate`] currently returns
//! `Err(QirToQasmError::Unsupported("translate is not yet implemented"))`
//! for every input. The IR parser, the OpenQASM 3 printer, and the
//! translator wiring will be added later; the stub is replaced once
//! they land.
//!
//! # Options
//!
//! All tunables flow through [`TranslateOptions`]. The struct is
//! `#[non_exhaustive]`; external callers construct it via
//! [`TranslateOptions::default()`] or the [builder methods] and add
//! fields with `..Default::default()` struct-update syntax, which
//! leaves room for append-only field additions without breaking them.
//!
//! [builder methods]: TranslateOptions#impl-TranslateOptions

#![deny(rust_2018_idioms)]
#![warn(missing_docs)]

pub mod error;
pub mod oq3;
pub mod profile;

pub use error::{QirToQasmError, Result};

/// Tunables for [`translate`]. Construct via [`Self::default()`];
/// `#[non_exhaustive]` reserves room for future fields.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct TranslateOptions {
    /// Upstream producer label surfaced as the `"producer"` field in
    /// the trailing `// generated-by:` comment (e.g.
    /// `Some("mylib 0.1.2".into())`). `None` or empty omits it.
    pub producer: Option<String>,
}

impl TranslateOptions {
    /// Builder-style setter for [`Self::producer`]. Empty clears.
    pub fn with_producer(mut self, producer: impl Into<String>) -> Self {
        let s = producer.into();
        self.producer = if s.is_empty() { None } else { Some(s) };
        self
    }
}

/// Parse QIR text and translate it to Braket-compatible OpenQASM 3.
///
/// **Scaffolding stub.** Returns
/// `Err(QirToQasmError::Unsupported("translate is not yet implemented"))`
/// for every input. The real translator replaces this stub once it is
/// implemented.
pub fn translate(_qir_text: &str, _options: &TranslateOptions) -> Result<String> {
    Err(QirToQasmError::Unsupported(
        "translate is not yet implemented".into(),
    ))
}

/// Package version, baked in at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_returns_not_yet_implemented() {
        let err = translate("", &TranslateOptions::default()).unwrap_err();
        assert_eq!(
            err,
            QirToQasmError::Unsupported("translate is not yet implemented".into())
        );
        assert_eq!(err.to_string(), "translate is not yet implemented");
    }

    #[test]
    fn translate_returns_not_yet_implemented_with_producer() {
        let err = translate(
            "anything",
            &TranslateOptions::default().with_producer("mylib 0.1.2"),
        )
        .unwrap_err();
        assert!(matches!(err, QirToQasmError::Unsupported(_)));
    }

    #[test]
    fn translate_options_default_is_none() {
        assert!(TranslateOptions::default().producer.is_none());
    }

    #[test]
    fn translate_options_with_producer_sets_field() {
        let opts = TranslateOptions::default().with_producer("mylib 0.1.2");
        assert_eq!(opts.producer.as_deref(), Some("mylib 0.1.2"));
    }

    #[test]
    fn translate_options_with_empty_producer_clears() {
        let opts = TranslateOptions::default()
            .with_producer("mylib 0.1.2")
            .with_producer("");
        assert!(opts.producer.is_none());
    }

    #[test]
    fn translate_options_is_cloneable() {
        let opts = TranslateOptions::default().with_producer("x");
        assert_eq!(opts.clone().producer.as_deref(), Some("x"));
    }
}
