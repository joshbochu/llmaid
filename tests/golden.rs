//! Golden snapshot tests.
//!
//! Each `tests/cases/NAME.mmd` is compared against:
//! - `NAME.ir` — parse IR dump
//! - `NAME.txt` — rendered diagram (default width 100, unicode)
//!
//! Frame invariants (B14) run on every case.
//!
//! Regenerate snapshots with: UPDATE_GOLDEN=1 cargo test
//! Only commit regenerated snapshots when you can say why the new output is better.

use llmaid::diagram::{self, Diagram};
use llmaid::layout;
use llmaid::parse;
use llmaid::render;
use llmaid::style::Style;
use std::fs;
use std::path::PathBuf;

fn cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases")
}

fn case_names() -> Vec<String> {
    let dir = cases_dir();
    let mut names: Vec<String> = fs::read_dir(&dir)
        .expect("tests/cases must exist")
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".mmd").map(str::to_string)
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no .mmd cases found in tests/cases");
    names
}

#[test]
fn golden_parse_snapshots() {
    let dir = cases_dir();
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    let mut failures = Vec::new();
    for name in case_names() {
        let src = fs::read_to_string(dir.join(format!("{name}.mmd"))).unwrap();
        let diagram = match diagram::parse(&src) {
            Ok(diagram) => diagram,
            Err(e) => panic!("case `{name}` failed to parse: {e}"),
        };
        let got = diagram::dump(&diagram);
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
                "case `{name}` IR diverged from snapshot:\n--- want ---\n{want}\n--- got ---\n{got}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn golden_render_snapshots() {
    let dir = cases_dir();
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    let style = Style { ascii: false };
    let mut failures = Vec::new();
    for name in case_names() {
        let src = fs::read_to_string(dir.join(format!("{name}.mmd"))).unwrap();
        let diagram = match diagram::parse(&src) {
            Ok(diagram) => diagram,
            Err(e) => panic!("case `{name}` failed to parse: {e}"),
        };
        if diagram.is_empty() {
            continue;
        }

        let scene = diagram::scene(&diagram, 100);
        let (got, inv) = render::render_scene_with_checks(&scene, style);
        if !inv.is_empty() {
            failures.push(format!(
                "case `{name}` invariant failures:\n  - {}",
                inv.join("\n  - ")
            ));
        }

        let txt_path = dir.join(format!("{name}.txt"));
        if update {
            fs::write(&txt_path, &got).unwrap();
            continue;
        }
        let want = fs::read_to_string(&txt_path).unwrap_or_else(|_| {
            panic!("missing snapshot {name}.txt — run UPDATE_GOLDEN=1 cargo test")
        });
        if got != want {
            failures.push(format!(
                "case `{name}` render diverged from snapshot:\n--- want ---\n{want}\n--- got ---\n{got}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn b14_frame_invariants_hold_for_all_cases() {
    let dir = cases_dir();
    let style = Style { ascii: false };
    let mut failures = Vec::new();
    for name in case_names() {
        let src = fs::read_to_string(dir.join(format!("{name}.mmd"))).unwrap();
        let parsed = diagram::parse(&src).unwrap_or_else(|e| panic!("{name}: {e}"));
        if parsed.is_empty() {
            continue;
        }
        let scene = diagram::scene(&parsed, 100);
        let (rendered, inv) = render::render_scene_with_checks(&scene, style);
        assert!(
            !rendered.contains('…') && !rendered.contains("..."),
            "case `{name}` truncated"
        );
        if !inv.is_empty() {
            failures.push(format!("`{name}`: {}", inv.join("; ")));
        }
        // Labels fully present in finished text (B14 / no overwrite).
        let labels: Vec<&str> = match &parsed {
            Diagram::Flowchart(graph) => graph
                .nodes
                .iter()
                .map(|node| node.label.as_str())
                .chain(graph.edges.iter().filter_map(|edge| edge.label.as_deref()))
                .collect(),
            Diagram::Sequence(sequence) => sequence
                .participants
                .iter()
                .map(|participant| participant.label.as_str())
                .chain(sequence.events.iter().filter_map(|event| match event {
                    llmaid::sequence::SequenceEvent::Message(message) => {
                        Some(message.label.as_str())
                    }
                    llmaid::sequence::SequenceEvent::Note(note) => Some(note.text.as_str()),
                    llmaid::sequence::SequenceEvent::Activation(_) => None,
                }))
                .collect(),
        };
        for label in labels {
            let want: String = label.chars().filter(|c| c.is_alphanumeric()).collect();
            if !want.is_empty() && !alnum_subsequence(&rendered, &want) {
                failures.push(format!("`{name}`: label missing: {label}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "B14 invariant failures:\n  - {}",
        failures.join("\n  - ")
    );
}

fn alnum_subsequence(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars().filter(|c| c.is_alphanumeric());
    for want in needle.chars() {
        loop {
            match it.next() {
                Some(c) if c == want => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

#[test]
fn parse_errors_name_line_and_expectation() {
    let err = parse::parse("flowchart LR\nA[unclosed").unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.msg.contains("unterminated"), "got: {}", err.msg);
    assert!(
        err.msg.contains("]"),
        "should name the expected closer: {}",
        err.msg
    );

    let err = parse::parse("flowchart LR\nA -->").unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.msg.contains("expected a node id"), "got: {}", err.msg);

    let err = parse::parse("flowchart LR\nA --> B\nB ->> C").unwrap_err();
    assert_eq!(err.line, 3);
    assert!(err.msg.contains("expected an edge"), "got: {}", err.msg);

    let err = parse::parse("flowchart LR\nA -->|no close B").unwrap_err();
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
    let a = parse::dump(&parse::parse(src).unwrap());
    let b = parse::dump(&parse::parse(src).unwrap());
    assert_eq!(a, b);
}

#[test]
fn determinism_same_input_same_render() {
    let src = "flowchart LR\nA -->|x| B & C\nB & C --> D\n";
    let g = parse::parse(src).unwrap();
    let style = Style { ascii: false };
    let a = render::render(&g, &layout::layout(&g, 100), style);
    let b = render::render(&g, &layout::layout(&g, 100), style);
    assert_eq!(a, b);
}
