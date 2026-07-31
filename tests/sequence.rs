use llmaid::diagram::{self, Diagram};
use llmaid::render;
use llmaid::sequence::{ActivationKind, MessageKind, NotePosition, SequenceEvent};
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
    let messages: Vec<_> = sequence
        .events
        .iter()
        .filter_map(|event| match event {
            SequenceEvent::Message(message) => Some(message),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].kind, MessageKind::Solid);
    assert_eq!(messages[0].label, "call");
    assert_eq!(messages[1].kind, MessageKind::Dashed);
    assert_eq!(messages[1].label, "result");
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

#[test]
fn parses_notes_and_activation_as_ordered_events() {
    let source = "\
sequenceDiagram
  participant Client
  participant API
  Note left of Client: Caller
  Client->>API: request
  activate API
  Note over Client,API: HTTPS request
  Note right of API: Service
  API-->>Client: response
  deactivate API
";
    let Diagram::Sequence(sequence) = diagram::parse(source).unwrap() else {
        panic!("expected sequence diagram");
    };

    assert_eq!(sequence.events.len(), 7);
    assert!(matches!(
        &sequence.events[0],
        SequenceEvent::Note(note)
            if note.position == NotePosition::LeftOf(0) && note.text == "Caller"
    ));
    assert!(matches!(
        &sequence.events[1],
        SequenceEvent::Message(message)
            if message.from == 0 && message.to == 1 && message.label == "request"
    ));
    assert!(matches!(
        &sequence.events[2],
        SequenceEvent::Activation(activation)
            if activation.participant == 1 && activation.kind == ActivationKind::Activate
    ));
    assert!(matches!(
        &sequence.events[3],
        SequenceEvent::Note(note)
            if note.position == NotePosition::Over(0, 1) && note.text == "HTTPS request"
    ));
    assert!(matches!(
        &sequence.events[4],
        SequenceEvent::Note(note)
            if note.position == NotePosition::RightOf(1) && note.text == "Service"
    ));
    assert!(matches!(
        &sequence.events[5],
        SequenceEvent::Message(message)
            if message.kind == MessageKind::Dashed && message.label == "response"
    ));
    assert!(matches!(
        &sequence.events[6],
        SequenceEvent::Activation(activation)
            if activation.participant == 1 && activation.kind == ActivationKind::Deactivate
    ));
}

#[test]
fn malformed_notes_and_activation_name_line_and_expectation() {
    let cases = [
        (
            "sequenceDiagram\nparticipant A\nNote beside A: nope\n",
            3,
            "expected `left of`, `right of`, or `over`",
        ),
        (
            "sequenceDiagram\nparticipant A\nNote left of Missing: nope\n",
            3,
            "unknown participant `Missing`",
        ),
        (
            "sequenceDiagram\nparticipant A\nNote over A:\n",
            3,
            "expected a note label after `:`",
        ),
        (
            "sequenceDiagram\nparticipant A\nparticipant B\nparticipant C\nNote over A,B,C: nope\n",
            5,
            "expected one participant or two comma-separated participants",
        ),
        (
            "sequenceDiagram\nparticipant A\ndeactivate A\n",
            3,
            "expected a matching `activate A`",
        ),
        (
            "sequenceDiagram\nparticipant A\nactivate A\n",
            3,
            "expected a matching `deactivate A`",
        ),
    ];
    for (source, line, expectation) in cases {
        let error = diagram::parse(source).unwrap_err();
        assert_eq!(error.line, line, "for {source:?}: {error}");
        assert!(
            error.msg.contains(expectation),
            "for {source:?}: got {error}"
        );
    }
}

#[test]
fn single_participant_note_and_nested_activations_render_cleanly() {
    let source = "\
sequenceDiagram
  participant A
  activate A
  Note over A: local work
  activate A
  A->>A: nested call
  deactivate A
  deactivate A
";
    let Diagram::Sequence(sequence) = diagram::parse(source).unwrap() else {
        panic!("expected sequence diagram");
    };
    assert!(matches!(
        &sequence.events[1],
        SequenceEvent::Note(note)
            if note.position == NotePosition::Over(0, 0) && note.text == "local work"
    ));

    let scene = llmaid::sequence::scene(&sequence, 100);
    let activations: Vec<_> = scene
        .foreground_boxes
        .iter()
        .filter(|box_| box_.lines.is_empty())
        .collect();
    assert_eq!(activations.len(), 2);
    assert_eq!((activations[0].rect.x - activations[1].rect.x).abs(), 2);
    assert_eq!(
        scene.edges[0].points.len(),
        4,
        "an active self-message must remain a routed loop"
    );
    let (output, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{output}", failures.join("\n"));
    assert!(output.contains("local work") && output.contains("nested call"));
}

#[test]
fn side_note_during_activation_clears_the_bar() {
    let source = "\
sequenceDiagram
  participant A
  activate A
  Note right of A: working
  deactivate A
";
    let parsed = diagram::parse(source).unwrap();
    let scene = diagram::scene(&parsed, 100);
    let activation = scene
        .foreground_boxes
        .iter()
        .find(|box_| box_.lines.is_empty())
        .expect("activation bar");
    let note = scene
        .foreground_boxes
        .iter()
        .find(|box_| box_.lines.first().is_some_and(|line| line == "working"))
        .expect("note");
    assert!(
        activation.rect.right() < note.rect.x,
        "active note should have a blank column after the bar: {activation:?} {note:?}"
    );
}

#[test]
fn event_after_empty_activation_starts_below_the_bar() {
    let source = "\
sequenceDiagram
  participant A
  participant B
  activate A
  deactivate A
  A->>B: after
";
    let parsed = diagram::parse(source).unwrap();
    let scene = diagram::scene(&parsed, 100);
    let activation = scene
        .foreground_boxes
        .iter()
        .find(|box_| box_.lines.is_empty())
        .expect("activation bar");
    assert!(
        activation.rect.bottom() <= scene.edges[0].points[0].y,
        "following event must clear empty activation: {activation:?} {:?}",
        scene.edges[0]
    );
}

#[test]
fn note_positions_and_activation_span_have_exact_clean_geometry() {
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
    let parsed = diagram::parse(source).unwrap();
    let scene = diagram::scene(&parsed, 100);
    let client_x = scene.paths[0].points[0].x;
    let api_x = scene.paths[1].points[0].x;

    let note = |label: &str| {
        scene
            .foreground_boxes
            .iter()
            .find(|box_| box_.lines.first().is_some_and(|line| line == label))
            .unwrap_or_else(|| panic!("missing note box {label:?}"))
    };
    let left = note("Caller");
    let right = note("Service");
    let over = note("HTTPS request");
    assert!(left.rect.right() < client_x, "{left:?}");
    assert!(right.rect.x > api_x, "{right:?}");
    assert_eq!(
        right.rect.y,
        left.rect.bottom(),
        "consecutive side notes should not spend a blank event row"
    );
    assert_eq!(
        over.rect.y,
        right.rect.bottom(),
        "the spanning note should pack directly after the side notes"
    );
    assert!(
        over.rect
            .contains(llmaid::scene::Point::new(client_x, over.rect.y + 1))
    );
    assert!(
        over.rect
            .contains(llmaid::scene::Point::new(api_x, over.rect.y + 1))
    );

    let activation = scene
        .foreground_boxes
        .iter()
        .find(|box_| box_.lines.is_empty())
        .expect("activation bar");
    let response_y = scene.edges[1].points[0].y;
    let response_label_y = scene.edges[1].label.as_ref().expect("response label").at.y;
    assert!(
        activation
            .rect
            .contains(llmaid::scene::Point::new(api_x, response_y)),
        "activation {activation:?} should cover response row {response_y}"
    );
    assert_eq!(
        activation.rect.y + 1,
        response_label_y,
        "activation cap and response label need separate rows"
    );
    assert_eq!(
        scene.edges[1].points[0].x, activation.rect.x,
        "return should leave from the activation bar's near boundary"
    );

    let (output, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{output}", failures.join("\n"));
    for text in ["Caller", "Service", "HTTPS request", "request", "response"] {
        assert!(output.contains(text), "missing {text:?}:\n{output}");
    }
}

#[test]
fn width_pressure_compacts_and_wraps_sequence_labels_at_word_boundaries() {
    let source = "\
sequenceDiagram
  participant C as Developer Client
  participant S as Application Service
  C->>S: compile DeveloperTool request
";
    let parsed = diagram::parse(source).unwrap();
    let comfortable = diagram::scene(&parsed, 200);
    let narrow = diagram::scene(&parsed, 20);

    assert!(
        narrow.bounds().w < comfortable.bounds().w,
        "{:?} should compact below {:?}",
        narrow.bounds(),
        comfortable.bounds()
    );
    assert!(
        narrow.boxes.iter().any(|box_| box_.lines.len() > 1),
        "participant labels did not wrap"
    );
    let message = narrow.edges[0].label.as_ref().expect("message label");
    assert!(message.text.contains('\n'), "{message:?}");
    assert!(
        message.text.contains("DeveloperTool"),
        "developer token was split: {message:?}"
    );

    let (rendered, failures) = render::render_scene_with_checks(&narrow, Style { ascii: false });
    assert!(failures.is_empty(), "{failures:#?}\n{rendered}");
    for word in [
        "Developer",
        "Client",
        "Application",
        "Service",
        "compile",
        "DeveloperTool",
        "request",
    ] {
        assert!(rendered.contains(word), "missing {word:?}:\n{rendered}");
    }
}
