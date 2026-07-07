// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! qirtoqasm-core: pure-Rust QIR → Braket-compatible OpenQASM 3 translator.
//!
//! The public surface is [`translate`], [`TranslateOptions`],
//! [`QirToQasmError`] / [`Result`], and [`VERSION`]. All other modules
//! are implementation detail — technically reachable as
//! `qirtoqasm_core::<module>::*` for workspace use, but not part of the
//! crate's stable surface.
//!
//! # Options
//!
//! All tunables flow through [`TranslateOptions`]. The struct is
//! `#[non_exhaustive]`; external callers construct it via
//! [`TranslateOptions::default()`] or the [builder methods] and add
//! fields with `..Default::default()` struct-update syntax, which
//! leaves room for append-only field additions without breaking them.
//!
//! ```
//! use qirtoqasm_core::{translate, TranslateOptions};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let qir = "\
//! # %Qubit = type opaque
//! # define void @main() #0 {
//! #   call void @__quantum__qis__h__body(%Qubit* null)
//! #   ret void
//! # }
//! # declare void @__quantum__qis__h__body(%Qubit*)
//! # attributes #0 = { \"entry_point\" \"qir_profiles\"=\"base_profile\" \"requiredQubits\"=\"1\" \"requiredResults\"=\"0\" }
//! # ";
//! let _qasm = translate(qir, &TranslateOptions::default())?;
//! let _qasm = translate(qir, &TranslateOptions::default().with_producer("mylib 0.1.2"))?;
//! # Ok(())
//! # }
//! ```
//!
//! [builder methods]: TranslateOptions#impl-TranslateOptions

#![deny(rust_2018_idioms)]
#![warn(missing_docs)]

pub mod boolean;
pub mod builders;
pub mod cfg;
pub mod error;
pub mod ir;
pub mod oq3;
pub mod profile;
pub mod signatures;
pub mod symbols;
pub mod translator;

#[cfg(test)]
pub(crate) mod test_support;

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
/// Pass `&TranslateOptions::default()` for defaults.
pub fn translate(qir_text: &str, options: &TranslateOptions) -> Result<String> {
    let module = ir::parser::parse_module(qir_text)?;
    let mut exporter = translator::Exporter::new();
    if let Some(p) = &options.producer {
        exporter = exporter.with_producer(p);
    }
    exporter.dumps(&module)
}

/// Package version, baked in at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_LL: &str = "\
%Qubit = type opaque
define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
attributes #0 = { \"entry_point\" \"qir_profiles\"=\"base_profile\" \"requiredQubits\"=\"1\" \"requiredResults\"=\"0\" }
";

    #[test]
    fn default_options_omit_producer_field() {
        let out = translate(MINIMAL_LL, &TranslateOptions::default()).unwrap();
        assert!(!out.contains("\"producer\""), "{out}");
    }

    #[test]
    fn producer_option_surfaces_in_generated_by_line() {
        let out = translate(
            MINIMAL_LL,
            &TranslateOptions::default().with_producer("mylib 0.1.2"),
        )
        .unwrap();
        assert!(out.contains(r#""producer":"mylib 0.1.2""#), "{out}");
    }

    #[test]
    fn empty_producer_option_omits_field() {
        let out = translate(MINIMAL_LL, &TranslateOptions::default().with_producer("")).unwrap();
        assert!(!out.contains("\"producer\""), "{out}");
    }

    #[test]
    fn with_producer_empty_overrides_prior_value() {
        let out = translate(
            MINIMAL_LL,
            &TranslateOptions::default()
                .with_producer("mylib 0.1.2")
                .with_producer(""),
        )
        .unwrap();
        assert!(!out.contains("\"producer\""), "{out}");
    }

    #[test]
    fn translate_options_default_is_none() {
        assert!(TranslateOptions::default().producer.is_none());
    }

    #[test]
    fn translate_options_is_cloneable() {
        let opts = TranslateOptions::default().with_producer("x");
        assert_eq!(opts.clone().producer.as_deref(), Some("x"));
    }
}
