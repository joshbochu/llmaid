use llmaid::diagram::{self, Diagram};
use llmaid::render;
use llmaid::sequence::MessageKind;
use llmaid::style::Style;

const CORE: &str = "\
sequenceDiagram
  participant Client
  actor API as Application API
  Client->>API: request
  API-->>Client: response
";

#[test]
fn dispatches_sequence_without_changing_flowchart_dispatch() {
    let sequence = diagram::parse(CORE).unwrap();
    assert!(matches!(sequence, Diagram::Sequence(_)));

    let flow = diagram::parse("flowchart LR\nA --> B\n").unwrap();
    assert!(matches!(flow, Diagram::Flowchart(_)));
}

#[test]
fn parses_declared_and_implicit_participants_in_stable_order() {
    let Diagram::Sequence(sequence) =
        diagram::parse("sequenceDiagram\nparticipant B as Backend\nA->>B: call\nB-->>C: result\n")
            .unwrap()
    else {
        panic!("expected sequence diagram");
    };

    let participants: Vec<(&str, &str)> = sequence
        .participants
        .iter()
        .map(|participant| (participant.id.as_str(), participant.label.as_str()))
        .collect();
    assert_eq!(participants, [("B", "Backend"), ("A", "A"), ("C", "C")]);
    assert_eq!(sequence.messages.len(), 2);
    assert_eq!(sequence.messages[0].kind, MessageKind::Solid);
    assert_eq!(sequence.messages[0].label, "call");
    assert_eq!(sequence.messages[1].kind, MessageKind::Dashed);
    assert_eq!(sequence.messages[1].label, "result");
}

#[test]
fn malformed_sequence_statement_names_line_and_expectation() {
    let error = diagram::parse("sequenceDiagram\nA->>B request\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(error.msg.contains("expected `:`"), "{}", error.msg);

    let error = diagram::parse("sequenceDiagram\nA-xB: nope\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(
        error.msg.contains("expected a message arrow"),
        "{}",
        error.msg
    );
}

#[test]
fn core_sequence_scene_has_headers_lifelines_messages_and_clean_invariants() {
    let diagram = diagram::parse(CORE).unwrap();
    let scene = diagram::scene(&diagram, 100);
    assert_eq!(scene.boxes.len(), 2, "participant headers");
    assert_eq!(scene.paths.len(), 2, "participant lifelines");
    assert_eq!(scene.edges.len(), 2, "messages");

    let (output, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{output}", failures.join("\n"));
    for text in ["Client", "Application API", "request", "response"] {
        assert!(output.contains(text), "missing {text:?}:\n{output}");
    }
    assert!(output.contains('▶'), "message arrows missing:\n{output}");
    assert!(output.contains('┊'), "dotted lifelines missing:\n{output}");
    assert!(output.contains("▶┊"), "call endpoint unclear:\n{output}");
    assert!(output.contains("┊←"), "return endpoint unclear:\n{output}");
    assert!(
        !output.contains('╌'),
        "return line is over-styled:\n{output}"
    );
}
