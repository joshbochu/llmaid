use llmaid::er::{self, AttributeKey, RelationshipKind};
use llmaid::parse::Dir;
use llmaid::render;
use llmaid::style::Style;

#[test]
fn parses_entities_aliases_and_attributes_without_losing_markers() {
    let source = "\
erDiagram
  direction LR
  customer[Customer Account] {
    string customer_id PK, UK \"public identifier\"
    string region_id FK
    decimal balance
  }
";
    let diagram = er::parse(source).unwrap();
    assert_eq!(diagram.direction, Some(Dir::LR));
    assert_eq!(diagram.entities[0].id, "customer");
    assert_eq!(diagram.entities[0].label, "Customer Account");
    let attributes = &diagram.entities[0].attributes;
    assert_eq!(attributes.len(), 3);
    assert_eq!(
        attributes[0].keys,
        [AttributeKey::Primary, AttributeKey::Unique]
    );
    assert_eq!(attributes[0].comment.as_deref(), Some("public identifier"));
    assert_eq!(attributes[1].keys, [AttributeKey::Foreign]);
}

#[test]
fn relationships_preserve_cardinality_kind_and_quoted_labels() {
    let diagram = er::parse(
        "erDiagram\nCUSTOMER ||--o{ ORDER : \"places online\"\nORDER }o..|| RECEIPT : generates\n",
    )
    .unwrap();
    assert_eq!(diagram.relationships.len(), 2);
    let first = &diagram.relationships[0];
    assert_eq!(first.left_cardinality, "||");
    assert_eq!(first.right_cardinality, "o{");
    assert_eq!(first.kind, RelationshipKind::Identifying);
    assert_eq!(first.label, "places online");
    let second = &diagram.relationships[1];
    assert_eq!(second.left_cardinality, "}o");
    assert_eq!(second.right_cardinality, "||");
    assert_eq!(second.kind, RelationshipKind::NonIdentifying);
}

#[test]
fn all_official_zero_one_many_tokens_parse_on_their_valid_side() {
    let source = "\
erDiagram
A ||--|| B : one
B |o--o| C : optional
C }o--o{ D : zero-many
D }|--|{ E : one-many
";
    let diagram = er::parse(source).unwrap();
    assert_eq!(diagram.relationships.len(), 4);
    assert_eq!(diagram.relationships[1].left_cardinality, "|o");
    assert_eq!(diagram.relationships[1].right_cardinality, "o|");
    assert_eq!(diagram.relationships[3].left_cardinality, "}|");
    assert_eq!(diagram.relationships[3].right_cardinality, "|{");
}

#[test]
fn implicit_entities_use_stable_first_seen_order() {
    let source = "erDiagram\nZ ||--o{ A : owns\nA ||--|| M : selects\n";
    let first = er::parse(source).unwrap();
    let second = er::parse(source).unwrap();
    let ids: Vec<_> = first
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect();
    assert_eq!(ids, ["Z", "A", "M"]);
    assert_eq!(er::dump(&first), er::dump(&second));
}

#[test]
fn styling_directives_warn_without_changing_entities() {
    let diagram = er::parse(
        "erDiagram\nACCOUNT\nclassDef important fill:#fff\nstyle ACCOUNT fill:#000\nclass ACCOUNT important\n",
    )
    .unwrap();
    assert_eq!(diagram.entities.len(), 1);
    assert_eq!(diagram.warnings.len(), 3);
    assert_eq!(diagram.warnings[0].line, 3);
}

#[test]
fn malformed_syntax_names_line_and_expectation() {
    let cases = [
        ("classDiagram\n", 1, "expected `erDiagram` header"),
        ("erDiagram\nentity[Alias\n", 2, "expected a closing `]`"),
        (
            "erDiagram\nA {\nstring\n}\n",
            3,
            "expected an attribute type and name",
        ),
        (
            "erDiagram\nA {\nstring id IX\n}\n",
            3,
            "expected only PK, FK, or UK",
        ),
        (
            "erDiagram\nA {\nstring id \"open\n}\n",
            3,
            "expected a closing `\"`",
        ),
        ("erDiagram\nA ||--o{ B\n", 2, "expected `:`"),
        (
            "erDiagram\nA xx--o{ B : bad\n",
            2,
            "expected valid zero/one/many",
        ),
        (
            "erDiagram\nA ||--o{ B :\n",
            2,
            "expected a relationship label",
        ),
        (
            "erDiagram\nA ||--o{ B : \"unfinished\n",
            2,
            "expected a closing `\"` for the relationship label",
        ),
        ("erDiagram\nA {\nstring id\n", 2, "expected a closing `}`"),
    ];
    for (source, line, expected) in cases {
        let error = er::parse(source).unwrap_err();
        assert_eq!(error.line, line, "{source:?}: {error}");
        assert!(error.msg.contains(expected), "{source:?}: {error}");
    }
}

#[test]
fn dump_labels_and_empty_contract_are_stable() {
    let empty = er::parse("erDiagram\n").unwrap();
    assert!(empty.is_empty());
    let diagram = er::parse(
        "erDiagram\nCUSTOMER[Customer] {\nstring id PK \"identifier\"\n}\nCUSTOMER ||--o{ ORDER : \"places\"\n",
    )
    .unwrap();
    assert_eq!(
        diagram.labels(),
        ["Customer", "string", "id", "identifier", "ORDER", "places"]
    );
    let dump = er::dump(&diagram);
    assert!(dump.contains("CUSTOMER [Customer]"));
    assert!(dump.contains("attribute: string id PK \"identifier\""));
    assert!(dump.contains("CUSTOMER ||--o{ ORDER : \"places\""));
}

#[test]
fn scene_is_deterministic_ascii_capable_and_invariant_clean() {
    let diagram = er::parse(
        "erDiagram\ndirection LR\nCUSTOMER[Customer] {\nstring id PK\n}\nCUSTOMER ||--o{ ORDER : \"places\"\nORDER }o..|| RECEIPT : generates\n",
    )
    .unwrap();
    let first = er::scene(&diagram, 120);
    assert_eq!(first, er::scene(&diagram, 120));
    assert!(first.edges.iter().all(|edge| edge.arrow.is_none()));
    let (unicode, failures) = render::render_scene_with_checks(&first, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{unicode}", failures.join("\n"));
    for visible in ["Customer", "string", "id", "PK", "places", "generates"] {
        assert!(unicode.contains(visible), "missing {visible:?}:\n{unicode}");
    }
    assert!(unicode.contains('○') && unicode.contains('<'), "{unicode}");
    assert!(
        !unicode.contains("||--o{") && !unicode.contains("}o..||"),
        "{unicode}"
    );
    assert!(
        unicode.contains("│─│"),
        "one/one bars need separation:\n{unicode}"
    );
    assert!(
        unicode.contains("○─<"),
        "optional-many marks need separation:\n{unicode}"
    );
    let ascii = render::render_scene(&first, Style { ascii: true });
    assert!(ascii.is_ascii(), "{ascii}");
    assert!(ascii.contains("o-<"), "{ascii}");
}

#[test]
fn directive_and_relationship_detection_require_token_boundaries() {
    let diagram = er::parse("erDiagram\ndirectional\nfoo--bar\n").unwrap();
    let ids: Vec<_> = diagram
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect();
    assert_eq!(ids, ["directional", "foo--bar"]);
}

#[test]
fn entity_alias_quotes_must_be_balanced() {
    let error = er::parse("erDiagram\nA[\"unclosed]\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(error.msg.contains("closing `\"`"), "{error}");
}

#[test]
fn visual_fidelity_uses_attribute_table_and_endpoint_cardinalities() {
    let diagram = er::parse(
        "erDiagram\ndirection LR\nCUSTOMER[Customer Account] {\nstring customer_id PK, UK \"public identifier\"\nstring region_id FK\n}\nCUSTOMER ||--o{ ORDER : places\nORDER }o..|| RECEIPT : generates\n",
    )
    .unwrap();
    let scene = er::scene(&diagram, 140);
    let (unicode, failures) = render::render_scene_with_checks(&scene, Style { ascii: false });
    assert!(failures.is_empty(), "{}\n{unicode}", failures.join("\n"));
    assert!(unicode.contains('├') && unicode.contains('┤'), "{unicode}");
    assert!(
        unicode.contains("string") && unicode.contains("customer_id"),
        "{unicode}"
    );
    assert!(
        unicode.contains("PK UK") && unicode.contains("public identifier"),
        "{unicode}"
    );
    assert!(
        unicode.contains('○') && (unicode.contains('<') || unicode.contains('>')),
        "{unicode}"
    );
    assert!(
        unicode.contains("places") && unicode.contains("generates"),
        "{unicode}"
    );
    assert!(
        !unicode.contains("||--o{") && !unicode.contains("}o..||"),
        "{unicode}"
    );

    let ascii = render::render_scene(&scene, Style { ascii: true });
    assert!(ascii.is_ascii(), "{ascii}");
    for width in [1, 50, 100] {
        let narrow = er::scene(&diagram, width);
        let (narrow_output, narrow_failures) =
            render::render_scene_with_checks(&narrow, Style { ascii: false });
        assert!(
            narrow_failures.is_empty(),
            "width {width}: {narrow_failures:#?}"
        );
        assert_eq!(
            narrow_output, unicode,
            "structured ER changed at width {width}"
        );
    }
}
