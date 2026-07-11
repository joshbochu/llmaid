use llmaid::parse::Dir;
use llmaid::render;
use llmaid::state::{self, Endpoint};
use llmaid::style::Style;

#[test]
fn parses_headers_declarations_aliases_and_bare_states() {
    let source = "\
%% flat state machine
stateDiagram-v2
  state Idle
  state \"Awaiting input\" as Waiting
  Complete
";
    let diagram = state::parse(source).unwrap();

    assert_eq!(diagram.direction(), Dir::TB);
    let states: Vec<_> = diagram
        .states
        .iter()
        .map(|state| (state.id.as_str(), state.label.as_str()))
        .collect();
    assert_eq!(
        states,
        [
            ("Idle", "Idle"),
            ("Waiting", "Awaiting input"),
            ("Complete", "Complete")
        ]
    );
    assert!(diagram.transitions.is_empty());
}

#[test]
fn transitions_create_implicit_states_in_first_use_order() {
    let diagram = state::parse(
        "stateDiagram\n  [*] --> Ready\n  Ready --> Running: start\n  Running --> Done\n  Done --> [*]\n",
    )
    .unwrap();

    let ids: Vec<_> = diagram
        .states
        .iter()
        .map(|state| state.id.as_str())
        .collect();
    assert_eq!(ids, ["Ready", "Running", "Done"]);
    assert_eq!(diagram.transitions.len(), 4);
    assert_eq!(diagram.transitions[0].from, Endpoint::Marker);
    assert_eq!(diagram.transitions[0].to, Endpoint::State(0));
    assert_eq!(diagram.transitions[1].label.as_deref(), Some("start"));
    assert_eq!(diagram.transitions[3].to, Endpoint::Marker);
}

#[test]
fn parses_all_canonical_directions() {
    for (token, expected) in [
        ("LR", Dir::LR),
        ("RL", Dir::RL),
        ("TB", Dir::TB),
        ("BT", Dir::BT),
    ] {
        let diagram = state::parse(&format!(
            "stateDiagram-v2\n  direction {token}\n  A --> B\n"
        ))
        .unwrap();
        assert_eq!(diagram.direction(), expected);
    }
}

#[test]
fn duplicate_declarations_warn_and_last_label_wins() {
    let diagram = state::parse(
        "stateDiagram-v2\n  state \"First\" as A\n  state \"Second\" as A\n  state A\n",
    )
    .unwrap();

    assert_eq!(diagram.states.len(), 1);
    assert_eq!(diagram.states[0].label, "A");
    assert_eq!(diagram.warnings.len(), 2);
    assert_eq!(diagram.warnings[0].line, 3);
    assert!(diagram.warnings[0].msg.contains("state `A` redeclared"));
    assert_eq!(diagram.warnings[1].line, 4);
}

#[test]
fn dump_is_stable_and_includes_warnings() {
    let diagram = state::parse(
        "stateDiagram\n  direction LR\n  state \"Ready now\" as Ready\n  [*] --> Ready\n  Ready --> Done: finish\n  state Done\n  state Done\n",
    )
    .unwrap();

    assert_eq!(
        state::dump(&diagram),
        concat!(
            "type: state\n",
            "direction: LR\n",
            "states:\n",
            "  Ready \"Ready now\"\n",
            "  Done \"Done\"\n",
            "transitions:\n",
            "  [*] --> Ready\n",
            "  Ready --> Done \"finish\"\n",
            "warnings:\n",
            "  line 7: state `Done` redeclared; last definition wins (was \"Done\")\n",
        )
    );
    assert_eq!(state::labels(&diagram), ["Ready now", "Done", "finish"]);
    assert!(!state::is_empty(&diagram));
    assert!(state::is_empty(&state::parse("stateDiagram-v2\n").unwrap()));
}

#[test]
fn malformed_input_names_the_line_and_expectation() {
    let cases = [
        (
            "flowchart LR\nA --> B\n",
            1,
            "expected `stateDiagram` or `stateDiagram-v2` header",
        ),
        (
            "stateDiagram-v2\n  direction sideways\n",
            2,
            "expected LR, RL, TB or BT",
        ),
        (
            "stateDiagram-v2\n  state\n",
            2,
            "expected a state identifier",
        ),
        (
            "stateDiagram-v2\n  state \"Missing alias\" A\n",
            2,
            "expected `state \"Label\" as ID`",
        ),
        (
            "stateDiagram-v2\n  A -->\n",
            2,
            "expected a transition target",
        ),
        (
            "stateDiagram-v2\n  --> B\n",
            2,
            "expected a transition source",
        ),
        (
            "stateDiagram-v2\n  A --> B:\n",
            2,
            "expected a transition label after `:`",
        ),
        (
            "stateDiagram-v2\n  A -> B\n",
            2,
            "expected a state identifier or `A --> B` transition",
        ),
        (
            "stateDiagram-v2\n  state Composite {\n  }\n",
            2,
            "nested/composite states are not supported",
        ),
    ];

    for (source, line, expected) in cases {
        let error = state::parse(source).unwrap_err();
        assert_eq!(error.line, line, "{source:?}: {error}");
        assert!(error.msg.contains(expected), "{source:?}: {error}");
    }
}

#[test]
fn duplicate_header_and_direction_are_deterministic_warnings() {
    let diagram =
        state::parse("stateDiagram\nstateDiagram-v2\ndirection LR\ndirection BT\nA --> B\n")
            .unwrap();
    assert_eq!(diagram.direction(), Dir::BT);
    assert_eq!(diagram.warnings.len(), 2);
    assert_eq!(diagram.warnings[0].line, 2);
    assert_eq!(diagram.warnings[1].line, 4);
}

#[test]
fn scene_has_distinct_start_end_markers_and_clean_connected_geometry() {
    let diagram = state::parse(
        "stateDiagram-v2\n  [*] --> Ready\n  Ready --> Done: finish\n  Done --> [*]\n",
    )
    .unwrap();
    let scene = state::scene(&diagram, 100);

    assert_eq!(scene.boxes.len(), 4, "two states plus two markers");
    assert_eq!(scene.boxes[0].lines.concat().trim(), "Ready");
    assert_eq!(scene.boxes[1].lines.concat().trim(), "Done");
    assert_eq!(scene.boxes[2].lines.concat().trim(), "*");
    assert_eq!(scene.boxes[3].lines.concat().trim(), "O");
    assert_ne!(scene.boxes[2].node, scene.boxes[3].node);
    assert_eq!(scene.edges.len(), 3);
    assert_eq!(scene.edges[1].label.as_ref().unwrap().text, " finish ");

    let (output, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{output}", failures.join("\n"));
    for label in ["Ready", "Done", "finish", "*", "O"] {
        assert!(output.contains(label), "missing {label:?}:\n{output}");
    }
}

#[test]
fn every_marker_occurrence_gets_a_distinct_scene_node() {
    let diagram =
        state::parse("stateDiagram-v2\n[*] --> A\n[*] --> B\nA --> [*]\nB --> [*]\n").unwrap();
    let scene = state::scene(&diagram, 100);

    assert_eq!(scene.boxes.len(), 6, "two states and four marker uses");
    let marker_nodes: Vec<_> = scene.boxes[2..].iter().map(|item| item.node).collect();
    assert_eq!(marker_nodes.len(), 4);
    for (index, marker) in marker_nodes.iter().enumerate() {
        assert!(
            !marker_nodes[..index].contains(marker),
            "marker node {marker} was reused"
        );
    }
    assert_eq!(scene.boxes[2].lines.concat().trim(), "*");
    assert_eq!(scene.boxes[3].lines.concat().trim(), "*");
    assert_eq!(scene.boxes[4].lines.concat().trim(), "O");
    assert_eq!(scene.boxes[5].lines.concat().trim(), "O");
    assert_eq!(scene.edges.len(), 4);
}

#[test]
fn transition_labels_may_contain_arrow_text() {
    let diagram = state::parse("stateDiagram-v2\nA --> B : emits --> token\n").unwrap();
    assert_eq!(
        diagram.transitions[0].label.as_deref(),
        Some("emits --> token")
    );
}

#[test]
fn state_scene_is_deterministic_directional_and_never_truncates_labels() {
    let long = "This state label is intentionally wider than the requested canvas";
    for (token, direction) in [
        ("LR", Dir::LR),
        ("RL", Dir::RL),
        ("TB", Dir::TB),
        ("BT", Dir::BT),
    ] {
        let diagram = state::parse(&format!(
            "stateDiagram-v2\ndirection {token}\nstate \"{long}\" as A\nA --> B: advance\n"
        ))
        .unwrap();
        assert_eq!(diagram.direction(), direction);
        let first = state::scene(&diagram, 20);
        let second = state::scene(&diagram, 20);
        assert_eq!(first, second);
        let output = render::render_scene(&first, Style { ascii: false });
        assert_eq!(
            first.boxes[0]
                .lines
                .concat()
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>(),
            long.chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>(),
            "wrapped label was not preserved for {token}:\n{output}"
        );

        let a = first.boxes[0].rect.center2();
        let b = first.boxes[1].rect.center2();
        match direction {
            Dir::LR => assert!(a.x < b.x),
            Dir::RL => assert!(a.x > b.x),
            Dir::TB => assert!(a.y < b.y),
            Dir::BT => assert!(a.y > b.y),
        }
    }
}

#[test]
fn state_scene_renders_with_ascii_structure() {
    let diagram = state::parse("stateDiagram-v2\nA --> B: go\n").unwrap();
    let scene = state::scene(&diagram, 80);
    let (output, failures) = render::render_scene_with_checks(&scene, Style { ascii: true });
    assert!(failures.is_empty(), "{}\n{output}", failures.join("\n"));
    assert!(output.contains("+---+"), "{output}");
    assert!(output.contains('v'), "{output}");
    assert!(output.contains("go"), "{output}");
}
