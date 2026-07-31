use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], stdin: &str) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_llmaid"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
        output.status.code().unwrap(),
    )
}

#[test]
fn audit_json_schema_is_byte_stable_for_a_simple_flowchart() {
    let source = "flowchart LR\nA --> B\n";
    let (stdout, stderr, code) = run(&["--audit=json"], source);

    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stderr, "");
    assert_eq!(
        stdout,
        concat!(
            "{\"schema\":\"llmaid.audit.v1\",\"diagram\":\"flowchart\",",
            "\"bounds\":{\"width\":15,\"height\":3,\"area\":45},",
            "\"elements\":{\"nodes\":2,\"edges\":1,\"ranks\":2},",
            "\"violations\":[],",
            "\"metrics\":{\"rank_axis_residual2\":0,\"mono_centerline_residual2\":0,",
            "\"fork_barycenter_residual2\":0,\"merge_barycenter_residual2\":0,",
            "\"diamond_motifs\":0,\"diamond_mirror_residual2\":0,",
            "\"crossing_cells\":0,\"bends\":0,\"wire_length\":5}}\n"
        )
    );

    let second = run(&["--audit=json"], source);
    assert_eq!(second.0, stdout, "audit JSON must be byte-deterministic");
}

#[test]
fn audit_json_uses_width_and_keeps_warnings_on_stderr() {
    let source = "flowchart LR\nclassDef default fill:#fff\nA[long label] --> B\n";
    let (stdout, stderr, code) = run(&["--audit=json", "--width", "12"], source);

    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with("{\"schema\":\"llmaid.audit.v1\""));
    assert!(stdout.ends_with("}\n"));
    assert!(
        !stdout.contains('╭'),
        "audit mode must not mix in a diagram"
    );
    assert!(stdout.contains(concat!(
        "\"name\":\"width_target_exceeded\",",
        "\"message\":\"rendered width 17 exceeds target width 12 by 5 columns\",",
        "\"witness\":{\"target_width\":12,\"rendered_width\":17,",
        "\"overflow_columns\":5}"
    )));
    assert!(stderr.contains("<stdin>:2: warning:"), "{stderr}");
}

#[test]
fn audit_json_reports_empty_input_as_a_valid_document() {
    let (stdout, stderr, code) = run(&["--audit=json"], "");

    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("nothing to render"), "{stderr}");
    assert!(stdout.contains("\"bounds\":{\"width\":0,\"height\":0,\"area\":0}"));
    assert!(stdout.contains("\"elements\":{\"nodes\":0,\"edges\":0,\"ranks\":0}"));
}

#[test]
fn audit_flag_rejects_unknown_formats() {
    let (stdout, stderr, code) = run(&["--audit=yaml"], "flowchart LR\nA --> B\n");
    assert_eq!(code, 64);
    assert_eq!(stdout, "");
    assert!(stderr.contains("--audit supports only `json`"), "{stderr}");
}

#[test]
fn help_documents_machine_readable_audit_mode() {
    let (stdout, stderr, code) = run(&["--help"], "");
    assert_eq!(code, 0);
    assert_eq!(stderr, "");
    assert!(stdout.contains("--audit=json"), "{stdout}");
}

#[test]
fn violation_has_a_stable_name_and_exact_normalized_witness() {
    use llmaid::layout;
    use llmaid::parse;
    use llmaid::scene::{EdgeKind, Point, Rect, RoutedEdge, Scene, SceneBox, Shape};

    let graph = parse::parse("flowchart LR\nA --> B\nC\n").unwrap();
    let placed = layout::layout(&graph, 100);
    let scene = Scene {
        boxes: vec![
            SceneBox {
                node: 0,
                rect: Rect::new(-2, 0, 3, 3),
                lines: vec!["A".into()],
                shape: Shape::Rect,
                table: None,
            },
            SceneBox {
                node: 1,
                rect: Rect::new(8, 0, 3, 3),
                lines: vec!["B".into()],
                shape: Shape::Rect,
                table: None,
            },
            SceneBox {
                node: 2,
                rect: Rect::new(3, 0, 3, 3),
                lines: vec!["C".into()],
                shape: Shape::Rect,
                table: None,
            },
        ],
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

    let json = llmaid::audit::flowchart_json(&graph, &placed, &scene);
    assert!(
        json.contains(concat!(
            "\"violations\":[{\"name\":\"edge_intersects_non_endpoint_box\",",
            "\"message\":\"edge 0 intersects non-endpoint box 2 at (5,1)\",",
            "\"witness\":{\"edge\":0,\"node\":2,\"at\":{\"x\":5,\"y\":1}}}"
        )),
        "{json}"
    );
}

#[test]
fn topology_residual_has_a_stable_name_and_exact_metric_witness() {
    use llmaid::{layout, parse, route};

    let graph = parse::parse("flowchart LR\nA --> B\n").unwrap();
    let mut placed = layout::layout(&graph, 100);
    let scene = route::route(&graph, &placed);

    // Deliberately perturb only the measured target center. This proves the
    // audit names an exact topology relationship without blessing any
    // currently accepted gallery imperfection as a permanent fixture.
    placed.boxes[1].c += 2;
    let json = llmaid::audit::flowchart_json(&graph, &placed, &scene);

    assert!(json.contains(concat!(
        "\"name\":\"mono_centerline_misalignment\",",
        "\"message\":\"mono_centerline_residual2 has 4 avoidable doubled-cell units\",",
        "\"witness\":{\"metric\":\"mono_centerline_residual2\",\"value\":4}"
    )));
    assert_eq!(
        json.matches("\"name\":\"mono_centerline_misalignment\"")
            .count(),
        1,
        "{json}"
    );
}

#[test]
fn sequence_diagrams_have_a_coherent_typed_audit() {
    let source = "sequenceDiagram\nparticipant A\nparticipant B\nA->>B: hello\n";
    let (stdout, stderr, code) = run(&["--audit=json"], source);

    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("\"diagram\":\"sequence\""), "{stdout}");
    assert!(stdout.contains("\"elements\":{\"nodes\":2,\"edges\":1,\"ranks\":0}"));
    assert!(stdout.contains("\"violations\":[]"), "{stdout}");
    assert!(stdout.contains("\"metrics\":null"), "{stdout}");
}

#[test]
fn design_diagrams_have_coherent_typed_audits() {
    let cases = [
        ("state", "stateDiagram\nA --> B\n", 2, 1),
        ("class", "classDiagram\nA --> B\n", 2, 1),
        ("er", "erDiagram\nA ||--o{ B : owns\n", 2, 1),
    ];
    for (diagram, source, nodes, edges) in cases {
        let (first, stderr, code) = run(&["--audit=json"], source);
        assert_eq!(code, 0, "{diagram}: {stderr}");
        assert!(
            first.contains(&format!("\"diagram\":\"{diagram}\"")),
            "{first}"
        );
        assert!(
            first.contains(&format!(
                "\"elements\":{{\"nodes\":{nodes},\"edges\":{edges},\"ranks\":0}}"
            )),
            "{first}"
        );
        assert!(first.contains("\"violations\":[]"), "{first}");
        assert!(first.contains("\"metrics\":null"), "{first}");
        assert_eq!(run(&["--audit=json"], source).0, first);
    }
}

#[test]
fn mindmaps_have_a_stable_typed_audit_with_tree_levels() {
    let source = "mindmap\n  Root\n    A\n      A1\n    B\n";
    let (first, stderr, code) = run(&["--audit=json"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(first.contains("\"diagram\":\"mindmap\""), "{first}");
    assert!(
        first.contains("\"elements\":{\"nodes\":4,\"edges\":3,\"ranks\":3}"),
        "{first}"
    );
    assert!(first.contains("\"violations\":[]"), "{first}");
    assert!(first.contains("\"metrics\":null"), "{first}");
    assert_eq!(run(&["--audit=json"], source).0, first);
}

#[test]
fn timelines_have_stable_semantic_counts_and_chronological_ranks() {
    let source = "timeline\n  section Build\n  Q1 : Design : Review\n  Q2 : Ship\n";
    let (first, stderr, code) = run(&["--audit=json"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(first.contains("\"diagram\":\"timeline\""), "{first}");
    assert!(
        first.contains("\"elements\":{\"nodes\":5,\"edges\":3,\"ranks\":2}"),
        "{first}"
    );
    assert!(first.contains("\"violations\":[]"), "{first}");
    assert!(first.contains("\"metrics\":null"), "{first}");
    assert_eq!(run(&["--audit=json"], source).0, first);
}
