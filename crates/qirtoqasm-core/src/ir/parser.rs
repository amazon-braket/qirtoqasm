// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Line-oriented recursive-descent parser for the QIR subset we
//! accept. Intentionally not a full LLVM textual-IR parser — we
//! recover the handful of shapes QIR emitters actually produce and
//! surface clear errors for everything else.

use crate::error::{QirToQasmError, Result};

use super::model::*;

/// Parse a QIR module from `.ll` source text.
pub fn parse_module(text: &str) -> Result<Module> {
    let mut p = Parser::new(text);
    // Parsing distinguishes declared-only `Function`s from `define` bodies
    // that carry attribute-group references which must be resolved in a
    // second pass. We keep them in a `ParsedItem` enum until then.
    enum ParsedItem {
        Decl(Function),
        Define(PendingFunction),
    }
    let mut items: Vec<ParsedItem> = Vec::new();

    // We parse attribute-group bodies into a map once, so each
    // function's `attributes #N` reference can be resolved eagerly.
    // The attribute body is stored as its raw source slice; we only
    // ever check it for the substring `entry_point`.
    let mut attr_groups: std::collections::HashMap<u32, String> = std::collections::HashMap::new();

    while !p.eof() {
        p.skip_blank_and_comment_lines();
        if p.eof() {
            break;
        }
        let raw_line = p.peek_line();
        let trimmed = raw_line.trim_start();

        if trimmed.starts_with("define") {
            let func = parse_define(&mut p)?;
            items.push(ParsedItem::Define(func));
        } else if trimmed.starts_with("declare") {
            let func = parse_declare(&mut p)?;
            items.push(ParsedItem::Decl(func));
        } else if let Some(rest) = trimmed.strip_prefix("attributes") {
            // `attributes #N = { ... }`
            if let Some((id, body)) = parse_attribute_group(rest) {
                attr_groups.insert(id, body);
            }
            p.consume_rest_of_line();
        } else {
            // Any other top-level line (comments, target triple, source_filename,
            // type aliases, global constants, module flags) is ignored. Advance.
            p.consume_rest_of_line();
        }
    }

    // Second pass: resolve attribute group references on function bodies.
    let mut functions: Vec<Function> = Vec::with_capacity(items.len());
    for item in items {
        match item {
            ParsedItem::Decl(f) => functions.push(f),
            ParsedItem::Define(mut pf) => {
                let mut qir_profile: Option<String> = None;
                for gid in std::mem::take(&mut pf.pending_group_refs) {
                    if let Some(body) = attr_groups.get(&gid) {
                        if !pf.is_entry_point && body.contains("entry_point") {
                            pf.is_entry_point = true;
                        }
                        if qir_profile.is_none() {
                            qir_profile = extract_qir_profile(body);
                        }
                    }
                }
                functions.push(Function {
                    name: pf.name,
                    is_declaration: pf.is_declaration,
                    is_entry_point: pf.is_entry_point,
                    qir_profile,
                    blocks: pf.blocks,
                });
            }
        }
    }

    Ok(Module {
        source_text: text.to_string(),
        functions,
    })
}

/// Mutable parser state.
struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

/// Helper used during parsing only: mirrors `Function` but carries
/// the attribute-group refs until the second pass resolves them.
struct PendingFunction {
    name: String,
    is_declaration: bool,
    is_entry_point: bool,
    blocks: Vec<Block>,
    pending_group_refs: Vec<u32>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek_line(&self) -> &'a str {
        let end = self.bytes[self.pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|o| self.pos + o)
            .unwrap_or(self.bytes.len());
        &self.src[self.pos..end]
    }

    fn consume_rest_of_line(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
        if self.pos < self.bytes.len() {
            self.pos += 1; // skip the newline
        }
    }

    fn skip_blank_and_comment_lines(&mut self) {
        loop {
            if self.eof() {
                return;
            }
            let line = self.peek_line();
            let stripped = line.trim_start();
            if stripped.is_empty() || stripped.starts_with(';') || stripped.starts_with('!') {
                self.consume_rest_of_line();
                continue;
            }
            return;
        }
    }
}

fn parse_attribute_group(rest: &str) -> Option<(u32, String)> {
    // rest: "   #0 = { \"entry_point\" ... }"
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('#')?;
    let (num, rest) = split_u32(rest);
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('{')?;
    let end = rest.rfind('}')?;
    Some((num?, rest[..end].to_string()))
}

/// Extract the `"qir_profiles"="<value>"` value from an attribute
/// group body, if present.
fn extract_qir_profile(body: &str) -> Option<String> {
    const KEY: &str = "\"qir_profiles\"=\"";
    let start = body.find(KEY)? + KEY.len();
    let end = body[start..].find('"')?;
    Some(body[start..start + end].to_string())
}

fn split_u32(s: &str) -> (Option<u32>, &str) {
    let end = s
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(s.len());
    if end == 0 {
        (None, s)
    } else {
        let n = s[..end].parse::<u32>().ok();
        (n, &s[end..])
    }
}

// ---------------------------------------------------------------------------
// declare / define
// ---------------------------------------------------------------------------

fn parse_declare(p: &mut Parser<'_>) -> Result<Function> {
    let line = p.peek_line().to_string();
    p.consume_rest_of_line();
    let (_rest, name) = extract_fn_name_from_header(&line, "declare").ok_or_else(|| {
        QirToQasmError::syntax(format!("could not parse declare: {}", line.trim()))
    })?;
    // declare lines may reference attribute groups like `#1`; those are
    // only used for `irreversible` markers we don't need. Ignore.
    Ok(Function {
        name,
        is_declaration: true,
        is_entry_point: false,
        qir_profile: None,
        blocks: vec![],
    })
}

fn parse_define(p: &mut Parser<'_>) -> Result<PendingFunction> {
    // The `define` header may span multiple lines if someone wrapped
    // it, but in every QIR module we ship it fits on one line. Accept
    // one-line headers.
    let header = p.peek_line().to_string();
    p.consume_rest_of_line();

    let (rest, name) = extract_fn_name_from_header(&header, "define").ok_or_else(|| {
        QirToQasmError::syntax(format!("could not parse define header: {}", header.trim()))
    })?;

    // Anything between the closing `)` of the parameter list and the
    // opening `{` of the body may include attrs like `#0` or inline
    // quoted key-value pairs. We scan `rest` (already trimmed to the
    // region after the signature) for both forms.
    let (inline_attrs, group_refs, body_starts_on_same_line) = parse_post_sig_attrs(rest);
    let has_entry_inline = inline_attrs.contains(&"entry_point".to_string());

    // If the `{` wasn't on the same line, we may need to consume the next
    // line(s) until we see it. Since none of our fixtures do this, we
    // bail for clarity if they ever diverge.
    if !body_starts_on_same_line {
        // consume until '{' appears at start-of-line
        loop {
            if p.eof() {
                return Err(QirToQasmError::syntax(
                    "define without body block".to_string(),
                ));
            }
            let line = p.peek_line();
            if line.trim_start().starts_with('{') {
                p.consume_rest_of_line();
                break;
            }
            p.consume_rest_of_line();
        }
    }

    // Parse the body. The body is a sequence of basic blocks separated
    // by labels; the first block starts immediately (no label) unless
    // a label appears first.
    let mut blocks: Vec<Block> = vec![Block {
        name: String::new(),
        instructions: Vec::new(),
    }];

    loop {
        if p.eof() {
            return Err(QirToQasmError::syntax("define body not terminated"));
        }
        let line = p.peek_line();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            p.consume_rest_of_line();
            continue;
        }
        if trimmed == "}" {
            p.consume_rest_of_line();
            break;
        }
        // Is this a bare label line?  `name:` or `name:   ; preds = ...`
        if let Some(label) = try_parse_label_line(trimmed) {
            if blocks
                .last()
                .map(|b| !b.instructions.is_empty())
                .unwrap_or(true)
                || !blocks.last().map(|b| b.name.is_empty()).unwrap_or(true)
            {
                blocks.push(Block {
                    name: label,
                    instructions: Vec::new(),
                });
            } else {
                // empty trailing block; rename it
                blocks.last_mut().unwrap().name = label;
            }
            p.consume_rest_of_line();
            continue;
        }
        // Logical instruction lines may wrap across newlines when the
        // operand list is long or a `call` has a function-type prefix
        // on its own line. Accumulate continuation lines until:
        //   1. paren depth is balanced, AND
        //   2. if the instruction is a `call`, we've seen an `@` token
        //      (the callee name) — otherwise the function-type prefix
        //      hasn't been followed by a callee on the same logical
        //      line.
        // `depth` and `seen_at` are tracked incrementally from each
        // newly-appended slice. `is_call_start` is determined by the
        // first line via `effective_opcode`, which sees through the
        // optional `%x =` assignment prefix and `tail`/`musttail`/
        // `notail` call qualifiers.
        let mut accumulated = String::new();
        accumulated.push_str(trimmed);
        p.consume_rest_of_line();
        let live_first = strip_trailing_comment(trimmed);
        let mut depth = paren_depth(live_first);
        let mut seen_at = live_first.contains('@');
        let is_call_start = effective_opcode(&accumulated) == "call";
        loop {
            let done = depth == 0 && (!is_call_start || seen_at);
            if done {
                break;
            }
            if p.eof() {
                return Err(QirToQasmError::syntax(format!(
                    "unterminated instruction: {accumulated:?}"
                )));
            }
            let extra = p.peek_line().trim();
            if extra.is_empty() {
                p.consume_rest_of_line();
                continue;
            }
            accumulated.push(' ');
            accumulated.push_str(extra);
            let live_extra = strip_trailing_comment(extra);
            depth += paren_depth(live_extra);
            if !seen_at {
                seen_at = live_extra.contains('@');
            }
            p.consume_rest_of_line();
        }
        let instr = parse_instruction_line(&accumulated)?;
        blocks.last_mut().unwrap().instructions.push(instr);
        continue;
    }

    Ok(PendingFunction {
        name,
        is_declaration: false,
        is_entry_point: has_entry_inline,
        blocks,
        pending_group_refs: group_refs,
    })
}

fn try_parse_label_line(line: &str) -> Option<String> {
    // Accept `name:` optionally followed by a comment.
    let (label_part, _) = match line.find(';') {
        Some(i) => line.split_at(i),
        None => (line, ""),
    };
    let label_part = label_part.trim();
    let label_part = label_part.strip_suffix(':')?;
    if label_part.is_empty() {
        return None;
    }
    // LLVM 15+ serializes unnamed basic-block labels as a quoted numeric:
    // `"0":`, `"42":`. Strip outer quotes before the identifier check.
    let label_part = if let Some(inner) = label_part
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
    {
        inner
    } else {
        label_part
    };
    if label_part.is_empty() {
        return None;
    }
    // Must not contain whitespace (a label is a single token).
    if label_part.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    // Must be a valid LLVM identifier: [A-Za-z._$][A-Za-z0-9._$]*
    // or a purely numeric label (e.g. `2:` for `%2` block).
    let first = label_part.chars().next()?;
    let ok_first = first.is_ascii_alphanumeric() || matches!(first, '_' | '.' | '$');
    if !ok_first {
        return None;
    }
    if !label_part
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$'))
    {
        return None;
    }
    Some(label_part.to_string())
}

/// Extract the function name from a `declare` or `define` header.
/// Returns (rest-of-line-after-closing-paren, function-name).
fn extract_fn_name_from_header<'b>(line: &'b str, keyword: &str) -> Option<(&'b str, String)> {
    let idx = line.find(keyword)?;
    let rest = &line[idx + keyword.len()..];
    // rest = " void @__quantum__qis__h__body(...)" possibly preceded by linkage etc
    // Find the '@' that starts the function name.
    let at = rest.find('@')?;
    let after_at = &rest[at + 1..];
    // Name can be bare or `"quoted"`.
    let (name, after_name) = if let Some(stripped) = after_at.strip_prefix('"') {
        let end = stripped.find('"')?;
        (stripped[..end].to_string(), &stripped[end + 1..])
    } else {
        let end = after_at
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$')))
            .unwrap_or(after_at.len());
        (after_at[..end].to_string(), &after_at[end..])
    };
    // Skip past the parameter list.
    let paren_open = after_name.find('(')?;
    let mut depth = 0usize;
    let mut close_idx: Option<usize> = None;
    for (i, c) in after_name[paren_open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close_idx = Some(paren_open + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let close_idx = close_idx?;
    let rest_after_sig = &after_name[close_idx..];
    Some((rest_after_sig, name))
}

/// Scan the slice between the function signature and the opening `{`.
/// Collect inline quoted attribute keys (`"entry_point"`, `"qir_profiles"`)
/// and attribute-group refs (`#0`).
fn parse_post_sig_attrs(rest: &str) -> (Vec<String>, Vec<u32>, bool) {
    // Does the `{` live on this line?
    let open = rest.find('{');
    let body_starts = open.is_some();
    let scan_end = open.unwrap_or(rest.len());
    let scan = &rest[..scan_end];

    let mut inline: Vec<String> = Vec::new();
    let mut groups: Vec<u32> = Vec::new();

    let bytes = scan.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'"' {
                j += 1;
            }
            if j < bytes.len() {
                inline.push(scan[start..j].to_string());
                i = j + 1;
                continue;
            } else {
                break;
            }
        }
        if b == b'#' {
            let num_start = i + 1;
            let mut j = num_start;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > num_start {
                if let Ok(n) = scan[num_start..j].parse::<u32>() {
                    groups.push(n);
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }

    (inline, groups, body_starts)
}

// ---------------------------------------------------------------------------
// Instruction-line parser
// ---------------------------------------------------------------------------

fn parse_instruction_line(line: &str) -> Result<Instruction> {
    // Strip trailing `;` comments.
    let line = strip_trailing_comment(line);
    let line = line.trim();

    // Detect assignment form: `%<id> = <opcode> ...`
    let (result, body) = if let Some(eq_idx) = line.find('=') {
        // Only treat '=' as assignment if what's before it is `%<id>`.
        let left = line[..eq_idx].trim();
        if let Some(id) = left.strip_prefix('%') {
            (Some(id.to_string()), line[eq_idx + 1..].trim_start())
        } else {
            (None, line)
        }
    } else {
        (None, line)
    };

    // Strip any instruction-prefix qualifiers (`tail`, `musttail`,
    // `notail`) before reading the opcode.
    let rest = strip_call_prefix_qualifiers(body);
    let opcode = first_token(rest);
    let args = after_token(rest).trim_start();

    match opcode {
        "call" => parse_call(result, args),
        "br" => parse_br(args),
        "ret" => Ok(Instruction::Ret),
        "icmp" => parse_icmp(result, args),
        "xor" => parse_binary_i1(result, args, BinaryI1Op::Xor, "xor"),
        "and" => parse_binary_i1(result, args, BinaryI1Op::And, "and"),
        "or" => parse_binary_i1(result, args, BinaryI1Op::Or, "or"),
        "select" => parse_select(result, args),
        "add" => parse_int_arith(result, args, IntArithOp::Add, "add"),
        "sub" => parse_int_arith(result, args, IntArithOp::Sub, "sub"),
        "mul" => parse_int_arith(result, args, IntArithOp::Mul, "mul"),
        "phi" => parse_phi(result, args),
        "alloca" => Ok(parse_alloca(result).unwrap_or(Instruction::Ignored {
            opcode: "alloca".into(),
        })),
        "bitcast" => Ok(parse_bitcast(result, args).unwrap_or(Instruction::Ignored {
            opcode: "bitcast".into(),
        })),
        "getelementptr" => Ok(
            parse_getelementptr(result, args).unwrap_or(Instruction::Ignored {
                opcode: "getelementptr".into(),
            }),
        ),
        "load" => Ok(parse_load(result, args).unwrap_or(Instruction::Ignored {
            opcode: "load".into(),
        })),
        "store" => Ok(parse_store(args).unwrap_or(Instruction::Ignored {
            opcode: "store".into(),
        })),
        "zext" => Ok(parse_zext(result, args).unwrap_or(Instruction::Ignored {
            opcode: "zext".into(),
        })),
        "sext" | "trunc" | "ptrtoint" | "insertvalue" | "extractvalue" => {
            Ok(Instruction::Ignored {
                opcode: opcode.to_string(),
            })
        }
        _ if opcode.is_empty() => Err(QirToQasmError::syntax(format!(
            "empty instruction in block body: {line:?}"
        ))),
        other => Ok(Instruction::Unsupported {
            opcode: other.to_string(),
        }),
    }
}

fn strip_trailing_comment(line: &str) -> &str {
    // A comment in LLVM IR is `;...` to end of line. Outside of strings,
    // the first `;` ends the logical content. The IR uses double-quoted
    // strings only in very limited places (global constants, attribute
    // keys) which don't appear in instruction lines, so a naive scan is
    // safe here.
    match line.find(';') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn first_token(s: &str) -> &str {
    let s = s.trim_start();
    let end = s
        .bytes()
        .position(|b| b.is_ascii_whitespace())
        .unwrap_or(s.len());
    &s[..end]
}

fn after_token(s: &str) -> &str {
    let s = s.trim_start();
    let end = s
        .bytes()
        .position(|b| b.is_ascii_whitespace())
        .unwrap_or(s.len());
    s[end..].trim_start()
}

/// Strip any leading instruction-prefix qualifiers (`tail`, `musttail`,
/// `notail`) from `s` and return the trimmed remainder. Fast-math flags
/// (`fast`, `nnan`, etc.) appear *after* the opcode in LLVM IR, not
/// before it, and are not stripped here.
fn strip_call_prefix_qualifiers(s: &str) -> &str {
    let mut rest = s.trim_start();
    loop {
        match first_token(rest) {
            "tail" | "musttail" | "notail" => rest = after_token(rest),
            _ => return rest,
        }
    }
}

/// Return the effective opcode of `line`: the first token after
/// stripping the optional `%<name> =` assignment prefix and any
/// leading call-prefix qualifiers. Empty string if the line has no
/// opcode token.
fn effective_opcode(line: &str) -> &str {
    let mut rest = line.trim_start();
    if is_assignment_prefix(rest) {
        if let Some(eq) = rest.find('=') {
            rest = rest[eq + 1..].trim_start();
        }
    }
    first_token(strip_call_prefix_qualifiers(rest))
}

// ---------------------------------------------------------------------------
// Individual instruction parsers
// ---------------------------------------------------------------------------

fn parse_call(result: Option<String>, args: &str) -> Result<Instruction> {
    // Shape:  <retty> [(param-type-list)] @<callee>(<operands>) [fn-attrs]
    //
    // When a callee is variadic, LLVM's textual form prefixes the call
    // with a function-type list, e.g.
    //   call void (i64, ..., i8*, ...) @generalizedInvoke(...)
    // We detect and skip that prefix parenthesised group so the `@name`
    // that follows is recognized as the callee.
    let mut rest = args;
    let return_type = first_token(rest);
    rest = after_token(rest);
    // Optional `(type, type, ..., ...)` function-type prefix.
    let after_optional_fn_type = {
        let s = rest.trim_start();
        if s.starts_with('(') {
            let close_rel = super::parser_util::matching_close_paren(s).ok_or_else(|| {
                QirToQasmError::syntax(format!(
                    "call function-type prefix has unbalanced parens: {args:?}"
                ))
            })?;
            &s[close_rel + 1..]
        } else {
            s
        }
    };
    rest = after_optional_fn_type.trim_start();
    let (callee, after_name) = super::parser_util::parse_global_name(rest).ok_or_else(|| {
        QirToQasmError::syntax(format!("could not parse callee in call: {args:?}"))
    })?;
    // Find the matching parens for the operand list.
    let open = after_name.find('(').ok_or_else(|| {
        QirToQasmError::syntax(format!("call missing ( after callee name: {args:?}"))
    })?;
    let close_rel = super::parser_util::matching_close_paren(&after_name[open..])
        .ok_or_else(|| QirToQasmError::syntax(format!("call has unbalanced parens: {args:?}")))?;
    let operands_src = &after_name[open + 1..open + close_rel];
    let operands = parse_operand_list(operands_src)?;

    Ok(Instruction::Call {
        result,
        callee,
        args: operands,
        return_type: return_type.to_string(),
    })
}

fn parse_operand_list(src: &str) -> Result<Vec<Operand>> {
    if src.trim().is_empty() {
        return Ok(vec![]);
    }
    let parts = super::parser_util::split_top_level_commas(src)?;
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        out.push(parse_operand(p.trim())?);
    }
    Ok(out)
}

fn parse_operand(chunk: &str) -> Result<Operand> {
    let chunk = chunk.trim();
    if chunk == "..." {
        return Err(QirToQasmError::syntax("unexpected ... in operand"));
    }

    // Recognize `getelementptr inbounds (...)` verbose form used in QIR for
    // string constants referenced from `*_record_output` calls.
    if chunk.starts_with("getelementptr") {
        return Ok(Operand::GetElementPtr);
    }

    // Tokenise: `<type> [<flags>] <value>`
    // We strip parameter/operand attributes conservatively.
    let mut tokens = chunk.split_ascii_whitespace().collect::<Vec<_>>();
    // Strip operand attrs that may appear before the value.
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
    tokens.retain(|t| !attrs.contains(t));

    if tokens.is_empty() {
        return Err(QirToQasmError::syntax("empty operand"));
    }

    let ty = tokens.remove(0);
    // The remaining tokens make up the value; re-join preserving spaces.
    let value = tokens.join(" ");
    let value = value.trim();

    // Struct-pointer operand: either typed (%Qubit*, %"Qubit"*) or opaque ("ptr"),
    // plus the `i8*` form producers emit for C-string arguments to
    // `__quantum__rt__*_record_output` calls.
    let struct_name = parse_struct_ptr_type(ty);

    if struct_name.is_some() || ty == "ptr" || ty == "i8*" {
        // Value forms: "null", "inttoptr (iN N to <ptr>)", "bitcast (...)".
        if value == "null" {
            if ty == "i8*" {
                return Ok(Operand::I8Null);
            }
            return Ok(Operand::PtrConst {
                struct_name,
                index: 0,
            });
        }
        if let Some(idx) = parse_inttoptr(value) {
            return Ok(Operand::PtrConst {
                struct_name,
                index: idx,
            });
        }
        if let Some(id) = value.strip_prefix('%') {
            return Ok(Operand::Ssa(id.to_string()));
        }
        if let Some(rest) = value.strip_prefix('@') {
            return Ok(Operand::GlobalRef(rest.to_string()));
        }
        if value.starts_with("getelementptr") {
            // Appears only as no-op record_output payloads. Represent opaquely.
            return Ok(Operand::GetElementPtr);
        }
        if value.starts_with("bitcast") {
            // `bitcast (... @<name> to i8*)` - the inner-function pointer
            // form used by the variadic generalized-invoke intrinsic. Fall
            // back to the opaque `GetElementPtr` form if the bitcast
            // doesn't name a global (as in record-output payloads).
            if let Some(name) = extract_bitcast_global_name(value) {
                return Ok(Operand::BitcastGlobal(name));
            }
            return Ok(Operand::GetElementPtr);
        }
        return Err(QirToQasmError::unsupported(format!(
            "could not resolve pointer operand from constant text {:?}",
            chunk
        )));
    }
    // Integer operand.
    if ty.starts_with('i') && ty[1..].bytes().all(|b| b.is_ascii_digit()) {
        // i1 constants: true/false/0/1; SSA: %id; general int constant.
        if ty == "i1" {
            match value {
                "true" | "1" => return Ok(Operand::ConstBool(true)),
                "false" | "0" => return Ok(Operand::ConstBool(false)),
                v if v.starts_with('%') => return Ok(Operand::Ssa(v[1..].to_string())),
                _ => {}
            }
        }
        if let Some(v) = value.strip_prefix('%') {
            return Ok(Operand::Ssa(v.to_string()));
        }
        if ty == "i8" && value == "null" {
            return Ok(Operand::I8Null);
        }
        if let Some(n) = parse_int_literal(value) {
            return Ok(Operand::ConstInt(n));
        }
        if let Some(rest) = value.strip_prefix('@') {
            return Ok(Operand::GlobalRef(rest.to_string()));
        }
        return Err(QirToQasmError::unsupported(format!(
            "could not parse integer operand {:?} (ty={ty})",
            value
        )));
    }

    // Float operand.
    if ty == "double" || ty == "float" || ty == "half" {
        if let Some(v) = value.strip_prefix('%') {
            return Ok(Operand::Ssa(v.to_string()));
        }
        if let Some(f) = parse_float_literal(value) {
            return Ok(Operand::ConstFloat(f));
        }
        return Err(QirToQasmError::unsupported(format!(
            "could not parse float operand {:?} (ty={ty})",
            value
        )));
    }

    Err(QirToQasmError::unsupported(format!(
        "unsupported operand type {ty:?} in operand {chunk:?}"
    )))
}

fn parse_struct_ptr_type(ty: &str) -> Option<String> {
    let ty = ty.strip_suffix('*')?;
    if let Some(q) = ty.strip_prefix("%\"").and_then(|r| r.strip_suffix('"')) {
        Some(q.to_string())
    } else {
        ty.strip_prefix('%').map(|name| name.to_string())
    }
}

fn parse_inttoptr(value: &str) -> Option<i64> {
    // inttoptr (iN N to <ptr>)
    let rest = value
        .strip_prefix("inttoptr")?
        .trim_start()
        .strip_prefix('(')?;
    // After '(' we expect "iN N to <something>)"
    let rest = rest.trim_start();
    // skip the `iN` type token
    let _it = rest.split_whitespace().next()?;
    let after_i = rest.split_whitespace().nth(1)?;
    let n: i64 = after_i.parse().ok()?;
    Some(n)
}

/// Extract the global name from a `bitcast (<ty> @<name> to <ty>)` operand.
/// Used to recover the inner QIS function pointer passed as the 5th
/// argument to `generalizedInvokeWithRotationsControlsTargets`.
/// Returns `None` if the bitcast does not name a global.
fn extract_bitcast_global_name(value: &str) -> Option<String> {
    let body = value
        .strip_prefix("bitcast")?
        .trim_start()
        .strip_prefix('(')?;
    // Scan char-by-char for an `@<name>` token; we can't tokenise robustly
    // against the parenthesised type spellings that surround it.
    let mut cursor = body;
    while !cursor.is_empty() {
        if cursor.starts_with('@') {
            let (name, _) = super::parser_util::parse_global_name(cursor)?;
            return Some(name);
        }
        let mut chars = cursor.char_indices();
        let _ = chars.next();
        cursor = chars.as_str();
    }
    None
}

fn parse_int_literal(s: &str) -> Option<i128> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<i128>().ok()
}

fn parse_float_literal(s: &str) -> Option<f64> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("0x") {
        // LLVM hex-float: exact IEEE-754 double bit pattern.
        if rest.len() == 16 && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
            let bits = u64::from_str_radix(rest, 16).ok()?;
            return Some(f64::from_bits(bits));
        }
        return None;
    }
    if let Some(rest) = t.strip_prefix("-0x") {
        if rest.len() == 16 && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
            let bits = u64::from_str_radix(rest, 16).ok()?;
            return Some(-f64::from_bits(bits));
        }
        return None;
    }
    t.parse::<f64>().ok()
}

// ---------------------------------------------------------------------------
// br / ret / icmp / xor / phi
// ---------------------------------------------------------------------------

fn parse_br(args: &str) -> Result<Instruction> {
    // Two shapes:
    //   br label %target
    //   br i1 %cond, label %true, label %false
    let s = args.trim();
    if let Some(rest) = s.strip_prefix("label") {
        let t = rest.trim_start();
        let t = t.strip_prefix('%').ok_or_else(|| {
            QirToQasmError::syntax(format!("br label missing %<target>: {args:?}"))
        })?;
        return Ok(Instruction::Br {
            target: parse_label_ident(t),
        });
    }
    if let Some(rest) = s.strip_prefix("i1 ") {
        let mut pieces = super::parser_util::split_top_level_commas(rest)?;
        if pieces.len() != 3 {
            return Err(QirToQasmError::syntax(format!(
                "br i1 expects cond, true_label, false_label: {args:?}"
            )));
        }
        let cond_src = pieces.remove(0).trim();
        let true_src = pieces.remove(0).trim();
        let false_src = pieces.remove(0).trim();
        let cond = if let Some(id) = cond_src.strip_prefix('%') {
            BrCondOperand::Ssa(id.to_string())
        } else if cond_src == "true" || cond_src == "1" {
            BrCondOperand::Const(true)
        } else if cond_src == "false" || cond_src == "0" {
            BrCondOperand::Const(false)
        } else {
            return Err(QirToQasmError::unsupported(format!(
                "unsupported constant branch condition {:?}",
                cond_src
            )));
        };
        let true_target = strip_label_ref(true_src).ok_or_else(|| {
            QirToQasmError::syntax(format!("br i1 expected 'label %name', got {true_src:?}"))
        })?;
        let false_target = strip_label_ref(false_src).ok_or_else(|| {
            QirToQasmError::syntax(format!("br i1 expected 'label %name', got {false_src:?}"))
        })?;
        return Ok(Instruction::BrCond {
            cond,
            true_target,
            false_target,
        });
    }
    Err(QirToQasmError::syntax(format!(
        "unsupported br form: {args:?}"
    )))
}

fn strip_label_ref(s: &str) -> Option<String> {
    let s = s
        .trim()
        .strip_prefix("label")?
        .trim_start()
        .strip_prefix('%')?;
    Some(parse_label_ident(s))
}

/// Parse a single label identifier after a leading `%`. Accepts both bare
/// identifiers (`entry`, `2`, `true_target`) and LLVM 15+ quoted numeric
/// form (`"0"`, `"42"`) — the latter is used whenever the serializer
/// decides an unnamed basic-block label needs quoting.
fn parse_label_ident(s: &str) -> String {
    if let Some(inner) = s.strip_prefix('"') {
        // `"<ident>"<rest>` — take chars up to the closing quote.
        if let Some(end) = inner.find('"') {
            return inner[..end].to_string();
        }
    }
    let end = s
        .bytes()
        .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$')))
        .unwrap_or(s.len());
    s[..end].to_string()
}

/// Parse `%r = icmp <pred> <ty> <lhs>, <rhs>`. The predicate token is
/// preserved verbatim; validation happens at lowering time.
fn parse_icmp(result: Option<String>, args: &str) -> Result<Instruction> {
    let result =
        result.ok_or_else(|| QirToQasmError::syntax("icmp without SSA result assignment"))?;
    let s = args.trim();
    let pred = first_token(s);
    let rest = after_token(s);
    let ty = first_token(rest);
    let rest = after_token(rest);
    let pieces = super::parser_util::split_top_level_commas(rest)?;
    if pieces.len() != 2 {
        return Err(QirToQasmError::syntax(format!(
            "icmp expects two operands: {args:?}"
        )));
    }
    let lhs = parse_operand(&format!("{ty} {}", pieces[0].trim()))?;
    let rhs = parse_operand(&format!("{ty} {}", pieces[1].trim()))?;
    Ok(Instruction::Icmp(Icmp {
        result,
        predicate: PredicateI1(pred.to_string()),
        lhs,
        rhs,
    }))
}

/// Parse `%r = <opcode> i1 <lhs>, <rhs>` for `xor` / `and` / `or`.
/// The `opcode` string is passed only for error messages.
fn parse_binary_i1(
    result: Option<String>,
    args: &str,
    op: BinaryI1Op,
    opcode: &str,
) -> Result<Instruction> {
    let result = result
        .ok_or_else(|| QirToQasmError::syntax(format!("{opcode} without SSA result assignment")))?;
    let s = args.trim();
    let ty = first_token(s);
    if ty != "i1" {
        return Err(QirToQasmError::unsupported(format!(
            "{opcode} on {ty} not yet supported (only i1 is handled)"
        )));
    }
    let rest = after_token(s);
    let pieces = super::parser_util::split_top_level_commas(rest)?;
    if pieces.len() != 2 {
        return Err(QirToQasmError::syntax(format!(
            "{opcode} expects two operands: {args:?}"
        )));
    }
    let lhs = parse_operand(&format!("{ty} {}", pieces[0].trim()))?;
    let rhs = parse_operand(&format!("{ty} {}", pieces[1].trim()))?;
    Ok(Instruction::BinaryI1 {
        result,
        op,
        lhs,
        rhs,
    })
}

/// Parse `%r = <opcode> [nuw] [nsw] <ty> <lhs>, <rhs>` for the
/// integer arithmetic opcodes `add` / `sub` / `mul`. The overflow
/// flags are accepted and discarded.
fn parse_int_arith(
    result: Option<String>,
    args: &str,
    op: IntArithOp,
    opcode: &str,
) -> Result<Instruction> {
    let result = result
        .ok_or_else(|| QirToQasmError::syntax(format!("{opcode} without SSA result assignment")))?;
    let mut s = args.trim();
    loop {
        let tok = first_token(s);
        if matches!(tok, "nuw" | "nsw") {
            s = after_token(s);
        } else {
            break;
        }
    }
    let ty = first_token(s);
    let rest = after_token(s);
    let pieces = super::parser_util::split_top_level_commas(rest)?;
    if pieces.len() != 2 {
        return Err(QirToQasmError::syntax(format!(
            "{opcode} expects two operands: {args:?}"
        )));
    }
    let lhs = parse_operand(&format!("{ty} {}", pieces[0].trim()))?;
    let rhs = parse_operand(&format!("{ty} {}", pieces[1].trim()))?;
    Ok(Instruction::IntArith {
        result,
        op,
        lhs,
        rhs,
    })
}

/// Parse `%r = select i1 <cond>, <ty> <a>, <ty> <b>`. The two branch
/// operands must share a type; the shared spelling is preserved on
/// `Instruction::Select` so the lowering can dispatch on it.
fn parse_select(result: Option<String>, args: &str) -> Result<Instruction> {
    let result =
        result.ok_or_else(|| QirToQasmError::syntax("select without SSA result assignment"))?;
    let s = args.trim();
    let cond_ty = first_token(s);
    if cond_ty != "i1" {
        return Err(QirToQasmError::syntax(format!(
            "select condition must be typed `i1`, got {cond_ty:?}"
        )));
    }
    let rest = after_token(s);
    let pieces = super::parser_util::split_top_level_commas(rest)?;
    if pieces.len() != 3 {
        return Err(QirToQasmError::syntax(format!(
            "select expects three comma-separated operands (cond, then, else): {args:?}"
        )));
    }
    let cond = parse_operand(&format!("{cond_ty} {}", pieces[0].trim()))?;
    let then_src = pieces[1].trim();
    let value_type = first_token(then_src).to_string();
    let then_value_text = after_token(then_src);
    let true_value = parse_operand(&format!("{value_type} {then_value_text}"))?;
    let else_src = pieces[2].trim();
    let else_ty = first_token(else_src);
    if else_ty != value_type {
        return Err(QirToQasmError::syntax(format!(
            "select branch types must match, got then={value_type:?} else={else_ty:?}"
        )));
    }
    let else_value_text = after_token(else_src);
    let false_value = parse_operand(&format!("{value_type} {else_value_text}"))?;
    Ok(Instruction::Select {
        result,
        value_type,
        cond,
        true_value,
        false_value,
    })
}

/// Parse `%r = phi <ty> [ <val>, %<block> ], [ <val>, %<block> ], ...`.
/// At least one incoming is required.
fn parse_phi(result: Option<String>, args: &str) -> Result<Instruction> {
    let result =
        result.ok_or_else(|| QirToQasmError::syntax("phi without SSA result assignment"))?;
    let s = args.trim();
    let ty = first_token(s);
    let rest = after_token(s);
    let mut out = Vec::new();
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] != b'[' {
            return Err(QirToQasmError::syntax(format!(
                "phi malformed incoming starting at {:?}",
                &rest[i..]
            )));
        }
        let close = rest[i..]
            .find(']')
            .map(|o| i + o)
            .ok_or_else(|| QirToQasmError::syntax("phi incoming missing ]"))?;
        let inner = &rest[i + 1..close];
        let pieces = super::parser_util::split_top_level_commas(inner)?;
        if pieces.len() != 2 {
            return Err(QirToQasmError::syntax(format!(
                "phi incoming must have value and predecessor: [{inner}]"
            )));
        }
        let val = parse_operand(&format!("{ty} {}", pieces[0].trim()))?;
        let pred = pieces[1].trim().strip_prefix('%').ok_or_else(|| {
            QirToQasmError::syntax(format!(
                "phi incoming predecessor must be %<label>, got {:?}",
                pieces[1]
            ))
        })?;
        let pred_name = parse_label_ident(pred);
        out.push(PhiIncoming {
            value: val,
            pred: pred_name,
        });
        i = close + 1;
    }
    if out.is_empty() {
        return Err(QirToQasmError::syntax("phi has no incomings"));
    }
    Ok(Instruction::Phi {
        result,
        value_type: ty.to_string(),
        incomings: out,
    })
}

/// Find the byte offset of `keyword` in `haystack` that's not nested
/// inside `{}`, `[]`, `<>`, or `()`.
fn find_top_level_keyword(haystack: &str, keyword: &str) -> Option<usize> {
    let mut depth = 0i32;
    let bytes = haystack.as_bytes();
    let kw_bytes = keyword.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' | b'[' | b'<' | b'(' => depth += 1,
            b'}' | b']' | b'>' | b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && bytes[i..].starts_with(kw_bytes) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Return the last `%<ident>` token in `s`.
fn last_percent_ident(s: &str) -> Option<String> {
    let last_pct = s.rfind('%')?;
    let rest = &s[last_pct + 1..];
    let end = rest
        .bytes()
        .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$')))
        .unwrap_or(rest.len());
    if end == 0 {
        None
    } else {
        Some(rest[..end].to_string())
    }
}

/// Parse `%r = zext i1 %src to i32` (or any narrow-to-wide integer
/// extension). Falls through to Ignored for non-SSA sources.
fn parse_zext(result: Option<String>, args: &str) -> Option<Instruction> {
    let result = result?;
    // Shape: `<fromty> <src> to <toty>` — we only need the src SSA.
    let rest = args.trim();
    let to_idx = find_top_level_keyword(rest, " to ")?;
    let lhs = rest[..to_idx].trim();
    let src = last_percent_ident(lhs)?;
    Some(Instruction::Zext { result, src })
}

// ---------------------------------------------------------------------------
// alloca / bitcast / getelementptr / load / store — minimal recognizers
// for the array-of-scratch idiom (alloca an array, store constants,
// bitcast / gep to element pointers, load back). Shapes that don't match
// the recognizer fall through to `Instruction::Ignored` via `unwrap_or`
// at the call site.
// ---------------------------------------------------------------------------

/// Parse `%r = alloca <ty>[, …]` into an `Alloca { result }` record.
/// The type and any align modifiers are not needed by the translator.
fn parse_alloca(result: Option<String>) -> Option<Instruction> {
    Some(Instruction::Alloca { result: result? })
}

/// Parse `%r = bitcast <ty>* %src to <ty>*` — the SSA-to-SSA form.
/// Global-reference and constant-expression bitcasts fall through to
/// `Ignored`; the translator doesn't need them.
fn parse_bitcast(result: Option<String>, args: &str) -> Option<Instruction> {
    let result = result?;
    let rest = args.trim();
    let to_idx = find_top_level_keyword(rest, " to ")?;
    let lhs = rest[..to_idx].trim();
    let src = last_percent_ident(lhs)?;
    Some(Instruction::BitcastAlias { result, src })
}

/// Parse `%r = getelementptr [inbounds] <ty>, <ty>* %src, i32 0, i32 <offset>`.
/// Only the `i32 0 / i64 0` first-index plus a constant second-index
/// shape is recognized; anything else falls through to `Ignored`.
fn parse_getelementptr(result: Option<String>, args: &str) -> Option<Instruction> {
    let result = result?;
    let rest = strip_trailing_comment(args).trim();
    let rest = rest
        .strip_prefix("inbounds")
        .map(str::trim_start)
        .unwrap_or(rest);
    let pieces = super::parser_util::split_top_level_commas(rest).ok()?;
    if pieces.len() != 4 {
        return None;
    }
    // pieces[0]: `<ty>` — array type, ignore.
    // pieces[1]: `<ty>* %src` — extract src.
    // pieces[2]: `i32 0` or `i64 0`.
    // pieces[3]: `i32 <offset>` or `i64 <offset>`.
    let src = last_percent_ident(pieces[1].trim())?;
    let leading = pieces[2].trim();
    if leading != "i32 0" && leading != "i64 0" {
        return None;
    }
    let offset_chunk = pieces[3].trim();
    let offset_str = offset_chunk
        .strip_prefix("i32 ")
        .or_else(|| offset_chunk.strip_prefix("i64 "))?;
    let offset: u64 = offset_str.parse().ok()?;
    Some(Instruction::GetElementPtrOffset {
        result,
        src,
        offset,
    })
}

/// Parse `%r = load <ty>, <ty>* %ptr[, align N]`.
fn parse_load(result: Option<String>, args: &str) -> Option<Instruction> {
    let result = result?;
    let rest = strip_trailing_comment(args).trim();
    let pieces = super::parser_util::split_top_level_commas(rest).ok()?;
    if pieces.len() < 2 {
        return None;
    }
    let ptr = last_percent_ident(pieces[1].trim())?;
    Some(Instruction::Load { result, ptr })
}

/// Parse `store <ty> <value>, <ty>* %ptr[, align N]`.
fn parse_store(args: &str) -> Option<Instruction> {
    let rest = strip_trailing_comment(args).trim();
    let pieces = super::parser_util::split_top_level_commas(rest).ok()?;
    if pieces.len() < 2 {
        return None;
    }
    let val_chunk = pieces[0].trim();
    let ptr_chunk = pieces[1].trim();
    // val_chunk: `<ty> <value>`. Split on first whitespace for the
    // type, remainder is the value expression.
    let (ty, val_text) = val_chunk.split_once(char::is_whitespace)?;
    let value = parse_operand(&format!("{} {}", ty, val_text.trim())).ok()?;
    let ptr = last_percent_ident(ptr_chunk)?;
    Some(Instruction::Store { value, ptr })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_operand_list_with_typed_pointer_and_opaque_pointer() {
        let ops =
            parse_operand_list("%Qubit* null, %Result* inttoptr (i64 1 to %Result*)").unwrap();
        assert_eq!(ops.len(), 2);
        let Operand::PtrConst { struct_name, index } = &ops[0] else {
            panic!("wrong operand: {:?}", ops[0])
        };
        assert_eq!(struct_name.as_deref(), Some("Qubit"));
        assert_eq!(*index, 0);
        let Operand::PtrConst { struct_name, index } = &ops[1] else {
            panic!("wrong operand: {:?}", ops[1])
        };
        assert_eq!(struct_name.as_deref(), Some("Result"));
        assert_eq!(*index, 1);

        let ops = parse_operand_list("ptr null, ptr inttoptr (i64 2 to ptr)").unwrap();
        let Operand::PtrConst { struct_name, .. } = &ops[0] else {
            panic!()
        };
        assert!(struct_name.is_none());
        let Operand::PtrConst { index, .. } = &ops[1] else {
            panic!()
        };
        assert_eq!(*index, 2);
    }

    #[test]
    fn parses_hex_float_to_exact_ieee754() {
        // 0x3FF921FB54442D18 == pi/2
        let v = parse_float_literal("0x3FF921FB54442D18").unwrap();
        assert_eq!(v, std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn parses_br_unconditional_and_conditional() {
        let Instruction::Br { target } = parse_instruction_line("br label %exit").unwrap() else {
            panic!()
        };
        assert_eq!(target, "exit");
        let Instruction::BrCond {
            cond,
            true_target,
            false_target,
        } = parse_instruction_line("br i1 %cond, label %t, label %f").unwrap()
        else {
            panic!("expected BrCond")
        };
        let super::super::model::BrCondOperand::Ssa(id) = &cond else {
            panic!("expected Ssa cond, got {cond:?}")
        };
        assert_eq!(id, "cond");
        assert_eq!(true_target, "t");
        assert_eq!(false_target, "f");

        let Instruction::BrCond { cond, .. } =
            parse_instruction_line("br i1 0, label %t, label %f").unwrap()
        else {
            panic!()
        };
        let super::super::model::BrCondOperand::Const(b) = cond else {
            panic!("expected Const cond, got {cond:?}")
        };
        assert!(!b);
    }

    #[test]
    fn parses_icmp_and_xor_and_phi() {
        let Instruction::Icmp(icmp) = parse_instruction_line("%c = icmp eq i1 %a, %b").unwrap()
        else {
            panic!("expected Icmp")
        };
        assert_eq!(icmp.result, "c");
        assert_eq!(icmp.predicate.0, "eq");

        let Instruction::BinaryI1 {
            result, op, rhs, ..
        } = parse_instruction_line("%c = xor i1 %a, true").unwrap()
        else {
            panic!("expected BinaryI1")
        };
        assert_eq!(result, "c");
        assert_eq!(op, BinaryI1Op::Xor);
        assert!(matches!(rhs, Operand::ConstBool(true)));

        let Instruction::Phi {
            result,
            value_type,
            incomings,
        } = parse_instruction_line("%c = phi i1 [false, %b0], [%r, %b1]").unwrap()
        else {
            panic!("expected Phi")
        };
        assert_eq!(result, "c");
        assert_eq!(value_type, "i1");
        assert_eq!(incomings.len(), 2);
        assert_eq!(incomings[0].pred, "b0");
        assert_eq!(incomings[1].pred, "b1");
    }

    #[test]
    fn parses_alloca_load_store_bitcast_gep() {
        let i = parse_instruction_line("%2 = alloca [2 x double], align 8").unwrap();
        assert!(matches!(i, Instruction::Alloca { .. }), "{i:?}");
        let i = parse_instruction_line("%3 = bitcast [2 x double]* %2 to double*").unwrap();
        assert!(matches!(i, Instruction::BitcastAlias { .. }), "{i:?}");
        let i = parse_instruction_line("store double 0.1, double* %3, align 8").unwrap();
        assert!(matches!(i, Instruction::Store { .. }), "{i:?}");
        let i = parse_instruction_line("%6 = load double, double* %3, align 8").unwrap();
        assert!(matches!(i, Instruction::Load { .. }), "{i:?}");
        let i = parse_instruction_line(
            "%4 = getelementptr [2 x double], [2 x double]* %2, i32 0, i32 1",
        )
        .unwrap();
        assert!(
            matches!(i, Instruction::GetElementPtrOffset { .. }),
            "{i:?}"
        );
    }

    #[test]
    fn parses_zext_i1_to_integer() {
        // Standard shape: `%r = zext i1 %src to i32` — used by producer
        // opt pipelines to promote measurement bits to integers before
        // summing them.
        let Instruction::Zext { result, src } =
            parse_instruction_line("%1 = zext i1 %0 to i32").unwrap()
        else {
            panic!("expected Zext")
        };
        assert_eq!(result, "1");
        assert_eq!(src, "0");
    }

    #[test]
    fn ignores_insertvalue_and_extractvalue() {
        for op in ["insertvalue", "extractvalue"] {
            let line = format!("%x = {op} {{ i1*, i64 }} undef, i64 2, 1");
            let Instruction::Ignored { opcode } = parse_instruction_line(&line).unwrap() else {
                panic!("{op} did not become Ignored")
            };
            assert_eq!(opcode, op);
        }
    }

    #[test]
    fn unknown_opcode_becomes_unsupported() {
        let Instruction::Unsupported { opcode } =
            parse_instruction_line("%x = fadd double 0.0, 0.0").unwrap()
        else {
            panic!()
        };
        assert_eq!(opcode, "fadd");
    }
}

fn paren_depth(s: &str) -> i32 {
    let mut depth = 0i32;
    for b in s.bytes() {
        match b {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn is_assignment_prefix(line: &str) -> bool {
    let trimmed = line.trim_start();
    if let Some(eq) = trimmed.find('=') {
        let left = trimmed[..eq].trim();
        left.starts_with('%')
    } else {
        false
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn translates_cudaq_llvm16_opaque_ptr_fixture() {
        // End-to-end: QIR with opaque pointers plus quoted numeric block
        // labels (the LLVM 15+ text form) must translate to valid Braket
        // OpenQASM 3.
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("test")
            .join("integ_tests")
            .join("fixtures_qir")
            .join("cudaq_llvm16_simple_x.ll");
        let src = std::fs::read_to_string(path).unwrap();
        let oq3 = crate::translator::Exporter::default()
            .dumps(&parse_module(&src).unwrap())
            .unwrap_or_else(|e| panic!("translate failed: {e}"));
        assert!(oq3.contains("OPENQASM 3.0;"));
        assert!(oq3.contains("qubit[1] q;"));
        assert!(oq3.contains("x q[0];"));
        assert!(oq3.contains("c[0] = measure q[0];"));
    }

    #[test]
    fn translates_cudaq_llvm16_mcm_feedforward_fixture() {
        // MCM plus a single-branch `if` with opaque pointers and quoted
        // numeric block labels (the LLVM 15+ text form) must produce the
        // same OpenQASM 3 as the typed-pointer / unquoted-label LLVM 14
        // form emits for the equivalent kernel — see the corresponding
        // non-`llvm16` fixture in the same directory.
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("test")
            .join("integ_tests")
            .join("fixtures_qir")
            .join("cudaq_llvm16_mcm_feedforward.ll");
        let src = std::fs::read_to_string(path).unwrap();
        let oq3 = crate::translator::Exporter::default()
            .dumps(&parse_module(&src).unwrap())
            .unwrap_or_else(|e| panic!("translate failed: {e}"));
        assert!(oq3.contains("h q[0];"), "\n{oq3}");
        assert!(oq3.contains("c[0] = measure q[0];"), "\n{oq3}");
        assert!(oq3.contains("if (c[0]"), "\n{oq3}");
        assert!(oq3.contains("x q[1];"), "\n{oq3}");
        assert!(oq3.contains("c[1] = measure q[1];"), "\n{oq3}");
    }

    #[test]
    fn split_u32_rejects_non_digit_prefix() {
        let (n, rest) = split_u32("abc");
        assert!(n.is_none());
        assert_eq!(rest, "abc");
    }

    #[test]
    fn parse_attribute_group_handles_braces() {
        let (id, body) = parse_attribute_group(" #7 = { \"entry_point\" }").unwrap();
        assert_eq!(id, 7);
        assert!(body.contains("entry_point"));
        assert!(parse_attribute_group("not an attribute group").is_none());
        assert!(parse_attribute_group(" #bad = { ").is_none());
    }

    #[test]
    fn extract_fn_name_handles_quoted_and_bare_names() {
        let line = "declare void @foo(i32)";
        let (rest, name) = extract_fn_name_from_header(line, "declare").unwrap();
        assert_eq!(name, "foo");
        assert!(rest.is_empty() || rest.starts_with(' ') || rest.is_empty());

        let line2 = "define void @\"my::fn\"() {";
        let (_rest2, name2) = extract_fn_name_from_header(line2, "define").unwrap();
        assert_eq!(name2, "my::fn");

        assert!(extract_fn_name_from_header("no keyword here", "declare").is_none());
        assert!(extract_fn_name_from_header("declare no at sign", "declare").is_none());
    }

    #[test]
    fn parse_post_sig_attrs_collects_inline_and_group_refs() {
        let (inline, groups, has_body) =
            parse_post_sig_attrs(" #0 #1 \"entry_point\" \"qir_profiles\"=\"base\" {");
        assert!(has_body);
        assert!(inline.iter().any(|s| s == "entry_point"));
        assert!(groups.contains(&0));
        assert!(groups.contains(&1));
        // Without the brace, `has_body` must be false.
        let (_, _, body) = parse_post_sig_attrs(" #0 no brace here");
        assert!(!body);
    }

    #[test]
    fn try_parse_label_line_tolerates_edge_cases() {
        assert_eq!(try_parse_label_line("entry:").as_deref(), Some("entry"));
        assert_eq!(
            try_parse_label_line("entry: ; preds = %0").as_deref(),
            Some("entry")
        );
        assert!(try_parse_label_line(":").is_none());
        assert!(try_parse_label_line("not a label").is_none());
        assert!(try_parse_label_line("with space:").is_none());
        assert!(try_parse_label_line("bad@chars:").is_none());
    }

    #[test]
    fn try_parse_label_line_accepts_quoted_numeric_llvm15_form() {
        // LLVM 15+ emits unnamed basic-block labels as `"N":` in textual IR.
        assert_eq!(try_parse_label_line("\"0\":").as_deref(), Some("0"));
        assert_eq!(
            try_parse_label_line("\"42\": ; preds = %entry").as_deref(),
            Some("42")
        );
        // Still reject an empty quoted label.
        assert!(try_parse_label_line("\"\":").is_none());
    }

    #[test]
    fn parse_global_name_bare_and_quoted() {
        use super::super::parser_util::parse_global_name;
        let (n, _) = parse_global_name("@foo(...)").unwrap();
        assert_eq!(n, "foo");
        let (n, _) = parse_global_name("@\"quoted\"(...)").unwrap();
        assert_eq!(n, "quoted");
        assert!(parse_global_name("no at prefix").is_none());
        assert!(parse_global_name("@\"unterminated").is_none());
        assert!(parse_global_name("@").is_none());
    }

    #[test]
    fn parse_instruction_line_handles_assignment_form() {
        let out = parse_instruction_line("%x = call i1 @read()").unwrap();
        let Instruction::Call { result, .. } = out else {
            panic!()
        };
        assert_eq!(result.as_deref(), Some("x"));
    }

    #[test]
    fn parse_instruction_line_skips_leading_qualifiers() {
        let out = parse_instruction_line("tail call void @__quantum__qis__h__body(%Qubit* null)")
            .unwrap();
        let Instruction::Call { callee, .. } = out else {
            panic!()
        };
        assert_eq!(callee, "__quantum__qis__h__body");
    }

    #[test]
    fn parse_br_errors_on_unknown_shape() {
        let err = parse_instruction_line("br something weird").unwrap_err();
        assert!(err.to_string().contains("unsupported br form"));
    }

    #[test]
    fn parse_br_label_missing_percent_errors() {
        let err = parse_instruction_line("br label exit").unwrap_err();
        assert!(err.to_string().contains("br label missing"));
    }

    #[test]
    fn parse_br_cond_rejects_weird_constant() {
        let err = parse_instruction_line("br i1 maybe, label %t, label %f").unwrap_err();
        assert!(err
            .to_string()
            .contains("unsupported constant branch condition"));
    }

    #[test]
    fn parse_br_cond_rejects_non_label_target() {
        let err = parse_instruction_line("br i1 %c, label %t, something_else").unwrap_err();
        assert!(err.to_string().contains("expected 'label %name'"));
    }

    #[test]
    fn parse_br_cond_wrong_arity_errors() {
        let err = parse_instruction_line("br i1 %c, label %t").unwrap_err();
        assert!(err.to_string().contains("cond, true_label, false_label"));
    }

    #[test]
    fn parse_icmp_without_assignment_errors() {
        let err = parse_instruction_line("icmp eq i1 %a, %b").unwrap_err();
        assert!(err.to_string().contains("icmp without SSA result"));
    }

    #[test]
    fn parse_icmp_wrong_operand_count_errors() {
        let err = parse_instruction_line("%c = icmp eq i1 %a").unwrap_err();
        assert!(err.to_string().contains("icmp expects two operands"));
    }

    #[test]
    fn parse_xor_without_assignment_errors() {
        let err = parse_instruction_line("xor i1 %a, true").unwrap_err();
        assert!(err.to_string().contains("xor without SSA result"));
    }

    #[test]
    fn parse_xor_wrong_operand_count_errors() {
        let err = parse_instruction_line("%c = xor i1 %a").unwrap_err();
        assert!(err.to_string().contains("xor expects two operands"));
    }

    #[test]
    fn parse_and_without_assignment_errors() {
        let err = parse_instruction_line("and i1 %a, %b").unwrap_err();
        assert!(err.to_string().contains("and without SSA result"));
    }

    #[test]
    fn parse_and_wrong_operand_count_errors() {
        let err = parse_instruction_line("%c = and i1 %a").unwrap_err();
        assert!(err.to_string().contains("and expects two operands"));
    }

    #[test]
    fn parse_and_happy_path_binds_instruction() {
        let Instruction::BinaryI1 { result, op, .. } =
            parse_instruction_line("%c = and i1 %a, %b").unwrap()
        else {
            panic!("expected BinaryI1")
        };
        assert_eq!(result, "c");
        assert_eq!(op, BinaryI1Op::And);
    }

    #[test]
    fn parse_or_without_assignment_errors() {
        let err = parse_instruction_line("or i1 %a, %b").unwrap_err();
        assert!(err.to_string().contains("or without SSA result"));
    }

    #[test]
    fn parse_or_wrong_operand_count_errors() {
        let err = parse_instruction_line("%c = or i1 %a").unwrap_err();
        assert!(err.to_string().contains("or expects two operands"));
    }

    #[test]
    fn parse_or_happy_path_binds_instruction() {
        let Instruction::BinaryI1 { result, op, .. } =
            parse_instruction_line("%c = or i1 %a, %b").unwrap()
        else {
            panic!("expected BinaryI1")
        };
        assert_eq!(result, "c");
        assert_eq!(op, BinaryI1Op::Or);
    }

    #[test]
    fn parse_binary_i1_rejects_non_i1_widths() {
        // `and`/`or`/`xor` on integer widths wider than i1 are bitwise
        // ops in LLVM; the current lowering only handles i1 (Boolean),
        // so wider widths must produce a clean unsupported error rather
        // than silently emitting `a && b` for what should be `a & b`.
        for shape in [
            "%c = and i64 %a, %b",
            "%c = or i32 %a, %b",
            "%c = xor i8 %a, %b",
        ] {
            let err = parse_instruction_line(shape).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("not yet supported"),
                "expected unsupported-error for {shape:?}, got {msg:?}"
            );
            assert!(
                msg.contains("only i1 is handled"),
                "expected narrowing hint for {shape:?}, got {msg:?}"
            );
        }
    }

    #[test]
    fn parse_select_without_assignment_errors() {
        let err = parse_instruction_line("select i1 %c, i1 %a, i1 %b").unwrap_err();
        assert!(err.to_string().contains("select without SSA result"));
    }

    #[test]
    fn parse_select_non_i1_cond_type_errors() {
        let err = parse_instruction_line("%r = select i32 %c, i32 %a, i32 %b").unwrap_err();
        assert!(err
            .to_string()
            .contains("select condition must be typed `i1`"));
    }

    #[test]
    fn parse_select_wrong_operand_count_errors() {
        let err = parse_instruction_line("%r = select i1 %c, i1 %a").unwrap_err();
        assert!(err.to_string().contains("three comma-separated operands"));
    }

    #[test]
    fn parse_select_mismatched_branch_types_error() {
        let err = parse_instruction_line("%r = select i1 %c, i1 %a, i32 %b").unwrap_err();
        assert!(err.to_string().contains("branch types must match"));
    }

    #[test]
    fn parse_select_happy_path_binds_instruction() {
        let Instruction::Select {
            result, value_type, ..
        } = parse_instruction_line("%r = select i1 %c, i1 %a, i1 %b").unwrap()
        else {
            panic!("expected Select")
        };
        assert_eq!(result, "r");
        assert_eq!(value_type, "i1");
    }

    #[test]
    fn parse_int_arith_without_assignment_errors() {
        let err = parse_instruction_line("add i32 %a, %b").unwrap_err();
        assert!(err.to_string().contains("add without SSA result"));
    }

    #[test]
    fn parse_int_arith_wrong_operand_count_errors() {
        let err = parse_instruction_line("%c = add i32 %a").unwrap_err();
        assert!(err.to_string().contains("add expects two operands"));
    }

    #[test]
    fn parse_int_arith_skips_nuw_nsw_flags() {
        let Instruction::IntArith { result, op, .. } =
            parse_instruction_line("%c = add nuw nsw i32 %a, 1").unwrap()
        else {
            panic!("expected IntArith")
        };
        assert_eq!(result, "c");
        assert_eq!(op, IntArithOp::Add);
    }

    #[test]
    fn parse_int_arith_does_not_swallow_exact_flag() {
        // `exact` is an LLVM flag for `udiv` / `sdiv` / `ashr` / `lshr`,
        // not for `add` / `sub` / `mul`. Encountering it on integer
        // arithmetic indicates malformed IR; the parser must not skip
        // over it silently.
        let err = parse_instruction_line("%c = add exact i32 %a, %b").unwrap_err();
        // The `exact` token is treated as the operand-type slot; the
        // resulting `exact %a` operand fails to parse.
        let msg = err.to_string();
        assert!(
            !msg.contains("without SSA result"),
            "unexpected error shape: {msg:?}"
        );
    }

    #[test]
    fn parse_int_arith_sub_and_mul() {
        let Instruction::IntArith { op, .. } =
            parse_instruction_line("%c = sub i32 %a, %b").unwrap()
        else {
            panic!()
        };
        assert_eq!(op, IntArithOp::Sub);
        let Instruction::IntArith { op, .. } =
            parse_instruction_line("%c = mul i32 %a, %b").unwrap()
        else {
            panic!()
        };
        assert_eq!(op, IntArithOp::Mul);
    }

    #[test]
    fn parse_phi_without_assignment_errors() {
        let err = parse_instruction_line("phi i1 [false, %b0]").unwrap_err();
        assert!(err.to_string().contains("phi without SSA result"));
    }

    #[test]
    fn parse_phi_malformed_bracket_errors() {
        let err = parse_instruction_line("%c = phi i1 foo").unwrap_err();
        assert!(err.to_string().contains("phi"));
    }

    #[test]
    fn parse_phi_bad_predecessor_errors() {
        let err = parse_instruction_line("%c = phi i1 [false, bad]").unwrap_err();
        assert!(err.to_string().contains("predecessor must be"));
    }

    #[test]
    fn parse_phi_wrong_incoming_shape_errors() {
        // An incoming with three pieces inside the brackets.
        let err = parse_instruction_line("%c = phi i1 [false, %a, %b]").unwrap_err();
        assert!(err.to_string().contains("incoming must have"));
    }

    #[test]
    fn parse_operand_chunks() {
        // Empty operand list returns an empty vec.
        let ops = parse_operand_list("").unwrap();
        assert!(ops.is_empty());
        // A lone `...` is rejected.
        let err = parse_operand("...").unwrap_err();
        assert!(err.to_string().contains("unexpected ..."));
    }

    #[test]
    fn parse_operand_handles_getelementptr() {
        let op = parse_operand("i8* getelementptr inbounds ([2 x i8], [2 x i8]* @0, i64 0, i64 0)")
            .unwrap();
        assert!(matches!(op, Operand::GetElementPtr));
    }

    #[test]
    fn parse_operand_accepts_boolean_in_i1_form() {
        let Operand::ConstBool(b) = parse_operand("i1 true").unwrap() else {
            panic!()
        };
        assert!(b);
        let Operand::ConstBool(b) = parse_operand("i1 false").unwrap() else {
            panic!()
        };
        assert!(!b);
        let Operand::Ssa(id) = parse_operand("i1 %x").unwrap() else {
            panic!()
        };
        assert_eq!(id, "x");
    }

    #[test]
    fn parse_operand_integer_global_ref() {
        let Operand::GlobalRef(name) = parse_operand("i32 @g").unwrap() else {
            panic!()
        };
        assert_eq!(name, "g");
    }

    #[test]
    fn parse_operand_i8_null_is_distinct() {
        assert!(matches!(
            parse_operand("i8* null").unwrap(),
            Operand::I8Null
        ));
    }

    #[test]
    fn parse_operand_rejects_unknown_type() {
        let err = parse_operand("weird_t %x").unwrap_err();
        assert!(err.to_string().contains("unsupported operand type"));
    }

    #[test]
    fn parse_operand_float_ssa() {
        let Operand::Ssa(id) = parse_operand("double %x").unwrap() else {
            panic!()
        };
        assert_eq!(id, "x");
    }

    #[test]
    fn parse_operand_pointer_ssa_and_global() {
        let Operand::Ssa(id) = parse_operand("%Qubit* %x").unwrap() else {
            panic!()
        };
        assert_eq!(id, "x");
        let Operand::GlobalRef(g) = parse_operand("ptr @g").unwrap() else {
            panic!()
        };
        assert_eq!(g, "g");
    }

    #[test]
    fn parse_operand_rejects_unparseable_int() {
        let err = parse_operand("i32 not_a_number").unwrap_err();
        assert!(err.to_string().contains("could not parse integer operand"));
    }

    #[test]
    fn parse_operand_rejects_unparseable_float() {
        let err = parse_operand("double garbage").unwrap_err();
        assert!(err.to_string().contains("could not parse float operand"));
    }

    #[test]
    fn parse_float_literal_rejects_malformed_hex() {
        assert!(parse_float_literal("0xDEADBEEF").is_none()); // wrong length
        assert!(parse_float_literal("0xGARBAGE12345678").is_none());
    }

    #[test]
    fn parse_float_literal_accepts_negative_hex() {
        assert_eq!(parse_float_literal("-0x3FF0000000000000"), Some(-1.0));
        assert!(parse_float_literal("-0xDEADBEEF").is_none());
        assert!(parse_float_literal("-0xGARBAGE12345678").is_none());
    }

    #[test]
    fn parse_inttoptr_handles_negative_and_missing_parts() {
        assert_eq!(parse_inttoptr("inttoptr (i64 5 to ptr)"), Some(5));
        assert_eq!(parse_inttoptr("inttoptr (i64 -3 to ptr)"), Some(-3));
        assert_eq!(
            parse_inttoptr("inttoptr (i64 -9223372036854775808 to ptr)"),
            Some(i64::MIN)
        );
        assert!(parse_inttoptr("something else").is_none());
        assert!(parse_inttoptr("inttoptr (i64 not-a-number to ptr)").is_none());
    }

    #[test]
    fn paren_depth_sums_correctly() {
        assert_eq!(paren_depth("()"), 0);
        assert_eq!(paren_depth("(()"), 1);
        assert_eq!(paren_depth("())"), -1);
        assert_eq!(paren_depth("[]"), 0);
        assert_eq!(paren_depth("(["), 2);
    }

    #[test]
    fn module_with_no_functions_parses_ok() {
        let m = parse_module("; just a comment\n").unwrap();
        assert!(m.functions.is_empty());
        assert!(m.entry_point().is_none());
    }

    #[test]
    fn attributes_group_reference_resolves_entry_point() {
        let src = "\
define void @main() #0 {
  ret void
}
attributes #0 = { \"entry_point\" }
";
        let m = parse_module(src).unwrap();
        assert!(m.entry_point().is_some());
    }

    #[test]
    fn declare_with_attribute_group_ref_is_not_entry_point() {
        let src = "\
declare void @decl() #0
attributes #0 = { \"entry_point\" }
";
        let m = parse_module(src).unwrap();
        // declare is never an entry point.
        assert!(m.entry_point().is_none());
    }

    #[test]
    fn is_assignment_and_token_helpers_cover_edge_cases() {
        assert!(is_assignment_prefix("  %x = call void @f()"));
        assert!(!is_assignment_prefix("call void @f()"));
        assert!(!is_assignment_prefix(""));
    }
}

#[cfg(test)]
mod final_coverage_tests {
    use super::*;

    #[test]
    fn unterminated_define_body_errors() {
        // A `define` without a closing `}` at EOF.
        let src = "%Qubit = type opaque\ndefine void @main() {\n  ret void\n";
        let err = parse_module(src).unwrap_err();
        assert!(err.to_string().contains("define body not terminated"));
    }

    #[test]
    fn call_missing_paren_after_callee_errors() {
        let err = parse_instruction_line("call void @foo").unwrap_err();
        assert!(err.to_string().contains("missing ( after callee"));
    }

    #[test]
    fn call_with_unbalanced_function_type_prefix_errors() {
        // `call void (unterminated` — the function-type prefix never closes.
        let err = parse_instruction_line("call void ( i64, i64 @bar(i64 0)").unwrap_err();
        // Either paren error path is acceptable here — what we care
        // about is that the parser produces a clear error rather than
        // silently mis-parsing.
        let msg = err.to_string();
        assert!(
            msg.contains("unbalanced") || msg.contains("missing"),
            "{msg}"
        );
    }

    #[test]
    fn empty_operand_errors() {
        // An operand that trims to nothing (no type token).
        let err = parse_operand("").unwrap_err();
        assert!(err.to_string().contains("empty operand"));
    }

    #[test]
    fn define_without_closing_brace_at_eof_errors() {
        // The `} at EOF` guard, reached by cutting off mid-block.
        let src = "\
%Qubit = type opaque
define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
";
        let err = parse_module(src).unwrap_err();
        assert!(err.to_string().contains("not terminated"));
    }

    #[test]
    fn attribute_group_parses_numeric_id() {
        let src = "\
define void @main() #3 {
  ret void
}
attributes #3 = { \"entry_point\" }
";
        let m = parse_module(src).unwrap();
        assert!(m.entry_point().is_some());
    }

    #[test]
    fn malformed_attribute_group_is_silently_skipped() {
        // `attributes` lines we can't parse are ignored — they have
        // no semantic bearing on anything except entry_point
        // detection.
        let src = "\
define void @main() #0 {
  ret void
}
attributes #0 = { \"entry_point\" }
attributes bad-line
";
        assert!(parse_module(src).is_ok());
    }

    #[test]
    fn br_i1_with_false_label_non_matching_errors() {
        let err = parse_instruction_line("br i1 %c, label %t, label bad").unwrap_err();
        assert!(err.to_string().contains("expected 'label %name'"));
    }

    #[test]
    fn instruction_with_no_opcode_surfaces_error() {
        // A line that starts with only whitespace and ends with just `=`
        // — parse_instruction_line sees no opcode token at all.
        let err = parse_instruction_line("  ").unwrap_err();
        // This input triggers the empty-instruction guard.
        assert!(!err.to_string().is_empty());
    }
}

#[cfg(test)]
mod last_mile_coverage {
    use super::*;

    #[test]
    fn define_header_with_brace_on_next_line() {
        // LLVM textual IR sometimes wraps a long signature so the `{`
        // lands on its own line. We need to skip until we see it.
        let src = "\
%Qubit = type opaque
define void @main() #0
{
  call void @__quantum__qis__h__body(%Qubit* null)
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
attributes #0 = { \"entry_point\" }
";
        let m = parse_module(src).unwrap();
        assert!(m.entry_point().is_some());
    }

    #[test]
    fn define_header_with_blank_lines_before_brace() {
        // Empty lines between the signature and the `{` are tolerated.
        let src = "\
%Qubit = type opaque
define void @main() #0

  ; trailing comment line
{
  ret void
}
attributes #0 = { \"entry_point\" }
";
        assert!(parse_module(src).is_ok());
    }

    #[test]
    fn opaque_ptr_ssa_operand() {
        let Operand::Ssa(id) = parse_operand("ptr %x").unwrap() else {
            panic!()
        };
        assert_eq!(id, "x");
    }

    #[test]
    fn opaque_ptr_global_ref_operand() {
        let Operand::GlobalRef(g) = parse_operand("ptr @g").unwrap() else {
            panic!()
        };
        assert_eq!(g, "g");
    }

    #[test]
    fn i8_typed_pointer_null_is_i8null() {
        assert!(matches!(
            parse_operand("i8* null").unwrap(),
            Operand::I8Null
        ));
    }

    #[test]
    fn pointer_operand_with_unrecognized_value_errors() {
        let err = parse_operand("ptr garbage").unwrap_err();
        assert!(err
            .to_string()
            .contains("could not resolve pointer operand"));
    }

    #[test]
    fn quoted_struct_typed_pointer_canonicalizes_to_name() {
        let ty = parse_struct_ptr_type("%\"MyStruct\"*");
        assert_eq!(ty, Some("MyStruct".to_string()));
    }

    #[test]
    fn struct_ptr_type_rejects_non_struct() {
        assert!(parse_struct_ptr_type("i32*").is_none());
        assert!(parse_struct_ptr_type("ptr").is_none());
        assert!(parse_struct_ptr_type("%Qubit").is_none()); // no trailing *
    }

    #[test]
    fn inttoptr_missing_index_returns_none() {
        assert!(parse_inttoptr("inttoptr (i64 to ptr)").is_none());
        assert!(parse_inttoptr("inttoptr garbage").is_none());
    }

    #[test]
    fn call_parser_recognizes_tail_call_qualifiers() {
        // Tail-call qualifiers followed by the call opcode.
        let instr =
            parse_instruction_line("musttail call void @__quantum__qis__h__body(%Qubit* null)")
                .unwrap();
        matches!(instr, Instruction::Call { .. });
    }

    /// Full module fixture for a Bell-state circuit — exercises
    /// `parse_module` end-to-end.
    const BELL_STATE_MODULE: &str = r#"
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cnot__body(%Qubit* null,
                                        %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*),
                                      %Result* inttoptr (i64 1 to %Result*))
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { "irreversible" }
"#;

    #[test]
    fn bell_state_module_parses_end_to_end() {
        let m = parse_module(BELL_STATE_MODULE).unwrap();
        // Three declares + one define.
        assert_eq!(m.functions.len(), 4);
        let main = m
            .functions
            .iter()
            .find(|f| f.name == "main")
            .expect("main missing");
        assert!(main.is_entry_point);
        assert!(!main.is_declaration);
        // Single `entry` block containing the four calls plus ret.
        assert_eq!(main.blocks.len(), 1);
        assert_eq!(main.blocks[0].name, "entry");
        assert_eq!(main.blocks[0].instructions.len(), 5);
        // Confirm the two wrapped calls (cnot and mz #2) were
        // accumulated correctly across their continuation lines.
        let call_count = main.blocks[0]
            .instructions
            .iter()
            .filter(|i| matches!(i, Instruction::Call { .. }))
            .count();
        assert_eq!(call_count, 4);
        assert!(matches!(
            main.blocks[0].instructions.last(),
            Some(Instruction::Ret)
        ));
    }

    /// Full module fixture exercising the complex-instruction opcodes
    /// (`icmp`, `xor`, `and`, `select`, `add`, `phi`, `br i1`).
    /// Confirms `parse_module` accepts them end-to-end.
    const COMPLEX_INSTRUCTIONS_MODULE: &str = r#"
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
entry:
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %r0 = call i1 @__quantum__qis__read_result__body(%Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*),
                                      %Result* inttoptr (i64 1 to %Result*))
  %r1 = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %not_r0 = xor i1 %r0, true
  %both = and i1 %r0, %r1
  %eq = icmp eq i1 %r0, %r1
  %count = select i1 %both, i32 2, i32 0
  %total = add nuw nsw i32 %count, 1
  br i1 %eq, label %then, label %exit
then:
  br label %exit
exit:
  %final = phi i32 [ %total, %entry ], [ 0, %then ]
  ret void
}
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)
attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { "irreversible" }
"#;

    #[test]
    fn complex_instructions_module_parses_end_to_end() {
        let m = parse_module(COMPLEX_INSTRUCTIONS_MODULE).unwrap();
        let main = m.functions.iter().find(|f| f.name == "main").unwrap();
        assert!(main.is_entry_point);
        assert_eq!(main.blocks.len(), 3);
        assert_eq!(main.blocks[0].name, "entry");
        assert_eq!(main.blocks[1].name, "then");
        assert_eq!(main.blocks[2].name, "exit");
        // Confirm each complex-instruction shape landed as the
        // expected variant somewhere in the function body.
        let all: Vec<&Instruction> = main.blocks.iter().flat_map(|b| &b.instructions).collect();
        assert!(all.iter().any(|i| matches!(i, Instruction::Icmp(_))));
        assert!(all.iter().any(|i| matches!(
            i,
            Instruction::BinaryI1 {
                op: BinaryI1Op::Xor,
                ..
            }
        )));
        assert!(all.iter().any(|i| matches!(
            i,
            Instruction::BinaryI1 {
                op: BinaryI1Op::And,
                ..
            }
        )));
        assert!(all.iter().any(|i| matches!(i, Instruction::Select { .. })));
        assert!(all.iter().any(|i| matches!(
            i,
            Instruction::IntArith {
                op: IntArithOp::Add,
                ..
            }
        )));
        assert!(all.iter().any(|i| matches!(i, Instruction::Phi { .. })));
    }

    /// Full module fixture exercising the memory-family opcodes
    /// (`alloca`, `bitcast`, `getelementptr`, `store`, `load`). These
    /// are recognizers: shapes that don't match the expected form fall
    /// through to `Instruction::Ignored`. This fixture uses the shapes
    /// the recognizers accept.
    const MEMORY_INSTRUCTIONS_MODULE: &str = r#"
define void @main() #0 {
entry:
  %arr = alloca [2 x double], align 8
  %arr_p = bitcast [2 x double]* %arr to double*
  store double 0.1, double* %arr_p, align 8
  %e1 = getelementptr [2 x double], [2 x double]* %arr, i32 0, i32 1
  store double 0.2, double* %e1, align 8
  %v0 = load double, double* %arr_p, align 8
  ret void
}
attributes #0 = { "entry_point" }
"#;

    #[test]
    fn memory_instructions_module_parses_end_to_end() {
        let m = parse_module(MEMORY_INSTRUCTIONS_MODULE).unwrap();
        let main = m.functions.iter().find(|f| f.name == "main").unwrap();
        let all: Vec<&Instruction> = main.blocks[0].instructions.iter().collect();
        assert!(all.iter().any(|i| matches!(i, Instruction::Alloca { .. })));
        assert!(all
            .iter()
            .any(|i| matches!(i, Instruction::BitcastAlias { .. })));
        assert!(all
            .iter()
            .any(|i| matches!(i, Instruction::GetElementPtrOffset { .. })));
        assert!(all.iter().any(|i| matches!(i, Instruction::Store { .. })));
        assert!(all.iter().any(|i| matches!(i, Instruction::Load { .. })));
    }

    #[test]
    fn wrapped_plain_call_accumulates_across_lines() {
        // A plain `call` whose operand list is wrapped across two
        // lines.
        let module = r#"
%Qubit = type opaque
define void @main() #0 {
entry:
  call void @__quantum__qis__cnot__body(%Qubit* null,
                                        %Qubit* inttoptr (i64 1 to %Qubit*))
  ret void
}
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="2" "requiredResults"="0" }
"#;
        let m = parse_module(module).unwrap();
        let main = m.functions.iter().find(|f| f.name == "main").unwrap();
        assert_eq!(main.blocks[0].instructions.len(), 2); // call + ret
        assert!(matches!(
            main.blocks[0].instructions[0],
            Instruction::Call { .. }
        ));
    }

    #[test]
    fn wrapped_tail_call_accumulates_across_lines() {
        // A `tail call` whose operand list wraps across multiple
        // lines. The callee `@name` appears on a line after the parens
        // have balanced once, so the accumulator must know the
        // instruction is a `call` (via `effective_opcode` seeing
        // through the `tail` qualifier) to continue reading.
        let module = r#"
%Qubit = type opaque
define void @main() #0 {
entry:
  tail call void (i64, i64, i64, i64, i8*, ...)
      @generalizedInvokeWithRotationsControlsTargets(
          i64 0, i64 0, i64 1, i64 1,
          i8* bitcast (void (%Qubit*)* @__quantum__qis__x__ctl to i8*),
          %Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  ret void
}
declare void @generalizedInvokeWithRotationsControlsTargets(i64, i64, i64, i64, i8*, ...)
declare void @__quantum__qis__x__ctl(%Qubit*)
attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="2" "requiredResults"="0" }
"#;
        let m = parse_module(module).unwrap();
        let main = m.functions.iter().find(|f| f.name == "main").unwrap();
        assert_eq!(main.blocks[0].instructions.len(), 2); // tail call + ret
        assert!(matches!(
            main.blocks[0].instructions[0],
            Instruction::Call { .. }
        ));
    }

    #[test]
    fn wrapped_assignment_tail_call_accumulates_across_lines() {
        // `%x = tail call ...` wrapped across lines — exercises both
        // the assignment-prefix and the qualifier stripping in
        // `effective_opcode`.
        let module = r#"
%Result = type opaque
define void @main() #0 {
entry:
  %r = tail call i1
      @__quantum__qis__read_result__body(
          %Result* inttoptr (i64 0 to %Result*))
  ret void
}
declare i1 @__quantum__qis__read_result__body(%Result*)
attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="0" "requiredResults"="1" }
"#;
        let m = parse_module(module).unwrap();
        let main = m.functions.iter().find(|f| f.name == "main").unwrap();
        assert_eq!(main.blocks[0].instructions.len(), 2);
        assert!(matches!(
            main.blocks[0].instructions[0],
            Instruction::Call { .. }
        ));
    }

    #[test]
    fn effective_opcode_sees_through_assignment_and_qualifiers() {
        assert_eq!(effective_opcode("call void @f()"), "call");
        assert_eq!(effective_opcode("tail call void @f()"), "call");
        assert_eq!(effective_opcode("musttail call void @f()"), "call");
        assert_eq!(effective_opcode("notail call void @f()"), "call");
        assert_eq!(effective_opcode("%x = call void @f()"), "call");
        assert_eq!(effective_opcode("%x = tail call i1 @f()"), "call");
        assert_eq!(effective_opcode("%x = musttail call void @f()"), "call");
        assert_eq!(effective_opcode("ret void"), "ret");
        assert_eq!(effective_opcode("br label %exit"), "br");
        // Assignment with no call qualifier still resolves opcode.
        assert_eq!(effective_opcode("%x = add i32 1, 2"), "add");
    }

    #[test]
    fn strip_call_prefix_qualifiers_handles_stacked_qualifiers() {
        // LLVM only allows one prefix qualifier at a time, but the
        // helper should still consume them left-to-right for safety.
        assert_eq!(strip_call_prefix_qualifiers("tail call foo"), "call foo");
        assert_eq!(
            strip_call_prefix_qualifiers("musttail call foo"),
            "call foo"
        );
        assert_eq!(strip_call_prefix_qualifiers("notail call foo"), "call foo");
        assert_eq!(strip_call_prefix_qualifiers("call foo"), "call foo");
        // No qualifier — pass-through.
        assert_eq!(strip_call_prefix_qualifiers("add i32 1, 2"), "add i32 1, 2");
    }
}
