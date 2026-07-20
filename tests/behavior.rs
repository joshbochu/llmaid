//! Behavior-driven tests. One test per contract in BEHAVIORS.md, named
//! `b<N>_given_..._then_...`. Parser behaviors call the library; CLI
//! behaviors (B6–B13) run the real binary; B14/B16 scene invariants also run
//! across every golden in `tests/golden.rs`.

use llmaid::layout;
use llmaid::parse::{Shape, parse};
use llmaid::render;
use llmaid::style::Style;
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
    assert!(
        !g.warnings
            .iter()
            .any(|w| w.msg.contains("subgraph ignored")),
        "subgraph should not be ignored: {:?}",
        g.warnings
    );

    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("Pipeline"),
        "subgraph title missing from render:\n{stdout}"
    );
    // Title sits on an interior row (│ … Pipeline …), not welded into ╭──╮.
    assert!(
        stdout
            .lines()
            .any(|l| l.contains('│') && l.contains("Pipeline")),
        "title should be on an interior frame row:\n{stdout}"
    );
    assert!(
        stdout.contains("src") && stdout.contains("tok") && stdout.contains("out"),
        "node labels missing:\n{stdout}"
    );
    assert!(
        !stderr.contains("subgraph ignored"),
        "should not warn ignore: {stderr}"
    );

    // Nested: outer frame above inner title.
    let nested = "\
flowchart TB
  subgraph Outer
    subgraph Inner
      A --> B
    end
    B --> C
  end
  C --> D
";
    let (nout, nerr, ncode) = run_llmaid(&[], nested);
    assert_eq!(ncode, 0, "{nerr}");
    assert!(nout.contains("Outer") && nout.contains("Inner"), "\n{nout}");
    let lines: Vec<&str> = nout.lines().collect();
    let outer_i = lines
        .iter()
        .position(|l| l.contains("Outer"))
        .expect("Outer");
    let inner_i = lines
        .iter()
        .position(|l| l.contains("Inner"))
        .expect("Inner");
    assert!(
        outer_i < inner_i,
        "Outer title should appear above Inner:\n{nout}"
    );
}

#[test]
fn b16_given_nested_merges_then_edges_avoid_non_endpoint_boxes() {
    let source = "\
flowchart TB
  D[Final]
  M[Merge]
  A[Left] --> M
  B[RightSide] --> M
  M --> D
  C[Other] --> D
";
    let graph = parse(source).unwrap();
    let placed = layout::layout(&graph, 100);
    let (_, failures) = render::render_with_checks(&graph, &placed, Style { ascii: false });

    assert!(
        failures
            .iter()
            .all(|failure| !failure.contains("intersects non-endpoint box")),
        "B16 failures:\n  - {}",
        failures.join("\n  - ")
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
fn b12_given_vertical_parallel_edges_then_each_label_has_its_own_lane() {
    let src = "\
flowchart TB
  A -->|one| B
  A -->|two| B
  A -->|three| B
";
    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    for label in ["one", "two", "three"] {
        assert_eq!(
            stdout.matches(label).count(),
            1,
            "edge label `{label}` should appear exactly once:\n{stdout}"
        );
    }
    assert_eq!(
        stdout.chars().filter(|&c| c == '▼').count(),
        3,
        "each parallel edge should retain its arrowhead:\n{stdout}"
    );
}

#[test]
fn b12_given_vertical_fork_merge_then_edge_labels_do_not_overlap() {
    let src = "\
flowchart TB
  A -->|left-edge| B
  A -->|right-edge| C
  B -->|merge-left| D
  C -->|merge-right| D
";
    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    for label in ["left-edge", "right-edge", "merge-left", "merge-right"] {
        assert_eq!(
            stdout.matches(label).count(),
            1,
            "edge label `{label}` should appear exactly once:\n{stdout}"
        );
    }
}

#[test]
fn b13_given_non_rect_shapes_then_rect_frame_with_shape_hints() {
    let src = "\
flowchart LR
  r[rect] --> ro(rounded) --> st([stadium]) --> ci((circle))
  cy[(cylinder)] --> sr[[subroutine]] --> di{diamond} --> hx{{hexagon}}
";
    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    // All labels present (never truncated / lost to shape drawing).
    for label in [
        "rect",
        "rounded",
        "stadium",
        "circle",
        "cylinder",
        "subroutine",
        "diamond",
        "hexagon",
    ] {
        assert!(stdout.contains(label), "missing label `{label}`:\n{stdout}");
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
        stdout.lines().any(|line| line.contains("││ subroutine │")),
        "subroutine should use doubled side hints:\n{stdout}"
    );
    let (ascii, stderr, code) = run_llmaid(&["--ascii"], src);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");
    assert!(ascii.contains("|| subroutine |"), "{ascii}");
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

#[test]
fn b15_given_connected_sibling_subgraphs_then_titles_and_frames_render() {
    let src = "\
flowchart TD
  subgraph one
    A --> B
  end
  subgraph two
    C --> D
  end
  B --> C
";
    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains(" one ") && stdout.contains(" two "),
        "{stdout}"
    );
    assert!(!stderr.contains("invariant failure"), "{stderr}");
}

#[test]
fn b17_given_core_sequence_then_order_labels_and_styles_are_deterministic() {
    let source = "\
sequenceDiagram
  participant Client
  actor API as Application API
  Client->>API: request
  API-->>Client: response
";
    let (unicode_a, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    let (unicode_b, _, _) = run_llmaid(&[], source);
    assert_eq!(unicode_a, unicode_b);
    assert!(
        unicode_a.contains("▶┊"),
        "call endpoint unclear:\n{unicode_a}"
    );
    assert!(
        unicode_a.contains("┊←"),
        "return endpoint unclear:\n{unicode_a}"
    );

    let (tight, stderr, code) = run_llmaid(&["--width", "12"], source);
    assert_eq!(code, 0, "{stderr}");
    for text in ["Application API", "request", "response"] {
        assert!(tight.contains(text), "truncated {text:?}:\n{tight}");
    }

    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");
    assert!(ascii.contains(">|"), "ASCII call cue missing:\n{ascii}");
    assert!(ascii.contains("|<--"), "ASCII return cue missing:\n{ascii}");
    assert!(ascii.contains("request") && ascii.contains("response"));
}

#[test]
fn b18_given_notes_and_activation_then_placement_labels_and_styles_are_deterministic() {
    let source = "\
sequenceDiagram
  participant Client
  participant API
  Note left of Client: Caller
  Note right of API: Service
  Note over Client,API: HTTPS request
  Client->>API: request
  activate API
  API-->>Client: response
  deactivate API
";
    let (unicode_a, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    let (unicode_b, _, _) = run_llmaid(&[], source);
    assert_eq!(unicode_a, unicode_b);
    for text in ["Caller", "Service", "HTTPS request", "request", "response"] {
        assert!(unicode_a.contains(text), "missing {text:?}:\n{unicode_a}");
    }

    let (tight, stderr, code) = run_llmaid(&["--width", "12"], source);
    assert_eq!(code, 0, "{stderr}");
    for text in ["Caller", "Service", "HTTPS request", "request", "response"] {
        assert!(tight.contains(text), "truncated {text:?}:\n{tight}");
    }

    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");
    for text in ["Caller", "Service", "HTTPS request", "request", "response"] {
        assert!(ascii.contains(text), "missing {text:?}:\n{ascii}");
    }
}

#[test]
fn b19_given_audit_json_then_machine_output_is_stable_and_separate_from_diagram_output() {
    let source = "flowchart LR\nA --> B\n";
    let (first, stderr, code) = run_llmaid(&["--audit=json"], source);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stderr, "");
    assert!(first.starts_with("{\"schema\":\"llmaid.audit.v1\""));
    assert!(first.contains("\"diagram\":\"flowchart\""));
    assert!(first.contains("\"violations\":[]"));
    assert!(first.contains("\"wire_length\":"));
    assert!(
        !first.contains('╭'),
        "audit mixed with diagram output: {first}"
    );

    let (second, _, code) = run_llmaid(&["--audit=json"], source);
    assert_eq!(code, 0);
    assert_eq!(first, second, "audit JSON must be byte-deterministic");
}

#[test]
fn b20_given_nested_sequence_controls_then_frames_labels_and_ascii_are_deterministic() {
    let source = "\
sequenceDiagram
  participant Client
  participant API
  loop retry
    Client->>API: request
    alt accepted
      API-->>Client: success
    else rejected
      opt retryable
        API-->>Client: retry
      end
    end
  end
";
    let (unicode, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    for label in [
        "loop retry",
        "alt accepted",
        "else rejected",
        "opt retryable",
        "request",
        "success",
        "retry",
    ] {
        assert!(unicode.contains(label), "missing {label:?}:\n{unicode}");
    }
    assert!(
        unicode
            .lines()
            .any(|line| line.contains("├─ else rejected ")),
        "else should be a labeled divider inside alt:\n{unicode}"
    );
    let (repeat, _, code) = run_llmaid(&[], source);
    assert_eq!(code, 0);
    assert_eq!(unicode, repeat);

    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");
    assert!(ascii.contains("else rejected") && ascii.contains("retry"));
    assert!(ascii.lines().any(|line| line.contains("+- else rejected ")));
}

#[test]
fn b21_given_flat_state_diagram_then_states_markers_and_transitions_are_preserved() {
    let source = "\
stateDiagram-v2
  direction LR
  state \"Waiting for request\" as Waiting
  [*] --> Waiting
  Waiting --> Processing : request received
  Processing --> [*] : complete
";
    let (unicode, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    for label in [
        "Waiting for request",
        "Processing",
        "request received",
        "complete",
    ] {
        assert!(unicode.contains(label), "missing {label:?}:\n{unicode}");
    }
    assert!(
        unicode.contains("( * )") && unicode.contains("( O )"),
        "{unicode}"
    );

    let (repeat, _, code) = run_llmaid(&[], source);
    assert_eq!(code, 0);
    assert_eq!(unicode, repeat);
    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");

    let (_, stderr, code) = run_llmaid(&[], "stateDiagram\nstate Outer {\n");
    assert_eq!(code, 64);
    assert!(
        stderr.contains("line 2") && stderr.contains("flat state"),
        "{stderr}"
    );
}

#[test]
fn b22_given_class_diagram_then_members_relations_and_multiplicities_are_preserved() {
    let source = "\
classDiagram
  direction LR
  class Customer {
    +String name
    +buy(ticket) bool
  }
  Customer \"1\" o-- \"0..*\" Ticket : owns
  Ticket ..|> Record : persists as
";
    let (unicode, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    for label in [
        "Customer",
        "+String name",
        "+buy(ticket) bool",
        "0..*",
        "owns",
        "persists as",
    ] {
        assert!(unicode.contains(label), "missing {label:?}:\n{unicode}");
    }
    assert!(unicode.contains('◇') && unicode.contains('▷'), "{unicode}");
    assert!(
        !unicode.contains("o--") && !unicode.contains("..|>"),
        "{unicode}"
    );
    let (repeat, _, code) = run_llmaid(&[], source);
    assert_eq!(code, 0);
    assert_eq!(unicode, repeat);
    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");

    let (_, stderr, code) = run_llmaid(&[], "classDiagram\nA ??? B\n");
    assert_eq!(code, 64);
    assert!(
        stderr.contains("line 2") && stderr.contains("relation operator"),
        "{stderr}"
    );
}

#[test]
fn b23_given_er_diagram_then_attributes_cardinalities_and_relation_kinds_are_preserved() {
    let source = "\
erDiagram
  direction LR
  CUSTOMER[Customer Account] {
    string customer_id PK, UK \"public identifier\"
    string region_id FK
  }
  CUSTOMER ||--o{ ORDER : \"places\"
  ORDER }o..|| RECEIPT : generates
";
    let (unicode, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    for label in [
        "Customer Account",
        "customer_id",
        "PK UK",
        "public identifier",
        "region_id",
        "places",
        "generates",
    ] {
        assert!(unicode.contains(label), "missing {label:?}:\n{unicode}");
    }
    assert!(unicode.contains('○') && unicode.contains('<'), "{unicode}");
    assert!(
        !unicode.contains("||--o{") && !unicode.contains("}o..||"),
        "{unicode}"
    );
    let (repeat, _, code) = run_llmaid(&[], source);
    assert_eq!(code, 0);
    assert_eq!(unicode, repeat);
    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");

    let (_, stderr, code) = run_llmaid(&[], "erDiagram\nA ||--o{ B\n");
    assert_eq!(code, 64);
    assert!(
        stderr.contains("line 2") && stderr.contains("expected `:`"),
        "{stderr}"
    );
}

#[test]
fn b24_given_runtime_invariant_failure_then_checked_render_returns_actionable_diagnostics() {
    use llmaid::parse::EdgeKind;
    use llmaid::scene::{Point, Rect, RoutedEdge, Scene, SceneBox};

    let scene = Scene {
        boxes: vec![SceneBox {
            node: 7,
            rect: Rect::new(2, 0, 5, 3),
            lines: vec![],
            shape: Shape::Rect,
            table: None,
        }],
        edges: vec![RoutedEdge {
            edge: 0,
            points: vec![Point::new(0, 1), Point::new(8, 1)],
            rounded: vec![],
            kind: EdgeKind::Solid,
            label: None,
            arrow: None,
        }],
        ..Scene::default()
    };
    let failures = render::render_scene_checked(&scene, Style { ascii: false }).unwrap_err();
    assert!(
        failures.iter().any(|failure| failure.contains("edge 0")
            && failure.contains("non-endpoint box 7")
            && failure.contains("(2,1)")),
        "{failures:#?}"
    );

    let (stdout, stderr, code) = run_llmaid(&[], "flowchart LR\nA --> B\n");
    assert_eq!(code, 0, "{stderr}");
    assert!(!stdout.is_empty());
    assert!(!stderr.contains("invariant failure"), "{stderr}");
}

#[test]
fn b25_given_core_mindmap_then_ordered_hierarchy_is_preserved_and_self_debuggable() {
    let source = "\
mindmap
  root((Agent loop))
    Parse
      Ordered IR
      Clear errors
    Render
";
    let (unicode, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    for label in [
        "Agent loop",
        "Parse",
        "Ordered IR",
        "Clear errors",
        "Render",
    ] {
        assert!(unicode.contains(label), "missing {label:?}:\n{unicode}");
    }
    assert_eq!(run_llmaid(&[], source).0, unicode);

    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");

    let (stdout, stderr, code) = run_llmaid(&[], "mindmap\n  Root\n      Missing parent\n");
    assert_eq!(code, 64);
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("line 3") && stderr.contains("missing parent"),
        "{stderr}"
    );
}

#[test]
fn b26_given_core_timeline_then_chronology_sections_and_events_are_preserved_and_self_debuggable() {
    let source = "\
timeline
  title Release plan
  section Foundation
    Q1 : Design : Prototype
       : Review
    Q2 : Build
  section Delivery
    Q3 : Ship
";
    let (unicode, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    for label in [
        "Release plan",
        "Foundation",
        "Q1",
        "Design",
        "Prototype",
        "Review",
        "Q2",
        "Build",
        "Delivery",
        "Q3",
        "Ship",
    ] {
        assert!(unicode.contains(label), "missing {label:?}:\n{unicode}");
    }
    assert_eq!(run_llmaid(&[], source).0, unicode);

    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");

    let (tight, stderr, code) = run_llmaid(&["--width", "18"], source);
    assert_eq!(code, 0, "{stderr}");
    for label in ["Release", "Foundation", "Prototype", "Delivery", "Ship"] {
        assert!(
            alnum_subsequence(&tight, label),
            "truncated {label:?}:\n{tight}"
        );
    }

    let (audit, stderr, code) = run_llmaid(&["--audit=json"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(audit.contains("\"diagram\":\"timeline\""), "{audit}");
    assert!(
        audit.contains("\"nodes\":8,\"edges\":5,\"ranks\":3"),
        "{audit}"
    );

    let (stdout, stderr, code) = run_llmaid(&[], "timeline\n  : orphan event\n");
    assert_eq!(code, 64);
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("line 2") && stderr.contains("period before event"),
        "{stderr}"
    );
}
