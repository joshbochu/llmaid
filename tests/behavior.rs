//! Behavior-driven tests. One test per contract in BEHAVIORS.md, named
//! `b<N>_given_..._then_...`. Parser behaviors call the library; CLI
//! behaviors run the real binary; B14/B16 scene invariants also run across
//! every golden in `tests/golden.rs`.

use llmaid::layout;
use llmaid::parse::{EdgeKind, Endpoint, FlowEndpointDecoration, Shape, parse};
use llmaid::render;
use llmaid::style::{E, N, S, Style, W};
use llmaid::{diagram, inspect, limits};
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
fn b39_given_declared_subgraph_id_as_an_edge_endpoint_then_the_frame_is_semantic_endpoint() {
    for direction in ["LR", "RL", "TB", "BT"] {
        let source = format!(
            "flowchart {direction}\n\
             Outside -.->|enter| inner\n\
             subgraph outer [Outer]\n\
               subgraph inner [Inner]\n\
                 A --> B\n\
               end\n\
             end\n\
             inner ==>|leave| After\n"
        );
        let graph = parse(&source).unwrap();
        let inner = graph
            .subgraphs
            .iter()
            .position(|group| group.id == "inner")
            .unwrap();
        assert!(
            graph.nodes.iter().all(|node| node.id != "inner"),
            "{direction}: a subgraph endpoint must not create a node"
        );
        assert!(matches!(graph.edges[0].target, Endpoint::Subgraph(group) if group == inner));
        assert!(matches!(graph.edges[2].source, Endpoint::Subgraph(group) if group == inner));
        assert_eq!(graph.edges[0].kind, EdgeKind::Dotted);
        assert_eq!(graph.edges[0].label.as_deref(), Some("enter"));
        assert_eq!(graph.edges[2].kind, EdgeKind::Thick);
        assert_eq!(graph.edges[2].label.as_deref(), Some("leave"));

        let placed = layout::layout(&graph, 100);
        let scene = llmaid::route::route(&graph, &placed);
        let frame = scene
            .groups
            .iter()
            .find(|group| group.subgraph == inner)
            .unwrap()
            .rect;
        for edge in [
            scene.edges.iter().find(|edge| edge.edge == 0).unwrap(),
            scene.edges.iter().find(|edge| edge.edge == 2).unwrap(),
        ] {
            let source = edge.points.first().copied().unwrap();
            let target = edge
                .arrow
                .as_ref()
                .map(|arrow| arrow.toward)
                .or_else(|| edge.points.last().copied())
                .unwrap();
            assert!(
                [source, target].iter().any(|point| {
                    frame.contains(*point)
                        && (point.x == frame.x
                            || point.x == frame.right() - 1
                            || point.y == frame.y
                            || point.y == frame.bottom() - 1)
                }),
                "{direction}: group endpoint must lie on its frame: {edge:?}"
            );
        }
        let semantic = llmaid::diagram::Diagram::Flowchart(graph);
        let inspected = llmaid::inspect::json(&semantic, 100, Style { ascii: false });
        assert!(
            inspected.contains("\"source\":\"group:inner\"")
                && inspected.contains("\"target\":\"group:inner\"")
                && !inspected.contains("node:inner"),
            "{direction}: {inspected}"
        );
        let scene = llmaid::diagram::scene(&semantic, 100);
        assert!(
            !llmaid::quality::evaluate(&semantic, &scene, 100).has_quality_failures(),
            "{direction}"
        );
    }

    let error = parse("flowchart LR\nSource --> empty\nsubgraph empty\nend\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(
        error.msg.contains("subgraph `empty`") && error.msg.contains("member node"),
        "{error}"
    );
}

#[test]
fn b40_given_circle_cross_and_bidirectional_flow_ends_then_every_direction_paints_exact_terminals()
{
    for direction in ["LR", "RL", "TB", "BT"] {
        let source = format!(
            "flowchart {direction}\n\
             A o--o B\n\
             B x--x C\n\
             C <--> D\n"
        );
        let graph = parse(&source).unwrap();
        assert_eq!(
            (
                graph.edges[0].source_decoration,
                graph.edges[0].target_decoration
            ),
            (
                FlowEndpointDecoration::Circle,
                FlowEndpointDecoration::Circle
            )
        );
        assert_eq!(
            (
                graph.edges[1].source_decoration,
                graph.edges[1].target_decoration
            ),
            (FlowEndpointDecoration::Cross, FlowEndpointDecoration::Cross)
        );
        assert_eq!(
            (
                graph.edges[2].source_decoration,
                graph.edges[2].target_decoration
            ),
            (FlowEndpointDecoration::Arrow, FlowEndpointDecoration::Arrow)
        );

        let semantic = llmaid::diagram::Diagram::Flowchart(graph);
        let scene = llmaid::diagram::scene(&semantic, 100);
        let report = llmaid::quality::evaluate(&semantic, &scene, 100);
        let terminal_check = report
            .checks
            .iter()
            .find(|check| check.id == "flow.endpoint_decorations")
            .unwrap();
        assert_eq!(
            terminal_check.status(),
            "pass",
            "{direction}: {terminal_check:#?}"
        );

        let (unicode, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
        assert!(failures.is_empty(), "{direction}: {failures:#?}\n{unicode}");
        assert!(
            unicode.contains('○') && unicode.contains('×'),
            "{direction}:\n{unicode}"
        );
        for arrow in if matches!(direction, "LR" | "RL") {
            ['◀', '▶']
        } else {
            ['▲', '▼']
        } {
            assert!(
                unicode.contains(arrow),
                "{direction}: missing {arrow}:\n{unicode}"
            );
        }

        let (ascii, failures) = render::render_scene_with_checks(&scene, Style { ascii: true });
        assert!(failures.is_empty(), "{direction}: {failures:#?}\n{ascii}");
        assert!(
            ascii.is_ascii() && ascii.contains('o') && ascii.contains('x'),
            "{direction}:\n{ascii}"
        );
        let inspect = llmaid::inspect::json(&semantic, 100, Style { ascii: false });
        assert!(
            inspect.contains("\"kind\":\"circle\"")
                && inspect.contains("\"kind\":\"cross\"")
                && inspect.contains("\"kind\":\"arrow\"")
                && inspect.contains("\"head\":\"filled\""),
            "{direction}: {inspect}"
        );
    }

    let semantic = llmaid::diagram::parse("flowchart LR\nA o--o B\nA x--x B\n").unwrap();
    let scene = llmaid::diagram::scene(&semantic, 100);
    let mut terminals: Vec<_> = scene
        .endpoint_decorations
        .iter()
        .map(|decoration| decoration.at)
        .collect();
    terminals.sort_by_key(|point| (point.y, point.x));
    terminals.dedup();
    assert_eq!(
        terminals.len(),
        4,
        "parallel terminal marks collapsed: {scene:#?}"
    );

    let grouped = llmaid::diagram::parse(
        "flowchart TB\nOutside o--o workers\nsubgraph workers\n  A --> B\nend\nworkers x--x After\n",
    )
    .unwrap();
    let grouped_scene = llmaid::diagram::scene(&grouped, 100);
    assert!(
        !llmaid::quality::evaluate(&grouped, &grouped_scene, 100).has_quality_failures(),
        "decorated group endpoints must retain distinct frame attachments: {grouped_scene:#?}"
    );

    // A decorated fork, merge, or self-loop must reserve a distinct terminal
    // cell for every semantic mark, rather than merely painting the last mark
    // that visits a shared junction.
    for source in [
        "flowchart LR\nA o--o B\nA x--x C\n",
        "flowchart LR\nA o--o B\nC x--x B\n",
        "flowchart TB\nA o--o A\nA x--x B\n",
        "flowchart LR\nA o--o A\nA x--x B\n",
    ] {
        let semantic = llmaid::diagram::parse(source).unwrap();
        let scene = llmaid::diagram::scene(&semantic, 100);
        let mut terminals: Vec<_> = scene
            .endpoint_decorations
            .iter()
            .map(|decoration| decoration.at)
            .collect();
        terminals.sort_by_key(|point| (point.y, point.x));
        terminals.dedup();
        assert_eq!(
            terminals.len(),
            4,
            "terminal collision:\n{source}\n{scene:#?}"
        );
        assert!(
            !llmaid::quality::evaluate(&semantic, &scene, 100).has_failures(),
            "terminal collision:\n{source}\n{scene:#?}"
        );
        let (_, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
        assert!(failures.is_empty(), "{source}: {failures:#?}");
    }

    // Containment in either semantic direction gets a real gutter route when
    // terminal marks consume both ends. Exercise circles, crosses, and
    // arrows through every flow orientation.
    for direction in ["LR", "RL", "TB", "BT"] {
        for relation in ["G x--x A", "A o--o G", "G <--> A", "A <--> G"] {
            let semantic = llmaid::diagram::parse(&format!(
                "flowchart {direction}\nsubgraph G\nA\nend\n{relation}\n"
            ))
            .unwrap();
            let scene = llmaid::diagram::scene(&semantic, 100);
            let edge = &scene.edges[0];
            assert!(
                edge.points.len() >= 2 && edge.points.first() != edge.points.last(),
                "containment route collapsed: {direction} {relation}: {scene:#?}"
            );
            assert!(
                !llmaid::quality::evaluate(&semantic, &scene, 100).has_failures(),
                "containment terminals: {direction} {relation}: {scene:#?}"
            );
            let (_, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
            assert!(
                failures.is_empty(),
                "containment collision: {direction} {relation}: {failures:#?}\n{scene:#?}"
            );
        }
    }

    // `o` and `x` remain ordinary one-character node IDs when they precede
    // inline labels; a true marker still needs a preceding source operand.
    for (source, source_id, kind, source_mark) in [
        (
            "flowchart LR\no -- text; still --> B\nC --> D\n",
            "o",
            EdgeKind::Solid,
            FlowEndpointDecoration::None,
        ),
        (
            "flowchart LR\nx -. text; still .-> B\nC --> D\n",
            "x",
            EdgeKind::Dotted,
            FlowEndpointDecoration::None,
        ),
        (
            "flowchart LR\nx == text; still ==> B\nC --> D\n",
            "x",
            EdgeKind::Thick,
            FlowEndpointDecoration::None,
        ),
        (
            "flowchart LR\nA o-- text; still --> B\nC --> D\n",
            "A",
            EdgeKind::Solid,
            FlowEndpointDecoration::Circle,
        ),
    ] {
        let graph = parse(source).unwrap();
        assert_eq!(graph.edges.len(), 2, "{source}");
        assert_eq!(graph.nodes[0].id, source_id, "{source}");
        assert_eq!(graph.edges[0].kind, kind, "{source}");
        assert_eq!(graph.edges[0].source_decoration, source_mark, "{source}");
        assert_eq!(
            graph.edges[0].label.as_deref(),
            Some("text; still"),
            "{source}"
        );
    }
}

#[test]
fn b37_given_quote_aware_flowchart_text_then_boundaries_and_entities_are_safe() {
    let source = r#"flowchart LR; A["cache[key]; AT&amp;T %% literal"] --> B("call(foo); #x41;"); C[AT&amp;T #quot;] --> D[#35;]; U[&unknown;] --> M[#xzz;] %% trailing comment; Z[ignored]"#;
    let graph = parse(source).unwrap();

    assert_eq!(graph.edges.len(), 3);
    assert_eq!(graph.nodes.len(), 6);
    assert_eq!(graph.nodes[0].label, "cache[key]; AT&T %% literal");
    assert_eq!(graph.nodes[1].label, "call(foo); A");
    assert_eq!(graph.nodes[1].shape, Shape::Rounded);
    assert_eq!(graph.nodes[2].label, "AT&T \"");
    assert_eq!(graph.nodes[3].label, "#");
    assert_eq!(graph.nodes[4].label, "&unknown;");
    assert_eq!(graph.nodes[5].label, "#xzz;");
    assert!(
        graph.nodes.iter().all(|node| node.id != "Z"),
        "a trailing comment must consume later semicolons: {:#?}",
        graph
    );

    let encoded_literal_tag = parse("flowchart LR\nA[&lt;br&gt;]").unwrap();
    assert_eq!(encoded_literal_tag.nodes[0].label, "<br>");

    let subgraph = parse("flowchart LR; subgraph Group [AT&amp;T #35;]; A; end").unwrap();
    assert_eq!(subgraph.subgraphs[0].title, "AT&T #");

    for (entity, control) in [
        ("#9;", "U+0009"),
        ("#x1B;", "U+001B"),
        ("&Tab;", "U+0009"),
        ("&NewLine;", "U+000A"),
    ] {
        let error = parse(&format!("flowchart LR\nA[{entity}]")).unwrap_err();
        assert_eq!(error.line, 2, "{entity}: {error}");
        assert!(
            error.msg.contains("decoded terminal control"),
            "{entity}: {error}"
        );
        assert!(error.msg.contains(control), "{entity}: {error}");
    }
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

#[test]
fn b42_given_a_better_observed_barycenter_order_then_it_is_selected_deterministically() {
    // The fixed legacy sweep sequence finishes with one strict adjacent-rank
    // inversion here; layout's focused counter test records the 1 -> 0 proof.
    let source = "flowchart LR\nA\nB\nC\nD\nE\nF\nA --> E\nA --> F\nB --> C\nC --> D\n";
    let (first, first_stderr, first_code) = run_llmaid(&[], source);
    let (second, second_stderr, second_code) = run_llmaid(&[], source);
    assert_eq!(first_code, 0, "{first_stderr}");
    assert_eq!(second_code, 0, "{second_stderr}");
    assert_eq!(first, second);
    for label in ["A", "B", "C", "D", "E", "F"] {
        assert!(first.contains(label), "missing {label}:\n{first}");
    }

    let diagram = diagram::parse(source).unwrap();
    let scene = diagram::scene(&diagram, 100);
    let quality = llmaid::quality::evaluate(&diagram, &scene, 100);
    let crossings = quality
        .checks
        .iter()
        .find(|check| check.id == "flow.edge_crossings")
        .unwrap();
    assert_eq!(crossings.status(), "pass", "{crossings:#?}");
}

#[test]
fn b41_given_resource_boundaries_then_refusals_are_bounded_actionable_and_inspectable() {
    let source_limit = "x".repeat(limits::MAX_SOURCE_BYTES + 1);
    let error = diagram::parse(&source_limit).unwrap_err();
    assert_eq!(error.line, 1);
    assert!(
        error.msg.contains("source bytes")
            && error
                .msg
                .contains(&(limits::MAX_SOURCE_BYTES + 1).to_string())
            && error.msg.contains(&limits::MAX_SOURCE_BYTES.to_string())
            && error.msg.contains("split or reduce"),
        "{error}"
    );

    let mut broad = String::from("flowchart LR\n");
    for node in 0..=limits::MAX_SEMANTIC_ELEMENTS {
        broad.push_str(&format!("N{node}\n"));
    }
    let error = diagram::parse(&broad).unwrap_err();
    assert!(
        error.msg.contains("semantic elements")
            && error
                .msg
                .contains(&(limits::MAX_SEMANTIC_ELEMENTS + 1).to_string()),
        "{error}"
    );

    let mut deep = String::from("mindmap\n");
    for depth in 0..=limits::MAX_NESTING_DEPTH {
        deep.push_str(&" ".repeat((depth + 1) * 2));
        deep.push_str(&format!("N{depth}\n"));
    }
    let error = diagram::parse(&deep).unwrap_err();
    assert!(
        error.msg.contains("nesting depth")
            && error
                .msg
                .contains(&(limits::MAX_NESTING_DEPTH + 1).to_string()),
        "{error}"
    );

    use llmaid::scene::{Rect, Scene, SceneBox};
    let oversized_scene = Scene {
        boxes: vec![SceneBox {
            node: 0,
            rect: Rect::new(0, 0, (limits::MAX_CANVAS_DIMENSION + 1) as i32, 3),
            lines: vec![],
            shape: Shape::Rect,
            table: None,
        }],
        ..Scene::default()
    };
    let limit = render::try_render_scene(&oversized_scene, Style { ascii: false }).unwrap_err();
    assert_eq!(limit.resource, "canvas width");
    assert_eq!(limit.observed, limits::MAX_CANVAS_DIMENSION + 1);
    assert_eq!(limit.limit, limits::MAX_CANVAS_DIMENSION);
    assert!(render::render_scene(&oversized_scene, Style { ascii: false }).is_empty());
    assert_eq!(
        limits::validate_canvas(usize::MAX, 2).unwrap_err().resource,
        "canvas cells"
    );

    let giant_label = "x".repeat(limits::MAX_CANVAS_DIMENSION);
    let wide_source = format!("flowchart LR\nA[{giant_label}]\n");
    let wide = diagram::parse(&wide_source).unwrap();
    let first = inspect::json(&wide, 100, Style { ascii: false });
    let second = inspect::json(&wide, 100, Style { ascii: false });
    assert_eq!(first, second);
    assert!(first.contains("\"scene.integrity\""), "{first}");
    assert!(first.contains("\"resource\":\"canvas width\""), "{first}");
    assert!(
        first.contains("\"canvas\":{\"width\":0,\"height\":0,\"rows\":[]}"),
        "{first}"
    );

    let over_width = (limits::MAX_TARGET_WIDTH + 1).to_string();
    let (stdout, stderr, code) = run_llmaid(&["--width", &over_width], "");
    assert_eq!(code, 64, "{stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.contains("target width") && stderr.contains("use a smaller --width"));

    let (stdout, stderr, code) = run_llmaid(&[], &source_limit);
    assert_eq!(code, 64, "{stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.contains("source bytes") && stderr.contains("split or reduce"));

    let (stdout, stderr, code) = run_llmaid(&[], &wide_source);
    assert_eq!(code, 64, "{stderr}");
    assert!(stdout.is_empty());
    assert!(stderr.contains("canvas width") && stderr.contains("diagram not written"));

    let (json, stderr, code) = run_llmaid(&["--inspect=json"], &wide_source);
    assert_eq!(code, 0, "{stderr}");
    assert!(json.starts_with("{\"schema\":\"llmaid.inspect.v1\""));
    assert!(json.contains("\"resource\":\"canvas width\""));
    assert!(json.contains("\"canvas\":{\"width\":0,\"height\":0,\"rows\":[]}"));
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
        .unwrap_or_else(|error| {
            assert_eq!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe,
                "failed to write llmaid stdin"
            );
        });
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
  c -->|eval result| d[Vec<DeveloperTool> output]
";
    let (stdout, stderr, code) = run_llmaid(&["--width", "36"], src);
    assert_eq!(code, 0, "must not fail on overflow: {stderr}");
    // Edge labels and whitespace-free developer tokens stay intact. Multiword
    // node labels may wrap, but only at their word boundaries.
    for needle in ["scan tokens", "parse tree", "eval result"] {
        assert!(
            stdout.contains(needle),
            "edge label `{needle}` missing under width pressure:\n{stdout}"
        );
    }
    assert!(
        stdout.contains("Vec<DeveloperTool>"),
        "identifier was split under width pressure:\n{stdout}"
    );
    for needle in [
        "sourcealpha",
        "VecofToken",
        "ExprASTnode",
        "VecDeveloperTooloutput",
    ] {
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
    for want in needle.chars().filter(|c| c.is_alphanumeric()) {
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
  cy[(cylinder)] --> di{diamond} --> hx{{hexagon}}
";
    let (stdout, stderr, code) = run_llmaid(&[], src);
    assert_eq!(code, 0, "{stderr}");
    // All labels present (never truncated / lost to shape drawing).
    for label in [
        "rect", "rounded", "stadium", "circle", "cylinder", "diamond", "hexagon",
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
        assert!(
            alnum_subsequence(&tight, text),
            "truncated {text:?}:\n{tight}"
        );
    }

    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");
    assert!(ascii.contains(">:"), "ASCII call cue missing:\n{ascii}");
    assert!(ascii.contains(":<--"), "ASCII return cue missing:\n{ascii}");
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
        assert!(
            alnum_subsequence(&tight, text),
            "truncated {text:?}:\n{tight}"
        );
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
    let (repeat, _, code) = run_llmaid(&[], source);
    assert_eq!(code, 0);
    assert_eq!(unicode, repeat);
    assert_eq!(unicode.matches("else rejected").count(), 1);
    assert!(
        unicode
            .lines()
            .any(|line| line.contains("├─ else rejected ─") && line.contains('┤')),
        "else must render as a labeled branch separator, not a nested frame:\n{unicode}"
    );

    let (tight, stderr, code) = run_llmaid(&["--width", "20"], source);
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
        assert!(
            alnum_subsequence(&tight, label),
            "truncated {label:?}:\n{tight}"
        );
    }
    assert!(
        tight
            .lines()
            .any(|line| line.contains("├─ else rejected ─")),
        "tight layout lost its branch separator:\n{tight}"
    );

    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");
    assert!(
        ascii
            .lines()
            .any(|line| line.contains("+- else rejected -")),
        "{ascii}"
    );
    assert!(ascii.contains("retry"));
}

#[test]
fn b36_given_final_sequence_fragment_then_lifelines_end_on_its_bottom_border() {
    let source = "\
sequenceDiagram
  participant App
  participant API
  loop Retry
    App->>API: request
  end
";
    let (unicode, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    let last = unicode.lines().last().unwrap();
    assert_eq!(last.matches('┴').count(), 2, "{unicode}");
    assert!(!last.contains('┼'), "{unicode}");
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
        stderr.contains("<stdin>:2:") && stderr.contains("flat state"),
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
        stderr.contains("<stdin>:2:") && stderr.contains("relation operator"),
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
        stderr.contains("<stdin>:2:") && stderr.contains("expected `:`"),
        "{stderr}"
    );
}

#[test]
fn b35_given_converging_vertical_er_relationships_then_terminal_lanes_stay_distinct() {
    let source = "\
erDiagram
  direction TB
  MEMBER ||--o{ LOAN : borrows
  BOOK ||--o{ LOAN : appears_in
";
    let (unicode, stderr, code) = run_llmaid(&[], source);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(unicode.matches("borrows").count(), 1, "{unicode}");
    assert_eq!(unicode.matches("appears_in").count(), 1, "{unicode}");
    assert!(
        unicode.lines().any(|line| line.matches('∨').count() == 2),
        "converging many-cardinalities need distinct vertical lanes:\n{unicode}"
    );
    assert!(!unicode.contains("○ <"), "{unicode}");

    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.is_ascii(), "{ascii}");
    assert!(ascii.lines().any(|line| line.matches('v').count() == 2));
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
    let failures = match render::render_scene_checked(&scene, Style { ascii: false }).unwrap_err() {
        render::CheckedRenderError::Invariants(failures) => failures,
        render::CheckedRenderError::Resource(limit) => panic!("unexpected resource limit: {limit}"),
    };
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
        stderr.contains("<stdin>:3:") && stderr.contains("missing parent"),
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
        stderr.contains("<stdin>:2:") && stderr.contains("period before event"),
        "{stderr}"
    );
}

#[test]
fn b27_given_known_unsupported_document_then_error_names_type_and_header_line() {
    for (source, line, header) in [
        ("%% generated\n\ngitGraph\n  commit\n", 3, "gitGraph"),
        ("pie title Adoption\n  \"Dogs\" : 4\n", 1, "pie"),
        ("gantt\n  title Release\n", 1, "gantt"),
    ] {
        let (stdout, stderr, code) = run_llmaid(&[], source);
        assert_eq!(code, 64, "for {header}: {stderr}");
        assert_eq!(stdout, "", "for {header}");
        assert!(
            stderr.contains(&format!("<stdin>:{line}: error:"))
                && stderr.contains(&format!("diagram type `{header}` is not supported"))
                && stderr.contains(&format!("{line} | {header}")),
            "for {header}:\n{stderr}"
        );
    }

    // A known type name remains legal as a node id in an intentionally
    // headerless flowchart when it is used as a flowchart statement.
    let (stdout, stderr, code) = run_llmaid(&[], "gitGraph --> result\n");
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("gitGraph") && stdout.contains("result"),
        "{stdout}"
    );
}

#[test]
fn b38_given_supported_type_text_then_only_an_exact_header_dispatches() {
    // A supported document name only dispatches when it is the complete
    // engine header. A semicolon makes this a conservative headerless
    // flowchart, matching the flowchart parser's statement grammar.
    for name in [
        "sequenceDiagram",
        "stateDiagram",
        "classDiagram",
        "erDiagram",
        "mindmap",
        "timeline",
    ] {
        let source = format!("{name}; A --> B\n");
        let (stdout, stderr, code) = run_llmaid(&[], &source);
        assert_eq!(code, 0, "{name}: {stderr}");
        assert!(
            stdout.contains(name) && stdout.contains('A') && stdout.contains('B'),
            "{name} should remain a headerless flowchart:\n{stdout}"
        );
    }
    for source in [
        "timeline --> A\n",
        "sequenceDiagram --> A\n",
        "timeline & A --> B\n",
    ] {
        let (stdout, stderr, code) = run_llmaid(&[], source);
        assert_eq!(code, 0, "{source}: {stderr}");
        assert!(
            stdout.contains("timeline") || stdout.contains("sequenceDiagram"),
            "{source} should remain a flowchart:\n{stdout}"
        );
        assert!(
            stdout.contains('A'),
            "{source} should retain its flowchart target:\n{stdout}"
        );
    }
}

#[test]
fn b28_given_cli_source_or_width_mistakes_then_usage_fails_before_reading() {
    let source = "flowchart LR\nA --> B\n";
    let (stdout, stderr, code) = run_llmaid(&["--width", "0"], source);
    assert_eq!(code, 64);
    assert_eq!(stdout, "");
    assert!(stderr.contains("--width must be at least 1"), "{stderr}");

    let (stdout, stderr, code) = run_llmaid(&["-"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains('A') && stdout.contains('B'), "{stdout}");

    for args in [["-", "other.mmd"], ["other.mmd", "-"], ["-", "-"]] {
        let (stdout, stderr, code) = run_llmaid(&args, "");
        assert_eq!(code, 64, "for {args:?}: {stderr}");
        assert_eq!(stdout, "", "for {args:?}");
        assert!(
            stderr.contains("more than one input source"),
            "for {args:?}: {stderr}"
        );
    }
}

#[test]
fn b29_given_parse_error_then_diagnostic_names_source_and_shows_excerpt() {
    let path = std::env::temp_dir().join(format!(
        "llmaid-behavior-diagnostic-{}.mmd",
        std::process::id()
    ));
    std::fs::write(&path, "flowchart LR\n  A[unclosed\n").unwrap();

    let (stdout, stderr, code) = run_llmaid(&[path.to_str().unwrap()], "");
    let _ = std::fs::remove_file(&path);

    assert_eq!(code, 64);
    assert_eq!(stdout, "");
    assert!(
        stderr.contains(&format!("{}:2: error:", path.display())),
        "{stderr}"
    );
    assert!(stderr.contains("expected closing `]`"), "{stderr}");
    assert!(stderr.contains("2 |   A[unclosed"), "{stderr}");
}

#[test]
fn b30_given_downstream_closes_stdout_then_broken_pipe_exits_successfully() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_llmaid"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Closing the read end before llmaid writes deterministically produces
    // EPIPE without relying on `head` timing or pipe-buffer capacity.
    drop(child.stdout.take().unwrap());
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"flowchart LR\nA --> B\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn b31_given_ascii_mode_then_structure_is_ascii_and_edge_styles_stay_distinct() {
    let style = Style { ascii: true };
    assert_eq!(style.line(N | S, false, EdgeKind::Solid), '|');
    assert_eq!(style.line(N | S, false, EdgeKind::Dotted), ':');
    assert_eq!(style.line(N | S, false, EdgeKind::Thick), '#');
    assert_eq!(style.line(E | W, false, EdgeKind::Solid), '-');
    assert_eq!(style.line(E | W, false, EdgeKind::Dotted), '.');
    assert_eq!(style.line(E | W, false, EdgeKind::Thick), '=');

    let source = "flowchart LR\nA[界] --> B\nB -.-> C\nC ==> D\n";
    let (ascii, stderr, code) = run_llmaid(&["--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        ascii.contains('界'),
        "labels must remain unchanged:\n{ascii}"
    );
    assert!(ascii.contains('.'), "dotted edge style missing:\n{ascii}");
    assert!(ascii.contains('='), "thick edge style missing:\n{ascii}");

    let (help, stderr, code) = run_llmaid(&["--help"], "");
    assert_eq!(code, 0, "{stderr}");
    assert!(
        help.contains("ASCII structural glyphs") && help.contains("label text is preserved"),
        "{help}"
    );
}

#[test]
fn b32_given_terminal_text_then_graphemes_breaks_and_controls_are_safe() {
    let safe_documents = [
        "flowchart LR\nA[cafe\u{301} .\u{301}] --> B[👩\u{200d}💻 Wait…]\n",
        "sequenceDiagram\nparticipant A as cafe\u{301} .\u{301}\nparticipant B as 👩\u{200d}💻 Wait…\nA->>B: ready\n",
        "stateDiagram-v2\ndirection LR\nstate \"cafe\u{301} .\u{301}\" as A\nstate \"👩\u{200d}💻 Wait…\" as B\nA --> B\n",
        "classDiagram\nclass A {\n  +cafe\u{301}().\u{301}\n}\nA --> B : 👩\u{200d}💻 Wait…\n",
        "erDiagram\nA[cafe\u{301} .\u{301}] {\n  string id PK \"👩\u{200d}💻 Wait…\"\n}\n",
        "mindmap\n  cafe\u{301} .\u{301}\n    👩\u{200d}💻 Wait…\n",
        "timeline\n  cafe\u{301} .\u{301} : 👩\u{200d}💻 Wait…\n",
    ];
    for source in safe_documents {
        let (stdout, stderr, code) = run_llmaid(&[], source);
        assert_eq!(code, 0, "{source:?}\n{stderr}");
        assert!(stdout.contains("cafe\u{301}"), "{source:?}\n{stdout}");
        assert!(stdout.contains(".\u{301}"), "{source:?}\n{stdout}");
        assert!(stdout.contains("👩\u{200d}💻"), "{source:?}\n{stdout}");
        assert!(stdout.contains("Wait…"), "{source:?}\n{stdout}");
        assert!(!stderr.contains("invariant failure"), "{stderr}");
    }

    let (multiline, stderr, code) = run_llmaid(&[], "flowchart LR\nA -->|one<br/>two| B\n");
    assert_eq!(code, 0, "{stderr}");
    let rows: Vec<&str> = multiline.lines().collect();
    assert!(rows.iter().any(|row| row.contains("one")), "{multiline}");
    assert!(rows.iter().any(|row| row.contains("two")), "{multiline}");
    assert_ne!(
        rows.iter().position(|row| row.contains("one")),
        rows.iter().position(|row| row.contains("two")),
        "{multiline}"
    );

    let unsafe_documents = [
        "flowchart LR\nA[bad\u{1b}[31m]\n",
        "sequenceDiagram\nA->>B: bad\u{1b}[31m\n",
        "stateDiagram-v2\nstate \"bad\u{1b}\" as A\n",
        "classDiagram\nclass A\u{1b}\n",
        "erDiagram\nA[bad\u{1b}]\n",
        "mindmap\n  bad\u{1b}\n",
        "timeline\n  bad\u{1b} : event\n",
    ];
    for source in unsafe_documents {
        let error = llmaid::diagram::parse(source).unwrap_err();
        assert_eq!(error.line, 2, "{source:?}: {error}");
        assert!(error.msg.contains("terminal control U+001B"), "{error}");
        assert!(!error.msg.contains('\u{1b}'), "{error:?}");
    }

    for (control, expected) in [
        ('\u{1b}', "U+001B"),
        ('\t', "spaces, not tabs"),
        ('\r', "U+000D"),
        ('\0', "U+0000"),
    ] {
        let source = format!("flowchart LR\nA[bad{control}text]\n");
        let (stdout, stderr, code) = run_llmaid(&[], &source);
        assert_eq!(code, 64, "{stderr}");
        assert_eq!(stdout, "");
        assert!(stderr.contains(expected), "{stderr:?}");
        assert!(!stderr.contains(control), "{stderr:?}");
    }

    for invisible in ['\u{301}', '\u{fe0f}', '\u{200b}'] {
        let source = format!("flowchart LR\nA[{invisible}]\n");
        let (stdout, stderr, code) = run_llmaid(&[], &source);
        assert_eq!(code, 64, "{stderr}");
        assert_eq!(stdout, "");
        assert!(
            stderr.contains("<stdin>:2: error:") && stderr.contains("zero-column Unicode grapheme"),
            "{stderr:?}"
        );
        assert!(!stderr.contains(invisible), "{stderr:?}");
    }

    let parsed_zero_column_documents = [
        "flowchart LR\nA[\u{301}]\n",
        "flowchart LR\nsubgraph S [\u{301}]\nA\nend\n",
        "flowchart LR\nA -->|\u{301}| B\n",
        "sequenceDiagram\nA->>B: \u{301}\n",
        "stateDiagram-v2\nstate \"\u{301}\" as A\n",
        "classDiagram\nA --> B : \u{301}\n",
        "erDiagram\nA[\u{301}]\n",
        "mindmap\n  \u{301}\n",
        "timeline\n  period : \u{301}\n",
    ];
    for source in parsed_zero_column_documents {
        let error = llmaid::diagram::parse(source).unwrap_err();
        assert_eq!(error.line, 2, "{source:?}: {error}");
        assert!(
            error.msg.contains("zero-column Unicode grapheme"),
            "{error}"
        );
    }

    let (stdout, stderr, code) = run_llmaid(&["--audit=json"], "flowchart LR\nA[bad\u{1b}text]\n");
    assert_eq!(code, 64, "{stderr}");
    assert_eq!(stdout, "");
    assert!(!stderr.contains('\u{1b}'), "{stderr:?}");

    let (stdout, stderr, code) = run_llmaid(&[], "flowchart LR\r\nA --> B\r\n");
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains('A') && stdout.contains('B'), "{stdout}");
}

#[test]
fn b33_given_audit_quality_or_fit_residual_then_v1_names_an_exact_witness() {
    let source = "flowchart LR\nA[averylongunbreakabletoken]\n";
    let (first, stderr, code) = run_llmaid(&["--audit=json", "--width", "8"], source);

    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stderr, "");
    assert!(
        first.starts_with("{\"schema\":\"llmaid.audit.v1\""),
        "{first}"
    );
    assert!(first.contains(concat!(
        "\"name\":\"width_target_exceeded\",",
        "\"message\":\"rendered width 29 exceeds target width 8 by 21 columns\",",
        "\"witness\":{\"target_width\":8,\"rendered_width\":29,",
        "\"overflow_columns\":21}"
    )));
    assert!(!first.contains("\"score\""), "{first}");

    let (second, stderr, code) = run_llmaid(&["--audit=json", "--width", "8"], source);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(first, second, "named audit diagnostics must be byte-stable");
}

#[test]
fn b34_given_inspect_json_then_semantic_geometry_checks_and_canvas_are_stable() {
    let source = "flowchart LR\nA[Input] --> B[Output]\n";
    let (first, stderr, code) = run_llmaid(&["--inspect=json"], source);

    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stderr, "");
    assert!(
        first.starts_with("{\"schema\":\"llmaid.inspect.v1\""),
        "{first}"
    );
    assert!(first.contains("\"element\":\"node:A\""), "{first}");
    assert!(
        first.contains("\"source\":\"node:A\",\"target\":\"node:B\""),
        "{first}"
    );
    assert!(
        first.contains(concat!(
            "\"id\":\"flow.mono_centerline\",",
            "\"class\":\"preference\",\"status\":\"pass\""
        )),
        "{first}"
    );
    assert!(first.contains("\"quality_failed_checks\":0"), "{first}");
    assert!(first.contains("\"canvas\":{"), "{first}");
    assert!(first.contains("\"rows\":[\"╭"), "{first}");

    let (second, stderr, code) = run_llmaid(&["--inspect=json"], source);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(first, second, "inspection JSON must be byte-deterministic");

    let (ascii, stderr, code) = run_llmaid(&["--inspect=json", "--ascii"], source);
    assert_eq!(code, 0, "{stderr}");
    assert!(ascii.contains("\"style\":\"ascii\""), "{ascii}");
    assert!(!ascii.contains('╭'), "{ascii}");

    let (stdout, stderr, code) = run_llmaid(&["--audit=json", "--inspect=json"], source);
    assert_eq!(code, 64, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("mutually exclusive"), "{stderr}");

    let (stdout, stderr, code) = run_llmaid(&["--inspect=yaml"], source);
    assert_eq!(code, 64, "{stderr}");
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("--inspect supports only `json`"),
        "{stderr}"
    );

    let (empty, stderr, code) = run_llmaid(&["--inspect=json"], "");
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("nothing to render"), "{stderr}");
    assert!(empty.contains("\"bounds\":{\"x\":0,\"y\":0,\"width\":0,\"height\":0}"));
    assert!(empty.contains("\"canvas\":{\"width\":0,\"height\":0,\"rows\":[]}"));
}
