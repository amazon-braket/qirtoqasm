// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Minimal OpenQASM 3 AST + byte-exact pretty-printer.
//!
//! Only the subset of OQ3 the translator actually emits is modeled.
//! The printer matches the formatting of the official
//! [`openqasm3`](https://pypi.org/project/openqasm3/) Python pretty-printer
//! (with the defaults `indent="  "`, `chain_else_if=True`,
//! `old_measurement=False`) for the emit subset we use.

pub mod ast;
pub mod printer;

pub use ast::*;
pub use printer::print;
