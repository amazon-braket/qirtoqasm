// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Text-level function-signature extraction from QIR source.
//!
//! Scans `declare` / `define` lines and recovers return type, function
//! name, parameter types, and varargs flag in their canonical forms
//! (`"Qubit"`, `"Result"`, `"i1"`, `"double"`, `"ptr"`, `"void"`).

use indexmap::IndexMap;

use crate::error::{QirToQasmError, Result};

/// Canonical signature of a QIR function.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionSignature {
    /// Function name (without leading `@`).
    pub name: String,
    /// Canonical return type string.
    pub return_type: String,
    /// Canonical parameter type strings, in source order.
    pub param_types: Vec<String>,
    /// `true` if the parameter list ends with `...`.
    pub is_variadic: bool,
}

impl FunctionSignature {
    /// Whether the return type is `"void"`.
    pub fn is_void(&self) -> bool {
        self.return_type == "void"
    }
}

/// Signature-table keyed by function name.
#[derive(Debug, Clone, Default)]
pub struct SignatureTable {
    map: IndexMap<String, FunctionSignature>,
}

impl SignatureTable {
    /// Does the table contain a signature for `name`?
    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    /// Look up a signature by name.
    pub fn get(&self, name: &str) -> Option<&FunctionSignature> {
        self.map.get(name)
    }

    /// Return the number of signatures.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Return `true` if the table is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Iterate over signatures in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &FunctionSignature> {
        self.map.values()
    }

    fn insert(&mut self, sig: FunctionSignature) {
        self.map.insert(sig.name.clone(), sig);
    }
}

/// Parse every `declare` / `define` line in `qir_source` and return the
/// resulting signature table.
pub fn extract_signatures(qir_source: &str) -> Result<SignatureTable> {
    let mut table = SignatureTable::default();
    for raw_line in qir_source.lines() {
        let line = raw_line.trim_end();
        let stripped = line.trim_start();
        let keyword = if starts_with_keyword(stripped, "declare") {
            "declare"
        } else if starts_with_keyword(stripped, "define") {
            "define"
        } else {
            continue;
        };
        let sig = parse_signature_line(line, keyword)?;
        let sig = validate_signature(sig)?;
        table.insert(sig);
    }
    Ok(table)
}

fn parse_signature_line(line: &str, keyword: &str) -> Result<FunctionSignature> {
    let idx = line.find(keyword).ok_or_else(|| {
        QirToQasmError::syntax(format!("could not parse signature line: {line:?}"))
    })?;
    let rest = &line[idx + keyword.len()..];

    // After the keyword, an optional linkage/visibility string may appear
    // before the return type. For `define`, common ones include
    // `dso_local`, `internal`, `weak`, `linkonce`, etc. We skip any
    // sequence of word tokens that don't begin with an LLVM type character.
    let mut rest = rest.trim_start();

    // Strip common linkage/visibility/calling-convention keywords that
    // can appear between `define` and the return type.
    let prefixes = [
        "dso_local",
        "local_unnamed_addr",
        "internal",
        "private",
        "linkonce",
        "linkonce_odr",
        "weak",
        "weak_odr",
        "external",
        "common",
        "appending",
        "available_externally",
        "hidden",
        "protected",
        "default",
        "signext",
        "zeroext",
        "ccc",
        "fastcc",
        "coldcc",
        "tailcc",
        "cc",
    ];
    loop {
        let tok_end = rest
            .bytes()
            .position(|b| b.is_ascii_whitespace())
            .unwrap_or(rest.len());
        let tok = &rest[..tok_end];
        if tok.is_empty() {
            return Err(QirToQasmError::syntax(format!(
                "could not parse signature line: {line:?}"
            )));
        }
        if prefixes.contains(&tok) {
            rest = rest[tok_end..].trim_start();
            continue;
        }
        break;
    }

    // Return type is the next whitespace-delimited token, OR a
    // brace-enclosed struct type like `{ i1*, i64 }`. Struct types
    // may be nested (`{ { i1 }, i64 }`), so we track brace depth
    // rather than stopping at the first `}`.
    let (return_type_raw, rest) = if rest.starts_with('{') {
        let close = matching_close_brace(rest).ok_or_else(|| {
            QirToQasmError::syntax(format!("unclosed struct return type: {line:?}"))
        })?;
        (&rest[..=close], rest[close + 1..].trim_start())
    } else {
        let tok_end = rest
            .bytes()
            .position(|b| b.is_ascii_whitespace())
            .unwrap_or(rest.len());
        (&rest[..tok_end], rest[tok_end..].trim_start())
    };

    // Function name follows, preceded by `@`.
    let rest = rest.strip_prefix('@').ok_or_else(|| {
        QirToQasmError::syntax(format!("could not parse signature line: {line:?}"))
    })?;
    let (name, after_name) = if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped
            .find('"')
            .ok_or_else(|| QirToQasmError::syntax(format!("unterminated quoted name: {line:?}")))?;
        (stripped[..end].to_string(), &stripped[end + 1..])
    } else {
        let end = rest
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$')))
            .unwrap_or(rest.len());
        if end == 0 {
            return Err(QirToQasmError::syntax(format!(
                "could not parse signature line: {line:?}"
            )));
        }
        (rest[..end].to_string(), &rest[end..])
    };

    // Parameter list is enclosed in `( … )`.
    let open = after_name
        .find('(')
        .ok_or_else(|| QirToQasmError::syntax(format!("signature missing '(': {line:?}")))?;
    let close_rel =
        crate::ir::parser_util::matching_close_paren(&after_name[open..]).ok_or_else(|| {
            QirToQasmError::syntax(format!("unbalanced parens in signature: {line:?}"))
        })?;
    let params_src = &after_name[open + 1..open + close_rel];

    let (param_types, is_variadic) = parse_param_list(params_src)?;

    Ok(FunctionSignature {
        name,
        return_type: return_type_raw.to_string(),
        param_types,
        is_variadic,
    })
}

fn validate_signature(sig: FunctionSignature) -> Result<FunctionSignature> {
    let FunctionSignature {
        name,
        return_type,
        param_types,
        is_variadic,
    } = sig;

    let ret_canon = canonicalize_type(&return_type)?;
    let params_canon: Result<Vec<String>> = param_types
        .into_iter()
        .map(|p| canonicalize_type(&p))
        .collect();

    Ok(FunctionSignature {
        name,
        return_type: ret_canon,
        param_types: params_canon?,
        is_variadic,
    })
}

/// Strip leading LLVM parameter attributes (writeonly, nocapture, …)
/// from a parameter chunk, returning the slice starting at the type.
fn strip_leading_attrs<'a>(chunk: &'a str, attrs: &[&str]) -> &'a str {
    let mut rest = chunk.trim_start();
    loop {
        let end = rest
            .bytes()
            .position(|b| b.is_ascii_whitespace())
            .unwrap_or(rest.len());
        if end == 0 {
            return rest;
        }
        let first = &rest[..end];
        if attrs.contains(&first) {
            rest = rest[end..].trim_start();
        } else {
            return rest;
        }
    }
}

fn parse_param_list(src: &str) -> Result<(Vec<String>, bool)> {
    let src = src.trim();
    if src.is_empty() {
        return Ok((Vec::new(), false));
    }
    let mut parts = crate::ir::parser_util::split_top_level_commas(src)?;
    let mut is_variadic = false;
    if parts.last().map(|s| *s == "...").unwrap_or(false) {
        is_variadic = true;
        parts.pop();
    }
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        if p.is_empty() {
            return Err(QirToQasmError::syntax(format!(
                "empty parameter in signature: {src:?}"
            )));
        }
        if p == "..." {
            return Err(QirToQasmError::syntax(format!(
                "unexpected '...' in non-terminal position of parameter list: {src:?}"
            )));
        }
        // Strip parameter attributes (writeonly, readonly, etc.). The
        // attribute always precedes the type; the type token we want is
        // the last non-attribute word.
        let attrs = [
            "writeonly",
            "readonly",
            "readnone",
            "nonnull",
            "noalias",
            "nocapture",
            "immarg",
            "signext",
            "zeroext",
            "byval",
            "sret",
            "inreg",
            "noundef",
        ];
        // If the parameter type is an inline struct literal like
        // `{ double*, i64 } %0`, the whitespace-split produces tokens
        // that individually look nothing like types. Detect the `{`
        // prefix and carve the struct type out wholesale. Struct
        // types may be nested, so we track brace depth from the
        // leading `{` rather than scanning for the last `}` in the
        // chunk.
        let p_stripped = strip_leading_attrs(p, &attrs);
        let ty = if p_stripped.starts_with('{') {
            match matching_close_brace(p_stripped) {
                Some(end) => p_stripped[..=end].to_string(),
                None => {
                    return Err(QirToQasmError::syntax(format!(
                        "unterminated struct parameter type in signature: {src:?}"
                    )))
                }
            }
        } else {
            let mut tokens: Vec<&str> = p_stripped.split_ascii_whitespace().collect();
            tokens.retain(|t| !attrs.contains(t));
            tokens
                .first()
                .copied()
                .ok_or_else(|| {
                    QirToQasmError::syntax(format!(
                        "could not parse parameter type from chunk {p:?} in {src:?}"
                    ))
                })?
                .to_string()
        };
        out.push(ty);
    }
    Ok((out, is_variadic))
}

/// Normalize an LLVM type token to our canonical form.
///
/// Rules:
///   * `%Qubit*` → `"Qubit"`    (typed struct pointer → struct name)
///   * `%"Qubit"*` → `"Qubit"`  (quoted identified struct pointer)
///   * `%Qubit` → `"Qubit"`     (identified struct without pointer)
///   * `%"Qubit"` → `"Qubit"`   (ditto, quoted)
///   * `ptr` → `"ptr"`          (opaque pointer passes through)
///   * `i1`, `i32`, `i64`, `double`, `float`, `half`, `void` → passthrough
///   * `i32*`, `double*`, etc. → the primitive name (trailing `*`
///     stripped). The signature table classifies parameter shape for
///     qubit/result/classical-scalar dispatch and does not preserve
///     pointer-vs-value distinctions on primitives, since those only
///     ever appear inside function bodies, not in the declare/define
///     signatures the table indexes.
pub fn canonicalize_type(raw: &str) -> Result<String> {
    let token = raw.trim();
    if token.is_empty() {
        return Err(QirToQasmError::syntax("empty type token in signature"));
    }
    let is_pointer = token.ends_with('*');
    let core = if is_pointer {
        &token[..token.len() - 1]
    } else {
        token
    };
    if let Some(stripped) = core.strip_prefix("%\"").and_then(|r| r.strip_suffix('"')) {
        return Ok(stripped.to_string());
    }
    if let Some(name) = core.strip_prefix('%') {
        if name.is_empty() || !crate::ir::parser_util::is_valid_llvm_ident(name) {
            return Err(QirToQasmError::syntax(format!(
                "could not parse parameter type {:?}",
                raw
            )));
        }
        return Ok(name.to_string());
    }
    // Bare primitive / opaque-ptr / keyword type. Accept only a known
    // set plus the integer-width form ``i<digits>`` and the floating
    // point family; reject anything else so errant tokens like
    // ``@@@bogus`` surface loudly.
    if is_known_primitive(core) || is_integer_type(core) {
        return Ok(core.to_string());
    }
    // LLVM struct-by-value types, e.g. `{ double*, i64 }` (the
    // list-typed kernel parameter lowering some producers emit). We
    // never treat these as qubit/result operands; canonicalize to the
    // literal "struct" so the signature stays parseable and downstream
    // dispatch doesn't confuse them with anything else.
    if core.starts_with('{') && core.ends_with('}') {
        return Ok("struct".to_string());
    }
    Err(QirToQasmError::syntax(format!(
        "could not parse parameter type {:?}",
        raw
    )))
}

fn is_integer_type(s: &str) -> bool {
    if let Some(rest) = s.strip_prefix('i') {
        !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
    } else {
        false
    }
}

fn is_known_primitive(s: &str) -> bool {
    matches!(
        s,
        "void" | "double" | "float" | "half" | "ptr" | "bfloat" | "fp128"
    )
}

/// Whether `s` starts with the keyword `kw` followed by a word
/// boundary (ASCII whitespace or end of string). Prevents
/// false-matches on lookalikes such as `declareSomething` or
/// `define_foo` that share a prefix with the real `declare` /
/// `define` keywords but are unrelated identifiers, while still
/// recognizing a bare `declare` / `define` with no trailing content
/// as a (malformed) signature line worth surfacing as a parse error
/// rather than silently skipping.
fn starts_with_keyword(s: &str, kw: &str) -> bool {
    s.strip_prefix(kw)
        .map(|rest| rest.bytes().next().is_none_or(|b| b.is_ascii_whitespace()))
        .unwrap_or(false)
}

/// Given a string that begins with `{`, return the byte index of the
/// matching closing `}` (depth-aware) or `None` if no match exists.
/// Used to carve out brace-enclosed struct types — including nested
/// shapes like `{ { i1 }, i64 }` — without bailing at the first `}`.
fn matching_close_brace(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_struct_and_primitive_types() {
        assert_eq!(canonicalize_type("%Qubit*").unwrap(), "Qubit");
        assert_eq!(canonicalize_type("%Result*").unwrap(), "Result");
        assert_eq!(canonicalize_type("%\"Qubit\"*").unwrap(), "Qubit");
        assert_eq!(canonicalize_type("%\"Qubit\"").unwrap(), "Qubit");
        assert_eq!(canonicalize_type("i1").unwrap(), "i1");
        assert_eq!(canonicalize_type("i32").unwrap(), "i32");
        assert_eq!(canonicalize_type("double").unwrap(), "double");
        assert_eq!(canonicalize_type("ptr").unwrap(), "ptr");
        assert_eq!(canonicalize_type("void").unwrap(), "void");
        assert!(canonicalize_type("").is_err());
    }

    #[test]
    fn extracts_variadic_flag_from_trailing_ellipsis() {
        let src = "declare void @foo(%Qubit*, ...)\n";
        let t = extract_signatures(src).unwrap();
        let sig = t.get("foo").unwrap();
        assert!(sig.is_variadic);
        assert_eq!(sig.param_types, vec!["Qubit"]);
    }

    #[test]
    fn rejects_nonterminal_ellipsis() {
        let src = "declare void @foo(..., %Qubit*)\n";
        let err = extract_signatures(src).unwrap_err();
        assert!(
            err.to_string()
                .contains("unexpected '...' in non-terminal position"),
            "{err}"
        );
    }

    #[test]
    fn parses_define_signature_with_linkage_attrs() {
        let src = "define dso_local void @pkg.foo() local_unnamed_addr {\n}\n";
        let t = extract_signatures(src).unwrap();
        let sig = t.get("pkg.foo").unwrap();
        assert_eq!(sig.return_type, "void");
        assert!(sig.param_types.is_empty());
        assert!(!sig.is_variadic);
    }

    #[test]
    fn parses_quoted_function_name() {
        let src = "define void @\"my.qualified::fn\"(%Qubit*) {\n}\n";
        let t = extract_signatures(src).unwrap();
        assert!(t.contains("my.qualified::fn"));
    }

    #[test]
    fn parses_multiple_declares() {
        let src = "\
declare void @__quantum__qis__h__body(%Qubit*)
declare i1 @__quantum__qis__read_result__body(%Result*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
";
        let t = extract_signatures(src).unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(
            t.get("__quantum__qis__h__body").unwrap().param_types,
            vec!["Qubit"]
        );
        assert_eq!(
            t.get("__quantum__qis__read_result__body")
                .unwrap()
                .return_type,
            "i1"
        );
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn default_and_accessor_coverage() {
        let t = SignatureTable::default();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert!(!t.contains("foo"));
        assert!(t.get("foo").is_none());
        assert!(t.iter().next().is_none());
    }

    #[test]
    fn function_signature_is_void_and_nonvoid() {
        let s = FunctionSignature {
            name: "f".into(),
            return_type: "void".into(),
            param_types: vec![],
            is_variadic: false,
        };
        assert!(s.is_void());
        let s2 = FunctionSignature {
            name: "g".into(),
            return_type: "i1".into(),
            param_types: vec![],
            is_variadic: false,
        };
        assert!(!s2.is_void());
    }

    #[test]
    fn canonicalize_type_empty_errors() {
        let err = canonicalize_type("").unwrap_err();
        assert!(err.to_string().contains("empty type token"));
    }

    #[test]
    fn canonicalize_type_rejects_bogus_bare() {
        let err = canonicalize_type("@@@bogus").unwrap_err();
        assert!(err.to_string().contains("could not parse parameter type"));
    }

    #[test]
    fn canonicalize_type_rejects_bogus_percent() {
        let err = canonicalize_type("%1badident").unwrap_err();
        assert!(err.to_string().contains("could not parse parameter type"));
    }

    #[test]
    fn canonicalize_type_accepts_fp128_and_bfloat() {
        assert_eq!(canonicalize_type("fp128").unwrap(), "fp128");
        assert_eq!(canonicalize_type("bfloat").unwrap(), "bfloat");
    }

    #[test]
    fn empty_parameter_in_list_errors() {
        let err = extract_signatures("declare void @foo(,)").unwrap_err();
        assert!(err.to_string().contains("empty parameter in signature"));
    }

    #[test]
    fn param_with_no_type_token_errors() {
        // `declare void @foo(writeonly)` — only an attribute, no type token.
        let err = extract_signatures("declare void @foo(writeonly)").unwrap_err();
        assert!(err.to_string().contains("could not parse parameter type"));
    }

    #[test]
    fn signature_line_missing_at_errors() {
        let err = extract_signatures("declare void foo(i32)").unwrap_err();
        assert!(err.to_string().contains("could not parse signature line"));
    }

    #[test]
    fn signature_line_with_unterminated_quoted_name_errors() {
        let err = extract_signatures("declare void @\"unterminated(i32)").unwrap_err();
        assert!(err.to_string().contains("unterminated quoted name"));
    }

    #[test]
    fn signature_line_missing_paren_errors() {
        let err = extract_signatures("declare void @foo").unwrap_err();
        assert!(err.to_string().contains("signature missing"));
    }

    #[test]
    fn signature_line_missing_name_token_errors() {
        let err = extract_signatures("declare void @").unwrap_err();
        assert!(err.to_string().contains("could not parse signature line"));
    }

    #[test]
    fn signature_hash_and_equality_roundtrip() {
        use std::collections::HashSet;
        let a = FunctionSignature {
            name: "f".into(),
            return_type: "void".into(),
            param_types: vec!["i32".into()],
            is_variadic: false,
        };
        let b = a.clone();
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }
}

#[cfg(test)]
mod more_tests_v2 {
    use super::*;

    #[test]
    fn malformed_declare_line_raises_parse_error() {
        // Missing the return-type position.
        let err = extract_signatures("declare").unwrap_err();
        assert!(err.to_string().contains("could not parse signature line"));
    }

    #[test]
    fn param_list_invalid_struct_identifier_errors() {
        let err = extract_signatures("declare void @f(%)").unwrap_err();
        assert!(err.to_string().contains("could not parse parameter type"));
    }

    #[test]
    fn parses_struct_return_type() {
        let src =
            "define { i1*, i64 } @__nvqpp__mlirgen__kernel() #0 {\n  ret { i1*, i64 } undef\n}\n";
        let t = extract_signatures(src).unwrap();
        let sig = t.get("__nvqpp__mlirgen__kernel").unwrap();
        assert_eq!(sig.return_type, "struct");
        assert!(!sig.is_void());
    }

    #[test]
    fn unclosed_struct_return_type_errors() {
        let src = "define { i1*, i64 @broken() {\n}\n";
        let err = extract_signatures(src).unwrap_err();
        assert!(err.to_string().contains("unclosed struct return type"));
    }

    #[test]
    fn extract_signatures_requires_word_boundary_after_declare_or_define() {
        // Lookalike identifiers that share a prefix with `declare` /
        // `define` but continue without whitespace must not be routed
        // into the signature parser. Without the word-boundary gate,
        // a line like `declareSomething = …` would false-match and
        // either error out as malformed or partially parse in
        // surprising ways. Only the genuine `declare` line below
        // should yield a signature.
        let src = "declareSomething = something\n\
                   defineFoo bar baz\n\
                   declare void @real()\n";
        let t = extract_signatures(src).unwrap();
        assert_eq!(t.len(), 1);
        assert!(t.contains("real"));
    }

    #[test]
    fn extract_signatures_handles_nested_struct_return_type() {
        // The first `}` closes the inner struct; without brace-depth
        // tracking the parser would have stopped there and produced a
        // malformed return-type slice. With depth tracking the outer
        // `}` is found and the return type canonicalizes to the
        // struct-by-value sentinel.
        let src = "declare { { i1 }, i64 } @nested()\n";
        let t = extract_signatures(src).unwrap();
        let sig = t.get("nested").unwrap();
        assert_eq!(sig.return_type, "struct");
    }

    #[test]
    fn extract_signatures_handles_nested_struct_parameter_type() {
        // Symmetric to the return-type case: a nested-struct
        // by-value parameter must not bail at the first inner `}`.
        let src = "declare void @nested_param({ { i1 }, i64 } %0)\n";
        let t = extract_signatures(src).unwrap();
        let sig = t.get("nested_param").unwrap();
        assert_eq!(sig.param_types, vec!["struct"]);
    }

    #[test]
    fn canonicalize_type_collapses_pointer_primitive_to_primitive() {
        // The signature table classifies parameter shape for
        // qubit/result/classical-scalar dispatch; it does not preserve
        // pointer-vs-value distinctions on primitive types, since
        // those forms never appear in declare/define signatures the
        // table indexes. Pin the current behavior so a future
        // refactor doesn't silently change it.
        assert_eq!(canonicalize_type("i32").unwrap(), "i32");
        assert_eq!(canonicalize_type("i32*").unwrap(), "i32");
        assert_eq!(canonicalize_type("i64*").unwrap(), "i64");
        assert_eq!(canonicalize_type("double").unwrap(), "double");
        assert_eq!(canonicalize_type("double*").unwrap(), "double");
        assert_eq!(canonicalize_type("float*").unwrap(), "float");
    }
}
