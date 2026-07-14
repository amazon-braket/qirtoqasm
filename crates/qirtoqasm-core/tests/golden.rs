//! Byte-exact golden-fixture integration tests.
//!
//! For every `.ll` file in `test/integ_tests/fixtures_qir/` that has a sibling
//! `.qasm` file, parse the QIR, translate it, and assert the output equals
//! the fixture bytes exactly. Lines starting with `// generated-by:`
//! are stripped from both sides before comparison so fixtures don't
//! need to be regenerated on version bumps.
//!
//! Fixtures without a sibling `.qasm` are negative cases handled by
//! `known_limitations.rs`.

use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    // The tests live at `crates/qirtoqasm-core/tests/` and the fixtures at
    // `test/integ_tests/fixtures_qir/` from the workspace root.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("..")
        .join("..")
        .join("test")
        .join("integ_tests")
        .join("fixtures_qir")
}

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

fn collect_positive_fixtures() -> Vec<(PathBuf, PathBuf)> {
    let dir = fixtures_dir();
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("fixtures dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("ll") {
            continue;
        }
        let qasm = path.with_extension("qasm");
        if qasm.exists() {
            out.push((path, qasm));
        }
    }
    out.sort();
    out
}

#[test]
fn every_positive_fixture_translates_byte_for_byte() {
    let fixtures = collect_positive_fixtures();
    assert!(!fixtures.is_empty(), "no fixtures found; wrong cwd?");

    let mut mismatches: Vec<(String, String, String)> = Vec::new();
    let mut errors: Vec<(String, String)> = Vec::new();

    for (ll, qasm) in &fixtures {
        let name = ll.file_stem().unwrap().to_string_lossy().to_string();
        let src = fs::read_to_string(ll).unwrap();
        let expected = fs::read_to_string(qasm).unwrap();
        match qirtoqasm_core::translate(&src, &qirtoqasm_core::TranslateOptions::default()) {
            Ok(got) => {
                let got_stripped = strip_generated_by(&got);
                let expected_stripped = strip_generated_by(&expected);
                if got_stripped != expected_stripped {
                    mismatches.push((name, expected_stripped, got_stripped));
                }
            }
            Err(e) => errors.push((name, format!("{e}"))),
        }
    }

    if !errors.is_empty() || !mismatches.is_empty() {
        let mut msg = String::new();
        for (n, e) in &errors {
            msg.push_str(&format!("\n[error] {n}: {e}"));
        }
        for (n, exp, got) in &mismatches {
            msg.push_str(&format!(
                "\n[mismatch] {n}:\n--- expected ---\n{exp}--- got ---\n{got}"
            ));
        }
        panic!(
            "golden fixture parity failures ({} errors, {} mismatches):{}",
            errors.len(),
            mismatches.len(),
            msg
        );
    }
}
