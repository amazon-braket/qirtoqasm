// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Minimal OpenQASM 3 AST modelled by the translator.
//!
//! Only the subset of OQ3 the translator actually emits is modelled.

pub mod ast;

pub use ast::*;
