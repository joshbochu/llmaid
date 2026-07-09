//! Golden snapshot tests.
//!
//! Each `tests/cases/NAME.mmd` is parsed and its IR dump compared byte-for-byte
//! against `tests/cases/NAME.ir`. (M2 adds `NAME.txt` for rendered output.)
//!
//! Regenerate snapshots with: UPDATE_GOLDEN=1 cargo test
//! Only commit regenerated snapshots when you can say why the new output is better.

use std::fs;
use std::path::PathBuf;

fn cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases")
}

#[test]
fn golden_parse_snapshots() {
    let dir = cases_dir();
    let update = std::env::var("UPDATE_GOLDEN").is_ok();

    let mut names: Vec<String> = fs::read_dir(&dir)
        .expect("tests/cases must exist")
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".mmd").map(str::to_string)
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no .mmd cases found in tests/cases");

    let mut failures = Vec::new();
    for name in &names {
        let src = fs::read_to_string(dir.join(format!("{name}.mmd"))).unwrap();
        let graph = match llmaid::parse::parse(&src) {
            Ok(g) => g,
            Err(e) => panic!("case `{name}` failed to parse: {e}"),
        };
        let got = llmaid::parse::dump(&graph);
        let ir_path = dir.join(format!("{name}.ir"));

        if update {
            fs::write(&ir_path, &got).unwrap();
            continue;
        }
        let want = fs::read_to_string(&ir_path).unwrap_or_else(|_| {
            panic!("missing snapshot {name}.ir — run UPDATE_GOLDEN=1 cargo test")
        });
        if got != want {
            failures.push(format!(
                "case `{name}` diverged from snapshot:\n--- want ---\n{want}\n--- got ---\n{got}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn parse_errors_name_line_and_expectation() {
    let err = llmaid::parse::parse("flowchart LR\nA[unclosed").unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.msg.contains("unterminated"), "got: {}", err.msg);
    assert!(
        err.msg.contains("]"),
        "should name the expected closer: {}",
        err.msg
    );

    let err = llmaid::parse::parse("flowchart LR\nA -->").unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.msg.contains("expected a node id"), "got: {}", err.msg);

    let err = llmaid::parse::parse("flowchart LR\nA --> B\nB ->> C").unwrap_err();
    assert_eq!(err.line, 3);
    assert!(err.msg.contains("expected an edge"), "got: {}", err.msg);

    let err = llmaid::parse::parse("flowchart LR\nA -->|no close B").unwrap_err();
    assert_eq!(err.line, 2);
    assert!(
        err.msg.contains("unterminated edge label"),
        "got: {}",
        err.msg
    );
}

#[test]
fn determinism_same_input_same_dump() {
    let src = "flowchart LR\nA --> B & C\nB & C --> D\nD -.->|retry| A\n";
    let a = llmaid::parse::dump(&llmaid::parse::parse(src).unwrap());
    let b = llmaid::parse::dump(&llmaid::parse::parse(src).unwrap());
    assert_eq!(a, b);
}
