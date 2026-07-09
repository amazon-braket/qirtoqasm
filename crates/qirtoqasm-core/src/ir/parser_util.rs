// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Low-level text utilities shared by the QIR parser and signature
//! extractor.

// The parser and signature-extraction consumers haven't been added
// yet, so these helpers have no in-crate callers in non-test builds.
// The inline tests below exercise each function; the file-level
// allow is removed once the consumers land.
#![allow(dead_code)]

/// Find the byte index of the `)` that closes the leading `(` in `s`.
/// Caller must ensure `s` starts with `(`. Returns `None` on unbalanced
/// input.
pub(crate) fn matching_close_paren(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
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

/// Split `src` at top-level commas, respecting nested `()`, `[]`, `<>`,
/// and `{}` brackets. Each returned chunk is whitespace-trimmed.
///
/// Returns an error if a closing bracket appears without a matching
/// opener — otherwise the depth counter would go negative and later
/// top-level commas could be missed.
pub(crate) fn split_top_level_commas(src: &str) -> crate::error::Result<Vec<&str>> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let bytes = src.as_bytes();
    let mut last = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' | b'[' | b'<' | b'(' => depth += 1,
            b'}' | b']' | b'>' | b')' => {
                depth -= 1;
                if depth < 0 {
                    return Err(crate::error::QirToQasmError::syntax(format!(
                        "unbalanced closing bracket in {src:?}"
                    )));
                }
            }
            b',' if depth == 0 => {
                out.push(src[last..i].trim());
                last = i + 1;
            }
            _ => {}
        }
    }
    out.push(src[last..].trim());
    Ok(out)
}

/// Parse a leading `@<name>` global reference, with both bare and
/// `"quoted"` forms. Returns `(name, rest_of_input)` on success.
pub(crate) fn parse_global_name(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let s = s.strip_prefix('@')?;
    if let Some(stripped) = s.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some((stripped[..end].to_string(), &stripped[end + 1..]))
    } else {
        let end = s
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$')))
            .unwrap_or(s.len());
        if end == 0 {
            None
        } else {
            Some((s[..end].to_string(), &s[end..]))
        }
    }
}

/// LLVM identifier rule: `[A-Za-z._$][A-Za-z0-9._$]*`.
pub(crate) fn is_valid_llvm_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || matches!(first, '_' | '.' | '$')) {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_close_paren_handles_simple_and_nested() {
        assert_eq!(matching_close_paren("()"), Some(1));
        assert_eq!(matching_close_paren("(a)"), Some(2));
        assert_eq!(matching_close_paren("(())"), Some(3));
        assert_eq!(matching_close_paren("(a, (b, c))"), Some(10));
    }

    #[test]
    fn matching_close_paren_unbalanced_returns_none() {
        assert!(matching_close_paren("(").is_none());
        assert!(matching_close_paren("(a, b").is_none());
        assert!(matching_close_paren("(a, (b, c)").is_none());
    }

    #[test]
    fn split_top_level_commas_respects_every_bracket_kind() {
        assert_eq!(
            split_top_level_commas("a, b, c").unwrap(),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            split_top_level_commas("a, (b, c), d").unwrap(),
            vec!["a", "(b, c)", "d"]
        );
        assert_eq!(
            split_top_level_commas("a, [b, c], d").unwrap(),
            vec!["a", "[b, c]", "d"]
        );
        assert_eq!(
            split_top_level_commas("a, {b, c}, d").unwrap(),
            vec!["a", "{b, c}", "d"]
        );
        assert_eq!(
            split_top_level_commas("a, <b, c>, d").unwrap(),
            vec!["a", "<b, c>", "d"]
        );
    }

    #[test]
    fn split_top_level_commas_trims_chunks_and_handles_empty() {
        assert_eq!(split_top_level_commas("").unwrap(), vec![""]);
        assert_eq!(
            split_top_level_commas("  a  ,  b  ").unwrap(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn split_top_level_commas_rejects_unbalanced_closing_bracket() {
        assert!(split_top_level_commas("a}, b").is_err());
        assert!(split_top_level_commas(")").is_err());
        assert!(split_top_level_commas("a, b, c]").is_err());
    }

    #[test]
    fn parse_global_name_handles_bare_form() {
        let (name, rest) = parse_global_name("@foo bar").unwrap();
        assert_eq!(name, "foo");
        assert_eq!(rest, " bar");
    }

    #[test]
    fn parse_global_name_handles_quoted_form() {
        let (name, rest) = parse_global_name("@\"qualified.name\" rest").unwrap();
        assert_eq!(name, "qualified.name");
        assert_eq!(rest, " rest");
    }

    #[test]
    fn parse_global_name_skips_leading_whitespace() {
        let (name, _rest) = parse_global_name("   @foo").unwrap();
        assert_eq!(name, "foo");
    }

    #[test]
    fn parse_global_name_rejects_invalid_inputs() {
        assert!(parse_global_name("foo").is_none());
        assert!(parse_global_name("@\"unterminated").is_none());
        assert!(parse_global_name("@@@").is_none());
    }

    #[test]
    fn is_valid_llvm_ident_accepts_letters_digits_punctuation() {
        assert!(is_valid_llvm_ident("foo"));
        assert!(is_valid_llvm_ident("Foo_bar"));
        assert!(is_valid_llvm_ident(".foo$bar"));
        assert!(is_valid_llvm_ident("_x"));
        assert!(is_valid_llvm_ident("a.b.c"));
    }

    #[test]
    fn is_valid_llvm_ident_rejects_invalid_inputs() {
        assert!(!is_valid_llvm_ident(""));
        assert!(!is_valid_llvm_ident("1foo"));
        assert!(!is_valid_llvm_ident("foo bar"));
        assert!(!is_valid_llvm_ident("foo!"));
    }
}
