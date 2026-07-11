use llmaid::diagram::{self, Diagram};
use llmaid::mindmap;
use llmaid::render;
use llmaid::style::Style;

#[test]
fn parses_one_ordered_rooted_tree_from_two_space_indentation() {
    let source = "\
mindmap
  root((Agent loop))
    Parse
      Source order
      Diagnostics
    Render
";
    let diagram = mindmap::parse(source).unwrap();

    assert_eq!(diagram.nodes.len(), 5);
    assert_eq!(diagram.nodes[0].label, "Agent loop");
    assert_eq!(diagram.nodes[0].parent, None);
    assert_eq!(diagram.nodes[1].parent, Some(0));
    assert_eq!(diagram.nodes[2].parent, Some(1));
    assert_eq!(diagram.nodes[3].parent, Some(1));
    assert_eq!(diagram.nodes[4].parent, Some(0));
    assert_eq!(
        diagram
            .nodes
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Agent loop",
            "Parse",
            "Source order",
            "Diagnostics",
            "Render"
        ]
    );
}

#[test]
fn dispatch_comments_empty_input_and_duplicate_labels_keep_type_semantics() {
    assert!(matches!(
        diagram::parse("%% comment\n\nmindmap\n  Same\n    Same\n").unwrap(),
        Diagram::Mindmap(_)
    ));
    let empty = mindmap::parse("mindmap\n%% no nodes yet\n").unwrap();
    assert!(empty.is_empty());

    let duplicate = mindmap::parse("mindmap\n  Same\n    Same\n    Same\n").unwrap();
    assert_eq!(duplicate.labels(), ["Same", "Same", "Same"]);
    assert_eq!(duplicate.nodes[1].parent, Some(0));
    assert_eq!(duplicate.nodes[2].parent, Some(0));
}

#[test]
fn deferred_advanced_syntax_is_rejected_instead_of_rendered_as_plain_text() {
    for source in [
        "mindmap\n  Root\n    square[Square]\n",
        "mindmap\n  Root\n    ::icon(fa fa-book)\n",
        "mindmap\n  Root\n    Child:::urgent\n",
        "mindmap\n  Root\n    `Markdown`\n",
    ] {
        let error = mindmap::parse(source).unwrap_err();
        assert_eq!(error.line, 3);
        assert!(
            error.msg.contains("unsupported advanced mindmap syntax")
                && error.msg.contains("plain label"),
            "{error}"
        );
    }
}

#[test]
fn indentation_errors_name_the_line_and_the_exact_repair() {
    let cases = [
        (
            "mindmap\n  Root\n   odd indent\n",
            3,
            "increments of two spaces",
        ),
        ("mindmap\n  Root\n      orphan\n", 3, "missing parent"),
        ("mindmap\n  Root\n  Other root\n", 3, "multiple roots"),
        ("mindmap\n  Root\n\tChild\n", 3, "spaces, not tabs"),
    ];

    for (source, line, expected) in cases {
        let error = mindmap::parse(source).unwrap_err();
        assert_eq!(error.line, line, "{source:?}");
        assert!(error.msg.contains(expected), "{source:?}: {error}");
    }
}

#[test]
fn scene_is_deterministic_checked_and_ascii_pure() {
    let source = "mindmap\n  Root\n    Alpha\n      One\n      Two\n    Beta\n";
    let diagram = mindmap::parse(source).unwrap();
    let first = mindmap::scene(&diagram, 100);
    let second = mindmap::scene(&diagram, 100);
    assert_eq!(first, second);
    assert_eq!(first.boxes.len(), 5);
    assert_eq!(first.edges.len(), 4);

    let (unicode, failures) = render::render_scene_with_checks(&first, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{unicode}", failures.join("; "));
    let (ascii, failures) = render::render_scene_with_checks(&first, Style { ascii: true });
    assert!(failures.is_empty(), "{}\n{ascii}", failures.join("; "));
    assert!(ascii.is_ascii(), "{ascii}");
}

#[test]
fn width_pressure_wraps_before_using_the_documented_over_width_fallback() {
    let source = "mindmap\n  A very long root label\n    A child label with several words\n";
    let diagram = mindmap::parse(source).unwrap();
    let comfortable = mindmap::scene(&diagram, 100);
    assert!(comfortable.boxes.iter().all(|node| node.lines.len() == 1));

    let narrow = mindmap::scene(&diagram, 24);
    assert!(narrow.boxes.iter().any(|node| node.lines.len() > 1));
    let rendered = render::render_scene(&narrow, Style { ascii: false });
    assert!(!rendered.contains('…'), "{rendered}");
    for word in ["very", "long", "root", "label", "child", "several", "words"] {
        assert!(rendered.contains(word), "missing {word:?}\n{rendered}");
    }
}

#[test]
fn zero_width_sequences_fail_explicitly_instead_of_corrupting_a_frame() {
    for source in ["mindmap\n  cafe\u{301}\n", "mindmap\n  👨\u{200d}💻\n"] {
        let error = mindmap::parse(source).unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.msg.contains("zero-width Unicode sequence"), "{error}");
    }
}
