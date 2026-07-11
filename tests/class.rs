use llmaid::class::{self, RelationKind};
use llmaid::parse::Dir;
use llmaid::render;
use llmaid::style::Style;

#[test]
fn parses_bare_and_block_classes_preserving_members() {
    let source = "\
classDiagram
  direction LR
  class Animal
  class BankAccount {
    +String owner
    -BigDecimal balance
    +deposit(amount) bool
  }
";
    let diagram = class::parse(source).unwrap();
    assert_eq!(diagram.direction, Some(Dir::LR));
    assert_eq!(diagram.classes.len(), 2);
    assert_eq!(diagram.classes[0].id, "Animal");
    assert_eq!(
        diagram.classes[1].members,
        [
            "+String owner",
            "-BigDecimal balance",
            "+deposit(amount) bool"
        ]
    );
}

#[test]
fn implicit_classes_follow_first_relation_use_deterministically() {
    let source = "classDiagram\nZebra --> Animal\nAnimal --> Habitat\n";
    let first = class::parse(source).unwrap();
    let second = class::parse(source).unwrap();
    let ids: Vec<_> = first
        .classes
        .iter()
        .map(|class| class.id.as_str())
        .collect();
    assert_eq!(ids, ["Zebra", "Animal", "Habitat"]);
    assert_eq!(class::dump(&first), class::dump(&second));
}

#[test]
fn parses_primary_relations_multiplicities_and_labels() {
    let operators = [
        ("A <|-- B", RelationKind::Inheritance),
        ("A *-- B", RelationKind::Composition),
        ("A o-- B", RelationKind::Aggregation),
        ("A --> B", RelationKind::Association),
        ("A -- B", RelationKind::Link),
        ("A ..> B", RelationKind::Dependency),
        ("A ..|> B", RelationKind::Realization),
    ];
    for (statement, expected) in operators {
        let source = format!("classDiagram\n{statement}\n");
        assert_eq!(class::parse(&source).unwrap().relations[0].kind, expected);
    }

    let diagram =
        class::parse("classDiagram\nCustomer \"1\" --> \"0..*\" Ticket : owns active tickets\n")
            .unwrap();
    let relation = &diagram.relations[0];
    assert_eq!(relation.from_multiplicity.as_deref(), Some("1"));
    assert_eq!(relation.to_multiplicity.as_deref(), Some("0..*"));
    assert_eq!(relation.label.as_deref(), Some("owns active tickets"));
}

#[test]
fn styling_directives_warn_without_changing_semantics() {
    let diagram = class::parse(
        "classDiagram\nclass A\nclassDef service fill:#fff\nstyle A fill:#000\nclick A href\n",
    )
    .unwrap();
    assert_eq!(diagram.classes.len(), 1);
    assert_eq!(diagram.warnings.len(), 3);
    assert_eq!(diagram.warnings[0].line, 3);
}

#[test]
fn malformed_input_names_line_and_expectation() {
    let cases = [
        ("notClass\n", 1, "expected `classDiagram` header"),
        (
            "classDiagram\nclass\n",
            2,
            "expected a class declaration or relation operator",
        ),
        ("classDiagram\nclass A {\n+x\n", 2, "expected a closing `}`"),
        (
            "classDiagram\nA ??? B\n",
            2,
            "expected a class declaration or relation operator",
        ),
        (
            "classDiagram\nA --> : owns\n",
            2,
            "expected class ids on both sides",
        ),
        (
            "classDiagram\nA --> B :\n",
            2,
            "expected a relation label after `:`",
        ),
        (
            "classDiagram\ndirection SIDEWAYS\n",
            2,
            "expected direction",
        ),
    ];
    for (source, line, expected) in cases {
        let error = class::parse(source).unwrap_err();
        assert_eq!(error.line, line, "{source:?}: {error}");
        assert!(error.msg.contains(expected), "{source:?}: {error}");
    }
}

#[test]
fn dump_is_stable_and_retains_all_visible_relation_semantics() {
    let diagram = class::parse(
        "classDiagram\ndirection TB\nclass Account {\n+deposit(value)\n}\nAccount \"1\" o-- \"*\" Entry : records\n",
    )
    .unwrap();
    let dumped = class::dump(&diagram);
    assert!(dumped.contains("direction: TB"));
    assert!(dumped.contains("member: +deposit(value)"));
    assert!(dumped.contains("Account \"1\" o-- \"*\" Entry : records"));
    assert!(!diagram.is_empty());
    assert_eq!(
        diagram.labels(),
        ["Account", "+deposit(value)", "Entry", "records"]
    );
}

#[test]
fn scene_is_invariant_clean_deterministic_and_preserves_visible_semantics() {
    let diagram = class::parse(
        "classDiagram\ndirection LR\nclass Customer {\n+name String\n+buy(ticket)\n}\nCustomer \"1\" o-- \"0..*\" Ticket : owns\nTicket ..|> Record : persists as\n",
    )
    .unwrap();
    let first = class::scene(&diagram, 120);
    let second = class::scene(&diagram, 120);
    assert_eq!(first, second);
    assert_eq!(first.boxes.len(), 3);
    assert_eq!(first.edges.len(), 2);

    let (unicode, failures) = render::render_scene_with_checks(&first, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{unicode}", failures.join("\n"));
    for visible in [
        "Customer",
        "+name String",
        "+buy(ticket)",
        "0..*",
        "owns",
        "persists as",
    ] {
        assert!(unicode.contains(visible), "missing {visible:?}:\n{unicode}");
    }

    let ascii = render::render_scene(&first, Style { ascii: true });
    assert!(ascii.is_ascii(), "{ascii}");
    assert!(ascii.contains("0..*"), "{ascii}");
    assert!(ascii.contains("persists as"), "{ascii}");
}

#[test]
fn direction_keyword_requires_a_token_boundary() {
    let diagram = class::parse("classDiagram\ndirectional --> B : works\n").unwrap();
    assert_eq!(diagram.classes[0].id, "directional");
    assert_eq!(diagram.relations[0].label.as_deref(), Some("works"));
}

#[test]
fn relation_labels_may_contain_relation_operators() {
    let diagram = class::parse("classDiagram\nA -- B : maps --> result with <|-- text\n").unwrap();
    assert_eq!(diagram.relations[0].kind, RelationKind::Link);
    assert_eq!(
        diagram.relations[0].label.as_deref(),
        Some("maps --> result with <|-- text")
    );
}

#[test]
fn class_relations_use_semantic_endpoint_decorations_not_generic_arrowheads() {
    let diagram = class::parse(
        "classDiagram\ndirection LR\nA <|-- B\nB *-- C\nC o-- D\nD --> E\nE -- F\nF ..> G\nG ..|> H\n",
    )
    .unwrap();
    let scene = class::scene(&diagram, 120);
    assert!(scene.edges.iter().all(|edge| edge.arrow.is_none()));
    assert_eq!(scene.endpoint_decorations.len(), 6);
    let rendered = render::render_scene(&scene, Style { ascii: false });
    assert!(
        rendered.contains("▶─┤"),
        "directed association/dependency head needs clear box spacing:\n{rendered}"
    );
}

#[test]
fn visual_fidelity_uses_compartments_and_endpoint_adornments() {
    let diagram = class::parse(
        "classDiagram\ndirection LR\nclass Customer {\n+String name\n}\nCustomer \"1\" o-- \"0..*\" Ticket : owns\nTicket ..|> Record : persists as\n",
    )
    .unwrap();
    let scene = class::scene(&diagram, 120);
    let (unicode, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{unicode}", failures.join("\n"));
    assert!(unicode.contains('├') && unicode.contains('┤'), "{unicode}");
    assert!(
        unicode.contains('◇'),
        "aggregation diamond missing:\n{unicode}"
    );
    assert!(
        unicode.contains('▷'),
        "realization triangle missing:\n{unicode}"
    );
    assert!(
        unicode.contains("owns") && unicode.contains("persists as"),
        "{unicode}"
    );
    assert!(
        unicode.contains('1') && unicode.contains("0..*"),
        "{unicode}"
    );
    assert!(
        !unicode.contains("o--") && !unicode.contains("..|>"),
        "{unicode}"
    );
    assert!(
        unicode.contains("├─◇"),
        "diamond needs space from its box:\n{unicode}"
    );
    assert!(
        unicode.contains("▷┄┤"),
        "triangle needs space from its box:\n{unicode}"
    );

    let ascii = render::render_scene(&scene, Style { ascii: true });
    assert!(ascii.is_ascii(), "{ascii}");
    for width in [1, 50, 100] {
        let narrow = class::scene(&diagram, width);
        let (narrow_output, narrow_failures) =
            render::render_scene_with_checks(&narrow, Style { ascii: false });
        assert!(
            narrow_failures.is_empty(),
            "width {width}: {narrow_failures:#?}"
        );
        assert_eq!(
            narrow_output, unicode,
            "structured class changed at width {width}"
        );
    }
}
