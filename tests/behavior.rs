//! Behavior-driven tests. One test per contract in BEHAVIORS.md, named
//! `b<N>_given_..._then_...`. Parser behaviors call the library; CLI
//! behaviors (B6–B13) run the real binary; B14 frame invariants live in
//! `tests/golden.rs` (canvas checks on every case).

use llmaid::parse::{Shape, parse};
use std::process::{Command, Stdio};

// ---------- Parsing ----------

#[test]
fn b1_given_br_tags_then_label_has_line_breaks() {
    let g = parse("flowchart LR\nA[first<br/>second<br>third<BR />fourth]").unwrap();
    assert_eq!(g.nodes[0].label, "first\nsecond\nthird\nfourth");

    // Not a br tag: stays literal.
    let g = parse("flowchart LR\nB[a <b> c]").unwrap();
    assert_eq!(g.nodes[0].label, "a <b> c");
}

#[test]
fn b2_given_redeclared_node_then_last_wins_with_warning() {
    let g = parse("flowchart LR\nA[First]\nA[Second]\nA --> B").unwrap();
    assert_eq!(g.nodes[0].label, "Second");
    assert!(
        g.warnings
            .iter()
            .any(|w| w.line == 3 && w.msg.contains("redeclared")),
        "expected a redeclaration warning, got: {:?}",
        g.warnings
    );

    // Bare reference and identical redeclaration never warn.
    let g = parse("flowchart LR\nA[First]\nA --> B\nA[First]").unwrap();
    assert!(g.warnings.is_empty(), "got: {:?}", g.warnings);
}

#[test]
fn b2_given_shape_change_then_warning_names_previous() {
    let g = parse("flowchart LR\nA[Box]\nA{Box}").unwrap();
    assert_eq!(g.nodes[0].shape, Shape::Diamond);
    assert!(
        g.warnings[0].msg.contains("rect \"Box\""),
        "got: {}",
        g.warnings[0].msg
    );
}

#[test]
fn b3_given_parallel_edges_then_both_kept_with_labels() {
    let g = parse("flowchart LR\nA -->|x| B\nA -->|y| B").unwrap();
    assert_eq!(g.edges.len(), 2);
    assert_eq!(g.edges[0].label.as_deref(), Some("x"));
    assert_eq!(g.edges[1].label.as_deref(), Some("y"));
    assert_eq!(
        (g.edges[0].from, g.edges[0].to),
        (g.edges[1].from, g.edges[1].to)
    );
}

#[test]
fn b4_given_malformed_input_then_error_names_line_and_expectation() {
    let cases: &[(&str, usize, &str)] = &[
        ("flowchart LR\nA[unclosed", 2, "expected closing `]`"),
        ("flowchart LR\nA -->", 2, "expected a node id"),
        ("flowchart LR\nA --> B\nB ->> C", 3, "expected an edge"),
        (
            "flowchart LR\nA -->|no close B",
            2,
            "unterminated edge label",
        ),
    ];
    for (src, line, needle) in cases {
        let err = parse(src).unwrap_err();
        assert_eq!(err.line, *line, "for {src:?}");
        assert!(err.msg.contains(needle), "for {src:?}: got `{}`", err.msg);
    }
}

#[test]
fn b5_given_unknown_directives_then_warn_and_continue() {
    let g = parse("flowchart TB\nclassDef default fill:#f9f\nA --> B").unwrap();
    assert_eq!(g.nodes.len(), 2);
    assert_eq!(g.edges.len(), 1);
    assert!(g.warnings.iter().any(|w| w.msg.contains("classDef")));
}

#[test]
fn b15_given_subgraph_then_members_recorded_and_frame_renders() {
    let src = "\
flowchart TB
  subgraph Pipe [Pipeline]
    A[src] --> B[tok]
  end
  B --> C[out]
";
    let g = parse(src).unwrap();
    assert_eq!(g.subgraphs.len(), 1);
    assert_eq!(g.subgraphs[0].id, "Pipe");
    assert_eq!(g.subgraphs[0].title, "Pipeline");
    assert_eq!(g.subgraphs[0].members.len(), 2);
    assert_eq!(g.nodes[g.subgraphs[0].members[0]].id, "A");
    assert_eq!(g.nodes[g.subgraphs[0].members[1]].id, "B");
    assert_eq!(g.node_sg[0], Some(0));
    assert_eq!(g.node_sg[1], Some(0));
    assert_eq!(g.node_sg[2], None); // C outside
    assert!(
        !g.warnings.iter().any(|w| w.msg.contains("subgraph ignored")),
        "subgraph should not be ignored: {:?}",
        g.warnings
    );

    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("Pipeline"),
        "subgraph title missing from render:\n{stdout}"
    );
    assert!(
        stdout.contains("src") && stdout.contains("tok") && stdout.contains("out"),
        "node labels missing:\n{stdout}"
    );
    assert!(
        !stderr.contains("subgraph ignored"),
        "should not warn ignore: {stderr}"
    );
}

// ---------- CLI ----------

fn run_llmaid(args: &[&str], stdin: &str) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_llmaid"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        out.status.code().unwrap(),
    )
}

#[test]
fn b5_given_strict_flag_then_warnings_fail_with_exit_64() {
    let (_, stderr, code) = run_llmaid(&["--strict"], "A --> B\n");
    assert_eq!(code, 64);
    assert!(stderr.contains("--strict"));
}

#[test]
fn b6_given_warnings_then_stdout_has_only_the_diagram() {
    let (stdout, stderr, code) = run_llmaid(&[], "classDef x fill:#f9f\nflowchart LR\nA --> B\n");
    assert_eq!(code, 0);
    assert!(stderr.contains("warning"), "warnings must go to stderr");
    assert!(
        !stdout.contains("warning"),
        "stdout must never carry diagnostics"
    );
    assert!(!stdout.trim().is_empty());
}

#[test]
fn b7_given_no_nodes_then_exit_0_empty_stdout_warning_on_stderr() {
    for input in ["", "flowchart LR\n", "%% just a comment\n"] {
        let (stdout, stderr, code) = run_llmaid(&[], input);
        assert_eq!(code, 0, "for {input:?}");
        assert_eq!(stdout, "", "for {input:?}");
        assert!(
            stderr.contains("nothing to render"),
            "for {input:?}: {stderr}"
        );
    }
}

#[test]
fn b8_given_same_input_then_byte_identical_output_across_runs() {
    let src = "flowchart LR\nA -->|x| B & C\nB & C --> D\n";
    let (out1, _, _) = run_llmaid(&[], src);
    let (out2, _, _) = run_llmaid(&[], src);
    assert_eq!(out1, out2);
    assert!(!out1.is_empty());
}

// ---------- Layout & rendering ----------

#[test]
fn b9_given_over_width_then_degrade_without_truncation_or_failure() {
    let src = "\
flowchart LR
  a[source alpha] -->|scan tokens| b[Vec of Token]
  b -->|parse tree| c[Expr AST node]
  c -->|eval result| d[Value output]
";
    let (stdout, stderr, code) = run_llmaid(&["--width", "36"], src);
    assert_eq!(code, 0, "must not fail on overflow: {stderr}");
    assert!(
        !stdout.contains('…') && !stdout.contains("..."),
        "must never truncate labels:\n{stdout}"
    );
    // Edge labels are short enough to stay intact; node labels may wrap mid-word
    // under extreme pressure — require full text as a char subsequence (B9).
    for needle in ["scan tokens", "parse tree", "eval result"] {
        assert!(
            stdout.contains(needle),
            "edge label `{needle}` missing under width pressure:\n{stdout}"
        );
    }
    for needle in ["sourcealpha", "VecofToken", "ExprASTnode", "Valueoutput"] {
        assert!(
            alnum_subsequence(&stdout, needle),
            "node label `{needle}` not fully present (wrapped or otherwise):\n{stdout}"
        );
    }
}

#[test]
fn b10_given_label_fits_then_stays_one_line() {
    let src = "flowchart LR\nA[hello world] --> B[ok]\n";
    // Default width (100) is comfortable — must not wrap "hello world".
    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("hello world"),
        "label should stay on one line when it fits:\n{stdout}"
    );
    // Under severe width pressure, wrapping is allowed (B9) — full text preserved.
    let (tight, _, code) = run_llmaid(&["--width", "12"], src);
    assert_eq!(code, 0);
    assert!(
        alnum_subsequence(&tight, "helloworld"),
        "wrapped form must still carry full text:\n{tight}"
    );
}

/// True if `needle`'s alphanumeric chars appear in order in `hay` (ignoring
/// other characters). Used when labels wrap across lines under B9 pressure.
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
fn phase0_rl_bt_mirror_and_tb_edge_labels() {
    // RL: flow flips; label and arrow still present.
    let (rl, _, code) = run_llmaid(&[], "flowchart RL\nA[src] -->|go| B[dst]\n");
    assert_eq!(code, 0);
    assert!(rl.contains("go"), "RL label missing:\n{rl}");
    assert!(
        rl.contains('◀') || rl.contains('<'),
        "RL should arrow toward the flow target:\n{rl}"
    );

    // BT: upward flow with on-shaft label.
    let (bt, _, code) = run_llmaid(&[], "flowchart BT\nA[bottom] -->|up| B[top]\n");
    assert_eq!(code, 0);
    assert!(bt.contains("up"), "BT label missing:\n{bt}");
    assert!(
        bt.contains('▲') || bt.contains('^'),
        "BT should arrow upward:\n{bt}"
    );

    // TB: labels sit beside the vertical run (Phase 0.3).
    let (tb, _, code) = run_llmaid(&[], "flowchart TB\nA[top] -->|down| B[bottom]\n");
    assert_eq!(code, 0);
    assert!(tb.contains("down"), "TB edge label missing:\n{tb}");
    assert!(
        tb.contains('▼') || tb.contains('v'),
        "TB should arrow downward:\n{tb}"
    );
}

#[test]
fn b11_given_self_loop_and_back_edge_then_routes_return_to_targets() {
    let src = "\
flowchart TB
  scan[Scanner] --> parse[Parser] --> eval[Interpreter]
  eval -->|next line| scan
  eval --> eval
";
    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("next line"),
        "back-edge label was lost:\n{stdout}"
    );
    assert!(
        stdout.contains('◀'),
        "back edge should return from the perimeter:\n{stdout}"
    );
    assert!(
        stdout.contains('▲'),
        "self-loop should return into the node:\n{stdout}"
    );
}

#[test]
fn b12_given_parallel_edges_then_distinct_paths_and_labels() {
    let src = "\
flowchart LR
  A -->|x| B
  A -->|y| B
";
    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains('x'),
        "first parallel edge label lost:\n{stdout}"
    );
    assert!(
        stdout.contains('y'),
        "second parallel edge label lost:\n{stdout}"
    );
    let arrows = stdout.chars().filter(|&c| c == '▶').count();
    assert!(
        arrows >= 2,
        "expected two distinct arrowheads for parallel edges, got {arrows}:\n{stdout}"
    );
}

#[test]
fn b13_given_non_rect_shapes_then_rect_frame_with_shape_hints() {
    let src = "\
flowchart LR
  r[rect] --> ro(rounded) --> st([stadium]) --> ci((circle))
  cy[(cylinder)] --> di{diamond} --> hx{{hexagon}}
";
    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    // All labels present (never truncated / lost to shape drawing).
    for label in ["rect", "rounded", "stadium", "circle", "cylinder", "diamond", "hexagon"] {
        assert!(
            stdout.contains(label),
            "missing label `{label}`:\n{stdout}"
        );
    }
    // D13 hints: diamond corners, stadium/circle caps, cylinder lid, hex facets.
    assert!(
        stdout.contains('◇'),
        "diamond should use ◇ corner hints:\n{stdout}"
    );
    assert!(
        stdout.contains('(') && stdout.contains(')'),
        "stadium/circle should use ( ) caps:\n{stdout}"
    );
    assert!(
        stdout.contains('═'),
        "cylinder should use a lid on the top edge:\n{stdout}"
    );
    assert!(
        stdout.contains('╱') && stdout.contains('╲'),
        "hexagon should use faceted corner hints:\n{stdout}"
    );
    // Rect frame still present (grid discipline).
    assert!(
        stdout.contains('│') || stdout.contains('╭') || stdout.contains('┌'),
        "expected rect-framed boxes:\n{stdout}"
    );
}
